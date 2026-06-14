#!/usr/bin/env python
# -*- coding: utf-8 -*-

import hashlib
import json
import os
import subprocess
from dataclasses import dataclass, field
from typing import List, Optional, Tuple

from settings import CONTENT_TYPE, PLAY_CACHE_PATH, logger


NATIVE_VIDEO_EXTS = frozenset({'mp4', 'm4v', 'webm', 'mov'})

# remux 进 mp4 后浏览器可解码的编码
BROWSER_MP4_VIDEO_CODECS = frozenset({'h264', 'avc1', 'avc'})

BROWSER_AUDIO_CODECS = frozenset({
    'aac', 'mp3', 'mp4a', 'opus', 'vorbis', 'flac',
})

# 必须转码为 H.264 的编码（VLC 能播但浏览器通常只有声音/黑屏）
TRANSCODE_VIDEO_CODECS = frozenset({
    'hevc', 'h265', 'mpeg4', 'msmpeg4v3', 'msmpeg4v2', 'mpeg2video',
    'vc1', 'wmv3', 'wmv2', 'rv40', 'theora', 'vp8', 'vp9', 'av1',
    'mjpeg', 'png', 'bmp',
})

CACHE_VERSION = 'v3'


@dataclass
class StreamInfo:
    index: int
    codec_type: str
    codec_name: str
    width: int = 0
    height: int = 0
    pix_fmt: str = ''
    attached_pic: bool = False


@dataclass
class MediaInfo:
    ext: str
    format_name: str
    duration: float
    video_codec: Optional[str]
    audio_codec: Optional[str]
    has_video: bool
    has_audio: bool
    video_width: int = 0
    video_height: int = 0
    pix_fmt: str = ''
    streams: List[StreamInfo] = field(default_factory=list)


def file_ext(path: str) -> str:
    _, ext = os.path.splitext(path)
    return ext.lstrip('.').lower()


def video_mime_for_ext(ext: str) -> str:
    return CONTENT_TYPE.get(ext, 'application/octet-stream')


def _run_ffprobe(args: list) -> Optional[dict]:
    try:
        result = subprocess.run(
            ['ffprobe', '-v', 'error', *args],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if result.returncode != 0:
            return None
        return json.loads(result.stdout or '{}')
    except (OSError, ValueError, json.JSONDecodeError, subprocess.TimeoutExpired):
        return None


def probe_streams(file_path: str) -> List[StreamInfo]:
    data = _run_ffprobe([
        '-show_entries', 'stream=index,codec_type,codec_name,width,height,pix_fmt',
        '-show_entries', 'stream_disposition=attached_pic',
        '-of', 'json',
        file_path,
    ])
    if not data:
        return []
    streams = []
    for raw in data.get('streams') or []:
        disp = raw.get('disposition') or {}
        streams.append(StreamInfo(
            index=int(raw.get('index', 0)),
            codec_type=raw.get('codec_type') or '',
            codec_name=(raw.get('codec_name') or '').lower(),
            width=int(raw.get('width') or 0),
            height=int(raw.get('height') or 0),
            pix_fmt=(raw.get('pix_fmt') or '').lower(),
            attached_pic=int(disp.get('attached_pic') or 0) == 1,
        ))
    return streams


def probe_media_info(file_path: str) -> Optional[MediaInfo]:
    if not os.path.isfile(file_path):
        return None
    streams = probe_streams(file_path)
    if not streams:
        return None
    data = _run_ffprobe([
        '-show_entries', 'format=format_name,duration',
        '-of', 'json',
        file_path,
    ])
    fmt = (data or {}).get('format') or {}
    duration = float(fmt.get('duration') or 0)

    video = pick_main_video_stream(streams)
    audio = pick_main_audio_stream(streams)
    return MediaInfo(
        ext=file_ext(file_path),
        format_name=(fmt.get('format_name') or '').lower(),
        duration=duration,
        video_codec=video.codec_name if video else None,
        audio_codec=audio.codec_name if audio else None,
        has_video=video is not None,
        has_audio=audio is not None,
        video_width=video.width if video else 0,
        video_height=video.height if video else 0,
        pix_fmt=video.pix_fmt if video else '',
        streams=streams,
    )


def probe_video_playable(file_path: str) -> bool:
    info = probe_media_info(file_path)
    return info is not None and info.has_video and info.duration > 0


def pick_main_video_stream(streams: List[StreamInfo]) -> Optional[StreamInfo]:
    candidates = []
    for s in streams:
        if s.codec_type != 'video':
            continue
        if s.attached_pic:
            continue
        if s.width < 32 or s.height < 32:
            continue
        if s.codec_name in ('mjpeg', 'png', 'bmp', 'gif'):
            continue
        candidates.append(s)
    if not candidates:
        for s in streams:
            if s.codec_type == 'video' and not s.attached_pic:
                candidates.append(s)
    if not candidates:
        return None
    return max(candidates, key=lambda s: s.width * s.height)


def pick_main_audio_stream(streams: List[StreamInfo]) -> Optional[StreamInfo]:
    audios = [s for s in streams if s.codec_type == 'audio']
    return audios[0] if audios else None


def _codec_base(codec: Optional[str]) -> str:
    if not codec:
        return ''
    return codec.split('.')[0].lower()


def _is_10bit_or_highpix(pix_fmt: str) -> bool:
    if not pix_fmt:
        return False
    return '10' in pix_fmt or '12' in pix_fmt or pix_fmt in ('yuv422p', 'yuv444p', 'gbrp')


def _needs_transcode(info: MediaInfo, video: StreamInfo) -> bool:
    codec = _codec_base(video.codec_name)
    if codec in TRANSCODE_VIDEO_CODECS:
        return True
    if codec in BROWSER_MP4_VIDEO_CODECS and _is_10bit_or_highpix(video.pix_fmt):
        return True
    if codec in BROWSER_MP4_VIDEO_CODECS:
        return False
    return True


def _codec_supported(codec: Optional[str], allowed: frozenset) -> bool:
    if not codec:
        return True
    base = _codec_base(codec)
    return base in allowed


def can_play_directly(file_path: str) -> bool:
    info = probe_media_info(file_path)
    if not info or not info.has_video:
        return False
    if info.ext not in NATIVE_VIDEO_EXTS:
        return False
    video = pick_main_video_stream(info.streams)
    if not video:
        return False
    if _needs_transcode(info, video):
        return False
    if info.has_audio and not _codec_supported(info.audio_codec, BROWSER_AUDIO_CODECS):
        return False
    return True


def _cache_path(source_path: str) -> str:
    stat = os.stat(source_path)
    digest = hashlib.sha256(
        f"{CACHE_VERSION}:{os.path.abspath(source_path)}:{stat.st_mtime_ns}:{stat.st_size}".encode()
    ).hexdigest()[:20]
    return os.path.join(PLAY_CACHE_PATH, f"{digest}.mp4")


def _validate_browser_mp4(path: str) -> bool:
    info = probe_media_info(path)
    if not info or not info.has_video:
        return False
    video = pick_main_video_stream(info.streams)
    if not video or video.width < 32 or video.height < 32:
        return False
    codec = _codec_base(video.codec_name)
    if codec not in BROWSER_MP4_VIDEO_CODECS:
        return False
    if _is_10bit_or_highpix(video.pix_fmt):
        return False
    return True


def _ffmpeg_maps(video: StreamInfo, audio: Optional[StreamInfo]) -> list:
    maps = ['-map', f'0:{video.index}']
    if audio:
        maps.extend(['-map', f'0:{audio.index}'])
    return maps


def remux_to_mp4(source_path: str, dest_path: str, video: StreamInfo, audio: Optional[StreamInfo]) -> bool:
    os.makedirs(os.path.dirname(dest_path), exist_ok=True)
    try:
        cmd = [
            'ffmpeg', '-y', '-nostdin', '-i', source_path,
            *_ffmpeg_maps(video, audio),
            '-c', 'copy',
            '-tag:v', 'avc1',
            '-movflags', '+faststart',
            dest_path,
        ]
        subprocess.run(cmd, capture_output=True, text=True, timeout=600, check=True)
        return _validate_browser_mp4(dest_path)
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
        logger.warning('remux failed %s: %s', source_path, e)
        if os.path.isfile(dest_path):
            try:
                os.remove(dest_path)
            except OSError:
                pass
        return False


def transcode_to_mp4(source_path: str, dest_path: str, video: StreamInfo, audio: Optional[StreamInfo]) -> bool:
    os.makedirs(os.path.dirname(dest_path), exist_ok=True)
    try:
        cmd = [
            'ffmpeg', '-y', '-nostdin', '-i', source_path,
            *_ffmpeg_maps(video, audio),
            '-c:v', 'libx264', '-preset', 'veryfast', '-crf', '23',
            '-pix_fmt', 'yuv420p', '-profile:v', 'high', '-level', '4.1',
            '-tag:v', 'avc1',
            '-c:a', 'aac', '-b:a', '192k', '-ac', '2',
            '-movflags', '+faststart',
            dest_path,
        ]
        subprocess.run(cmd, capture_output=True, text=True, timeout=3600, check=True)
        return _validate_browser_mp4(dest_path)
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
        logger.warning('transcode failed %s: %s', source_path, e)
        if os.path.isfile(dest_path):
            try:
                os.remove(dest_path)
            except OSError:
                pass
        return False


def _prepare_browser_mp4(source_path: str, dest_path: str) -> bool:
    info = probe_media_info(source_path)
    if not info:
        return False
    video = pick_main_video_stream(info.streams)
    if not video:
        logger.warning('no usable video stream: %s', source_path)
        return False
    audio = pick_main_audio_stream(info.streams)

    if _needs_transcode(info, video):
        logger.info('transcode for browser: %s (%s/%s)', source_path, video.codec_name, video.pix_fmt)
        return transcode_to_mp4(source_path, dest_path, video, audio)

    logger.info('remux for browser: %s (%s)', source_path, video.codec_name)
    if remux_to_mp4(source_path, dest_path, video, audio):
        return True
    logger.info('remux failed, fallback transcode: %s', source_path)
    return transcode_to_mp4(source_path, dest_path, video, audio)


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
            if _validate_browser_mp4(cached):
                return cached, 'video/mp4'
            os.remove(cached)
    except OSError:
        pass

    if _prepare_browser_mp4(source_path, cached):
        return cached, 'video/mp4'

    return source_path, video_mime_for_ext(ext)


def predict_stream_mime(source_path: str) -> str:
    if not os.path.isfile(source_path):
        return video_mime_for_ext(file_ext(source_path))
    if can_play_directly(source_path):
        return video_mime_for_ext(file_ext(source_path))
    return 'video/mp4'
