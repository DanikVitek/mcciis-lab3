use thiserror::Error;

#[derive(Debug, Error)]
pub enum DssError {
    #[error("Помилка вводу/виводу: {0}")]
    Io(
        #[from]
        #[source]
        std::io::Error,
    ),

    #[error("Помилка розбору TOML бази знань: {0}")]
    TomlDe(
        #[from]
        #[source]
        toml::de::Error,
    ),

    #[error("Некоректне правило або база знань: {0}")]
    InvalidRule(String),
}

pub type DssResult<T> = Result<T, DssError>;
