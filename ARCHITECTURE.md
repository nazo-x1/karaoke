# Karaoke 模块化架构

后端按五条业务主线组织：**曲库、歌曲配置、点歌队列、演出播放、资源准备**。

## 目录结构

```
karaoke/
├── api/
│   ├── router.py              # /api/v1 路由聚合
│   └── routes/                # 薄路由层
├── services/                  # 应用服务层（base.py 公共工具）
├── domain/                    # 纯领域逻辑
├── infra/                     # 媒体、扫描、流式、仓储、模型
│   ├── media.py               # ffprobe / ffmpeg / 转码
│   ├── embedded.py            # MKV 内嵌缓存
│   ├── audio_layout.py        # 音轨布局
│   ├── scanner.py             # 目录扫描导入
│   ├── db_schema.py           # SQLite 增量迁移
│   ├── models.py              # Tortoise ORM
│   ├── streaming.py           # 分片流式响应
│   └── repositories/          # 数据访问
├── dto/
│   ├── api_result.py          # 统一 JSON envelope（ApiResult）
│   ├── mappers.py             # 实体 → DTO
│   └── schemas.py             # TypedDict 文档
├── events/bus.py              # SSE 事件总线
└── errors.py                  # 异常 → API 消息
```

## 依赖规则

- `api` → `services` → `domain` / `infra` / `events`
- `domain` 不依赖 `services` / `api`
- `infra` 不依赖 `services`
- 统一响应：`services/*` 返回 `dto.ApiResult`（JSON 字段 `code/msg/data/total/page/totalPage`）
- ORM 模型：`karaoke.infra.models`（Tortoise 注册路径）

## API（/api/v1）

| 域 | 路径 | 说明 |
|----|------|------|
| 曲库 | `POST /library/upload` | 上传视频 |
| 曲库 | `POST /library/scan` | 扫描导入 |
| 曲库 | `GET /library/scan/preview` | 扫描预览 |
| 曲库 | `GET /library/songs` | 歌曲列表 |
| 曲库 | `DELETE /library/songs/{id}` | 删除歌曲 |
| 配置 | `GET/PATCH /songs/{id}` | 详情 / 更新 |
| 配置 | `POST /songs/{id}/detect` | 检测播放能力 |
| 配置 | `POST /songs/{id}/prepare` | 预生成内嵌缓存 |
| 队列 | `POST /queue/songs/{id}` | 点歌 |
| 队列 | `GET /queue` | 待播队列（含 `state`: pending/playing/sung） |
| 队列 | `GET /queue/history?page=` | 已唱历史（分页） |
| 队列 | `GET /queue/usually?page=` | 常点歌曲（分页） |
| 队列 | `POST /queue/songs/{id}/top` | 置顶 |
| 队列 | `DELETE /queue/songs/{id}` | 从队列移除 |
| 播放 | `GET /playback/songs/{id}` | 播放配置 |
| 播放 | `GET /playback/songs/{id}/prepare` | 准备状态 |
| 播放 | `POST /playback/songs/{id}/prepare` | 开始准备 |
| 播放 | `GET /playback/stream/{id}/{kind}` | 流媒体（video/vocals/accompaniment） |
| 播放 | `POST /playback/session/singing/{id}` | 标记正在播放 |
| 播放 | `POST /playback/session/finished/{id}` | 标记已唱完 |
| 事件 | `GET /events` | SSE 订阅（30s 心跳保活） |
| 事件 | `POST /events/command` | 遥控指令（JSON body: `{code, data}`） |

## 页面路由（HTML，非 API）

| 路径 | 页面 |
|------|------|
| `/` | 曲库管理（上传 / 扫描 / 编辑 / 删除） |
| `/song/edit/{id}` | 歌曲编辑 |
| `/sing` | Web 播放屏 |

## 前端结构

```
static/js/
├── core/          # config, http, events
├── domains/       # library, config, queue, playback
└── pages/         # index, client, playing, edit
```

OpenAPI 文档：启动服务后访问 `/docs`。
