#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""播放资源准备判定（点歌 / 后台任务共用）。"""

from karaoke.audio_layout import has_dual_roles, parse_audio_layout
from karaoke.domain.playback import PlaybackProfile, has_full_override, resolve
from karaoke.media import can_play_directly
from karaoke.models import Song


def profile_needs_prepare(song: Song, profile: PlaybackProfile = None) -> bool:
    """判断歌曲是否需后台 prepare（内嵌拆轨或 plain 浏览器转码）。"""
    if has_full_override(song.display_name)[0]:
        return False
    if profile is None:
        profile = resolve(song, prepare_embedded=False)
    if profile.playback_source == 'embedded':
        layout = parse_audio_layout(song.audio_layout)
        return bool(layout and has_dual_roles(layout) and not profile.embedded_cache_ready)
    if profile.mode == 'plain' and profile.video_path:
        return not can_play_directly(profile.video_path)
    return False
