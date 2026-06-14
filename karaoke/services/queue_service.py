#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
import traceback

from tortoise.exceptions import DoesNotExist

from karaoke.domain.playback import refresh_playback_mode
from karaoke.dto.mappers import history_item
from karaoke.events.bus import event_bus
from karaoke.infra.repositories.history_repo import HistoryRepository
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.media import can_play_directly
from karaoke.results import Result
from karaoke.services.prepare_service import PrepareService
from settings import logger


class QueueService:
    def __init__(
        self,
        songs: SongRepository = None,
        histories: HistoryRepository = None,
        prepare: PrepareService = None,
    ) -> None:
        self._songs = songs or SongRepository()
        self._histories = histories or HistoryRepository()
        self._prepare = prepare or PrepareService()

    async def init_on_startup(self) -> None:
        await self._histories.reset_stale_singing()

    async def _build_list(self, histories) -> list:
        if not histories:
            return []
        song_map = await self._songs.map_by_ids([h.id for h in histories])
        return [history_item(h, song_map.get(h.id)) for h in histories]

    async def enqueue(self, song_id: int) -> Result:
        result = Result()
        try:
            song = await self._songs.get(song_id)
            profile = await refresh_playback_mode(song)
            if not profile.can_queue:
                result.code = 1
                if not song.is_playable or not os.path.isfile(song.source_path):
                    result.msg = "源视频不可播放或不存在"
                else:
                    result.msg = "增强资源不完整且源视频不可用"
                return result

            history = await self._histories.get_optional(song.id)
            if history:
                if history.is_sing == 1:
                    history.is_sing = 0
                    history.is_top = 0
                    await self._histories.save(history, ['is_sing', 'is_top', 'update_time'])
            else:
                await self._histories.create(
                    id=song.id, name=song.display_name, is_sing=0, is_top=0
                )

            prep = None
            if profile.playback_source == 'embedded' and not profile.embedded_cache_ready:
                prep = await self._prepare.schedule(song.id)
            elif profile.mode == 'plain' and profile.video_path:
                if not can_play_directly(profile.video_path):
                    prep = await self._prepare.schedule(song.id)

            await event_bus.publish_queue_changed()
            result.msg = f"{song.display_name} 点歌成功"
            result.data = {'playback_mode': profile.mode, 'prepare': prep}
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def list_pending(self) -> Result:
        return await self._list_by_type('pendingAll')

    async def list_history(self) -> Result:
        return await self._list_by_type('history')

    async def list_usually(self) -> Result:
        return await self._list_by_type('usually')

    async def _list_by_type(self, query_type: str) -> Result:
        result = Result()
        try:
            if query_type == 'history':
                histories = await self._histories.list_history()
            elif query_type == 'usually':
                histories = await self._histories.list_usually()
            elif query_type == 'pendingAll':
                histories = await self._histories.list_pending()
            else:
                result.code = 1
                result.msg = f"未知查询类型: {query_type}"
                return result
            result.data = await self._build_list(histories)
            result.total = len(result.data)
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def set_top(self, song_id: int) -> Result:
        result = Result()
        try:
            history = await self._histories.get(song_id)
            history.is_top = 1
            await self._histories.save(history, ['is_top', 'update_time'])
            result.msg = f"{history.name} 置顶成功"
            await event_bus.publish_queue_changed()
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def remove(self, song_id: int) -> Result:
        result = Result()
        try:
            history = await self._histories.get(song_id)
            await self._histories.delete(history)
            result.msg = f"{history.name} 播放记录删除成功"
            await event_bus.publish_queue_changed()
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result
