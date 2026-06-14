#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os

from tortoise.exceptions import DoesNotExist

from karaoke.domain.playback import persist_playback_mode, resolve
from karaoke.domain.prepare_policy import profile_needs_prepare
from karaoke.domain.queue_policy import QueueState
from karaoke.dto.mappers import history_item
from karaoke.errors import fail_result
from karaoke.events.bus import event_bus
from karaoke.infra.repositories.history_repo import HistoryRepository
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.dto.api_result import ApiResult
from karaoke.services.base import apply_pagination, run_guarded
from karaoke.services.prepare_service import PrepareService
from settings import PAGE_SIZE


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

    async def enqueue(self, song_id: int) -> ApiResult:
        result = ApiResult()
        try:
            song = await self._songs.get(song_id)
            profile = resolve(song)
            if not profile.can_queue:
                result.code = 1
                if not song.is_playable or not os.path.isfile(song.source_path):
                    result.msg = "源视频不可播放或不存在"
                else:
                    result.msg = "增强资源不完整且源视频不可用"
                return result

            if profile_needs_prepare(song, profile):
                prep_status = await self._prepare.status(song.id)
                if not prep_status.get('ready'):
                    prep = await self._prepare.schedule(song.id)
                    result.code = 1
                    result.msg = "播放资源正在准备中，请耐心等待"
                    result.data = {'prepare': prep}
                    return result

            history = await self._histories.get_optional(song.id)
            if history:
                if history.is_sing == QueueState.SUNG:
                    history.is_sing = QueueState.PENDING
                    history.is_top = 0
                    await self._histories.save(history, ['is_sing', 'is_top', 'update_time'])
            else:
                await self._histories.create(
                    id=song.id, name=song.display_name,
                    is_sing=QueueState.PENDING, is_top=0,
                )

            await persist_playback_mode(song, profile)

            await event_bus.publish_queue_changed()
            result.msg = f"{song.display_name} 点歌成功"
            result.data = {'playback_mode': profile.mode}
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception as exc:
            fail_result(result, exc, "点歌失败")
        return result

    async def list_pending(self) -> ApiResult:
        return await self._list_by_type('pendingAll')

    async def list_history(self, page: int = 1) -> ApiResult:
        return await self._list_by_type('history', page)

    async def list_usually(self, page: int = 1) -> ApiResult:
        return await self._list_by_type('usually', page)

    async def _list_by_type(self, query_type: str, page: int = 1) -> ApiResult:
        async def load():
            total_num = 0
            if query_type == 'history':
                histories, total_num = await self._histories.list_history_page(page)
            elif query_type == 'usually':
                histories, total_num = await self._histories.list_usually_page(page)
            elif query_type == 'pendingAll':
                histories = await self._histories.list_pending()
                total_num = len(histories)
            else:
                return ApiResult.fail(f"未知查询类型: {query_type}")
            data = await self._build_list(histories)
            result = ApiResult(data=data)
            apply_pagination(result, total_num, page, PAGE_SIZE)
            if query_type == 'pendingAll':
                result.totalPage = 1 if total_num else 0
            return result

        return await run_guarded('获取队列失败', load)

    async def set_top(self, song_id: int) -> ApiResult:
        result = ApiResult()
        try:
            history = await self._histories.get(song_id)
            history.is_top = 1
            await self._histories.save(history, ['is_top', 'update_time'])
            result.msg = f"{history.name} 置顶成功"
            await event_bus.publish_queue_changed()
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不在队列中"
        except Exception as exc:
            fail_result(result, exc, "置顶失败")
        return result

    async def remove_if_exists(self, song_id: int) -> None:
        history = await self._histories.get_optional(song_id)
        if history:
            await self._histories.delete(history)

    async def remove(self, song_id: int) -> ApiResult:
        result = ApiResult()
        try:
            history = await self._histories.get(song_id)
            await self._histories.delete(history)
            result.msg = f"{history.name} 播放记录删除成功"
            await event_bus.publish_queue_changed()
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不在队列中"
        except Exception as exc:
            fail_result(result, exc, "移除队列项失败")
        return result
