from typing import Optional

from fastapi import Body

from karaoke.services.song_config_service import SongConfigService

_config = SongConfigService()


async def get_song(song_id: int):
    return await _config.get_detail(song_id)


async def patch_song(song_id: int, body: dict = Body(...)):
    return await _config.patch(song_id, body)


async def detect_playback(song_id: int):
    return await _config.detect_playback(song_id)


async def prepare_embedded(song_id: int, wait: bool = False):
    return await _config.request_prepare(song_id, wait=wait)
