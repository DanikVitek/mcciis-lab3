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
