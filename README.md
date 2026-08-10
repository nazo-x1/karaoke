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
- `PREPARE_MAX_CONCURRENT`：同时进行的播放资源准备（ffmpeg）任务数

与 V1 的环境变量约定保持一致，`docker-compose.yml` 无需改动拓扑即可切换。

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

## 与 V1 的差异（内部实现，不影响外部契约）

- 数据库 schema 重新设计（`audio_layout` 由 `TEXT` 改为 `JSONB`，补充索引）。
- 配置文件格式由 `config.conf`（ini）改为 `config.toml`。
- ffmpeg/ffprobe 全部异步执行 + 硬超时 + 信号量限流，修复卡死子进程拖垮服务的问题。
- SSE 显式连接数上限 + 死连接自动清理 + `Lagged` 时补发 resync 提示。
- prepare 后台任务状态使用终态 TTL 清理，避免无限增长。

对外路由路径、`ApiResult` envelope 字段名、SSE 事件码语义均保持不变，前端（独立的 `rc/` 仓库、本仓库内
`static/js`）无需改动。

## 回滚

如需回滚到 Python 版本：`git checkout V1`，或将部署镜像 tag 切回 `V1` 对应的历史构建。
