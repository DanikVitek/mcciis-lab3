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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use time::OffsetDateTime;

    use super::*;
    use crate::config::HttpSiteConfig;
    use crate::feeds::MatchedKeyword;

    proptest! {
        #[test]
        fn sanitize_key_is_idempotent_and_safe_ascii(raw in ".*") {
            let s1 = sanitize_key(&raw);
            let s2 = sanitize_key(&s1);
            prop_assert_eq!(s1.chars().count(), raw.chars().count());
            prop_assert_eq!(&s1, &s2);
            prop_assert!(s1.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        }

        #[test]
        fn extract_facts_maintains_count_and_ratio_invariants(
            site_flags in prop::collection::vec((any::<bool>(), any::<bool>(), any::<bool>()), 0..8),
            feed_disabled in prop::collection::vec(any::<bool>(), 0..5),
            nbu_disabled: bool,
            article_specs in prop::collection::vec(
                (
                    prop_oneof![Just("cybersecurity"), Just("economics"), Just("general")],
                    0.0_f64..50.0_f64,
                    any::<bool>(),
                    prop::option::of(1000_u32..9999_u32),
                ),
                0..10,
            ),
            usd in 10.0_f64..80.0_f64,
            eur in 10.0_f64..90.0_f64,
            usd_delta in -20.0_f64..20.0_f64,
            eur_delta in -20.0_f64..20.0_f64,
        ) {
            let sites: Vec<HttpSiteState> = site_flags
                .iter()
                .enumerate()
                .map(|(i, &(changed, acknowledged, disabled))| {
                    let mut st = HttpSiteState::from_config(&HttpSiteConfig {
                        id: format!("site-{i}"),
                        name: format!("Site {i}"),
                        url: format!("http://mock/{i}"),
                        category: "test".into(),
                    });
                    st.changed = changed;
                    st.acknowledged = acknowledged;
                    st.health.disabled = disabled;
                    st.body_size = Some((i as u64 + 1) * 100);
                    st
                })
                .collect();

            let feeds: Vec<FeedMonitorState> = feed_disabled
                .iter()
                .enumerate()
                .map(|(i, &disabled)| FeedMonitorState {
                    id: format!("feed-{i}"),
                    name: format!("Feed {i}"),
                    url: format!("http://mock/rss/{i}"),
                    group: "cybersecurity".into(),
                    matched_count: 0,
                    health: SourceHealth {
                        disabled,
                        ..Default::default()
                    },
                })
                .collect();

            let nbu_health = SourceHealth {
                disabled: nbu_disabled,
                ..Default::default()
            };

            let articles: Vec<NewsArticle> = article_specs
                .iter()
                .enumerate()
                .map(|(i, &(group, score, critical_kw, cve_num))| {
                    let matched_keywords = if critical_kw {
                        vec![MatchedKeyword {
                            word: "ransomware".into(),
                            weight: 3.0,
                            in_title: true,
                            in_body: false,
                            contribution: 6.0,
                        }]
                    } else {
                        vec![]
                    };
                    let cves = cve_num
                        .map(|n| vec![format!("CVE-2026-{n:04}")])
                        .unwrap_or_default();
                    NewsArticle {
                        id: format!("{i:016x}"),
                        feed_id: "f".into(),
                        feed_name: "F".into(),
                        group: group.into(),
                        title: format!("Art {i}"),
                        link: format!("http://mock/a/{i}"),
                        published: None,
                        summary: String::new(),
                        matched_keywords,
                        relevance_score: score,
                        cves,
                        fetched_at: OffsetDateTime::UNIX_EPOCH,
                    }
                })
                .collect();

            let currencies = vec![
                CurrencyRate {
                    cc: "USD".into(),
                    txt: "USD".into(),
                    rate: usd,
                    previous_rate: Some(usd),
                    delta_pct: Some(usd_delta),
                    exchangedate: "23.11.2026".into(),
                },
                CurrencyRate {
                    cc: "EUR".into(),
                    txt: "EUR".into(),
                    rate: eur,
                    previous_rate: Some(eur),
                    delta_pct: Some(eur_delta),
                    exchangedate: "23.11.2026".into(),
                },
            ];

            let facts = extract_facts(&sites, &feeds, &articles, &currencies, &nbu_health);

            // Усі витягнуті факти належать WebAgent і мають CF у [0, 1]
            for fact in facts.values() {
                prop_assert_eq!(fact.origin, FactOrigin::WebAgent);
                prop_assert!((0.0..=1.0).contains(&fact.cf));
            }

            let changed = facts["http_changed_sites_count"].value.as_f64().unwrap();
            let monitored = facts["http_monitored_sites_count"].value.as_f64().unwrap();
            let disabled = facts["disabled_sources_count"].value.as_f64().unwrap();
            let total_art = facts["total_matched_articles"].value.as_f64().unwrap();
            let cyber_art = facts["cyber_articles_count"].value.as_f64().unwrap();
            let cyber_crit = facts["cyber_critical_articles"].value.as_f64().unwrap();
            let cve_count = facts["cve_mentions_count"].value.as_f64().unwrap();

            prop_assert!(changed <= monitored);
            prop_assert_eq!(monitored as usize, sites.len());

            let expected_disabled = sites.iter().filter(|s| s.health.disabled).count()
                + feeds.iter().filter(|f| f.health.disabled).count()
                + usize::from(nbu_disabled);
            prop_assert_eq!(disabled as usize, expected_disabled);

            prop_assert_eq!(total_art as usize, articles.len());
            prop_assert!(cyber_crit <= cyber_art);
            prop_assert!(cyber_art <= total_art);

            // detected_cves присутній тоді й лише тоді, коли знайдено хоча б 1 унікальний CVE, і відсортований
            if cve_count > 0.0 {
                let text = facts["detected_cves"].value.as_text();
                let list: Vec<&str> = text.split(", ").collect();
                prop_assert_eq!(list.len(), cve_count as usize);
                for pair in list.windows(2) {
                    prop_assert!(pair[0] < pair[1]);
                }
            } else {
                prop_assert!(!facts.contains_key("detected_cves"));
            }

            // Перевірка відношення EUR/USD та максимального модуля дельти
            let expected_ratio = ((eur / usd) * 10_000.0).round() / 10_000.0;
            prop_assert!((facts["nbu_eur_usd_ratio"].value.as_f64().unwrap() - expected_ratio).abs() < 1e-9);

            let expected_max_delta = (usd_delta.abs().max(eur_delta.abs()) * 10_000.0).round() / 10_000.0;
            prop_assert!((facts["nbu_max_abs_delta_pct"].value.as_f64().unwrap() - expected_max_delta).abs() < 1e-9);
        }
    }
}
