//! SSE 事件与遥控指令路由。对应 Python `karaoke/api/routes/events.py`。

use crate::response::ApiJson;
use crate::state::AppState;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use karaoke_events::{heartbeat_payload, resync_hint_payload, HEARTBEAT_INTERVAL_SECS};
use karaoke_services::ApiResult;
use serde_json::Value;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

pub async fn sse_events(State(state): State<AppState>) -> Response {
    let Some(client) = state.services.events.try_subscribe() else {
        return ApiJson(ApiResult::fail("当前连接数已达上限，请稍后重试")).into_response();
    };

    let stream = async_stream::stream! {
        // 持有订阅句柄直至流结束（客户端断开/Drop），离开作用域时自动从在线计数中移除。
        let mut client = client;
        loop {
            let outcome = tokio::time::timeout(
                Duration::from_secs(HEARTBEAT_INTERVAL_SECS),
                client.receiver.recv(),
            )
            .await;

            let payload = match outcome {
                Ok(Ok(message)) => message,
                Ok(Err(RecvError::Lagged(missed))) => resync_hint_payload(missed),
                Ok(Err(RecvError::Closed)) => break,
                Err(_) => heartbeat_payload(),
            };
            yield Ok::<Event, Infallible>(Event::default().data(payload));
        }
    };

    // 心跳已在业务层按 HEARTBEAT_INTERVAL_SECS 自行发送（与 Python 版一致的 data 事件），
    // 不再叠加 axum 默认的注释型 keep-alive。
    Sse::new(stream).into_response()
}

pub async fn send_command(
    State(state): State<AppState>,
    body: Option<axum::Json<Value>>,
) -> ApiJson {
    let body = body.map(|axum::Json(v)| v).unwrap_or(Value::Null);
    let code = body.get("code").and_then(Value::as_i64).unwrap_or_default() as i32;
    let data = body.get("data").cloned().unwrap_or(Value::from(0));
    state.services.playback.send_command(code, data).into()
}
