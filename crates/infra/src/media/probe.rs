use super::MediaSettings;
use karaoke_domain::media::{file_ext, MediaInfo, StreamInfo};
use karaoke_domain::{match_role, AudioTrack};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::warn;

/// 执行一次 ffprobe 并解析 JSON 输出；失败/超时均记录日志后返回 `None`
/// （修复 Python 版 `_run_ffprobe` 静默吞掉失败原因的问题，P1）。
async fn run_ffprobe(settings: &MediaSettings, args: &[&str]) -> Option<Value> {
    let _permit = settings.probe_semaphore.acquire().await.ok()?;

    let mut cmd = Command::new("ffprobe");
    cmd.arg("-v").arg("error");
    cmd.arg("-probesize").arg(settings.probe_size.to_string());
    cmd.arg("-analyzeduration")
        .arg(settings.analyze_duration.to_string());
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            warn!("ffprobe spawn failed: {e}");
            return None;
        }
    };

    let output = match timeout(settings.probe_timeout, child.wait_with_output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            warn!("ffprobe wait failed: {e}");
            return None;
        }
        Err(_) => {
            warn!(
                "ffprobe timed out after {:?}: args={:?}",
                settings.probe_timeout, args
            );
            return None;
        }
    };

    if !output.status.success() {
        warn!(
            "ffprobe exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }

    match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(v) => Some(v),
        Err(e) => {
            warn!("ffprobe output parse failed: {e}");
            None
        }
    }
}

fn parse_streams(data: &Value) -> Vec<StreamInfo> {
    let Some(streams) = data.get("streams").and_then(|s| s.as_array()) else {
        return vec![];
    };

    streams
        .iter()
        .map(|raw| StreamInfo {
            index: raw.get("index").and_then(Value::as_i64).unwrap_or(0) as i32,
            codec_type: raw
                .get("codec_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            codec_name: raw
                .get("codec_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase(),
            width: raw.get("width").and_then(Value::as_i64).unwrap_or(0) as i32,
            height: raw.get("height").and_then(Value::as_i64).unwrap_or(0) as i32,
            pix_fmt: raw
                .get("pix_fmt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase(),
            attached_pic: raw
                .get("disposition")
                .and_then(|d| d.get("attached_pic"))
                .and_then(Value::as_i64)
                .unwrap_or(0)
                == 1,
        })
        .collect()
}

pub async fn probe_streams(settings: &MediaSettings, file_path: &str) -> Vec<StreamInfo> {
    let data = run_ffprobe(
        settings,
        &[
            "-show_entries",
            "stream=index,codec_type,codec_name,width,height,pix_fmt",
            "-show_entries",
            "stream_disposition=attached_pic",
            "-of",
            "json",
            file_path,
        ],
    )
    .await;

    let Some(data) = data else { return vec![] };
    parse_streams(&data)
}

pub async fn probe_media_info(settings: &MediaSettings, file_path: &str) -> Option<MediaInfo> {
    if !Path::new(file_path).is_file() {
        return None;
    }

    // 合并 stream + format 为单次 ffprobe，避免每个文件两次子进程。
    let data = run_ffprobe(
        settings,
        &[
            "-show_entries",
            "stream=index,codec_type,codec_name,width,height,pix_fmt",
            "-show_entries",
            "stream_disposition=attached_pic",
            "-show_entries",
            "format=format_name,duration",
            "-of",
            "json",
            file_path,
        ],
    )
    .await?;

    let streams = parse_streams(&data);
    if streams.is_empty() {
        return None;
    }

    let fmt = data.get("format");
    let duration = fmt
        .and_then(|f| f.get("duration"))
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let format_name = fmt
        .and_then(|f| f.get("format_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();

    let video = karaoke_domain::media::pick_main_video_stream(&streams);
    let audio = karaoke_domain::media::pick_main_audio_stream(&streams);

    Some(MediaInfo {
        ext: file_ext(file_path),
        format_name,
        duration,
        video_codec: video.map(|v| v.codec_name.clone()),
        audio_codec: audio.map(|a| a.codec_name.clone()),
        has_video: video.is_some(),
        has_audio: audio.is_some(),
        video_width: video.map(|v| v.width).unwrap_or(0),
        video_height: video.map(|v| v.height).unwrap_or(0),
        pix_fmt: video.map(|v| v.pix_fmt.clone()).unwrap_or_default(),
        streams,
    })
}

/// K 歌场景：有音频流且 duration > 0 即可唱；视频缺失只影响 MV 显示。
pub async fn probe_video_playable(settings: &MediaSettings, file_path: &str) -> bool {
    match probe_media_info(settings, file_path).await {
        Some(info) => info.has_audio && info.duration > 0.0,
        None => false,
    }
}

pub async fn probe_audio_tracks(settings: &MediaSettings, file_path: &str) -> Vec<AudioTrack> {
    if !Path::new(file_path).is_file() {
        return vec![];
    }
    let data = run_ffprobe(
        settings,
        &[
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index,codec_name,channels",
            "-show_entries",
            "stream_tags=title,language",
            "-of",
            "json",
            file_path,
        ],
    )
    .await;
    let Some(data) = data else { return vec![] };
    let Some(streams) = data.get("streams").and_then(|s| s.as_array()) else {
        return vec![];
    };

    streams
        .iter()
        .map(|raw| {
            let tags = raw.get("tags");
            let title = tags
                .and_then(|t| t.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let language = tags
                .and_then(|t| t.get("language"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            AudioTrack {
                index: raw.get("index").and_then(Value::as_i64).unwrap_or(0) as i32,
                codec: raw
                    .get("codec_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase(),
                channels: raw.get("channels").and_then(Value::as_i64).unwrap_or(0) as i32,
                role: match_role(&title, &language),
                title,
                language,
            }
        })
        .collect()
}
