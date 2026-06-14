#!/usr/bin/env python
# -*- coding: utf-8 -*-

from karaoke.prepare_tasks import (
    get_prepare_status,
    schedule_playback_prepare,
    wait_playback_ready,
)


class PrepareService:
    async def schedule(self, song_id: int) -> dict:
        return await schedule_playback_prepare(song_id)

    async def status(self, song_id: int) -> dict:
        return await get_prepare_status(song_id)

    async def wait_until_ready(self, song_id: int, timeout: float = 3600.0) -> dict:
        return await wait_playback_ready(song_id, timeout)

    async def ensure_ready(self, song_id: int) -> dict:
        return await schedule_playback_prepare(song_id)
