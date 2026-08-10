//! 数据库行模型（对应 Python `karaoke/infra/models.py` 的 Tortoise 模型）。

use chrono::{DateTime, Utc};
use karaoke_domain::AudioLayout;
use sqlx::types::Json;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SongRow {
    pub id: i64,
    pub display_name: String,
    pub source_path: String,
    pub source_origin: String,
    pub source_rel: Option<String>,
    pub media_kind: String,
    pub playback_mode: String,
    pub playback_source: Option<String>,
    pub can_queue: Option<bool>,
    pub is_playable: bool,
    pub scan_root: Option<String>,
    pub audio_layout: Option<Json<AudioLayout>>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl SongRow {
    pub fn layout(&self) -> Option<&AudioLayout> {
        self.audio_layout.as_ref().map(|j| &j.0)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HistoryRow {
    pub id: i64,
    pub name: String,
    pub times: i32,
    pub is_sing: i32,
    pub is_top: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

/// 新建歌曲所需字段（对应 scanner/upload 场景的 `Song(...)` 构造）。
#[derive(Debug, Clone, Default)]
pub struct NewSong {
    pub display_name: String,
    pub source_path: String,
    pub source_origin: String,
    pub source_rel: Option<String>,
    pub media_kind: String,
    pub playback_mode: String,
    pub playback_source: Option<String>,
    pub can_queue: Option<bool>,
    pub is_playable: bool,
    pub scan_root: Option<String>,
}
