from fastapi import Request

from karaoke.services.playback_service import PlaybackService

_playback = PlaybackService()


async def get_profile(song_id: int):
    return await _playback.get_profile(song_id)


async def prepare_status(song_id: int):
    return await _playback.prepare_status(song_id)


async def ensure_ready(song_id: int):
    return await _playback.ensure_ready(song_id)


async def stream(request: Request, song_id: int, kind: str):
    return await _playback.stream(request, song_id, kind)


async def mark_singing(song_id: int):
    return await _playback.mark_singing(song_id)


async def mark_finished(song_id: int):
    return await _playback.mark_finished(song_id)


async def skip_if_not_ready(song_id: int):
    return await _playback.skip_if_not_ready(song_id)
