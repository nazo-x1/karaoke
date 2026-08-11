//! JSON DTO 构建。对应 Python `karaoke/dto/mappers.py`。

use chrono::{DateTime, Utc};
use karaoke_domain::audio_layout::layout_summary;
use karaoke_domain::playback::{effective_mode, PlaybackProfile, SongMeta};
use karaoke_infra::models::{HistoryRow, SongRow};
use serde::Serialize;

pub fn fmt_time(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct SongItem {
    pub id: i64,
    pub display_name: String,
    pub source_origin: String,
    pub playback_mode: String,
    pub playback_source: String,
    pub can_queue: bool,
    pub is_playable: bool,
    pub source_path: String,
    pub create_time: String,
    pub update_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare: Option<karaoke_jobs::PrepareStatus>,
}

pub fn song_item(
    song: &SongRow,
    profile: Option<&PlaybackProfile>,
    prepare: Option<karaoke_jobs::PrepareStatus>,
) -> SongItem {
    let (mode, source, can_queue) = match profile {
        Some(p) => (
            effective_mode(p, &song.playback_mode),
            p.playback_source.as_str().to_string(),
            p.can_queue,
        ),
        None => {
            let meta = SongMeta {
                playback_mode: Some(song.playback_mode.clone()),
                playback_source: song.playback_source.clone(),
                can_queue: song.can_queue,
                is_playable: song.is_playable,
                audio_layout: song.layout().cloned(),
            };
            karaoke_domain::playback::list_meta_from_song(&meta)
        }
    };
    SongItem {
        id: song.id,
        display_name: song.display_name.clone(),
        source_origin: song.source_origin.clone(),
        playback_mode: mode,
        playback_source: source,
        can_queue,
        is_playable: song.is_playable,
        source_path: song.source_path.clone(),
        create_time: fmt_time(&song.create_time),
        update_time: fmt_time(&song.update_time),
        prepare,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackDetail {
    pub playback_mode: String,
    pub playback_source: String,
    pub can_queue: bool,
    pub embedded_cache_ready: bool,
    pub audio_layout: karaoke_domain::audio_layout::LayoutSummary,
}

pub fn playback_detail(song: &SongRow, profile: &PlaybackProfile) -> PlaybackDetail {
    PlaybackDetail {
        playback_mode: effective_mode(profile, &song.playback_mode),
        playback_source: profile.playback_source.as_str().to_string(),
        can_queue: profile.can_queue,
        embedded_cache_ready: profile.embedded_cache_ready,
        audio_layout: layout_summary(song.layout()),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamsAvailable {
    pub video: bool,
    pub vocals: bool,
    pub accompaniment: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackApi {
    pub id: i64,
    pub display_name: String,
    pub mode: String,
    pub playback_source: String,
    pub can_queue: bool,
    pub ready_to_stream: bool,
    pub prepare: karaoke_jobs::PrepareStatus,
    pub video_mime: Option<String>,
    pub video_ext: Option<String>,
    pub embedded_cache_ready: bool,
    pub streams: StreamsAvailable,
}

pub fn playback_api(
    song: &SongRow,
    profile: &PlaybackProfile,
    prepare: karaoke_jobs::PrepareStatus,
) -> PlaybackApi {
    let ready = if prepare.ready {
        true
    } else if profile.playback_source == karaoke_domain::playback::PlaybackSource::Embedded {
        profile.embedded_cache_ready
    } else {
        profile.can_queue
    };
    PlaybackApi {
        id: song.id,
        display_name: song.display_name.clone(),
        mode: profile.mode.as_str().to_string(),
        playback_source: profile.playback_source.as_str().to_string(),
        can_queue: profile.can_queue,
        ready_to_stream: ready,
        streams: StreamsAvailable {
            video: profile.video_path.is_some() && ready,
            vocals: profile.vocals_path.is_some() && ready,
            accompaniment: profile.accompaniment_path.is_some() && ready,
        },
        video_mime: profile.video_mime.clone(),
        video_ext: profile.video_ext.clone(),
        embedded_cache_ready: profile.embedded_cache_ready,
        prepare,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: i64,
    pub name: String,
    pub times: i32,
    pub state: String,
    pub is_top: i32,
    pub playback_mode: String,
}

pub fn history_item(history: &HistoryRow, song: Option<&SongRow>) -> HistoryItem {
    HistoryItem {
        id: history.id,
        name: history.name.clone(),
        times: history.times,
        state: karaoke_domain::queue_state_label(history.is_sing).to_string(),
        is_top: history.is_top,
        playback_mode: song
            .map(|s| s.playback_mode.clone())
            .unwrap_or_else(|| "plain".to_string()),
    }
}
