<div align="center">

# ⚡ FastPhoto

**一个高性能、自托管的智能照片管理系统**

Rust + React · AI 语义搜索 · 人脸识别 · Live Photos

</div>

---

## ✨ 功能特性

### 📸 照片管理
- **时间线浏览** — 按时间顺序展示照片，无限滚动
- **文件夹浏览** — 按原始目录结构浏览
- **地图视图** — GPS 定位照片在 Leaflet 地图上展示
- **相册管理** — 创建相册，支持分享链接 + 密码保护
- **照片上传** — 拖拽或选择文件上传

### 🤖 AI 能力
- **语义搜索** — 基于 CLIP 模型，用自然语言搜索照片（如「蓝天白云」「海边的人」）
- **场景分类** — 自动识别 25 种场景类别（风景、人物、美食、建筑等）
- **人脸识别** — 检测人脸并自动聚类，支持命名

### 🛠 高级功能
- **Live Photos** — 自动配对 MOV/MP4，悬停或长按播放
- **视频支持** — FFmpeg 转码 + 在线播放
- **收藏 & 回收站** — 星标收藏 + 软删除（可还原）
- **批量操作** — 多选照片进行收藏/删除
- **重复检测** — 感知哈希识别视觉相似照片
- **标签系统** — 手动/AI 标签 + 按标签搜索
- **活动日志** — 记录用户操作历史
- **主题切换** — 深色/浅色主题，一键切换

### 🏗 架构特点
- **高性能后端** — Rust (Axum) + SQLite，低资源占用
- **多级缩略图** — 自动生成 small/medium/large WebP 缩略图
- **外部存储** — 支持本地、S3、WebDAV 存储后端
- **Docker 部署** — 多阶段构建，一键启动

---

## 🚀 快速开始

### Docker 部署（推荐）

```bash
git clone https://github.com/boe1900/fast-photo.git
cd fast-photo
docker compose up -d
```

访问 `http://localhost:8080`，首次使用请注册账号。

### 本地开发

**前置要求：**
- Rust 1.75+
- Node.js 18+
- FFmpeg（视频功能）

```bash
# 克隆项目
git clone https://github.com/boe1900/fast-photo.git
cd fast-photo

# 下载 AI 模型
bash models/download.sh        # CLIP 模型
bash models/download_face.sh   # 人脸识别模型（可选）

# 启动后端
cargo run

# 启动前端（新终端）
cd web
npm install
npm run dev
```

后端默认运行在 `http://localhost:8080`，前端开发服务器在 `http://localhost:3000`。

### 测试命令

```bash
# 后端测试
cargo test

# 前端 E2E
cd web && npm run test:e2e

# S3/WebDAV 连通性联调（会自动拉起 MinIO + WebDAV）
./scripts/test_storage_backends.sh

# 设置页远程存储 UI 联调 E2E（S3/WebDAV）
cd web && FASTPHOTO_ENABLE_STORAGE_UI_E2E=1 npm run test:e2e
```

---

## 📁 项目结构

```
fast-photo/
├── crates/
│   ├── core/          # 数据库、模型、扫描器、缩略图
│   ├── server/        # HTTP API (Axum)
│   ├── ai/            # CLIP/人脸 AI 客户端
│   └── common/        # 共享配置和错误类型
├── web/               # React + TypeScript 前端
├── models/            # AI 模型文件（.onnx）
├── docs/              # 功能文档
├── Dockerfile         # 多阶段 Docker 构建
└── docker-compose.yml
```

---

## 🖥 技术栈

| 层 | 技术 |
|----|------|
| 后端 | Rust, Axum, SQLx, SQLite |
| 前端 | React 19, TypeScript, Vite |
| AI | ONNX Runtime, CLIP, ArcFace |
| 地图 | Leaflet + React-Leaflet |
| 容器 | Docker 多阶段构建 |

---

## ⚙️ 配置

创建 `config.toml` 自定义配置（可选）：

```toml
[server]
host = "0.0.0.0"
port = 8080

[database]
url = "sqlite:data/fast-photo.db"

[storage]
thumbnail_dir = "data/thumbnails"

[ai]
models_dir = "models"
```

---

## 📖 文档

- 功能与 API：`docs/FEATURES.md`
- 测试清单：`docs/TESTING.md`
- 部署说明：`docs/DEPLOYMENT.md`
- 备份恢复：`docs/BACKUP.md`
- 发版清单：`docs/RELEASE_CHECKLIST.md`

---

## 🗺 路线图

- [ ] OCR 文字识别
- [ ] HEIC/HEIF 格式支持
- [ ] 多用户独立图库
- [ ] 国际化 (i18n)
- [ ] PWA / 移动端适配
- [ ] 照片编辑（裁剪/滤镜）
- [ ] 自动备份与同步

---

## 📄 License

MIT License

---

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'feat: add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 提交 Pull Request
