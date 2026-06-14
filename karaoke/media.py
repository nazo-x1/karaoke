#!/usr/bin/env python
# -*- coding: utf-8 -*-

import hashlib
import json
import os
import subprocess
from dataclasses import dataclass
from typing import Optional, Tuple

from settings import CONTENT_TYPE, PLAY_CACHE_PATH


# 浏览器 <video> 通常可直接播放的容器（仍取决于内部编码）
NATIVE_VIDEO_EXTS = frozenset({'mp4', 'm4v', 'webm', 'mov'})

# 常见且浏览器普遍支持的 video codec（小写）
BROWSER_VIDEO_CODECS = frozenset({
    'h264', 'avc1', 'avc', 'vp8', 'vp9', 'av1', 'theora',
})

BROWSER_AUDIO_CODECS = frozenset({
    'aac', 'mp3', 'mp4a', 'opus', 'vorbis', 'flac',
})


@dataclass
class MediaInfo:
    ext: str
    format_name: str
    duration: float
    video_codec: Optional[str]
    audio_codec: Optional[str]
    has_video: bool
    has_audio: bool


def file_ext(path: str) -> str:
    _, ext = os.path.splitext(path)
    return ext.lstrip('.').lower()


def video_mime_for_ext(ext: str) -> str:
    return CONTENT_TYPE.get(ext, 'application/octet-stream')


def probe_video_playable(file_path: str) -> bool:
    info = probe_media_info(file_path)
    return info is not None and info.has_video and info.duration > 0


def probe_media_info(file_path: str) -> Optional[MediaInfo]:
    if not os.path.isfile(file_path):
        return None
    try:
        result = subprocess.run(
            [
                'ffprobe', '-v', 'error',
                '-show_entries', 'stream=codec_name,codec_type',
                '-show_entries', 'format=format_name,duration',
                '-of', 'json',
                file_path,
            ],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if result.returncode != 0:
            return None
        data = json.loads(result.stdout or '{}')
        video_codec = None
        audio_codec = None
        for stream in data.get('streams') or []:
            codec = (stream.get('codec_name') or '').lower()
            if stream.get('codec_type') == 'video' and not video_codec:
                video_codec = codec
            if stream.get('codec_type') == 'audio' and not audio_codec:
                audio_codec = codec
        fmt = data.get('format') or {}
        duration = float(fmt.get('duration') or 0)
        return MediaInfo(
            ext=file_ext(file_path),
            format_name=(fmt.get('format_name') or '').lower(),
            duration=duration,
            video_codec=video_codec,
            audio_codec=audio_codec,
            has_video=video_codec is not None,
            has_audio=audio_codec is not None,
        )
    except (OSError, ValueError, json.JSONDecodeError, subprocess.TimeoutExpired):
        return None


def _codec_supported(codec: Optional[str], allowed: frozenset) -> bool:
    if not codec:
        return True
    base = codec.split('.')[0].lower()
    return base in allowed or any(base.startswith(a) for a in allowed)


def can_play_directly(file_path: str) -> bool:
    info = probe_media_info(file_path)
    if not info or not info.has_video:
        return False
    if info.ext not in NATIVE_VIDEO_EXTS:
        return False
    if not _codec_supported(info.video_codec, BROWSER_VIDEO_CODECS):
        return False
    if info.has_audio and not _codec_supported(info.audio_codec, BROWSER_AUDIO_CODECS):
        return False
    return True


def _cache_path(source_path: str) -> str:
    stat = os.stat(source_path)
    digest = hashlib.sha256(
        f"{os.path.abspath(source_path)}:{stat.st_mtime_ns}:{stat.st_size}".encode()
    ).hexdigest()[:20]
    return os.path.join(PLAY_CACHE_PATH, f"{digest}.mp4")


def remux_to_mp4(source_path: str, dest_path: str) -> bool:
    os.makedirs(os.path.dirname(dest_path), exist_ok=True)
    try:
        subprocess.run(
            [
                'ffmpeg', '-y', '-nostdin', '-i', source_path,
                '-map', '0:v:0?', '-map', '0:a:0?',
                '-c', 'copy', '-movflags', '+faststart',
                dest_path,
            ],
            capture_output=True,
            text=True,
            timeout=600,
            check=True,
        )
        return os.path.isfile(dest_path) and os.path.getsize(dest_path) > 0
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        if os.path.isfile(dest_path):
            try:
                os.remove(dest_path)
            except OSError:
                pass
        return False


def resolve_browser_video_path(source_path: str) -> Tuple[str, str]:
    """返回 (实际读取路径, Content-Type)。"""
    if not os.path.isfile(source_path):
        return source_path, video_mime_for_ext(file_ext(source_path))

    ext = file_ext(source_path)
    if can_play_directly(source_path):
        return source_path, video_mime_for_ext(ext)

    cached = _cache_path(source_path)
    try:
        if os.path.isfile(cached) and os.path.getmtime(cached) >= os.path.getmtime(source_path):
            return cached, 'video/mp4'
    except OSError:
        pass

    if remux_to_mp4(source_path, cached):
        return cached, 'video/mp4'

    return source_path, video_mime_for_ext(ext)


def predict_stream_mime(source_path: str) -> str:
    if not os.path.isfile(source_path):
        return video_mime_for_ext(file_ext(source_path))
    if can_play_directly(source_path):
        return video_mime_for_ext(file_ext(source_path))
    return 'video/mp4'
