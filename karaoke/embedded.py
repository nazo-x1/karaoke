#!/usr/bin/env python
# -*- coding: utf-8 -*-

import hashlib
import json
import os
from dataclasses import dataclass
from typing import Optional

from karaoke.audio_layout import get_track_index, has_dual_roles, parse_audio_layout, serialize_audio_layout
from karaoke.media import (
    build_audio_layout,
    extract_audio_track,
    prepare_video_only_mp4,
)
from karaoke.models import Song
from settings import PLAY_CACHE_PATH, logger

EMBEDDED_CACHE_VERSION = 'v1'


@dataclass
class EmbeddedPaths:
    video: Optional[str] = None
    vocals: Optional[str] = None
    accompaniment: Optional[str] = None
    ready: bool = False
    cache_dir: str = ''


def _layout_hash(source_path: str, layout: dict) -> str:
    stat = os.stat(source_path)
    payload = json.dumps(layout, sort_keys=True, ensure_ascii=False)
    digest = hashlib.sha256(
        f"{EMBEDDED_CACHE_VERSION}:{os.path.abspath(source_path)}:"
        f"{stat.st_mtime_ns}:{stat.st_size}:{payload}".encode()
    ).hexdigest()[:24]
    return digest


def cache_dir_for(song: Song, layout: dict) -> str:
    return os.path.join(PLAY_CACHE_PATH, 'embedded', _layout_hash(song.source_path, layout))


def _manifest_path(cache_dir: str) -> str:
    return os.path.join(cache_dir, 'manifest.json')


def _read_manifest(cache_dir: str) -> Optional[dict]:
    path = _manifest_path(cache_dir)
    if not os.path.isfile(path):
        return None
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return json.load(f)
    except (OSError, json.JSONDecodeError):
        return None


def _write_manifest(cache_dir: str, song: Song, layout: dict) -> None:
    stat = os.stat(song.source_path)
    manifest = {
        'source_path': song.source_path,
        'source_mtime_ns': stat.st_mtime_ns,
        'source_size': stat.st_size,
        'layout': layout,
        'version': EMBEDDED_CACHE_VERSION,
    }
    with open(_manifest_path(cache_dir), 'w', encoding='utf-8') as f:
        json.dump(manifest, f, ensure_ascii=False)


def expected_paths(cache_dir: str) -> EmbeddedPaths:
    return EmbeddedPaths(
        video=os.path.join(cache_dir, 'video.mp4'),
        vocals=os.path.join(cache_dir, 'vocals.m4a'),
        accompaniment=os.path.join(cache_dir, 'accompaniment.m4a'),
        cache_dir=cache_dir,
    )


def is_cache_valid(song: Song, layout: dict, paths: EmbeddedPaths) -> bool:
    if not all(
        p and os.path.isfile(p) and os.path.getsize(p) > 0
        for p in (paths.video, paths.vocals, paths.accompaniment)
    ):
        return False
    manifest = _read_manifest(paths.cache_dir)
    if not manifest:
        return False
    try:
        stat = os.stat(song.source_path)
    except OSError:
        return False
    return (
        manifest.get('source_path') == song.source_path
        and manifest.get('source_mtime_ns') == stat.st_mtime_ns
        and manifest.get('source_size') == stat.st_size
        and manifest.get('layout') == layout
    )


def ensure_embedded_cache(song: Song, layout: dict, prepare: bool = False) -> EmbeddedPaths:
    cache_dir = cache_dir_for(song, layout)
    paths = expected_paths(cache_dir)
    paths.cache_dir = cache_dir

    if is_cache_valid(song, layout, paths):
        paths.ready = True
        return paths

    if not prepare:
        paths.ready = False
        return paths

    os.makedirs(cache_dir, exist_ok=True)
    vocals_idx = get_track_index(layout, 'vocals')
    accomp_idx = get_track_index(layout, 'accompaniment')
    if vocals_idx is None or accomp_idx is None:
        paths.ready = False
        return paths

    ok_video = prepare_video_only_mp4(song.source_path, paths.video)
    ok_vocals = extract_audio_track(song.source_path, vocals_idx, paths.vocals)
    ok_accomp = extract_audio_track(song.source_path, accomp_idx, paths.accompaniment)

    if ok_video and ok_vocals and ok_accomp:
        _write_manifest(cache_dir, song, layout)
        paths.ready = True
        logger.info('embedded cache ready: song=%s dir=%s', song.id, cache_dir)
    else:
        paths.ready = False
        logger.warning('embedded cache incomplete: song=%s', song.id)

    return paths


async def probe_and_save_layout(song: Song, assigned_by: str = 'auto') -> dict:
    if not os.path.isfile(song.source_path):
        layout = {'tracks': [], 'layout': 'unknown', 'assigned_by': assigned_by}
    elif assigned_by == 'manual':
        existing = parse_audio_layout(song.audio_layout)
        layout = existing or build_audio_layout(song.source_path, assigned_by='auto')
    else:
        layout = build_audio_layout(song.source_path, assigned_by=assigned_by)
    song.audio_layout = serialize_audio_layout(layout)
    await song.save(update_fields=['audio_layout', 'update_time'])
    return layout


def embedded_cache_ready(song: Song) -> bool:
    layout = parse_audio_layout(song.audio_layout)
    if not layout or not has_dual_roles(layout):
        return False
    paths = ensure_embedded_cache(song, layout, prepare=False)
    return paths.ready
