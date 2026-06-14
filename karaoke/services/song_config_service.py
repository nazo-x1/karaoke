#!/usr/bin/env python
# -*- coding: utf-8 -*-

from tortoise.exceptions import DoesNotExist

from karaoke.audio_layout import merge_manual_roles, parse_audio_layout, serialize_audio_layout
from karaoke.domain.playback import has_full_override, refresh_playback_mode, resolve
from karaoke.domain.prepare_policy import profile_needs_prepare
from karaoke.dto.mappers import playback_detail, song_item
from karaoke.embedded import probe_and_save_layout
from karaoke.errors import fail_result
from karaoke.infra.repositories.history_repo import HistoryRepository
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.results import Result
from karaoke.services.prepare_service import PrepareService


class SongConfigService:
    def __init__(
        self,
        songs: SongRepository = None,
        histories: HistoryRepository = None,
        prepare: PrepareService = None,
    ) -> None:
        self._songs = songs or SongRepository()
        self._histories = histories or HistoryRepository()
        self._prepare = prepare or PrepareService()

    async def get_detail(self, song_id: int) -> Result:
        result = Result()
        try:
            song = await self._songs.get(song_id)
            profile = resolve(song)
            result.data = {
                'id': song.id,
                'display_name': song.display_name,
                'source_path': song.source_path,
                'source_origin': song.source_origin,
                'source_rel': song.source_rel,
                'is_playable': song.is_playable,
                **playback_detail(song, profile),
            }
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception as exc:
            fail_result(result, exc, "获取歌曲详情失败")
        return result

    async def patch(self, song_id: int, body: dict) -> Result:
        result = Result()
        try:
            song = await self._songs.get(song_id)
            display_name = body.get('display_name')
            audio_tracks = body.get('audio_tracks')
            if display_name:
                song.display_name = display_name.strip()[:256]
                await self._songs.save(song, ['display_name', 'update_time'])
                for h in await self._histories.list_for_song(song.id):
                    h.name = song.display_name
                    await self._histories.save(h, ['name', 'update_time'])
            if audio_tracks is not None:
                layout = merge_manual_roles(parse_audio_layout(song.audio_layout), audio_tracks)
                song.audio_layout = serialize_audio_layout(layout)
                await self._songs.save(song, ['audio_layout', 'update_time'])
            profile = await refresh_playback_mode(song)
            result.data = song_item(song, profile)
            result.msg = "更新成功"
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception as exc:
            fail_result(result, exc, "更新歌曲失败")
        return result

    async def detect_playback(self, song_id: int) -> Result:
        result = Result()
        try:
            song = await self._songs.get(song_id)
            if not has_full_override(song.display_name)[0]:
                await probe_and_save_layout(song, assigned_by='auto')
            profile = await refresh_playback_mode(song)
            result.data = playback_detail(song, profile)
            if profile_needs_prepare(song, profile):
                prep = await self._prepare.schedule(song_id)
                result.data = {**result.data, 'prepare': prep}
            result.msg = "播放能力检测完成"
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception as exc:
            fail_result(result, exc, "检测播放能力失败")
        return result

    async def request_prepare(self, song_id: int, wait: bool = False) -> Result:
        result = Result()
        try:
            song = await self._songs.get(song_id)
            if has_full_override(song.display_name)[0]:
                profile = resolve(song)
                result.data = playback_detail(song, profile)
                result.msg = "已有 __override__ 三件套，无需预生成内嵌缓存"
                return result
            prep = await self._prepare.schedule(song_id)
            if wait:
                prep = await self._prepare.wait_until_ready(song_id)
            profile = await refresh_playback_mode(song)
            result.data = {
                **playback_detail(song, profile),
                'prepare': prep,
                'cache_ready': prep.get('ready', False),
            }
            if prep.get('ready'):
                result.msg = "缓存已就绪"
            elif prep.get('status') in ('pending', 'running'):
                result.msg = "正在后台生成缓存"
            else:
                result.code = 1
                result.msg = prep.get('error') or "缓存生成失败"
        except DoesNotExist:
            result.code = 1
            result.msg = "歌曲不存在"
        except Exception as exc:
            fail_result(result, exc, "预生成缓存失败")
        return result
