#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""SSE 事件总线：解耦业务模块与实时推送。"""

import asyncio
import json
import traceback
from enum import IntEnum
from typing import List

from settings import logger


class EventCode(IntEnum):
    PLAYBACK_CONTROL = 1
    RESING = 2
    NEXT_SONG = 3
    VOCAL_SWITCH = 4
    VOCALS_VOLUME = 5
    ACC_VOLUME = 6
    SFX = 7
    QUEUE_CHANGED = 8
    PREPARE_READY = 9


class EventBus:
    def __init__(self) -> None:
        self._clients: List[asyncio.Queue] = []

    def subscribe(self) -> asyncio.Queue:
        queue: asyncio.Queue = asyncio.Queue()
        self._clients.append(queue)
        return queue

    def unsubscribe(self, queue: asyncio.Queue) -> None:
        try:
            self._clients.remove(queue)
        except ValueError:
            pass

    async def publish(self, code: int, data=0) -> None:
        await self.broadcast({'code': code, 'data': data})

    async def broadcast(self, payload: dict) -> None:
        message = json.dumps(payload, ensure_ascii=False)
        for client in self._clients[:]:
            try:
                await client.put(message)
            except Exception:
                logger.error(traceback.format_exc())

    async def publish_queue_changed(self) -> None:
        await self.publish(EventCode.QUEUE_CHANGED, 0)

    async def publish_prepare_ready(self, song_id: int) -> None:
        await self.publish(EventCode.PREPARE_READY, str(song_id))


event_bus = EventBus()
