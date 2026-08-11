//! 系统维护路由。对应 Python `karaoke/api/routes/system.py`。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::extract::State;
use karaoke_services::ApiResult;
use serde_json::json;

pub async fn clear_play_cache(State(state): State<AppState>) -> ApiJson {
    state.services.cache.clear_play_cache().await.into()
}

/// 前端能力开关：是否显示工坊 AI 等。
pub async fn features(State(state): State<AppState>) -> ApiJson {
    let cfg = &state.services.config;
    ApiResult::ok_with_data(json!({
        "separator_enabled": state.services.workshop.separator_enabled(),
        "separator": {
            "enabled": cfg.separator.enabled,
            "base_url": cfg.separator.base_url,
            "default_model": cfg.separator.default_model,
            "max_concurrent": cfg.separator.max_concurrent,
            "callback_configured": !cfg.separator.callback_base_url.is_empty(),
        },
        "workshop": {
            "dir_name": cfg.workshop.dir_name,
            "session_ttl_secs": cfg.workshop.session_ttl_secs,
        },
    }))
    .into()
}
