# 部署说明（单机）

## 前置依赖

- Rust 工具链（构建后端）
- Node.js + npm（构建前端）
- 可写目录：`data/`

## 1. 构建

```bash
cargo build --release -p fast-photo-server --bin fast-photo
cd web && npm ci && npm run build
```

## 2. 配置

可选创建 `config.toml`，未提供则使用内置默认值。

关键项：

- `server.host`
- `server.port`
- `database.url`
- `storage.data_dir`
- `storage.thumbnail_dir`
- `auth.jwt_secret`

## 3. 启动

```bash
./target/release/fast-photo
```

前端静态文件可由 Nginx 等服务托管，API 反向代理到后端 `/api`。

启动后健康检查：

```bash
curl -s http://127.0.0.1:8080/healthz
```

## 4. 升级

1. 拉取新代码。
2. 停服务。
3. 备份数据库：`./scripts/backup_db.sh`
4. 重新构建并启动。
5. 回归冒烟：登录、时间线、上传、设置页。

## 5. 回滚

1. 停服务。
2. 恢复数据库：`./scripts/restore_db.sh <backup-file>`
3. 切回上个可用版本并重启。

## 6. 可观测性（最小集）

- 健康检查端点：`/healthz`
- 每个响应返回 `x-request-id`，用于串联日志排障
- 5xx 会写入告警文件：`data/alerts.ndjson`
- 实时查看告警：

```bash
./scripts/tail_alerts.sh
```
