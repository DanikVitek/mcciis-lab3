use std::path::PathBuf;

use dss::{
    InferenceEngine,
    KnowledgeBase,
};
use tokio::net::TcpListener;
use tracing::info;
use web_agent::WebAgent;
use web_agent::config::AppConfig;
use web_agent_server::logging::init_logging;
use web_agent_server::web::build_router;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("CONFIG_PATH").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    let config = AppConfig::load_from_file(&config_path)?;
    let _log_guard = init_logging(&config.logging)?;

    let kb = KnowledgeBase::load_from_file(&config.dss.rules_path)?;
    let engine = InferenceEngine::new(kb);
    let bind_addr = config.server.bind_addr.clone();
    let log_dir = config.logging.dir.display().to_string();
    let log_prefix = config.logging.file_prefix.clone();
    let log_rotation = config.logging.rotation.as_str();

    let agent = WebAgent::new(config, engine)?;

    // Фоновий потік/завдання збору даних працює паралельно й не блокує Web-інтерфейс (п. 6).
    let bg_agent = agent.clone();
    let poller_handle = tokio::spawn(async move {
        bg_agent.run_background_loop().await;
    });

    let app = build_router(agent);
    let listener = TcpListener::bind(&bind_addr).await?;

    info!(
        url = %format!("http://{bind_addr}/index.html"),
        conf = %format!("http://{bind_addr}/conf.html"),
        dss = %format!("http://{bind_addr}/dss.html"),
        log_dir = %log_dir,
        log_prefix = %log_prefix,
        log_rotation = %log_rotation,
        "Web-агент та ІСППР запущено"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    poller_handle.abort();
    info!("Сервер коректно завершив роботу.");
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("Отримано сигнал завершення (Ctrl+C)...");
}
