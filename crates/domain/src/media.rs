//! 媒体编码判定的纯逻辑部分（对应 Python `media.py` 中不涉及 IO 的分类函数）。
//! 实际调用 ffprobe/ffmpeg 属于 IO，由 `karaoke-infra` 负责；探测结果以
//! [`MediaInfo`] 形式传入本模块做判定。

use serde::{Deserialize, Serialize};

pub const NATIVE_VIDEO_EXTS: &[&str] = &["mp4", "m4v", "webm", "mov"];
pub const BROWSER_MP4_VIDEO_CODECS: &[&str] = &["h264", "avc1", "avc"];
pub const BROWSER_AUDIO_CODECS: &[&str] = &["aac", "mp3", "mp4a", "opus", "vorbis", "flac"];
pub const TRANSCODE_VIDEO_CODECS: &[&str] = &[
    "hevc",
    "h265",
    "mpeg4",
    "msmpeg4v3",
    "msmpeg4v2",
    "mpeg2video",
    "vc1",
    "wmv3",
    "wmv2",
    "rv40",
    "theora",
    "vp8",
    "vp9",
    "av1",
    "mjpeg",
    "png",
    "bmp",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamInfo {
    pub index: i32,
    pub codec_type: String,
    pub codec_name: String,
    pub width: i32,
    pub height: i32,
    pub pix_fmt: String,
    pub attached_pic: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub ext: String,
    pub format_name: String,
    pub duration: f64,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub has_video: bool,
    pub has_audio: bool,
    pub video_width: i32,
    pub video_height: i32,
    pub pix_fmt: String,
    pub streams: Vec<StreamInfo>,
}

fn codec_base(codec: &str) -> String {
    codec.split('.').next().unwrap_or("").to_lowercase()
}

fn is_10bit_or_highpix(pix_fmt: &str) -> bool {
    if pix_fmt.is_empty() {
        return false;
    }
    pix_fmt.contains("10")
        || pix_fmt.contains("12")
        || matches!(pix_fmt, "yuv422p" | "yuv444p" | "gbrp")
}

pub fn pick_main_video_stream(streams: &[StreamInfo]) -> Option<&StreamInfo> {
    let mut candidates: Vec<&StreamInfo> = streams
        .iter()
        .filter(|s| {
            s.codec_type == "video"
                && !s.attached_pic
                && s.width >= 32
                && s.height >= 32
                && !matches!(s.codec_name.as_str(), "mjpeg" | "png" | "bmp" | "gif")
        })
        .collect();
    if candidates.is_empty() {
        candidates = streams
            .iter()
            .filter(|s| s.codec_type == "video" && !s.attached_pic)
            .collect();
    }
    candidates.into_iter().max_by_key(|s| s.width * s.height)
}

pub fn pick_main_audio_stream(streams: &[StreamInfo]) -> Option<&StreamInfo> {
    streams.iter().find(|s| s.codec_type == "audio")
}

fn needs_transcode(video: &StreamInfo) -> bool {
    let codec = codec_base(&video.codec_name);
    if TRANSCODE_VIDEO_CODECS.contains(&codec.as_str()) {
        return true;
    }
    if BROWSER_MP4_VIDEO_CODECS.contains(&codec.as_str()) {
        return is_10bit_or_highpix(&video.pix_fmt);
    }
    true
}

fn codec_supported(codec: Option<&str>, allowed: &[&str]) -> bool {
    match codec {
        None => true,
        Some(c) => allowed.contains(&codec_base(c).as_str()),
    }
}

/// 是否需要转码为浏览器可播 H.264（与 Python `_needs_transcode` 等价）。
pub fn stream_needs_transcode(video: &StreamInfo) -> bool {
    needs_transcode(video)
}

/// 源文件是否可被浏览器直接播放，无需转码（对应 Python `can_play_directly`）。
pub fn can_play_directly(info: &MediaInfo) -> bool {
    if !info.has_video {
        return false;
    }
    if !NATIVE_VIDEO_EXTS.contains(&info.ext.as_str()) {
        return false;
    }
    let Some(video) = pick_main_video_stream(&info.streams) else {
        return false;
    };
    if needs_transcode(video) {
        return false;
    }
    if info.has_audio && !codec_supported(info.audio_codec.as_deref(), BROWSER_AUDIO_CODECS) {
        return false;
    }
    true
}

/// 转码产物校验：是否是浏览器可播的 H.264 mp4（对应 Python `_validate_browser_mp4`）。
pub fn is_valid_browser_mp4(info: &MediaInfo) -> bool {
    if !info.has_video {
        return false;
    }
    let Some(video) = pick_main_video_stream(&info.streams) else {
        return false;
    };
    if video.width < 32 || video.height < 32 {
        return false;
    }
    let codec = codec_base(&video.codec_name);
    if !BROWSER_MP4_VIDEO_CODECS.contains(&codec.as_str()) {
        return false;
    }
    !is_10bit_or_highpix(&video.pix_fmt)
}

pub fn file_ext(path: &str) -> String {
    path.rsplit('.')
        .next()
        .filter(|_| path.contains('.'))
        .unwrap_or("")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_stream(codec: &str, w: i32, h: i32, pix_fmt: &str) -> StreamInfo {
        StreamInfo {
            index: 0,
            codec_type: "video".into(),
            codec_name: codec.into(),
            width: w,
            height: h,
            pix_fmt: pix_fmt.into(),
            attached_pic: false,
        }
    }

    #[test]
    fn h264_native_mp4_plays_directly() {
        let info = MediaInfo {
            ext: "mp4".into(),
            has_video: true,
            has_audio: true,
            audio_codec: Some("aac".into()),
            streams: vec![video_stream("h264", 1920, 1080, "yuv420p")],
            ..Default::default()
        };
        assert!(can_play_directly(&info));
    }

    #[test]
    fn hevc_requires_transcode() {
        let info = MediaInfo {
            ext: "mp4".into(),
            has_video: true,
            streams: vec![video_stream("hevc", 1920, 1080, "yuv420p")],
            ..Default::default()
        };
        assert!(!can_play_directly(&info));
    }

    #[test]
    fn ten_bit_h264_requires_transcode() {
        let info = MediaInfo {
            ext: "mp4".into(),
            has_video: true,
            streams: vec![video_stream("h264", 1920, 1080, "yuv420p10le")],
            ..Default::default()
        };
        assert!(!can_play_directly(&info));
    }

    #[test]
    fn mkv_container_always_needs_remux() {
        let info = MediaInfo {
            ext: "mkv".into(),
            has_video: true,
            streams: vec![video_stream("h264", 1920, 1080, "yuv420p")],
            ..Default::default()
        };
        assert!(!can_play_directly(&info));
    }

    #[test]
    fn unsupported_audio_codec_blocks_direct_play() {
        let info = MediaInfo {
            ext: "mp4".into(),
            has_video: true,
            has_audio: true,
            audio_codec: Some("dts".into()),
            streams: vec![video_stream("h264", 1920, 1080, "yuv420p")],
            ..Default::default()
        };
        assert!(!can_play_directly(&info));
    }

    #[test]
    fn attached_pic_stream_is_excluded_from_main_video() {
        let streams = vec![
            StreamInfo {
                attached_pic: true,
                ..video_stream("mjpeg", 200, 200, "")
            },
            video_stream("h264", 1280, 720, "yuv420p"),
        ];
        let picked = pick_main_video_stream(&streams).unwrap();
        assert_eq!(picked.codec_name, "h264");
    }

    #[test]
    fn file_ext_lowercases_and_strips_dot() {
        assert_eq!(file_ext("/a/b/Song.MP4"), "mp4");
        assert_eq!(file_ext("noext"), "");
    }
}
