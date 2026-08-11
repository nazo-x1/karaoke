//! 媒体预检：可播检测 + layout 启发式 → plain / enhanced / unknown。

use super::MediaSettings;
use crate::embedded::probe_layout;
use karaoke_domain::audio_layout::has_dual_roles;
use karaoke_domain::AudioLayout;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PreflightResult {
    pub playable: bool,
    /// `plain` | `enhanced` | `unknown`
    pub mode_hint: String,
    pub suggestion: String,
    pub layout: AudioLayout,
}

/// 对本地媒体文件做入库前预检（严格上传 / 工坊共用）。
pub async fn preflight_media(settings: &MediaSettings, path: &str) -> PreflightResult {
    let playable = crate::media::probe_video_playable(settings, path).await;
    let layout = probe_layout(settings, path, "auto").await;

    let (mode_hint, suggestion) = if !playable {
        (
            "unknown".to_string(),
            "无法播放：缺少可用音频流或时长无效，请换文件后重试。".to_string(),
        )
    } else if has_dual_roles(Some(&layout)) {
        (
            "enhanced".to_string(),
            "已识别原唱/伴奏双音轨，入库后将进入增强模式（后台抽取音频缓存）。".to_string(),
        )
    } else if layout.tracks.len() >= 2 {
        (
            "unknown".to_string(),
            "检测到多条音轨但未能自动识别角色；可入库后在歌曲编辑页手动映射原唱/伴奏。".to_string(),
        )
    } else {
        (
            "plain".to_string(),
            "普通单轨视频，可直接入库点唱；如需原唱开关，请在工坊用组装或 AI 分轨生成双轨成品。"
                .to_string(),
        )
    };

    PreflightResult {
        playable,
        mode_hint,
        suggestion,
        layout,
    }
}
