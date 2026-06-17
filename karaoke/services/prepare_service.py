#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""后台播放资源准备：内嵌 MKV 拆轨、plain 浏览器转码等。"""

import asyncio
import os
from collections import deque
from dataclasses import dataclass, field
from enum import Enum
from typing import Callable, Deque, Dict, Optional

from tortoise.exceptions import DoesNotExist

from karaoke.infra.audio_layout import has_dual_roles, parse_audio_layout
from karaoke.domain.playback import (
    has_full_override,
    persist_playback_mode,
    resolve,
)
from karaoke.domain.prepare_policy import profile_needs_prepare
from karaoke.infra.embedded import ensure_embedded_cache, probe_and_save_layout
from karaoke.errors import format_api_error
from karaoke.events.bus import event_bus
from karaoke.infra.models import Song
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.infra.media import (
    browser_mp4_cache_path,
    can_play_directly,
    ensure_browser_mp4_cache,
    _validate_browser_mp4,
)
from settings import PREPARE_MAX_CONCURRENT, logger


class PrepareState(str, Enum):
    IDLE = 'idle'
    PENDING = 'pending'
    RUNNING = 'running'
    READY = 'ready'
    NOT_NEEDED = 'not_needed'
    FAILED = 'failed'


@dataclass
class _PrepareTask:
    song_id: int
    state: PrepareState = PrepareState.PENDING
    phase: str = 'pending'
    progress: float = 0.0
    message: str = '排队等待中'
    prepare_kind: str = 'unknown'
    error: Optional[str] = None
    asyncio_task: Optional[asyncio.Task] = field(default=None, repr=False)


class PrepareTaskManager:
    def __init__(
        self,
        songs: Optional[SongRepository] = None,
        max_concurrent: Optional[int] = None,
    ) -> None:
        self._songs = songs or SongRepository()
        self._max_concurrent = max(1, max_concurrent or PREPARE_MAX_CONCURRENT)
        self._lock = asyncio.Lock()
        self._tasks: Dict[int, _PrepareTask] = {}
        self._wait_queue: Deque[int] = deque()
        self._running = 0

    async def schedule(self, song_id: int) -> dict:
        async with self._lock:
            existing = self._tasks.get(song_id)
            if existing and existing.state in (PrepareState.PENDING, PrepareState.RUNNING):
                return self._to_dict(existing)

        if await self._is_stream_ready(song_id):
            return await self.status(song_id)

        async with self._lock:
            existing = self._tasks.get(song_id)
            if existing and existing.state in (
                PrepareState.READY,
                PrepareState.NOT_NEEDED,
                PrepareState.FAILED,
            ):
                del self._tasks[song_id]

            task = _PrepareTask(song_id=song_id)
            self._tasks[song_id] = task
            self._enqueue(song_id)
            snapshot = self._to_dict(task)

        await self._pump_queue()
        return snapshot

    def active_tasks(self) -> Dict[int, dict]:
        return {
            song_id: self._to_dict(task)
            for song_id, task in self._tasks.items()
            if task.state in (PrepareState.PENDING, PrepareState.RUNNING)
        }

    async def status(self, song_id: int) -> dict:
        task = self._tasks.get(song_id)
        if task:
            return self._to_dict(task)

        if await self._is_stream_ready(song_id):
            return self._ready_status(song_id)

        song = await self._songs.get_optional(song_id)
        if not song or not profile_needs_prepare(song):
            return {
                'song_id': song_id,
                'status': PrepareState.NOT_NEEDED.value,
                'ready': True,
                'phase': 'done',
                'progress': 100.0,
                'message': '无需准备',
                'prepare_kind': 'none',
                'error': None,
            }

        return {
            'song_id': song_id,
            'status': PrepareState.IDLE.value,
            'ready': False,
            'phase': 'idle',
            'progress': 0.0,
            'message': '尚未开始准备',
            'prepare_kind': 'unknown',
            'error': None,
        }

    async def wait_until_ready(self, song_id: int, timeout: float = 3600.0) -> dict:
        deadline = asyncio.get_event_loop().time() + timeout
        while True:
            st = await self.status(song_id)
            if st['ready']:
                return st
            if st['status'] == PrepareState.FAILED.value:
                return st
            if st['status'] == PrepareState.IDLE.value:
                await self.schedule(song_id)
            if asyncio.get_event_loop().time() >= deadline:
                st = await self.status(song_id)
                st['error'] = st.get('error') or '等待播放资源超时'
                st['status'] = PrepareState.FAILED.value
                return st
            await asyncio.sleep(1.0)

    def _enqueue(self, song_id: int) -> None:
        self._wait_queue = deque(sid for sid in self._wait_queue if sid != song_id)
        self._wait_queue.append(song_id)

    async def _pump_queue(self) -> None:
        async with self._lock:
            while self._running < self._max_concurrent:
                song_id = self._pop_runnable()
                if song_id is None:
                    break
                task = self._tasks.get(song_id)
                if not task or task.state != PrepareState.PENDING or task.asyncio_task is not None:
                    continue
                self._running += 1
                task.asyncio_task = asyncio.create_task(self._run_and_release(task))

    def _pop_runnable(self) -> Optional[int]:
        while self._wait_queue:
            song_id = self._wait_queue.popleft()
            task = self._tasks.get(song_id)
            if task and task.state == PrepareState.PENDING and task.asyncio_task is None:
                return song_id
        return None

    async def _run_and_release(self, task: _PrepareTask) -> None:
        try:
            await self._run(task)
        finally:
            async with self._lock:
                self._running = max(0, self._running - 1)
            await self._pump_queue()

    async def _is_stream_ready(self, song_id: int) -> bool:
        song = await self._songs.get_optional(song_id)
        if not song:
            return False
        return await asyncio.to_thread(self._check_stream_ready_sync, song)

    @staticmethod
    def _check_stream_ready_sync(song: Song) -> bool:
        profile = resolve(song, prepare_embedded=False)
        if profile.playback_source == 'override':
            return True
        if profile.playback_source == 'embedded':
            return profile.embedded_cache_ready
        if profile.mode == 'plain' and profile.video_path:
            if can_play_directly(profile.video_path):
                return True
            cached = browser_mp4_cache_path(profile.video_path)
            return os.path.isfile(cached) and _validate_browser_mp4(cached)
        return profile.can_queue

    @staticmethod
    def _ready_status(song_id: int) -> dict:
        return {
            'song_id': song_id,
            'status': PrepareState.READY.value,
            'ready': True,
            'phase': 'done',
            'progress': 100.0,
            'message': '播放资源已就绪',
            'prepare_kind': 'none',
            'error': None,
        }

    async def _run(self, task: _PrepareTask) -> None:
        task.state = PrepareState.RUNNING
        task.message = '准备中'
        song_id = task.song_id
        loop = asyncio.get_running_loop()
        updater = self._make_progress_updater(task, loop)
        try:
            song = await self._songs.get(song_id)
            if has_full_override(song.display_name)[0]:
                task.state = PrepareState.NOT_NEEDED
                task.phase = 'done'
                task.progress = 100.0
                task.message = '无需准备'
                return

            profile = resolve(song, prepare_embedded=False)

            if profile.playback_source == 'embedded':
                task.prepare_kind = 'embedded'
                task.message = 'MKV 双音轨拆轨中'
                layout = parse_audio_layout(song.audio_layout)
                if not layout:
                    layout = await probe_and_save_layout(song, assigned_by='auto')
                if not layout or not has_dual_roles(layout):
                    task.state = PrepareState.FAILED
                    task.error = '无有效双音轨布局，请先检测播放能力'
                    task.message = '准备失败'
                    return

                def embedded_progress(progress: float, phase: str, message: str) -> None:
                    updater(progress, phase, message)

                paths = await asyncio.to_thread(
                    ensure_embedded_cache, song, layout, True, embedded_progress
                )
                await persist_playback_mode(song, resolve(song))
                if paths.ready:
                    task.state = PrepareState.READY
                    task.phase = 'done'
                    task.progress = 100.0
                    task.message = '缓存就绪'
                    await event_bus.publish_prepare_ready(song_id)
                else:
                    task.state = PrepareState.FAILED
                    task.error = '内嵌缓存生成失败'
                    task.message = '内嵌缓存生成失败'
                return

            if profile.mode == 'plain' and profile.video_path:
                task.prepare_kind = 'plain'
                task.message = '浏览器转码中'
                task.phase = 'transcode'

                def plain_progress(pct: float) -> None:
                    updater(pct, 'transcode', '浏览器转码中')

                ok = await asyncio.to_thread(
                    ensure_browser_mp4_cache, profile.video_path, plain_progress
                )
                if ok:
                    task.state = PrepareState.READY
                    task.phase = 'done'
                    task.progress = 100.0
                    task.message = '转码完成'
                    await event_bus.publish_prepare_ready(song_id)
                else:
                    task.state = PrepareState.FAILED
                    task.error = '浏览器转码缓存生成失败'
                    task.message = '转码失败'
                return

            task.state = PrepareState.NOT_NEEDED
            task.phase = 'done'
            task.progress = 100.0
            task.message = '无需准备'
        except DoesNotExist:
            task.state = PrepareState.FAILED
            task.error = '歌曲不存在'
            task.message = '准备失败'
        except Exception as exc:
            task.state = PrepareState.FAILED
            task.error = format_api_error(exc, "播放资源准备失败")
            task.message = '准备失败'

    @staticmethod
    def _make_progress_updater(
        task: _PrepareTask,
        loop: asyncio.AbstractEventLoop,
    ) -> Callable[[float, str, str], None]:
        def update(progress: float, phase: str, message: str) -> None:
            def apply() -> None:
                task.progress = round(min(100.0, max(0.0, progress)), 1)
                task.phase = phase
                task.message = message

            loop.call_soon_threadsafe(apply)

        return update

    def _to_dict(self, task: _PrepareTask) -> dict:
        ready = task.state in (PrepareState.READY, PrepareState.NOT_NEEDED)
        message = task.message
        if task.state == PrepareState.PENDING and task.asyncio_task is None:
            ahead = self._queue_ahead_count(task.song_id)
            if ahead > 0:
                message = f'排队等待中（前方 {ahead} 首）'
        return {
            'song_id': task.song_id,
            'status': task.state.value,
            'ready': ready,
            'phase': task.phase,
            'progress': task.progress,
            'message': message,
            'prepare_kind': task.prepare_kind,
            'error': task.error,
        }

    def _queue_ahead_count(self, song_id: int) -> int:
        ahead = 0
        for sid in self._wait_queue:
            if sid == song_id:
                break
            ahead += 1
        return ahead + self._running


_manager: Optional[PrepareTaskManager] = None


def get_manager() -> PrepareTaskManager:
    global _manager
    if _manager is None:
        _manager = PrepareTaskManager()
    return _manager


class PrepareService:
    def __init__(self, manager: Optional[PrepareTaskManager] = None) -> None:
        self._manager = manager or get_manager()

    async def schedule(self, song_id: int) -> dict:
        return await self._manager.schedule(song_id)

    async def status(self, song_id: int) -> dict:
        return await self._manager.status(song_id)

    def active_tasks(self) -> Dict[int, dict]:
        return self._manager.active_tasks()

    async def wait_until_ready(self, song_id: int, timeout: float = 3600.0) -> dict:
        return await self._manager.wait_until_ready(song_id, timeout)

    @staticmethod
    def needs_prepare(song, profile=None) -> bool:
        return profile_needs_prepare(song, profile)
