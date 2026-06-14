#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""API 载荷 TypedDict（文档与 IDE 提示，运行时仍为 dict）。"""

from typing import Literal, NotRequired, TypedDict


class PrepareStatusDto(TypedDict, total=False):
    song_id: int
    status: Literal['idle', 'pending', 'running', 'ready', 'failed', 'not_needed']
    ready: bool
    phase: str
    progress: float
    message: str
    prepare_kind: Literal['embedded', 'plain', 'none', 'unknown']
    error: NotRequired[str]


class SongItemDto(TypedDict):
    id: int
    display_name: str
    source_origin: str
    playback_mode: str
    playback_source: NotRequired[str]
    can_queue: bool
    is_playable: NotRequired[bool]
    source_path: NotRequired[str]
    create_time: str
    update_time: NotRequired[str]


class QueueItemDto(TypedDict):
    id: int
    name: str
    times: int
    state: Literal['pending', 'playing', 'sung']
    is_top: int
    playback_mode: str
