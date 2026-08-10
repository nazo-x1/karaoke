//! 播放服务：播放配置/流媒体/会话状态。对应 Python `karaoke/services/playback_service.py`。

use crate::dto::{db_error_message, ApiResult};
use crate::mappers::playback_api;
use karaoke_events::EventBus;
use karaoke_infra::repositories::{HistoryRepository, SongRepository};
use karaoke_infra::PlaybackResolver;
use karaoke_jobs::PrepareTaskManager;
use std::path::PathBuf;
use std::sync::Arc;

/// 流媒体查询结果；具体 HTTP 响应（Range/206 等）由 `karaoke-api` 组装。
pub enum StreamOutcome {
    NotFound,
    Invalid(String),
    CacheNotReady(karaoke_jobs::PrepareStatus),
    File { path: PathBuf, media_type: String },
}

#[derive(Clone)]
pub struct PlaybackService {
    pub songs: SongRepository,
    pub histories: HistoryRepository,
    pub resolver: PlaybackResolver,
    pub prepare: Arc<PrepareTaskManager>,
    pub events: EventBus,
}

impl PlaybackService {
    pub async fn get_profile(&self, song_id: i64) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::fail("歌曲不存在"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "获取播放配置失败")),
        };
        let profile = self.resolver.resolve(&song).await;
        let prep = self.prepare.status(song_id).await;
        ApiResult::ok_with_data(playback_api(&song, &profile, prep))
    }

    pub async fn get_prepare(&self, song_id: i64) -> ApiResult {
        match self.songs.get_optional(song_id).await {
            Ok(Some(_)) => ApiResult::ok_with_data(self.prepare.status(song_id).await),
            Ok(None) => ApiResult::fail("歌曲不存在"),
            Err(e) => ApiResult::fail(db_error_message(&e, "获取准备状态失败")),
        }
    }

    pub async fn schedule_prepare(&self, song_id: i64) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::fail("歌曲不存在"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "准备播放资源失败")),
        };
        let prep = self.prepare.schedule(song_id).await;
        let profile = self.resolver.resolve(&song).await;
        if let Err(e) = self
            .songs
            .update_playback_meta(
                song_id,
                karaoke_domain::playback::effective_mode(&profile, &song.playback_mode).as_str(),
                Some(profile.playback_source.as_str()),
                profile.can_queue,
            )
            .await
        {
            tracing::warn!("persist playback meta failed for song {song_id}: {e}");
        }

        if prep.ready {
            ApiResult::ok_msg_data("播放资源已就绪", prep)
        } else if prep.status == "pending" || prep.status == "running" {
            ApiResult::ok_msg_data("正在后台准备播放资源", prep)
        } else if prep.status == "failed" {
            let msg = prep
                .error
                .clone()
                .unwrap_or_else(|| "播放资源准备失败".to_string());
            ApiResult::fail_with_data(msg, prep)
        } else {
            ApiResult::ok_msg_data("等待准备播放资源", prep)
        }
    }

    pub async fn stream(&self, song_id: i64, kind: &str) -> StreamOutcome {
        if !matches!(kind, "video" | "vocals" | "accompaniment") {
            return StreamOutcome::Invalid("无效的流类型".to_string());
        }
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return StreamOutcome::NotFound,
            Err(e) => return StreamOutcome::Invalid(db_error_message(&e, "获取播放流失败")),
        };

        match self.resolver.stream_path_for_kind(&song, kind).await {
            Some((path, media_type)) => StreamOutcome::File { path, media_type },
            None => {
                let profile = self.resolver.resolve(&song).await;
                if profile.playback_source == karaoke_domain::playback::PlaybackSource::Embedded
                    && !profile.embedded_cache_ready
                {
                    StreamOutcome::CacheNotReady(self.prepare.status(song_id).await)
                } else {
                    StreamOutcome::Invalid("播放文件不存在或未就绪".to_string())
                }
            }
        }
    }

    pub async fn mark_singing(&self, song_id: i64) -> ApiResult {
        let history = match self.histories.get_optional(song_id).await {
            Ok(Some(h)) => h,
            Ok(None) => return ApiResult::fail("歌曲不在队列中"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "标记正在播放失败")),
        };
        if let Err(e) = self.histories.mark_singing(song_id).await {
            return ApiResult::fail(db_error_message(&e, "标记正在播放失败"));
        }
        self.events.publish_queue_changed();
        ApiResult::ok_msg(format!("{} 设置-1成功", history.name))
    }

    pub async fn mark_finished(&self, song_id: i64) -> ApiResult {
        let history = match self.histories.get_optional(song_id).await {
            Ok(Some(h)) => h,
            Ok(None) => return ApiResult::fail("歌曲不在队列中"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "标记已唱完失败")),
        };
        if let Err(e) = self.histories.mark_finished(song_id).await {
            return ApiResult::fail(db_error_message(&e, "标记已唱完失败"));
        }
        self.events.publish_queue_changed();
        ApiResult::ok_msg(format!("{} 设置1成功", history.name))
    }

    pub async fn skip_if_not_ready(&self, song_id: i64) -> ApiResult {
        let history = match self.histories.get_optional(song_id).await {
            Ok(Some(h)) => h,
            Ok(None) => return ApiResult::fail("歌曲不在队列中"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "跳过未就绪歌曲失败")),
        };
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::fail("歌曲不在队列中"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "跳过未就绪歌曲失败")),
        };
        let profile = self.resolver.resolve(&song).await;
        let prep_status = self.prepare.status(song_id).await;
        let stream_ready = prep_status.ready;

        if stream_ready && profile.can_queue {
            return ApiResult::fail("歌曲已就绪，无需跳过");
        }

        let mut prep = None;
        if self.resolver.needs_prepare(&song, &profile).await && !stream_ready {
            prep = Some(self.prepare.schedule(song_id).await);
        }

        if let Err(e) = self.histories.mark_finished(song_id).await {
            return ApiResult::fail(db_error_message(&e, "标记已唱完失败"));
        }
        self.events.publish_queue_changed();

        let msg = format!("{} 未就绪，已跳过", history.name);
        match prep {
            Some(p) => ApiResult::ok_msg_data(msg, serde_json::json!({"prepare": p})),
            None => ApiResult::ok_msg(msg),
        }
    }

    pub fn send_command(&self, code: i32, data: serde_json::Value) -> ApiResult {
        self.events.publish(code, data);
        ApiResult::ok()
    }
}
