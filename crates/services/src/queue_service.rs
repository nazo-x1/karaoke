//! 点歌队列服务。对应 Python `karaoke/services/queue_service.py`。

use crate::dto::{db_error_message, ApiResult};
use crate::mappers::history_item;
use karaoke_domain::playback::effective_mode;
use karaoke_domain::queue_policy::QueueState;
use karaoke_events::EventBus;
use karaoke_infra::repositories::history_repo::PAGE_SIZE;
use karaoke_infra::repositories::{HistoryRepository, SongRepository};
use karaoke_infra::PlaybackResolver;
use karaoke_jobs::PrepareTaskManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct QueueService {
    pub songs: SongRepository,
    pub histories: HistoryRepository,
    pub resolver: PlaybackResolver,
    pub prepare: Arc<PrepareTaskManager>,
    pub events: EventBus,
}

impl QueueService {
    pub async fn init_on_startup(&self) {
        match self.histories.reset_stale_singing().await {
            Ok(n) if n > 0 => tracing::info!("reset {n} stale singing rows on startup"),
            Ok(_) => {}
            Err(e) => tracing::warn!("reset_stale_singing failed: {e}"),
        }
    }

    async fn build_list(
        &self,
        histories: &[karaoke_infra::models::HistoryRow],
    ) -> Vec<crate::mappers::HistoryItem> {
        if histories.is_empty() {
            return vec![];
        }
        let ids: Vec<i64> = histories.iter().map(|h| h.id).collect();
        let song_map = self.songs.map_by_ids(&ids).await.unwrap_or_default();
        histories
            .iter()
            .map(|h| history_item(h, song_map.get(&h.id)))
            .collect()
    }

    pub async fn enqueue(&self, song_id: i64) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::not_found("歌曲"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "点歌失败")),
        };

        let profile = self.resolver.resolve(&song).await;
        if !profile.can_queue {
            let msg = if !song.is_playable || !std::path::Path::new(&song.source_path).is_file() {
                "源视频不可播放或不存在"
            } else {
                "增强资源不完整且源视频不可用"
            };
            return ApiResult::fail(msg);
        }

        if self.resolver.needs_prepare(&song, &profile).await {
            let prep_status = self.prepare.status(song_id).await;
            if !prep_status.ready {
                let prep = self.prepare.schedule(song_id).await;
                return ApiResult::fail_with_data(
                    "播放资源正在准备中，请耐心等待",
                    serde_json::json!({ "prepare": prep }),
                );
            }
        }

        match self.histories.get_optional(song_id).await {
            Ok(Some(history)) => {
                if history.is_sing == QueueState::Sung.to_db() {
                    if let Err(e) = self.histories.set_pending(song_id).await {
                        return ApiResult::fail(db_error_message(&e, "点歌失败"));
                    }
                }
            }
            Ok(None) => {
                if let Err(e) = self
                    .histories
                    .create(song_id, &song.display_name, QueueState::Pending.to_db(), 0)
                    .await
                {
                    return ApiResult::fail(db_error_message(&e, "点歌失败"));
                }
            }
            Err(e) => return ApiResult::fail(db_error_message(&e, "点歌失败")),
        }

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

        self.events.publish_queue_changed();
        let display = if song.display_name.trim().is_empty() {
            format!("歌曲 #{song_id}")
        } else {
            song.display_name.clone()
        };
        ApiResult::ok_msg_data(
            format!("{display} 点歌成功"),
            serde_json::json!({"playback_mode": profile.mode.as_str()}),
        )
    }

    pub async fn list_pending(&self) -> ApiResult {
        let histories = match self.histories.list_pending().await {
            Ok(h) => h,
            Err(e) => return ApiResult::fail(db_error_message(&e, "获取队列失败")),
        };
        let total = histories.len() as i64;
        let data = self.build_list(&histories).await;
        let mut result = ApiResult::ok_with_data(data).with_pagination(total, 1, PAGE_SIZE);
        result.total_page = if total > 0 { 1 } else { 0 };
        result
    }

    pub async fn list_history(&self, page: i64) -> ApiResult {
        let (histories, total) = match self.histories.list_history_page(page).await {
            Ok(v) => v,
            Err(e) => return ApiResult::fail(db_error_message(&e, "获取队列失败")),
        };
        let data = self.build_list(&histories).await;
        ApiResult::ok_with_data(data).with_pagination(total, page.max(1), PAGE_SIZE)
    }

    pub async fn list_usually(&self, page: i64) -> ApiResult {
        let (histories, total) = match self.histories.list_usually_page(page).await {
            Ok(v) => v,
            Err(e) => return ApiResult::fail(db_error_message(&e, "获取队列失败")),
        };
        let data = self.build_list(&histories).await;
        ApiResult::ok_with_data(data).with_pagination(total, page.max(1), PAGE_SIZE)
    }

    pub async fn set_top(&self, song_id: i64) -> ApiResult {
        let history = match self.histories.get_optional(song_id).await {
            Ok(Some(h)) => h,
            Ok(None) => return ApiResult::fail("歌曲不在队列中"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "置顶失败")),
        };
        if let Err(e) = self.histories.set_top(song_id).await {
            return ApiResult::fail(db_error_message(&e, "置顶失败"));
        }
        self.events.publish_queue_changed();
        ApiResult::ok_msg(format!("{} 置顶成功", history.name))
    }

    pub async fn remove_if_exists(&self, song_id: i64) {
        if let Ok(Some(_)) = self.histories.get_optional(song_id).await {
            let _ = self.histories.delete(song_id).await;
        }
    }

    pub async fn remove(&self, song_id: i64) -> ApiResult {
        let history = match self.histories.get_optional(song_id).await {
            Ok(Some(h)) => h,
            Ok(None) => return ApiResult::fail("歌曲不在队列中"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "移除队列项失败")),
        };
        if let Err(e) = self.histories.delete(song_id).await {
            return ApiResult::fail(db_error_message(&e, "移除队列项失败"));
        }
        self.events.publish_queue_changed();
        ApiResult::ok_msg(format!("{} 播放记录删除成功", history.name))
    }
}
