#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""数据库增量 schema 迁移（ALTER ADD COLUMN），不清库。"""

from __future__ import annotations

import os
import sqlite3
from typing import Iterable

from settings import DATABASE_URL, FILE_PATH, is_postgres_url, logger, postgres_dsn

DATABASE_FILE = os.path.join(FILE_PATH, 'sqlite3.db')

# song 表缺失时追加的列：(列名, SQL 类型)
SONG_ADDITIVE_COLUMNS: tuple[tuple[str, str], ...] = (
    ('source_rel', 'VARCHAR(512)'),
    ('media_kind', 'VARCHAR(16)'),
    ('playback_mode', 'VARCHAR(16)'),
    ('playback_source', 'VARCHAR(16)'),
    ('can_queue', 'INT'),
    ('is_playable', 'INT'),
    ('scan_root', 'VARCHAR(1024)'),
    ('audio_layout', 'TEXT'),
)

POSTGRES_SONG_ADDITIVE_COLUMNS: tuple[tuple[str, str], ...] = (
    ('source_rel', 'VARCHAR(512)'),
    ('media_kind', 'VARCHAR(16)'),
    ('playback_mode', 'VARCHAR(16)'),
    ('playback_source', 'VARCHAR(16)'),
    ('can_queue', 'BOOLEAN'),
    ('is_playable', 'BOOLEAN'),
    ('scan_root', 'VARCHAR(1024)'),
    ('audio_layout', 'TEXT'),
)


def _apply_additive_columns(
    existing: set[str],
    columns: Iterable[tuple[str, str]],
    alter_sql: str,
    execute,
    commit,
) -> list[str]:
    added: list[str] = []
    for name, sql_type in columns:
        if name not in existing:
            execute(alter_sql.format(name=name, sql_type=sql_type))
            added.append(name)
    if added:
        commit()
        logger.info('schema migrated: added columns %s', ', '.join(added))
    return added


def needs_fatal_reset_sqlite(conn: sqlite3.Connection) -> bool:
    tables = {row[0] for row in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table'"
    )}
    if 'files' in tables:
        return True
    if 'song' not in tables:
        return True
    return False


def needs_fatal_reset_postgres(conn) -> bool:
    with conn.cursor() as cur:
        cur.execute(
            "SELECT tablename FROM pg_tables WHERE schemaname = 'public'"
        )
        tables = {row[0] for row in cur.fetchall()}
    if not tables:
        return False
    if 'files' in tables:
        return True
    if 'song' not in tables:
        return True
    return False


def _ensure_schema_sqlite(db_path: str = DATABASE_FILE) -> None:
    if not os.path.isfile(db_path):
        return

    conn = sqlite3.connect(db_path)
    try:
        if needs_fatal_reset_sqlite(conn):
            logger.warning(
                'database schema incompatible (files table or missing song), skip additive migrate'
            )
            return

        cols = {row[1] for row in conn.execute('PRAGMA table_info(song)')}
        _apply_additive_columns(
            cols,
            SONG_ADDITIVE_COLUMNS,
            'ALTER TABLE song ADD COLUMN {name} {sql_type}',
            conn.execute,
            conn.commit,
        )
    finally:
        conn.close()


def _ensure_schema_postgres(dsn: str) -> None:
    import psycopg2

    conn = psycopg2.connect(dsn)
    try:
        if needs_fatal_reset_postgres(conn):
            logger.warning(
                'database schema incompatible (files table or missing song), skip additive migrate'
            )
            return

        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT column_name
                FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'song'
                """
            )
            cols = {row[0] for row in cur.fetchall()}

            def execute(sql: str) -> None:
                cur.execute(sql)

            _apply_additive_columns(
                cols,
                POSTGRES_SONG_ADDITIVE_COLUMNS,
                'ALTER TABLE song ADD COLUMN {name} {sql_type}',
                execute,
                conn.commit,
            )
    finally:
        conn.close()


def ensure_schema(dsn: str | None = None) -> None:
    url = dsn or DATABASE_URL
    if is_postgres_url(url):
        _ensure_schema_postgres(postgres_dsn(url))
    else:
        db_path = url.removeprefix('sqlite://') if url.startswith('sqlite://') else DATABASE_FILE
        _ensure_schema_sqlite(db_path)
