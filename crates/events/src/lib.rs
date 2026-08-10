//! SSE 事件总线。对应 Python `karaoke/events/bus.py`。
//!
//! 相较 Python 版的关键修复（P1）：
//! - 显式连接数上限（`try_subscribe` 超限返回 `None`，由 API 层拒绝新连接而非无限增长）；
//! - 基于 `tokio::sync::broadcast`，客户端处理慢时收到 `Lagged` 而不是被无提示地丢消息——
//!   API 层据此向该客户端补发一个 resync 提示事件（`RESYNC_HINT`），而不是静默吞掉业务事件；
//! - 客户端 Drop 时自动从计数中移除，不存在“死连接不清理”问题。

use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

pub const HEARTBEAT_INTERVAL_SECS: u64 = 15;
pub const DEFAULT_MAX_CLIENTS: usize = 100;
const CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EventCode {
    PlaybackControl = 1,
    Resing = 2,
    NextSong = 3,
    VocalSwitch = 4,
    VocalsVolume = 5,
    AccVolume = 6,
    Sfx = 7,
    QueueChanged = 8,
    PrepareReady = 9,
}

impl EventCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EventPayload {
    pub code: i32,
    pub data: serde_json::Value,
}

/// 心跳载荷，与 Python `_HEARTBEAT_PAYLOAD` 一致（`code=0`）。
pub fn heartbeat_payload() -> String {
    serde_json::json!({"code": 0, "data": "heartbeat"}).to_string()
}

/// 客户端 `Lagged` 时补发的提示事件：`code=0`，`data` 为字符串标记，
/// 前端据此可选择性触发一次全量刷新（新增语义，纯粹是可用性增强，不影响既有 1-9 事件码）。
pub fn resync_hint_payload(missed: u64) -> String {
    serde_json::json!({"code": 0, "data": format!("resync:{missed}")}).to_string()
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<String>,
    client_count: Arc<AtomicUsize>,
    max_clients: usize,
}

/// 持有期间计入在线连接数，Drop 时自动释放（修复 Python 版“死连接不清理”）。
pub struct ClientHandle {
    pub receiver: broadcast::Receiver<String>,
    count: Arc<AtomicUsize>,
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
    }
}

impl EventBus {
    pub fn new(max_clients: usize) -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender,
            client_count: Arc::new(AtomicUsize::new(0)),
            max_clients: max_clients.max(1),
        }
    }

    /// 达到连接上限时返回 `None`，调用方（API 层）应以 503/429 拒绝新的 SSE 连接。
    pub fn try_subscribe(&self) -> Option<ClientHandle> {
        loop {
            let current = self.client_count.load(Ordering::SeqCst);
            if current >= self.max_clients {
                return None;
            }
            if self
                .client_count
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Some(ClientHandle {
                    receiver: self.sender.subscribe(),
                    count: self.client_count.clone(),
                });
            }
        }
    }

    pub fn client_count(&self) -> usize {
        self.client_count.load(Ordering::SeqCst)
    }

    pub fn max_clients(&self) -> usize {
        self.max_clients
    }

    /// 广播事件；若当前无订阅者，`send` 返回错误，忽略即可（等价于 Python 版
    /// “无客户端时静默无操作”）。
    pub fn publish(&self, code: i32, data: serde_json::Value) {
        let payload = EventPayload { code, data };
        if let Ok(json) = serde_json::to_string(&payload) {
            let _ = self.sender.send(json);
        }
    }

    pub fn publish_queue_changed(&self) {
        self.publish(EventCode::QueueChanged.code(), serde_json::json!(0));
    }

    pub fn publish_prepare_ready(&self, song_id: i64) {
        self.publish(
            EventCode::PrepareReady.code(),
            serde_json::json!(song_id.to_string()),
        );
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CLIENTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_reaches_all_subscribers() {
        let bus = EventBus::new(10);
        let mut c1 = bus.try_subscribe().unwrap();
        let mut c2 = bus.try_subscribe().unwrap();
        bus.publish_queue_changed();
        let m1 = c1.receiver.recv().await.unwrap();
        let m2 = c2.receiver.recv().await.unwrap();
        assert!(m1.contains("\"code\":8"));
        assert_eq!(m1, m2);
    }

    #[test]
    fn subscribe_beyond_limit_returns_none() {
        let bus = EventBus::new(1);
        let _first = bus.try_subscribe().unwrap();
        assert!(bus.try_subscribe().is_none());
    }

    #[test]
    fn dropping_client_frees_capacity() {
        let bus = EventBus::new(1);
        {
            let _first = bus.try_subscribe().unwrap();
            assert_eq!(bus.client_count(), 1);
        }
        assert_eq!(bus.client_count(), 0);
        assert!(bus.try_subscribe().is_some());
    }

    #[test]
    fn prepare_ready_payload_stringifies_song_id_like_python() {
        let bus = EventBus::new(10);
        let mut c1 = bus.try_subscribe().unwrap();
        bus.publish_prepare_ready(42);
        let msg = c1.receiver.try_recv().unwrap();
        assert_eq!(msg, r#"{"code":9,"data":"42"}"#);
    }
}
