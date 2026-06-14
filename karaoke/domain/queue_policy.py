#!/usr/bin/env python
# -*- coding: utf-8 -*-

from typing import List

from karaoke.models import History


def sort_pending(rows: List[History]) -> List[History]:
    if not rows:
        return []

    def _sort_key(h: History):
        if h.is_sing == -1:
            return (0, 0.0)
        if h.is_top == 1:
            return (1, -h.update_time.timestamp())
        return (2, h.update_time.timestamp())

    return sorted(rows, key=_sort_key)
