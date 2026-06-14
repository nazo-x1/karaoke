#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set

from karaoke.media import file_ext, probe_video_playable
from karaoke.models import Song
from settings import (
    DEFAULT_DUPLICATE_POLICY,
    FFPROBE_ON_IMPORT,
    KEEP_DIR_NAME,
    OVERRIDE_DIR_NAME,
    SCAN_VIDEO_EXTS,
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


def _should_skip_dir(parts: tuple) -> bool:
    skip_names = {KEEP_DIR_NAME, OVERRIDE_DIR_NAME}
    return any(part in skip_names for part in parts)


def _display_base(filename: str) -> str:
    return os.path.splitext(os.path.basename(filename))[0]


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
    stats = ScanStats()

    existing_by_path: Dict[str, Song] = {}
    existing_by_name: Dict[str, Song] = {}
    for song in await Song.all():
        existing_by_path[song.source_path] = song
        existing_by_name[song.display_name] = song

    used_names = set(existing_by_name.keys())
    to_create: List[Song] = []
    to_update: List[Song] = []

    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = os.path.relpath(dirpath, root)
        parts = () if rel_dir == '.' else tuple(rel_dir.split(os.sep))
        if _should_skip_dir(parts):
            dirnames.clear()
            continue

        dirnames[:] = [d for d in dirnames if d not in (KEEP_DIR_NAME, OVERRIDE_DIR_NAME)]

        for filename in filenames:
            abs_path = os.path.join(dirpath, filename)
            ext = file_ext(filename)
            if ext not in SCAN_VIDEO_EXTS:
                continue

            is_playable = True
            if do_probe:
                is_playable = probe_video_playable(abs_path)
                if not is_playable:
                    stats.invalid += 1
                    if dry_run:
                        stats.preview.append({'path': abs_path, 'action': 'invalid'})
                    continue

            base_name = _display_base(filename)
            source_rel = os.path.relpath(dirpath, root)
            if source_rel == '.':
                source_rel = ''

            if abs_path in existing_by_path:
                if policy == 'overwrite':
                    song = existing_by_path[abs_path]
                    song.is_playable = is_playable
                    song.scan_root = root
                    song.source_rel = source_rel or None
                    to_update.append(song)
                    if dry_run:
                        stats.preview.append({'path': abs_path, 'action': 'update'})
                    else:
                        stats.added += 1
                else:
                    stats.skipped += 1
                    if dry_run:
                        stats.preview.append({'path': abs_path, 'action': 'skip'})
                continue

            display_name = base_name
            renamed = False
            existing_same_name = existing_by_name.get(display_name)
            if existing_same_name and existing_same_name.source_path != abs_path:
                if policy == 'skip':
                    stats.skipped += 1
                    if dry_run:
                        stats.preview.append({'path': abs_path, 'action': 'skip'})
                    continue
                if policy == 'overwrite':
                    if not dry_run:
                        existing_same_name.source_path = abs_path
                        existing_same_name.source_rel = source_rel or None
                        existing_same_name.is_playable = is_playable
                        existing_same_name.scan_root = root
                        existing_same_name.source_origin = 'scan'
                        to_update.append(existing_same_name)
                        existing_by_path[abs_path] = existing_same_name
                    stats.added += 1
                    if dry_run:
                        stats.preview.append({'path': abs_path, 'action': 'overwrite'})
                    continue
                display_name, renamed = _make_unique_display_name(base_name, source_rel, used_names)
                if renamed:
                    stats.renamed += 1

            used_names.add(display_name)
            if dry_run:
                stats.preview.append({
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
                is_playable=is_playable,
                scan_root=root,
            )
            to_create.append(song)
            existing_by_path[abs_path] = song
            existing_by_name[display_name] = song
            stats.added += 1

    if not dry_run:
        if to_create:
            await Song.bulk_create(to_create)
        for song in to_update:
            await song.save()

    return stats
