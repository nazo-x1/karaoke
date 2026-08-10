//! 播放缓存清理。对应 Python `karaoke/services/cache_service.py`。

use crate::dto::ApiResult;
use karaoke_domain::audio_layout::has_dual_roles;
use karaoke_infra::media::{browser_mp4_cache_path, MediaSettings};
use karaoke_infra::repositories::{HistoryRepository, SongRepository};
use karaoke_jobs::PrepareTaskManager;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct CacheService {
    pub songs: SongRepository,
    pub histories: HistoryRepository,
    pub prepare: Arc<PrepareTaskManager>,
    pub media: MediaSettings,
}

impl CacheService {
    pub async fn clear_play_cache(&self) -> ApiResult {
        let protected_ids = self.protected_song_ids().await;
        let (protected_dirs, protected_files) = self.protected_cache_paths(&protected_ids).await;
        let (removed, skipped) = Self::clear_directory_except(
            &self.media.play_cache_path,
            &protected_dirs,
            &protected_files,
        );
        let _ = std::fs::create_dir_all(&self.media.play_cache_path);

        let mut msg = format!("已清除播放转码缓存（{removed} 项");
        if skipped > 0 {
            msg += &format!("，跳过队列中 {skipped} 项");
        }
        msg += "）";

        let mut ids: Vec<i64> = protected_ids.into_iter().collect();
        ids.sort_unstable();
        ApiResult::ok_msg_data(
            msg,
            serde_json::json!({
                "removed": removed,
                "skipped": skipped,
                "protected_song_ids": ids,
                "path": self.media.play_cache_path.to_string_lossy(),
            }),
        )
    }

    async fn protected_song_ids(&self) -> HashSet<i64> {
        let mut ids: HashSet<i64> = self.prepare.active_tasks().keys().copied().collect();
        if let Ok(pending) = self.histories.list_pending().await {
            ids.extend(pending.iter().map(|h| h.id));
        }
        ids
    }

    async fn protected_cache_paths(
        &self,
        ids: &HashSet<i64>,
    ) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
        let mut dirs = HashSet::new();
        let mut files = HashSet::new();
        if ids.is_empty() {
            return (dirs, files);
        }
        let id_list: Vec<i64> = ids.iter().copied().collect();
        let Ok(song_map) = self.songs.map_by_ids(&id_list).await else {
            return (dirs, files);
        };

        for song in song_map.values() {
            let layout = song.layout();
            if has_dual_roles(layout) && std::path::Path::new(&song.source_path).is_file() {
                if let Some(l) = layout {
                    if let Ok(dir) = karaoke_infra::media::transcode::embedded_cache_dir(
                        &self.media,
                        &song.source_path,
                        l,
                    ) {
                        dirs.insert(dir);
                    }
                }
            }
            if std::path::Path::new(&song.source_path).is_file() {
                if let Ok(p) = browser_mp4_cache_path(&self.media, &song.source_path) {
                    files.insert(p);
                }
            }
        }
        (dirs, files)
    }

    fn clear_directory_except(
        root: &std::path::Path,
        protected_dirs: &HashSet<PathBuf>,
        protected_files: &HashSet<PathBuf>,
    ) -> (i64, i64) {
        if !root.is_dir() {
            return (0, 0);
        }
        let mut removed = 0i64;
        let mut skipped = 0i64;

        let Ok(entries) = std::fs::read_dir(root) else {
            return (0, 0);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name == "embedded" && path.is_dir() {
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    for sub in sub_entries.flatten() {
                        let sub_path = sub.path();
                        if protected_dirs.contains(&sub_path) {
                            skipped += 1;
                            continue;
                        }
                        let result = if sub_path.is_dir() {
                            std::fs::remove_dir_all(&sub_path)
                        } else {
                            std::fs::remove_file(&sub_path)
                        };
                        match result {
                            Ok(()) => removed += 1,
                            Err(e) => {
                                tracing::warn!("remove cache failed {}: {e}", sub_path.display())
                            }
                        }
                    }
                }
                continue;
            }

            if protected_files.contains(&path) {
                skipped += 1;
                continue;
            }

            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match result {
                Ok(()) => removed += 1,
                Err(e) => tracing::warn!("remove cache failed {}: {e}", path.display()),
            }
        }

        (removed, skipped)
    }
}
