use std::path::{
    Path,
    PathBuf,
};

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
        Self::new(
            raw.server,
            raw.logging,
            raw.dss,
            raw.nbu_metrics,
            raw.http_sites,
            raw.news_feeds,
        )
    }
}

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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minutely => "minutely",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawLoggingConfig")]
pub struct LoggingConfig {
    pub dir: PathBuf,
    pub file_prefix: String,
    pub rotation: LogRotation,
    pub max_files: usize,
}

#[derive(Debug, Deserialize)]
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
        Self::new(raw.dir, raw.file_prefix, raw.rotation, raw.max_files)
    }
}

impl LoggingConfig {
    pub fn new(
        dir: PathBuf,
        file_prefix: String,
        rotation: LogRotation,
        max_files: usize,
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
        Self::new(
            raw.bind_addr,
            raw.poll_interval_secs,
            raw.request_timeout_secs,
            raw.max_concurrent_requests,
            raw.retry_policy,
        )
    }
}

impl ServerConfig {
    pub fn new(
        bind_addr: String,
        poll_interval_secs: u64,
        request_timeout_secs: u64,
        max_concurrent_requests: usize,
        retry_policy: RetryPolicy,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DssConfig {
    pub rules_path: PathBuf,
    pub state_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpSiteConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default = "default_category")]
    pub category: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewsFeedConfig {
    pub id: String,
    pub name: String,
    pub group: String,
    pub url: String,
    #[serde(default)]
    pub keywords: Vec<KeywordCriterion>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RawKeywordCriterion")]
pub struct KeywordCriterion {
    pub word: String,
    pub weight: f64,
}

#[derive(Debug, Deserialize)]
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
    pub fn new(word: String, weight: f64) -> AgentResult<Self> {
        if word.trim().is_empty() {
            return Err(AgentError::Config(
                "Ключове слово не може бути порожнім".to_string(),
            ));
        }
        if !weight.is_finite() || weight <= 0.0 {
            return Err(AgentError::Config(format!(
                "Вага ключового слова `{word}` має бути скінченним додатним числом"
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

fn default_log_max_files() -> usize {
    7
}

fn default_poll_interval() -> u64 {
    300
}

fn default_request_timeout() -> u64 {
    15
}

fn default_max_concurrent() -> usize {
    8
}

fn default_true() -> bool {
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

fn default_weight() -> f64 {
    1.0
}

impl AppConfig {
    pub fn new(
        server: ServerConfig,
        logging: LoggingConfig,
        dss: DssConfig,
        nbu_metrics: NbuMetricsConfig,
        http_sites: Vec<HttpSiteConfig>,
        news_feeds: Vec<NewsFeedConfig>,
    ) -> AgentResult<Self> {
        {
            let mut seen_site_ids = HashSet::new();
            for site in &http_sites {
                if site.id.trim().is_empty() || site.url.trim().is_empty() {
                    return Err(AgentError::Config(
                        "HTTP-сайт повинен мати непорожні id та url".to_string(),
                    ));
                }
                if !seen_site_ids.insert(site.id.as_str()) {
                    return Err(AgentError::Config(format!(
                        "Дубльований ідентифікатор HTTP-сайту: `{}`",
                        site.id
                    )));
                }
            }
        }

        {
            let mut seen_feed_ids = HashSet::new();
            for feed in &news_feeds {
                if feed.id.trim().is_empty() || feed.url.trim().is_empty() {
                    return Err(AgentError::Config(
                        "Стрічка новин повинна мати непорожні id та url".to_string(),
                    ));
                }
                if !seen_feed_ids.insert(feed.id.as_str()) {
                    return Err(AgentError::Config(format!(
                        "Дубльований ідентифікатор стрічки новин: `{}`",
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

    pub fn load_from_file(path: impl AsRef<Path>) -> AgentResult<Self> {
        let raw_toml = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&raw_toml)?;
        Ok(config)
    }
}
