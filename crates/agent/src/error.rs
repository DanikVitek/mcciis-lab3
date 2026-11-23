use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Помилка вводу/виводу: {0}")]
    Io(
        #[from]
        #[source]
        std::io::Error,
    ),

    #[error("Помилка розбору TOML конфігурації: {0}")]
    TomlDe(
        #[from]
        #[source]
        toml::de::Error,
    ),

    #[error("Помилка серіалізації JSON: {0}")]
    Json(
        #[from]
        #[source]
        serde_json::Error,
    ),

    #[error("Помилка HTTP-запиту: {0}")]
    Http(
        #[from]
        #[source]
        reqwest::Error,
    ),

    #[error("Помилка розбору стрічки новин: {0}")]
    FeedParse(
        #[from]
        #[source]
        feed_rs::parser::ParseFeedError,
    ),

    #[error("Помилка підсистеми ІСППР: {0}")]
    Dss(
        #[from]
        #[source]
        dss::error::DssError,
    ),

    #[error("Некоректна конфігурація: {0}")]
    Config(String),
}

pub type AgentResult<T> = Result<T, AgentError>;
