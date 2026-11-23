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

#[derive(Debug, Clone, Deserialize)]
struct RawNbuEntry {
    cc: String,
    txt: String,
    rate: f64,
    exchangedate: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CurrencyRate {
    pub cc: String,
    pub txt: String,
    pub rate: f64,
    pub previous_rate: Option<f64>,
    pub delta_pct: Option<f64>,
    pub exchangedate: String,
}

pub async fn fetch_nbu_rates(
    client: &Client,
    cfg: &NbuMetricsConfig,
    previous: &[CurrencyRate],
) -> AgentResult<Vec<CurrencyRate>> {
    if !cfg.enabled {
        return Ok(Vec::new());
    }

    let raw_entries: Vec<RawNbuEntry> = client.get(&cfg.url).send().await?.json().await?;
    Ok(extract_configured_rates(raw_entries, cfg, previous))
}

fn extract_configured_rates(
    raw_entries: Vec<RawNbuEntry>,
    cfg: &NbuMetricsConfig,
    previous: &[CurrencyRate],
) -> Vec<CurrencyRate> {
    let wanted: HashSet<String> = cfg.currencies.iter().map(|c| c.to_uppercase()).collect();

    let prev_map: HashMap<&str, f64> = previous.iter().map(|c| (c.cc.as_str(), c.rate)).collect();

    let mut out = Vec::new();
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

pub fn parse_nbu_json(
    json_str: &str,
    cfg: &NbuMetricsConfig,
    previous: &[CurrencyRate],
) -> AgentResult<Vec<CurrencyRate>> {
    let raw_entries: Vec<RawNbuEntry> = serde_json::from_str(json_str)?;
    Ok(extract_configured_rates(raw_entries, cfg, previous))
}
