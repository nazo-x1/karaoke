//! `karaoke-api`：axum 路由、`ApiResult` envelope、minijinja 模板渲染、静态文件挂载。
//! 对应 Python `main.py` + `karaoke/api/`。

pub mod lenient_bool;
pub mod response;
pub mod routes;
pub mod state;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::Router;
use karaoke_services::AppServices;
use state::AppState;
use std::path::Path;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub fn build_router(
    services: AppServices,
    templates_dir: &Path,
    static_dir: &Path,
) -> anyhow::Result<Router> {
    let templates = state::load_templates(templates_dir)?;
    let app_state = AppState {
        services,
        templates: std::sync::Arc::new(templates),
    };

    let api_v1 = Router::new()
        .route(
            "/library/upload",
            post(routes::library::upload_file).layer(DefaultBodyLimit::disable()),
        )
        .route("/library/scan", post(routes::library::run_scan))
        .route("/library/scan/preview", get(routes::library::preview_scan))
        .route("/library/scan/status", get(routes::library::scan_status))
        .route("/library/songs", get(routes::library::get_list))
        .route(
            "/library/songs/:song_id",
            delete(routes::library::delete_song),
        )
        .route(
            "/songs/:song_id",
            get(routes::config::get_song).patch(routes::config::patch_song),
        )
        .route(
            "/songs/:song_id/detect",
            post(routes::config::detect_playback),
        )
        .route(
            "/songs/:song_id/prepare",
            post(routes::config::prepare_embedded),
        )
        .route(
            "/queue/songs/:song_id",
            post(routes::queue::enqueue).delete(routes::queue::remove),
        )
        .route("/queue", get(routes::queue::list_pending))
        .route("/queue/history", get(routes::queue::list_history))
        .route("/queue/usually", get(routes::queue::list_usually))
        .route("/queue/songs/:song_id/top", post(routes::queue::set_top))
        .route(
            "/playback/songs/:song_id",
            get(routes::playback::get_profile),
        )
        .route(
            "/playback/songs/:song_id/prepare",
            get(routes::playback::get_prepare).post(routes::playback::schedule_prepare),
        )
        .route(
            "/playback/stream/:song_id/:kind",
            get(routes::playback::stream),
        )
        .route(
            "/playback/session/singing/:song_id",
            post(routes::playback::mark_singing),
        )
        .route(
            "/playback/session/finished/:song_id",
            post(routes::playback::mark_finished),
        )
        .route(
            "/playback/session/skip-unready/:song_id",
            post(routes::playback::skip_if_not_ready),
        )
        .route(
            "/playback/songs/:song_id/report-unplayable",
            post(routes::playback::report_unplayable),
        )
        .route("/events", get(routes::events::sse_events))
        .route("/events/command", post(routes::events::send_command))
        .route(
            "/system/play-cache/clear",
            post(routes::system::clear_play_cache),
        )
        .route("/system/features", get(routes::system::features))
        .route("/system/health", get(health))
        .route("/workshop/sessions", post(routes::workshop::create_session))
        .route(
            "/workshop/sessions/:session_id",
            get(routes::workshop::get_session).delete(routes::workshop::destroy_session),
        )
        .route(
            "/workshop/sessions/:session_id/preflight",
            post(routes::workshop::preflight).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/workshop/sessions/:session_id/assemble",
            post(routes::workshop::assemble).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/workshop/sessions/:session_id/ai-separate",
            post(routes::workshop::ai_separate).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/workshop/sessions/:session_id/commit",
            post(routes::workshop::commit),
        )
        .route(
            "/internal/workshop-separation-callback",
            post(routes::workshop::separation_callback),
        );

    let pages = Router::new()
        .route("/", get(routes::pages::index))
        .route("/sing", get(routes::pages::sing_page))
        .route("/song/edit/:song_id", get(routes::pages::song_edit_page))
        .route("/workshop", get(routes::pages::workshop_page));

    let router = Router::new()
        .merge(pages)
        .nest("/api/v1", api_v1)
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    Ok(router)
}

async fn health() -> StatusCode {
    StatusCode::OK
}
