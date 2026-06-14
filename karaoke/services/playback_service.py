#!/usr/bin/env python
# -*- coding: utf-8 -*-

import asyncio
import os
import traceback

from tortoise.exceptions import DoesNotExist
from fastapi import Request

from karaoke.domain.playback import refresh_playback_mode, resolve, stream_media_for_kind
from karaoke.dto.mappers import playback_api
from karaoke.events.bus import event_bus
from karaoke.infra.repositories.history_repo import HistoryRepository
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.infra.streaming import build_stream_response, cache_not_ready_response
from karaoke.results import Result
from karaoke.services.prepare_service import PrepareService
from settings import logger


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

    async def get_profile(self, song_id: int) -> Result:
        result = Result()
        try:
            song = await self._songs.get(song_id)
            profile = await refresh_playback_mode(song)
            prep = await self._prepare.status(song_id)
            result.data = playback_api(song, profile, prep)
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def ensure_ready(self, song_id: int) -> Result:
        result = Result()
        try:
            song = await self._songs.get(song_id)
            await refresh_playback_mode(song)
            prep = await self._prepare.ensure_ready(song_id)
            result.data = prep
            if prep.get('ready'):
                result.msg = "播放资源已就绪"
            elif prep.get('status') in ('pending', 'running'):
                result.msg = "正在后台准备播放资源"
            elif prep.get('status') == 'failed':
                result.code = 1
                result.msg = prep.get('error') or "播放资源准备失败"
            else:
                result.msg = "等待准备播放资源"
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def prepare_status(self, song_id: int) -> Result:
        result = Result()
        try:
            await self._songs.get(song_id)
            result.data = await self._prepare.status(song_id)
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def stream(self, request: Request, song_id: int, kind: str):
        try:
            if kind not in ('video', 'vocals', 'accompaniment'):
                return Result(code=1, msg="无效的流类型")
            song = await self._songs.get(song_id)
            profile = resolve(song, prepare_embedded=False)
            file_path, media_type = await asyncio.to_thread(stream_media_for_kind, song, kind)
            if not file_path or not os.path.isfile(file_path):
                if profile.playback_source == 'embedded' and not profile.embedded_cache_ready:
                    prep = await self._prepare.status(song_id)
                    return cache_not_ready_response(prep)
                return Result(code=1, msg="播放文件不存在或未就绪")
            return build_stream_response(request, file_path, media_type)
        except DoesNotExist:
            return Result(code=1, msg="歌曲不存在")
        except Exception:
            logger.error(traceback.format_exc())
            return Result(code=1, msg="系统错误")

    async def mark_singing(self, song_id: int) -> Result:
        result = Result()
        try:
            history = await self._histories.get(song_id)
            history.is_sing = -1
            history.is_top = 0
            await self._histories.save(history, ['is_sing', 'is_top', 'update_time'])
            result.msg = f"{history.name} 设置-1成功"
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def mark_finished(self, song_id: int) -> Result:
        result = Result()
        try:
            history = await self._histories.get(song_id)
            history.is_sing = 1
            history.is_top = 0
            history.times += 1
            await self._histories.save(history, ['is_sing', 'is_top', 'times', 'update_time'])
            result.msg = f"{history.name} 设置1成功"
            await event_bus.publish_queue_changed()
        except Exception:
            logger.error(traceback.format_exc())
            result.code = 1
            result.msg = "系统错误"
        return result

    async def send_command(self, code: int, data) -> Result:
        await event_bus.publish(code, data)
        return Result()
