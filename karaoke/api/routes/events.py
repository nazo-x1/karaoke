import traceback

from fastapi import Request, Body
from sse_starlette import EventSourceResponse

from karaoke.events.bus import event_bus
from karaoke.services.playback_service import PlaybackService
from settings import logger

_playback = PlaybackService()


async def sse_events(request: Request):
    client_queue = event_bus.subscribe()

    async def event_generator():
        try:
            while True:
                if await request.is_disconnected():
                    break
                message = await client_queue.get()
                yield message
        except Exception:
            logger.error(traceback.format_exc())
        finally:
            event_bus.unsubscribe(client_queue)

    return EventSourceResponse(event_generator())


async def send_command(code: int, data=0):
    return await _playback.send_command(code, data)


async def send_command_post(body: dict = Body(...)):
    return await _playback.send_command(body.get('code'), body.get('data', 0))
