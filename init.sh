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
import sqlite3, sys
conn = sqlite3.connect("/KTV/sqlite3.db")
tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
conn.close()
if "files" in tables or "song" not in tables:
    sys.exit(1)
sys.exit(0)
PY
  python - <<'PY'
from karaoke.infra.db_schema import ensure_schema
ensure_schema("/KTV/sqlite3.db")
PY
fi
if [ ! -f "$DATABASE_FILE" ]; then
  aerich init -t settings.TORTOISE_ORM 2>/dev/null || true
  aerich init-db
else
  echo "Database file found. Skipping initialization."
fi
mkdir -p /KTV/__keep__ /KTV/__override__ /KTV/__play_cache__
exec "$@"
