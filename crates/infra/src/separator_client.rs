//! Separator HTTP 客户端：仅工坊 AI 分轨使用；未使能时不应构造。
//! 契约见 `separator/README.md`：上传/下载走 body，不涉及文件路径。

use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SeparatorError {
    #[error("separator HTTP 错误: {0}")]
    Http(#[from] reqwest::Error),
    #[error("separator 返回 {status}: {body}")]
    Status { status: u16, body: String },
    #[error("separator 响应解析失败: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeparatorJobStatus {
    pub job_id: String,
    pub status: String,
    #[serde(default)]
    pub model_used: Option<String>,
    #[serde(default)]
    pub device_used: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Clone)]
pub struct SeparatorClient {
    base_url: String,
    api_token: String,
    http: reqwest::Client,
}

impl SeparatorClient {
    pub fn new(
        base_url: &str,
        api_token: &str,
        request_timeout_secs: u64,
    ) -> Result<Self, SeparatorError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(request_timeout_secs.max(5)))
            .build()?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_token: api_token.to_string(),
            http,
        })
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.api_token.is_empty() {
            req
        } else {
            req.header("Authorization", format!("Bearer {}", self.api_token))
                .header("X-Separator-Token", &self.api_token)
        }
    }

    pub fn token_matches(&self, header_token: Option<&str>) -> bool {
        if self.api_token.is_empty() {
            return true;
        }
        header_token == Some(self.api_token.as_str())
    }

    pub async fn submit_job(
        &self,
        audio_bytes: Vec<u8>,
        filename: &str,
        job_id: Option<&str>,
        model: Option<&str>,
        callback_url: Option<&str>,
    ) -> Result<SeparatorJobStatus, SeparatorError> {
        let mut form = Form::new().part(
            "file",
            Part::bytes(audio_bytes)
                .file_name(filename.to_string())
                .mime_str("application/octet-stream")
                .map_err(|e| SeparatorError::Parse(e.to_string()))?,
        );
        if let Some(id) = job_id.filter(|s| !s.is_empty()) {
            form = form.text("job_id", id.to_string());
        }
        if let Some(m) = model.filter(|s| !s.is_empty()) {
            form = form.text("model", m.to_string());
        }
        if let Some(cb) = callback_url.filter(|s| !s.is_empty()) {
            form = form.text("callback_url", cb.to_string());
        }

        let url = format!("{}/v1/jobs", self.base_url);
        let req = self.apply_auth(self.http.post(url).multipart(form));
        let resp = req.send().await?;
        Self::parse_job_response(resp).await
    }

    pub async fn get_job(&self, job_id: &str) -> Result<SeparatorJobStatus, SeparatorError> {
        let url = format!("{}/v1/jobs/{job_id}", self.base_url);
        let req = self.apply_auth(self.http.get(url));
        let resp = req.send().await?;
        Self::parse_job_response(resp).await
    }

    pub async fn download_stem(&self, job_id: &str, name: &str) -> Result<Vec<u8>, SeparatorError> {
        let url = format!("{}/v1/jobs/{job_id}/stems/{name}", self.base_url);
        // stem 可能较大：单独放宽超时
        let req = self.apply_auth(self.http.get(url).timeout(Duration::from_secs(600)));
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(SeparatorError::Status {
                status: status.as_u16(),
                body,
            });
        }
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn delete_job(&self, job_id: &str) -> Result<(), SeparatorError> {
        let url = format!("{}/v1/jobs/{job_id}", self.base_url);
        let req = self.apply_auth(self.http.delete(url));
        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 204 || status.is_success() {
            return Ok(());
        }
        let body = resp.text().await.unwrap_or_default();
        Err(SeparatorError::Status {
            status: status.as_u16(),
            body,
        })
    }

    async fn parse_job_response(
        resp: reqwest::Response,
    ) -> Result<SeparatorJobStatus, SeparatorError> {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !(status.is_success() || status.as_u16() == 202) {
            return Err(SeparatorError::Status {
                status: status.as_u16(),
                body,
            });
        }
        serde_json::from_str(&body).map_err(|e| SeparatorError::Parse(format!("{e}: {body}")))
    }
}
