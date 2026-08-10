//! 汇总"解析播放路径所需事实"（覆盖三件套是否存在、内嵌缓存是否有效等）后
//! 调用 `karaoke_domain::playback::resolve`。这是 domain/infra 分层的关键胶水：
//! 本模块只做事实采集（IO），决策逻辑完全交给 domain。

use crate::embedded;
use crate::media::{self, MediaSettings};
use crate::models::SongRow;
use karaoke_domain::playback::{
    self, EmbeddedAvailability, EmbeddedTriplet, OverrideStatus, PlaybackInput, PlaybackMode,
    PlaybackProfile,
};
use std::path::PathBuf;

#[derive(Clone)]
pub struct PlaybackResolver {
    pub override_path: PathBuf,
    pub media: MediaSettings,
}

impl PlaybackResolver {
    pub fn new(override_path: PathBuf, media: MediaSettings) -> Self {
        Self {
            override_path,
            media,
        }
    }

    async fn override_status(
        &self,
        display_name: &str,
    ) -> (OverrideStatus, playback::OverrideTriplet) {
        let override_dir = self.override_path.to_string_lossy().to_string();
        let paths = playback::override_triplet_paths(&override_dir, display_name);
        let status = OverrideStatus {
            video: tokio::fs::metadata(&paths.video).await.is_ok(),
            vocals: tokio::fs::metadata(&paths.vocals).await.is_ok(),
            accompaniment: tokio::fs::metadata(&paths.accompaniment).await.is_ok(),
        };
        (status, paths)
    }

    pub async fn override_complete(&self, display_name: &str) -> bool {
        self.override_status(display_name).await.0.complete()
    }

    /// 暴露覆盖三件套的存在性（供歌曲详情页展示，对应 Python `override_file_status`）。
    pub async fn override_file_status(&self, display_name: &str) -> OverrideStatus {
        self.override_status(display_name).await.0
    }

    /// 解析播放路径，不触发内嵌拆轨 ffmpeg（读路径，如列表/详情/点歌前校验）。
    pub async fn resolve(&self, song: &SongRow) -> PlaybackProfile {
        self.resolve_inner(song, false).await
    }

    /// 解析并在需要时触发内嵌拆轨 ffmpeg（写路径，如 prepare 任务）。
    pub async fn resolve_and_prepare_embedded(&self, song: &SongRow) -> PlaybackProfile {
        self.resolve_inner(song, true).await
    }

    async fn resolve_inner(&self, song: &SongRow, prepare_embedded: bool) -> PlaybackProfile {
        let (override_status, override_paths) = self.override_status(&song.display_name).await;
        let has_source_file = tokio::fs::metadata(&song.source_path).await.is_ok();
        let layout = song.layout().cloned();

        let embedded_avail =
            if has_source_file && karaoke_domain::audio_layout::has_dual_roles(layout.as_ref()) {
                match &layout {
                    Some(l) => {
                        let paths = embedded::ensure_embedded_cache(
                            &self.media,
                            &song.source_path,
                            l,
                            prepare_embedded,
                            None,
                        )
                        .await;
                        Some(EmbeddedAvailability {
                            paths: EmbeddedTriplet {
                                video: paths.video.to_string_lossy().to_string(),
                                vocals: paths.vocals.to_string_lossy().to_string(),
                                accompaniment: paths.accompaniment.to_string_lossy().to_string(),
                            },
                            ready: paths.ready,
                        })
                    }
                    None => None,
                }
            } else {
                None
            };

        let input = PlaybackInput {
            source_path: song.source_path.clone(),
            source_ext: karaoke_domain::file_ext(&song.source_path),
            has_source_file,
            is_playable: song.is_playable,
            override_status,
            override_paths,
            audio_layout: layout,
            embedded: embedded_avail,
        };
        playback::resolve(&input)
    }

    /// 是否需要后台 prepare（对应 `prepare_policy::needs_prepare`，补齐 IO 事实）。
    pub async fn needs_prepare(&self, song: &SongRow, profile: &PlaybackProfile) -> bool {
        let (override_status, _) = self.override_status(&song.display_name).await;
        let layout_has_dual = karaoke_domain::audio_layout::has_dual_roles(song.layout());
        let can_play_directly = if profile.mode == PlaybackMode::Plain {
            match &profile.video_path {
                Some(path) => media::can_play_directly(&self.media, path).await,
                None => false,
            }
        } else {
            false
        };
        karaoke_domain::needs_prepare(
            profile,
            override_status.complete(),
            layout_has_dual,
            can_play_directly,
        )
    }

    /// 计算某个播放流（video/vocals/accompaniment）的实际可读路径与 MIME。
    /// `None` 表示暂不可播放（对应 Python `stream_media_for_kind` 返回 `(None, None)`）。
    /// 只读，不会触发转码 ffmpeg（video 走 `resolve_browser_video_path_readonly`）。
    pub async fn stream_path_for_kind(
        &self,
        song: &SongRow,
        kind: &str,
    ) -> Option<(PathBuf, String)> {
        let profile = self.resolve(song).await;
        if profile.playback_source == playback::PlaybackSource::Embedded
            && !profile.embedded_cache_ready
        {
            return None;
        }

        let path = match kind {
            "video" => profile.video_path.clone(),
            "vocals" => profile.vocals_path.clone(),
            "accompaniment" => profile.accompaniment_path.clone(),
            _ => None,
        }?;
        if !std::path::Path::new(&path).is_file() {
            return None;
        }

        if kind == "video" {
            if profile.playback_source == playback::PlaybackSource::Embedded {
                return Some((PathBuf::from(path), "video/mp4".to_string()));
            }
            return media::resolve_browser_video_path_readonly(&self.media, &path).await;
        }

        let ext = karaoke_domain::file_ext(&path);
        let mime = playback::video_mime_for_ext(&ext)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        Some((PathBuf::from(path), mime))
    }

    /// 内嵌拆轨场景下判断流是否已就绪，不做任何写操作（对应 Python
    /// `PrepareTaskManager._check_stream_ready_sync`）。
    pub async fn is_stream_ready(&self, song: &SongRow) -> bool {
        let profile = self.resolve(song).await;
        match profile.playback_source {
            playback::PlaybackSource::Override => true,
            playback::PlaybackSource::Embedded => profile.embedded_cache_ready,
            playback::PlaybackSource::Plain => {
                let Some(path) = &profile.video_path else {
                    return false;
                };
                if media::can_play_directly(&self.media, path).await {
                    return true;
                }
                match media::browser_mp4_cache_path(&self.media, path) {
                    Ok(cached) => {
                        cached.is_file()
                            && media::validate_browser_mp4(&self.media, &cached.to_string_lossy())
                                .await
                    }
                    Err(_) => false,
                }
            }
        }
    }
}
