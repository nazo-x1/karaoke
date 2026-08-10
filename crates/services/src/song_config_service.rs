//! 歌曲配置服务：详情/编辑/播放能力检测/预生成缓存。
//! 对应 Python `karaoke/services/song_config_service.py`。

use crate::dto::{db_error_message, ApiResult};
use crate::mappers::{playback_detail, song_item};
use karaoke_domain::audio_layout::{merge_manual_roles, TrackRole};
use karaoke_domain::playback::effective_mode;
use karaoke_infra::embedded::probe_layout;
use karaoke_infra::repositories::{HistoryRepository, SongRepository};
use karaoke_infra::PlaybackResolver;
use karaoke_jobs::PrepareTaskManager;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct SongConfigService {
    pub songs: SongRepository,
    pub histories: HistoryRepository,
    pub resolver: PlaybackResolver,
    pub prepare: Arc<PrepareTaskManager>,
}

impl SongConfigService {
    pub async fn get_detail(&self, song_id: i64) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::fail("歌曲不存在"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "获取歌曲详情失败")),
        };
        let profile = self.resolver.resolve(&song).await;
        let override_status = self.resolver.override_file_status(&song.display_name).await;
        let detail = playback_detail(&song, &profile, override_status);
        let mut value = serde_json::to_value(detail).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut value {
            map.insert("id".to_string(), serde_json::json!(song.id));
            map.insert(
                "display_name".to_string(),
                serde_json::json!(song.display_name),
            );
            map.insert(
                "source_path".to_string(),
                serde_json::json!(song.source_path),
            );
            map.insert(
                "source_origin".to_string(),
                serde_json::json!(song.source_origin),
            );
            map.insert("source_rel".to_string(), serde_json::json!(song.source_rel));
            map.insert(
                "is_playable".to_string(),
                serde_json::json!(song.is_playable),
            );
        }
        ApiResult::ok_with_data(value)
    }

    pub async fn patch(&self, song_id: i64, body: &Value) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::fail("歌曲不存在"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "更新歌曲失败")),
        };

        if let Some(name) = body.get("display_name").and_then(Value::as_str) {
            let trimmed: String = name.trim().chars().take(256).collect();
            if !trimmed.is_empty() {
                if let Err(e) = self.songs.update_display_name(song_id, &trimmed).await {
                    return ApiResult::fail(db_error_message(&e, "更新歌曲失败"));
                }
                for h in self
                    .histories
                    .list_for_song(song_id)
                    .await
                    .unwrap_or_default()
                {
                    let _ = self.histories.update_name(h.id, &trimmed).await;
                }
            }
        }

        if let Some(tracks) = body.get("audio_tracks").and_then(Value::as_array) {
            let updates: Vec<(i32, TrackRole)> = tracks
                .iter()
                .filter_map(|item| {
                    let index = item.get("index").and_then(Value::as_i64)? as i32;
                    let role = item
                        .get("role")
                        .and_then(Value::as_str)
                        .and_then(TrackRole::parse)?;
                    Some((index, role))
                })
                .collect();
            let merged = merge_manual_roles(song.layout(), &updates);
            if let Err(e) = self.songs.update_audio_layout(song_id, &merged).await {
                return ApiResult::fail(db_error_message(&e, "更新歌曲失败"));
            }
        }

        let song = match self.songs.get(song_id).await {
            Ok(s) => s,
            Err(e) => return ApiResult::fail(db_error_message(&e, "更新歌曲失败")),
        };
        let profile = self.resolver.resolve(&song).await;
        if let Err(e) = self
            .songs
            .update_playback_meta(
                song_id,
                effective_mode(&profile, &song.playback_mode).as_str(),
                Some(profile.playback_source.as_str()),
                profile.can_queue,
            )
            .await
        {
            tracing::warn!("persist playback meta failed for song {song_id}: {e}");
        }
        let song = self.songs.get(song_id).await.unwrap_or(song);
        ApiResult::ok_msg_data("更新成功", song_item(&song, Some(&profile), None))
    }

    pub async fn detect_playback(&self, song_id: i64) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::fail("歌曲不存在"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "检测播放能力失败")),
        };

        if !self.resolver.override_complete(&song.display_name).await {
            let layout = probe_layout(&self.resolver.media, &song.source_path, "auto").await;
            if let Err(e) = self.songs.update_audio_layout(song_id, &layout).await {
                tracing::warn!("persist audio_layout failed for song {song_id}: {e}");
            }
        }

        let song = self.songs.get(song_id).await.unwrap_or(song);
        let profile = self.resolver.resolve(&song).await;
        if let Err(e) = self
            .songs
            .update_playback_meta(
                song_id,
                effective_mode(&profile, &song.playback_mode).as_str(),
                Some(profile.playback_source.as_str()),
                profile.can_queue,
            )
            .await
        {
            tracing::warn!("persist playback meta failed for song {song_id}: {e}");
        }
        let song = self.songs.get(song_id).await.unwrap_or(song);

        let override_status = self.resolver.override_file_status(&song.display_name).await;
        let detail = playback_detail(&song, &profile, override_status);
        let mut data = serde_json::to_value(&detail).unwrap_or(Value::Null);

        if self.resolver.needs_prepare(&song, &profile).await {
            let prep = self.prepare.schedule(song_id).await;
            if let Value::Object(map) = &mut data {
                map.insert(
                    "prepare".to_string(),
                    serde_json::to_value(prep).unwrap_or(Value::Null),
                );
            }
        }
        ApiResult::ok_msg_data("播放能力检测完成", data)
    }

    pub async fn request_prepare(&self, song_id: i64, wait: bool) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::fail("歌曲不存在"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "预生成缓存失败")),
        };

        if self.resolver.override_complete(&song.display_name).await {
            let profile = self.resolver.resolve(&song).await;
            let override_status = self.resolver.override_file_status(&song.display_name).await;
            let detail = playback_detail(&song, &profile, override_status);
            return ApiResult::ok_msg_data("已有 __override__ 三件套，无需预生成内嵌缓存", detail);
        }

        let mut prep = self.prepare.schedule(song_id).await;
        if wait {
            prep = self
                .prepare
                .wait_until_ready(song_id, Duration::from_secs(3600))
                .await;
        }

        let song = self.songs.get(song_id).await.unwrap_or(song);
        let profile = self.resolver.resolve(&song).await;
        if let Err(e) = self
            .songs
            .update_playback_meta(
                song_id,
                effective_mode(&profile, &song.playback_mode).as_str(),
                Some(profile.playback_source.as_str()),
                profile.can_queue,
            )
            .await
        {
            tracing::warn!("persist playback meta failed for song {song_id}: {e}");
        }

        let override_status = self.resolver.override_file_status(&song.display_name).await;
        let detail = playback_detail(&song, &profile, override_status);
        let mut data = serde_json::to_value(&detail).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut data {
            map.insert(
                "prepare".to_string(),
                serde_json::to_value(&prep).unwrap_or(Value::Null),
            );
            map.insert("cache_ready".to_string(), serde_json::json!(prep.ready));
        }

        if prep.ready {
            ApiResult::ok_msg_data("缓存已就绪", data)
        } else if prep.status == "pending" || prep.status == "running" {
            ApiResult::ok_msg_data("正在后台生成缓存", data)
        } else {
            let msg = prep
                .error
                .clone()
                .unwrap_or_else(|| "缓存生成失败".to_string());
            ApiResult::fail_with_data(msg, data)
        }
    }
}
