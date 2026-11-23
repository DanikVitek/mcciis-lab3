use tracing_appender::non_blocking::{
    NonBlocking,
    WorkerGuard,
};
use tracing_appender::rolling::{
    RollingFileAppender,
    Rotation,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{
    EnvFilter,
    fmt,
};
use web_agent::config::{
    LogRotation,
    LoggingConfig,
};

pub fn build_rotating_appender(
    cfg: &LoggingConfig,
) -> Result<(NonBlocking, WorkerGuard), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&cfg.dir)?;

    let rotation = match cfg.rotation {
        LogRotation::Minutely => Rotation::MINUTELY,
        LogRotation::Hourly => Rotation::HOURLY,
        LogRotation::Daily => Rotation::DAILY,
        LogRotation::Never => Rotation::NEVER,
    };

    let file_appender = RollingFileAppender::builder()
        .rotation(rotation)
        .filename_prefix(&cfg.file_prefix)
        .max_log_files(cfg.max_files)
        .build(&cfg.dir)?;

    Ok(tracing_appender::non_blocking(file_appender))
}

/// Ініціалізує глобальний `tracing_subscriber` з виводом у консоль та
/// неблокуючим append-only ротованим файлом логів.
/// Повертає `WorkerGuard`, який необхідно утримувати до завершення роботи сервера.
pub fn init_logging(cfg: &LoggingConfig) -> Result<WorkerGuard, Box<dyn std::error::Error>> {
    let (file_writer, guard) = build_rotating_appender(cfg)?;

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,web_agent=debug,web_agent_server=debug"));

    let stdout_layer = fmt::layer().with_target(true);
    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()?;

    Ok(guard)
}
