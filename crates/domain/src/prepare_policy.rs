//! 播放资源准备判定。对应 Python `karaoke/domain/prepare_policy.py`。

use crate::playback::{PlaybackMode, PlaybackProfile, PlaybackSource};

/// 判断是否需要后台 prepare（内嵌拆轨或 plain 浏览器转码）。
///
/// - `override_complete`：`__override__` 三件套是否齐全（齐全则永不需要 prepare）。
/// - `layout_has_dual_roles`：当前音轨布局是否为双轨（由 infra 解析 `audio_layout` 得出）。
/// - `can_play_directly`：源文件是否可被浏览器直接播放（由 infra 调 ffprobe 得出）。
pub fn needs_prepare(
    profile: &PlaybackProfile,
    override_complete: bool,
    layout_has_dual_roles: bool,
    can_play_directly: bool,
) -> bool {
    if override_complete {
        return false;
    }
    match profile.playback_source {
        PlaybackSource::Embedded => layout_has_dual_roles && !profile.embedded_cache_ready,
        _ => {
            profile.mode == PlaybackMode::Plain
                && profile.video_path.is_some()
                && !can_play_directly
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::{resolve, PlaybackInput};

    #[test]
    fn override_complete_never_needs_prepare() {
        let profile = resolve(&PlaybackInput::default());
        assert!(!needs_prepare(&profile, true, false, false));
    }

    #[test]
    fn plain_mode_needs_prepare_when_not_directly_playable() {
        let input = PlaybackInput {
            source_path: "/KTV/a.mkv".into(),
            source_ext: "mkv".into(),
            has_source_file: true,
            is_playable: true,
            ..Default::default()
        };
        let profile = resolve(&input);
        assert!(needs_prepare(&profile, false, false, false));
        assert!(!needs_prepare(&profile, false, false, true));
    }

    #[test]
    fn embedded_needs_prepare_until_cache_ready() {
        use crate::audio_layout::{build_layout, AudioTrack, TrackRole};
        use crate::playback::{EmbeddedAvailability, EmbeddedTriplet};

        let layout = build_layout(
            vec![
                AudioTrack {
                    index: 1,
                    title: "".into(),
                    language: "".into(),
                    codec: "aac".into(),
                    channels: 2,
                    role: TrackRole::Vocals,
                },
                AudioTrack {
                    index: 2,
                    title: "".into(),
                    language: "".into(),
                    codec: "aac".into(),
                    channels: 2,
                    role: TrackRole::Accompaniment,
                },
            ],
            None,
            "auto",
        );
        let input = PlaybackInput {
            has_source_file: true,
            audio_layout: Some(layout),
            embedded: Some(EmbeddedAvailability {
                paths: EmbeddedTriplet::default(),
                ready: false,
            }),
            ..Default::default()
        };
        let profile = resolve(&input);
        assert!(needs_prepare(&profile, false, true, false));
    }
}
