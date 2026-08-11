//! 应用服务编排层：镜像 Python 版业务服务边界
//! （library/queue/playback/song_config/cache/workshop），组合 domain + infra + events + jobs。

pub mod cache_service;
pub mod dto;
pub mod library_service;
pub mod mappers;
pub mod playback_service;
pub mod queue_service;
pub mod song_config_service;
pub mod workshop_service;

pub use cache_service::CacheService;
pub use dto::ApiResult;
pub use library_service::LibraryService;
pub use playback_service::{PlaybackService, StreamOutcome};
pub use queue_service::QueueService;
pub use song_config_service::SongConfigService;
pub use workshop_service::WorkshopService;

use karaoke_events::EventBus;
use karaoke_infra::repositories::{HistoryRepository, SongRepository};
use karaoke_infra::scanner::ScanTaskManager;
use karaoke_infra::{AppConfig, MediaSettings, PlaybackResolver, WorkshopStore};
use karaoke_jobs::PrepareTaskManager;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;

/// 一次性装配所有服务，供 `karaoke-api`/`karaoke-app` 通过 `axum::extract::State` 共享。
#[derive(Clone)]
pub struct AppServices {
    pub library: LibraryService,
    pub queue: QueueService,
    pub playback: PlaybackService,
    pub song_config: SongConfigService,
    pub cache: CacheService,
    pub workshop: WorkshopService,
    pub events: EventBus,
    pub config: Arc<AppConfig>,
}

impl AppServices {
    pub fn new(pool: PgPool, config: Arc<AppConfig>) -> Self {
        let songs = SongRepository::new(pool.clone());
        let histories = HistoryRepository::new(pool);
        let media = MediaSettings::new(
            config.play_cache_path.clone(),
            config.prepare_max_concurrent,
            config.probe_size,
            config.analyze_duration,
            config.transcode_max_height,
            config.scan_validate_concurrency,
        );
        let resolver = PlaybackResolver::new(media.clone());
        let events = EventBus::new(karaoke_events::DEFAULT_MAX_CLIENTS);
        let prepare = PrepareTaskManager::new(
            resolver.clone(),
            songs.clone(),
            events.clone(),
            config.prepare_max_concurrent,
        );
        let scan_tasks = ScanTaskManager::new();

        let scan_video_exts: HashSet<String> = config.scan_video_exts.iter().cloned().collect();
        let skip_dir_names = config.scan_skip_dir_names();

        let library = LibraryService {
            songs: songs.clone(),
            resolver: resolver.clone(),
            prepare: prepare.clone(),
            media: media.clone(),
            scan_tasks,
            keep_path: config.keep_path.clone(),
            scan_video_exts,
            skip_dir_names,
            default_duplicate_policy: config.default_duplicate_policy.clone(),
            ffprobe_on_import: config.ffprobe_on_import,
        };

        let workshop_store = WorkshopStore::new(
            config.workshop_path.clone(),
            config.workshop.session_ttl_secs,
        );
        let workshop = WorkshopService::new(
            workshop_store,
            media.clone(),
            library.clone(),
            config.separator.clone(),
        );

        let queue = QueueService {
            songs: songs.clone(),
            histories: histories.clone(),
            resolver: resolver.clone(),
            prepare: prepare.clone(),
            events: events.clone(),
        };
        let playback = PlaybackService {
            songs: songs.clone(),
            histories: histories.clone(),
            resolver: resolver.clone(),
            prepare: prepare.clone(),
            events: events.clone(),
        };
        let song_config = SongConfigService {
            songs: songs.clone(),
            histories: histories.clone(),
            resolver,
            prepare: prepare.clone(),
        };
        let cache = CacheService {
            songs,
            histories,
            prepare,
            media,
        };

        Self {
            library,
            queue,
            playback,
            song_config,
            cache,
            workshop,
            events,
            config,
        }
    }

    pub async fn init_on_startup(&self) {
        self.queue.init_on_startup().await;
        // 启动期补偿：为未就绪的双轨歌曲调度音频抽取预热。
        self.library.warmup_needing_prepare().await;
        // 清理过期工坊会话
        let removed = self.workshop.store.cleanup_expired().await;
        if removed > 0 {
            tracing::info!(removed, "cleaned expired workshop sessions");
        }
        // 后台定期清理
        let workshop = self.workshop.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                let n = workshop.store.cleanup_expired().await;
                if n > 0 {
                    tracing::info!(removed = n, "workshop TTL cleanup");
                }
            }
        });
    }
}
