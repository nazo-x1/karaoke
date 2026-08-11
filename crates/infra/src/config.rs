//! 配置加载：`config.toml` + 环境变量覆盖（对应 Python `settings.py`/`config.conf`）。

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    pub path: PathBuf,
    #[serde(default)]
    pub keep_dir_name: Option<String>,
    #[serde(default = "default_scan_video_exts")]
    pub scan_video_exts: Vec<String>,
    #[serde(default = "default_true")]
    pub ffprobe_on_import: bool,
    #[serde(default = "default_duplicate_policy")]
    pub default_duplicate_policy: String,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default = "default_prepare_concurrent")]
    pub prepare_max_concurrent: usize,
    #[serde(default = "default_transcode_max_height")]
    pub transcode_max_height: u32,
    #[serde(default = "default_probe_size")]
    pub probe_size: u64,
    #[serde(default = "default_analyze_duration")]
    pub analyze_duration: u64,
    #[serde(default = "default_scan_validate_concurrency")]
    pub scan_validate_concurrency: usize,
    #[serde(default)]
    pub separator: SeparatorFileConfig,
    #[serde(default)]
    pub workshop: WorkshopFileConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeparatorFileConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_separator_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub callback_base_url: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default = "default_separator_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_separator_request_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_separator_job_timeout")]
    pub job_timeout_secs: u64,
}

impl Default for SeparatorFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_separator_base_url(),
            api_token: String::new(),
            callback_base_url: String::new(),
            default_model: String::new(),
            max_concurrent: default_separator_max_concurrent(),
            request_timeout_secs: default_separator_request_timeout(),
            job_timeout_secs: default_separator_job_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkshopFileConfig {
    #[serde(default = "default_workshop_dir_name")]
    pub dir_name: String,
    #[serde(default = "default_workshop_session_ttl")]
    pub session_ttl_secs: u64,
}

impl Default for WorkshopFileConfig {
    fn default() -> Self {
        Self {
            dir_name: default_workshop_dir_name(),
            session_ttl_secs: default_workshop_session_ttl(),
        }
    }
}

fn default_port() -> u16 {
    15233
}
fn default_true() -> bool {
    true
}
fn default_scan_video_exts() -> Vec<String> {
    vec!["mp4".to_string()]
}
fn default_duplicate_policy() -> String {
    "skip".to_string()
}
fn default_prepare_concurrent() -> usize {
    1
}
fn default_transcode_max_height() -> u32 {
    1080
}
fn default_probe_size() -> u64 {
    2_000_000
}
fn default_analyze_duration() -> u64 {
    2_000_000
}
fn default_scan_validate_concurrency() -> usize {
    2
}
fn default_separator_base_url() -> String {
    "http://separator:8080".to_string()
}
fn default_separator_max_concurrent() -> usize {
    1
}
fn default_separator_request_timeout() -> u64 {
    60
}
fn default_separator_job_timeout() -> u64 {
    3600
}
fn default_workshop_dir_name() -> String {
    "__workshop__".to_string()
}
fn default_workshop_session_ttl() -> u64 {
    86400
}

#[derive(Debug, Clone)]
pub struct SeparatorConfig {
    pub enabled: bool,
    pub base_url: String,
    pub api_token: String,
    pub callback_base_url: String,
    pub default_model: String,
    pub max_concurrent: usize,
    pub request_timeout_secs: u64,
    pub job_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct WorkshopConfig {
    pub dir_name: String,
    pub session_ttl_secs: u64,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: Option<String>,
    pub port: u16,
    pub data_path: PathBuf,
    pub keep_path: PathBuf,
    pub play_cache_path: PathBuf,
    pub embedded_cache_path: PathBuf,
    pub workshop_path: PathBuf,
    pub keep_dir_name: String,
    pub scan_video_exts: Vec<String>,
    pub ffprobe_on_import: bool,
    pub default_duplicate_policy: String,
    pub log_level: String,
    pub prepare_max_concurrent: usize,
    pub transcode_max_height: u32,
    pub probe_size: u64,
    pub analyze_duration: u64,
    pub scan_validate_concurrency: usize,
    pub database_url: String,
    pub separator: SeparatorConfig,
    pub workshop: WorkshopConfig,
}

impl AppConfig {
    pub fn load(config_path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(config_path)
            .map_err(|e| anyhow::anyhow!("读取配置文件 {config_path} 失败: {e}"))?;
        let file: FileConfig = toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("解析配置文件 {config_path} 失败: {e}"))?;

        if !file.path.exists() {
            anyhow::bail!("配置 path 指向的目录不存在: {}", file.path.display());
        }

        let keep_dir_name = file
            .keep_dir_name
            .clone()
            .unwrap_or_else(|| "__keep__".to_string());
        let workshop_dir_name = file.workshop.dir_name.clone();

        let keep_path = file.path.join(&keep_dir_name);
        let play_cache_path = file.path.join("__play_cache__");
        let embedded_cache_path = play_cache_path.join("embedded");
        let workshop_path = file.path.join(&workshop_dir_name);

        std::fs::create_dir_all(&keep_path)?;
        std::fs::create_dir_all(&play_cache_path)?;
        std::fs::create_dir_all(&embedded_cache_path)?;
        std::fs::create_dir_all(&workshop_path)?;

        let database_url = build_database_url();
        let prepare_max_concurrent = std::env::var("PREPARE_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(file.prepare_max_concurrent)
            .max(1);

        let mut sep = file.separator;
        if let Ok(v) = std::env::var("SEPARATOR_ENABLED") {
            if let Some(b) = parse_env_bool(&v) {
                sep.enabled = b;
            }
        }
        if let Ok(v) = std::env::var("SEPARATOR_BASE_URL") {
            if !v.trim().is_empty() {
                sep.base_url = v.trim().to_string();
            }
        }
        if let Ok(v) = std::env::var("SEPARATOR_API_TOKEN") {
            sep.api_token = v;
        }
        if let Ok(v) = std::env::var("SEPARATOR_CALLBACK_BASE_URL") {
            sep.callback_base_url = v.trim().to_string();
        }
        if let Ok(v) = std::env::var("SEPARATOR_DEFAULT_MODEL") {
            sep.default_model = v.trim().to_string();
        }

        Ok(Self {
            host: file.host,
            port: file.port,
            data_path: file.path,
            keep_path,
            play_cache_path,
            embedded_cache_path,
            workshop_path,
            keep_dir_name,
            scan_video_exts: file.scan_video_exts,
            ffprobe_on_import: file.ffprobe_on_import,
            default_duplicate_policy: file.default_duplicate_policy,
            log_level: file.log_level.unwrap_or_else(|| "info".to_string()),
            prepare_max_concurrent,
            transcode_max_height: file.transcode_max_height.max(1),
            probe_size: file.probe_size.max(32_768),
            analyze_duration: file.analyze_duration.max(100_000),
            scan_validate_concurrency: file.scan_validate_concurrency.max(1),
            database_url,
            separator: SeparatorConfig {
                enabled: sep.enabled,
                base_url: sep.base_url.trim_end_matches('/').to_string(),
                api_token: sep.api_token,
                callback_base_url: sep.callback_base_url.trim_end_matches('/').to_string(),
                default_model: sep.default_model,
                max_concurrent: sep.max_concurrent.max(1),
                request_timeout_secs: sep.request_timeout_secs.max(5),
                job_timeout_secs: sep.job_timeout_secs.max(30),
            },
            workshop: WorkshopConfig {
                dir_name: workshop_dir_name,
                session_ttl_secs: file.workshop.session_ttl_secs.max(60),
            },
        })
    }

    /// 扫描时跳过的目录名（任意嵌套层级）。
    pub fn scan_skip_dir_names(&self) -> std::collections::HashSet<String> {
        [
            self.keep_dir_name.clone(),
            self.workshop.dir_name.clone(),
            "__play_cache__".to_string(),
        ]
        .into_iter()
        .collect()
    }
}

fn parse_env_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// 优先 `DATABASE_URL`，其次 `POSTGRES_*` 拼装，与 V1 Python `settings.build_database_url` 等价。
fn build_database_url() -> String {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        if !url.trim().is_empty() {
            return url;
        }
    }
    if let Ok(host) = std::env::var("POSTGRES_HOST") {
        if !host.trim().is_empty() {
            let port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
            let user = std::env::var("POSTGRES_USER").unwrap_or_else(|_| "karaoke".to_string());
            let password =
                std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "karaoke".to_string());
            let db = std::env::var("POSTGRES_DB").unwrap_or_else(|_| "karaoke".to_string());
            return format!("postgres://{user}:{password}@{host}:{port}/{db}");
        }
    }
    "postgres://karaoke:karaoke@localhost:5432/karaoke".to_string()
}
