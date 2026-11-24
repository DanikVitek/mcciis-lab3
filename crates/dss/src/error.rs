//! Типи помилок ядра ІСППР (`dss`).

use thiserror::Error;

/// Помилка завантаження, розбору або валідації бази знань ІСППР.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DssError {
    /// Помилка читання файлу бази знань.
    #[error("помилка вводу/виводу: {0}")]
    Io(#[from] std::io::Error),

    /// Помилка десеріалізації TOML-файлу правил.
    #[error("помилка розбору TOML бази знань: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// Порушення інваріантів правила або бази знань.
    #[error("некоректне правило або база знань: {0}")]
    InvalidRule(String),
}

/// Результат операції підсистеми ІСППР.
pub type DssResult<T> = Result<T, DssError>;
