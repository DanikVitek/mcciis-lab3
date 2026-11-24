use std::path::PathBuf;
use std::time::Duration;

use color_eyre::eyre::{
    Result,
    WrapErr,
};
use dss::{
    InferenceEngine,
    KnowledgeBase,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
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

const POLLER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("CONFIG_PATH").ok())
        .map_or_else(|| PathBuf::from("config.toml"), PathBuf::from);

    let config = AppConfig::load_from_file(&config_path).wrap_err_with(|| {
        format!(
            "не вдалося завантажити конфігурацію з {}",
            config_path.display()
        )
    })?;
    let _log_guard =
        init_logging(&config.logging).wrap_err("не вдалося ініціалізувати систему логування")?;

    let rules_path = config.dss.rules_path.clone();
    let kb = KnowledgeBase::load_from_file(&rules_path).wrap_err_with(|| {
        format!(
            "не вдалося завантажити базу знань із {}",
            rules_path.display()
        )
    })?;
    let engine = InferenceEngine::new(kb);
    let bind_addr = config.server.bind_addr.clone();
    let log_dir = config.logging.dir.clone();
    let log_prefix = config.logging.file_prefix.clone();
    let log_rotation = config.logging.rotation.as_str();
    let frontend_mode = default_frontend_mode();

    let agent = WebAgent::new(config, engine).wrap_err("не вдалося ініціалізувати Web-агент")?;

    // Фоновий потік/завдання збору даних працює паралельно й не блокує Web/API (п. 6).
    let shutdown = CancellationToken::new();
    let bg_agent = agent.clone();
    let bg_shutdown = shutdown.clone();
    let mut poller_handle = tokio::spawn(async move {
        bg_agent.run_background_loop(bg_shutdown).await;
    });

    let app = build_router_with_frontend_mode(agent, frontend_mode.clone());
    let listener = TcpListener::bind(&bind_addr)
        .await
        .wrap_err_with(|| format!("не вдалося відкрити TCP-сокет на {bind_addr}"))?;

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
                bind_addr = %bind_addr,
                frontend_dir = %frontend_dir.display(),
                log_dir = %log_dir.display(),
                log_prefix = %log_prefix,
                log_rotation = %log_rotation,
                "Сервер Web-агента та ІСППР запущено"
            );
        }
        #[cfg(debug_assertions)]
        FrontendMode::DevRedirect(dev_url) => {
            info!(
                mode = "dev-api",
                bind_addr = %bind_addr,
                dev_url = %dev_url,
                log_dir = %log_dir.display(),
                log_prefix = %log_prefix,
                log_rotation = %log_rotation,
                "Сервер API Web-агента запущено в режимі розробки"
            );
        }
    }

    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown.clone()))
        .await
        .wrap_err("помилка HTTP-сервера axum");

    shutdown.cancel();
    match tokio::time::timeout(POLLER_SHUTDOWN_TIMEOUT, &mut poller_handle).await {
        Ok(Ok(())) => {}
        Ok(Err(join_err)) => {
            warn!(error = %join_err, "Фоновий цикл Web-агента завершився з помилкою");
        }
        Err(_) => {
            warn!(
                timeout_secs = POLLER_SHUTDOWN_TIMEOUT.as_secs(),
                "Фоновий цикл Web-агента не встиг завершитися вчасно; примусове скасування"
            );
            poller_handle.abort();
        }
    }

    serve_result?;
    info!("Сервер коректно завершив роботу.");
    Ok(())
}

async fn shutdown_signal(shutdown: CancellationToken) {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            warn!(error = %err, "Не вдалося підписатися на сигнал Ctrl+C");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sigterm) => {
                sigterm.recv().await;
            }
            Err(err) => {
                warn!(error = %err, "Не вдалося підписатися на сигнал SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("Отримано сигнал завершення (Ctrl+C)...");
        }
        () = terminate => {
            info!("Отримано сигнал завершення (SIGTERM)...");
        }
    }
    shutdown.cancel();
}
