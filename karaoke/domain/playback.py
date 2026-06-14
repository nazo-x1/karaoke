#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""播放路径解析领域逻辑。"""

import os
from dataclasses import dataclass
from typing import Optional

from karaoke.infra.audio_layout import has_dual_roles, parse_audio_layout
from karaoke.infra.embedded import ensure_embedded_cache
from karaoke.infra.media import (
    resolve_browser_video_path_readonly,
    video_mime_for_ext,
    file_ext,
)
from karaoke.infra.models import Song
from settings import OVERRIDE_PATH


@dataclass
class PlaybackProfile:
    mode: str
    playback_source: str
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
    triplet = override_triplet_paths(display_name)
    status = {k: os.path.isfile(p) for k, p in triplet.items()}
    complete = all(status.values())
    return complete, status, triplet


def has_full_override(display_name: str) -> tuple:
    complete, _, triplet = override_file_status(display_name)
    return complete, triplet


def list_meta_from_song(song: Song) -> tuple:
    mode = song.playback_mode or 'plain'
    source = song.playback_source
    can_queue = song.can_queue

    if source is None:
        if mode == 'enhanced':
            layout = parse_audio_layout(song.audio_layout)
            source = 'embedded' if has_dual_roles(layout) else 'override'
        else:
            source = 'plain'

    if can_queue is None:
        can_queue = bool(song.is_playable)

    return mode, source, can_queue


def resolve(song: Song, prepare_embedded: bool = False) -> PlaybackProfile:
    override_ok, _, triplet = override_file_status(song.display_name)
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
        )

    return PlaybackProfile(
        mode='not_ready',
        playback_source='plain',
        can_queue=False,
    )


async def persist_playback_mode(song: Song, profile: PlaybackProfile) -> None:
    """将解析结果写回 Song（仅在导入/检测/prepare 完成等写场景调用）。"""
    mode = profile.mode if profile.mode != 'not_ready' else 'plain'
    fields = []
    if song.playback_mode != mode:
        song.playback_mode = mode
        fields.append('playback_mode')
    if song.playback_source != profile.playback_source:
        song.playback_source = profile.playback_source
        fields.append('playback_source')
    if song.can_queue != profile.can_queue:
        song.can_queue = profile.can_queue
        fields.append('can_queue')
    if fields:
        fields.append('update_time')
        await song.save(update_fields=fields)


async def refresh_playback_mode(song: Song) -> PlaybackProfile:
    """解析并持久化播放模式（写场景便捷方法）。"""
    profile = resolve(song)
    await persist_playback_mode(song, profile)
    return profile


def stream_media_for_kind(song: Song, kind: str) -> tuple:
    profile = resolve(song, prepare_embedded=False)
    if profile.playback_source == 'embedded' and not profile.embedded_cache_ready:
        return None, None

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
        resolved, mime = resolve_browser_video_path_readonly(path)
        if not resolved:
            return None, None
        return resolved, mime

    return path, video_mime_for_ext(file_ext(path))
