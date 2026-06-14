#!/usr/bin/env python
# -*- coding: utf-8 -*-

from enum import IntEnum
from typing import List

from karaoke.models import History


class QueueState(IntEnum):
    """队列条目状态（与 History.is_sing 存库值一致）。"""
    PENDING = 0
    SUNG = 1
    SINGING = -1


STATE_LABELS = {
    QueueState.PENDING: 'pending',
    QueueState.SUNG: 'sung',
    QueueState.SINGING: 'playing',
}


def queue_state_label(is_sing: int) -> str:
    try:
        return STATE_LABELS[QueueState(is_sing)]
    except ValueError:
        return STATE_LABELS[QueueState.PENDING]


def is_playing(is_sing: int) -> bool:
    return is_sing == QueueState.SINGING


def sort_pending(rows: List[History]) -> List[History]:
    if not rows:
        return []

    def _sort_key(h: History):
        if h.is_sing == QueueState.SINGING:
            return (0, 0.0)
        if h.is_top == 1:
            return (1, -h.update_time.timestamp())
        return (2, h.update_time.timestamp())

    return sorted(rows, key=_sort_key)
