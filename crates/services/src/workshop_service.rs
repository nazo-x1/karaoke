//! 上传编辑页（工坊）临时会话：预检 / 组装 / AI 分轨 → 临时双轨容器 → 严格入库。

use crate::dto::ApiResult;
use crate::LibraryService;
use karaoke_infra::config::SeparatorConfig;
use karaoke_infra::media::{
    extract_mix_audio, preflight_media, remux_dual_container, MediaSettings,
};
use karaoke_infra::workshop::WorkshopPreflight;
use karaoke_infra::{SeparatorClient, WorkshopSessionMeta, WorkshopStatus, WorkshopStore};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct WorkshopService {
    pub store: WorkshopStore,
    pub media: MediaSettings,
    pub library: LibraryService,
    pub separator: Option<SeparatorClient>,
    pub separator_cfg: SeparatorConfig,
    pub ai_slots: Arc<Semaphore>,
}

impl WorkshopService {
    pub fn new(
        store: WorkshopStore,
        media: MediaSettings,
        library: LibraryService,
        separator_cfg: SeparatorConfig,
    ) -> Self {
        let separator = if separator_cfg.enabled {
            match SeparatorClient::new(
                &separator_cfg.base_url,
                &separator_cfg.api_token,
                separator_cfg.request_timeout_secs,
            ) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::error!("create SeparatorClient failed: {e}");
                    None
                }
            }
        } else {
            None
        };
        let ai_slots = Arc::new(Semaphore::new(separator_cfg.max_concurrent.max(1)));
        Self {
            store,
            media,
            library,
            separator,
            separator_cfg,
            ai_slots,
        }
    }

    pub fn separator_enabled(&self) -> bool {
        self.separator_cfg.enabled && self.separator.is_some()
    }

    pub async fn create_session(&self) -> ApiResult {
        match self.store.create_session().await {
            Ok(meta) => ApiResult::ok_msg_data("会话已创建", self.session_public(&meta)),
            Err(e) => ApiResult::fail(format!("创建工坊会话失败: {e}")),
        }
    }

    pub async fn get_session(&self, session_id: &str) -> ApiResult {
        let Some(meta) = self.store.read_meta(session_id).await else {
            return ApiResult::not_found("工坊会话");
        };
        // 轮询兜底：若 AI 进行中则尝试刷新 separator 状态
        if matches!(
            meta.status,
            WorkshopStatus::AiQueued | WorkshopStatus::AiRunning
        ) {
            if let Some(job_id) = meta.separator_job_id.clone() {
                let _ = self.try_finalize_ai(session_id, &job_id).await;
            }
        }
        let Some(meta) = self.store.read_meta(session_id).await else {
            return ApiResult::not_found("工坊会话");
        };
        ApiResult::ok_with_data(self.session_public(&meta))
    }

    pub async fn destroy_session(&self, session_id: &str) -> ApiResult {
        if self.store.read_meta(session_id).await.is_none() {
            return ApiResult::not_found("工坊会话");
        }
        // 尽力清理 separator job
        if let (Some(client), Some(meta)) =
            (&self.separator, self.store.read_meta(session_id).await)
        {
            if let Some(job_id) = &meta.separator_job_id {
                let _ = client.delete_job(job_id).await;
            }
        }
        match self.store.destroy_session(session_id).await {
            Ok(()) => ApiResult::ok_msg("会话已删除"),
            Err(e) => ApiResult::fail(format!("删除会话失败: {e}")),
        }
    }

    pub async fn preflight(&self, session_id: &str, filename: &str, bytes: Vec<u8>) -> ApiResult {
        let Some(mut meta) = self.store.read_meta(session_id).await else {
            return ApiResult::not_found("工坊会话");
        };
        let filename = WorkshopStore::safe_filename(filename);
        if filename.is_empty() {
            return ApiResult::fail("未选择文件");
        }
        let path = match self
            .store
            .write_bytes(session_id, &format!("input_{filename}"), &bytes)
            .await
        {
            Ok(p) => p,
            Err(e) => return ApiResult::fail(format!("保存预检文件失败: {e}")),
        };
        let pf = preflight_media(&self.media, &path.to_string_lossy()).await;
        meta.preflight = Some(WorkshopPreflight {
            playable: pf.playable,
            mode_hint: pf.mode_hint.clone(),
            suggestion: pf.suggestion.clone(),
            layout: pf.layout.clone(),
        });
        meta.status = WorkshopStatus::Preflighted;
        meta.display_name = stem(&filename);
        if let Err(e) = self.store.write_meta(&meta).await {
            return ApiResult::fail(format!("写入会话失败: {e}"));
        }
        ApiResult::ok_msg_data(
            "预检完成",
            json!({
                "session": self.session_public(&meta),
                "preflight": {
                    "playable": pf.playable,
                    "mode_hint": pf.mode_hint,
                    "suggestion": pf.suggestion,
                    "layout": karaoke_domain::audio_layout::layout_summary(Some(&pf.layout)),
                }
            }),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn assemble(
        &self,
        session_id: &str,
        video_name: &str,
        video: Vec<u8>,
        vocals_name: &str,
        vocals: Vec<u8>,
        accomp_name: &str,
        accomp: Vec<u8>,
    ) -> ApiResult {
        let Some(mut meta) = self.store.read_meta(session_id).await else {
            return ApiResult::not_found("工坊会话");
        };
        let video_name = WorkshopStore::safe_filename(video_name);
        let vocals_name = WorkshopStore::safe_filename(vocals_name);
        let accomp_name = WorkshopStore::safe_filename(accomp_name);
        if video_name.is_empty() || vocals_name.is_empty() || accomp_name.is_empty() {
            return ApiResult::fail("请上传视频、原唱音频与伴奏音频");
        }

        meta.status = WorkshopStatus::Assembling;
        let _ = self.store.write_meta(&meta).await;

        let video_path = match self
            .store
            .write_bytes(session_id, &format!("assemble_video_{video_name}"), &video)
            .await
        {
            Ok(p) => p,
            Err(e) => return ApiResult::fail(format!("保存视频失败: {e}")),
        };
        let vocals_path = match self
            .store
            .write_bytes(
                session_id,
                &format!("assemble_vocals_{vocals_name}"),
                &vocals,
            )
            .await
        {
            Ok(p) => p,
            Err(e) => return ApiResult::fail(format!("保存原唱失败: {e}")),
        };
        let accomp_path = match self
            .store
            .write_bytes(
                session_id,
                &format!("assemble_accomp_{accomp_name}"),
                &accomp,
            )
            .await
        {
            Ok(p) => p,
            Err(e) => return ApiResult::fail(format!("保存伴奏失败: {e}")),
        };

        let product_filename = format!("{}.mkv", stem(&video_name));
        let dest = self.store.product_path(session_id, &product_filename);
        let ok = remux_dual_container(
            &self.media,
            &video_path.to_string_lossy(),
            &vocals_path.to_string_lossy(),
            &accomp_path.to_string_lossy(),
            &dest,
        )
        .await;
        if !ok {
            meta.status = WorkshopStatus::Idle;
            meta.ai_error = Some("组装双轨容器失败".to_string());
            let _ = self.store.write_meta(&meta).await;
            return ApiResult::fail("组装双轨容器失败");
        }

        let pf = preflight_media(&self.media, &dest.to_string_lossy()).await;
        if !pf.playable {
            let _ = tokio::fs::remove_file(&dest).await;
            meta.status = WorkshopStatus::Idle;
            let _ = self.store.write_meta(&meta).await;
            return ApiResult::fail("组装产物不可播放");
        }

        meta.product_filename = product_filename;
        meta.display_name = stem(&video_name);
        meta.status = WorkshopStatus::ProductReady;
        meta.preflight = Some(WorkshopPreflight {
            playable: pf.playable,
            mode_hint: pf.mode_hint,
            suggestion: pf.suggestion,
            layout: pf.layout,
        });
        meta.ai_error = None;
        if let Err(e) = self.store.write_meta(&meta).await {
            return ApiResult::fail(format!("写入会话失败: {e}"));
        }
        ApiResult::ok_msg_data("组装完成", self.session_public(&meta))
    }

    pub async fn ai_separate(&self, session_id: &str, filename: &str, bytes: Vec<u8>) -> ApiResult {
        if !self.separator_enabled() {
            return ApiResult::fail("未启用 Separator，无法使用 AI 分轨");
        }
        let Some(client) = self.separator.clone() else {
            return ApiResult::fail("Separator 客户端不可用");
        };
        let Some(mut meta) = self.store.read_meta(session_id).await else {
            return ApiResult::not_found("工坊会话");
        };
        let filename = WorkshopStore::safe_filename(filename);
        if filename.is_empty() {
            return ApiResult::fail("未选择文件");
        }

        let permit = match self.ai_slots.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => return ApiResult::fail("AI 分轨繁忙，请稍后再试"),
        };

        let video_path = match self
            .store
            .write_bytes(session_id, &format!("ai_video_{filename}"), &bytes)
            .await
        {
            Ok(p) => p,
            Err(e) => return ApiResult::fail(format!("保存视频失败: {e}")),
        };

        let mix_path = self.store.product_path(session_id, "mix.wav");
        if !extract_mix_audio(&self.media, &video_path.to_string_lossy(), &mix_path).await {
            return ApiResult::fail("抽取混合音频失败");
        }
        let mix_bytes = match tokio::fs::read(&mix_path).await {
            Ok(b) => b,
            Err(e) => return ApiResult::fail(format!("读取混合音频失败: {e}")),
        };

        let callback_url = if self.separator_cfg.callback_base_url.is_empty() {
            None
        } else {
            Some(format!(
                "{}/api/v1/internal/workshop-separation-callback",
                self.separator_cfg.callback_base_url
            ))
        };
        let model = if self.separator_cfg.default_model.is_empty() {
            None
        } else {
            Some(self.separator_cfg.default_model.as_str())
        };

        let job = match client
            .submit_job(
                mix_bytes,
                "mix.wav",
                Some(session_id),
                model,
                callback_url.as_deref(),
            )
            .await
        {
            Ok(j) => j,
            Err(e) => return ApiResult::fail(format!("提交 Separator 任务失败: {e}")),
        };

        meta.separator_job_id = Some(job.job_id.clone());
        meta.display_name = stem(&filename);
        meta.ai_error = None;
        meta.status = if job.status == "done" {
            WorkshopStatus::AiRunning
        } else if job.status == "failed" {
            WorkshopStatus::AiFailed
        } else {
            WorkshopStatus::AiQueued
        };
        // 记住视频文件名供 remux 产出命名
        let _ = self
            .store
            .write_bytes(session_id, "ai_source_name.txt", filename.as_bytes())
            .await;
        if let Err(e) = self.store.write_meta(&meta).await {
            return ApiResult::fail(format!("写入会话失败: {e}"));
        }

        // 后台轮询 + 完成后 remux（webhook 也会触发 finalize）
        let this = self.clone();
        let sid = session_id.to_string();
        let jid = job.job_id.clone();
        let timeout = Duration::from_secs(self.separator_cfg.job_timeout_secs);
        tokio::spawn(async move {
            let _permit = permit;
            let start = std::time::Instant::now();
            loop {
                if start.elapsed() > timeout {
                    if let Some(mut m) = this.store.read_meta(&sid).await {
                        m.status = WorkshopStatus::AiFailed;
                        m.ai_error = Some("AI 分轨超时".to_string());
                        let _ = this.store.write_meta(&m).await;
                    }
                    break;
                }
                match this.try_finalize_ai(&sid, &jid).await {
                    Ok(true) => break,
                    Ok(false) => {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                    Err(msg) => {
                        if let Some(mut m) = this.store.read_meta(&sid).await {
                            m.status = WorkshopStatus::AiFailed;
                            m.ai_error = Some(msg);
                            let _ = this.store.write_meta(&m).await;
                        }
                        break;
                    }
                }
            }
        });

        ApiResult::ok_msg_data("AI 分轨已提交", self.session_public(&meta))
    }

    /// Webhook / 轮询：若 job 完成则下载 stems 并 remux。返回 Ok(true) 表示已终态处理。
    pub async fn try_finalize_ai(&self, session_id: &str, job_id: &str) -> Result<bool, String> {
        let Some(client) = &self.separator else {
            return Err("Separator 未启用".to_string());
        };
        let Some(mut meta) = self.store.read_meta(session_id).await else {
            return Err("会话不存在".to_string());
        };
        if matches!(
            meta.status,
            WorkshopStatus::ProductReady | WorkshopStatus::Committed | WorkshopStatus::AiFailed
        ) {
            return Ok(true);
        }

        let job = client
            .get_job(job_id)
            .await
            .map_err(|e| format!("查询 Separator 失败: {e}"))?;

        match job.status.as_str() {
            "queued" => {
                meta.status = WorkshopStatus::AiQueued;
                let _ = self.store.write_meta(&meta).await;
                Ok(false)
            }
            "running" => {
                meta.status = WorkshopStatus::AiRunning;
                let _ = self.store.write_meta(&meta).await;
                Ok(false)
            }
            "failed" | "cancelled" => {
                meta.status = WorkshopStatus::AiFailed;
                meta.ai_error = job
                    .error
                    .or_else(|| Some(format!("separator status={}", job.status)));
                let _ = self.store.write_meta(&meta).await;
                let _ = client.delete_job(job_id).await;
                Ok(true)
            }
            "done" => {
                meta.status = WorkshopStatus::AiRunning;
                let _ = self.store.write_meta(&meta).await;

                let vocals = client
                    .download_stem(job_id, "vocals")
                    .await
                    .map_err(|e| format!("下载 vocals 失败: {e}"))?;
                let instrumental = client
                    .download_stem(job_id, "instrumental")
                    .await
                    .map_err(|e| format!("下载 instrumental 失败: {e}"))?;

                let vocals_path = self
                    .store
                    .write_bytes(session_id, "vocals.wav", &vocals)
                    .await
                    .map_err(|e| e.to_string())?;
                let accomp_path = self
                    .store
                    .write_bytes(session_id, "instrumental.wav", &instrumental)
                    .await
                    .map_err(|e| e.to_string())?;

                let video_path = find_ai_video(&self.store.session_dir(session_id)).await;
                let Some(video_path) = video_path else {
                    return Err("找不到 AI 输入视频".to_string());
                };

                let display = if meta.display_name.is_empty() {
                    stem(
                        &tokio::fs::read_to_string(
                            self.store.product_path(session_id, "ai_source_name.txt"),
                        )
                        .await
                        .unwrap_or_else(|_| "product".to_string()),
                    )
                } else {
                    meta.display_name.clone()
                };
                let product_filename = format!("{display}.mkv");
                let dest = self.store.product_path(session_id, &product_filename);
                let ok = remux_dual_container(
                    &self.media,
                    &video_path.to_string_lossy(),
                    &vocals_path.to_string_lossy(),
                    &accomp_path.to_string_lossy(),
                    &dest,
                )
                .await;
                let _ = client.delete_job(job_id).await;
                if !ok {
                    meta.status = WorkshopStatus::AiFailed;
                    meta.ai_error = Some("AI 分轨 remux 失败".to_string());
                    let _ = self.store.write_meta(&meta).await;
                    return Err("AI 分轨 remux 失败".to_string());
                }

                let pf = preflight_media(&self.media, &dest.to_string_lossy()).await;
                meta.product_filename = product_filename;
                meta.display_name = display;
                meta.status = WorkshopStatus::ProductReady;
                meta.ai_error = None;
                meta.preflight = Some(WorkshopPreflight {
                    playable: pf.playable,
                    mode_hint: pf.mode_hint,
                    suggestion: pf.suggestion,
                    layout: pf.layout,
                });
                self.store
                    .write_meta(&meta)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(true)
            }
            other => {
                tracing::warn!(status = other, "unexpected separator status");
                Ok(false)
            }
        }
    }

    pub async fn handle_separator_callback(
        &self,
        job_id: &str,
        status: &str,
        token: Option<&str>,
    ) -> ApiResult {
        if !self.separator_enabled() {
            return ApiResult::fail("未启用 Separator");
        }
        if let Some(client) = &self.separator {
            if !client.token_matches(token) {
                return ApiResult::fail("鉴权失败");
            }
        }
        // job_id 提交时使用 session_id
        let session_id = job_id;
        match self.try_finalize_ai(session_id, job_id).await {
            Ok(_) => {
                let meta = self.store.read_meta(session_id).await;
                ApiResult::ok_msg_data(
                    format!("callback processed ({status})"),
                    meta.map(|m| self.session_public(&m))
                        .unwrap_or(json!({"job_id": job_id})),
                )
            }
            Err(e) => ApiResult::fail(e),
        }
    }

    pub async fn commit(&self, session_id: &str, duplicate_policy: Option<&str>) -> ApiResult {
        let Some(mut meta) = self.store.read_meta(session_id).await else {
            return ApiResult::not_found("工坊会话");
        };
        if meta.product_filename.is_empty() || meta.status != WorkshopStatus::ProductReady {
            return ApiResult::fail("成品尚未就绪，无法入库");
        }
        let product = self.store.product_path(session_id, &meta.product_filename);
        if !product.is_file() {
            return ApiResult::fail("成品文件丢失");
        }
        let result = self
            .library
            .upload_strict_from_path(&meta.product_filename, &product, duplicate_policy)
            .await;
        if result.code == 0 {
            meta.status = WorkshopStatus::Committed;
            let _ = self.store.write_meta(&meta).await;
            let _ = self.store.destroy_session(session_id).await;
        }
        result
    }

    fn session_public(&self, meta: &WorkshopSessionMeta) -> serde_json::Value {
        json!({
            "session_id": meta.session_id,
            "status": meta.status.as_str(),
            "display_name": meta.display_name,
            "product_ready": meta.status == WorkshopStatus::ProductReady
                && !meta.product_filename.is_empty(),
            "product_filename": meta.product_filename,
            "separator_job_id": meta.separator_job_id,
            "ai_error": meta.ai_error,
            "created_at": meta.created_at.to_rfc3339(),
            "expires_at": meta.expires_at.to_rfc3339(),
            "preflight": meta.preflight.as_ref().map(|p| json!({
                "playable": p.playable,
                "mode_hint": p.mode_hint,
                "suggestion": p.suggestion,
                "layout": karaoke_domain::audio_layout::layout_summary(Some(&p.layout)),
            })),
            "separator_enabled": self.separator_enabled(),
        })
    }
}

fn stem(filename: &str) -> String {
    match filename.rfind('.') {
        Some(0) | None => filename.to_string(),
        Some(idx) => filename[..idx].to_string(),
    }
}

async fn find_ai_video(dir: &Path) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("ai_video_") {
            return Some(entry.path());
        }
    }
    None
}
