from karaoke.services.queue_service import QueueService

_queue = QueueService()


async def enqueue(song_id: int):
    return await _queue.enqueue(song_id)


async def list_pending():
    return await _queue.list_pending()


async def list_history(page: int = 1):
    return await _queue.list_history(page)


async def list_usually(page: int = 1):
    return await _queue.list_usually(page)


async def set_top(song_id: int):
    return await _queue.set_top(song_id)


async def remove(song_id: int):
    return await _queue.remove(song_id)
