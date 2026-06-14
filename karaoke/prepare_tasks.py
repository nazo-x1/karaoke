#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""兼容层：请使用 karaoke.services.prepare_service。"""

from karaoke.services.prepare_service import (  # noqa: F401
    PrepareService,
    PrepareState,
    PrepareTaskManager,
    get_manager,
)


async def schedule_playback_prepare(song_id: int) -> dict:
    return await get_manager().schedule(song_id)


async def get_prepare_status(song_id: int) -> dict:
    return await get_manager().status(song_id)


async def wait_playback_ready(song_id: int, timeout: float = 3600.0) -> dict:
    return await get_manager().wait_until_ready(song_id, timeout)
