//! 歌曲配置路由。对应 Python `karaoke/api/routes/config.py`。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::Value;

pub async fn get_song(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.song_config.get_detail(song_id).await.into()
}

pub async fn patch_song(
    State(state): State<AppState>,
    Path(song_id): Path<i64>,
    body: Option<axum::Json<Value>>,
) -> ApiJson {
    let body = body.map(|axum::Json(v)| v).unwrap_or(Value::Null);
    state
        .services
        .song_config
        .patch(song_id, &body)
        .await
        .into()
}

pub async fn detect_playback(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state
        .services
        .song_config
        .detect_playback(song_id)
        .await
        .into()
}

#[derive(Deserialize)]
pub struct PrepareQuery {
    // 前端历史写法为 `?wait=1`（见 app/static/js/domains/config.js），需宽松解析。
    #[serde(default, deserialize_with = "crate::lenient_bool::deserialize")]
    wait: bool,
}

pub async fn prepare_embedded(
    State(state): State<AppState>,
    Path(song_id): Path<i64>,
    Query(q): Query<PrepareQuery>,
) -> ApiJson {
    state
        .services
        .song_config
        .request_prepare(song_id, q.wait)
        .await
        .into()
}
