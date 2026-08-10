//! 曲库服务：上传/扫描导入/列表/删除。对应 Python `karaoke/services/library_service.py`。

use crate::dto::{db_error_message, ApiResult};
use crate::mappers::song_item;
use karaoke_infra::media::MediaSettings;
use karaoke_infra::repositories::song_repo::PAGE_SIZE;
use karaoke_infra::repositories::SongRepository;
use karaoke_infra::scanner::{scan_root, ScanOptions};
use karaoke_infra::PlaybackResolver;
use karaoke_jobs::PrepareTaskManager;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct LibraryService {
    pub songs: SongRepository,
    pub resolver: PlaybackResolver,
    pub prepare: Arc<PrepareTaskManager>,
    pub media: MediaSettings,
    pub keep_path: PathBuf,
    pub scan_video_exts: HashSet<String>,
    pub skip_dir_names: HashSet<String>,
    pub default_duplicate_policy: String,
    pub ffprobe_on_import: bool,
}

fn safe_filename(filename: &str) -> String {
    let decoded = urlencoding::decode(filename)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| filename.to_string());
    let base = Path::new(decoded.trim())
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    base.replace('\0', "")
}

fn stem(filename: &str) -> String {
    match filename.rfind('.') {
        Some(0) | None => filename.to_string(),
        Some(idx) => filename[..idx].to_string(),
    }
}

impl LibraryService {
    pub async fn upload_file(
        &self,
        filename: &str,
        duplicate_policy: Option<&str>,
        bytes: Vec<u8>,
    ) -> ApiResult {
        if filename.trim().is_empty() {
            return ApiResult::fail("未选择文件");
        }
        let filename = safe_filename(filename);
        let ext = karaoke_domain::file_ext(&filename);
        if !self.scan_video_exts.contains(&ext) {
            return ApiResult::fail_with_data(format!("不支持的格式: {ext}"), &filename);
        }

        let policy = duplicate_policy
            .unwrap_or(&self.default_duplicate_policy)
            .to_string();
        let display_base = stem(&filename);
        let mut dest_path = self.keep_path.join(&filename);

        match self
            .save_upload(&filename, &display_base, &mut dest_path, &policy, bytes)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                tracing::error!("upload {filename} failed: {e:?}");
                ApiResult::fail_with_data(format!("{filename} 上传失败：{e}"), &filename)
            }
        }
    }

    async fn save_upload(
        &self,
        filename: &str,
        display_base: &str,
        dest_path: &mut PathBuf,
        policy: &str,
        bytes: Vec<u8>,
    ) -> anyhow::Result<ApiResult> {
        let existing = self
            .songs
            .find_by_source_path(&dest_path.to_string_lossy())
            .await?;
        let name_conflict = self.songs.find_by_display_name(display_base).await?;

        if existing.is_some() && policy == "skip" {
            return Ok(ApiResult::ok_msg_data(
                format!("{filename} 已存在，已跳过"),
                display_base,
            ));
        }

        let mut display_name = display_base.to_string();
        if let Some(conflict) = &name_conflict {
            if conflict.source_path != dest_path.to_string_lossy() {
                if policy == "skip" {
                    return Ok(ApiResult::ok_msg_data(
                        format!("{display_name} 已存在，已跳过"),
                        display_base,
                    ));
                }
                if policy == "rename" {
                    let used = self.songs.all_display_names().await?;
                    let mut index = 2;
                    while used.contains(&display_name) {
                        display_name = format!("{display_base} ({index})");
                        index += 1;
                    }
                    let ext_part = Path::new(filename)
                        .extension()
                        .map(|e| e.to_string_lossy().to_string());
                    let new_name = match &ext_part {
                        Some(e) => format!("{display_name}.{e}"),
                        None => display_name.clone(),
                    };
                    *dest_path = self.keep_path.join(new_name);
                }
            }
        }

        tokio::fs::write(&dest_path, &bytes).await?;

        let is_playable = if self.ffprobe_on_import {
            karaoke_infra::media::probe_video_playable(&self.media, &dest_path.to_string_lossy())
                .await
        } else {
            true
        };

        let song = if let Some(existing) = existing {
            self.songs
                .update_upload_fields(existing.id, &display_name, is_playable, "upload")
                .await?;
            self.songs.get(existing.id).await?
        } else if let Some(conflict) = name_conflict
            .filter(|c| policy == "overwrite" && c.source_path != dest_path.to_string_lossy())
        {
            if conflict.source_origin == "upload" && Path::new(&conflict.source_path).is_file() {
                let _ = tokio::fs::remove_file(&conflict.source_path).await;
            }
            self.songs
                .overwrite_source(
                    conflict.id,
                    &dest_path.to_string_lossy(),
                    &display_name,
                    is_playable,
                    "upload",
                    None,
                )
                .await?;
            self.songs.get(conflict.id).await?
        } else {
            self.songs
                .create(&karaoke_infra::models::NewSong {
                    display_name: display_name.clone(),
                    source_path: dest_path.to_string_lossy().to_string(),
                    source_origin: "upload".to_string(),
                    source_rel: None,
                    media_kind: "video".to_string(),
                    playback_mode: "plain".to_string(),
                    playback_source: Some("plain".to_string()),
                    can_queue: Some(is_playable),
                    is_playable,
                    scan_root: None,
                })
                .await?
        };

        if self.ffprobe_on_import {
            let layout =
                karaoke_infra::embedded::probe_layout(&self.media, &song.source_path, "auto").await;
            self.songs.update_audio_layout(song.id, &layout).await?;
        }
        let song = self.songs.get(song.id).await?;
        let profile = self.resolver.resolve(&song).await;
        self.songs
            .update_playback_meta(
                song.id,
                karaoke_domain::playback::effective_mode(&profile, &song.playback_mode).as_str(),
                Some(profile.playback_source.as_str()),
                profile.can_queue,
            )
            .await?;

        tracing::info!("{filename} 上传成功");
        Ok(ApiResult::ok_msg_data(
            format!("{filename} 上传成功"),
            &song.display_name,
        ))
    }

    pub async fn get_list(&self, q: &str, page: i64, page_size: i64) -> ApiResult {
        let size = if page_size > 0 { page_size } else { PAGE_SIZE };
        let (songs, total) = match self.songs.list_page(q, page, size).await {
            Ok(v) => v,
            Err(e) => return ApiResult::fail(db_error_message(&e, "获取曲库列表失败")),
        };
        let active = self.prepare.active_tasks();
        let items: Vec<_> = songs
            .iter()
            .map(|s| song_item(s, None, active.get(&s.id).cloned()))
            .collect();
        ApiResult::ok_with_data(items).with_pagination(total, page.max(1), size)
    }

    pub async fn delete_song(&self, song_id: i64, delete_disk: bool) -> ApiResult {
        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            Ok(None) => return ApiResult::not_found("歌曲"),
            Err(e) => return ApiResult::fail(db_error_message(&e, "删除歌曲失败")),
        };
        if delete_disk && song.source_origin == "upload" && Path::new(&song.source_path).is_file() {
            if let Err(e) = tokio::fs::remove_file(&song.source_path).await {
                tracing::warn!("delete source file failed {}: {e}", song.source_path);
            }
        }
        if let Err(e) = self.songs.delete(song_id).await {
            return ApiResult::fail(db_error_message(&e, "删除歌曲失败"));
        }
        ApiResult::ok_msg(format!("{} 删除成功", song.display_name))
    }

    pub async fn run_scan(
        &self,
        root: &str,
        duplicate_policy: Option<&str>,
        validate: Option<bool>,
    ) -> ApiResult {
        if root.trim().is_empty() {
            return ApiResult::fail("请指定扫描根路径");
        }
        let options = ScanOptions {
            duplicate_policy: duplicate_policy
                .unwrap_or(&self.default_duplicate_policy)
                .to_string(),
            validate: validate.unwrap_or(self.ffprobe_on_import),
            dry_run: false,
        };
        match scan_root(
            &self.media,
            &self.songs,
            root,
            &self.scan_video_exts,
            &self.skip_dir_names,
            options,
        )
        .await
        {
            Ok(stats) => ApiResult::ok_msg_data("扫描完成", stats),
            Err(karaoke_infra::scanner::ScanError::RootNotFound(_)) => {
                ApiResult::fail("扫描路径不存在或不可读")
            }
            Err(karaoke_infra::scanner::ScanError::Db(e)) => {
                ApiResult::fail(db_error_message(&e, "扫描导入失败"))
            }
        }
    }

    pub async fn preview_scan(
        &self,
        root: &str,
        duplicate_policy: Option<&str>,
        validate: Option<bool>,
    ) -> ApiResult {
        if root.trim().is_empty() {
            return ApiResult::fail("请指定扫描根路径");
        }
        let options = ScanOptions {
            duplicate_policy: duplicate_policy
                .unwrap_or(&self.default_duplicate_policy)
                .to_string(),
            validate: validate.unwrap_or(self.ffprobe_on_import),
            dry_run: true,
        };
        match scan_root(
            &self.media,
            &self.songs,
            root,
            &self.scan_video_exts,
            &self.skip_dir_names,
            options,
        )
        .await
        {
            Ok(stats) => ApiResult::ok_msg_data("预览完成", stats),
            Err(karaoke_infra::scanner::ScanError::RootNotFound(_)) => {
                ApiResult::fail("扫描路径不存在或不可读")
            }
            Err(karaoke_infra::scanner::ScanError::Db(e)) => {
                ApiResult::fail(db_error_message(&e, "扫描预览失败"))
            }
        }
    }
}
