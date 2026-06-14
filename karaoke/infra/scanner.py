#!/usr/bin/env python
# -*- coding: utf-8 -*-

import asyncio
import os
import time
from dataclasses import dataclass, field
from typing import Dict, Iterable, List, Optional, Set, Tuple

from karaoke.infra.media import probe_video_playable
from karaoke.infra.models import Song
from settings import (
    DEFAULT_DUPLICATE_POLICY,
    FFPROBE_ON_IMPORT,
    KEEP_DIR_NAME,
    OVERRIDE_DIR_NAME,
    SCAN_VIDEO_EXTS,
    logger,
)

_VIDEO_EXTS = frozenset(SCAN_VIDEO_EXTS)
_SKIP_DIR_NAMES = frozenset({KEEP_DIR_NAME, OVERRIDE_DIR_NAME})
_BULK_BATCH = 500
_PREVIEW_LIMIT = 200
_UPDATE_FIELDS = (
    'display_name', 'source_path', 'source_rel', 'source_origin',
    'is_playable', 'scan_root', 'media_kind', 'playback_mode',
    'playback_source', 'can_queue',
)


@dataclass
class ScanStats:
    added: int = 0
    skipped: int = 0
    renamed: int = 0
    invalid: int = 0
    preview: List[dict] = field(default_factory=list)

    def as_dict(self) -> dict:
        return {
            'added': self.added,
            'skipped': self.skipped,
            'renamed': self.renamed,
            'invalid': self.invalid,
        }


def _display_base(filename: str) -> str:
    return os.path.splitext(filename)[0]


def _make_unique_display_name(base: str, source_rel: str, used: Set[str]) -> tuple:
    candidate = base
    if candidate not in used:
        return candidate, False
    rel_suffix = source_rel.replace(os.sep, '/').strip('./')
    if rel_suffix and rel_suffix != '.':
        candidate = f"{base} ({rel_suffix})"
        if candidate not in used:
            return candidate, True
    index = 2
    while True:
        candidate = f"{base} ({index})"
        if candidate not in used:
            return candidate, True
        index += 1


def _append_preview(stats: ScanStats, item: dict) -> None:
    if len(stats.preview) < _PREVIEW_LIMIT:
        stats.preview.append(item)


def _iter_video_files(root: str) -> Iterable[Tuple[str, str]]:
    """用 os.scandir 深度遍历，仅 yield (绝对路径, 所在目录)。"""
    stack = [root]
    while stack:
        dirpath = stack.pop()
        try:
            with os.scandir(dirpath) as entries:
                subdirs: List[str] = []
                for entry in entries:
                    try:
                        if entry.is_dir(follow_symlinks=False):
                            if entry.name not in _SKIP_DIR_NAMES:
                                subdirs.append(entry.path)
                            continue
                        if not entry.is_file(follow_symlinks=False):
                            continue
                        name = entry.name
                        dot = name.rfind('.')
                        if dot <= 0:
                            continue
                        if name[dot + 1:].lower() in _VIDEO_EXTS:
                            yield entry.path, dirpath
                    except OSError:
                        continue
                stack.extend(reversed(subdirs))
        except OSError:
            continue


def _collect_scan(
    root: str,
    policy: str,
    do_probe: bool,
    dry_run: bool,
    existing_by_path: Dict[str, Song],
    existing_by_name: Dict[str, Song],
    used_names: Set[str],
) -> Tuple[ScanStats, List[Song], List[Song]]:
    stats = ScanStats()
    to_create: List[Song] = []
    to_update: Dict[int, Song] = {}
    t0 = time.monotonic()
    file_count = 0

    for abs_path, dirpath in _iter_video_files(root):
        file_count += 1
        filename = os.path.basename(abs_path)
        base_name = _display_base(filename)
        source_rel = os.path.relpath(dirpath, root)
        if source_rel == '.':
            source_rel = ''

        is_playable = True
        if do_probe:
            is_playable = probe_video_playable(abs_path)
            if not is_playable:
                stats.invalid += 1
                if dry_run:
                    _append_preview(stats, {'path': abs_path, 'action': 'invalid'})
                continue

        if abs_path in existing_by_path:
            if policy == 'overwrite':
                song = existing_by_path[abs_path]
                song.is_playable = is_playable
                song.scan_root = root
                song.source_rel = source_rel or None
                song.playback_mode = 'plain'
                song.playback_source = 'plain'
                song.can_queue = is_playable
                to_update[song.id] = song
                if dry_run:
                    _append_preview(stats, {'path': abs_path, 'action': 'update'})
                else:
                    stats.added += 1
            else:
                stats.skipped += 1
                if dry_run:
                    _append_preview(stats, {'path': abs_path, 'action': 'skip'})
            continue

        display_name = base_name
        renamed = False
        existing_same_name = existing_by_name.get(display_name)
        if existing_same_name and existing_same_name.source_path != abs_path:
            if policy == 'skip':
                stats.skipped += 1
                if dry_run:
                    _append_preview(stats, {'path': abs_path, 'action': 'skip'})
                continue
            if policy == 'overwrite':
                if not dry_run:
                    existing_same_name.source_path = abs_path
                    existing_same_name.source_rel = source_rel or None
                    existing_same_name.is_playable = is_playable
                    existing_same_name.scan_root = root
                    existing_same_name.source_origin = 'scan'
                    existing_same_name.playback_mode = 'plain'
                    existing_same_name.playback_source = 'plain'
                    existing_same_name.can_queue = is_playable
                    to_update[existing_same_name.id] = existing_same_name
                    existing_by_path[abs_path] = existing_same_name
                stats.added += 1
                if dry_run:
                    _append_preview(stats, {'path': abs_path, 'action': 'overwrite'})
                continue
            display_name, renamed = _make_unique_display_name(base_name, source_rel, used_names)
            if renamed:
                stats.renamed += 1

        used_names.add(display_name)
        if dry_run:
            _append_preview(stats, {
                'path': abs_path,
                'action': 'rename' if renamed else 'add',
                'display_name': display_name,
            })
            stats.added += 1
            continue

        song = Song(
            display_name=display_name,
            source_path=abs_path,
            source_origin='scan',
            source_rel=source_rel or None,
            media_kind='video',
            playback_mode='plain',
            playback_source='plain',
            can_queue=is_playable,
            is_playable=is_playable,
            scan_root=root,
        )
        to_create.append(song)
        existing_by_path[abs_path] = song
        existing_by_name[display_name] = song
        stats.added += 1

    elapsed = time.monotonic() - t0
    logger.info(
        'scan collect: files=%s added=%s skipped=%s invalid=%s probe=%s elapsed=%.2fs',
        file_count, stats.added, stats.skipped, stats.invalid, do_probe, elapsed,
    )
    return stats, to_create, list(to_update.values())


async def scan_root(
    root: str,
    duplicate_policy: Optional[str] = None,
    validate: Optional[bool] = None,
    dry_run: bool = False,
) -> ScanStats:
    root = os.path.abspath(root)
    if not os.path.isdir(root):
        raise FileNotFoundError(root)

    policy = duplicate_policy or DEFAULT_DUPLICATE_POLICY
    do_probe = FFPROBE_ON_IMPORT if validate is None else validate

    songs = await Song.all().only(
        'id', 'source_path', 'display_name', 'source_rel',
        'source_origin', 'is_playable', 'scan_root', 'media_kind',
        'playback_mode', 'audio_layout',
    )
    existing_by_path = {s.source_path: s for s in songs}
    existing_by_name = {s.display_name: s for s in songs}
    used_names = set(existing_by_name.keys())

    stats, to_create, to_update = await asyncio.to_thread(
        _collect_scan,
        root,
        policy,
        do_probe,
        dry_run,
        existing_by_path,
        existing_by_name,
        used_names,
    )

    if not dry_run:
        t0 = time.monotonic()
        if to_create:
            await Song.bulk_create(to_create, batch_size=_BULK_BATCH)
        if to_update:
            await Song.bulk_update(to_update, fields=_UPDATE_FIELDS, batch_size=_BULK_BATCH)
        logger.info(
            'scan persist: create=%s update=%s elapsed=%.2fs',
            len(to_create), len(to_update), time.monotonic() - t0,
        )

    return stats
