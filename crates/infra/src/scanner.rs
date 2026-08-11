//! 目录扫描导入。对应 Python `karaoke/infra/scanner.py`。

use crate::media::{probe_video_playable, MediaSettings};
use crate::models::NewSong;
use crate::repositories::SongRepository;
use futures::stream::{self, StreamExt};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::info;

const PREVIEW_LIMIT: usize = 200;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("扫描路径不存在或不可读: {0}")]
    RootNotFound(String),
    #[error("已有扫描任务正在运行")]
    Busy,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewItem {
    pub path: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanStats {
    pub added: i64,
    pub skipped: i64,
    pub renamed: i64,
    pub invalid: i64,
    /// 落库后待后台校验的文件数（validate=true 时）。
    #[serde(default)]
    pub pending_validate: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<PreviewItem>,
}

impl ScanStats {
    fn push_preview(&mut self, item: PreviewItem) {
        if self.preview.len() < PREVIEW_LIMIT {
            self.preview.push(item);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanValidateStatus {
    pub running: bool,
    pub total: i64,
    pub done: i64,
    pub invalid: i64,
}

/// 扫描校验后台进度（同一时间通常只有一个扫描校验在跑）。
#[derive(Clone, Default)]
pub struct ScanTaskManager {
    inner: Arc<Mutex<ScanValidateStatus>>,
}

impl ScanTaskManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> ScanValidateStatus {
        self.inner.lock().unwrap().clone()
    }

    fn try_begin(&self, total: i64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.running {
            return false;
        }
        *g = ScanValidateStatus {
            running: true,
            total,
            done: 0,
            invalid: 0,
        };
        true
    }

    fn tick(&self, invalid: bool) {
        let mut g = self.inner.lock().unwrap();
        g.done += 1;
        if invalid {
            g.invalid += 1;
        }
    }

    fn finish(&self) {
        let mut g = self.inner.lock().unwrap();
        g.running = false;
    }
}

pub struct ScanOptions {
    pub duplicate_policy: String,
    /// 同步校验（预览用）；正式扫描应设 false，校验改走后台。
    pub validate: bool,
    pub dry_run: bool,
}

fn walk_video_files(
    root: &Path,
    video_exts: &HashSet<String>,
    skip_dirs: &HashSet<String>,
) -> Vec<(PathBuf, PathBuf)> {
    let mut results = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !skip_dirs.contains(&name) {
                    subdirs.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(dot) = name.rfind('.') {
                if dot > 0 {
                    let ext = name[dot + 1..].to_lowercase();
                    if video_exts.contains(&ext) {
                        results.push((path, dir.clone()));
                    }
                }
            }
        }
        stack.extend(subdirs.into_iter().rev());
    }
    results
}

fn display_base(filename: &str) -> String {
    match filename.rfind('.') {
        Some(0) | None => filename.to_string(),
        Some(idx) => filename[..idx].to_string(),
    }
}

fn make_unique_display_name(
    base: &str,
    source_rel: &str,
    used: &HashSet<String>,
) -> (String, bool) {
    if !used.contains(base) {
        return (base.to_string(), false);
    }
    let rel_suffix = source_rel.trim_matches('/').trim_start_matches("./");
    if !rel_suffix.is_empty() {
        let candidate = format!("{base} ({rel_suffix})");
        if !used.contains(&candidate) {
            return (candidate, true);
        }
    }
    let mut index = 2;
    loop {
        let candidate = format!("{base} ({index})");
        if !used.contains(&candidate) {
            return (candidate, true);
        }
        index += 1;
    }
}

/// 正式扫描结果：统计 + 待后台校验的 source_path 列表。
pub struct ScanPersistResult {
    pub stats: ScanStats,
    pub paths_to_validate: Vec<String>,
}

pub async fn scan_root(
    settings: &MediaSettings,
    songs: &SongRepository,
    root: &str,
    video_exts: &HashSet<String>,
    skip_dir_names: &HashSet<String>,
    options: ScanOptions,
) -> Result<ScanPersistResult, ScanError> {
    let root_path =
        std::fs::canonicalize(root).map_err(|_| ScanError::RootNotFound(root.to_string()))?;
    if !root_path.is_dir() {
        return Err(ScanError::RootNotFound(root.to_string()));
    }
    let root_str = root_path.to_string_lossy().to_string();

    let existing = songs.all_for_scan().await?;
    let mut existing_by_path: HashMap<String, i64> = existing
        .iter()
        .map(|s| (s.source_path.clone(), s.id))
        .collect();
    let mut name_to_path: HashMap<String, String> = existing
        .iter()
        .map(|s| (s.display_name.clone(), s.source_path.clone()))
        .collect();
    let mut used_names: HashSet<String> = name_to_path.keys().cloned().collect();

    let files = {
        let root_path = root_path.clone();
        let video_exts = video_exts.clone();
        let skip_dir_names = skip_dir_names.clone();
        tokio::task::spawn_blocking(move || {
            walk_video_files(&root_path, &video_exts, &skip_dir_names)
        })
        .await
        .unwrap_or_default()
    };
    info!("scan collect: root={root_str} files={}", files.len());

    // 预览且需要校验：先并发探测，再走决策逻辑。
    let mut playable_map: HashMap<String, bool> = HashMap::new();
    if options.validate && options.dry_run {
        let concurrency = settings.scan_validate_concurrency;
        let paths: Vec<String> = files
            .iter()
            .map(|(p, _)| p.to_string_lossy().to_string())
            .collect();
        let results: Vec<(String, bool)> = stream::iter(paths)
            .map(|path| {
                let settings = settings.clone();
                async move {
                    let ok = probe_video_playable(&settings, &path).await;
                    (path, ok)
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;
        playable_map.extend(results);
    }

    let mut stats = ScanStats::default();
    let mut to_create: Vec<NewSong> = Vec::new();
    let mut overwrite_updates: Vec<(i64, NewSong)> = Vec::new();
    let mut paths_to_validate: Vec<String> = Vec::new();

    for (abs_path, dirpath) in files {
        let abs_path_str = abs_path.to_string_lossy().to_string();
        let filename = abs_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let base_name = display_base(&filename);
        let mut source_rel = dirpath
            .strip_prefix(&root_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if source_rel == "." {
            source_rel = String::new();
        }

        // 正式扫描：暂标可播，校验后台补齐；预览：同步用探测结果。
        let is_playable = if options.validate && options.dry_run {
            let ok = playable_map.get(&abs_path_str).copied().unwrap_or(false);
            if !ok {
                stats.invalid += 1;
                stats.push_preview(PreviewItem {
                    path: abs_path_str.clone(),
                    action: "invalid".to_string(),
                    display_name: None,
                });
                continue;
            }
            true
        } else {
            true
        };

        if let Some(&existing_id) = existing_by_path.get(&abs_path_str) {
            if options.duplicate_policy == "overwrite" {
                if options.dry_run {
                    stats.push_preview(PreviewItem {
                        path: abs_path_str.clone(),
                        action: "update".to_string(),
                        display_name: None,
                    });
                } else {
                    overwrite_updates.push((
                        existing_id,
                        NewSong {
                            display_name: base_name.clone(),
                            source_path: abs_path_str.clone(),
                            source_origin: "scan".to_string(),
                            source_rel: (!source_rel.is_empty()).then(|| source_rel.clone()),
                            media_kind: "video".to_string(),
                            playback_mode: "plain".to_string(),
                            playback_source: Some("plain".to_string()),
                            can_queue: Some(is_playable),
                            is_playable,
                            scan_root: Some(root_str.clone()),
                        },
                    ));
                    if options.validate {
                        paths_to_validate.push(abs_path_str.clone());
                    }
                }
                stats.added += 1;
            } else {
                stats.skipped += 1;
                if options.dry_run {
                    stats.push_preview(PreviewItem {
                        path: abs_path_str.clone(),
                        action: "skip".to_string(),
                        display_name: None,
                    });
                }
            }
            continue;
        }

        let mut display_name = base_name.clone();
        let mut renamed = false;
        if let Some(existing_path) = name_to_path.get(&display_name) {
            if existing_path != &abs_path_str {
                match options.duplicate_policy.as_str() {
                    "skip" => {
                        stats.skipped += 1;
                        if options.dry_run {
                            stats.push_preview(PreviewItem {
                                path: abs_path_str.clone(),
                                action: "skip".to_string(),
                                display_name: None,
                            });
                        }
                        continue;
                    }
                    "overwrite" => {
                        if !options.dry_run {
                            if let Some(&existing_id) = existing_by_path.get(existing_path) {
                                overwrite_updates.push((
                                    existing_id,
                                    NewSong {
                                        display_name: display_name.clone(),
                                        source_path: abs_path_str.clone(),
                                        source_origin: "scan".to_string(),
                                        source_rel: (!source_rel.is_empty())
                                            .then(|| source_rel.clone()),
                                        media_kind: "video".to_string(),
                                        playback_mode: "plain".to_string(),
                                        playback_source: Some("plain".to_string()),
                                        can_queue: Some(is_playable),
                                        is_playable,
                                        scan_root: Some(root_str.clone()),
                                    },
                                ));
                            }
                            existing_by_path.remove(existing_path);
                            existing_by_path.insert(abs_path_str.clone(), 0);
                            if options.validate {
                                paths_to_validate.push(abs_path_str.clone());
                            }
                        }
                        stats.added += 1;
                        if options.dry_run {
                            stats.push_preview(PreviewItem {
                                path: abs_path_str.clone(),
                                action: "overwrite".to_string(),
                                display_name: None,
                            });
                        }
                        continue;
                    }
                    _ => {
                        let (unique_name, was_renamed) =
                            make_unique_display_name(&base_name, &source_rel, &used_names);
                        display_name = unique_name;
                        renamed = was_renamed;
                        if renamed {
                            stats.renamed += 1;
                        }
                    }
                }
            }
        }

        used_names.insert(display_name.clone());
        name_to_path.insert(display_name.clone(), abs_path_str.clone());

        if options.dry_run {
            stats.push_preview(PreviewItem {
                path: abs_path_str.clone(),
                action: if renamed {
                    "rename".to_string()
                } else {
                    "add".to_string()
                },
                display_name: Some(display_name),
            });
            stats.added += 1;
            continue;
        }

        to_create.push(NewSong {
            display_name,
            source_path: abs_path_str.clone(),
            source_origin: "scan".to_string(),
            source_rel: (!source_rel.is_empty()).then_some(source_rel),
            media_kind: "video".to_string(),
            playback_mode: "plain".to_string(),
            playback_source: Some("plain".to_string()),
            can_queue: Some(is_playable),
            is_playable,
            scan_root: Some(root_str.clone()),
        });
        existing_by_path.insert(abs_path_str.clone(), 0);
        if options.validate {
            paths_to_validate.push(abs_path_str);
        }
        stats.added += 1;
    }

    if !options.dry_run {
        if !to_create.is_empty() {
            songs.bulk_insert(&to_create).await?;
        }
        for (id, update) in overwrite_updates {
            songs
                .apply_scan_overwrite(
                    id,
                    &update.source_path,
                    &update.display_name,
                    update.source_rel.as_deref(),
                    update.is_playable,
                    update.scan_root.as_deref().unwrap_or(&root_str),
                )
                .await?;
        }
        info!(
            "scan persist: root={root_str} created={} pending_validate={}",
            stats.added,
            paths_to_validate.len()
        );
    }

    stats.pending_validate = paths_to_validate.len() as i64;
    Ok(ScanPersistResult {
        stats,
        paths_to_validate,
    })
}

/// 后台并发校验：更新 is_playable / can_queue。
pub async fn run_validate_paths(
    settings: &MediaSettings,
    songs: &SongRepository,
    task: &ScanTaskManager,
    paths: Vec<String>,
) {
    if paths.is_empty() {
        return;
    }
    if !task.try_begin(paths.len() as i64) {
        tracing::warn!("scan validate skipped: another task is running");
        return;
    }

    let concurrency = settings.scan_validate_concurrency;
    stream::iter(paths)
        .map(|path| {
            let settings = settings.clone();
            let songs = songs.clone();
            let task = task.clone();
            async move {
                let ok = probe_video_playable(&settings, &path).await;
                if let Ok(Some(song)) = songs.find_by_source_path(&path).await {
                    if let Err(e) = songs.update_playable_flags(song.id, ok, ok).await {
                        tracing::warn!("update playable flags failed for {}: {e}", song.id);
                    }
                }
                task.tick(!ok);
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

    task.finish();
    info!("scan validate finished: {:?}", task.status());
}
