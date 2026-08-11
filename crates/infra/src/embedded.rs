//! MKV 双音轨内嵌缓存构建。视频直发源文件；此处只抽取 vocals/accompaniment 音频。

use crate::media::{MediaSettings, ProgressFn};
use karaoke_domain::{get_track_index, AudioLayout, TrackRole};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Clone, Default)]
pub struct EmbeddedPaths {
    pub vocals: PathBuf,
    pub accompaniment: PathBuf,
    pub ready: bool,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    source_path: String,
    source_mtime_ns: i64,
    source_size: u64,
    layout: AudioLayout,
    version: String,
}

const VERSION: &str = "v2";

fn expected_paths(cache_dir: PathBuf) -> EmbeddedPaths {
    EmbeddedPaths {
        vocals: cache_dir.join("vocals.m4a"),
        accompaniment: cache_dir.join("accompaniment.m4a"),
        ready: false,
        cache_dir,
    }
}

fn manifest_path(cache_dir: &std::path::Path) -> PathBuf {
    cache_dir.join("manifest.json")
}

fn read_manifest(cache_dir: &std::path::Path) -> Option<Manifest> {
    let raw = std::fs::read_to_string(manifest_path(cache_dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_manifest(
    cache_dir: &std::path::Path,
    source_path: &str,
    layout: &AudioLayout,
) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(source_path)?;
    let manifest = Manifest {
        source_path: source_path.to_string(),
        source_mtime_ns: meta.mtime_nsec(),
        source_size: meta.size(),
        layout: layout.clone(),
        version: VERSION.to_string(),
    };
    std::fs::write(manifest_path(cache_dir), serde_json::to_string(&manifest)?)
}

fn is_cache_valid(source_path: &str, layout: &AudioLayout, paths: &EmbeddedPaths) -> bool {
    for p in [&paths.vocals, &paths.accompaniment] {
        match std::fs::metadata(p) {
            Ok(m) if m.len() > 0 => {}
            _ => return false,
        }
    }
    let Some(manifest) = read_manifest(&paths.cache_dir) else {
        return false;
    };
    let Ok(meta) = std::fs::metadata(source_path) else {
        return false;
    };
    use std::os::unix::fs::MetadataExt;
    manifest.source_path == source_path
        && manifest.source_mtime_ns == meta.mtime_nsec()
        && manifest.source_size == meta.size()
        && manifest.version == VERSION
        && serde_json::to_string(&manifest.layout).ok() == serde_json::to_string(layout).ok()
}

/// 计算/生成内嵌音轨缓存。`prepare=false` 时只查询是否已存在有效缓存（不触发 ffmpeg）。
pub async fn ensure_embedded_cache(
    settings: &MediaSettings,
    source_path: &str,
    layout: &AudioLayout,
    prepare: bool,
    on_progress: Option<ProgressFn>,
) -> EmbeddedPaths {
    let cache_dir = match crate::media::transcode::embedded_cache_dir(settings, source_path, layout)
    {
        Ok(d) => d,
        Err(e) => {
            warn!("embedded cache dir compute failed for {source_path}: {e}");
            return EmbeddedPaths::default();
        }
    };
    let mut paths = expected_paths(cache_dir.clone());

    if is_cache_valid(source_path, layout, &paths) {
        paths.ready = true;
        return paths;
    }

    if !prepare {
        paths.ready = false;
        return paths;
    }

    if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
        warn!(
            "create embedded cache dir failed {}: {e}",
            cache_dir.display()
        );
        return paths;
    }

    let vocals_idx = get_track_index(Some(layout), TrackRole::Vocals);
    let accomp_idx = get_track_index(Some(layout), TrackRole::Accompaniment);
    let (Some(vocals_idx), Some(accomp_idx)) = (vocals_idx, accomp_idx) else {
        paths.ready = false;
        return paths;
    };

    let info = crate::media::probe_media_info(settings, source_path).await;
    let duration = info
        .as_ref()
        .and_then(|i| (i.duration > 0.0).then_some(i.duration));

    let report = |pct: f64| {
        if let Some(cb) = &on_progress {
            cb(pct);
        }
    };

    report(0.0);
    let ok_vocals = crate::media::transcode::extract_audio_track(
        settings,
        source_path,
        vocals_idx,
        &paths.vocals,
        on_progress.clone(),
        duration,
    )
    .await;

    report(50.0);
    let ok_accomp = crate::media::transcode::extract_audio_track(
        settings,
        source_path,
        accomp_idx,
        &paths.accompaniment,
        on_progress.clone(),
        duration,
    )
    .await;

    if ok_vocals && ok_accomp {
        report(100.0);
        if let Err(e) = write_manifest(&cache_dir, source_path, layout) {
            warn!("write manifest failed {}: {e}", cache_dir.display());
        }
        paths.ready = true;
        info!("embedded audio cache ready: {}", cache_dir.display());
    } else {
        paths.ready = false;
        warn!("embedded audio cache incomplete: {source_path}");
    }

    paths
}

/// 探测源文件音轨布局（不做持久化，由调用方决定是否写库）。
pub async fn probe_layout(
    settings: &MediaSettings,
    source_path: &str,
    assigned_by: &str,
) -> AudioLayout {
    if !std::path::Path::new(source_path).is_file() {
        return AudioLayout {
            tracks: vec![],
            layout: karaoke_domain::LayoutType::Unknown,
            assigned_by: assigned_by.to_string(),
            video_stream_index: None,
        };
    }
    let streams = crate::media::probe_streams(settings, source_path).await;
    let video_index = karaoke_domain::media::pick_main_video_stream(&streams).map(|v| v.index);
    let tracks = crate::media::probe_audio_tracks(settings, source_path).await;
    karaoke_domain::audio_layout::build_layout(tracks, video_index, assigned_by)
}
