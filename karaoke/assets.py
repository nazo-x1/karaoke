#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
import subprocess

from settings import FILE_PATH


def parse_song_name(filename: str) -> str | None:
    if filename.endswith('_vocals.mp3'):
        return filename[:-len('_vocals.mp3')]
    if filename.endswith('_accompaniment.mp3'):
        return filename[:-len('_accompaniment.mp3')]
    if filename.endswith('.mp4'):
        return filename[:-len('.mp4')]
    return None


def song_paths(name: str, base_dir: str = FILE_PATH) -> dict:
    return {
        'video': os.path.join(base_dir, f'{name}.mp4'),
        'vocals': os.path.join(base_dir, f'{name}_vocals.mp3'),
        'accompaniment': os.path.join(base_dir, f'{name}_accompaniment.mp3'),
    }


def has_embedded_audio(mp4_path: str) -> bool:
    if not os.path.isfile(mp4_path):
        return False
    try:
        result = subprocess.run(
            [
                'ffprobe', '-v', 'error', '-select_streams', 'a',
                '-show_entries', 'stream=index', '-of', 'csv=p=0', mp4_path,
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        return bool(result.stdout.strip())
    except Exception:
        return False


def get_song_assets(name: str, probe_embedded: bool = False) -> dict:
    paths = song_paths(name)
    has_video = os.path.isfile(paths['video'])
    has_vocals = os.path.isfile(paths['vocals'])
    has_accompaniment = os.path.isfile(paths['accompaniment'])
    can_switch = has_vocals and has_accompaniment
    if can_switch:
        play_mode = 'full'
    elif has_video and probe_embedded and has_embedded_audio(paths['video']):
        play_mode = 'embedded'
    elif has_video:
        play_mode = 'silent'
    else:
        play_mode = 'none'
    return {
        'has_video': has_video,
        'has_vocals': has_vocals,
        'has_accompaniment': has_accompaniment,
        'can_switch': can_switch,
        'play_mode': play_mode,
    }


def sync_is_sing_flag(has_video: bool) -> int:
    return 1 if has_video else 0
