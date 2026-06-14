#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
from typing import Optional
from urllib.parse import unquote

from fastapi import Request

from karaoke.domain.playback import refresh_playback_mode
from karaoke.dto.api_result import ApiResult
from karaoke.dto.mappers import song_item
from karaoke.errors import format_api_error
from karaoke.infra.embedded import probe_and_save_layout
from karaoke.infra.media import file_ext, probe_video_playable
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.infra.scanner import scan_root
from karaoke.services.base import apply_pagination, run_guarded, run_scan_guarded
from karaoke.services.queue_service import QueueService
from settings import (
    logger,
    PAGE_SIZE,
    KEEP_PATH,
    SCAN_VIDEO_EXTS,
    DEFAULT_DUPLICATE_POLICY,
    FFPROBE_ON_IMPORT,
)


def _safe_filename(filename: str) -> str:
    if not filename:
        return ''
    return os.path.basename(unquote(filename.strip())).replace('\x00', '').strip()


def _stem(filename: str) -> str:
    return os.path.splitext(_safe_filename(filename))[0]


class LibraryService:
    def __init__(
        self,
        songs: Optional[SongRepository] = None,
        queue: Optional[QueueService] = None,
    ) -> None:
        self._songs = songs or SongRepository()
        self._queue = queue or QueueService()

    async def upload_file(self, query: Request) -> ApiResult:
        form = await query.form()
        upload = form.get('file')
        if not upload or not upload.filename:
            return ApiResult.fail('未选择文件')

        filename = _safe_filename(upload.filename)
        ext = file_ext(filename)
        if ext not in SCAN_VIDEO_EXTS:
            return ApiResult.fail(f'不支持的格式: {ext}', data=filename)

        duplicate_policy = form.get('duplicate_policy') or DEFAULT_DUPLICATE_POLICY
        display_base = _stem(filename)
        dest_path = os.path.join(KEEP_PATH, filename)

        try:
            file_bytes = upload.file.read()
            return await self._save_upload(
                filename, display_base, dest_path, duplicate_policy, file_bytes,
            )
        except Exception as exc:
            return ApiResult.fail(
                format_api_error(exc, f'{filename} 上传失败'),
                data=filename,
            )

    async def _save_upload(
        self,
        filename: str,
        display_base: str,
        dest_path: str,
        duplicate_policy: str,
        file_bytes: bytes,
    ) -> ApiResult:
        existing = await self._songs.find_by_source_path(dest_path)
        name_conflict = await self._songs.find_by_display_name(display_base)

        if existing and duplicate_policy == 'skip':
            return ApiResult(msg=f'{filename} 已存在，已跳过', data=display_base)

        display_name = display_base
        if name_conflict and name_conflict.source_path != dest_path:
            if duplicate_policy == 'skip':
                return ApiResult(msg=f'{display_name} 已存在，已跳过', data=display_base)
            if duplicate_policy == 'rename':
                used = await self._songs.all_display_names()
                index = 2
                while display_name in used:
                    display_name = f'{display_base} ({index})'
                    index += 1
                _, ext_part = os.path.splitext(filename)
                dest_path = os.path.join(KEEP_PATH, f'{display_name}{ext_part}')

        with open(dest_path, 'wb') as f:
            f.write(file_bytes)

        is_playable = probe_video_playable(dest_path) if FFPROBE_ON_IMPORT else True

        if existing:
            existing.display_name = display_name
            existing.is_playable = is_playable
            existing.source_origin = 'upload'
            await self._songs.save(existing)
            song = existing
        elif name_conflict and duplicate_policy == 'overwrite' and name_conflict.source_path != dest_path:
            if os.path.isfile(name_conflict.source_path) and name_conflict.source_origin == 'upload':
                os.remove(name_conflict.source_path)
            name_conflict.source_path = dest_path
            name_conflict.display_name = display_name
            name_conflict.is_playable = is_playable
            name_conflict.source_origin = 'upload'
            name_conflict.source_rel = None
            await self._songs.save(name_conflict)
            song = name_conflict
        else:
            song = await self._songs.create(
                display_name=display_name,
                source_path=dest_path,
                source_origin='upload',
                source_rel=None,
                media_kind='video',
                playback_mode='plain',
                playback_source='plain',
                can_queue=is_playable,
                is_playable=is_playable,
            )

        if FFPROBE_ON_IMPORT:
            await probe_and_save_layout(song)
        await refresh_playback_mode(song)
        logger.info('%s 上传成功', filename)
        return ApiResult(msg=f'{filename} 上传成功', data=song.display_name)

    async def get_list(self, q: str, page: int) -> ApiResult:
        return await run_guarded('获取曲库列表失败', lambda: self._load_list(q, page))

    async def _load_list(self, q: str, page: int) -> ApiResult:
        songs, total_num = await self._songs.list_page(q, page)
        result = ApiResult(data=[song_item(s) for s in songs])
        return apply_pagination(result, total_num, page, PAGE_SIZE)

    async def delete_song(self, song_id: int, delete_disk: bool = False) -> ApiResult:
        async def action():
            song = await self._songs.get(song_id)
            if delete_disk and song.source_origin == 'upload' and os.path.isfile(song.source_path):
                os.remove(song.source_path)
            await self._queue.remove_if_exists(song_id)
            name = song.display_name
            await self._songs.delete(song)
            return ApiResult(msg=f'{name} 删除成功')

        return await run_guarded('删除歌曲失败', action, not_found_msg='歌曲不存在')

    async def run_scan(self, body: dict) -> ApiResult:
        root = (body.get('root') or '').strip()
        if not root:
            return ApiResult.fail('请指定扫描根路径')

        async def action():
            stats = await scan_root(
                root,
                duplicate_policy=body.get('duplicate_policy'),
                validate=body.get('validate'),
                dry_run=False,
            )
            return ApiResult(data=stats.as_dict(), msg='扫描完成')

        return await run_scan_guarded('扫描导入失败', action)

    async def preview_scan(
        self,
        root: str,
        duplicate_policy: Optional[str],
        validate: Optional[bool],
    ) -> ApiResult:
        root = (root or '').strip()
        if not root:
            return ApiResult.fail('请指定扫描根路径')

        async def action():
            stats = await scan_root(
                root,
                duplicate_policy=duplicate_policy,
                validate=validate,
                dry_run=True,
            )
            return ApiResult(
                data={**stats.as_dict(), 'preview': stats.preview},
                msg='预览完成',
            )

        return await run_scan_guarded('扫描预览失败', action)
