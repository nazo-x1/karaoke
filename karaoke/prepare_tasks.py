#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""后台播放资源准备：内嵌 MKV 拆轨、plain 浏览器转码等，去重且不占请求线程。"""

import asyncio
import os
import traceback
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, Optional

from tortoise.exceptions import DoesNotExist

from karaoke.audio_layout import has_dual_roles, parse_audio_layout
from karaoke.embedded import ensure_embedded_cache, probe_and_save_layout
from karaoke.media import (
    can_play_directly,
    browser_mp4_cache_path,
    ensure_browser_mp4_cache,
    _validate_browser_mp4,
)
from karaoke.models import Song
from karaoke.playback import has_full_override, refresh_playback_mode, resolve
from settings import logger


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
    error: Optional[str] = None
    asyncio_task: Optional[asyncio.Task] = field(default=None, repr=False)


_manager: Optional['PrepareTaskManager'] = None


class PrepareTaskManager:
    def __init__(self) -> None:
        self._lock = asyncio.Lock()
        self._tasks: Dict[int, _PrepareTask] = {}

    async def schedule(self, song_id: int) -> dict:
        """若需准备则启动后台任务；已在跑或已完成则复用。"""
        if await self._is_stream_ready(song_id):
            return await self.status(song_id)

        async with self._lock:
            existing = self._tasks.get(song_id)
            if existing:
                if existing.state in (PrepareState.PENDING, PrepareState.RUNNING):
                    return self._to_dict(existing)
                if existing.state in (PrepareState.READY, PrepareState.NOT_NEEDED):
                    if await self._is_stream_ready(song_id):
                        return self._to_dict(existing)
                del self._tasks[song_id]

            task = _PrepareTask(song_id=song_id)
            self._tasks[song_id] = task
            task.asyncio_task = asyncio.create_task(self._run(task))
            return self._to_dict(task)

    async def status(self, song_id: int) -> dict:
        if await self._is_stream_ready(song_id):
            return {
                'song_id': song_id,
                'status': PrepareState.READY.value,
                'ready': True,
                'error': None,
            }

        task = self._tasks.get(song_id)
        if task:
            return self._to_dict(task)

        need = await self._needs_prepare(song_id)
        if not need:
            return {
                'song_id': song_id,
                'status': PrepareState.NOT_NEEDED.value,
                'ready': True,
                'error': None,
            }

        return {
            'song_id': song_id,
            'status': PrepareState.IDLE.value,
            'ready': False,
            'error': None,
        }

    async def wait_until_ready(self, song_id: int, timeout: float = 3600.0) -> dict:
        """等待后台任务完成（供手动预生成等场景）。"""
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

    async def _is_stream_ready(self, song_id: int) -> bool:
        try:
            song = await Song.get(id=song_id)
        except DoesNotExist:
            return False
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

    async def _needs_prepare(self, song_id: int) -> bool:
        try:
            song = await Song.get(id=song_id)
        except DoesNotExist:
            return False
        if has_full_override(song.display_name)[0]:
            return False
        profile = resolve(song, prepare_embedded=False)
        if profile.playback_source == 'embedded':
            layout = parse_audio_layout(song.audio_layout)
            return bool(layout and has_dual_roles(layout) and not profile.embedded_cache_ready)
        if profile.mode == 'plain' and profile.video_path:
            return not can_play_directly(profile.video_path)
        return False

    async def _run(self, task: _PrepareTask) -> None:
        task.state = PrepareState.RUNNING
        song_id = task.song_id
        try:
            song = await Song.get(id=song_id)
            if has_full_override(song.display_name)[0]:
                task.state = PrepareState.NOT_NEEDED
                return

            profile = resolve(song, prepare_embedded=False)

            if profile.playback_source == 'embedded':
                layout = parse_audio_layout(song.audio_layout)
                if not layout:
                    layout = await probe_and_save_layout(song, assigned_by='auto')
                if not layout or not has_dual_roles(layout):
                    task.state = PrepareState.FAILED
                    task.error = '无有效双音轨布局，请先检测播放能力'
                    return
                paths = await asyncio.to_thread(
                    ensure_embedded_cache, song, layout, True
                )
                await refresh_playback_mode(song)
                if paths.ready:
                    task.state = PrepareState.READY
                    await _broadcast_ready(song_id)
                else:
                    task.state = PrepareState.FAILED
                    task.error = '内嵌缓存生成失败'
                return

            if profile.mode == 'plain' and profile.video_path:
                ok = await asyncio.to_thread(
                    ensure_browser_mp4_cache, profile.video_path
                )
                if ok:
                    task.state = PrepareState.READY
                    await _broadcast_ready(song_id)
                else:
                    task.state = PrepareState.FAILED
                    task.error = '浏览器转码缓存生成失败'
                return

            task.state = PrepareState.NOT_NEEDED
        except DoesNotExist:
            task.state = PrepareState.FAILED
            task.error = '歌曲不存在'
        except Exception:
            logger.error('prepare task failed song=%s\n%s', song_id, traceback.format_exc())
            task.state = PrepareState.FAILED
            task.error = '系统错误'

    @staticmethod
    def _to_dict(task: _PrepareTask) -> dict:
        ready = task.state in (PrepareState.READY, PrepareState.NOT_NEEDED)
        return {
            'song_id': task.song_id,
            'status': task.state.value,
            'ready': ready,
            'error': task.error,
        }


def get_manager() -> PrepareTaskManager:
    global _manager
    if _manager is None:
        _manager = PrepareTaskManager()
    return _manager


async def schedule_playback_prepare(song_id: int) -> dict:
    return await get_manager().schedule(song_id)


async def get_prepare_status(song_id: int) -> dict:
    return await get_manager().status(song_id)


async def wait_playback_ready(song_id: int, timeout: float = 3600.0) -> dict:
    return await get_manager().wait_until_ready(song_id, timeout)


async def _broadcast_ready(song_id: int) -> None:
    from karaoke import views
    await views.broadcast_data({'code': 9, 'data': str(song_id)})
