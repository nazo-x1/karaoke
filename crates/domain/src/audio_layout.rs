//! 音轨布局：解析/合并/角色判定。对应 Python `karaoke/infra/audio_layout.py`
//! 与 `media.py` 中 `_match_role`/`guess_track_roles`/`build_audio_layout` 的纯逻辑部分。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackRole {
    Vocals,
    Accompaniment,
    Full,
    Ignore,
    Unknown,
}

impl TrackRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrackRole::Vocals => "vocals",
            TrackRole::Accompaniment => "accompaniment",
            TrackRole::Full => "full",
            TrackRole::Ignore => "ignore",
            TrackRole::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "vocals" => Some(TrackRole::Vocals),
            "accompaniment" => Some(TrackRole::Accompaniment),
            "full" => Some(TrackRole::Full),
            "ignore" => Some(TrackRole::Ignore),
            "unknown" => Some(TrackRole::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutType {
    Dual,
    Single,
    Unknown,
}

impl LayoutType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LayoutType::Dual => "dual",
            LayoutType::Single => "single",
            LayoutType::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrack {
    pub index: i32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub channels: i32,
    pub role: TrackRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioLayout {
    pub tracks: Vec<AudioTrack>,
    pub layout: LayoutType,
    #[serde(default = "default_assigned_by")]
    pub assigned_by: String,
    #[serde(default)]
    pub video_stream_index: Option<i32>,
}

fn default_assigned_by() -> String {
    "auto".to_string()
}

const VOCALS_KEYWORDS: &[&str] = &["原唱", "vocal", "vocals", "人声", "演唱", "歌手"];
const ACCOMPANIMENT_KEYWORDS: &[&str] = &[
    "伴奏",
    "instrumental",
    "karaoke",
    "off vocal",
    "off-vocal",
    "accompaniment",
    "bgm",
    "music",
    "カラオケ",
];
const FULL_KEYWORDS: &[&str] = &["complete", "full", "mix", "混音", "完整"];

/// 依据音轨标题/语言标签猜测角色，供探测阶段（infra 已拿到 ffprobe 结果后）调用。
pub fn match_role(title: &str, language: &str) -> TrackRole {
    let text = format!("{title} {language}").to_lowercase();
    if ACCOMPANIMENT_KEYWORDS.iter().any(|k| text.contains(k)) {
        TrackRole::Accompaniment
    } else if VOCALS_KEYWORDS.iter().any(|k| text.contains(k)) {
        TrackRole::Vocals
    } else if FULL_KEYWORDS.iter().any(|k| text.contains(k)) {
        TrackRole::Full
    } else {
        TrackRole::Unknown
    }
}

/// 双轨/单轨兜底猜测：两条都未知则视为人声+伴奏；仅一条则视为完整版。
pub fn guess_track_roles(mut tracks: Vec<AudioTrack>) -> Vec<AudioTrack> {
    if tracks.is_empty() {
        return tracks;
    }
    let has_vocals = tracks.iter().any(|t| t.role == TrackRole::Vocals);
    let has_accomp = tracks.iter().any(|t| t.role == TrackRole::Accompaniment);
    if has_vocals && has_accomp {
        return tracks;
    }
    let unknown_count = tracks
        .iter()
        .filter(|t| t.role == TrackRole::Unknown)
        .count();
    if tracks.len() == 2 && unknown_count == 2 {
        tracks[0].role = TrackRole::Vocals;
        tracks[1].role = TrackRole::Accompaniment;
    } else if tracks.len() == 1 {
        tracks[0].role = TrackRole::Full;
    }
    tracks
}

pub fn has_dual_roles(layout: Option<&AudioLayout>) -> bool {
    let Some(layout) = layout else { return false };
    let has_vocals = layout.tracks.iter().any(|t| t.role == TrackRole::Vocals);
    let has_accomp = layout
        .tracks
        .iter()
        .any(|t| t.role == TrackRole::Accompaniment);
    has_vocals && has_accomp
}

pub fn get_track_index(layout: Option<&AudioLayout>, role: TrackRole) -> Option<i32> {
    layout?
        .tracks
        .iter()
        .find(|t| t.role == role)
        .map(|t| t.index)
}

fn infer_layout_type(tracks: &[AudioTrack]) -> LayoutType {
    let active: Vec<&AudioTrack> = tracks
        .iter()
        .filter(|t| t.role != TrackRole::Ignore)
        .collect();
    let has_vocals = active.iter().any(|t| t.role == TrackRole::Vocals);
    let has_accomp = active.iter().any(|t| t.role == TrackRole::Accompaniment);
    if has_vocals && has_accomp {
        return LayoutType::Dual;
    }
    let non_ignore_unknown: Vec<&&AudioTrack> = active
        .iter()
        .filter(|t| t.role != TrackRole::Unknown)
        .collect();
    if non_ignore_unknown.len() == 1 && non_ignore_unknown[0].role == TrackRole::Full {
        return LayoutType::Single;
    }
    if active.len() == 1 {
        return LayoutType::Single;
    }
    LayoutType::Unknown
}

/// 构建音轨布局（由 infra 在拿到 ffprobe 原始结果后调用，本函数本身不做 IO）。
pub fn build_layout(
    tracks: Vec<AudioTrack>,
    video_stream_index: Option<i32>,
    assigned_by: &str,
) -> AudioLayout {
    let mut tracks = guess_track_roles(tracks);
    let layout_type = infer_layout_type(&tracks);
    if layout_type == LayoutType::Single {
        if let Some(t) = tracks.iter_mut().find(|t| t.role == TrackRole::Unknown) {
            t.role = TrackRole::Full;
        }
    }
    AudioLayout {
        tracks,
        layout: layout_type,
        assigned_by: assigned_by.to_string(),
        video_stream_index,
    }
}

/// 手动指定角色后合并（歌曲编辑页 PATCH audio_tracks 场景）。
pub fn merge_manual_roles(
    current: Option<&AudioLayout>,
    updates: &[(i32, TrackRole)],
) -> AudioLayout {
    let mut by_index: std::collections::BTreeMap<i32, AudioTrack> = current
        .map(|l| l.tracks.iter().map(|t| (t.index, t.clone())).collect())
        .unwrap_or_default();

    for (index, role) in updates {
        by_index
            .entry(*index)
            .and_modify(|t| t.role = *role)
            .or_insert(AudioTrack {
                index: *index,
                title: String::new(),
                language: String::new(),
                codec: String::new(),
                channels: 0,
                role: *role,
            });
    }

    let tracks: Vec<AudioTrack> = by_index.into_values().collect();
    let layout_type = infer_layout_type(&tracks);
    AudioLayout {
        tracks,
        layout: layout_type,
        assigned_by: "manual".to_string(),
        video_stream_index: current.and_then(|l| l.video_stream_index),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutSummary {
    pub layout: String,
    pub assigned_by: String,
    pub track_count: usize,
    pub tracks: Vec<AudioTrack>,
}

pub fn layout_summary(layout: Option<&AudioLayout>) -> LayoutSummary {
    match layout {
        None => LayoutSummary {
            layout: "unknown".to_string(),
            assigned_by: "auto".to_string(),
            track_count: 0,
            tracks: vec![],
        },
        Some(l) => LayoutSummary {
            layout: l.layout.as_str().to_string(),
            assigned_by: l.assigned_by.clone(),
            track_count: l.tracks.len(),
            tracks: l.tracks.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(index: i32, title: &str, role: TrackRole) -> AudioTrack {
        AudioTrack {
            index,
            title: title.to_string(),
            language: String::new(),
            codec: "aac".to_string(),
            channels: 2,
            role,
        }
    }

    #[test]
    fn match_role_detects_vocals_and_accompaniment() {
        assert_eq!(match_role("原唱", ""), TrackRole::Vocals);
        assert_eq!(
            match_role("伴奏 Instrumental", ""),
            TrackRole::Accompaniment
        );
        assert_eq!(match_role("Full Mix", ""), TrackRole::Full);
        assert_eq!(match_role("Track 1", ""), TrackRole::Unknown);
    }

    #[test]
    fn guess_track_roles_assigns_dual_when_two_unknown() {
        let tracks = vec![
            track(0, "", TrackRole::Unknown),
            track(1, "", TrackRole::Unknown),
        ];
        let guessed = guess_track_roles(tracks);
        assert_eq!(guessed[0].role, TrackRole::Vocals);
        assert_eq!(guessed[1].role, TrackRole::Accompaniment);
    }

    #[test]
    fn guess_track_roles_single_track_becomes_full() {
        let tracks = vec![track(0, "", TrackRole::Unknown)];
        let guessed = guess_track_roles(tracks);
        assert_eq!(guessed[0].role, TrackRole::Full);
    }

    #[test]
    fn has_dual_roles_requires_both_vocals_and_accompaniment() {
        let layout = build_layout(
            vec![
                track(0, "原唱", TrackRole::Vocals),
                track(1, "伴奏", TrackRole::Accompaniment),
            ],
            Some(0),
            "auto",
        );
        assert!(has_dual_roles(Some(&layout)));
        assert_eq!(layout.layout, LayoutType::Dual);
    }

    #[test]
    fn merge_manual_roles_updates_existing_and_adds_new() {
        let current = build_layout(vec![track(0, "", TrackRole::Unknown)], None, "auto");
        let merged = merge_manual_roles(
            Some(&current),
            &[(0, TrackRole::Vocals), (1, TrackRole::Accompaniment)],
        );
        assert_eq!(merged.assigned_by, "manual");
        assert_eq!(merged.tracks.len(), 2);
        assert!(has_dual_roles(Some(&merged)));
    }

    #[test]
    fn get_track_index_finds_role() {
        let layout = build_layout(
            vec![
                track(3, "原唱", TrackRole::Vocals),
                track(4, "伴奏", TrackRole::Accompaniment),
            ],
            None,
            "auto",
        );
        assert_eq!(get_track_index(Some(&layout), TrackRole::Vocals), Some(3));
        assert_eq!(
            get_track_index(Some(&layout), TrackRole::Accompaniment),
            Some(4)
        );
        assert_eq!(get_track_index(None, TrackRole::Vocals), None);
    }
}
