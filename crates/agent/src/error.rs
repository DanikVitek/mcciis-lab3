//! Типи помилок автономного Web-агента (`web-agent`).

use thiserror::Error;

/// Помилка роботи автономного Web-агента.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AgentError {
    /// Помилка файлового вводу/виводу.
    #[error("помилка вводу/виводу: {0}")]
    Io(#[from] std::io::Error),

    /// Помилка розбору TOML конфігурації.
    #[error("помилка розбору TOML конфігурації: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// Помилка серіалізації/десеріалізації JSON.
    #[error("помилка серіалізації JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// Помилка HTTP-запиту або статусу відповіді.
    #[error("помилка HTTP-запиту: {0}")]
    Http(#[from] reqwest::Error),

    /// Помилка розбору RSS/Atom стрічки новин.
    #[error("помилка розбору стрічки новин: {0}")]
    FeedParse(#[from] feed_rs::parser::ParseFeedError),

    /// Помилка підсистеми ІСППР.
    #[error("помилка підсистеми ІСППР: {0}")]
    Dss(#[from] dss::error::DssError),

    /// Помилка валідації конфігурації.
    #[error("некоректна конфігурація: {0}")]
    Config(String),

    /// Перевищено максимально допустимий розмір тіла HTTP-відповіді.
    #[error("розмір тіла відповіді перевищив ліміт {limit_bytes} байт")]
    ResponseTooLarge { limit_bytes: usize },

    /// Внутрішня помилка виконання фонового завдання.
    #[error("помилка фонового завдання: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}

/// Результат операції Web-агента.
pub type AgentResult<T> = Result<T, AgentError>;
