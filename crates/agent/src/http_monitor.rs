use reqwest::{
    Client,
    header,
};
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use time::OffsetDateTime;

use crate::config::HttpSiteConfig;
use crate::health::{
    ErrorKind,
    FailureTransition,
    RetryPolicy,
    SourceHealth,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpSiteState {
    pub id: String,
    pub name: String,
    pub url: String,
    pub category: String,
    pub status_code: Option<u16>,
    pub content_length: Option<u64>,
    pub body_size: Option<u64>,
    pub previous_size: Option<u64>,
    pub content_hash: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub changed: bool,
    pub acknowledged: bool,
    pub change_count: u64,
    #[serde(default)]
    pub health: SourceHealth,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_changed: Option<OffsetDateTime>,
}

impl HttpSiteState {
    pub fn from_config(cfg: &HttpSiteConfig) -> Self {
        Self {
            id: cfg.id.clone(),
            name: cfg.name.clone(),
            url: cfg.url.clone(),
            category: cfg.category.clone(),
            status_code: None,
            content_length: None,
            body_size: None,
            previous_size: None,
            content_hash: None,
            etag: None,
            last_modified: None,
            changed: false,
            acknowledged: false,
            change_count: 0,
            health: SourceHealth::default(),
            last_changed: None,
        }
    }

    pub fn size_delta(&self) -> Option<i64> {
        match (self.body_size, self.previous_size) {
            (Some(curr), Some(prev)) => {
                let diff = i128::from(curr) - i128::from(prev);
                Some(diff.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
            }
            _ => None,
        }
    }

    pub fn is_visible_in_changed_table(&self) -> bool {
        !self.health.disabled && self.changed && !self.acknowledged
    }
}

#[derive(Debug, Clone)]
pub struct SiteProbe {
    pub status_code: u16,
    pub content_length: Option<u64>,
    pub body_size: u64,
    pub content_hash: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SiteProbeError {
    pub message: String,
    pub kind: ErrorKind,
}

pub async fn probe_site(client: &Client, url: &str) -> Result<SiteProbe, SiteProbeError> {
    let head_meta = match client.head(url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let content_length = resp.content_length();
            let etag = header_to_string(resp.headers().get(header::ETAG));
            let last_modified = header_to_string(resp.headers().get(header::LAST_MODIFIED));
            Some((content_length, etag, last_modified))
        }
        _ => None,
    };

    let resp = client.get(url).send().await.map_err(|e| SiteProbeError {
        kind: if e.is_builder() || e.is_decode() {
            ErrorKind::Parse
        } else {
            ErrorKind::Transient
        },
        message: format!("Помилка запиту: {e}"),
    })?;

    let status_code = resp.status().as_u16();
    let get_content_length = resp.content_length();
    let get_etag = header_to_string(resp.headers().get(header::ETAG));
    let get_last_modified = header_to_string(resp.headers().get(header::LAST_MODIFIED));

    let bytes = resp.bytes().await.map_err(|e| SiteProbeError {
        kind: if e.is_decode() {
            ErrorKind::Parse
        } else {
            ErrorKind::Transient
        },
        message: format!("Помилка читання/декодування тіла відповіді: {e}"),
    })?;

    let body_size = bytes.len() as u64;
    let content_hash = hex::encode(Sha256::digest(&bytes));

    let (head_len, head_etag, head_lm) = head_meta.unwrap_or((None, None, None));

    Ok(SiteProbe {
        status_code,
        content_length: get_content_length.or(head_len),
        body_size,
        content_hash,
        etag: get_etag.or(head_etag),
        last_modified: get_last_modified.or(head_lm),
    })
}

pub fn apply_probe(
    state: &mut HttpSiteState,
    result: Result<SiteProbe, SiteProbeError>,
    policy: &RetryPolicy,
) -> Option<FailureTransition> {
    let now = OffsetDateTime::now_utc();

    match result {
        Ok(probe) => {
            state.health.record_success(now);
            state.status_code = Some(probe.status_code);

            let is_first_observation = state.body_size.is_none() && state.content_hash.is_none();
            let size_changed = state.body_size.is_some_and(|prev| prev != probe.body_size);
            let hash_changed = state
                .content_hash
                .as_ref()
                .is_some_and(|prev| prev != &probe.content_hash);

            if is_first_observation {
                state.previous_size = None;
                state.changed = true;
                state.acknowledged = false;
                state.change_count = 1;
                state.last_changed = Some(now);
            } else if size_changed || hash_changed {
                state.previous_size = state.body_size;
                state.changed = true;
                state.acknowledged = false;
                state.change_count = state.change_count.saturating_add(1);
                state.last_changed = Some(now);
            }

            state.content_length = probe.content_length;
            state.body_size = Some(probe.body_size);
            state.content_hash = Some(probe.content_hash);
            state.etag = probe.etag;
            state.last_modified = probe.last_modified;
            None
        }
        Err(err) => Some(
            state
                .health
                .record_failure(now, err.kind, err.message, policy),
        ),
    }
}

fn header_to_string(val: Option<&header::HeaderValue>) -> Option<String> {
    val.and_then(|v| v.to_str().ok()).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn sample_site() -> HttpSiteState {
        HttpSiteState::from_config(&HttpSiteConfig {
            id: "site-1".into(),
            name: "Sample".into(),
            url: "http://example.local".into(),
            category: "test".into(),
        })
    }

    fn make_probe(size: u64, hash: String) -> SiteProbe {
        SiteProbe {
            status_code: 200,
            content_length: Some(size),
            body_size: size,
            content_hash: hash,
            etag: None,
            last_modified: None,
        }
    }

    proptest! {
        #[test]
        fn apply_probe_tracks_changes_deltas_and_acknowledgements(
            s1: u64,
            s2: u64,
            h1 in "[0-9a-f]{8}",
            h2 in "[0-9a-f]{8}",
        ) {
            let policy = RetryPolicy::default();
            let mut state = sample_site();

            // Перша фіксація
            prop_assert!(apply_probe(&mut state, Ok(make_probe(s1, h1.clone())), &policy).is_none());
            prop_assert!(state.is_visible_in_changed_table());
            prop_assert_eq!(state.change_count, 1);
            prop_assert_eq!(state.size_delta(), None);

            // Позначаємо переглянутим
            state.acknowledged = true;
            prop_assert!(!state.is_visible_in_changed_table());

            // Повторне опитування без змін не скидає acknowledged і не змінює change_count
            prop_assert!(apply_probe(&mut state, Ok(make_probe(s1, h1.clone())), &policy).is_none());
            prop_assert_eq!(state.change_count, 1);
            prop_assert!(state.acknowledged);
            prop_assert!(!state.is_visible_in_changed_table());

            // Наступне опитування з довільними (s2, h2)
            prop_assert!(apply_probe(&mut state, Ok(make_probe(s2, h2.clone())), &policy).is_none());
            let expected_changed = s1 != s2 || h1 != h2;
            if expected_changed {
                prop_assert_eq!(state.change_count, 2);
                prop_assert!(state.is_visible_in_changed_table());
                let expected_delta = (i128::from(s2) - i128::from(s1))
                    .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
                prop_assert_eq!(state.size_delta(), Some(expected_delta));
            } else {
                prop_assert_eq!(state.change_count, 1);
                prop_assert!(!state.is_visible_in_changed_table());
            }
        }

        #[test]
        fn disabled_site_is_never_visible_in_changed_table(
            changed: bool,
            acknowledged: bool,
        ) {
            let mut state = sample_site();
            state.changed = changed;
            state.acknowledged = acknowledged;
            state.health.disabled = true;
            prop_assert!(!state.is_visible_in_changed_table());
        }
    }
}
