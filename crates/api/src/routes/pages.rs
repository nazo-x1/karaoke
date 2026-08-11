//! 页面路由：首页 / 遥控播放页 / 歌曲编辑页 / 上传编辑页（工坊）。

use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use minijinja::context;

fn render(state: &AppState, name: &str, ctx: minijinja::Value) -> Response {
    match state
        .templates
        .get_template(name)
        .and_then(|tpl| tpl.render(ctx))
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("render template {name} failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "template render failed").into_response()
        }
    }
}

pub async fn index(State(state): State<AppState>) -> Response {
    render(&state, "index.html", context! { prefix => "" })
}

pub async fn sing_page(State(state): State<AppState>) -> Response {
    render(&state, "playing.html", context! { prefix => "" })
}

pub async fn song_edit_page(State(state): State<AppState>, Path(song_id): Path<i64>) -> Response {
    render(
        &state,
        "song_edit.html",
        context! { prefix => "", song_id => song_id },
    )
}

pub async fn workshop_page(State(state): State<AppState>) -> Response {
    render(&state, "workshop.html", context! { prefix => "" })
}
