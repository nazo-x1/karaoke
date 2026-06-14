#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
from dataclasses import dataclass
from typing import Optional

from karaoke.media import predict_stream_mime, resolve_browser_video_path, video_mime_for_ext, file_ext
from karaoke.models import Song
from settings import OVERRIDE_PATH


@dataclass
class PlaybackProfile:
    mode: str  # plain | enhanced | not_ready
    can_queue: bool
    video_path: Optional[str] = None
    vocals_path: Optional[str] = None
    accompaniment_path: Optional[str] = None
    video_mime: Optional[str] = None
    video_ext: Optional[str] = None
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


def _video_meta(path: Optional[str]) -> tuple:
    if not path:
        return None, None
    ext = file_ext(path)
    return predict_stream_mime(path), ext


def resolve(song: Song) -> PlaybackProfile:
    triplet = override_triplet_paths(song.display_name)
    has_video = os.path.isfile(triplet["video"])
    has_vocals = os.path.isfile(triplet["vocals"])
    has_accompaniment = os.path.isfile(triplet["accompaniment"])
    has_source = os.path.isfile(song.source_path)

    if has_video and has_vocals and has_accompaniment:
        mime, ext = _video_meta(triplet["video"])
        return PlaybackProfile(
            mode='enhanced',
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

    if song.is_playable and has_source:
        mime, ext = _video_meta(song.source_path)
        return PlaybackProfile(
            mode='plain',
            can_queue=True,
            video_path=song.source_path,
            video_mime=mime,
            video_ext=ext,
            has_source=True,
            has_override_video=has_video,
            has_override_vocals=has_vocals,
            has_override_accompaniment=has_accompaniment,
        )

    return PlaybackProfile(
        mode='not_ready',
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
    path = stream_path_for_kind(song, kind)
    if not path:
        return None, None
    if kind == 'video':
        return resolve_browser_video_path(path)
    ext = file_ext(path)
    return path, video_mime_for_ext(ext)
