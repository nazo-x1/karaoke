//! 播放资源准备判定。对应 Python `karaoke/domain/prepare_policy.py`。

use crate::playback::{PlaybackProfile, PlaybackSource};

/// 判断是否需要后台 prepare。
///
/// 仅 embedded（双轨）且音频缓存未就绪时需要；plain 默认直发源文件，永不主动 prepare
/// （兜底转码仅由播放失败上报被动触发）。
pub fn needs_prepare(profile: &PlaybackProfile, layout_has_dual_roles: bool) -> bool {
    matches!(profile.playback_source, PlaybackSource::Embedded)
        && layout_has_dual_roles
        && !profile.embedded_cache_ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::{resolve, PlaybackInput};

    #[test]
    fn plain_mode_never_needs_prepare() {
        let input = PlaybackInput {
            source_path: "/KTV/a.mkv".into(),
            source_ext: "mkv".into(),
            has_source_file: true,
            is_playable: true,
            ..Default::default()
        };
        let profile = resolve(&input);
        assert!(!needs_prepare(&profile, false));
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
            source_path: "/KTV/a.mkv".into(),
            source_ext: "mkv".into(),
            has_source_file: true,
            audio_layout: Some(layout),
            embedded: Some(EmbeddedAvailability {
                paths: EmbeddedTriplet::default(),
                ready: false,
            }),
            ..Default::default()
        };
        let profile = resolve(&input);
        assert!(needs_prepare(&profile, true));
    }
}
