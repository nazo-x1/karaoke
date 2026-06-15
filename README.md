# karaoke
自建家庭版的 KTV，个人的 Karaoke。

如需了解更多信息，[快点我看看吧](https://blog.ihuster.top/p/987654323.html)。

模块化架构与 `/api/v1` 接口说明见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

## Docker

```bash
docker compose up -d
# 或
docker build -t ghcr.io/nazo-x1/karaoke:latest .
docker run -d -p 15233:15233 -v karaoke-data:/KTV ghcr.io/nazo-x1/karaoke:latest
```

数据目录挂载到 `/KTV`（含 `sqlite3.db` 与媒体库）。

## CI/CD

GitHub Actions 见 [`.github/workflows/ci.yml`](.github/workflows/ci.yml)：

| 触发 | 行为 |
|------|------|
| PR | Python 依赖安装 + `compileall` 校验 |
| push `main`/`master` | 构建并推送镜像至 `ghcr.io/<owner>/karaoke:latest` |
| push tag `v*` | 推送 semver 标签镜像 + GitHub Release |

首次推送后需在仓库 **Packages** 中将镜像可见性设为 public（若需公开拉取）。
