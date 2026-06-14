#!/usr/bin/env python
# -*- coding: utf-8 -*-

import fnmatch
import os
import shutil
from dataclasses import dataclass, field
from typing import List, Optional

from karaoke.assets import get_song_assets, parse_song_name, song_paths, sync_is_sing_flag
from karaoke.pipeline import submit_from_library
from settings import FILE_PATH, get_config_section, logger


@dataclass
class ImportItem:
    name: str
    action: str
    reason: str
    files_found: List[str] = field(default_factory=list)
    job_id: Optional[str] = None


def _import_config(key: str, fallback: str) -> str:
    return get_config_section('import', key, fallback)


def _bool_config(key: str, fallback: bool) -> bool:
    val = _import_config(key, str(fallback)).lower()
    return val in ('1', 'true', 'yes', 'on')


def _glob_match(name: str, pattern: str) -> bool:
    return fnmatch.fnmatch(name, pattern.strip())


def _should_include(filename: str, include_glob: str, exclude_globs: List[str]) -> bool:
    if not _glob_match(filename, include_glob):
        return False
    for pat in exclude_globs:
        if pat and _glob_match(filename, pat):
            return False
    return True


def _library_has_conflict(name: str, duplicate_policy: str) -> bool:
    paths = song_paths(name)
    exists = any(os.path.isfile(p) for p in paths.values())
    if not exists:
        return False
    return duplicate_policy == 'skip'


def scan_directory(
    local_path: Optional[str] = None,
    dry_run: bool = True,
    auto_separate: Optional[bool] = None,
    skip_incomplete: Optional[bool] = None,
    duplicate_policy: Optional[str] = None,
) -> dict:
    scan_path = local_path or _import_config('scan_path', '/import')
    recursive = _bool_config('recursive', True)
    auto_sep = auto_separate if auto_separate is not None else _bool_config('auto_separate', False)
    skip_incomp = skip_incomplete if skip_incomplete is not None else _bool_config('skip_incomplete', False)
    dup_policy = duplicate_policy or _import_config('duplicate_policy', 'skip')
    include_glob = _import_config('include_glob', '*')
    exclude_globs = [p.strip() for p in _import_config('exclude_glob', '*_origin.*,*_voice.*,*.tmp').split(',')]
    min_size = int(_import_config('min_size_bytes', '1024'))
    skip_hidden = _bool_config('skip_hidden', True)

    if not os.path.isdir(scan_path):
        raise FileNotFoundError(scan_path)

    groups: dict[str, set[str]] = {}
    walker = os.walk(scan_path) if recursive else [(scan_path, [], os.listdir(scan_path))]

    for root, _, files in walker:
        for filename in files:
            if skip_hidden and filename.startswith('.'):
                continue
            full = os.path.join(root, filename)
            if os.path.getsize(full) < min_size:
                continue
            if not _should_include(filename, include_glob, exclude_globs):
                continue
            name = parse_song_name(filename)
            if not name:
                continue
            groups.setdefault(name, set()).add(full)

    items: List[ImportItem] = []
    imported = 0
    skipped = 0
    separated = 0

    for name, paths in sorted(groups.items()):
        files_found = []
        has_mp4 = any(p.endswith('.mp4') for p in paths)
        has_vocals = any(p.endswith('_vocals.mp3') for p in paths)
        has_acc = any(p.endswith('_accompaniment.mp3') for p in paths)
        for p in paths:
            files_found.append(os.path.basename(p))

        complete = has_mp4 and has_vocals and has_acc
        only_mp4 = has_mp4 and not has_vocals and not has_acc

        if _library_has_conflict(name, dup_policy):
            items.append(ImportItem(name, 'skip', 'duplicate_skip', files_found))
            skipped += 1
            continue

        if complete:
            action = 'import'
            reason = 'ready'
        elif only_mp4 and auto_sep:
            action = 'separate'
            reason = 'auto_separate'
        elif skip_incomp and not complete:
            items.append(ImportItem(name, 'skip', 'incomplete', files_found))
            skipped += 1
            continue
        else:
            action = 'partial_import'
            reason = 'partial'

        item = ImportItem(name, action, reason, files_found)
        items.append(item)

        if dry_run:
            continue

        if action == 'separate':
            for p in paths:
                if p.endswith('.mp4'):
                    dst = os.path.join(FILE_PATH, f'{name}.mp4')
                    shutil.copy2(p, dst)
            try:
                job_id = submit_from_library(name)
                item.job_id = job_id
                separated += 1
            except Exception as e:
                item.action = 'skip'
                item.reason = f'separate_failed:{e}'
                skipped += 1
            continue

        for p in paths:
            basename = os.path.basename(p)
            shutil.copy2(p, os.path.join(FILE_PATH, basename))
        imported += 1

    return {
        'items': [i.__dict__ for i in items],
        'imported': imported,
        'skipped': skipped,
        'separated': separated,
        'dry_run': dry_run,
        'scan_path': scan_path,
    }


async def register_song_after_import(name: str):
    from tortoise.exceptions import DoesNotExist
    from karaoke.models import Files

    assets = get_song_assets(name)
    is_sing = sync_is_sing_flag(assets['has_video'])
    try:
        f = await Files.get(name=name)
        f.is_sing = is_sing
        await f.save()
    except DoesNotExist:
        await Files.create(name=name, is_sing=is_sing)
