//! Контроль відмовостійкості зовнішніх джерел (експоненційний backoff та авто-вимкнення).

use bon::bon;
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

/// Класифікація помилки зовнішнього джерела.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Структурна помилка формату/парсингу (після серії повторів джерело вимикається).
    Parse,
    /// Тимчасова мережева або серверна помилка (backoff без вимкнення).
    Transient,
}

impl ErrorKind {
    /// Повертає локалізовану назву типу помилки українською.
    #[must_use]
    pub const fn label_uk(self) -> &'static str {
        match self {
            Self::Parse => "Помилка парсингу",
            Self::Transient => "Тимчасова мережева помилка",
        }
    }
}

/// Політика повторних спроб і експоненційного backoff для зовнішніх джерел.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "RawRetryPolicy")]
pub struct RetryPolicy {
    pub parse_max_retries: u32,
    pub initial_backoff_secs: u64,
    pub max_backoff_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
        Self::builder()
            .parse_max_retries(raw.parse_max_retries)
            .initial_backoff_secs(raw.initial_backoff_secs)
            .max_backoff_secs(raw.max_backoff_secs)
            .build()
    }
}

#[bon]
impl RetryPolicy {
    /// Створює та валідує політику повторних спроб через типобезпечний builder (`bon`).
    ///
    /// # Errors
    ///
    /// Повертає [`AgentError::Config`], якщо `parse_max_retries == 0`
    /// або `max_backoff_secs < initial_backoff_secs`.
    #[builder(derive(Debug))]
    pub fn new(
        #[builder(default = default_parse_max_retries())] parse_max_retries: u32,
        #[builder(default = default_initial_backoff_secs())] initial_backoff_secs: u64,
        #[builder(default = default_max_backoff_secs())] max_backoff_secs: u64,
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

    /// Обчислює тривалість експоненційного backoff для заданого номера невдалої спроби.
    #[must_use]
    pub fn backoff_for_attempt(&self, consecutive_failures: u32) -> Duration {
        let exp = consecutive_failures.saturating_sub(1).min(20);
        let factor = 1_u64.checked_shl(exp).unwrap_or(u64::MAX);
        let secs = self
            .initial_backoff_secs
            .saturating_mul(factor)
            .min(self.max_backoff_secs);
        Duration::seconds(i64::try_from(secs).unwrap_or(i64::MAX))
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

const fn default_parse_max_retries() -> u32 {
    3
}

const fn default_initial_backoff_secs() -> u64 {
    30
}

const fn default_max_backoff_secs() -> u64 {
    1800
}

/// Поточний стан здоров'я та backoff зовнішнього джерела даних.
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

/// Результат переходу стану джерела після невдалого опитування.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureTransition {
    /// Джерело тимчасово відкладене на `backoff_secs` секунд.
    Throttled { attempt: u32, backoff_secs: i64 },
    /// Джерело автоматично вимкнене після серії помилок парсингу.
    Disabled { attempts: u32 },
}

impl SourceHealth {
    /// Перевіряє, чи дозволено опитувати джерело в момент `now`.
    #[must_use]
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

    /// Повертає `true`, якщо джерело активне, але тимчасово перебуває в backoff.
    #[must_use]
    pub fn is_throttled(&self, now: OffsetDateTime) -> bool {
        !self.disabled && self.next_retry_at.is_some_and(|t| now < t)
    }

    /// Фіксує успішне опитування й скидає лічильники помилок.
    pub fn record_success(&mut self, now: OffsetDateTime) {
        self.last_checked = Some(now);
        self.disabled = false;
        self.consecutive_failures = 0;
        self.last_error = None;
        self.last_error_kind = None;
        self.next_retry_at = None;
    }

    /// Фіксує невдале опитування та обчислює перехід стану (backoff або вимкнення).
    pub fn record_failure(
        &mut self,
        now: OffsetDateTime,
        kind: ErrorKind,
        message: impl Into<String>,
        policy: &RetryPolicy,
    ) -> FailureTransition {
        self.last_checked = Some(now);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error = Some(message.into());
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

    /// Ручне скидання стану помилок та повторне увімкнення джерела.
    pub fn reset_manual(&mut self) {
        self.disabled = false;
        self.consecutive_failures = 0;
        self.last_error = None;
        self.last_error_kind = None;
        self.next_retry_at = None;
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn retry_policy_validation_matches_invariants(
            max_retries: u32,
            initial: u64,
            max_backoff: u64,
        ) {
            let res = RetryPolicy::builder()
                .parse_max_retries(max_retries)
                .initial_backoff_secs(initial)
                .max_backoff_secs(max_backoff)
                .build();
            let expected_ok = max_retries > 0 && max_backoff >= initial;
            prop_assert_eq!(res.is_ok(), expected_ok);
        }

        #[test]
        fn backoff_is_monotonic_and_bounded(
            max_retries: u32,
            a: u64,
            b: u64,
            a1: u32,
            a2: u32,
        ) {
            let max_retries = max_retries.max(1);
            let initial = a.min(b);
            let max_backoff = a.max(b);
            let policy = RetryPolicy::builder()
                .parse_max_retries(max_retries)
                .initial_backoff_secs(initial)
                .max_backoff_secs(max_backoff)
                .build()
                .unwrap();

            let (lo, hi) = (a1.min(a2), a1.max(a2));
            let b_lo = policy.backoff_for_attempt(lo).whole_seconds();
            let b_hi = policy.backoff_for_attempt(hi).whole_seconds();

            let expected_min = i64::try_from(initial).unwrap_or(i64::MAX);
            let expected_max = i64::try_from(max_backoff).unwrap_or(i64::MAX);

            prop_assert!(b_lo >= expected_min);
            prop_assert!(b_lo <= expected_max);
            prop_assert!(b_hi >= b_lo);
            prop_assert!(b_hi <= expected_max);
        }

        #[test]
        fn parse_errors_throttle_then_disable_exactly_at_threshold(
            max_retries in 1_u32..=10,
            initial: u32,
            extra: u32,
        ) {
            let initial = u64::from(initial.max(1));
            let max_backoff = initial + u64::from(extra);
            let policy = RetryPolicy::builder()
                .parse_max_retries(max_retries)
                .initial_backoff_secs(initial)
                .max_backoff_secs(max_backoff)
                .build()
                .unwrap();
            let mut health = SourceHealth::default();
            let mut now = OffsetDateTime::UNIX_EPOCH + Duration::days(10_000);

            for attempt in 1..max_retries {
                let tr = health.record_failure(now, ErrorKind::Parse, "parse err", &policy);
                match tr {
                    FailureTransition::Throttled {
                        attempt: a,
                        backoff_secs,
                    } => {
                        prop_assert_eq!(a, attempt);
                        prop_assert!(backoff_secs >= i64::try_from(initial).unwrap());
                        prop_assert!(backoff_secs <= i64::try_from(max_backoff).unwrap());
                        prop_assert!(!health.disabled);
                        prop_assert!(health.is_throttled(now));
                        prop_assert!(!health.should_poll(now));
                        prop_assert!(health.should_poll(now + Duration::seconds(backoff_secs)));
                        now += Duration::seconds(backoff_secs);
                    }
                    FailureTransition::Disabled { .. } => {
                        prop_assert!(false, "disabled too early on attempt {attempt}/{max_retries}");
                    }
                }
            }

            let final_tr = health.record_failure(now, ErrorKind::Parse, "parse err", &policy);
            prop_assert_eq!(final_tr, FailureTransition::Disabled { attempts: max_retries });
            prop_assert!(health.disabled);
            prop_assert!(!health.is_throttled(now));
            prop_assert!(!health.should_poll(now + Duration::days(365)));

            health.reset_manual();
            prop_assert!(!health.disabled);
            prop_assert_eq!(health.consecutive_failures, 0);
            prop_assert!(health.should_poll(now));
        }

        #[test]
        fn transient_errors_never_permanently_disable_source(
            max_retries: u32,
            initial: u32,
            extra: u32,
            failures in 1_usize..=25,
        ) {
            let initial = u64::from(initial);
            let max_backoff = initial + u64::from(extra);
            let policy = RetryPolicy::builder()
                .parse_max_retries(max_retries.max(1))
                .initial_backoff_secs(initial)
                .max_backoff_secs(max_backoff)
                .build()
                .unwrap();
            let mut health = SourceHealth::default();
            let base = OffsetDateTime::UNIX_EPOCH + Duration::days(10_000);

            for i in 0..failures {
                let now = base + Duration::seconds(i64::try_from(i).unwrap() * 1000);
                let tr = health.record_failure(now, ErrorKind::Transient, "timeout", &policy);
                let is_throttled = matches!(tr, FailureTransition::Throttled { .. });
                prop_assert!(is_throttled);
                prop_assert!(!health.disabled);
            }

            health.record_success(base + Duration::days(1));
            prop_assert!(!health.disabled);
            prop_assert_eq!(health.consecutive_failures, 0);
            prop_assert!(health.last_error.is_none());
            prop_assert!(health.should_poll(base + Duration::days(1)));
        }
    }
}
