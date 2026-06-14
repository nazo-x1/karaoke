#!/usr/bin/env python
# -*- coding: utf-8 -*-

from typing import List, Optional

from tortoise.exceptions import DoesNotExist

from karaoke.models import Song
from settings import PAGE_SIZE


class SongRepository:
    async def get(self, song_id: int) -> Song:
        return await Song.get(id=song_id)

    async def get_optional(self, song_id: int) -> Optional[Song]:
        try:
            return await Song.get(id=song_id)
        except DoesNotExist:
            return None

    async def find_by_source_path(self, path: str) -> Optional[Song]:
        return await Song.filter(source_path=path).first()

    async def find_by_display_name(self, name: str) -> Optional[Song]:
        return await Song.filter(display_name=name).first()

    async def all_display_names(self) -> set:
        return {s.display_name for s in await Song.all().only('display_name')}

    async def create(self, **kwargs) -> Song:
        return await Song.create(**kwargs)

    async def save(self, song: Song, fields: Optional[List[str]] = None) -> None:
        if fields:
            await song.save(update_fields=fields)
        else:
            await song.save()

    async def delete(self, song: Song) -> None:
        await song.delete()

    async def list_page(self, q: str, page: int) -> tuple:
        page = max(1, int(page))
        qs = Song.filter(display_name__contains=q) if q else Song.all()
        total_num = await qs.count()
        songs = await qs.order_by('-id').offset((page - 1) * PAGE_SIZE).limit(PAGE_SIZE)
        return songs, total_num

    async def map_by_ids(self, ids: List[int]) -> dict:
        if not ids:
            return {}
        return {s.id: s for s in await Song.filter(id__in=ids)}
