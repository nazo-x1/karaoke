#!/usr/bin/env python
# -*- coding: utf-8 -*-
# @Author: leeyoshinari

import json
import asyncio
import traceback
from typing import Optional

from fastapi import APIRouter, Request, Body
from sse_starlette import EventSourceResponse
import karaoke.views as views
from karaoke.results import Result
from settings import logger


router = APIRouter(prefix='/song', tags=['歌曲'], responses={404: {'description': 'Not found'}})


@router.on_event('startup')
async def startup_event():
    await views.init_history()


@router.post('/upload', summary="上传歌曲")
async def upload_file(query: Request):
    return await views.upload_file(query)


@router.post('/scan', summary="扫描导入")
async def scan_songs(body: dict = Body(...)):
    return await views.run_scan(body)


@router.get('/scan/preview', summary="扫描预览")
async def scan_preview(
    root: str,
    duplicate_policy: Optional[str] = None,
    validate: Optional[bool] = None,
):
    return await views.preview_scan(root, duplicate_policy, validate)


@router.get("/list", summary="歌曲列表")
async def song_list(q: str = "", page: int = 1):
    return await views.get_list(q, page)


@router.get("/delete/{file_id}", summary="删除歌曲")
async def delete_song(file_id: int, delete_disk: bool = False):
    return await views.delete_song(file_id, delete_disk=delete_disk)


@router.get("/deleteHistory/{file_id}", summary="删除点歌历史记录")
async def delete_history(file_id: int):
    return await views.delete_history(file_id)


@router.get("/sing/{file_id}", summary="点歌")
async def song_sing(file_id: int):
    return await views.sing_song(file_id)


@router.get("/singHistory/{query_type}", summary="点歌历史纪录列表")
async def history_list(query_type: str):
    return await views.history_list(query_type)


@router.get("/setTop/{file_id}", summary="置顶")
async def set_top(file_id: int):
    return await views.set_top(file_id)


@router.get("/setSinged/{file_id}", summary="设置已经播放过")
async def set_singed(file_id: int):
    return await views.set_singed(file_id)


@router.get("/setSinging/{file_id}", summary="设置正在播放")
async def set_dinging(file_id: int):
    return await views.set_singing(file_id)


@router.get("/stream/{song_id}/{kind}", summary="流媒体")
async def stream_song(song_id: int, kind: str, request: Request):
    return await views.stream_song(request, song_id, kind)


@router.get("/events", summary="SSE")
async def get_events(request: Request):
    client_queue = asyncio.Queue()
    views.clients.append(client_queue)

    async def event_generator():
        try:
            while True:
                if await request.is_disconnected():
                    break
                message = await client_queue.get()
                yield message
        except Exception:
            logger.error(traceback.format_exc())
        finally:
            views.clients.remove(client_queue)

    return EventSourceResponse(event_generator())


@router.get("/send/event", summary="发送数据")
async def send_event(code: int, data, request: Request):
    data = json.dumps({'code': code, 'data': data})
    for client in views.clients:
        await client.put(data)
    return Result()


@router.get("/{song_id}/playback", summary="播放配置")
async def song_playback(song_id: int):
    return await views.get_playback_profile(song_id)


@router.post("/{song_id}/detect-override", summary="检测覆写文件")
async def detect_override(song_id: int):
    return await views.detect_override(song_id)


@router.post("/{song_id}/detect-playback", summary="检测播放能力")
async def detect_playback(song_id: int):
    return await views.detect_playback(song_id)


@router.post("/{song_id}/prepare-embedded", summary="预生成内嵌缓存")
async def prepare_embedded(song_id: int):
    return await views.prepare_embedded(song_id)


@router.get("/{song_id}", summary="歌曲详情")
async def song_detail(song_id: int):
    return await views.get_song_detail(song_id)


@router.patch("/{song_id}", summary="更新歌曲")
async def update_song(song_id: int, body: dict = Body(...)):
    return await views.patch_song(song_id, body)
