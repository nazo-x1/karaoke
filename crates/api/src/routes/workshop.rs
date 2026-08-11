//! 上传编辑页（工坊）临时会话 API。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::HeaderMap;
use karaoke_services::ApiResult;
use serde_json::Value;

pub async fn create_session(State(state): State<AppState>) -> ApiJson {
    state.services.workshop.create_session().await.into()
}

pub async fn get_session(State(state): State<AppState>, Path(session_id): Path<String>) -> ApiJson {
    state
        .services
        .workshop
        .get_session(&session_id)
        .await
        .into()
}

pub async fn destroy_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiJson {
    state
        .services
        .workshop
        .destroy_session(&session_id)
        .await
        .into()
}

pub async fn preflight(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    mut multipart: Multipart,
) -> ApiJson {
    let mut filename = String::new();
    let mut bytes = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return ApiResult::fail(format!("解析上传表单失败: {e}")).into(),
        };
        if field.name() == Some("file") {
            filename = field.file_name().unwrap_or("video").to_string();
            bytes = match field.bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => return ApiResult::fail(format!("读取上传内容失败: {e}")).into(),
            };
        }
    }
    state
        .services
        .workshop
        .preflight(&session_id, &filename, bytes)
        .await
        .into()
}

pub async fn assemble(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    mut multipart: Multipart,
) -> ApiJson {
    let mut video_name = String::new();
    let mut video = Vec::new();
    let mut vocals_name = String::new();
    let mut vocals = Vec::new();
    let mut accomp_name = String::new();
    let mut accomp = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return ApiResult::fail(format!("解析上传表单失败: {e}")).into(),
        };
        let name = field.name().unwrap_or("").to_string();
        let fname = field.file_name().unwrap_or("file").to_string();
        let data = match field.bytes().await {
            Ok(b) => b.to_vec(),
            Err(e) => return ApiResult::fail(format!("读取上传内容失败: {e}")).into(),
        };
        match name.as_str() {
            "video" => {
                video_name = fname;
                video = data;
            }
            "vocals" => {
                vocals_name = fname;
                vocals = data;
            }
            "accompaniment" | "instrumental" => {
                accomp_name = fname;
                accomp = data;
            }
            _ => {}
        }
    }

    state
        .services
        .workshop
        .assemble(
            &session_id,
            &video_name,
            video,
            &vocals_name,
            vocals,
            &accomp_name,
            accomp,
        )
        .await
        .into()
}

pub async fn ai_separate(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    mut multipart: Multipart,
) -> ApiJson {
    let mut filename = String::new();
    let mut bytes = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return ApiResult::fail(format!("解析上传表单失败: {e}")).into(),
        };
        if field.name() == Some("file") || field.name() == Some("video") {
            filename = field.file_name().unwrap_or("video").to_string();
            bytes = match field.bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => return ApiResult::fail(format!("读取上传内容失败: {e}")).into(),
            };
        }
    }
    state
        .services
        .workshop
        .ai_separate(&session_id, &filename, bytes)
        .await
        .into()
}

pub async fn commit(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    body: Option<axum::Json<Value>>,
) -> ApiJson {
    let policy = body.and_then(|axum::Json(v)| {
        v.get("duplicate_policy")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    state
        .services
        .workshop
        .commit(&session_id, policy.as_deref())
        .await
        .into()
}

pub async fn separation_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::Json<Value>,
) -> ApiJson {
    let token = headers
        .get("x-separator-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
        });
    let job_id = body
        .get("job_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if job_id.is_empty() {
        return ApiResult::fail("缺少 job_id").into();
    }
    state
        .services
        .workshop
        .handle_separator_callback(&job_id, &status, token)
        .await
        .into()
}
