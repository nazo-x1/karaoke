#!/usr/bin/env python
# -*- coding: utf-8 -*-

import asyncio
import os

from tortoise.exceptions import DoesNotExist
from fastapi import Request

from karaoke.domain.playback import persist_playback_mode, resolve, stream_media_for_kind
from karaoke.domain.prepare_policy import profile_needs_prepare
from karaoke.domain.queue_policy import QueueState
from karaoke.dto.api_result import ApiResult
from karaoke.dto.mappers import playback_api
from karaoke.errors import format_api_error
from karaoke.events.bus import event_bus
from karaoke.infra.repositories.history_repo import HistoryRepository
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.infra.streaming import build_stream_response, cache_not_ready_response
from karaoke.services.base import run_guarded
from karaoke.services.prepare_service import PrepareService


class PlaybackService:
    def __init__(
        self,
        songs: SongRepository = None,
        histories: HistoryRepository = None,
        prepare: PrepareService = None,
    ) -> None:
        self._songs = songs or SongRepository()
        self._histories = histories or HistoryRepository()
        self._prepare = prepare or PrepareService()

    async def get_profile(self, song_id: int) -> ApiResult:
        async def load():
            song = await self._songs.get(song_id)
            profile = resolve(song)
            prep = await self._prepare.status(song_id)
            return playback_api(song, profile, prep)

        return await run_guarded('获取播放配置失败', load, not_found_msg='歌曲不存在')

    async def get_prepare(self, song_id: int) -> ApiResult:
        async def load():
            await self._songs.get(song_id)
            return await self._prepare.status(song_id)

        return await run_guarded('获取准备状态失败', load, not_found_msg='歌曲不存在')

    async def schedule_prepare(self, song_id: int) -> ApiResult:
        async def action():
            song = await self._songs.get(song_id)
            prep = await self._prepare.schedule(song_id)
            await persist_playback_mode(song, resolve(song))
            if prep.get('ready'):
                return ApiResult(data=prep, msg='播放资源已就绪')
            if prep.get('status') in ('pending', 'running'):
                return ApiResult(data=prep, msg='正在后台准备播放资源')
            if prep.get('status') == 'failed':
                return ApiResult.fail(prep.get('error') or '播放资源准备失败', data=prep)
            return ApiResult(data=prep, msg='等待准备播放资源')

        return await run_guarded('准备播放资源失败', action, not_found_msg='歌曲不存在')

    async def stream(self, request: Request, song_id: int, kind: str):
        try:
            if kind not in ('video', 'vocals', 'accompaniment'):
                return ApiResult.fail('无效的流类型')
            song = await self._songs.get(song_id)
            profile = resolve(song, prepare_embedded=False)
            file_path, media_type = await asyncio.to_thread(stream_media_for_kind, song, kind)
            if not file_path or not os.path.isfile(file_path):
                if profile.playback_source == 'embedded' and not profile.embedded_cache_ready:
                    prep = await self._prepare.status(song_id)
                    return cache_not_ready_response(prep)
                return ApiResult.fail('播放文件不存在或未就绪')
            return build_stream_response(request, file_path, media_type)
        except DoesNotExist:
            return ApiResult.not_found('歌曲')
        except Exception as exc:
            return ApiResult.fail(format_api_error(exc, '获取播放流失败'))

    async def mark_singing(self, song_id: int) -> ApiResult:
        async def action():
            history = await self._histories.get(song_id)
            history.is_sing = QueueState.SINGING
            history.is_top = 0
            await self._histories.save(history, ['is_sing', 'is_top', 'update_time'])
            await event_bus.publish_queue_changed()
            return ApiResult(msg=f'{history.name} 设置-1成功')

        return await run_guarded('标记正在播放失败', action, not_found_msg='歌曲不在队列中')

    async def skip_if_not_ready(self, song_id: int) -> ApiResult:
        return await run_guarded(
            '跳过未就绪歌曲失败',
            lambda: self._skip_if_not_ready(song_id),
            not_found_msg='歌曲不在队列中',
        )

    async def _skip_if_not_ready(self, song_id: int) -> ApiResult:
        history = await self._histories.get(song_id)
        song = await self._songs.get(song_id)
        profile = resolve(song)
        prep_status = await self._prepare.status(song_id)
        stream_ready = prep_status.get('ready', False)

        if stream_ready and profile.can_queue:
            return ApiResult.fail('歌曲已就绪，无需跳过')

        prep = None
        if profile_needs_prepare(song, profile) and not stream_ready:
            prep = await self._prepare.schedule(song_id)

        mark_result = await self.mark_finished(song_id)
        if mark_result.code != 0:
            return mark_result

        result = ApiResult(msg=f'{history.name} 未就绪，已跳过')
        if prep:
            result.data = {'prepare': prep}
        return result

    async def mark_finished(self, song_id: int) -> ApiResult:
        async def action():
            history = await self._histories.get(song_id)
            history.is_sing = QueueState.SUNG
            history.is_top = 0
            history.times += 1
            await self._histories.save(history, ['is_sing', 'is_top', 'times', 'update_time'])
            await event_bus.publish_queue_changed()
            return ApiResult(msg=f'{history.name} 设置1成功')

        return await run_guarded('标记已唱完失败', action, not_found_msg='歌曲不在队列中')

    async def send_command(self, code: int, data) -> ApiResult:
        await event_bus.publish(code, data)
        return ApiResult()
