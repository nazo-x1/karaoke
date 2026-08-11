//! 曲库路由。对应 Python `karaoke/api/routes/library.py`。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::extract::{Multipart, Path, Query, State};
use karaoke_services::ApiResult;
use serde::Deserialize;
use serde_json::Value;

pub async fn upload_file(State(state): State<AppState>, mut multipart: Multipart) -> ApiJson {
    let mut filename: Option<String> = None;
    let mut bytes: Vec<u8> = Vec::new();
    let mut duplicate_policy: Option<String> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return ApiResult::fail(format!("解析上传表单失败: {e}")).into(),
        };
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = field.file_name().map(|s| s.to_string());
            bytes = match field.bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => return ApiResult::fail(format!("读取上传内容失败: {e}")).into(),
            };
        } else if name == "duplicate_policy" {
            duplicate_policy = field.text().await.ok();
        }
    }

    let Some(filename) = filename.filter(|f| !f.is_empty()) else {
        return ApiResult::fail("未选择文件").into();
    };
    state
        .services
        .library
        .upload_file(&filename, duplicate_policy.as_deref(), bytes)
        .await
        .into()
}

#[derive(Deserialize)]
pub struct ScanBody {
    root: Option<String>,
    duplicate_policy: Option<String>,
    validate: Option<bool>,
}

pub async fn run_scan(State(state): State<AppState>, body: Option<axum::Json<Value>>) -> ApiJson {
    let body: ScanBody = body
        .map(|axum::Json(v)| ScanBody {
            root: v.get("root").and_then(Value::as_str).map(str::to_string),
            duplicate_policy: v
                .get("duplicate_policy")
                .and_then(Value::as_str)
                .map(str::to_string),
            validate: v.get("validate").and_then(Value::as_bool),
        })
        .unwrap_or(ScanBody {
            root: None,
            duplicate_policy: None,
            validate: None,
        });
    state
        .services
        .library
        .run_scan(
            body.root.as_deref().unwrap_or(""),
            body.duplicate_policy.as_deref(),
            body.validate,
        )
        .await
        .into()
}

#[derive(Deserialize)]
pub struct PreviewScanQuery {
    #[serde(default)]
    root: String,
    duplicate_policy: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::lenient_bool::deserialize_optional"
    )]
    validate: Option<bool>,
}

pub async fn preview_scan(
    State(state): State<AppState>,
    Query(q): Query<PreviewScanQuery>,
) -> ApiJson {
    state
        .services
        .library
        .preview_scan(&q.root, q.duplicate_policy.as_deref(), q.validate)
        .await
        .into()
}

pub async fn scan_status(State(state): State<AppState>) -> ApiJson {
    ApiResult::ok_with_data(state.services.library.scan_status()).into()
}

#[derive(Deserialize)]
pub struct SongListQuery {
    #[serde(default)]
    q: String,
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default)]
    page_size: i64,
}

fn default_page() -> i64 {
    1
}

pub async fn get_list(State(state): State<AppState>, Query(q): Query<SongListQuery>) -> ApiJson {
    state
        .services
        .library
        .get_list(&q.q, q.page, q.page_size)
        .await
        .into()
}

#[derive(Deserialize)]
pub struct DeleteSongQuery {
    #[serde(default, deserialize_with = "crate::lenient_bool::deserialize")]
    delete_disk: bool,
}

pub async fn delete_song(
    State(state): State<AppState>,
    Path(song_id): Path<i64>,
    Query(q): Query<DeleteSongQuery>,
) -> ApiJson {
    state
        .services
        .library
        .delete_song(song_id, q.delete_disk)
        .await
        .into()
}
