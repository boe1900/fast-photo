#!/usr/bin/env bash
set -euo pipefail

DEFAULT_DB_URL="sqlite:data/fast-photo.db?mode=rwc"
DB_URL="${FAST_PHOTO_DB_URL:-$DEFAULT_DB_URL}"
OUT_DIR="${1:-backups}"

extract_sqlite_path() {
  local url="$1"
  local without_prefix="${url#sqlite:}"
  echo "${without_prefix%%\?*}"
}

DB_PATH="$(extract_sqlite_path "$DB_URL")"
if [[ -z "$DB_PATH" ]]; then
  echo "[backup] 无法从 DB URL 解析 SQLite 路径: $DB_URL" >&2
  exit 1
fi

if [[ ! -f "$DB_PATH" ]]; then
  echo "[backup] 数据库文件不存在: $DB_PATH" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_FILE="$OUT_DIR/fast-photo-$TIMESTAMP.db"

if command -v sqlite3 >/dev/null 2>&1; then
  sqlite3 "$DB_PATH" ".timeout 5000" ".backup '$BACKUP_FILE'"
else
  cp "$DB_PATH" "$BACKUP_FILE"
fi

if command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "$BACKUP_FILE" > "$BACKUP_FILE.sha256"
fi

echo "[backup] 完成: $BACKUP_FILE"
