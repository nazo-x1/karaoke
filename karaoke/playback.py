#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
from dataclasses import dataclass
from typing import Optional

from karaoke.audio_layout import has_dual_roles, parse_audio_layout
from karaoke.embedded import ensure_embedded_cache, embedded_cache_ready
from karaoke.media import predict_stream_mime, resolve_browser_video_path, video_mime_for_ext, file_ext
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
    has_override_video: bool = False
    has_override_vocals: bool = False
    has_override_accompaniment: bool = False
    has_source: bool = False


def override_triplet_paths(display_name: str) -> dict:
    base = os.path.join(OVERRIDE_PATH, display_name)
    return {
        "video": f"{base}.mp4",
        "vocals": f"{base}_vocals.mp3",
        "accompaniment": f"{base}_accompaniment.mp3",
    }


def has_full_override(display_name: str) -> tuple:
    triplet = override_triplet_paths(display_name)
    has_video = os.path.isfile(triplet["video"])
    has_vocals = os.path.isfile(triplet["vocals"])
    has_accompaniment = os.path.isfile(triplet["accompaniment"])
    return has_video and has_vocals and has_accompaniment, triplet


def _video_meta(path: Optional[str]) -> tuple:
    if not path:
        return None, None
    ext = file_ext(path)
    return predict_stream_mime(path), ext


def resolve(song: Song, prepare_embedded: bool = False) -> PlaybackProfile:
    triplet = override_triplet_paths(song.display_name)
    has_video = os.path.isfile(triplet["video"])
    has_vocals = os.path.isfile(triplet["vocals"])
    has_accompaniment = os.path.isfile(triplet["accompaniment"])
    has_source = os.path.isfile(song.source_path)
    override_ok, _ = has_full_override(song.display_name)

    # 1. __override__ 三件套优先
    if override_ok:
        mime, ext = _video_meta(triplet["video"])
        return PlaybackProfile(
            mode='enhanced',
            playback_source='override',
            can_queue=True,
            video_path=triplet["video"],
            vocals_path=triplet["vocals"],
            accompaniment_path=triplet["accompaniment"],
            video_mime=mime or 'video/mp4',
            video_ext=ext or 'mp4',
            has_override_video=True,
            has_override_vocals=True,
            has_override_accompaniment=True,
            has_source=has_source,
        )

    # 2. 无完整 override → 内嵌双轨
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
            has_source=True,
            has_override_video=has_video,
            has_override_vocals=has_vocals,
            has_override_accompaniment=has_accompaniment,
        )

    # 3. plain
    if song.is_playable and has_source:
        mime, ext = _video_meta(song.source_path)
        return PlaybackProfile(
            mode='plain',
            playback_source='plain',
            can_queue=True,
            video_path=song.source_path,
            video_mime=mime,
            video_ext=ext,
            has_source=True,
            has_override_video=has_video,
            has_override_vocals=has_vocals,
            has_override_accompaniment=has_accompaniment,
            embedded_cache_ready=embedded_cache_ready(song),
        )

    return PlaybackProfile(
        mode='not_ready',
        playback_source='plain',
        can_queue=False,
        has_source=has_source,
        has_override_video=has_video,
        has_override_vocals=has_vocals,
        has_override_accompaniment=has_accompaniment,
    )


async def refresh_playback_mode(song: Song) -> PlaybackProfile:
    profile = resolve(song)
    mode = profile.mode if profile.mode != 'not_ready' else 'plain'
    if song.playback_mode != mode:
        song.playback_mode = mode
        await song.save(update_fields=['playback_mode', 'update_time'])
    return profile


def stream_path_for_kind(song: Song, kind: str) -> Optional[str]:
    profile = resolve(song)
    if kind == 'video' and profile.video_path:
        return profile.video_path
    if kind == 'vocals' and profile.vocals_path:
        return profile.vocals_path
    if kind == 'accompaniment' and profile.accompaniment_path:
        return profile.accompaniment_path
    return None


def stream_media_for_kind(song: Song, kind: str) -> tuple:
    """返回 (文件路径, Content-Type)。"""
    profile = resolve(song, prepare_embedded=True)
    path = None
    if kind == 'video':
        path = profile.video_path
    elif kind == 'vocals':
        path = profile.vocals_path
    elif kind == 'accompaniment':
        path = profile.accompaniment_path

    if not path or not os.path.isfile(path):
        return None, None

    if kind == 'video':
        if profile.playback_source == 'embedded':
            return path, 'video/mp4'
        return resolve_browser_video_path(path)

    ext = file_ext(path)
    return path, video_mime_for_ext(ext)
