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

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: Option<String>,
    pub port: u16,
    pub data_path: PathBuf,
    pub keep_path: PathBuf,
    pub play_cache_path: PathBuf,
    pub embedded_cache_path: PathBuf,
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

        let keep_path = file.path.join(&keep_dir_name);
        let play_cache_path = file.path.join("__play_cache__");
        let embedded_cache_path = play_cache_path.join("embedded");

        std::fs::create_dir_all(&keep_path)?;
        std::fs::create_dir_all(&play_cache_path)?;
        std::fs::create_dir_all(&embedded_cache_path)?;

        let database_url = build_database_url();
        let prepare_max_concurrent = std::env::var("PREPARE_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(file.prepare_max_concurrent)
            .max(1);

        Ok(Self {
            host: file.host,
            port: file.port,
            data_path: file.path,
            keep_path,
            play_cache_path,
            embedded_cache_path,
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
        })
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
