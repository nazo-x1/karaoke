#!/bin/bash
set -euo pipefail
mkdir -p /KTV/__keep__ /KTV/__override__ /KTV/__play_cache__ /KTV/logs

wait_for_db() {
  python - <<'PY'
import sys
import time

from settings import DATABASE_URL, is_postgres_url, postgres_dsn

if not is_postgres_url(DATABASE_URL):
    sys.exit(0)

import psycopg2

dsn = postgres_dsn(DATABASE_URL)
for _ in range(60):
    try:
        conn = psycopg2.connect(dsn)
        conn.close()
        sys.exit(0)
    except Exception:
        time.sleep(1)
print("Timed out waiting for PostgreSQL", file=sys.stderr)
sys.exit(1)
PY
}

reset_db() {
  python - <<'PY'
from settings import DATABASE_URL, is_postgres_url, postgres_dsn

if is_postgres_url(DATABASE_URL):
    import psycopg2

    conn = psycopg2.connect(postgres_dsn(DATABASE_URL))
    conn.autocommit = True
    with conn.cursor() as cur:
        cur.execute("DROP SCHEMA IF EXISTS public CASCADE")
        cur.execute("CREATE SCHEMA public")
        cur.execute("GRANT ALL ON SCHEMA public TO public")
    conn.close()
    print("Database reset: PostgreSQL schema recreated")
else:
    import os

    db_file = DATABASE_URL.removeprefix("sqlite://")
    if os.path.isfile(db_file):
        os.remove(db_file)
    print(f"Database reset: {db_file}")
PY
}

schema_needs_reset() {
  python - <<'PY'
import sys

from karaoke.infra.db_schema import needs_fatal_reset_postgres, needs_fatal_reset_sqlite
from settings import DATABASE_URL, is_postgres_url, postgres_dsn

if is_postgres_url(DATABASE_URL):
    import psycopg2

    conn = psycopg2.connect(postgres_dsn(DATABASE_URL))
    try:
        sys.exit(1 if needs_fatal_reset_postgres(conn) else 0)
    finally:
        conn.close()
else:
    import os
    import sqlite3

    db_file = DATABASE_URL.removeprefix("sqlite://")
    if not os.path.isfile(db_file):
        sys.exit(0)
    conn = sqlite3.connect(db_file)
    try:
        sys.exit(1 if needs_fatal_reset_sqlite(conn) else 0)
    finally:
        conn.close()
PY
}

wait_for_db

if [ "${FORCE_DB_RESET:-0}" = "1" ]; then
  reset_db
elif schema_needs_reset; then
  echo "Incompatible database schema detected, resetting..."
  reset_db
else
  python - <<'PY'
from karaoke.infra.db_schema import ensure_schema

ensure_schema()
PY
fi

if python - <<'PY'
import sys

from settings import DATABASE_URL, is_postgres_url, postgres_dsn

if is_postgres_url(DATABASE_URL):
    import psycopg2

    conn = psycopg2.connect(postgres_dsn(DATABASE_URL))
    try:
        with conn.cursor() as cur:
            cur.execute(
                "SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename = 'aerich'"
            )
            sys.exit(0 if cur.fetchone() else 1)
    finally:
        conn.close()
else:
    import os
    import sqlite3

    db_file = DATABASE_URL.removeprefix("sqlite://")
    if not os.path.isfile(db_file):
        sys.exit(1)
    conn = sqlite3.connect(db_file)
    try:
        tables = {
            row[0]
            for row in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table'"
            )
        }
        sys.exit(0 if 'aerich' in tables else 1)
    finally:
        conn.close()
PY
then
  aerich upgrade
  echo "Database migrated."
else
  if [ ! -f aerich.ini ]; then
    aerich init -t settings.TORTOISE_ORM
  fi
  aerich init-db
  echo "Database initialized."
fi

exec "$@"
