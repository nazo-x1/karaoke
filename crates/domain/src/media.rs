//! 媒体编码判定的纯逻辑部分（对应 Python `media.py` 中不涉及 IO 的分类函数）。
//! 实际调用 ffprobe/ffmpeg 属于 IO，由 `karaoke-infra` 负责；探测结果以
//! [`MediaInfo`] 形式传入本模块做判定。

use serde::{Deserialize, Serialize};

pub const BROWSER_MP4_VIDEO_CODECS: &[&str] = &["h264", "avc1", "avc"];
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

/// 是否需要转码为浏览器可播 H.264（与 Python `_needs_transcode` 等价）。
pub fn stream_needs_transcode(video: &StreamInfo) -> bool {
    needs_transcode(video)
}

/// 音轨是否可直接 copy 进 m4a（AAC）。
pub fn audio_can_copy(codec: &str) -> bool {
    matches!(codec_base(codec).as_str(), "aac" | "mp4a")
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
    fn hevc_requires_transcode() {
        let video = video_stream("hevc", 1920, 1080, "yuv420p");
        assert!(stream_needs_transcode(&video));
    }

    #[test]
    fn ten_bit_h264_requires_transcode() {
        let video = video_stream("h264", 1920, 1080, "yuv420p10le");
        assert!(stream_needs_transcode(&video));
    }

    #[test]
    fn aac_audio_can_copy() {
        assert!(audio_can_copy("aac"));
        assert!(audio_can_copy("mp4a.40.2"));
        assert!(!audio_can_copy("dts"));
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
