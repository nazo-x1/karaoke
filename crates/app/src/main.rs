//! `karaoke-server`：装配配置/tracing/Postgres连接/服务/路由并启动 HTTP 服务。
//! 对应 Python `main.py`。

use anyhow::Context;
use karaoke_infra::{db, AppConfig};
use karaoke_services::AppServices;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;

fn config_path() -> String {
    std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string())
}

fn templates_dir() -> PathBuf {
    std::env::var("TEMPLATES_DIR")
        .unwrap_or_else(|_| "templates".to_string())
        .into()
}

fn static_dir() -> PathBuf {
    std::env::var("STATIC_DIR")
        .unwrap_or_else(|_| "static".to_string())
        .into()
}

/// 通过向公共 DNS 建立 UDP "连接"取本机出网 IP，不实际发包。对应 Python `get_local_ip`。
fn local_ip() -> IpAddr {
    (|| -> anyhow::Result<IpAddr> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("114.114.114.114:80")?;
        Ok(socket.local_addr()?.ip())
    })()
    .unwrap_or_else(|_| IpAddr::from([0, 0, 0, 0]))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load(&config_path())
        .with_context(|| format!("加载配置文件 {} 失败", config_path()))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.log_level)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    tracing::info!(port = config.port, data_path = %config.data_path.display(), "starting karaoke-server v2 (rust)");

    let pool = db::connect(&config.database_url)
        .await
        .context("连接 PostgreSQL 失败")?;
    db::run_migrations(&pool)
        .await
        .context("执行数据库迁移失败")?;
    tracing::info!("database migrations applied");

    let config = Arc::new(config);
    let services = AppServices::new(pool, config.clone());
    services.init_on_startup().await;

    let router = karaoke_api::build_router(services, &templates_dir(), &static_dir())
        .context("构建 HTTP 路由失败")?;

    let host = match &config.host {
        Some(h) if !h.trim().is_empty() => h.parse::<IpAddr>().unwrap_or_else(|_| local_ip()),
        _ => local_ip(),
    };
    let addr = SocketAddr::from((host, config.port));
    tracing::info!(%addr, "listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("绑定 {addr} 失败"))?;
    axum::serve(listener, router)
        .await
        .context("HTTP 服务异常退出")?;
    Ok(())
}
