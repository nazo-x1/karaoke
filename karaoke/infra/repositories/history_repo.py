#!/usr/bin/env python
# -*- coding: utf-8 -*-

from typing import List, Optional

from tortoise.exceptions import DoesNotExist

from karaoke.domain.queue_policy import QueueState, sort_pending
from settings import PAGE_SIZE
from karaoke.infra.models import History


class HistoryRepository:
    async def get(self, song_id: int) -> History:
        return await History.get(id=song_id)

    async def get_optional(self, song_id: int) -> Optional[History]:
        try:
            return await History.get(id=song_id)
        except DoesNotExist:
            return None

    async def create(self, **kwargs) -> History:
        return await History.create(**kwargs)

    async def save(self, history: History, fields: List[str]) -> None:
        await history.save(update_fields=fields)

    async def delete(self, history: History) -> None:
        await history.delete()

    async def list_for_song(self, song_id: int) -> List[History]:
        return await History.filter(id=song_id)

    async def list_pending(self) -> List[History]:
        rows = await History.filter(is_sing__in=[QueueState.SINGING, QueueState.PENDING])
        return sort_pending(rows)

    async def list_history(self, limit: int = 200) -> List[History]:
        return await History.filter(is_sing=QueueState.SUNG).order_by('-update_time').limit(limit)

    async def list_history_page(self, page: int) -> tuple:
        page = max(1, int(page))
        qs = History.filter(is_sing=QueueState.SUNG).order_by('-update_time')
        total = await qs.count()
        rows = await qs.offset((page - 1) * PAGE_SIZE).limit(PAGE_SIZE)
        return rows, total

    async def list_usually(self, limit: int = 200) -> List[History]:
        return await History.all().order_by('-times').limit(limit)

    async def list_usually_page(self, page: int) -> tuple:
        page = max(1, int(page))
        qs = History.all().order_by('-times')
        total = await qs.count()
        rows = await qs.offset((page - 1) * PAGE_SIZE).limit(PAGE_SIZE)
        return rows, total

    async def reset_stale_singing(self) -> None:
        for h in await History.filter(is_sing=QueueState.SINGING):
            h.is_sing = QueueState.SUNG
            await h.save(update_fields=['is_sing', 'update_time'])
