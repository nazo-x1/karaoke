# Karaoke V2 后端 — Rust (axum + tokio + sqlx) + FFmpeg
# 构建：docker build -t ghcr.io/<owner>/karaoke:latest .
# 国内镜像可选：--build-arg APT_MIRROR=mirrors.tuna.tsinghua.edu.cn --build-arg CARGO_REGISTRY=tuna

ARG RUST_IMAGE=rust:1-slim-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS builder

ARG CARGO_REGISTRY=

# sqlx 使用 rustls（非系统 OpenSSL），无需额外系统库；rust 官方镜像自带的 gcc 足够编译
# ring/aws-lc-rs 等含 C 代码的依赖，因此构建阶段无需 apt-get。
RUN if [ -n "$CARGO_REGISTRY" ]; then \
      mkdir -p /usr/local/cargo && printf '[source.crates-io]\nreplace-with = "%s"\n[source.%s]\nregistry = "sparse+https://rsproxy.cn/index/"\n' "$CARGO_REGISTRY" "$CARGO_REGISTRY" > /usr/local/cargo/config.toml; \
    fi

WORKDIR /build

# 先只拷贝 manifest 以最大化利用 Docker layer 缓存（依赖不变时无需重新下载/编译）。
COPY Cargo.toml Cargo.lock ./
COPY crates/domain/Cargo.toml crates/domain/Cargo.toml
COPY crates/infra/Cargo.toml crates/infra/Cargo.toml
COPY crates/events/Cargo.toml crates/events/Cargo.toml
COPY crates/jobs/Cargo.toml crates/jobs/Cargo.toml
COPY crates/services/Cargo.toml crates/services/Cargo.toml
COPY crates/api/Cargo.toml crates/api/Cargo.toml
COPY crates/app/Cargo.toml crates/app/Cargo.toml

COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release --locked -p karaoke-app

FROM ${RUNTIME_IMAGE}

ARG APT_MIRROR=

ENV DEBIAN_FRONTEND=noninteractive \
    KTV_PATH=/KTV \
    PORT=15233 \
    CONFIG_PATH=/app/config.toml \
    TEMPLATES_DIR=/app/templates \
    STATIC_DIR=/app/static

RUN if [ -n "$APT_MIRROR" ]; then \
      sed -i "s|deb.debian.org|${APT_MIRROR}|g" /etc/apt/sources.list.d/debian.sources && \
      sed -i "s|security.debian.org|${APT_MIRROR}/debian-security|g" /etc/apt/sources.list.d/debian.sources; \
    fi \
    && apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/release/karaoke-server ./karaoke-server
COPY config.toml ./config.toml
COPY templates ./templates
COPY static ./static

RUN mkdir -p "${KTV_PATH}/__keep__" "${KTV_PATH}/__override__" "${KTV_PATH}/__play_cache__/embedded"

EXPOSE 15233

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD wget -qO- "http://127.0.0.1:${PORT}/api/v1/system/health" || exit 1

ENTRYPOINT ["/app/karaoke-server"]
