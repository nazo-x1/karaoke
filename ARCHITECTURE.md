# Karaoke 模块化架构

后端按五条业务主线组织：**曲库、歌曲配置、点歌队列、演出播放、资源准备**。

## 目录结构

```
karaoke/
├── api/
│   ├── router.py              # /api/v1 路由聚合
│   └── routes/                # 薄路由层
├── services/                  # 应用服务层
├── domain/                    # 纯领域逻辑
├── infra/                     # 媒体、扫描、流式、仓储
├── events/bus.py              # SSE 事件总线
└── dto/mappers.py             # DTO 组装
```

## 依赖规则

- `api` → `services` → `domain` / `infra` / `events`
- `domain` 不依赖 `services` / `api`
- `infra` 不依赖 `services`

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
| 队列 | `GET /queue` | 待播队列 |
| 队列 | `GET /queue/history` | 已唱历史 |
| 队列 | `GET /queue/usually` | 常点统计 |
| 队列 | `POST /queue/songs/{id}/top` | 置顶 |
| 队列 | `DELETE /queue/songs/{id}` | 从队列移除 |
| 播放 | `GET /playback/songs/{id}` | 播放配置 |
| 播放 | `GET /playback/songs/{id}/prepare-status` | 准备状态 |
| 播放 | `POST /playback/songs/{id}/ensure-ready` | 确保就绪 |
| 播放 | `GET /playback/stream/{id}/{kind}` | 流媒体（video/vocals/accompaniment） |
| 播放 | `POST /playback/session/singing/{id}` | 标记正在播放 |
| 播放 | `POST /playback/session/finished/{id}` | 标记已唱完 |
| 事件 | `GET /events` | SSE 订阅 |
| 事件 | `POST /events/command` | 遥控指令（JSON body: `{code, data}`） |

## 页面路由（HTML，非 API）

| 路径 | 页面 |
|------|------|
| `/` | 曲库 |
| `/song/edit/{id}` | 歌曲编辑 |
| `/sing` | 播放屏 |
| `/song` | 控制台 |

## 前端结构

```
static/js/
├── core/          # config, http, events
├── domains/       # library, config, queue, playback
└── pages/         # index, client, playing, edit
```

## 已废弃

旧 REST API 路径 `/song/*`（如 `GET /song/sing/{id}`、`GET /song/list`）已移除。请统一使用 `/api/v1/*`。

OpenAPI 文档：启动服务后访问 `/docs`。
