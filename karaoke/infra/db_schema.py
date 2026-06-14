#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""SQLite 增量 schema 迁移（ALTER ADD COLUMN），不清库。"""

import os
import sqlite3

from settings import FILE_PATH, logger

DATABASE_FILE = os.path.join(FILE_PATH, 'sqlite3.db')

# song 表缺失时追加的列：(列名, SQL 类型)
SONG_ADDITIVE_COLUMNS = (
    ('source_rel', 'VARCHAR(512)'),
    ('media_kind', 'VARCHAR(16)'),
    ('playback_mode', 'VARCHAR(16)'),
    ('playback_source', 'VARCHAR(16)'),
    ('can_queue', 'INT'),
    ('is_playable', 'INT'),
    ('scan_root', 'VARCHAR(1024)'),
    ('audio_layout', 'TEXT'),
)


def _song_columns(conn: sqlite3.Connection) -> set:
    return {row[1] for row in conn.execute('PRAGMA table_info(song)')}


def needs_fatal_reset(conn: sqlite3.Connection) -> bool:
    tables = {row[0] for row in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table'"
    )}
    if 'files' in tables:
        return True
    if 'song' not in tables:
        return True
    return False


def ensure_schema(db_path: str = DATABASE_FILE) -> None:
    if not os.path.isfile(db_path):
        return

    conn = sqlite3.connect(db_path)
    try:
        if needs_fatal_reset(conn):
            logger.warning('database schema incompatible (files table or missing song), skip additive migrate')
            return

        cols = _song_columns(conn)
        added = []
        for name, sql_type in SONG_ADDITIVE_COLUMNS:
            if name not in cols:
                conn.execute(f'ALTER TABLE song ADD COLUMN {name} {sql_type}')
                added.append(name)

        if added:
            conn.commit()
            logger.info('schema migrated: added columns %s', ', '.join(added))
    finally:
        conn.close()
