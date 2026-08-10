//! `ApiResult` -> HTTP 响应映射。与 Python 版一致：业务成功/失败统一走 200 + envelope
//! （`code` 字段区分），只有资源类响应（流媒体、缓存未就绪）才使用非 200 状态码。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use karaoke_services::ApiResult;

pub struct ApiJson(pub ApiResult);

impl IntoResponse for ApiJson {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self.0)).into_response()
    }
}

impl From<ApiResult> for ApiJson {
    fn from(value: ApiResult) -> Self {
        ApiJson(value)
    }
}
