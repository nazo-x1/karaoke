#!/usr/bin/env python
# -*- coding: utf-8 -*-

from typing import Optional

from karaoke.models import Song
from karaoke.audio_layout import layout_summary, parse_audio_layout
from karaoke.domain.playback import (
    PlaybackProfile,
    list_meta_from_song,
    override_file_status,
)


def fmt_time(dt) -> str:
    return dt.strftime("%Y-%m-%d %H:%M:%S")


def effective_mode(song: Song, profile: PlaybackProfile) -> str:
    return profile.mode if profile.mode != 'not_ready' else song.playback_mode


def song_item(song: Song, profile: Optional[PlaybackProfile] = None) -> dict:
    if profile is not None:
        mode = effective_mode(song, profile)
        source = profile.playback_source
        can_queue = profile.can_queue
    else:
        mode, source, can_queue = list_meta_from_song(song)
    return {
        'id': song.id,
        'display_name': song.display_name,
        'source_origin': song.source_origin,
        'playback_mode': mode,
        'playback_source': source,
        'can_queue': can_queue,
        'is_playable': song.is_playable,
        'source_path': song.source_path,
        'create_time': fmt_time(song.create_time),
        'update_time': fmt_time(song.update_time),
    }


def playback_detail(song: Song, profile: PlaybackProfile) -> dict:
    override_complete, override_files, _ = override_file_status(song.display_name)
    return {
        'playback_mode': effective_mode(song, profile),
        'playback_source': profile.playback_source,
        'can_queue': profile.can_queue,
        'embedded_cache_ready': profile.embedded_cache_ready,
        'audio_layout': layout_summary(parse_audio_layout(song.audio_layout)),
        'override_files': override_files,
        'override_complete': override_complete,
    }


def playback_api(song: Song, profile: PlaybackProfile, prepare: Optional[dict] = None) -> dict:
    prep = prepare or {}
    ready = prep.get('ready')
    if ready is None:
        if profile.playback_source == 'embedded':
            ready = profile.embedded_cache_ready
        else:
            ready = profile.can_queue
    return {
        'id': song.id,
        'display_name': song.display_name,
        'mode': profile.mode,
        'playback_source': profile.playback_source,
        'can_queue': profile.can_queue,
        'ready_to_stream': ready,
        'prepare': prep,
        'video_mime': profile.video_mime,
        'video_ext': profile.video_ext,
        'embedded_cache_ready': profile.embedded_cache_ready,
        'streams': {
            'video': profile.video_path is not None and ready,
            'vocals': profile.vocals_path is not None and ready,
            'accompaniment': profile.accompaniment_path is not None and ready,
        },
    }


def history_item(history, song: Optional[Song] = None) -> dict:
    return {
        'id': history.id,
        'name': history.name,
        'times': history.times,
        'is_sing': history.is_sing,
        'is_top': history.is_top,
        'playback_mode': song.playback_mode if song else 'plain',
    }
