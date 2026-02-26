#!/usr/bin/env bash
set -euo pipefail

ALERT_FILE="${1:-data/alerts.ndjson}"

if [[ ! -f "$ALERT_FILE" ]]; then
  echo "[alerts] 文件不存在，先创建: $ALERT_FILE"
  mkdir -p "$(dirname "$ALERT_FILE")"
  touch "$ALERT_FILE"
fi

echo "[alerts] 正在跟踪: $ALERT_FILE"
if command -v jq >/dev/null 2>&1; then
  tail -n 50 -f "$ALERT_FILE" | jq -c .
else
  tail -n 50 -f "$ALERT_FILE"
fi
