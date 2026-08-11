# karaoke (V2, Rust)

家庭 KTV 点歌系统后端。V2 起由 Python/FastAPI 全量重写为 Rust（axum + tokio + sqlx），
对外 HTTP/SSE 契约与 V1 保持兼容，用于替换在小机器（2–4 vCPU / 2–4GB 内存）上运行不稳定的
Python 实现。旧版 Python 实现见本仓库 `V1` tag，作为回滚基线保留。

## 架构

Cargo workspace，多 crate 编译期强制分层（禁止 `domain` 反向依赖 `infra`）：

```
karaoke-app       主程序：配置/tracing/装配/启动 HTTP 服务
  └─ karaoke-api        axum 路由、ApiResult envelope、minijinja 模板、静态文件
       └─ karaoke-services   应用服务编排（library/queue/playback/song_config/cache）
            ├─ karaoke-domain    纯业务逻辑，无 IO 依赖
            ├─ karaoke-infra     sqlx 仓储、ffmpeg/ffprobe、扫描、Range 流
            ├─ karaoke-events    SSE 广播总线（连接上限+心跳+resync）
            └─ karaoke-jobs      prepare 后台任务队列（并发限制+终态 TTL 清理）
```

## 播放模型

- **plain（普通）**：单轨源文件，默认直发；客户端解码失败时调用
  `POST /api/v1/playback/songs/{id}/report-unplayable`，服务端跳过当前曲并强制后台生成
  `__play_cache__` 兜底 mp4；下次点歌若缓存就绪则自动走缓存。
- **embedded（增强）**：源容器已含原唱/伴奏双音轨时进入增强模式。视频直发源文件；
  prepare 只抽取两路音频缓存到 `__play_cache__/embedded/`。
- 曲库一等公民只有**单个视频源文件**。AI 分轨与「视频+双音频」组装只在**上传编辑页（工坊）**完成，
  再严格入库；库内歌曲无 AI/组装/侧车三文件入口。
- 扫描跳过目录：`__keep__`、`__workshop__`、`__play_cache__`。

## 导入与工坊

| 入口 | 行为 |
|------|------|
| `POST /library/upload` | **严格**单曲上传：临时落盘 → 预检必须可播 → 写入 `__keep__`+建行；失败零残留 |
| `POST /library/scan` | **宽松**扫描：可关校验、可先录入后验失败文件 |
| `/workshop` + `/api/v1/workshop/*` | 临时会话：预检 / 组装 / AI 分轨 → 成品 → `commit` 严格入库 |

工坊 AI 依赖独立 [separator](../separator/README.md) 服务，由 `[separator] enabled`（或环境变量
`SEPARATOR_ENABLED`）门控。未启用时隐藏 AI，预检与组装仍可用。

`GET /api/v1/system/features` 返回 `separator_enabled` 等前端开关。

## 本地开发

```bash
# 格式化 / lint / 测试
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 需要本机可访问的 PostgreSQL（见 config.toml / 环境变量）
cargo run -p karaoke-app
```

默认从当前工作目录读取 `config.toml`、`templates/`、`static/`（可用 `CONFIG_PATH` /
`TEMPLATES_DIR` / `STATIC_DIR` 环境变量覆盖）。

## 配置

`config.toml`（详见文件内注释）+ 环境变量覆盖：

- `DATABASE_URL`，或 `POSTGRES_HOST`/`POSTGRES_PORT`/`POSTGRES_USER`/`POSTGRES_PASSWORD`/`POSTGRES_DB`
- `PREPARE_MAX_CONCURRENT`：同时进行的播放资源准备（ffmpeg）任务数（默认 1）
- `transcode_max_height`：兜底转码分辨率上限（默认 1080）
- `probe_size` / `analyze_duration`：ffprobe 探测上限
- `scan_validate_concurrency`：扫描校验并发（默认 2）
- `[separator]` / `SEPARATOR_ENABLED`、`SEPARATOR_BASE_URL`、`SEPARATOR_API_TOKEN`、
  `SEPARATOR_CALLBACK_BASE_URL`：工坊 AI 对接（默认关闭）
- `[workshop]`：临时目录名与会话 TTL

扫描：`POST /library/scan` 先落库后返回；若开启校验，ffprobe 在后台跑，进度见
`GET /library/scan/status`（`total`/`done`/`invalid`/`running`）。

启用 separator 示例：

```bash
# 根目录
export SEPARATOR_API_TOKEN=change-me
export SEPARATOR_ENABLED=true
docker compose --profile separator up -d --build
```

## 数据库迁移

使用 `sqlx migrate`，迁移文件位于 `migrations/`；应用启动时自动执行 `run_migrations`。

## Docker 构建

```bash
docker build -t ghcr.io/<owner>/karaoke:latest .
# 国内网络可选：
docker build \
  --build-arg CARGO_REGISTRY=rsproxy-sparse \
  --build-arg APT_MIRROR=mirrors.tuna.tsinghua.edu.cn \
  -t ghcr.io/<owner>/karaoke:latest .
```

多阶段构建：builder 阶段 `cargo build --release`，运行阶段基于 `debian:bookworm-slim` + 系统 `ffmpeg`。

## 与 V1 的差异（内部实现）

- 数据库 schema 重新设计（`audio_layout` 由 `TEXT` 改为 `JSONB`，补充索引）。
- 配置文件格式由 `config.conf`（ini）改为 `config.toml`。
- ffmpeg/ffprobe 全部异步执行 + 硬超时 + 信号量限流。
- 播放默认直发源文件，不再用 ffprobe 预判浏览器兼容性；失败上报触发兜底转码。
- 扫描落库与校验解耦，校验进度可轮询。

## 回滚

如需回滚到 Python 版本：`git checkout V1`，或将部署镜像 tag 切回 `V1` 对应的历史构建。
