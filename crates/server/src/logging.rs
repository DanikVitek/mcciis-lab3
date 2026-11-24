//! Ініціалізація структурованого логування `tracing` із ротованим append-only файлом.

use thiserror::Error;
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

/// Помилки ініціалізації файлового логування та глобального `tracing_subscriber`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LoggingError {
    /// Не вдалося створити каталог для файлів журналів.
    #[error("помилка створення каталогу логів: {0}")]
    Io(#[from] std::io::Error),
    /// Помилка конструювання ротованого файлового апендера.
    #[error("помилка ініціалізації ротованого файлу логів: {0}")]
    RollingInit(#[from] tracing_appender::rolling::InitError),
    /// Глобальний `tracing_subscriber` уже був зареєстрований або не ініціалізувався.
    #[error("помилка реєстрації глобального tracing-підписника: {0}")]
    SubscriberInit(#[from] tracing_subscriber::util::TryInitError),
}

/// Створює неблокуючий ротований файловий апендер за конфігурацією [`LoggingConfig`].
///
/// # Errors
///
/// Повертає [`LoggingError::Io`] або [`LoggingError::RollingInit`], якщо каталог
/// недоступний або неможливо створити ротований файл логів.
pub fn build_rotating_appender(
    cfg: &LoggingConfig,
) -> Result<(NonBlocking, WorkerGuard), LoggingError> {
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
///
/// Повертає [`WorkerGuard`], який необхідно утримувати до завершення роботи сервера.
///
/// # Errors
///
/// Повертає [`LoggingError`], якщо не вдалося ініціалізувати апендер або зареєструвати підписник.
pub fn init_logging(cfg: &LoggingConfig) -> Result<WorkerGuard, LoggingError> {
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
