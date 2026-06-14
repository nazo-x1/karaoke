#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
from typing import Optional, Tuple
from urllib.parse import quote

from fastapi import Request
from fastapi.responses import JSONResponse

from karaoke.media import file_ext
from karaoke.responses import StreamResponse
from settings import CONTENT_TYPE


async def read_stream_file(file_path: str, start_index: int = 0, end_index: Optional[int] = None):
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


def build_stream_response(
    request: Request,
    file_path: str,
    media_type: Optional[str] = None,
):
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


def cache_not_ready_response(prep: dict) -> JSONResponse:
    return JSONResponse(
        status_code=503,
        content={
            'code': 1,
            'msg': '内嵌缓存未就绪，请等待后台生成完成',
            'data': prep,
        },
    )
