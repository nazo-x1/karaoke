//! 纯业务逻辑：不依赖任何 IO（文件系统/数据库/子进程）。
//!
//! 所有需要探测文件系统或运行 ffprobe 才能得到的事实，均由调用方（infra/services 层）
//! 计算完成后以普通数据结构传入，本 crate 只负责基于事实做出的判定与转换。
//! Cargo 依赖图上本 crate 不允许出现 `sqlx`/`tokio`，从编译期保证分层不被破坏。

pub mod audio_layout;
pub mod media;
pub mod playback;
pub mod prepare_policy;
pub mod queue_policy;

pub use audio_layout::{
    get_track_index, match_role, AudioLayout, AudioTrack, LayoutType, TrackRole,
};
pub use media::{audio_can_copy, file_ext, is_valid_browser_mp4, MediaInfo, StreamInfo};
pub use playback::{
    resolve, EmbeddedAvailability, EmbeddedTriplet, PlaybackInput, PlaybackMode, PlaybackProfile,
    PlaybackSource,
};
pub use prepare_policy::needs_prepare;
pub use queue_policy::{queue_state_label, sort_pending, QueueSortItem, QueueState};
