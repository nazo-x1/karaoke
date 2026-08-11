//! 工坊临时会话目录：输入/中间 stems/成品容器；TTL 清理。
//! 路径仅 separator/app 内部使用，不出现在对外 API 响应中。

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkshopStatus {
    Idle,
    Preflighted,
    Assembling,
    AiQueued,
    AiRunning,
    AiFailed,
    ProductReady,
    Committed,
}

impl WorkshopStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkshopStatus::Idle => "idle",
            WorkshopStatus::Preflighted => "preflighted",
            WorkshopStatus::Assembling => "assembling",
            WorkshopStatus::AiQueued => "ai_queued",
            WorkshopStatus::AiRunning => "ai_running",
            WorkshopStatus::AiFailed => "ai_failed",
            WorkshopStatus::ProductReady => "product_ready",
            WorkshopStatus::Committed => "committed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopPreflight {
    pub playable: bool,
    pub mode_hint: String,
    pub suggestion: String,
    pub layout: karaoke_domain::AudioLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopSessionMeta {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: WorkshopStatus,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub product_filename: String,
    #[serde(default)]
    pub separator_job_id: Option<String>,
    #[serde(default)]
    pub ai_error: Option<String>,
    #[serde(default)]
    pub preflight: Option<WorkshopPreflight>,
}

impl WorkshopSessionMeta {
    pub fn new(session_id: String, ttl_secs: u64) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            created_at: now,
            expires_at: now + Duration::seconds(ttl_secs as i64),
            status: WorkshopStatus::Idle,
            display_name: String::new(),
            product_filename: String::new(),
            separator_job_id: None,
            ai_error: None,
            preflight: None,
        }
    }
}

#[derive(Clone)]
pub struct WorkshopStore {
    pub root: PathBuf,
    pub session_ttl_secs: u64,
}

impl WorkshopStore {
    pub fn new(root: PathBuf, session_ttl_secs: u64) -> Self {
        Self {
            root,
            session_ttl_secs: session_ttl_secs.max(60),
        }
    }

    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.root.join(session_id)
    }

    pub fn meta_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("meta.json")
    }

    pub fn product_path(&self, session_id: &str, filename: &str) -> PathBuf {
        self.session_dir(session_id).join(filename)
    }

    pub async fn create_session(&self) -> std::io::Result<WorkshopSessionMeta> {
        let session_id = Uuid::new_v4().to_string();
        let dir = self.session_dir(&session_id);
        tokio::fs::create_dir_all(&dir).await?;
        let meta = WorkshopSessionMeta::new(session_id, self.session_ttl_secs);
        self.write_meta(&meta).await?;
        Ok(meta)
    }

    pub async fn read_meta(&self, session_id: &str) -> Option<WorkshopSessionMeta> {
        let path = self.meta_path(session_id);
        let raw = tokio::fs::read_to_string(path).await.ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub async fn write_meta(&self, meta: &WorkshopSessionMeta) -> std::io::Result<()> {
        let path = self.meta_path(&meta.session_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let raw = serde_json::to_string_pretty(meta)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        tokio::fs::write(path, raw).await
    }

    pub async fn destroy_session(&self, session_id: &str) -> std::io::Result<()> {
        let dir = self.session_dir(session_id);
        if dir.is_dir() {
            tokio::fs::remove_dir_all(dir).await?;
        }
        Ok(())
    }

    pub async fn write_bytes(
        &self,
        session_id: &str,
        filename: &str,
        bytes: &[u8],
    ) -> std::io::Result<PathBuf> {
        let dir = self.session_dir(session_id);
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(filename);
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }

    pub async fn cleanup_expired(&self) -> usize {
        let mut removed = 0usize;
        let Ok(mut entries) = tokio::fs::read_dir(&self.root).await else {
            return 0;
        };
        let now = Utc::now();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(ft) = entry.file_type().await else {
                continue;
            };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let expired = match self.read_meta(&name).await {
                Some(meta) => meta.expires_at < now,
                None => {
                    // 无 meta：按目录 mtime 粗判
                    match entry.metadata().await {
                        Ok(m) => m
                            .modified()
                            .ok()
                            .and_then(|t| t.elapsed().ok())
                            .map(|d| d.as_secs() > self.session_ttl_secs)
                            .unwrap_or(false),
                        Err(_) => false,
                    }
                }
            };
            if expired && self.destroy_session(&name).await.is_ok() {
                removed += 1;
                tracing::info!(session_id = %name, "workshop session expired and removed");
            }
        }
        removed
    }

    pub fn safe_filename(filename: &str) -> String {
        let base = Path::new(filename.trim())
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        base.replace('\0', "")
    }
}
