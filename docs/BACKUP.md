# 数据库备份与恢复

## 默认路径

- 数据库 URL 默认值：`sqlite:data/fast-photo.db?mode=rwc`
- 实际数据库文件：`data/fast-photo.db`

可通过环境变量覆盖：

- `FAST_PHOTO_DB_URL=sqlite:/your/path/fast-photo.db?mode=rwc`

## 备份

```bash
./scripts/backup_db.sh
```

可选参数：

```bash
./scripts/backup_db.sh /path/to/backups
```

输出示例：

- `backups/fast-photo-YYYYMMDD-HHMMSS.db`
- `backups/fast-photo-YYYYMMDD-HHMMSS.db.sha256`（如果系统有 `shasum`）

## 恢复

> 恢复前请先停掉服务，避免写入冲突。

```bash
./scripts/restore_db.sh backups/fast-photo-YYYYMMDD-HHMMSS.db
```

恢复时会自动备份当前数据库：

- `data/fast-photo.db.before-restore.YYYYMMDD-HHMMSS`

## 建议策略

1. 至少每日备份一次。
2. 发版前做一次手动备份。
3. 保留最近 7~30 天备份（按存储容量调整）。
