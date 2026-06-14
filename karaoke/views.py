#!/usr/bin/env python
# -*- coding: utf-8 -*-
# @Author: leeyoshinari

import json
import os
import os.path
import asyncio
import traceback
from typing import List
from urllib.parse import unquote
from tortoise.exceptions import DoesNotExist
from karaoke.results import Result
from karaoke.models import Files, History, FileList, HistoryList
from karaoke.assets import get_song_assets, sync_is_sing_flag, parse_song_name
from karaoke.ffmpeg_util import remove_if_exists, run_ffmpeg
from karaoke.pipeline import (
    get_job, submit_from_upload, submit_from_library, import_job_to_library,
)
from karaoke.import_scan import scan_directory, register_song_after_import
from settings import logger, FILE_PATH, PAGE_SIZE, VIDEO_PATH


clients: List[asyncio.Queue] = []


def _safe_filename(filename: str) -> str:
    if not filename:
        return ''
    name = os.path.basename(unquote(filename.strip()))
    return name.replace('\x00', '').strip()


def _stem_and_ext(filename: str) -> tuple:
    name = _safe_filename(filename)
    if '.' in name:
        stem, ext = name.rsplit('.', 1)
        return stem, ext.lower()
    return name, ''


async def read_preprocess_file(file_path, start_index=0):
    with open(file_path, 'rb') as f:
        f.seek(start_index)
        while True:
            chunk = f.read(65536)
            if not chunk:
                break
            yield chunk


async def broadcast_data(data: dict):
    for client in clients[:]:
        try:
            await client.put(json.dumps(data, ensure_ascii=False))
        except:
            logger.error(traceback.format_exc())


async def init_history():
    try:
        songs = await History.filter(is_sing=-1)
        for s in songs:
            s.is_sing = 1
            await s.save()
    except:
        logger.error(traceback.format_exc())


async def upload_file(query) -> Result:
    result = Result()
    query = await query.form()
    file_name = query['file'].filename
    data = query['file'].file
    try:
        file_path = os.path.join(FILE_PATH, file_name)
        song_name = parse_song_name(file_name) or file_name.replace('.mp4', '').replace('_vocals.mp3', '').replace('_accompaniment.mp3', '')
        try:
            file = await Files.get(name=song_name)
        except DoesNotExist:
            file = await Files.create(name=song_name, is_sing=0)
        with open(file_path, 'wb') as f:
            f.write(data.read())
        assets = get_song_assets(song_name)
        file.is_sing = sync_is_sing_flag(assets['has_video'])
        await file.save()
        result.msg = f"{file_name} 上传成功"
        result.data = file.name
        logger.info(result.msg)
    except:
        result.code = 1
        result.data = file_name
        result.msg = "系统错误"
        logger.error(f"{file_name} 上传失败")
        logger.error(traceback.format_exc())
    return result


async def get_list(q: str, page: int) -> Result:
    result = Result()
    try:
        if q:
            files = await Files.filter(name__contains=q).order_by('-id').offset((page - 1) * PAGE_SIZE).limit(PAGE_SIZE)
            total_num = await Files.filter(name__contains=q).count()
        else:
            files = await Files.all().order_by('-id').offset((page - 1) * PAGE_SIZE).limit(PAGE_SIZE)
            total_num = await Files.all().count()
        file_list = [FileList.from_orm_format(f, probe_embedded=True).dict() for f in files]
        result.data = file_list
        result.page = page
        result.total = len(result.data)
        result.totalPage = (total_num + PAGE_SIZE - 1) // PAGE_SIZE
        logger.info("查询歌曲列表成功 ~")
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def delete_song(file_id: int) -> Result:
    result = Result()
    try:
        file = await Files.get(id=file_id)
        if os.path.exists(f"{FILE_PATH}/{file.name}.mp4"):
            os.remove(f"{FILE_PATH}/{file.name}.mp4")
        if os.path.exists(f"{FILE_PATH}/{file.name}_vocals.mp3"):
            os.remove(f'{FILE_PATH}/{file.name}_vocals.mp3')
        if os.path.exists(f"{FILE_PATH}/{file.name}_accompaniment.mp3"):
            os.remove(f'{FILE_PATH}/{file.name}_accompaniment.mp3')
        try:
            history = await History.get(id=file_id)
            await history.delete()
        except DoesNotExist:
            pass
        await file.delete()
        result.msg = f"{file.name} 删除成功"
        logger.info(result.msg)
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def delete_history(file_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=file_id)
        await history.delete()
        result.msg = f"{history.name} 播放记录删除成功"
        await broadcast_data({"code": 8})
        logger.info(result.msg)
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def sing_song(file_id: int) -> Result:
    result = Result()
    try:
        file = await Files.get(id=file_id)
        if file.is_sing == 0:
            assets = get_song_assets(file.name)
            if not assets['has_video']:
                result.code = 1
                result.msg = '视频文件不存在'
                return result
            file.is_sing = sync_is_sing_flag(True)
            await file.save()
            _ = await History.create(id=file.id, name=file.name)
            await broadcast_data({"code": 8})
        else:
            try:
                history = await History.get(id=file.id)
                if history.is_sing == 1:
                    history.is_sing = 0
                    history.is_top = 0
                    await history.save()
            except DoesNotExist:
                _ = await History.create(id=file.id, name=file.name, is_sing=0, is_top=0)
            finally:
                await broadcast_data({"code": 8})
        result.msg = f"{file.name} 点歌成功"
        logger.info(result.msg)
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def history_list(query_type: str) -> Result:
    result = Result()
    try:
        if query_type == "history":
            songs = await History.filter(is_sing=1).order_by('-update_time').offset(0).limit(200)
            msg = "查询K歌历史列表成功"
        elif query_type == "usually":
            songs = await History.all().order_by('-times').offset(0).limit(200)
            msg = "查询经常K歌的歌曲列表成功"
        elif query_type == "pendingAll":
            songs = await History.filter(is_sing=-1)
            songs = songs + await History.filter(is_sing=0, is_top=1).order_by('-update_time')
            songs = songs + await History.filter(is_sing=0, is_top=0).order_by('update_time')
            msg = "查询已点列表的歌曲成功"
        else:
            songs = await History.filter(is_sing=-1)
            songs = songs + await History.filter(is_sing=0, is_top=1).order_by('-update_time')
            songs = songs + await History.filter(is_sing=0, is_top=0).order_by('update_time').offset(0).limit(4)
            msg = "查询已点列表最近的歌曲成功"
        song_list = []
        for f in songs:
            probe = not get_song_assets(f.name)['can_switch']
            song_list.append(HistoryList.from_history(f, probe_embedded=probe).dict())
        result.data = song_list
        result.total = len(result.data)
        logger.info(msg)
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def set_top(file_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=file_id)
        history.is_top = 1
        await history.save()
        result.msg = f"{history.name} 置顶成功"
        await broadcast_data({"code": 8})
        logger.info(result.msg)
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def set_singing(file_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=file_id)
        history.is_sing = -1
        history.is_top = 0
        await history.save()
        result.msg = f"{history.name} 设置-1成功"
        logger.info(result.msg)
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def set_singed(file_id: int) -> Result:
    result = Result()
    try:
        history = await History.get(id=file_id)
        history.is_sing = 1
        history.is_top = 0
        history.times = history.times + 1
        await history.save()
        result.msg = f"{history.name} 设置1成功"
        await broadcast_data({"code": 8})
        logger.info(result.msg)
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def upload_video(query) -> Result:
    result = Result()
    form = await query.form()
    upload = form['file']
    file_name = _safe_filename(form.get('filename') or upload.filename)
    data = upload.file
    try:
        if not file_name:
            result.code = 1
            result.msg = "无法获取文件名"
            return result
        stem, file_format = _stem_and_ext(file_name)
        if not file_format:
            file_format = 'mp4'
            stem = file_name
        file_path = os.path.join(VIDEO_PATH, f"{stem}_origin.{file_format}")
        with open(file_path, 'wb') as f:
            f.write(data.read())
        result.msg = f"{file_name} 上传成功"
        result.data = file_name
        logger.info(result.msg)
    except:
        result.code = 1
        result.data = file_name
        result.msg = "系统错误"
        logger.error(f"{file_name} 上传失败")
        logger.error(traceback.format_exc())
    return result


async def download_preprocess_file(file_name: str):
    from urllib.parse import quote
    from karaoke.responses import StreamResponse
    from settings import CONTENT_TYPE
    try:
        file_name = _safe_filename(file_name)
        if not file_name:
            return Result(code=1, msg="无效的文件名")
        file_path = os.path.join(VIDEO_PATH, file_name)
        if not os.path.isfile(file_path):
            return Result(code=1, msg="文件不存在")
        file_format = file_name.rsplit('.', 1)[-1].lower()
        headers = {
            'Content-Length': str(os.path.getsize(file_path)),
            'Content-Disposition': f"attachment; filename*=UTF-8''{quote(file_name)}",
        }
        return StreamResponse(
            read_preprocess_file(file_path),
            media_type=CONTENT_TYPE.get(file_format, 'application/octet-stream'),
            headers=headers,
        )
    except:
        logger.error(traceback.format_exc())
        return Result(code=1, msg="系统错误")


async def deal_video(file_name: str) -> Result:
    result = Result()
    try:
        stem, ext = _stem_and_ext(file_name)
        if not stem:
            result.code = 1
            result.msg = "无效的文件名"
            return result
        mp4_file = os.path.join(VIDEO_PATH, f"{stem}_origin.mp4")
        mp3_file = os.path.join(VIDEO_PATH, f"{stem}.wav")
        no_voice_file = os.path.join(VIDEO_PATH, f"{stem}_voice.mp4")
        video_file = os.path.join(VIDEO_PATH, f"{stem}.mp4")
        for path in (mp3_file, no_voice_file, video_file):
            remove_if_exists(path)
        run_ffmpeg(['-i', mp4_file, '-q:a', '0', '-map', 'a', mp3_file])
        run_ffmpeg(['-i', mp4_file, '-an', '-vcodec', 'copy', no_voice_file])
        run_ffmpeg(['-i', no_voice_file, '-map_metadata', '0', '-c:v', 'copy', '-c:a', 'copy',
                    '-movflags', '+faststart', video_file])
        result.data = {"mp3": f"{stem}.wav", "video": f"{stem}.mp4"}
        logger.info(f"{stem} 视频预处理完成")
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def convert_video(file_name: str) -> Result:
    result = Result()
    try:
        stem, file_format = _stem_and_ext(file_name)
        if not stem or not file_format:
            result.code = 1
            result.msg = "无效的文件名"
            return result
        audio_file = os.path.join(VIDEO_PATH, f"{stem}_origin.{file_format}")
        mp4_file = os.path.join(VIDEO_PATH, f"{stem}.mp4")
        remove_if_exists(mp4_file)
        run_ffmpeg(['-i', audio_file, '-c:v', 'libx264', '-c:a', 'aac', mp4_file])
        result.data = {"mp4": f"{stem}.mp4", "video": f"{stem}.{file_format}"}
        logger.info(f"{stem} 视频转 mp4 完成")
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def convert_audio(file_name: str) -> Result:
    result = Result()
    try:
        stem, file_format = _stem_and_ext(file_name)
        if not stem or not file_format:
            result.code = 1
            result.msg = "无效的文件名"
            return result
        audio_file = os.path.join(VIDEO_PATH, f"{stem}_origin.{file_format}")
        mp3_file = os.path.join(VIDEO_PATH, f"{stem}.mp3")
        remove_if_exists(mp3_file)
        run_ffmpeg(['-i', audio_file, '-codec:a', 'libmp3lame', mp3_file])
        result.data = {"mp3": f"{stem}.mp3", "audio": f"{stem}.{file_format}"}
        logger.info(f"{stem} 音频转 mp3 完成")
    except:
        logger.error(traceback.format_exc())
        result.code = 1
        result.msg = "系统错误"
    return result


async def import_local_file(file_path: str) -> Result:
    result = Result()
    try:
        data = scan_directory(local_path=file_path, dry_run=False)
        for item in data['items']:
            if item['action'] in ('import', 'partial_import'):
                await register_song_after_import(item['name'])
        result.data = data
        result.msg = f"扫描完成：导入 {data['imported']}，跳过 {data['skipped']}，分离 {data['separated']}"
    except:
        result.code = 1
        logger.error(traceback.format_exc())
    return result


async def import_scan(body: dict) -> Result:
    result = Result()
    try:
        dry_run = bool(body.get('dry_run', True))
        data = scan_directory(
            local_path=body.get('local_path'),
            dry_run=dry_run,
            auto_separate=body.get('auto_separate'),
            skip_incomplete=body.get('skip_incomplete'),
            duplicate_policy=body.get('duplicate_policy'),
        )
        if not dry_run:
            for item in data['items']:
                if item['action'] in ('import', 'partial_import'):
                    await register_song_after_import(item['name'])
        result.data = data
        result.msg = '预览完成' if dry_run else '导入完成'
    except:
        result.code = 1
        result.msg = "系统错误"
        logger.error(traceback.format_exc())
    return result


async def create_pipeline(query) -> Result:
    result = Result()
    try:
        form = await query.form()
        upload = form['file']
        filename = _safe_filename(form.get('filename') or upload.filename)
        stem, ext = _stem_and_ext(filename)
        song_name = form.get('song_name') or stem
        if not song_name:
            result.code = 1
            result.msg = '无效的歌曲名'
            return result
        from settings import get_config_section
        work_root = os.path.join(get_config_section('pipeline', 'work_path', '/work'), 'uploads')
        os.makedirs(work_root, exist_ok=True)
        upload_path = os.path.join(work_root, f'{song_name}_{ext or "bin"}')
        with open(upload_path, 'wb') as f:
            f.write(upload.file.read())
        job_id = submit_from_upload(upload_path, song_name)
        result.data = {'job_id': job_id, 'song_name': song_name}
        result.msg = '任务已提交'
    except:
        result.code = 1
        result.msg = "系统错误"
        logger.error(traceback.format_exc())
    return result


async def get_pipeline_status(job_id: str) -> Result:
    result = Result()
    job = get_job(job_id)
    if not job:
        result.code = 1
        result.msg = '任务不存在'
        return result
    result.data = {
        'job_id': job.job_id,
        'song_name': job.song_name,
        'status': job.status,
        'step': job.step,
        'error': job.error,
        'outputs': job.outputs,
    }
    return result


async def commit_pipeline(job_id: str) -> Result:
    result = Result()
    try:
        info = import_job_to_library(job_id)
        await register_song_after_import(info['name'])
        result.data = info
        result.msg = f'{info["name"]} 已导入曲库'
    except:
        result.code = 1
        result.msg = "系统错误"
        logger.error(traceback.format_exc())
    return result


async def enrich_song(file_id: int) -> Result:
    result = Result()
    try:
        file = await Files.get(id=file_id)
        job_id = submit_from_library(file.name)
        result.data = {'job_id': job_id, 'song_name': file.name}
        result.msg = '补全任务已提交'
    except:
        result.code = 1
        result.msg = "系统错误"
        logger.error(traceback.format_exc())
    return result


async def commit_enrich(job_id: str) -> Result:
    result = Result()
    try:
        info = import_job_to_library(job_id)
        await register_song_after_import(info['name'])
        result.data = info
        result.msg = f'{info["name"]} 音轨已补全'
    except:
        result.code = 1
        result.msg = "系统错误"
        logger.error(traceback.format_exc())
    return result


async def download_pipeline_file(job_id: str, file_name: str):
    from urllib.parse import quote
    from karaoke.responses import StreamResponse
    from settings import CONTENT_TYPE
    job = get_job(job_id)
    if not job:
        return Result(code=1, msg='任务不存在')
    file_name = _safe_filename(file_name)
    file_path = os.path.join(job.work_dir, file_name)
    if not os.path.isfile(file_path):
        return Result(code=1, msg='文件不存在')
    file_format = file_name.rsplit('.', 1)[-1].lower()
    headers = {
        'Content-Length': str(os.path.getsize(file_path)),
        'Content-Disposition': f"attachment; filename*=UTF-8''{quote(file_name)}",
    }
    return StreamResponse(
        read_preprocess_file(file_path),
        media_type=CONTENT_TYPE.get(file_format, 'application/octet-stream'),
        headers=headers,
    )
