//! Завантаження та обробка офіційних валютних курсів API НБУ.

use hashbrown::{
    HashMap,
    HashSet,
};
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};

use crate::config::NbuMetricsConfig;
use crate::error::AgentResult;
use crate::feeds::read_bounded_bytes;

/// Максимальний розмір JSON-відповіді API НБУ (5 МБ).
pub const MAX_NBU_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct RawNbuEntry {
    cc: String,
    txt: String,
    rate: f64,
    exchangedate: String,
}

/// Офіційний курс валюти НБУ з обчисленою зміною відносно попереднього циклу.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CurrencyRate {
    pub cc: String,
    pub txt: String,
    pub rate: f64,
    pub previous_rate: Option<f64>,
    pub delta_pct: Option<f64>,
    pub exchangedate: String,
}

/// Завантажує курси валют з API НБУ, перевіряє HTTP-статус і ліміт розміру тіла.
///
/// # Errors
///
/// Повертає [`crate::error::AgentError::Http`], [`crate::error::AgentError::ResponseTooLarge`]
/// або [`crate::error::AgentError::Json`].
#[tracing::instrument(skip(client, cfg, previous))]
pub async fn fetch_nbu_rates(
    client: &Client,
    cfg: &NbuMetricsConfig,
    previous: &[CurrencyRate],
) -> AgentResult<Vec<CurrencyRate>> {
    if !cfg.enabled {
        return Ok(Vec::new());
    }

    let resp = client.get(&cfg.url).send().await?.error_for_status()?;
    let bytes = read_bounded_bytes(resp, MAX_NBU_BYTES).await?;
    let raw_entries: Vec<RawNbuEntry> = serde_json::from_slice(&bytes)?;
    Ok(extract_configured_rates(raw_entries, cfg, previous))
}

fn extract_configured_rates(
    raw_entries: Vec<RawNbuEntry>,
    cfg: &NbuMetricsConfig,
    previous: &[CurrencyRate],
) -> Vec<CurrencyRate> {
    let wanted: HashSet<String> = cfg.currencies.iter().map(|c| c.to_uppercase()).collect();

    let prev_map: HashMap<&str, f64> = previous.iter().map(|c| (c.cc.as_str(), c.rate)).collect();

    let mut out = Vec::with_capacity(wanted.len());
    for entry in raw_entries {
        let code = entry.cc.to_uppercase();
        if !wanted.is_empty() && !wanted.contains(&code) {
            continue;
        }
        let previous_rate = prev_map.get(code.as_str()).copied();
        let delta_pct = previous_rate.and_then(|prev| {
            if prev.abs() > f64::EPSILON {
                let pct = ((entry.rate - prev) / prev) * 100.0;
                Some((pct * 10_000.0).round() / 10_000.0)
            } else {
                None
            }
        });

        out.push(CurrencyRate {
            cc: code,
            txt: entry.txt,
            rate: entry.rate,
            previous_rate,
            delta_pct,
            exchangedate: entry.exchangedate,
        });
    }

    out.sort_by_key(|item| {
        cfg.currencies
            .iter()
            .position(|c| c.eq_ignore_ascii_case(&item.cc))
            .unwrap_or(usize::MAX)
    });

    out
}

/// Розбирає JSON-рядок відповіді API НБУ та обчислює відсоткові зміни курсів.
///
/// # Errors
///
/// Повертає [`crate::error::AgentError::Json`] при некоректному JSON.
pub fn parse_nbu_json(
    json_str: &str,
    cfg: &NbuMetricsConfig,
    previous: &[CurrencyRate],
) -> AgentResult<Vec<CurrencyRate>> {
    let raw_entries: Vec<RawNbuEntry> = serde_json::from_str(json_str)?;
    Ok(extract_configured_rates(raw_entries, cfg, previous))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn parse_nbu_json_filters_orders_and_computes_deltas(
            usd_prev in 1.0_f64..100.0_f64,
            usd_curr in 1.0_f64..100.0_f64,
            eur_curr in 1.0_f64..100.0_f64,
            pln_curr in 1.0_f64..100.0_f64,
        ) {
            let cfg = NbuMetricsConfig {
                enabled: true,
                url: "http://mock.local/nbu".into(),
                currencies: vec!["EUR".into(), "USD".into()],
            };

            let prev = vec![CurrencyRate {
                cc: "USD".into(),
                txt: "Долар США".into(),
                rate: usd_prev,
                previous_rate: None,
                delta_pct: None,
                exchangedate: "22.11.2026".into(),
            }];

            // Навмисно подаємо у зворотному порядку, у нижньому регістрі та із зайвою валютою PLN
            let json = serde_json::json!([
                { "cc": "pln", "txt": "Злотий", "rate": pln_curr, "exchangedate": "23.11.2026" },
                { "cc": "usd", "txt": "Долар США", "rate": usd_curr, "exchangedate": "23.11.2026" },
                { "cc": "eur", "txt": "Євро", "rate": eur_curr, "exchangedate": "23.11.2026" },
            ])
            .to_string();

            let parsed = parse_nbu_json(&json, &cfg, &prev).unwrap();

            // Відфільтровано PLN і збережено порядок конфігурації [EUR, USD]
            prop_assert_eq!(parsed.len(), 2);
            prop_assert_eq!(parsed[0].cc.as_str(), "EUR");
            prop_assert_eq!(parsed[1].cc.as_str(), "USD");

            // Для EUR не було попереднього значення -> delta_pct == None
            prop_assert!(parsed[0].previous_rate.is_none());
            prop_assert!(parsed[0].delta_pct.is_none());

            // Для USD обчислено точну відсоткову дельту з округленням до 4 знаків
            let expected_delta =
                ((((usd_curr - usd_prev) / usd_prev) * 100.0) * 10_000.0).round() / 10_000.0;
            prop_assert_eq!(parsed[1].previous_rate, Some(usd_prev));
            prop_assert!((parsed[1].delta_pct.unwrap() - expected_delta).abs() < 1e-9);
        }

        #[test]
        fn zero_previous_rate_produces_none_delta_without_nan_or_inf(
            curr in 0.01_f64..1000.0_f64,
        ) {
            let cfg = NbuMetricsConfig {
                enabled: true,
                url: "http://mock.local/nbu".into(),
                currencies: vec!["USD".into()],
            };
            let prev = vec![CurrencyRate {
                cc: "USD".into(),
                txt: "USD".into(),
                rate: 0.0,
                previous_rate: None,
                delta_pct: None,
                exchangedate: "22.11.2026".into(),
            }];
            let json = serde_json::json!([
                { "cc": "USD", "txt": "USD", "rate": curr, "exchangedate": "23.11.2026" },
            ])
            .to_string();

            let parsed = parse_nbu_json(&json, &cfg, &prev).unwrap();
            prop_assert_eq!(parsed.len(), 1);
            prop_assert!(parsed[0].delta_pct.is_none());
        }
    }
}
