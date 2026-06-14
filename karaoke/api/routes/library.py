from karaoke.services.library_service import LibraryService

_library = LibraryService()


async def upload_file(query):
    return await _library.upload_file(query)


async def run_scan(body):
    return await _library.run_scan(body)


async def preview_scan(root, duplicate_policy, validate):
    return await _library.preview_scan(root, duplicate_policy, validate)


async def get_list(q: str = '', page: int = 1):
    return await _library.get_list(q, page)


async def delete_song(song_id, delete_disk=False):
    return await _library.delete_song(song_id, delete_disk)
