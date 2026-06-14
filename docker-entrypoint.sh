#!/bin/bash
set -euo pipefail
DATABASE_FILE="/KTV/sqlite3.db"

reset_db() {
  rm -f "$DATABASE_FILE"
  echo "Database reset: ${DATABASE_FILE}"
}

if [ "${FORCE_DB_RESET:-0}" = "1" ]; then
  reset_db
elif [ -f "$DATABASE_FILE" ]; then
  python - <<'PY' || reset_db
import sqlite3
import sys

conn = sqlite3.connect("/KTV/sqlite3.db")
tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
cols = []
if "song" in tables:
    cols = [r[1] for r in conn.execute("PRAGMA table_info(song)")]
conn.close()
if "files" in tables or "song" not in tables or "audio_layout" not in cols:
    sys.exit(1)
sys.exit(0)
PY
fi

if [ ! -f "$DATABASE_FILE" ]; then
  if [ ! -f aerich.ini ]; then
    aerich init -t settings.TORTOISE_ORM
  fi
  aerich init-db
  echo "Database initialized at ${DATABASE_FILE}"
else
  echo "Database found, skipping initialization."
fi

mkdir -p /KTV/__keep__ /KTV/__override__ /KTV/__play_cache__
exec "$@"
