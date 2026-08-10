//! 系统维护路由。对应 Python `karaoke/api/routes/system.py`。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::extract::State;

pub async fn clear_play_cache(State(state): State<AppState>) -> ApiJson {
    state.services.cache.clear_play_cache().await.into()
}
