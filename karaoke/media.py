#!/usr/bin/env python
# -*- coding: utf-8 -*-

import json
import os
import subprocess


def probe_video_playable(file_path: str) -> bool:
    if not os.path.isfile(file_path):
        return False
    try:
        result = subprocess.run(
            [
                'ffprobe', '-v', 'error',
                '-select_streams', 'v:0',
                '-show_entries', 'stream=codec_type',
                '-show_entries', 'format=duration',
                '-of', 'json',
                file_path,
            ],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if result.returncode != 0:
            return False
        data = json.loads(result.stdout or '{}')
        streams = data.get('streams') or []
        if not streams:
            return False
        duration = float((data.get('format') or {}).get('duration') or 0)
        return duration > 0
    except (OSError, ValueError, json.JSONDecodeError, subprocess.TimeoutExpired):
        return False


def file_ext(path: str) -> str:
    _, ext = os.path.splitext(path)
    return ext.lstrip('.').lower()
