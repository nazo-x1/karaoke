//! 基础设施层：数据库仓储、ffmpeg/ffprobe 执行、目录扫描、内嵌缓存、流式响应。
//! 依赖 `karaoke-domain` 做决策，自身只负责 IO。

pub mod config;
pub mod db;
pub mod embedded;
pub mod media;
pub mod models;
pub mod playback_resolver;
pub mod repositories;
pub mod scanner;
pub mod separator_client;
pub mod streaming;
pub mod workshop;

pub use config::AppConfig;
pub use media::MediaSettings;
pub use playback_resolver::PlaybackResolver;
pub use separator_client::{SeparatorClient, SeparatorError, SeparatorJobStatus};
pub use workshop::{WorkshopPreflight, WorkshopSessionMeta, WorkshopStatus, WorkshopStore};
