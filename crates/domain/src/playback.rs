//! 播放路径解析。对应 Python `karaoke/domain/playback.py`。
//!
//! 与 Python 版本的关键差异：所有需要 `stat`/`isfile`/文件哈希 才能得到的事实
//! （覆盖三件套是否存在、内嵌缓存是否有效等）均由调用方预先算好，通过
//! [`PlaybackInput`] 传入；本模块只做纯决策，因此可以直接用固定数据做单元测试。

use crate::audio_layout::{has_dual_roles, AudioLayout};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    Enhanced,
    Plain,
    NotReady,
}

impl PlaybackMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlaybackMode::Enhanced => "enhanced",
            PlaybackMode::Plain => "plain",
            PlaybackMode::NotReady => "not_ready",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackSource {
    Override,
    Embedded,
    Plain,
}

impl PlaybackSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlaybackSource::Override => "override",
            PlaybackSource::Embedded => "embedded",
            PlaybackSource::Plain => "plain",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OverrideTriplet {
    pub video: String,
    pub vocals: String,
    pub accompaniment: String,
}

/// 覆盖三件套的路径命名规则（纯字符串拼接，无 IO）。
pub fn override_triplet_paths(override_dir: &str, display_name: &str) -> OverrideTriplet {
    let base = format!("{}/{}", override_dir.trim_end_matches('/'), display_name);
    OverrideTriplet {
        video: format!("{base}.mp4"),
        vocals: format!("{base}_vocals.mp3"),
        accompaniment: format!("{base}_accompaniment.mp3"),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OverrideStatus {
    pub video: bool,
    pub vocals: bool,
    pub accompaniment: bool,
}

impl OverrideStatus {
    pub fn complete(&self) -> bool {
        self.video && self.vocals && self.accompaniment
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmbeddedTriplet {
    pub video: String,
    pub vocals: String,
    pub accompaniment: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddedAvailability {
    pub paths: EmbeddedTriplet,
    pub ready: bool,
}

/// 直接可播放判定所需事实（source 是否为浏览器原生可播编码，由 infra 通过 ffprobe 算好）。
#[derive(Debug, Clone, Default)]
pub struct PlaybackInput {
    pub source_path: String,
    pub source_ext: String,
    pub has_source_file: bool,
    pub is_playable: bool,
    pub override_status: OverrideStatus,
    pub override_paths: OverrideTriplet,
    pub audio_layout: Option<AudioLayout>,
    pub embedded: Option<EmbeddedAvailability>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackProfile {
    pub mode: PlaybackMode,
    pub playback_source: PlaybackSource,
    pub can_queue: bool,
    pub video_path: Option<String>,
    pub vocals_path: Option<String>,
    pub accompaniment_path: Option<String>,
    pub video_mime: Option<String>,
    pub video_ext: Option<String>,
    pub embedded_cache_ready: bool,
}

/// 与 `settings.CONTENT_TYPE` 一致的扩展名 -> MIME 映射（涵盖视频与音轨产物）。
pub fn video_mime_for_ext(ext: &str) -> Option<String> {
    let mime = match ext {
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "m4v" => "video/x-m4v",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        _ => return None,
    };
    Some(mime.to_string())
}

/// 解析播放路径（对应 Python `resolve()`）。
pub fn resolve(input: &PlaybackInput) -> PlaybackProfile {
    if input.override_status.complete() {
        return PlaybackProfile {
            mode: PlaybackMode::Enhanced,
            playback_source: PlaybackSource::Override,
            can_queue: true,
            video_path: Some(input.override_paths.video.clone()),
            vocals_path: Some(input.override_paths.vocals.clone()),
            accompaniment_path: Some(input.override_paths.accompaniment.clone()),
            video_mime: Some("video/mp4".to_string()),
            video_ext: Some("mp4".to_string()),
            embedded_cache_ready: false,
        };
    }

    if input.has_source_file && has_dual_roles(input.audio_layout.as_ref()) {
        let (paths, ready) = match &input.embedded {
            Some(avail) => (avail.paths.clone(), avail.ready),
            None => (EmbeddedTriplet::default(), false),
        };
        return PlaybackProfile {
            mode: PlaybackMode::Enhanced,
            playback_source: PlaybackSource::Embedded,
            can_queue: true,
            video_path: Some(paths.video),
            vocals_path: Some(paths.vocals),
            accompaniment_path: Some(paths.accompaniment),
            video_mime: Some("video/mp4".to_string()),
            video_ext: Some("mp4".to_string()),
            embedded_cache_ready: ready,
        };
    }

    if input.is_playable && input.has_source_file {
        return PlaybackProfile {
            mode: PlaybackMode::Plain,
            playback_source: PlaybackSource::Plain,
            can_queue: true,
            video_path: Some(input.source_path.clone()),
            vocals_path: None,
            accompaniment_path: None,
            video_mime: video_mime_for_ext(&input.source_ext),
            video_ext: Some(input.source_ext.clone()),
            embedded_cache_ready: false,
        };
    }

    PlaybackProfile {
        mode: PlaybackMode::NotReady,
        playback_source: PlaybackSource::Plain,
        can_queue: false,
        video_path: None,
        vocals_path: None,
        accompaniment_path: None,
        video_mime: None,
        video_ext: None,
        embedded_cache_ready: false,
    }
}

/// 已持久化的曲库元数据（对应 Python `list_meta_from_song`，用于列表页在未重新
/// 解析 profile 时的兜底展示）。
#[derive(Debug, Clone, Default)]
pub struct SongMeta {
    pub playback_mode: Option<String>,
    pub playback_source: Option<String>,
    pub can_queue: Option<bool>,
    pub is_playable: bool,
    pub audio_layout: Option<AudioLayout>,
}

pub fn list_meta_from_song(meta: &SongMeta) -> (String, String, bool) {
    let mode = meta
        .playback_mode
        .clone()
        .unwrap_or_else(|| "plain".to_string());
    let source = meta.playback_source.clone().unwrap_or_else(|| {
        if mode == "enhanced" {
            if has_dual_roles(meta.audio_layout.as_ref()) {
                "embedded".to_string()
            } else {
                "override".to_string()
            }
        } else {
            "plain".to_string()
        }
    });
    let can_queue = meta.can_queue.unwrap_or(meta.is_playable);
    (mode, source, can_queue)
}

/// `mode` 字段展示逻辑：`not_ready` 时回退到持久化的 `playback_mode`。
pub fn effective_mode(profile: &PlaybackProfile, persisted_mode: &str) -> String {
    if profile.mode == PlaybackMode::NotReady {
        persisted_mode.to_string()
    } else {
        profile.mode.as_str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_layout::{build_layout, AudioTrack, TrackRole};

    fn dual_layout() -> AudioLayout {
        build_layout(
            vec![
                AudioTrack {
                    index: 1,
                    title: "原唱".into(),
                    language: String::new(),
                    codec: "aac".into(),
                    channels: 2,
                    role: TrackRole::Vocals,
                },
                AudioTrack {
                    index: 2,
                    title: "伴奏".into(),
                    language: String::new(),
                    codec: "aac".into(),
                    channels: 2,
                    role: TrackRole::Accompaniment,
                },
            ],
            Some(0),
            "auto",
        )
    }

    #[test]
    fn override_complete_wins_over_everything() {
        let input = PlaybackInput {
            has_source_file: true,
            is_playable: true,
            override_status: OverrideStatus {
                video: true,
                vocals: true,
                accompaniment: true,
            },
            override_paths: override_triplet_paths("/KTV/__override__", "song"),
            ..Default::default()
        };
        let profile = resolve(&input);
        assert_eq!(profile.mode, PlaybackMode::Enhanced);
        assert_eq!(profile.playback_source, PlaybackSource::Override);
        assert!(profile.can_queue);
        assert_eq!(profile.video_path.unwrap(), "/KTV/__override__/song.mp4");
    }

    #[test]
    fn dual_layout_without_ready_cache_still_can_queue_but_not_ready() {
        let input = PlaybackInput {
            has_source_file: true,
            audio_layout: Some(dual_layout()),
            embedded: Some(EmbeddedAvailability {
                paths: EmbeddedTriplet {
                    video: "/cache/video.mp4".into(),
                    vocals: "/cache/vocals.m4a".into(),
                    accompaniment: "/cache/accompaniment.m4a".into(),
                },
                ready: false,
            }),
            ..Default::default()
        };
        let profile = resolve(&input);
        assert_eq!(profile.playback_source, PlaybackSource::Embedded);
        assert!(profile.can_queue);
        assert!(!profile.embedded_cache_ready);
    }

    #[test]
    fn plain_playable_source_resolves_to_plain_mode() {
        let input = PlaybackInput {
            source_path: "/KTV/song.mp4".into(),
            source_ext: "mp4".into(),
            has_source_file: true,
            is_playable: true,
            ..Default::default()
        };
        let profile = resolve(&input);
        assert_eq!(profile.mode, PlaybackMode::Plain);
        assert_eq!(profile.video_mime.unwrap(), "video/mp4");
    }

    #[test]
    fn missing_source_resolves_to_not_ready_and_blocks_queue() {
        let input = PlaybackInput::default();
        let profile = resolve(&input);
        assert_eq!(profile.mode, PlaybackMode::NotReady);
        assert!(!profile.can_queue);
    }

    #[test]
    fn effective_mode_falls_back_to_persisted_when_not_ready() {
        let profile = resolve(&PlaybackInput::default());
        assert_eq!(effective_mode(&profile, "plain"), "plain");
    }
}
