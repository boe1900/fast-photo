#!/usr/bin/env bash
set -euo pipefail

DEFAULT_DB_URL="sqlite:data/fast-photo.db?mode=rwc"
DB_URL="${FAST_PHOTO_DB_URL:-$DEFAULT_DB_URL}"

if [[ $# -lt 1 ]]; then
  echo "用法: $0 <backup-file.db>" >&2
  exit 1
fi

BACKUP_FILE="$1"

if [[ ! -f "$BACKUP_FILE" ]]; then
  echo "[restore] 备份文件不存在: $BACKUP_FILE" >&2
  exit 1
fi

extract_sqlite_path() {
  local url="$1"
  local without_prefix="${url#sqlite:}"
  echo "${without_prefix%%\?*}"
}

DB_PATH="$(extract_sqlite_path "$DB_URL")"
if [[ -z "$DB_PATH" ]]; then
  echo "[restore] 无法从 DB URL 解析 SQLite 路径: $DB_URL" >&2
  exit 1
fi

mkdir -p "$(dirname "$DB_PATH")"

if command -v sqlite3 >/dev/null 2>&1; then
  CHECK_RESULT="$(sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;" | head -n 1 || true)"
  if [[ "$CHECK_RESULT" != "ok" ]]; then
    echo "[restore] 备份文件完整性校验失败: $CHECK_RESULT" >&2
    exit 1
  fi
fi

if [[ -f "$DB_PATH" ]]; then
  PRE_RESTORE="${DB_PATH}.before-restore.$(date +%Y%m%d-%H%M%S)"
  cp "$DB_PATH" "$PRE_RESTORE"
  echo "[restore] 已备份当前数据库: $PRE_RESTORE"
fi

cp "$BACKUP_FILE" "$DB_PATH"

echo "[restore] 已恢复数据库到: $DB_PATH"
