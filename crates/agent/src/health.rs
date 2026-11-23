use serde::{
    Deserialize,
    Serialize,
};
use time::{
    Duration,
    OffsetDateTime,
};

use crate::error::{
    AgentError,
    AgentResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Parse,
    Transient,
}

impl ErrorKind {
    pub fn label_uk(self) -> &'static str {
        match self {
            Self::Parse => "Помилка парсингу",
            Self::Transient => "Тимчасова мережева помилка",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(try_from = "RawRetryPolicy")]
pub struct RetryPolicy {
    pub parse_max_retries: u32,
    pub initial_backoff_secs: u64,
    pub max_backoff_secs: u64,
}

#[derive(Debug, Deserialize)]
struct RawRetryPolicy {
    #[serde(default = "default_parse_max_retries")]
    parse_max_retries: u32,
    #[serde(default = "default_initial_backoff_secs")]
    initial_backoff_secs: u64,
    #[serde(default = "default_max_backoff_secs")]
    max_backoff_secs: u64,
}

impl TryFrom<RawRetryPolicy> for RetryPolicy {
    type Error = AgentError;

    fn try_from(raw: RawRetryPolicy) -> Result<Self, Self::Error> {
        Self::new(
            raw.parse_max_retries,
            raw.initial_backoff_secs,
            raw.max_backoff_secs,
        )
    }
}

impl RetryPolicy {
    pub fn new(
        parse_max_retries: u32,
        initial_backoff_secs: u64,
        max_backoff_secs: u64,
    ) -> AgentResult<Self> {
        if parse_max_retries == 0 {
            return Err(AgentError::Config(
                "parse_max_retries має бути більшим за 0".to_string(),
            ));
        }
        if max_backoff_secs < initial_backoff_secs {
            return Err(AgentError::Config(
                "max_backoff_secs не може бути меншим за initial_backoff_secs".to_string(),
            ));
        }
        Ok(Self {
            parse_max_retries,
            initial_backoff_secs,
            max_backoff_secs,
        })
    }

    pub fn backoff_for_attempt(&self, consecutive_failures: u32) -> Duration {
        let exp = consecutive_failures.saturating_sub(1).min(20);
        let factor = 1_u64.checked_shl(exp).unwrap_or(u64::MAX);
        let secs = self
            .initial_backoff_secs
            .saturating_mul(factor)
            .min(self.max_backoff_secs);
        Duration::seconds(secs as i64)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            parse_max_retries: default_parse_max_retries(),
            initial_backoff_secs: default_initial_backoff_secs(),
            max_backoff_secs: default_max_backoff_secs(),
        }
    }
}

fn default_parse_max_retries() -> u32 {
    3
}

fn default_initial_backoff_secs() -> u64 {
    30
}

fn default_max_backoff_secs() -> u64 {
    1800
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SourceHealth {
    pub disabled: bool,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub last_error_kind: Option<ErrorKind>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub next_retry_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_checked: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureTransition {
    Throttled { attempt: u32, backoff_secs: i64 },
    Disabled { attempts: u32 },
}

impl SourceHealth {
    pub fn should_poll(&self, now: OffsetDateTime) -> bool {
        if self.disabled {
            return false;
        }
        if let Some(next_retry) = self.next_retry_at
            && now < next_retry
        {
            return false;
        }
        true
    }

    pub fn is_throttled(&self, now: OffsetDateTime) -> bool {
        !self.disabled && self.next_retry_at.is_some_and(|t| now < t)
    }

    pub fn record_success(&mut self, now: OffsetDateTime) {
        self.last_checked = Some(now);
        self.disabled = false;
        self.consecutive_failures = 0;
        self.last_error = None;
        self.last_error_kind = None;
        self.next_retry_at = None;
    }

    pub fn record_failure(
        &mut self,
        now: OffsetDateTime,
        kind: ErrorKind,
        message: String,
        policy: &RetryPolicy,
    ) -> FailureTransition {
        self.last_checked = Some(now);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error = Some(message);
        self.last_error_kind = Some(kind);

        if kind == ErrorKind::Parse && self.consecutive_failures >= policy.parse_max_retries {
            self.disabled = true;
            self.next_retry_at = None;
            FailureTransition::Disabled {
                attempts: self.consecutive_failures,
            }
        } else {
            let backoff = policy.backoff_for_attempt(self.consecutive_failures);
            self.next_retry_at = Some(now + backoff);
            FailureTransition::Throttled {
                attempt: self.consecutive_failures,
                backoff_secs: backoff.whole_seconds(),
            }
        }
    }

    pub fn reset_manual(&mut self) {
        self.disabled = false;
        self.consecutive_failures = 0;
        self.last_error = None;
        self.last_error_kind = None;
        self.next_retry_at = None;
    }
}
