#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
from dataclasses import dataclass
from typing import Optional

from karaoke.audio_layout import has_dual_roles, parse_audio_layout
from karaoke.embedded import ensure_embedded_cache, embedded_cache_ready
from karaoke.media import resolve_browser_video_path, video_mime_for_ext, file_ext
from karaoke.models import Song
from settings import OVERRIDE_PATH


@dataclass
class PlaybackProfile:
    mode: str  # plain | enhanced | not_ready
    playback_source: str  # override | embedded | plain
    can_queue: bool
    video_path: Optional[str] = None
    vocals_path: Optional[str] = None
    accompaniment_path: Optional[str] = None
    video_mime: Optional[str] = None
    video_ext: Optional[str] = None
    embedded_cache_ready: bool = False


def override_triplet_paths(display_name: str) -> dict:
    base = os.path.join(OVERRIDE_PATH, display_name)
    return {
        "video": f"{base}.mp4",
        "vocals": f"{base}_vocals.mp3",
        "accompaniment": f"{base}_accompaniment.mp3",
    }


def override_file_status(display_name: str) -> tuple:
    """返回 (三件套是否齐全, 各文件是否存在, 路径 dict)。"""
    triplet = override_triplet_paths(display_name)
    status = {k: os.path.isfile(p) for k, p in triplet.items()}
    complete = all(status.values())
    return complete, status, triplet


def has_full_override(display_name: str) -> tuple:
    complete, _, triplet = override_file_status(display_name)
    return complete, triplet


def resolve(song: Song, prepare_embedded: bool = False) -> PlaybackProfile:
    override_ok, override_status, triplet = override_file_status(song.display_name)
    has_source = os.path.isfile(song.source_path)

    if override_ok:
        ext = file_ext(triplet["video"])
        return PlaybackProfile(
            mode='enhanced',
            playback_source='override',
            can_queue=True,
            video_path=triplet["video"],
            vocals_path=triplet["vocals"],
            accompaniment_path=triplet["accompaniment"],
            video_mime=video_mime_for_ext(ext) or 'video/mp4',
            video_ext=ext or 'mp4',
        )

    layout = parse_audio_layout(song.audio_layout)
    if has_source and layout and has_dual_roles(layout):
        paths = ensure_embedded_cache(song, layout, prepare=prepare_embedded)
        return PlaybackProfile(
            mode='enhanced',
            playback_source='embedded',
            can_queue=True,
            video_path=paths.video,
            vocals_path=paths.vocals,
            accompaniment_path=paths.accompaniment,
            video_mime='video/mp4',
            video_ext='mp4',
            embedded_cache_ready=paths.ready,
        )

    if song.is_playable and has_source:
        ext = file_ext(song.source_path)
        return PlaybackProfile(
            mode='plain',
            playback_source='plain',
            can_queue=True,
            video_path=song.source_path,
            video_mime=video_mime_for_ext(ext),
            video_ext=ext,
            embedded_cache_ready=embedded_cache_ready(song),
        )

    return PlaybackProfile(
        mode='not_ready',
        playback_source='plain',
        can_queue=False,
    )


async def refresh_playback_mode(song: Song) -> PlaybackProfile:
    profile = resolve(song)
    mode = profile.mode if profile.mode != 'not_ready' else 'plain'
    if song.playback_mode != mode:
        song.playback_mode = mode
        await song.save(update_fields=['playback_mode', 'update_time'])
    return profile


def stream_media_for_kind(song: Song, kind: str) -> tuple:
    """返回 (文件路径, Content-Type)。"""
    profile = resolve(song, prepare_embedded=True)
    path = {
        'video': profile.video_path,
        'vocals': profile.vocals_path,
        'accompaniment': profile.accompaniment_path,
    }.get(kind)

    if not path or not os.path.isfile(path):
        return None, None

    if kind == 'video':
        if profile.playback_source == 'embedded':
            return path, 'video/mp4'
        return resolve_browser_video_path(path)

    return path, video_mime_for_ext(file_ext(path))
