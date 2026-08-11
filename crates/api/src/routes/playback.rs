//! 播放路由：播放配置/准备状态/流媒体/会话状态。对应 Python `karaoke/api/routes/playback.py`。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use karaoke_infra::streaming::{compute_range, stream_file_range};
use karaoke_services::{ApiResult, StreamOutcome};

pub async fn get_profile(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.playback.get_profile(song_id).await.into()
}

pub async fn get_prepare(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.playback.get_prepare(song_id).await.into()
}

pub async fn schedule_prepare(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state
        .services
        .playback
        .schedule_prepare(song_id)
        .await
        .into()
}

pub async fn stream(
    State(state): State<AppState>,
    Path((song_id, kind)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Response {
    match state.services.playback.stream(song_id, &kind).await {
        StreamOutcome::NotFound => ApiJson(ApiResult::not_found("歌曲")).into_response(),
        StreamOutcome::Invalid(msg) => ApiJson(ApiResult::fail(msg)).into_response(),
        StreamOutcome::CacheNotReady(prep) => {
            let body = serde_json::json!({
                "code": 1,
                "msg": "内嵌缓存未就绪，请等待后台生成完成",
                "data": prep,
            });
            (StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response()
        }
        StreamOutcome::File { path, media_type } => {
            build_stream_response(&path, &media_type, &headers).await
        }
    }
}

async fn build_stream_response(
    path: &std::path::Path,
    media_type: &str,
    headers: &HeaderMap,
) -> Response {
    let file_size = match tokio::fs::metadata(path).await {
        Ok(meta) => meta.len(),
        Err(e) => {
            return ApiJson(ApiResult::fail(format!("获取播放流失败: {e}"))).into_response();
        }
    };

    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let range = compute_range(file_size, range_header);
    let status = if range.is_partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let disposition = format!(
        "inline; filename*=UTF-8''{}",
        urlencoding::encode(&filename)
    );

    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(v) = HeaderValue::from_str(&disposition) {
        response_headers.insert(header::CONTENT_DISPOSITION, v);
    }
    if let Ok(v) = HeaderValue::from_str(media_type) {
        response_headers.insert(header::CONTENT_TYPE, v);
    }
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&range.content_length().to_string()).unwrap(),
    );
    if range.is_partial {
        if let Ok(v) = HeaderValue::from_str(&format!(
            "bytes {}-{}/{}",
            range.start, range.end, file_size
        )) {
            response_headers.insert(header::CONTENT_RANGE, v);
        }
    }

    let stream = stream_file_range(path.to_path_buf(), range.start, range.end);
    let body = Body::from_stream(stream);
    (status, response_headers, body).into_response()
}

pub async fn mark_singing(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.playback.mark_singing(song_id).await.into()
}

pub async fn mark_finished(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state.services.playback.mark_finished(song_id).await.into()
}

pub async fn skip_if_not_ready(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state
        .services
        .playback
        .skip_if_not_ready(song_id)
        .await
        .into()
}

pub async fn report_unplayable(State(state): State<AppState>, Path(song_id): Path<i64>) -> ApiJson {
    state
        .services
        .playback
        .report_unplayable(song_id)
        .await
        .into()
}
