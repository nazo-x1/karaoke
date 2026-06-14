#!/usr/bin/env python
# -*- coding: utf-8 -*-

import json
from typing import Any, Dict, List, Optional, Tuple

VALID_ROLES = frozenset({'vocals', 'accompaniment', 'full', 'ignore', 'unknown'})


def parse_audio_layout(raw: Optional[str]) -> Optional[dict]:
    if not raw:
        return None
    try:
        data = json.loads(raw)
        return data if isinstance(data, dict) else None
    except (json.JSONDecodeError, TypeError):
        return None


def serialize_audio_layout(layout: dict) -> str:
    return json.dumps(layout, ensure_ascii=False)


def has_dual_roles(layout: Optional[dict]) -> bool:
    if not layout:
        return False
    tracks = layout.get('tracks') or []
    roles = {t.get('role') for t in tracks if t.get('role') != 'ignore'}
    return 'vocals' in roles and 'accompaniment' in roles


def get_track_index(layout: Optional[dict], role: str) -> Optional[int]:
    if not layout:
        return None
    for track in layout.get('tracks') or []:
        if track.get('role') == role:
            return track.get('index')
    return None


def merge_manual_roles(current: Optional[dict], tracks_update: List[dict]) -> dict:
    layout = dict(current or {})
    existing = {t['index']: t for t in layout.get('tracks') or [] if 'index' in t}
    for item in tracks_update:
        idx = item.get('index')
        role = item.get('role')
        if idx is None or role not in VALID_ROLES:
            continue
        if idx in existing:
            existing[idx]['role'] = role
        else:
            existing[idx] = {'index': idx, 'role': role}
    tracks = sorted(existing.values(), key=lambda t: t['index'])
    layout['tracks'] = tracks
    layout['assigned_by'] = 'manual'
    layout['layout'] = _infer_layout_type(tracks)
    return layout


def _infer_layout_type(tracks: List[dict]) -> str:
    roles = [t.get('role') for t in tracks if t.get('role') != 'ignore']
    if 'vocals' in roles and 'accompaniment' in roles:
        return 'dual'
    active = [r for r in roles if r not in ('ignore', 'unknown')]
    if len(active) == 1 and active[0] == 'full':
        return 'single'
    if len([t for t in tracks if t.get('role') not in ('ignore', 'unknown', None)]) == 1:
        return 'single'
    if has_dual_roles({'tracks': tracks}):
        return 'dual'
    return 'unknown'


def layout_summary(layout: Optional[dict]) -> dict:
    if not layout:
        return {'layout': 'unknown', 'track_count': 0}
    tracks = layout.get('tracks') or []
    return {
        'layout': layout.get('layout', 'unknown'),
        'assigned_by': layout.get('assigned_by', 'auto'),
        'track_count': len(tracks),
        'tracks': tracks,
    }
