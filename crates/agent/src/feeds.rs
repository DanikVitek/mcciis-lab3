use feed_rs::parser;
use hashbrown::HashSet;
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use time::OffsetDateTime;

use crate::config::{
    KeywordCriterion,
    NewsFeedConfig,
};
use crate::error::AgentResult;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchedKeyword {
    pub word: String,
    pub weight: f64,
    pub in_title: bool,
    pub in_body: bool,
    pub contribution: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewsArticle {
    pub id: String,
    pub feed_id: String,
    pub feed_name: String,
    pub group: String,
    pub title: String,
    pub link: String,
    pub published: Option<String>,
    pub summary: String,
    pub matched_keywords: Vec<MatchedKeyword>,
    pub relevance_score: f64,
    pub cves: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct RawFeedItem {
    pub title: String,
    pub link: String,
    pub published: Option<String>,
    pub summary: String,
}

pub async fn fetch_and_rank_feed(
    client: &Client,
    feed: &NewsFeedConfig,
) -> AgentResult<Vec<NewsArticle>> {
    let bytes = client.get(&feed.url).send().await?.bytes().await?;
    let raw_items = parse_feed_bytes(&bytes)?;
    Ok(filter_and_rank_items(feed, raw_items))
}

pub fn parse_feed_bytes(bytes: &[u8]) -> AgentResult<Vec<RawFeedItem>> {
    let parsed = parser::parse(bytes)?;
    let mut items = Vec::with_capacity(parsed.entries.len());

    for entry in parsed.entries {
        let title = entry
            .title
            .map(|t| html_to_plain_text(&t.content))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let link = entry
            .links
            .into_iter()
            .next()
            .map(|l| l.href)
            .unwrap_or_default();

        let published = entry.published.or(entry.updated).map(|dt| dt.to_string());

        let raw_summary = entry
            .summary
            .map(|s| s.content)
            .or_else(|| entry.content.and_then(|c| c.body))
            .unwrap_or_default();
        let summary = html_to_plain_text(&raw_summary);

        items.push(RawFeedItem {
            title,
            link,
            published,
            summary,
        });
    }

    Ok(items)
}

pub fn filter_and_rank_items(
    feed: &NewsFeedConfig,
    raw_items: Vec<RawFeedItem>,
) -> Vec<NewsArticle> {
    let now = OffsetDateTime::now_utc();
    let mut ranked = Vec::new();

    for item in raw_items {
        let title_lower = item.title.to_lowercase();
        let summary_lower = item.summary.to_lowercase();

        let (matched_keywords, base_score) =
            score_keywords(&feed.keywords, &title_lower, &summary_lower);

        if !feed.keywords.is_empty() && matched_keywords.is_empty() {
            continue;
        }

        let combined_text = format!("{} {}", item.title, item.summary);
        let cves = extract_cves(&combined_text);
        let diversity_bonus = if matched_keywords.len() > 1 {
            (matched_keywords.len() as f64 - 1.0) * 0.75
        } else {
            0.0
        };
        let cve_bonus = if cves.is_empty() { 0.0 } else { 2.0 };
        let relevance_score = ((base_score + diversity_bonus + cve_bonus) * 100.0).round() / 100.0;

        let id = article_id(&feed.id, &item.link, &item.title);
        ranked.push(NewsArticle {
            id,
            feed_id: feed.id.clone(),
            feed_name: feed.name.clone(),
            group: feed.group.clone(),
            title: item.title,
            link: item.link,
            published: item.published,
            summary: item.summary,
            matched_keywords,
            relevance_score,
            cves,
            fetched_at: now,
        });
    }

    ranked.sort_by(|a, b| {
        b.relevance_score
            .total_cmp(&a.relevance_score)
            .then_with(|| b.matched_keywords.len().cmp(&a.matched_keywords.len()))
            .then_with(|| a.title.cmp(&b.title))
    });

    ranked
}

fn score_keywords(
    keywords: &[KeywordCriterion],
    title_lower: &str,
    summary_lower: &str,
) -> (Vec<MatchedKeyword>, f64) {
    let mut matched = Vec::new();
    let mut total = 0.0;

    for kw in keywords {
        let needle = kw.word.to_lowercase();
        let in_title = title_lower.contains(&needle);
        let in_body = summary_lower.contains(&needle);
        if in_title || in_body {
            let mut contribution = 0.0;
            if in_title {
                contribution += kw.weight * 2.0;
            }
            if in_body {
                contribution += kw.weight;
            }
            total += contribution;
            matched.push(MatchedKeyword {
                word: kw.word.clone(),
                weight: kw.weight,
                in_title,
                in_body,
                contribution,
            });
        }
    }

    (matched, total)
}

/// Детермінований сканер ідентифікаторів `CVE-YYYY-NNNN` (4–7 цифр у другій частині)
/// без використання регулярних виразів.
pub fn extract_cves(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for token in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
        let trimmed = token.trim_matches('-');
        if let Some(cve) = parse_cve_token(trimmed)
            && seen.insert(cve.clone())
        {
            out.push(cve);
        }
    }

    out
}

fn parse_cve_token(token: &str) -> Option<String> {
    let mut parts = token.split('-');
    let prefix = parts.next()?;
    let year = parts.next()?;
    let number = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    if !prefix.eq_ignore_ascii_case("cve") {
        return None;
    }
    if year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !(4..=7).contains(&number.len()) || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    Some(format!("CVE-{year}-{number}"))
}

fn article_id(feed_id: &str, link: &str, title: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(feed_id.as_bytes());
    hasher.update(b"|");
    hasher.update(link.as_bytes());
    hasher.update(b"|");
    hasher.update(title.as_bytes());
    hex::encode(&hasher.finalize()[..8])
}

fn html_to_plain_text(input: &str) -> String {
    let rendered = html2text::from_read(input.as_bytes(), 1000)
        .unwrap_or_else(|_| html_escape::decode_html_entities(input).into_owned());
    rendered.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn is_canonical_cve(s: &str) -> bool {
        let parts: Vec<&str> = s.split('-').collect();
        parts.len() == 3
            && parts[0] == "CVE"
            && parts[1].len() == 4
            && parts[1].bytes().all(|b| b.is_ascii_digit())
            && (4..=7).contains(&parts[2].len())
            && parts[2].bytes().all(|b| b.is_ascii_digit())
    }

    proptest! {
        #[test]
        fn extract_cves_always_outputs_unique_canonical_ids(text in ".*") {
            let cves = extract_cves(&text);
            let mut seen = HashSet::new();
            for cve in &cves {
                prop_assert!(is_canonical_cve(cve));
                prop_assert!(seen.insert(cve.as_str()));
            }
        }

        #[test]
        fn extract_cves_finds_embedded_valid_ids_regardless_of_case_and_deduplicates(
            year in 1999_u32..=2099,
            seq_digits in "[0-9]{4,7}",
            prefix_case in prop_oneof![Just("CVE"), Just("cve"), Just("CvE"), Just("cVe")],
            left_sep in "[ ,.;:()\\[\\]\\n]{1,4}",
            right_sep in "[ ,.;:()\\[\\]\\n]{1,4}",
        ) {
            let raw_cve = format!("{prefix_case}-{year}-{seq_digits}");
            let expected = format!("CVE-{year}-{seq_digits}");
            let text = format!("alert{left_sep}{raw_cve}{right_sep}repeat {raw_cve}");

            let found = extract_cves(&text);
            prop_assert_eq!(found, vec![expected]);
        }

        #[test]
        fn extract_cves_rejects_malformed_digit_lengths(
            bad_year in prop_oneof!["[0-9]{1,3}", "[0-9]{5,6}"],
            good_year in "[0-9]{4}",
            bad_seq in prop_oneof!["[0-9]{1,3}", "[0-9]{8,10}"],
            good_seq in "[0-9]{4,7}",
        ) {
            let text = format!("CVE-{bad_year}-{good_seq} CVE-{good_year}-{bad_seq}");
            prop_assert!(extract_cves(&text).is_empty());
        }

        #[test]
        fn filter_and_rank_sorts_strictly_and_weights_title_higher_than_body(
            weight in 0.5_f64..10.0_f64,
            extra_items in prop::collection::vec(
                (
                    "[A-Za-z0-9 ]{1,30}",
                    "[A-Za-z0-9 ]{0,40}",
                    any::<bool>(),
                    any::<bool>(),
                ),
                0..15,
            ),
        ) {
            let feed = NewsFeedConfig {
                id: "prop-feed".into(),
                name: "Prop Feed".into(),
                group: "cybersecurity".into(),
                url: "http://mock.local/rss".into(),
                keywords: vec![
                    KeywordCriterion::new("sentinel".into(), weight).unwrap(),
                    KeywordCriterion::new("rustlang".into(), 1.5).unwrap(),
                ],
            };

            let mut raw_items = vec![
                RawFeedItem {
                    title: "Only body match".into(),
                    link: "http://mock.local/body".into(),
                    published: None,
                    summary: "mentions sentinel once".into(),
                },
                RawFeedItem {
                    title: "Title sentinel match".into(),
                    link: "http://mock.local/title".into(),
                    published: None,
                    summary: "plain text".into(),
                },
                RawFeedItem {
                    title: "Both sentinel match".into(),
                    link: "http://mock.local/both".into(),
                    published: None,
                    summary: "and sentinel again".into(),
                },
                RawFeedItem {
                    title: "Both sentinel with CVE-2026-12345".into(),
                    link: "http://mock.local/both-cve".into(),
                    published: None,
                    summary: "and sentinel again".into(),
                },
            ];

            for (i, (t, s, has_kw, has_cve)) in extra_items.into_iter().enumerate() {
                let title = if has_kw { format!("{t} rustlang") } else { t };
                let summary = if has_cve { format!("{s} CVE-2026-9999") } else { s };
                raw_items.push(RawFeedItem {
                    title,
                    link: format!("http://mock.local/extra/{i}"),
                    published: None,
                    summary,
                });
            }

            let ranked = filter_and_rank_items(&feed, raw_items);

            // Усі відібрані статті мають хоча б одне ключове слово та додатній рейтинг
            for a in &ranked {
                prop_assert!(!a.matched_keywords.is_empty());
                prop_assert!(a.relevance_score > 0.0);
                prop_assert_eq!(a.id.len(), 16);
                prop_assert!(a.id.bytes().all(|b| b.is_ascii_hexdigit()));
            }

            // Порядок сортування відповідає інваріанту
            for pair in ranked.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                let ord = b
                    .relevance_score
                    .total_cmp(&a.relevance_score)
                    .then_with(|| b.matched_keywords.len().cmp(&a.matched_keywords.len()))
                    .then_with(|| a.title.cmp(&b.title));
                prop_assert!(ord != std::cmp::Ordering::Greater);
            }

            let score_of = |link: &str| {
                ranked
                    .iter()
                    .find(|a| a.link == link)
                    .map(|a| a.relevance_score)
                    .unwrap()
            };
            let s_body = score_of("http://mock.local/body");
            let s_title = score_of("http://mock.local/title");
            let s_both = score_of("http://mock.local/both");
            let s_both_cve = score_of("http://mock.local/both-cve");

            prop_assert!(s_title > s_body);
            prop_assert!(s_both > s_title);
            prop_assert!((s_both_cve - (s_both + 2.0)).abs() < 1e-6);
        }

        #[test]
        fn article_id_is_deterministic_16_char_hex(
            feed_id in ".{0,32}",
            link in ".{0,64}",
            title in ".{0,64}",
        ) {
            let id1 = article_id(&feed_id, &link, &title);
            let id2 = article_id(&feed_id, &link, &title);
            prop_assert_eq!(&id1, &id2);
            prop_assert_eq!(id1.len(), 16);
            prop_assert!(id1.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
        }
    }
}
