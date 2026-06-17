#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
import shutil
from typing import Set, Tuple

from karaoke.infra.audio_layout import has_dual_roles, parse_audio_layout
from karaoke.infra.embedded import cache_dir_for
from karaoke.infra.media import browser_mp4_cache_path
from karaoke.infra.repositories.history_repo import HistoryRepository
from karaoke.infra.repositories.song_repo import SongRepository
from karaoke.dto.api_result import ApiResult
from karaoke.services.base import run_guarded
from karaoke.services.prepare_service import PrepareService
from settings import PLAY_CACHE_PATH, logger


class CacheService:
    def __init__(
        self,
        prepare: PrepareService = None,
        songs: SongRepository = None,
        histories: HistoryRepository = None,
    ) -> None:
        self._prepare = prepare or PrepareService()
        self._songs = songs or SongRepository()
        self._histories = histories or HistoryRepository()

    async def clear_play_cache(self) -> ApiResult:
        async def action():
            protected_ids = await self._protected_song_ids()
            protected_dirs, protected_files = await self._protected_cache_paths(protected_ids)
            removed, skipped = self._clear_directory_except(
                PLAY_CACHE_PATH, protected_dirs, protected_files,
            )
            os.makedirs(PLAY_CACHE_PATH, exist_ok=True)
            msg = f'已清除播放转码缓存（{removed} 项'
            if skipped:
                msg += f'，跳过队列中 {skipped} 项'
            msg += '）'
            return ApiResult(
                msg=msg,
                data={
                    'removed': removed,
                    'skipped': skipped,
                    'protected_song_ids': sorted(protected_ids),
                    'path': PLAY_CACHE_PATH,
                },
            )

        return await run_guarded('清除播放缓存失败', action)

    async def _protected_song_ids(self) -> Set[int]:
        ids = set(self._prepare.active_tasks().keys())
        for history in await self._histories.list_pending():
            ids.add(history.id)
        return ids

    async def _protected_cache_paths(self, song_ids: Set[int]) -> Tuple[Set[str], Set[str]]:
        dirs: Set[str] = set()
        files: Set[str] = set()
        if not song_ids:
            return dirs, files

        song_map = await self._songs.map_by_ids(list(song_ids))
        for song in song_map.values():
            layout = parse_audio_layout(song.audio_layout)
            if layout and has_dual_roles(layout) and os.path.isfile(song.source_path):
                try:
                    dirs.add(os.path.abspath(cache_dir_for(song, layout)))
                except OSError as exc:
                    logger.warning('skip protect embedded cache song=%s: %s', song.id, exc)

            if song.source_path and os.path.isfile(song.source_path):
                try:
                    files.add(os.path.abspath(browser_mp4_cache_path(song.source_path)))
                except OSError as exc:
                    logger.warning('skip protect plain cache song=%s: %s', song.id, exc)

        return dirs, files

    @staticmethod
    def _clear_directory_except(
        root: str,
        protected_dirs: Set[str],
        protected_files: Set[str],
    ) -> Tuple[int, int]:
        if not os.path.isdir(root):
            return 0, 0

        removed = 0
        skipped = 0

        for name in os.listdir(root):
            path = os.path.join(root, name)
            abspath = os.path.abspath(path)

            if name == 'embedded' and os.path.isdir(path):
                for sub_name in os.listdir(path):
                    sub_path = os.path.join(path, sub_name)
                    sub_abs = os.path.abspath(sub_path)
                    if sub_abs in protected_dirs:
                        skipped += 1
                        continue
                    try:
                        if os.path.isdir(sub_path):
                            shutil.rmtree(sub_path)
                        else:
                            os.remove(sub_path)
                        removed += 1
                    except OSError as exc:
                        logger.warning('remove cache failed %s: %s', sub_path, exc)
                continue

            if abspath in protected_files:
                skipped += 1
                continue

            if os.path.isdir(path):
                try:
                    shutil.rmtree(path)
                    removed += 1
                except OSError as exc:
                    logger.warning('remove cache failed %s: %s', path, exc)
                continue

            try:
                os.remove(path)
                removed += 1
            except OSError as exc:
                logger.warning('remove cache failed %s: %s', path, exc)

        return removed, skipped
