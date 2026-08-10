//! ffmpeg/ffprobe 执行层：全部走 `tokio::process`，配合硬超时 + 信号量，
//! 修复 Python 版「同步子进程占用线程池、超时对卡死进程失效」的问题（P0）。

pub mod probe;
pub mod transcode;

pub use probe::{probe_audio_tracks, probe_media_info, probe_streams, probe_video_playable};
pub use transcode::{
    browser_mp4_cache_path, can_play_directly, ensure_browser_mp4_cache,
    resolve_browser_video_path_readonly, validate_browser_mp4, ProgressFn,
};

use std::sync::Arc;
use tokio::sync::Semaphore;

/// ffprobe/ffmpeg 共享的并发与超时策略。
#[derive(Clone)]
pub struct MediaSettings {
    pub play_cache_path: std::path::PathBuf,
    pub probe_semaphore: Arc<Semaphore>,
    pub transcode_semaphore: Arc<Semaphore>,
    pub probe_timeout: std::time::Duration,
    pub transcode_timeout: std::time::Duration,
}

impl MediaSettings {
    pub fn new(play_cache_path: std::path::PathBuf, transcode_concurrency: usize) -> Self {
        let cpu_probe_limit = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(1, 4);
        Self {
            play_cache_path,
            probe_semaphore: Arc::new(Semaphore::new(cpu_probe_limit)),
            transcode_semaphore: Arc::new(Semaphore::new(transcode_concurrency.max(1))),
            probe_timeout: std::time::Duration::from_secs(30),
            transcode_timeout: std::time::Duration::from_secs(3600),
        }
    }
}
