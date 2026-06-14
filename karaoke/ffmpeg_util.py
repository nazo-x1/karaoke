#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
import subprocess

FFMPEG_CMD = 'ffmpeg'


def remove_if_exists(file_path: str):
    if os.path.isfile(file_path):
        os.remove(file_path)


def run_ffmpeg(args: list):
    cmd = [FFMPEG_CMD, '-y', '-nostdin'] + args
    subprocess.run(cmd, check=True, capture_output=True, text=True)


def probe_duration(file_path: str) -> float:
    result = subprocess.run(
        [
            'ffprobe', '-v', 'error', '-show_entries', 'format=duration',
            '-of', 'default=noprint_wrappers=1:nokey=1', file_path,
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    return float(result.stdout.strip())
