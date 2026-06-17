from karaoke.services.cache_service import CacheService

_service = CacheService()


async def clear_play_cache():
    return await _service.clear_play_cache()
