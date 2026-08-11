//! ffmpeg 转码/校验：异步子进程 + 硬超时（超时即 kill），修复 Python 版
//! “stderr 不 EOF 时超时失效、线程永久占用”的 P0 问题。

use super::probe::probe_media_info;
use super::MediaSettings;
use karaoke_domain::media::{file_ext, StreamInfo};
use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};

pub type ProgressFn = Arc<dyn Fn(f64) + Send + Sync>;

const CACHE_VERSION: &str = "v3";
const EMBEDDED_CACHE_VERSION: &str = "v2";

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("ffmpeg 执行超时（已终止子进程）")]
    Timeout,
    #[error("ffmpeg 退出码非零: {0:?}")]
    NonZeroExit(Option<i32>),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

static DURATION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Duration:\s*(\d+):(\d+):(\d+(?:\.\d+)?)").unwrap());
static TIME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"time=(\d+):(\d+):(\d+(?:\.\d+)?)").unwrap());

fn parse_ts(h: &str, m: &str, s: &str) -> f64 {
    h.parse::<f64>().unwrap_or(0.0) * 3600.0
        + m.parse::<f64>().unwrap_or(0.0) * 60.0
        + s.parse::<f64>().unwrap_or(0.0)
}

/// 运行 ffmpeg 并解析 stderr 中的进度行；`timeout` 包裹“读取 stderr 直到进程结束”
/// 整个过程，卡死时会被强制 kill，而不是像 Python 版那样在读循环里永久阻塞。
async fn run_ffmpeg_with_progress(
    settings: &MediaSettings,
    args: Vec<String>,
    mut duration_sec: Option<f64>,
    on_progress: Option<ProgressFn>,
    hard_timeout: std::time::Duration,
) -> Result<(), MediaError> {
    let _permit = settings.transcode_semaphore.acquire().await.ok();

    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stderr = child.stderr.take().expect("stderr piped");

    let run = async {
        let mut reader = BufReader::new(stderr).lines();
        let mut last_pct = -1.0f64;
        while let Ok(Some(line)) = reader.next_line().await {
            if duration_sec.is_none() {
                if let Some(m) = DURATION_RE.captures(&line) {
                    duration_sec = Some(parse_ts(&m[1], &m[2], &m[3]));
                }
            }
            if let (Some(m), Some(dur)) = (TIME_RE.captures(&line), duration_sec) {
                if dur > 0.0 {
                    let current = parse_ts(&m[1], &m[2], &m[3]);
                    let pct = (current / dur * 100.0).min(99.0);
                    if pct - last_pct >= 0.5 {
                        last_pct = pct;
                        if let Some(cb) = &on_progress {
                            cb(pct);
                        }
                    }
                }
            }
        }
        let status = child.wait().await?;
        Ok::<_, std::io::Error>(status)
    };

    match timeout(hard_timeout, run).await {
        Ok(Ok(status)) if status.success() => {
            if let Some(cb) = &on_progress {
                cb(100.0);
            }
            Ok(())
        }
        Ok(Ok(status)) => Err(MediaError::NonZeroExit(status.code())),
        Ok(Err(e)) => Err(MediaError::Io(e)),
        Err(_) => {
            warn!("ffmpeg timed out after {hard_timeout:?}, killing process: args={args:?}");
            let _ = child.kill().await;
            Err(MediaError::Timeout)
        }
    }
}

fn ffmpeg_maps(video: &StreamInfo, audio: Option<&StreamInfo>) -> Vec<String> {
    let mut maps = vec!["-map".to_string(), format!("0:{}", video.index)];
    if let Some(a) = audio {
        maps.push("-map".to_string());
        maps.push(format!("0:{}", a.index));
    }
    maps
}

async fn remove_if_exists(path: &Path) {
    if path.is_file() {
        if let Err(e) = tokio::fs::remove_file(path).await {
            warn!("remove failed {}: {e}", path.display());
        }
    }
}

pub async fn validate_browser_mp4(settings: &MediaSettings, path: &str) -> bool {
    match probe_media_info(settings, path).await {
        Some(info) => karaoke_domain::is_valid_browser_mp4(&info),
        None => false,
    }
}

fn cache_fresher_than_source(source_path: &str, cached: &Path) -> bool {
    match (std::fs::metadata(source_path), std::fs::metadata(cached)) {
        (Ok(s), Ok(c)) => match (s.modified(), c.modified()) {
            (Ok(sm), Ok(cm)) => cm >= sm,
            _ => false,
        },
        _ => false,
    }
}

fn scale_filter_for_max_height(max_height: u32, video: &StreamInfo) -> Option<String> {
    if max_height == 0 || video.height <= 0 || (video.height as u32) <= max_height {
        return None;
    }
    Some(format!("scale=-2:{max_height}"))
}

async fn remux_to_mp4(
    settings: &MediaSettings,
    source: &str,
    dest: &Path,
    video: &StreamInfo,
    audio: Option<&StreamInfo>,
    on_progress: Option<ProgressFn>,
) -> bool {
    if let Some(parent) = dest.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let mut args = vec![
        "-y".to_string(),
        "-nostdin".to_string(),
        "-i".to_string(),
        source.to_string(),
    ];
    args.extend(ffmpeg_maps(video, audio));
    args.extend([
        "-c".to_string(),
        "copy".to_string(),
        "-tag:v".to_string(),
        "avc1".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        dest.to_string_lossy().to_string(),
    ]);

    if let Some(cb) = &on_progress {
        cb(10.0);
    }
    let result = run_ffmpeg_with_progress(
        settings,
        args,
        None,
        on_progress,
        std::time::Duration::from_secs(600),
    )
    .await;

    match result {
        Ok(()) => validate_browser_mp4(settings, &dest.to_string_lossy()).await,
        Err(e) => {
            warn!("remux failed {source}: {e}");
            remove_if_exists(dest).await;
            false
        }
    }
}

async fn transcode_to_mp4(
    settings: &MediaSettings,
    source: &str,
    dest: &Path,
    video: &StreamInfo,
    audio: Option<&StreamInfo>,
    on_progress: Option<ProgressFn>,
    duration_sec: Option<f64>,
) -> bool {
    if let Some(parent) = dest.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let mut args = vec![
        "-y".to_string(),
        "-nostdin".to_string(),
        "-i".to_string(),
        source.to_string(),
    ];
    args.extend(ffmpeg_maps(video, audio));
    if let Some(vf) = scale_filter_for_max_height(settings.transcode_max_height, video) {
        args.extend(["-vf".to_string(), vf]);
    }
    args.extend(
        [
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
            "-level",
            "4.1",
            "-tag:v",
            "avc1",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-ac",
            "2",
            "-movflags",
            "+faststart",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    args.push(dest.to_string_lossy().to_string());

    let result = run_ffmpeg_with_progress(
        settings,
        args,
        duration_sec,
        on_progress,
        settings.transcode_timeout,
    )
    .await;

    match result {
        Ok(()) => validate_browser_mp4(settings, &dest.to_string_lossy()).await,
        Err(e) => {
            warn!("transcode failed {source}: {e}");
            remove_if_exists(dest).await;
            false
        }
    }
}

async fn prepare_browser_mp4(
    settings: &MediaSettings,
    source: &str,
    dest: &Path,
    on_progress: Option<ProgressFn>,
) -> bool {
    let Some(info) = probe_media_info(settings, source).await else {
        return false;
    };
    let Some(video) = karaoke_domain::media::pick_main_video_stream(&info.streams).cloned() else {
        warn!("no usable video stream: {source}");
        return false;
    };
    let audio = karaoke_domain::media::pick_main_audio_stream(&info.streams).cloned();
    let duration = if info.duration > 0.0 {
        Some(info.duration)
    } else {
        None
    };

    if karaoke_domain::media::stream_needs_transcode(&video) {
        info!(
            "transcode for browser: {source} ({}/{})",
            video.codec_name, video.pix_fmt
        );
        return transcode_to_mp4(
            settings,
            source,
            dest,
            &video,
            audio.as_ref(),
            on_progress,
            duration,
        )
        .await;
    }

    info!("remux for browser: {source} ({})", video.codec_name);
    if remux_to_mp4(
        settings,
        source,
        dest,
        &video,
        audio.as_ref(),
        on_progress.clone(),
    )
    .await
    {
        return true;
    }
    info!("remux failed, fallback transcode: {source}");
    transcode_to_mp4(
        settings,
        source,
        dest,
        &video,
        audio.as_ref(),
        on_progress,
        duration,
    )
    .await
}

fn stat_fingerprint(path: &str) -> std::io::Result<(i64, u64)> {
    let meta = std::fs::metadata(path)?;
    Ok((meta.mtime_nsec(), meta.size()))
}

/// 浏览器可播 mp4 缓存路径（对应 Python `_cache_path`：源文件路径 + mtime + size 哈希）。
pub fn browser_mp4_cache_path(
    settings: &MediaSettings,
    source_path: &str,
) -> std::io::Result<PathBuf> {
    let (mtime_ns, size) = stat_fingerprint(source_path)?;
    let abs = std::fs::canonicalize(source_path).unwrap_or_else(|_| PathBuf::from(source_path));
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "{CACHE_VERSION}:{}:{mtime_ns}:{size}",
        abs.display()
    ));
    let digest = hex::encode(hasher.finalize());
    Ok(settings
        .play_cache_path
        .join(format!("{}.mp4", &digest[..20])))
}

/// 有兜底缓存且比源新则发缓存，否则直发源文件。纯文件系统检查，不跑 ffprobe。
pub async fn resolve_browser_video_path_readonly(
    settings: &MediaSettings,
    source_path: &str,
) -> Option<(PathBuf, String)> {
    if !Path::new(source_path).is_file() {
        return None;
    }
    if let Ok(cached) = browser_mp4_cache_path(settings, source_path) {
        if cached.is_file() && cache_fresher_than_source(source_path, &cached) {
            return Some((cached, "video/mp4".to_string()));
        }
    }
    let ext = file_ext(source_path);
    let mime = karaoke_domain::playback::video_mime_for_ext(&ext)
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Some((PathBuf::from(source_path), mime))
}

/// 后台任务：生成浏览器可播 mp4 兜底缓存（由播放失败上报强制触发）。
pub async fn ensure_browser_mp4_cache(
    settings: &MediaSettings,
    source_path: &str,
    on_progress: Option<ProgressFn>,
) -> bool {
    if !Path::new(source_path).is_file() {
        return false;
    }
    let Ok(cached) = browser_mp4_cache_path(settings, source_path) else {
        return false;
    };
    if cached.is_file() && cache_fresher_than_source(source_path, &cached) {
        if validate_browser_mp4(settings, &cached.to_string_lossy()).await {
            return true;
        }
        remove_if_exists(&cached).await;
    }
    prepare_browser_mp4(settings, source_path, &cached, on_progress).await
}

/// 提取单条音轨为 AAC；源轨已兼容时 `-c:a copy`。
pub async fn extract_audio_track(
    settings: &MediaSettings,
    source: &str,
    track_index: i32,
    dest: &Path,
    on_progress: Option<ProgressFn>,
    duration_sec: Option<f64>,
) -> bool {
    if let Some(parent) = dest.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let info = probe_media_info(settings, source).await;
    let duration = duration_sec.or_else(|| {
        info.as_ref()
            .and_then(|i| (i.duration > 0.0).then_some(i.duration))
    });
    let codec = info
        .as_ref()
        .and_then(|i| {
            i.streams
                .iter()
                .find(|s| s.index == track_index && s.codec_type == "audio")
                .map(|s| s.codec_name.clone())
        })
        .unwrap_or_default();
    let copy = karaoke_domain::audio_can_copy(&codec);

    let mut args: Vec<String> = [
        "-y",
        "-nostdin",
        "-i",
        source,
        "-map",
        &format!("0:{track_index}"),
        "-vn",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if copy {
        args.extend(["-c:a".to_string(), "copy".to_string()]);
    } else {
        args.extend(
            ["-c:a", "aac", "-b:a", "192k", "-ac", "2"]
                .iter()
                .map(|s| s.to_string()),
        );
    }
    args.push(dest.to_string_lossy().to_string());

    let result = run_ffmpeg_with_progress(
        settings,
        args,
        duration,
        on_progress,
        settings.transcode_timeout,
    )
    .await;
    match result {
        Ok(()) => dest.is_file() && dest.metadata().map(|m| m.len() > 0).unwrap_or(false),
        Err(e) => {
            warn!("audio extract failed {source} track {track_index}: {e}");
            remove_if_exists(dest).await;
            false
        }
    }
}

/// 从视频抽出混合音频（供 Separator 上传）；优先 wav PCM，兼容性最好。
pub async fn extract_mix_audio(settings: &MediaSettings, source: &str, dest: &Path) -> bool {
    if let Some(parent) = dest.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let info = probe_media_info(settings, source).await;
    let duration = info
        .as_ref()
        .and_then(|i| (i.duration > 0.0).then_some(i.duration));
    let mut args: Vec<String> = [
        "-y",
        "-nostdin",
        "-i",
        source,
        "-vn",
        "-ac",
        "2",
        "-ar",
        "44100",
        "-c:a",
        "pcm_s16le",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    args.push(dest.to_string_lossy().to_string());

    let result =
        run_ffmpeg_with_progress(settings, args, duration, None, settings.transcode_timeout).await;
    match result {
        Ok(()) => dest.is_file() && dest.metadata().map(|m| m.len() > 0).unwrap_or(false),
        Err(e) => {
            warn!("mix audio extract failed {source}: {e}");
            remove_if_exists(dest).await;
            false
        }
    }
}

/// 视频 + 原唱音频 + 伴奏音频 → 单个双轨容器（轨标题可识别角色）。
pub async fn remux_dual_container(
    settings: &MediaSettings,
    video: &str,
    vocals: &str,
    accompaniment: &str,
    dest: &Path,
) -> bool {
    if let Some(parent) = dest.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let streams = crate::media::probe_streams(settings, video).await;
    let Some(v) = karaoke_domain::media::pick_main_video_stream(&streams) else {
        warn!("remux_dual_container: no video stream in {video}");
        return false;
    };
    let info = probe_media_info(settings, video).await;
    let duration = info
        .as_ref()
        .and_then(|i| (i.duration > 0.0).then_some(i.duration));

    let args = vec![
        "-y".to_string(),
        "-nostdin".to_string(),
        "-i".to_string(),
        video.to_string(),
        "-i".to_string(),
        vocals.to_string(),
        "-i".to_string(),
        accompaniment.to_string(),
        "-map".to_string(),
        format!("0:{}", v.index),
        "-map".to_string(),
        "1:a:0".to_string(),
        "-map".to_string(),
        "2:a:0".to_string(),
        "-c:v".to_string(),
        "copy".to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        "192k".to_string(),
        "-ac".to_string(),
        "2".to_string(),
        "-metadata:s:a:0".to_string(),
        "title=原唱".to_string(),
        "-metadata:s:a:1".to_string(),
        "title=伴奏".to_string(),
        "-shortest".to_string(),
        dest.to_string_lossy().to_string(),
    ];

    let result =
        run_ffmpeg_with_progress(settings, args, duration, None, settings.transcode_timeout).await;
    match result {
        Ok(()) => {
            let ok = dest.is_file() && dest.metadata().map(|m| m.len() > 0).unwrap_or(false);
            if !ok {
                remove_if_exists(dest).await;
            }
            ok
        }
        Err(e) => {
            warn!("remux_dual_container failed: {e}");
            remove_if_exists(dest).await;
            false
        }
    }
}

/// 音轨布局哈希（源文件指纹 + layout 内容），对应 Python `_layout_hash`。
pub fn embedded_cache_dir(
    settings: &MediaSettings,
    source_path: &str,
    layout: &karaoke_domain::AudioLayout,
) -> std::io::Result<PathBuf> {
    let (mtime_ns, size) = stat_fingerprint(source_path)?;
    let abs = std::fs::canonicalize(source_path).unwrap_or_else(|_| PathBuf::from(source_path));
    let payload = serde_json::to_string(layout).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "{EMBEDDED_CACHE_VERSION}:{}:{mtime_ns}:{size}:{payload}",
        abs.display()
    ));
    let digest = hex::encode(hasher.finalize());
    Ok(settings
        .play_cache_path
        .join("embedded")
        .join(&digest[..24]))
}
