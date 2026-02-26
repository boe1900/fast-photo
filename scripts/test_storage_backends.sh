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

upload_photo_call() {
  local file_path="$1"
  local token="$2"

  local body_file
  body_file="$(mktemp)"
  local status
  status="$(curl -sS -o "$body_file" -w "%{http_code}" \
    -X POST "$API_BASE/photos/upload" \
    -H "Authorization: Bearer $token" \
    -F "file=@${file_path};type=image/png")"
  local body
  body="$(cat "$body_file")"
  rm -f "$body_file"

  API_STATUS="$status"
  API_BODY="$body"
}

auth_status_call() {
  local method="$1"
  local url="$2"
  local token="$3"
  local body_file
  body_file="$(mktemp)"
  local status
  status="$(curl -sS -o "$body_file" -w "%{http_code}" \
    -X "$method" "$url" \
    -H "Authorization: Bearer $token")"
  local body
  body="$(cat "$body_file")"
  rm -f "$body_file"
  API_STATUS="$status"
  API_BODY="$body"
}

wait_library_scan_completed() {
  local lib_id="$1"
  local token="$2"
  local tries="${3:-120}"
  for _ in $(seq 1 "$tries"); do
    api_call "GET" "$API_BASE/libraries" "" "$token"
    if [[ "$API_STATUS" != "200" ]]; then
      sleep 1
      continue
    fi

    local status
    status="$(printf '%s' "$API_BODY" | jq -r --argjson id "$lib_id" '.[] | select(.id == $id) | .scan_status')"
    if [[ "$status" == "completed" ]]; then
      return 0
    fi
    if [[ "$status" == "error" ]]; then
      echo "[storage-test] 图库扫描失败: lib_id=$lib_id" >&2
      return 1
    fi
    sleep 1
  done
  echo "[storage-test] 图库扫描超时: lib_id=$lib_id" >&2
  return 1
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

TEST_LIB_DIR="$(mktemp -d /tmp/fast-photo-storage-lib.XXXXXX)"
LIB_NAME="storage-lib-$(date +%s)"
log "创建上传用图库: $TEST_LIB_DIR"
api_call "POST" "$API_BASE/libraries" "{\"name\":\"$LIB_NAME\",\"path\":\"$TEST_LIB_DIR\"}" "$TOKEN"
assert_status "$API_STATUS" "201" "创建图库"
LIB_ID="$(printf '%s' "$API_BODY" | jq -r '.id')"
if [[ -z "$LIB_ID" || "$LIB_ID" == "null" ]]; then
  echo "[storage-test] 创建图库未返回 id" >&2
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

SAMPLE_PNG="$(mktemp /tmp/fast-photo-storage-upload.XXXXXX)"
mv "$SAMPLE_PNG" "${SAMPLE_PNG}.png"
SAMPLE_PNG="${SAMPLE_PNG}.png"
if base64 --help >/dev/null 2>&1; then
  printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR42mP4z8AAAAMBAQD3A0FDAAAAAElFTkSuQmCC' | base64 --decode >"$SAMPLE_PNG"
else
  printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR42mP4z8AAAAMBAQD3A0FDAAAAAElFTkSuQmCC' | base64 -D >"$SAMPLE_PNG"
fi

S3_SCAN_PREFIX="s3-scan-$(date +%s)-$RANDOM"
S3_SCAN_FILE_NAME="s3-scanned-$(date +%s)-$RANDOM.png"
S3_SCAN_NESTED_FILE_NAME="s3-scanned-nested-$(date +%s)-$RANDOM.png"
log "创建 S3 远端扫描图库: $S3_SCAN_PREFIX"
api_call "POST" "$API_BASE/libraries" "{\"name\":\"s3-scan-lib\",\"path\":\"$S3_SCAN_PREFIX\"}" "$TOKEN"
assert_status "$API_STATUS" "201" "创建 S3 远端扫描图库"
S3_SCAN_LIB_ID="$(printf '%s' "$API_BODY" | jq -r '.id')"
if [[ -z "$S3_SCAN_LIB_ID" || "$S3_SCAN_LIB_ID" == "null" ]]; then
  echo "[storage-test] S3 扫描图库未返回 id" >&2
  exit 1
fi
S3_SCAN_REMOTE_KEY="$S3_SCAN_PREFIX/$S3_SCAN_FILE_NAME"
S3_SCAN_NESTED_REMOTE_KEY="$S3_SCAN_PREFIX/nested/$S3_SCAN_NESTED_FILE_NAME"
cat "$SAMPLE_PNG" | docker run --rm -i \
  --network "${COMPOSE_PROJECT}_default" \
  -e "MC_HOST_local=http://$MINIO_USER:$MINIO_PASS@minio:9000" \
  minio/mc pipe "local/$BUCKET/$S3_SCAN_REMOTE_KEY" >/dev/null
cat "$SAMPLE_PNG" | docker run --rm -i \
  --network "${COMPOSE_PROJECT}_default" \
  -e "MC_HOST_local=http://$MINIO_USER:$MINIO_PASS@minio:9000" \
  minio/mc pipe "local/$BUCKET/$S3_SCAN_NESTED_REMOTE_KEY" >/dev/null
log "触发 S3 远端源头扫描入库"
auth_status_call "POST" "$API_BASE/libraries/$S3_SCAN_LIB_ID/scan" "$TOKEN"
assert_status "$API_STATUS" "200" "触发 S3 远端扫描"
wait_library_scan_completed "$S3_SCAN_LIB_ID" "$TOKEN" 120
api_call "GET" "$API_BASE/photos/timeline?page=1&per_page=200" "" "$TOKEN"
assert_status "$API_STATUS" "200" "读取时间线（S3 扫描后）"
printf '%s' "$API_BODY" | jq -e --arg name1 "$S3_SCAN_FILE_NAME" --arg name2 "$S3_SCAN_NESTED_FILE_NAME" '.data | any(.file_name == $name1) and any(.file_name == $name2)' >/dev/null

log "S3 主链路上传并读取原图/缩略图"
upload_photo_call "$SAMPLE_PNG" "$TOKEN"
assert_status "$API_STATUS" "200" "S3 上传"
S3_PHOTO_ID="$(printf '%s' "$API_BODY" | jq -r '.uploaded[0].id')"
S3_FILE_NAME="$(printf '%s' "$API_BODY" | jq -r '.uploaded[0].file_name')"
if [[ -z "$S3_PHOTO_ID" || "$S3_PHOTO_ID" == "null" ]]; then
  echo "[storage-test] S3 上传未返回 photo id" >&2
  exit 1
fi
if [[ -z "$S3_FILE_NAME" || "$S3_FILE_NAME" == "null" ]]; then
  echo "[storage-test] S3 上传未返回 file_name" >&2
  exit 1
fi
api_call "GET" "$API_BASE/photos/$S3_PHOTO_ID" "" "$TOKEN"
assert_status "$API_STATUS" "200" "读取 S3 照片详情"
S3_REMOTE_KEY="$(printf '%s' "$API_BODY" | jq -r '.photo.file_path')"
if [[ -z "$S3_REMOTE_KEY" || "$S3_REMOTE_KEY" == "null" ]]; then
  echo "[storage-test] S3 照片详情未返回 file_path" >&2
  exit 1
fi
S3_ORIG_FILE="$(mktemp)"
S3_ORIG_STATUS="$(curl -sS -o "$S3_ORIG_FILE" -w "%{http_code}" -H "Authorization: Bearer $TOKEN" "$API_BASE/photos/$S3_PHOTO_ID/original")"
if [[ "$S3_ORIG_STATUS" != "200" ]]; then
  echo "[storage-test] S3 原图读取失败: status=$S3_ORIG_STATUS" >&2
  exit 1
fi
if [[ ! -s "$S3_ORIG_FILE" ]]; then
  echo "[storage-test] S3 原图响应为空" >&2
  exit 1
fi
S3_THUMB_STATUS="$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $TOKEN" "$API_BASE/photos/$S3_PHOTO_ID/thumbnail/small")"
if [[ "$S3_THUMB_STATUS" != "200" ]]; then
  echo "[storage-test] S3 缩略图读取失败: status=$S3_THUMB_STATUS" >&2
  exit 1
fi
docker run --rm \
  --network "${COMPOSE_PROJECT}_default" \
  -e "MC_HOST_local=http://$MINIO_USER:$MINIO_PASS@minio:9000" \
  minio/mc stat "local/$BUCKET/$S3_REMOTE_KEY" >/dev/null

log "S3 回收站清空后应删除远端对象"
auth_status_call "POST" "$API_BASE/photos/$S3_PHOTO_ID/trash" "$TOKEN"
assert_status "$API_STATUS" "200" "S3 移入回收站"
auth_status_call "POST" "$API_BASE/photos/trash/empty" "$TOKEN"
assert_status "$API_STATUS" "200" "S3 清空回收站"
if docker run --rm \
  --network "${COMPOSE_PROJECT}_default" \
  -e "MC_HOST_local=http://$MINIO_USER:$MINIO_PASS@minio:9000" \
  minio/mc stat "local/$BUCKET/$S3_REMOTE_KEY" >/dev/null 2>&1; then
  echo "[storage-test] S3 清空回收站后对象仍存在" >&2
  exit 1
fi

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

DAV_SCAN_PREFIX="webdav-scan-$(date +%s)-$RANDOM"
DAV_SCAN_FILE_NAME="webdav-scanned-$(date +%s)-$RANDOM.png"
DAV_SCAN_NESTED_FILE_NAME="webdav-scanned-nested-$(date +%s)-$RANDOM.png"
log "创建 WebDAV 远端扫描图库: $DAV_SCAN_PREFIX"
api_call "POST" "$API_BASE/libraries" "{\"name\":\"webdav-scan-lib\",\"path\":\"$DAV_SCAN_PREFIX\"}" "$TOKEN"
assert_status "$API_STATUS" "201" "创建 WebDAV 远端扫描图库"
DAV_SCAN_LIB_ID="$(printf '%s' "$API_BODY" | jq -r '.id')"
if [[ -z "$DAV_SCAN_LIB_ID" || "$DAV_SCAN_LIB_ID" == "null" ]]; then
  echo "[storage-test] WebDAV 扫描图库未返回 id" >&2
  exit 1
fi
curl -sS -o /dev/null -u "$WEBDAV_USER:$WEBDAV_PASS" -X MKCOL "$WEBDAV_URL/$DAV_SCAN_PREFIX" || true
curl -sS -o /dev/null -u "$WEBDAV_USER:$WEBDAV_PASS" -X MKCOL "$WEBDAV_URL/$DAV_SCAN_PREFIX/nested" || true
curl -sS -o /dev/null -u "$WEBDAV_USER:$WEBDAV_PASS" -T "$SAMPLE_PNG" "$WEBDAV_URL/$DAV_SCAN_PREFIX/$DAV_SCAN_FILE_NAME"
curl -sS -o /dev/null -u "$WEBDAV_USER:$WEBDAV_PASS" -T "$SAMPLE_PNG" "$WEBDAV_URL/$DAV_SCAN_PREFIX/nested/$DAV_SCAN_NESTED_FILE_NAME"
log "触发 WebDAV 远端源头扫描入库"
auth_status_call "POST" "$API_BASE/libraries/$DAV_SCAN_LIB_ID/scan" "$TOKEN"
assert_status "$API_STATUS" "200" "触发 WebDAV 远端扫描"
wait_library_scan_completed "$DAV_SCAN_LIB_ID" "$TOKEN" 120
api_call "GET" "$API_BASE/photos/timeline?page=1&per_page=400" "" "$TOKEN"
assert_status "$API_STATUS" "200" "读取时间线（WebDAV 扫描后）"
printf '%s' "$API_BODY" | jq -e --arg name1 "$DAV_SCAN_FILE_NAME" --arg name2 "$DAV_SCAN_NESTED_FILE_NAME" '.data | any(.file_name == $name1) and any(.file_name == $name2)' >/dev/null

log "WebDAV 主链路上传并读取原图/缩略图"
upload_photo_call "$SAMPLE_PNG" "$TOKEN"
assert_status "$API_STATUS" "200" "WebDAV 上传"
DAV_PHOTO_ID="$(printf '%s' "$API_BODY" | jq -r '.uploaded[0].id')"
DAV_FILE_NAME="$(printf '%s' "$API_BODY" | jq -r '.uploaded[0].file_name')"
if [[ -z "$DAV_PHOTO_ID" || "$DAV_PHOTO_ID" == "null" ]]; then
  echo "[storage-test] WebDAV 上传未返回 photo id" >&2
  exit 1
fi
if [[ -z "$DAV_FILE_NAME" || "$DAV_FILE_NAME" == "null" ]]; then
  echo "[storage-test] WebDAV 上传未返回 file_name" >&2
  exit 1
fi
api_call "GET" "$API_BASE/photos/$DAV_PHOTO_ID" "" "$TOKEN"
assert_status "$API_STATUS" "200" "读取 WebDAV 照片详情"
DAV_REMOTE_KEY="$(printf '%s' "$API_BODY" | jq -r '.photo.file_path')"
if [[ -z "$DAV_REMOTE_KEY" || "$DAV_REMOTE_KEY" == "null" ]]; then
  echo "[storage-test] WebDAV 照片详情未返回 file_path" >&2
  exit 1
fi
DAV_ORIG_FILE="$(mktemp)"
DAV_ORIG_STATUS="$(curl -sS -o "$DAV_ORIG_FILE" -w "%{http_code}" -H "Authorization: Bearer $TOKEN" "$API_BASE/photos/$DAV_PHOTO_ID/original")"
if [[ "$DAV_ORIG_STATUS" != "200" ]]; then
  echo "[storage-test] WebDAV 原图读取失败: status=$DAV_ORIG_STATUS" >&2
  exit 1
fi
if [[ ! -s "$DAV_ORIG_FILE" ]]; then
  echo "[storage-test] WebDAV 原图响应为空" >&2
  exit 1
fi
DAV_THUMB_STATUS="$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $TOKEN" "$API_BASE/photos/$DAV_PHOTO_ID/thumbnail/small")"
if [[ "$DAV_THUMB_STATUS" != "200" ]]; then
  echo "[storage-test] WebDAV 缩略图读取失败: status=$DAV_THUMB_STATUS" >&2
  exit 1
fi
DAV_REMOTE_STATUS="$(curl -sS -o /dev/null -w "%{http_code}" -u "$WEBDAV_USER:$WEBDAV_PASS" "$WEBDAV_URL/$DAV_REMOTE_KEY")"
if [[ "$DAV_REMOTE_STATUS" != "200" ]]; then
  echo "[storage-test] WebDAV 远端对象未找到: status=$DAV_REMOTE_STATUS" >&2
  exit 1
fi

log "WebDAV 永久删除应删除远端对象"
auth_status_call "DELETE" "$API_BASE/photos/$DAV_PHOTO_ID/permanent" "$TOKEN"
assert_status "$API_STATUS" "200" "WebDAV 永久删除"
DAV_REMOTE_AFTER_DELETE="$(curl -sS -o /dev/null -w "%{http_code}" -u "$WEBDAV_USER:$WEBDAV_PASS" "$WEBDAV_URL/$DAV_REMOTE_KEY")"
if [[ "$DAV_REMOTE_AFTER_DELETE" != "404" ]]; then
  echo "[storage-test] WebDAV 永久删除后对象仍存在: status=$DAV_REMOTE_AFTER_DELETE" >&2
  exit 1
fi

WEBDAV_BAD_JSON="{\"type\":\"webdav\",\"url\":\"$WEBDAV_URL\",\"username\":\"$WEBDAV_USER\",\"password\":\"wrong-pass\",\"prefix\":null}"
log "测试 WebDAV 配置（错误凭据）"
api_call "POST" "$API_BASE/settings/storage/test" "$WEBDAV_BAD_JSON" "$TOKEN"
assert_status "$API_STATUS" "400" "WebDAV 测试失败场景"

log "读取当前存储配置（应为 WebDAV）"
api_call "GET" "$API_BASE/settings/storage" "" "$TOKEN"
assert_status "$API_STATUS" "200" "读取存储配置"
printf '%s' "$API_BODY" | jq -e '.type == "webdav" and .username == "webdav_user"' >/dev/null

log "所有 S3/WebDAV 联调测试通过"
