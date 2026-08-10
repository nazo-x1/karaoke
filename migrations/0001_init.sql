-- V2 初始 schema。相较 V1 (aerich/Tortoise 生成)：
--   * can_queue / is_playable 统一为 BOOLEAN（V1 后期已在 Postgres 路径修正，这里延续）
--   * audio_layout 由 TEXT(JSON字符串) 改为 JSONB，可查询、类型安全
--   * 新增 song.display_name 的 trigram 索引，加速曲库模糊搜索（对应 Python `display_name__contains`）
--   * 新增 history.is_sing 索引，加速待播/正在唱查询；新增 history.times 索引配合“常点”排序
--   * history.id 显式声明为 song.id 的外键（一比一关系），ON DELETE CASCADE 删歌自动清队列记录

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE song (
    id              BIGSERIAL PRIMARY KEY,
    display_name    VARCHAR(256) NOT NULL,
    source_path     VARCHAR(1024) NOT NULL,
    source_origin   VARCHAR(16) NOT NULL,
    source_rel      VARCHAR(512),
    media_kind      VARCHAR(16) NOT NULL DEFAULT 'video',
    playback_mode   VARCHAR(16) NOT NULL DEFAULT 'plain',
    playback_source VARCHAR(16),
    can_queue       BOOLEAN,
    is_playable     BOOLEAN NOT NULL DEFAULT FALSE,
    scan_root       VARCHAR(1024),
    audio_layout    JSONB,
    create_time     TIMESTAMPTZ NOT NULL DEFAULT now(),
    update_time     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_song_source_path UNIQUE (source_path)
);

CREATE INDEX idx_song_display_name_trgm ON song USING gin (display_name gin_trgm_ops);

CREATE TABLE history (
    id          BIGINT PRIMARY KEY REFERENCES song (id) ON DELETE CASCADE,
    name        VARCHAR(256) NOT NULL,
    times       INT NOT NULL DEFAULT 0,
    is_sing     INT NOT NULL DEFAULT 0,
    is_top      INT NOT NULL DEFAULT 0,
    create_time TIMESTAMPTZ NOT NULL DEFAULT now(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_history_update_time ON history (update_time);
CREATE INDEX idx_history_is_sing ON history (is_sing);
CREATE INDEX idx_history_times ON history (times DESC);
