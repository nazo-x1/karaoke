//! 点歌队列路由。对应 Python `karaoke/api/routes/queue.py`。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

pub async fn enqueue(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.queue.enqueue(song_id).await.into()
}

pub async fn list_pending(State(state): State<AppState>) -> ApiJson {
    state.services.queue.list_pending().await.into()
}

#[derive(Deserialize)]
pub struct PageQuery {
    #[serde(default = "default_page")]
    page: i64,
}

fn default_page() -> i64 {
    1
}

pub async fn list_history(State(state): State<AppState>, Query(q): Query<PageQuery>) -> ApiJson {
    state.services.queue.list_history(q.page).await.into()
}

pub async fn list_usually(State(state): State<AppState>, Query(q): Query<PageQuery>) -> ApiJson {
    state.services.queue.list_usually(q.page).await.into()
}

pub async fn set_top(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.queue.set_top(song_id).await.into()
}

pub async fn remove(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.queue.remove(song_id).await.into()
}
