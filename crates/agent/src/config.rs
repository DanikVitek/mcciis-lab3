//! Конфігурація Web-агента, логування, підсистеми ІСППР і зовнішніх джерел.

use std::path::{
    Path,
    PathBuf,
};

use bon::bon;
use hashbrown::HashSet;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    AgentError,
    AgentResult,
};
use crate::health::RetryPolicy;

/// Повна валідована конфігурація застосунку.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawAppConfig")]
pub struct AppConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub dss: DssConfig,
    pub nbu_metrics: NbuMetricsConfig,
    pub http_sites: Vec<HttpSiteConfig>,
    pub news_feeds: Vec<NewsFeedConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAppConfig {
    server: ServerConfig,
    #[serde(default)]
    logging: LoggingConfig,
    dss: DssConfig,
    #[serde(default)]
    nbu_metrics: NbuMetricsConfig,
    #[serde(default)]
    http_sites: Vec<HttpSiteConfig>,
    #[serde(default)]
    news_feeds: Vec<NewsFeedConfig>,
}

impl TryFrom<RawAppConfig> for AppConfig {
    type Error = AgentError;

    fn try_from(raw: RawAppConfig) -> Result<Self, Self::Error> {
        Self::builder()
            .server(raw.server)
            .logging(raw.logging)
            .dss(raw.dss)
            .nbu_metrics(raw.nbu_metrics)
            .http_sites(raw.http_sites)
            .news_feeds(raw.news_feeds)
            .build()
    }
}

/// Періодичність ротації файлових журналів.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogRotation {
    Minutely,
    Hourly,
    #[default]
    Daily,
    Never,
}

impl LogRotation {
    /// Повертає рядкове позначення політики ротації.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minutely => "minutely",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Never => "never",
        }
    }
}

/// Параметри файлового логування з ротацією.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawLoggingConfig")]
pub struct LoggingConfig {
    pub dir: PathBuf,
    pub file_prefix: String,
    pub rotation: LogRotation,
    pub max_files: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLoggingConfig {
    #[serde(default = "default_log_dir")]
    dir: PathBuf,
    #[serde(default = "default_log_prefix")]
    file_prefix: String,
    #[serde(default)]
    rotation: LogRotation,
    #[serde(default = "default_log_max_files")]
    max_files: usize,
}

impl TryFrom<RawLoggingConfig> for LoggingConfig {
    type Error = AgentError;

    fn try_from(raw: RawLoggingConfig) -> Result<Self, Self::Error> {
        Self::builder()
            .dir(raw.dir)
            .file_prefix(raw.file_prefix)
            .rotation(raw.rotation)
            .max_files(raw.max_files)
            .build()
    }
}

#[bon]
impl LoggingConfig {
    /// Створює та валідує конфігурацію логування через типобезпечний builder (`bon`).
    ///
    /// # Errors
    ///
    /// Повертає [`AgentError::Config`], якщо `dir` або `file_prefix` порожні чи `max_files == 0`.
    #[builder(derive(Debug), on(String, into), on(PathBuf, into))]
    pub fn new(
        #[builder(default = default_log_dir())] dir: PathBuf,
        #[builder(default = default_log_prefix())] file_prefix: String,
        #[builder(default)] rotation: LogRotation,
        #[builder(default = default_log_max_files())] max_files: usize,
    ) -> AgentResult<Self> {
        if dir.as_os_str().is_empty() {
            return Err(AgentError::Config(
                "logging.dir не може бути порожнім".to_string(),
            ));
        }
        if file_prefix.trim().is_empty() {
            return Err(AgentError::Config(
                "logging.file_prefix не може бути порожнім".to_string(),
            ));
        }
        if max_files == 0 {
            return Err(AgentError::Config(
                "logging.max_files має бути більшим за 0".to_string(),
            ));
        }
        Ok(Self {
            dir,
            file_prefix,
            rotation,
            max_files,
        })
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            dir: default_log_dir(),
            file_prefix: default_log_prefix(),
            rotation: LogRotation::default(),
            max_files: default_log_max_files(),
        }
    }
}

/// Мережеві та планувальні параметри сервера і Web-агента.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawServerConfig")]
pub struct ServerConfig {
    pub bind_addr: String,
    pub poll_interval_secs: u64,
    pub request_timeout_secs: u64,
    pub max_concurrent_requests: usize,
    pub retry_policy: RetryPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServerConfig {
    bind_addr: String,
    #[serde(default = "default_poll_interval")]
    poll_interval_secs: u64,
    #[serde(default = "default_request_timeout")]
    request_timeout_secs: u64,
    #[serde(default = "default_max_concurrent")]
    max_concurrent_requests: usize,
    #[serde(default)]
    retry_policy: RetryPolicy,
}

impl TryFrom<RawServerConfig> for ServerConfig {
    type Error = AgentError;

    fn try_from(raw: RawServerConfig) -> Result<Self, Self::Error> {
        Self::builder()
            .bind_addr(raw.bind_addr)
            .poll_interval_secs(raw.poll_interval_secs)
            .request_timeout_secs(raw.request_timeout_secs)
            .max_concurrent_requests(raw.max_concurrent_requests)
            .retry_policy(raw.retry_policy)
            .build()
    }
}

#[bon]
impl ServerConfig {
    /// Створює та валідує параметри сервера через типобезпечний builder (`bon`).
    ///
    /// # Errors
    ///
    /// Повертає [`AgentError::Config`], якщо `bind_addr` порожній або будь-який числовий ліміт дорівнює 0.
    #[builder(derive(Debug), on(String, into))]
    pub fn new(
        bind_addr: String,
        #[builder(default = default_poll_interval())] poll_interval_secs: u64,
        #[builder(default = default_request_timeout())] request_timeout_secs: u64,
        #[builder(default = default_max_concurrent())] max_concurrent_requests: usize,
        #[builder(default)] retry_policy: RetryPolicy,
    ) -> AgentResult<Self> {
        if bind_addr.trim().is_empty() {
            return Err(AgentError::Config(
                "bind_addr не може бути порожнім".to_string(),
            ));
        }
        if poll_interval_secs == 0 {
            return Err(AgentError::Config(
                "poll_interval_secs має бути більшим за 0".to_string(),
            ));
        }
        if request_timeout_secs == 0 {
            return Err(AgentError::Config(
                "request_timeout_secs має бути більшим за 0".to_string(),
            ));
        }
        if max_concurrent_requests == 0 {
            return Err(AgentError::Config(
                "max_concurrent_requests має бути більшим за 0".to_string(),
            ));
        }

        Ok(Self {
            bind_addr,
            poll_interval_secs,
            request_timeout_secs,
            max_concurrent_requests,
            retry_policy,
        })
    }
}

/// Шляхи до бази знань і файлу збереження стану ІСППР.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DssConfig {
    pub rules_path: PathBuf,
    pub state_path: PathBuf,
}

/// Налаштування збору валютних метрик з API НБУ.
///
/// Якщо секція `[nbu_metrics]` повністю відсутня в конфігурації, використовується
/// [`Default`] (`enabled = false`); якщо секція задана явно без поля `enabled`,
/// `enabled` за замовчуванням дорівнює `true`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NbuMetricsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_nbu_url")]
    pub url: String,
    #[serde(default = "default_currencies")]
    pub currencies: Vec<String>,
}

impl Default for NbuMetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_nbu_url(),
            currencies: default_currencies(),
        }
    }
}

/// Конфігурація окремого HTTP/HTTPS-сайту для моніторингу змін.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpSiteConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default = "default_category")]
    pub category: String,
}

/// Конфігурація RSS/Atom стрічки новин і її критеріїв ранжування.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewsFeedConfig {
    pub id: String,
    pub name: String,
    pub group: String,
    pub url: String,
    #[serde(default)]
    pub keywords: Vec<KeywordCriterion>,
}

/// Ключове слово та його додатна вага для оцінки релевантності новини.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawKeywordCriterion")]
pub struct KeywordCriterion {
    pub word: String,
    pub weight: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKeywordCriterion {
    word: String,
    #[serde(default = "default_weight")]
    weight: f64,
}

impl TryFrom<RawKeywordCriterion> for KeywordCriterion {
    type Error = AgentError;

    fn try_from(raw: RawKeywordCriterion) -> Result<Self, Self::Error> {
        Self::new(raw.word, raw.weight)
    }
}

impl KeywordCriterion {
    /// Створює валідований критерій ключового слова.
    ///
    /// # Errors
    ///
    /// Повертає [`AgentError::Config`], якщо `word` порожнє або `weight` не є скінченним додатним числом.
    pub fn new(word: impl Into<String>, weight: f64) -> AgentResult<Self> {
        let word = word.into();
        if word.trim().is_empty() {
            return Err(AgentError::Config(
                "ключове слово не може бути порожнім".to_string(),
            ));
        }
        if !weight.is_finite() || weight <= 0.0 {
            return Err(AgentError::Config(format!(
                "вага ключового слова `{word}` має бути скінченним додатним числом"
            )));
        }
        Ok(Self { word, weight })
    }
}

fn default_log_dir() -> PathBuf {
    PathBuf::from("logs")
}

fn default_log_prefix() -> String {
    "web-agent-server.log".to_string()
}

const fn default_log_max_files() -> usize {
    7
}

const fn default_poll_interval() -> u64 {
    300
}

const fn default_request_timeout() -> u64 {
    15
}

const fn default_max_concurrent() -> usize {
    8
}

const fn default_true() -> bool {
    true
}

fn default_nbu_url() -> String {
    "https://bank.gov.ua/NBUStatService/v1/statdirectory/exchange?json".to_string()
}

fn default_currencies() -> Vec<String> {
    vec!["USD".to_string(), "EUR".to_string()]
}

fn default_category() -> String {
    "general".to_string()
}

const fn default_weight() -> f64 {
    1.0
}

fn is_http_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

#[bon]
impl AppConfig {
    /// Створює та валідує повну конфігурацію через типобезпечний builder (`bon`).
    ///
    /// # Errors
    ///
    /// Повертає [`AgentError::Config`] при порожніх або дубльованих ідентифікаторах джерел
    /// чи некоректній URL-схемі.
    #[builder(derive(Debug))]
    pub fn new(
        server: ServerConfig,
        #[builder(default)] logging: LoggingConfig,
        dss: DssConfig,
        #[builder(default)] nbu_metrics: NbuMetricsConfig,
        #[builder(default)] http_sites: Vec<HttpSiteConfig>,
        #[builder(default)] news_feeds: Vec<NewsFeedConfig>,
    ) -> AgentResult<Self> {
        if nbu_metrics.enabled && !is_http_url(&nbu_metrics.url) {
            return Err(AgentError::Config(format!(
                "URL API НБУ повинен починатися з http:// або https://: `{}`",
                nbu_metrics.url
            )));
        }

        {
            let mut seen_site_ids = HashSet::with_capacity(http_sites.len());
            for site in &http_sites {
                if site.id.trim().is_empty() || !is_http_url(&site.url) {
                    return Err(AgentError::Config(
                        "HTTP-сайт повинен мати непорожній id та коректний http(s):// url"
                            .to_string(),
                    ));
                }
                if !seen_site_ids.insert(site.id.as_str()) {
                    return Err(AgentError::Config(format!(
                        "дубльований ідентифікатор HTTP-сайту: `{}`",
                        site.id
                    )));
                }
            }
        }

        {
            let mut seen_feed_ids = HashSet::with_capacity(news_feeds.len());
            for feed in &news_feeds {
                if feed.id.trim().is_empty() || !is_http_url(&feed.url) {
                    return Err(AgentError::Config(
                        "стрічка новин повинна мати непорожній id та коректний http(s):// url"
                            .to_string(),
                    ));
                }
                if !seen_feed_ids.insert(feed.id.as_str()) {
                    return Err(AgentError::Config(format!(
                        "дубльований ідентифікатор стрічки новин: `{}`",
                        feed.id
                    )));
                }
            }
        }

        Ok(Self {
            server,
            logging,
            dss,
            nbu_metrics,
            http_sites,
            news_feeds,
        })
    }

    /// Завантажує та валідує конфігурацію з TOML-файлу.
    ///
    /// # Errors
    ///
    /// Повертає [`AgentError::Io`] або [`AgentError::TomlDe`] при помилці читання чи розбору.
    pub fn load_from_file(path: impl AsRef<Path>) -> AgentResult<Self> {
        let raw_toml = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&raw_toml)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn server_config_new_matches_validation_rules(
            bind_addr in "[ \tA-Za-z0-9.:]{0,20}",
            poll_interval_secs: u64,
            request_timeout_secs: u64,
            max_concurrent_requests: usize,
        ) {
            let expected_ok = !bind_addr.trim().is_empty()
                && poll_interval_secs > 0
                && request_timeout_secs > 0
                && max_concurrent_requests > 0;
            let res = ServerConfig::builder()
                .bind_addr(bind_addr)
                .poll_interval_secs(poll_interval_secs)
                .request_timeout_secs(request_timeout_secs)
                .max_concurrent_requests(max_concurrent_requests)
                .retry_policy(RetryPolicy::default())
                .build();
            prop_assert_eq!(res.is_ok(), expected_ok);
        }

        #[test]
        fn logging_config_new_matches_validation_rules(
            dir in "[A-Za-z0-9/_-]{0,16}",
            file_prefix in "[ \tA-Za-z0-9._-]{0,20}",
            max_files: usize,
        ) {
            let expected_ok = !dir.is_empty() && !file_prefix.trim().is_empty() && max_files > 0;
            let res = LoggingConfig::builder()
                .dir(PathBuf::from(dir))
                .file_prefix(file_prefix)
                .rotation(LogRotation::Daily)
                .max_files(max_files)
                .build();
            prop_assert_eq!(res.is_ok(), expected_ok);
        }

        #[test]
        fn keyword_criterion_new_enforces_non_empty_word_and_positive_finite_weight(
            word in "[ \tA-Za-z0-9_-]{0,16}",
            weight: f64,
        ) {
            let expected_ok = !word.trim().is_empty() && weight.is_finite() && weight > 0.0;
            let res = KeywordCriterion::new(word, weight);
            prop_assert_eq!(res.is_ok(), expected_ok);
        }

        #[test]
        fn app_config_new_rejects_duplicate_or_empty_site_and_feed_ids(
            site_ids in prop::collection::vec("[ \ta-c]{0,3}", 0..6),
            feed_ids in prop::collection::vec("[ \ta-c]{0,3}", 0..6),
        ) {
            let sites_valid = {
                let mut seen = HashSet::new();
                site_ids.iter().all(|id| !id.trim().is_empty() && seen.insert(id.as_str()))
            };
            let feeds_valid = {
                let mut seen = HashSet::new();
                feed_ids.iter().all(|id| !id.trim().is_empty() && seen.insert(id.as_str()))
            };

            let sites = site_ids
                .into_iter()
                .map(|id| HttpSiteConfig {
                    id,
                    name: "Site".into(),
                    url: "http://mock.local".into(),
                    category: "test".into(),
                })
                .collect();
            let feeds = feed_ids
                .into_iter()
                .map(|id| NewsFeedConfig {
                    id,
                    name: "Feed".into(),
                    group: "cyber".into(),
                    url: "http://mock.local/rss".into(),
                    keywords: vec![],
                })
                .collect();

            let res = AppConfig::builder()
                .server(
                    ServerConfig::builder()
                        .bind_addr("127.0.0.1:0")
                        .poll_interval_secs(60)
                        .request_timeout_secs(5)
                        .max_concurrent_requests(4)
                        .build()
                        .unwrap(),
                )
                .dss(DssConfig {
                    rules_path: PathBuf::from("rules.toml"),
                    state_path: PathBuf::from("state.json"),
                })
                .http_sites(sites)
                .news_feeds(feeds)
                .build();

            prop_assert_eq!(res.is_ok(), sites_valid && feeds_valid);
        }
    }
}
