use std::collections::BTreeMap;

use dss::{
    Fact,
    FactOrigin,
    FactValue,
};
use hashbrown::HashSet;

use crate::FeedMonitorState;
use crate::feeds::NewsArticle;
use crate::health::SourceHealth;
use crate::http_monitor::HttpSiteState;
use crate::nbu::CurrencyRate;

const CRITICAL_CYBER_TERMS: &[&str] = &[
    "critical",
    "zero-day",
    "ransomware",
    "rce",
    "exploit",
    "breach",
];

/// Підсистема автоматичного поповнення бази знань (п. 9–10 методички):
/// трансформує зібрані Web-агентом дані (зміни HTTP-сайтів, проранжовані новини,
/// виявлені CVE, стан джерел та числові курси НБУ) у формалізовані факти ІСППР.
pub fn extract_facts(
    sites: &[HttpSiteState],
    feeds: &[FeedMonitorState],
    articles: &[NewsArticle],
    currencies: &[CurrencyRate],
    nbu_health: &SourceHealth,
) -> BTreeMap<String, Fact> {
    let mut facts = BTreeMap::new();

    // 1. Факти з HTTP/HTTPS-моніторингу сайтів і стану джерел
    let changed_unacked = sites
        .iter()
        .filter(|s| s.is_visible_in_changed_table())
        .count();
    let total_sites = sites.len();
    let error_sites = sites
        .iter()
        .filter(|s| s.health.last_error.is_some())
        .count();
    let disabled_sources = sites.iter().filter(|s| s.health.disabled).count()
        + feeds.iter().filter(|f| f.health.disabled).count()
        + usize::from(nbu_health.disabled);

    insert_fact(
        &mut facts,
        "http_changed_sites_count",
        FactValue::Number(changed_unacked as f64),
        "HTTP-монітор Web-агента",
        1.0,
    );
    insert_fact(
        &mut facts,
        "http_monitored_sites_count",
        FactValue::Number(total_sites as f64),
        "HTTP-монітор Web-агента",
        1.0,
    );
    insert_fact(
        &mut facts,
        "http_error_sites_count",
        FactValue::Number(error_sites as f64),
        "HTTP-монітор Web-агента",
        1.0,
    );
    insert_fact(
        &mut facts,
        "disabled_sources_count",
        FactValue::Number(disabled_sources as f64),
        "Контролер відмовостійкості Web-агента",
        1.0,
    );

    for site in sites {
        if let Some(size) = site.body_size {
            insert_fact(
                &mut facts,
                format!("site_{}_size_bytes", sanitize_key(&site.id)),
                FactValue::Number(size as f64),
                format!("HTTP-монітор ({})", site.name),
                1.0,
            );
        }
        insert_fact(
            &mut facts,
            format!("site_{}_changed", sanitize_key(&site.id)),
            FactValue::Bool(site.is_visible_in_changed_table()),
            format!("HTTP-монітор ({})", site.name),
            1.0,
        );
    }

    // 2. Факти з аналізу новинних стрічок (кібербезпека, технології, економіка)
    let mut cyber_articles = 0usize;
    let mut cyber_critical_articles = 0usize;
    let mut econ_articles = 0usize;
    let mut unique_cves = HashSet::new();
    let mut top_score = 0.0_f64;

    for article in articles {
        if article.relevance_score > top_score {
            top_score = article.relevance_score;
        }
        for cve in &article.cves {
            unique_cves.insert(cve.clone());
        }

        let group_lower = article.group.to_lowercase();
        let is_cyber_group = group_lower.contains("cyber") || group_lower.contains("security");
        let is_econ_group = group_lower.contains("econ") || group_lower.contains("finance");

        if is_cyber_group {
            cyber_articles += 1;
            let has_critical_term = article.matched_keywords.iter().any(|mk| {
                let w = mk.word.to_lowercase();
                CRITICAL_CYBER_TERMS.iter().any(|t| w.contains(t))
            });
            if has_critical_term || !article.cves.is_empty() {
                cyber_critical_articles += 1;
            }
        }

        if is_econ_group {
            econ_articles += 1;
        }
    }

    insert_fact(
        &mut facts,
        "total_matched_articles",
        FactValue::Number(articles.len() as f64),
        "Новинний аналізатор Web-агента",
        0.95,
    );
    insert_fact(
        &mut facts,
        "cyber_articles_count",
        FactValue::Number(cyber_articles as f64),
        "Новинний аналізатор (кібербезпека)",
        0.92,
    );
    insert_fact(
        &mut facts,
        "cyber_critical_articles",
        FactValue::Number(cyber_critical_articles as f64),
        "Новинний аналізатор (критичні індикатори)",
        0.90,
    );
    insert_fact(
        &mut facts,
        "econ_articles_count",
        FactValue::Number(econ_articles as f64),
        "Новинний аналізатор (економіка)",
        0.88,
    );
    insert_fact(
        &mut facts,
        "cve_mentions_count",
        FactValue::Number(unique_cves.len() as f64),
        "Сканер CVE Web-агента",
        0.96,
    );
    insert_fact(
        &mut facts,
        "top_article_score",
        FactValue::Number((top_score * 100.0).round() / 100.0),
        "Модуль ранжування новин",
        0.92,
    );

    if !unique_cves.is_empty() {
        let mut sorted_cves: Vec<String> = unique_cves.into_iter().collect();
        sorted_cves.sort();
        insert_fact(
            &mut facts,
            "detected_cves",
            FactValue::Text(sorted_cves.join(", ")),
            "Сканер CVE Web-агента",
            0.96,
        );
    }

    // 3. Числові динамічні факти з API НБУ
    let mut max_abs_delta_pct = 0.0_f64;
    let mut usd_rate = None;
    let mut eur_rate = None;

    for cur in currencies {
        let code_lower = cur.cc.to_lowercase();
        let source = format!("API НБУ ({})", cur.exchangedate);
        insert_fact(
            &mut facts,
            format!("nbu_rate_{code_lower}"),
            FactValue::Number(cur.rate),
            &source,
            1.0,
        );
        if let Some(delta) = cur.delta_pct {
            let abs_delta = delta.abs();
            if abs_delta > max_abs_delta_pct {
                max_abs_delta_pct = abs_delta;
            }
            insert_fact(
                &mut facts,
                format!("nbu_delta_pct_{code_lower}"),
                FactValue::Number(delta),
                &source,
                0.98,
            );
        }

        match cur.cc.as_str() {
            "USD" => usd_rate = Some(cur.rate),
            "EUR" => eur_rate = Some(cur.rate),
            _ => {}
        }
    }

    if !currencies.is_empty() {
        insert_fact(
            &mut facts,
            "nbu_max_abs_delta_pct",
            FactValue::Number((max_abs_delta_pct * 10_000.0).round() / 10_000.0),
            "Аналізатор волатильності НБУ",
            0.98,
        );
    }

    if let (Some(usd), Some(eur)) = (usd_rate, eur_rate)
        && usd > f64::EPSILON
    {
        let ratio = ((eur / usd) * 10_000.0).round() / 10_000.0;
        insert_fact(
            &mut facts,
            "nbu_eur_usd_ratio",
            FactValue::Number(ratio),
            "Аналізатор курсів НБУ",
            1.0,
        );
    }

    facts
}

fn insert_fact(
    facts: &mut BTreeMap<String, Fact>,
    key: impl Into<String>,
    value: FactValue,
    source: impl Into<String>,
    cf: f64,
) {
    let key = key.into();
    facts.insert(
        key.clone(),
        Fact::new(key, value, source, cf, FactOrigin::WebAgent),
    );
}

fn sanitize_key(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}
