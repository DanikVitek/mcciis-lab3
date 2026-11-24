use std::path::PathBuf;

use dss::{
    InferenceEngine,
    KnowledgeBase,
};
use tokio::net::TcpListener;
use tracing::{
    info,
    warn,
};
use web_agent::WebAgent;
use web_agent::config::AppConfig;
use web_agent_server::logging::init_logging;
use web_agent_server::web::{
    FrontendMode,
    build_router_with_frontend_mode,
    default_frontend_mode,
};

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
    let frontend_mode = default_frontend_mode();

    let agent = WebAgent::new(config, engine)?;

    // Фоновий потік/завдання збору даних працює паралельно й не блокує Web/API (п. 6).
    let bg_agent = agent.clone();
    let poller_handle = tokio::spawn(async move {
        bg_agent.run_background_loop().await;
    });

    let app = build_router_with_frontend_mode(agent, frontend_mode.clone());
    let listener = TcpListener::bind(&bind_addr).await?;

    match &frontend_mode {
        FrontendMode::StaticDir(frontend_dir) => {
            if !frontend_dir.join("index.html").exists() {
                warn!(
                    frontend_dir = %frontend_dir.display(),
                    "Зібраний Web-інтерфейс не знайдено; виконайте `cargo make frontend-build` або запустіть `cargo make dev`"
                );
            }
            info!(
                mode = "static-spa",
                ui = %format!("http://{bind_addr}/"),
                api_state = %format!("http://{bind_addr}/api/state"),
                frontend_dir = %frontend_dir.display(),
                log_dir = %log_dir,
                log_prefix = %log_prefix,
                log_rotation = %log_rotation,
                "Сервер Web-агента та ІСППР запущено"
            );
        }
        #[cfg(debug_assertions)]
        FrontendMode::DevRedirect(dev_url) => {
            info!(
                mode = "dev-api",
                dev_ui = %format!("{dev_url}/"),
                api_state = %format!("http://{bind_addr}/api/state"),
                log_dir = %log_dir,
                log_prefix = %log_prefix,
                log_rotation = %log_rotation,
                "Сервер API Web-агента запущено в режимі розробки"
            );
        }
    }

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
