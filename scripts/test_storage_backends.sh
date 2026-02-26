#!/usr/bin/env bash
set -euo pipefail

COMPOSE_FILE="docker-compose.storage-backends.yml"
COMPOSE_PROJECT="fast-photo-storage-it"
API_BASE="http://127.0.0.1:8080/api"
MINIO_ENDPOINT="http://127.0.0.1:19000"
WEBDAV_URL="http://127.0.0.1:19080"
MINIO_USER="minioadmin"
MINIO_PASS="minioadmin123"
WEBDAV_USER="webdav_user"
WEBDAV_PASS="webdav_pass"
BUCKET="fast-photo-test"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[storage-test] 缺少命令: $1" >&2
    exit 1
  fi
}

require_cmd docker
require_cmd curl
require_cmd jq

SERVER_PID=""
cleanup() {
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  docker compose -p "$COMPOSE_PROJECT" -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true
}
trap cleanup EXIT

log() {
  echo "[storage-test] $*"
}

wait_http() {
  local url="$1"
  local tries="${2:-60}"
  for _ in $(seq 1 "$tries"); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "[storage-test] 超时等待: $url" >&2
  return 1
}

api_call() {
  local method="$1"
  local url="$2"
  local data="${3:-}"
  local token="${4:-}"

  local body_file
  body_file="$(mktemp)"

  local -a args
  args=( -sS -o "$body_file" -w "%{http_code}" -X "$method" "$url" )
  if [[ -n "$token" ]]; then
    args+=( -H "Authorization: Bearer $token" )
  fi
  args+=( -H "Content-Type: application/json" )
  if [[ -n "$data" ]]; then
    args+=( -d "$data" )
  fi

  local status
  status="$(curl "${args[@]}")"
  local body
  body="$(cat "$body_file")"
  rm -f "$body_file"

  API_STATUS="$status"
  API_BODY="$body"
}

assert_status() {
  local got="$1"
  local expect="$2"
  local context="$3"
  if [[ "$got" != "$expect" ]]; then
    echo "[storage-test] ${context} 状态码错误: got=$got expect=$expect" >&2
    if [[ -n "${API_BODY:-}" ]]; then
      echo "[storage-test] 响应体: $API_BODY" >&2
    fi
    exit 1
  fi
}

log "启动 MinIO + WebDAV 容器"
docker compose -p "$COMPOSE_PROJECT" -f "$COMPOSE_FILE" up -d

log "等待 MinIO 就绪"
wait_http "$MINIO_ENDPOINT/minio/health/ready" 120

log "等待 WebDAV 就绪"
for _ in $(seq 1 60); do
  if curl -fsS -u "$WEBDAV_USER:$WEBDAV_PASS" -X PROPFIND "$WEBDAV_URL/" >/dev/null 2>&1; then
    break
  fi
  sleep 1
  if [[ "$_" == "60" ]]; then
    echo "[storage-test] WebDAV 未就绪" >&2
    exit 1
  fi
done

log "创建 MinIO bucket: $BUCKET"
docker run --rm \
  --network "${COMPOSE_PROJECT}_default" \
  -e "MC_HOST_local=http://$MINIO_USER:$MINIO_PASS@minio:9000" \
  minio/mc mb -p "local/$BUCKET" >/dev/null || true

log "启动 FastPhoto 服务"
cargo run -p fast-photo-server --bin fast-photo >/tmp/fast-photo-storage-test.log 2>&1 &
SERVER_PID="$!"
wait_http "http://127.0.0.1:8080/healthz" 90

USERNAME="storage_it_$(date +%s)_$RANDOM"
PASSWORD="e2e-pass-123"

log "注册测试用户: $USERNAME"
api_call "POST" "$API_BASE/auth/register" "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}"
assert_status "$API_STATUS" "201" "注册"
TOKEN="$(printf '%s' "$API_BODY" | jq -r '.token')"
if [[ -z "$TOKEN" || "$TOKEN" == "null" ]]; then
  echo "[storage-test] 注册响应未返回 token" >&2
  exit 1
fi

S3_JSON="{\"type\":\"s3\",\"bucket\":\"$BUCKET\",\"region\":\"us-east-1\",\"endpoint\":\"$MINIO_ENDPOINT\",\"access_key\":\"$MINIO_USER\",\"secret_key\":\"$MINIO_PASS\",\"prefix\":null}"

log "保存 S3 配置"
api_call "PUT" "$API_BASE/settings/storage" "$S3_JSON" "$TOKEN"
assert_status "$API_STATUS" "200" "S3 保存"
printf '%s' "$API_BODY" | jq -e '.type == "s3" and .bucket == "fast-photo-test"' >/dev/null

log "测试 S3 配置（正确凭据）"
api_call "POST" "$API_BASE/settings/storage/test" "$S3_JSON" "$TOKEN"
assert_status "$API_STATUS" "200" "S3 测试成功场景"
printf '%s' "$API_BODY" | jq -e '.ok == true' >/dev/null

S3_BAD_JSON="{\"type\":\"s3\",\"bucket\":\"$BUCKET\",\"region\":\"us-east-1\",\"endpoint\":\"$MINIO_ENDPOINT\",\"access_key\":\"$MINIO_USER\",\"secret_key\":\"wrong-secret\",\"prefix\":null}"
log "测试 S3 配置（错误凭据）"
api_call "POST" "$API_BASE/settings/storage/test" "$S3_BAD_JSON" "$TOKEN"
assert_status "$API_STATUS" "400" "S3 测试失败场景"

WEBDAV_JSON="{\"type\":\"webdav\",\"url\":\"$WEBDAV_URL\",\"username\":\"$WEBDAV_USER\",\"password\":\"$WEBDAV_PASS\",\"prefix\":null}"

log "保存 WebDAV 配置"
api_call "PUT" "$API_BASE/settings/storage" "$WEBDAV_JSON" "$TOKEN"
assert_status "$API_STATUS" "200" "WebDAV 保存"
printf '%s' "$API_BODY" | jq -e '.type == "webdav" and .url == "http://127.0.0.1:19080"' >/dev/null

log "测试 WebDAV 配置（正确凭据）"
api_call "POST" "$API_BASE/settings/storage/test" "$WEBDAV_JSON" "$TOKEN"
assert_status "$API_STATUS" "200" "WebDAV 测试成功场景"
printf '%s' "$API_BODY" | jq -e '.ok == true' >/dev/null

WEBDAV_BAD_JSON="{\"type\":\"webdav\",\"url\":\"$WEBDAV_URL\",\"username\":\"$WEBDAV_USER\",\"password\":\"wrong-pass\",\"prefix\":null}"
log "测试 WebDAV 配置（错误凭据）"
api_call "POST" "$API_BASE/settings/storage/test" "$WEBDAV_BAD_JSON" "$TOKEN"
assert_status "$API_STATUS" "400" "WebDAV 测试失败场景"

log "读取当前存储配置（应为 WebDAV）"
api_call "GET" "$API_BASE/settings/storage" "" "$TOKEN"
assert_status "$API_STATUS" "200" "读取存储配置"
printf '%s' "$API_BODY" | jq -e '.type == "webdav" and .username == "webdav_user"' >/dev/null

log "所有 S3/WebDAV 联调测试通过"
