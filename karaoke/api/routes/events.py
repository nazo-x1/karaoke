import asyncio
import json
import traceback

from fastapi import Request, Body
from sse_starlette import EventSourceResponse

from karaoke.events.bus import HEARTBEAT_INTERVAL, event_bus
from karaoke.services.playback_service import PlaybackService
from settings import logger

_playback = PlaybackService()

_HEARTBEAT_PAYLOAD = json.dumps({'code': 0, 'data': 'heartbeat'}, ensure_ascii=False)


async def sse_events(request: Request):
    client_queue = event_bus.subscribe()

    async def event_generator():
        try:
            while True:
                if await request.is_disconnected():
                    break
                try:
                    message = await asyncio.wait_for(
                        client_queue.get(), timeout=HEARTBEAT_INTERVAL
                    )
                    yield {'data': message}
                except asyncio.TimeoutError:
                    yield {'data': _HEARTBEAT_PAYLOAD}
        except Exception:
            logger.error(traceback.format_exc())
        finally:
            event_bus.unsubscribe(client_queue)

    return EventSourceResponse(event_generator())


async def send_command(code: int, data=0):
    return await _playback.send_command(code, data)


async def send_command_post(body: dict = Body(...)):
    return await _playback.send_command(body.get('code'), body.get('data', 0))
