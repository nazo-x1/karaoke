#!/usr/bin/env python
# -*- coding: utf-8 -*-

import asyncio
import json
import os
import traceback
from typing import List, Optional
from urllib.parse import quote, unquote

from tortoise.exceptions import DoesNotExist
from fastapi import Request

from karaoke.media import file_ext, probe_video_playable
from karaoke.models import Song, History
from karaoke.audio_layout import layout_summary, merge_manual_roles, parse_audio_layout, serialize_audio_layout
from karaoke.embedded import probe_and_save_layout, ensure_embedded_cache
from karaoke.playback import (
    refresh_playback_mode,
    resolve,
    stream_media_for_kind,
    override_file_status,
    has_full_override,
)
from karaoke.results import Result
from karaoke.responses import StreamResponse
from karaoke.scanner import scan_root
from settings import (
    logger,
    PAGE_SIZE,
    KEEP_PATH,
    SCAN_VIDEO_EXTS,
    DEFAULT_DUPLICATE_POLICY,
    FFPROBE_ON_IMPORT,
    CONTENT_TYPE,
)

clients: List[asyncio.Queue] = []


def _safe_filename(filename: str) -> str:
    if not filename:
        return ''
    return os.path.basename(unquote(filename.strip())).replace('\x00', '').strip()


def _stem(filename: str) -> str:
    return os.path.splitext(_safe_filename(filename))[0]


def _fmt_time(dt) -> str:
    return dt.strftime("%Y-%m-%d %H:%M:%S")


def _effective_mode(song: Song, profile) -> str:
    return profile.mode if profile.mode != 'not_ready' else song.playback_mode


def _song_item(song: Song, profile=None) -> dict:
    profile = profile or resolve(song)
    return {
        'id': song.id,
        'display_name': song.display_name,
        'source_origin': song.source_origin,
        'playback_mode': _effective_mode(song, profile),
        'playback_source': profile.playback_source,
        'can_queue': profile.can_queue,
        'is_playable': song.is_playable,
        'source_path': song.source_path,
        'create_time': _fmt_time(song.create_time),
        'update_time': _fmt_time(song.update_time),
    }


def _playback_detail(song: Song, profile) -> dict:
    override_complete, override_files, _ = override_file_status(song.display_name)
    return {
        'playback_mode': _effective_mode(song, profile),
        'playback_source': profile.playback_source,
        'can_queue': profile.can_queue,
        'embedded_cache_ready': profile.embedded_cache_ready,
        'audio_layout': layout_summary(parse_audio_layout(song.audio_layout)),
        'override_files': override_files,
        'override_complete': override_complete,
    }


def _playback_api(song: Song, profile) -> dict:
    return {
        'id': song.id,
        'display_name': song.display_name,
        'mode': profile.mode,
        'playback_source': profile.playback_source,
        'can_queue': profile.can_queue,
        'video_mime': profile.video_mime,
        'video_ext': profile.video_ext,
        'embedded_cache_ready': profile.embedded_cache_ready,
        'streams': {
            'video': profile.video_path is not None,
            'vocals': profile.vocals_path is not None,
            'accompaniment': profile.accompaniment_path is not None,
        },
    }


async def read_stream_file(file_path, start_index=0, end_index=None):
    with open(file_path, 'rb') as f:
        f.seek(start_index)
        remaining = None if end_index is None else end_index - start_index + 1
        while True:
            chunk_size = 65536 if remaining is None else min(65536, remaining)
            chunk = f.read(chunk_size)
            if not chunk:
                break
            yield chunk
            if remaining is not None:
                remaining -= len(chunk)
                if remaining <= 0:
                    break


async def broadcast_data(data: dict):
    payload = json.dumps(data, ensure_ascii=False)
    for client in clients[:]:
        try:
            await client.put(payload)
        except Exception:
            logger.error(traceback.format_exc())


async def init_history():
    try:
        for h in await History.filter(is_sing=-1):
            h.is_sing = 1
            await h.save(update_fields=['is_sing', 'update_time'])
    except Exception:
        logger.error(traceback.format_exc())


async def _build_history_list(histories: List[History]) -> List[dict]:
    if not histories:
        return []
    ids = [h.id for h in histories]
    song_map = {s.id: s for s in await Song.filter(id__in=ids)}
    items = []
    for h in histories:
        song = song_map.get(h.id)
        mode = resolve(song).mode if song else 'plain'
        if song and mode == 'not_ready':
            mode = song.playback_mode
        items.append({
            'id': h.id,
            'name': h.name,
            'times': h.times,
            'is_sing': h.is_sing,
            'is_top': h.is_top,
            'playback_mode': mode,
        })
    return items


async def _pending_histories() -> List[History]:
    singing = await History.filter(is_sing=-1)
    topped = await History.filter(is_sing=0, is_top=1).order_by('-update_time')
    waiting = await History.filter(is_sing=0, is_top=0).order_by('update_time')
    return list(singing) + list(topped) + list(waiting)


async def upload_file(query: Request) -> Result:
    result = Result()
    form = await query.form()
    upload = form.get('file')
    if not upload or not upload.filename:
        result.code = 1
        result.msg = "未选择文件"
        return result

    filename = _safe_filename(upload.filename)
    ext = file_ext(filename)
    if ext not in SCAN_VIDEO_EXTS:
        result.code = 1
        result.msg = f"不支持的格式: {ext}"
        result.data = filename
        return result

    duplicate_policy = form.get('duplicate_policy') or DEFAULT_DUPLICATE_POLICY
    display_base = _stem(filename)
    dest_path = os.path.join(KEEP_PATH, filename)

    try:
        existing = await Song.filter(source_path=dest_path).first()
        name_conflict = await Song.filter(display_name=display_base).first()

        if existing and duplicate_policy == 'skip':
            result.msg = f"{filename} 已存在，已跳过"
            result.data = display_base
            return result

        display_name = display_base
        if name_conflict and name_conflict.source_path != dest_path:
            if duplicate_policy == 'skip':
                result.msg = f"{display_name} 已存在，已跳过"
                result.data = display_base
                return result
            if duplicate_policy == 'rename':
                used = {s.display_name for s in await Song.all().only('display_name')}
                index = 2
                while display_name in used:
                    display_name = f"{display_base} ({index})"
                    index += 1
                _, ext_part = os.path.splitext(filename)
                dest_path = os.path.join(KEEP_PATH, f"{display_name}{ext_part}")

        with open(dest_path, 'wb') as f:
            f.write(upload.file.read())

        is_playable = probe_video_playable(dest_path) if FFPROBE_ON_IMPORT else True

        if existing:
            existing.display_name = display_name
            existing.is_playable = is_playable
            existing.source_origin = 'upload'
            await existing.save()
            song = existing
        elif name_conflict and duplicate_policy == 'overwrite' and name_conflict.source_path != dest_path:
            if os.path.isfile(name_conflict.source_path) and name_conflict.source_origin == 'upload':
                os.remove(name_conflict.source_path)
            name_conflict.source_path = dest_path
            name_conflict.display_name = display_name
            name_conflict.is_playable = is_playable
            name_conflict.source_origin = 'upload'
            name_conflict.source_rel = None
            await name_conflict.save()
            song = name_conflict
        else:
            song = await Song.create(
                display_name=display_name,
                source_path=dest_path,
                source_origin='upload',
                source_rel=None,
                media_kind='video',
                playback_mode='plain',
                is_playable=is_playable,
            )

        if FFPROBE_ON_IMPORT:
            await probe_and_save_layout(song)
        await refresh_playback_mode(song)
        result.msg = f"{filename} 上传成功"
        result.data = song.display_name
        logger.info(result.msg)
    except Exception:
        result.code = 1
        result.data = filename
        result.msg = "系统错误"
        logger.error(f"{filename} 上传失败\n{traceback.format_exc()}")
    return result


async def get_list(q: str, page: int) -> Result:
    result = Result()
    try:
        qs = Song.filter(display_name__contains=q) if q else Song.all()
        total_num = await qs.count()
        songs = await qs.order_by('-id').offset((page - 1) * PAGE_SIZE).limit(PAGE_SIZE)
        result.data = [_song_item(s) for s in songs]
        result.page = page
        result.total = len(result.data)
        result.totalPage = (total_num + PAGE_SIZE - 1) // PAGE_SIZE
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def get_song_detail(song_id: int) -> Result:
    result = Result()
    try:
        song = await Song.get(id=song_id)
        profile = await refresh_playback_mode(song)
        result.data = {
            'id': song.id,
            'display_name': song.display_name,
            'source_path': song.source_path,
            'source_origin': song.source_origin,
            'source_rel': song.source_rel,
            'is_playable': song.is_playable,
            **_playback_detail(song, profile),
        }
    except DoesNotExist:
        result.code = 1
        result.msg = "歌曲不存在"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def get_playback_profile(song_id: int) -> Result:
    result = Result()
    try:
        song = await Song.get(id=song_id)
        profile = await refresh_playback_mode(song)
        result.data = _playback_api(song, profile)
    except DoesNotExist:
        result.code = 1
        result.msg = "歌曲不存在"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def patch_song(song_id: int, body: dict) -> Result:
    result = Result()
    try:
        song = await Song.get(id=song_id)
        display_name = body.get('display_name')
        audio_tracks = body.get('audio_tracks')
        if display_name:
            song.display_name = display_name.strip()[:256]
            await song.save(update_fields=['display_name', 'update_time'])
            for h in await History.filter(id=song.id):
                h.name = song.display_name
                await h.save(update_fields=['name', 'update_time'])
        if audio_tracks is not None:
            layout = merge_manual_roles(parse_audio_layout(song.audio_layout), audio_tracks)
            song.audio_layout = serialize_audio_layout(layout)
            await song.save(update_fields=['audio_layout', 'update_time'])
        profile = await refresh_playback_mode(song)
        result.data = _song_item(song, profile)
        result.msg = "更新成功"
    except DoesNotExist:
        result.code = 1
        result.msg = "歌曲不存在"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def detect_playback(song_id: int) -> Result:
    result = Result()
    try:
        song = await Song.get(id=song_id)
        if not has_full_override(song.display_name)[0]:
            await probe_and_save_layout(song, assigned_by='auto')
        profile = await refresh_playback_mode(song)
        result.data = _playback_detail(song, profile)
        result.msg = "播放能力检测完成"
    except DoesNotExist:
        result.code = 1
        result.msg = "歌曲不存在"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def prepare_embedded(song_id: int) -> Result:
    result = Result()
    try:
        song = await Song.get(id=song_id)
        if has_full_override(song.display_name)[0]:
            profile = resolve(song)
            result.data = _playback_detail(song, profile)
            result.msg = "已有 __override__ 三件套，无需预生成内嵌缓存"
            return result
        layout = parse_audio_layout(song.audio_layout)
        if not layout:
            await probe_and_save_layout(song)
            layout = parse_audio_layout(song.audio_layout)
        if not layout:
            result.code = 1
            result.msg = "无法探测音轨布局"
            return result
        paths = await asyncio.to_thread(ensure_embedded_cache, song, layout, True)
        profile = await refresh_playback_mode(song)
        result.data = {
            **_playback_detail(song, profile),
            'cache_ready': paths.ready,
            'cache_dir': paths.cache_dir,
        }
        result.msg = "缓存生成完成" if paths.ready else "缓存生成失败"
        if not paths.ready:
            result.code = 1
    except DoesNotExist:
        result.code = 1
        result.msg = "歌曲不存在"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def delete_song(song_id: int, delete_disk: bool = False) -> Result:
    result = Result()
    try:
        song = await Song.get(id=song_id)
        if delete_disk and song.source_origin == 'upload' and os.path.isfile(song.source_path):
            os.remove(song.source_path)
        try:
            history = await History.get(id=song_id)
            await history.delete()
        except DoesNotExist:
            pass
        name = song.display_name
        await song.delete()
        result.msg = f"{name} 删除成功"
    except DoesNotExist:
        result.code = 1
        result.msg = "歌曲不存在"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def delete_history(song_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=song_id)
        await history.delete()
        result.msg = f"{history.name} 播放记录删除成功"
        await broadcast_data({"code": 8})
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def sing_song(song_id: int) -> Result:
    result = Result()
    try:
        song = await Song.get(id=song_id)
        profile = await refresh_playback_mode(song)
        if not profile.can_queue:
            result.code = 1
            if not song.is_playable or not os.path.isfile(song.source_path):
                result.msg = "源视频不可播放或不存在"
            else:
                result.msg = "增强资源不完整且源视频不可用"
            return result

        try:
            history = await History.get(id=song.id)
            if history.is_sing == 1:
                history.is_sing = 0
                history.is_top = 0
                await history.save(update_fields=['is_sing', 'is_top', 'update_time'])
        except DoesNotExist:
            await History.create(id=song.id, name=song.display_name, is_sing=0, is_top=0)

        await broadcast_data({"code": 8})
        result.msg = f"{song.display_name} 点歌成功"
        result.data = {'playback_mode': profile.mode}
    except DoesNotExist:
        result.code = 1
        result.msg = "歌曲不存在"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def history_list(query_type: str) -> Result:
    result = Result()
    try:
        if query_type == "history":
            histories = await History.filter(is_sing=1).order_by('-update_time').limit(200)
        elif query_type == "usually":
            histories = await History.all().order_by('-times').limit(200)
        elif query_type == "pendingAll":
            histories = await _pending_histories()
        else:
            result.code = 1
            result.msg = f"未知查询类型: {query_type}"
            return result
        result.data = await _build_history_list(histories)
        result.total = len(result.data)
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def set_top(song_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=song_id)
        history.is_top = 1
        await history.save(update_fields=['is_top', 'update_time'])
        result.msg = f"{history.name} 置顶成功"
        await broadcast_data({"code": 8})
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def set_singing(song_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=song_id)
        history.is_sing = -1
        history.is_top = 0
        await history.save(update_fields=['is_sing', 'is_top', 'update_time'])
        result.msg = f"{history.name} 设置-1成功"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def set_singed(song_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=song_id)
        history.is_sing = 1
        history.is_top = 0
        history.times += 1
        await history.save(update_fields=['is_sing', 'is_top', 'times', 'update_time'])
        result.msg = f"{history.name} 设置1成功"
        await broadcast_data({"code": 8})
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def run_scan(body: dict) -> Result:
    result = Result()
    try:
        root = body.get('root', '').strip()
        if not root:
            result.code = 1
            result.msg = "请指定扫描根路径"
            return result
        stats = await scan_root(
            root,
            duplicate_policy=body.get('duplicate_policy'),
            validate=body.get('validate'),
            dry_run=False,
        )
        result.data = stats.as_dict()
        result.msg = "扫描完成"
    except FileNotFoundError:
        result.code = 1
        result.msg = "扫描路径不存在或不可读"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def preview_scan(root: str, duplicate_policy: Optional[str], validate: Optional[bool]) -> Result:
    result = Result()
    try:
        if not root.strip():
            result.code = 1
            result.msg = "请指定扫描根路径"
            return result
        stats = await scan_root(
            root.strip(),
            duplicate_policy=duplicate_policy,
            validate=validate,
            dry_run=True,
        )
        result.data = {**stats.as_dict(), 'preview': stats.preview}
        result.msg = "预览完成"
    except FileNotFoundError:
        result.code = 1
        result.msg = "扫描路径不存在或不可读"
    except Exception:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def stream_song(request: Request, song_id: int, kind: str):
    try:
        if kind not in ('video', 'vocals', 'accompaniment'):
            return Result(code=1, msg="无效的流类型")
        song = await Song.get(id=song_id)
        file_path, media_type = await asyncio.to_thread(stream_media_for_kind, song, kind)
        if not file_path or not os.path.isfile(file_path):
            return Result(code=1, msg="文件不存在")

        file_size = os.path.getsize(file_path)
        if not media_type:
            media_type = CONTENT_TYPE.get(file_ext(file_path), 'application/octet-stream')

        range_header = request.headers.get('range')
        start = 0
        end = file_size - 1
        status_code = 200
        headers = {
            'Accept-Ranges': 'bytes',
            'Content-Disposition': f"inline; filename*=UTF-8''{quote(os.path.basename(file_path))}",
        }

        if range_header and range_header.startswith('bytes='):
            range_spec = range_header.replace('bytes=', '').split('-', 1)
            start = int(range_spec[0]) if range_spec[0] else 0
            end = int(range_spec[1]) if len(range_spec) > 1 and range_spec[1] else file_size - 1
            end = min(end, file_size - 1)
            status_code = 206
            headers['Content-Range'] = f'bytes {start}-{end}/{file_size}'
            headers['Content-Length'] = str(end - start + 1)
        else:
            headers['Content-Length'] = str(file_size)

        return StreamResponse(
            read_stream_file(file_path, start, end),
            status_code=status_code,
            media_type=media_type,
            headers=headers,
        )
    except DoesNotExist:
        return Result(code=1, msg="歌曲不存在")
    except Exception:
        logger.error(traceback.format_exc())
        return Result(code=1, msg="系统错误")
