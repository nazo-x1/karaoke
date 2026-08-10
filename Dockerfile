# Karaoke 后端 — FastAPI + FFmpeg
# 构建：docker build -t ghcr.io/<owner>/karaoke:latest .
# 国内镜像可选：--build-arg APT_MIRROR=mirrors.tuna.tsinghua.edu.cn ...

ARG PYTHON_IMAGE=python:3.12-slim-bookworm
FROM ${PYTHON_IMAGE}

ARG APT_MIRROR=
ARG PIP_INDEX_URL=
ARG PIP_TRUSTED_HOST=

ENV DEBIAN_FRONTEND=noninteractive \
    PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    KTV_PATH=/KTV \
    PORT=15233 \
    DATABASE_URL=sqlite:///tmp/aerich-build.db

RUN if [ -n "$APT_MIRROR" ]; then \
      sed -i "s|deb.debian.org|${APT_MIRROR}|g" /etc/apt/sources.list.d/debian.sources && \
      sed -i "s|security.debian.org|${APT_MIRROR}/debian-security|g" /etc/apt/sources.list.d/debian.sources; \
    fi \
    && apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY requirements.txt ./
RUN if [ -n "$PIP_INDEX_URL" ]; then \
      pip install --no-cache-dir -i "$PIP_INDEX_URL" --trusted-host "${PIP_TRUSTED_HOST:-pypi.org}" -r requirements.txt; \
    else \
      pip install --no-cache-dir -r requirements.txt; \
    fi

COPY . /app/

RUN mkdir -p "${KTV_PATH}" /tmp /work \
    && sed -i 's/^# host = 0.0.0.0/host = 0.0.0.0/' config.conf \
    && aerich init -t settings.TORTOISE_ORM \
    && aerich init-db \
    && chmod +x /app/docker-entrypoint.sh

EXPOSE 15233

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:15233/')" || exit 1

ENTRYPOINT ["/bin/bash", "/app/docker-entrypoint.sh"]
CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "15233"]
