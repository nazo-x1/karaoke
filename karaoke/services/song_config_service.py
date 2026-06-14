#!/usr/bin/env python
# -*- coding: utf-8 -*-

from karaoke.dto.api_result import ApiResult
from karaoke.infra.audio_layout import merge_manual_roles, parse_audio_layout, serialize_audio_layout
from karaoke.domain.playback import has_full_override, refresh_playback_mode, resolve
from karaoke.domain.prepare_policy import profile_needs_prepare
from karaoke.dto.mappers import playback_detail, song_item
from karaoke.infra.embedded import probe_and_save_layout
from karaoke.infra.repositories.history_repo import HistoryRepository
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.services.base import run_guarded
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

    async def get_detail(self, song_id: int) -> ApiResult:
        async def load():
            song = await self._songs.get(song_id)
            profile = resolve(song)
            return {
                'id': song.id,
                'display_name': song.display_name,
                'source_path': song.source_path,
                'source_origin': song.source_origin,
                'source_rel': song.source_rel,
                'is_playable': song.is_playable,
                **playback_detail(song, profile),
            }

        return await run_guarded('获取歌曲详情失败', load, not_found_msg='歌曲不存在')

    async def patch(self, song_id: int, body: dict) -> ApiResult:
        async def action():
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
            return ApiResult(data=song_item(song, profile), msg='更新成功')

        return await run_guarded('更新歌曲失败', action, not_found_msg='歌曲不存在')

    async def detect_playback(self, song_id: int) -> ApiResult:
        async def action():
            song = await self._songs.get(song_id)
            if not has_full_override(song.display_name)[0]:
                await probe_and_save_layout(song, assigned_by='auto')
            profile = await refresh_playback_mode(song)
            data = playback_detail(song, profile)
            if profile_needs_prepare(song, profile):
                prep = await self._prepare.schedule(song_id)
                data = {**data, 'prepare': prep}
            return ApiResult(data=data, msg='播放能力检测完成')

        return await run_guarded('检测播放能力失败', action, not_found_msg='歌曲不存在')

    async def request_prepare(self, song_id: int, wait: bool = False) -> ApiResult:
        async def action():
            song = await self._songs.get(song_id)
            if has_full_override(song.display_name)[0]:
                profile = resolve(song)
                return ApiResult(
                    data=playback_detail(song, profile),
                    msg='已有 __override__ 三件套，无需预生成内嵌缓存',
                )
            prep = await self._prepare.schedule(song_id)
            if wait:
                prep = await self._prepare.wait_until_ready(song_id)
            profile = await refresh_playback_mode(song)
            data = {
                **playback_detail(song, profile),
                'prepare': prep,
                'cache_ready': prep.get('ready', False),
            }
            if prep.get('ready'):
                return ApiResult(data=data, msg='缓存已就绪')
            if prep.get('status') in ('pending', 'running'):
                return ApiResult(data=data, msg='正在后台生成缓存')
            return ApiResult.fail(prep.get('error') or '缓存生成失败', data=data)

        return await run_guarded('预生成缓存失败', action, not_found_msg='歌曲不存在')
