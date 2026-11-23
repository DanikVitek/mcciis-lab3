use askama::Template;
use axum::http::StatusCode;
use axum::response::{
    Html,
    IntoResponse,
    Response,
};
use dss::{
    Decision,
    FactOrigin,
    Rule,
};
use hashbrown::HashSet;
use time::OffsetDateTime;
use time::macros::format_description;
use web_agent::AgentSnapshot;
use web_agent::config::AppConfig;
use web_agent::feeds::NewsArticle;
use web_agent::health::SourceHealth;

pub struct HtmlTemplate<T>(pub T);

impl<T: Template> IntoResponse for HtmlTemplate<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Помилка рендерингу шаблону: {err}"),
            )
                .into_response(),
        }
    }
}

fn format_dt(dt: Option<OffsetDateTime>) -> String {
    let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]:[second] UTC");
    match dt {
        Some(t) => t.format(&fmt).unwrap_or_else(|_| t.to_string()),
        None => "—".to_string(),
    }
}

pub struct HealthAlertView {
    pub kind_label: &'static str,
    pub name: String,
    pub status_text: String,
    pub error_kind: &'static str,
    pub error_message: String,
    pub enable_action: String,
}

pub struct ChangedSiteView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub category: String,
    pub status_code: String,
    pub size: String,
    pub delta: String,
    pub changed_at: String,
}

pub struct AllSiteView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub status_badge_class: &'static str,
    pub status_badge_text: &'static str,
    pub size: String,
    pub last_checked: String,
    pub disabled: bool,
    pub acknowledged: bool,
}

pub struct KeywordBadgeView {
    pub word: String,
    pub tooltip: String,
}

pub struct ArticleRowView {
    pub id: String,
    pub score: String,
    pub group: String,
    pub title: String,
    pub published: String,
    pub keywords: Vec<KeywordBadgeView>,
    pub cves: Vec<String>,
    pub feed_name: String,
}

pub struct CurrencyRowView {
    pub cc: String,
    pub txt: String,
    pub rate: String,
    pub delta: String,
    pub exchangedate: String,
}

pub struct DecisionSummaryView {
    pub rule_id: String,
    pub rule_name: String,
    pub severity_class: &'static str,
    pub severity_label: &'static str,
    pub cf: String,
    pub recommendation: String,
}

pub struct LogRowView {
    pub timestamp: String,
    pub message: String,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub active_nav: &'static str,
    pub poll_count: u64,
    pub last_poll_at: String,
    pub changed_sites: Vec<ChangedSiteView>,
    pub all_sites: Vec<AllSiteView>,
    pub articles: Vec<ArticleRowView>,
    pub total_facts: usize,
    pub total_decisions: usize,
    pub operational_mode: String,
    pub disabled_count: usize,
    pub throttled_count: usize,
    pub health_alerts: Vec<HealthAlertView>,
    pub currencies: Vec<CurrencyRowView>,
    pub top_decisions: Vec<DecisionSummaryView>,
    pub recent_logs: Vec<LogRowView>,
}

impl IndexTemplate {
    pub fn from_snapshot(snapshot: &AgentSnapshot) -> Self {
        let now = OffsetDateTime::now_utc();

        let disabled_count = snapshot.sites.iter().filter(|s| s.health.disabled).count()
            + snapshot.feeds.iter().filter(|f| f.health.disabled).count()
            + usize::from(snapshot.nbu_health.disabled);

        let throttled_count = snapshot
            .sites
            .iter()
            .filter(|s| s.health.is_throttled(now))
            .count()
            + snapshot
                .feeds
                .iter()
                .filter(|f| f.health.is_throttled(now))
                .count()
            + usize::from(snapshot.nbu_health.is_throttled(now));

        let mut health_alerts = Vec::new();
        for site in &snapshot.sites {
            if site.health.disabled || site.health.last_error.is_some() {
                health_alerts.push(build_health_alert(
                    "HTTP-сайт",
                    &site.name,
                    &site.health,
                    format!("/sites/{}/enable", site.id),
                    now,
                ));
            }
        }
        for feed in &snapshot.feeds {
            if feed.health.disabled || feed.health.last_error.is_some() {
                health_alerts.push(build_health_alert(
                    "Стрічка новин",
                    &feed.name,
                    &feed.health,
                    format!("/feeds/{}/enable", feed.id),
                    now,
                ));
            }
        }
        if snapshot.nbu_health.disabled || snapshot.nbu_health.last_error.is_some() {
            health_alerts.push(build_health_alert(
                "API НБУ",
                "Офіційні курси валют",
                &snapshot.nbu_health,
                "/nbu/enable".to_string(),
                now,
            ));
        }

        let changed_sites = snapshot
            .sites
            .iter()
            .filter(|s| s.is_visible_in_changed_table())
            .map(|site| ChangedSiteView {
                id: site.id.clone(),
                name: site.name.clone(),
                url: site.url.clone(),
                category: site.category.clone(),
                status_code: site
                    .status_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                size: site
                    .body_size
                    .map(|b| format!("{b} B"))
                    .unwrap_or_else(|| "—".to_string()),
                delta: match site.size_delta() {
                    Some(d) if d > 0 => format!("+{d} B"),
                    Some(d) => format!("{d} B"),
                    None => "перша фіксація".to_string(),
                },
                changed_at: format_dt(site.last_changed),
            })
            .collect();

        let all_sites = snapshot
            .sites
            .iter()
            .map(|site| {
                let (status_badge_class, status_badge_text) = if site.health.disabled {
                    ("sev-critical", "Вимкнено (помилка парсингу)")
                } else if site.health.is_throttled(now) {
                    ("sev-high", "Backoff (повторна спроба)")
                } else if site.is_visible_in_changed_table() {
                    ("sev-medium", "Є зміни")
                } else if site.acknowledged {
                    ("badge-ok", "Переглянуто")
                } else {
                    ("badge-group", "Очікує перевірки")
                };

                AllSiteView {
                    id: site.id.clone(),
                    name: site.name.clone(),
                    url: site.url.clone(),
                    status_badge_class,
                    status_badge_text,
                    size: site
                        .body_size
                        .map(|b| format!("{b} B"))
                        .unwrap_or_else(|| "—".to_string()),
                    last_checked: format_dt(site.health.last_checked),
                    disabled: site.health.disabled,
                    acknowledged: site.acknowledged,
                }
            })
            .collect();

        let articles = snapshot
            .articles
            .iter()
            .map(|a| ArticleRowView {
                id: a.id.clone(),
                score: format!("{:.2}", a.relevance_score),
                group: a.group.clone(),
                title: a.title.clone(),
                published: a.published.clone().unwrap_or_default(),
                keywords: a
                    .matched_keywords
                    .iter()
                    .map(|mk| {
                        let loc = match (mk.in_title, mk.in_body) {
                            (true, true) => "заголовок+текст",
                            (true, false) => "заголовок",
                            (false, true) => "текст",
                            (false, false) => "",
                        };
                        KeywordBadgeView {
                            word: mk.word.clone(),
                            tooltip: format!(
                                "Вага: {}, внесок: {:.1} ({loc})",
                                mk.weight, mk.contribution
                            ),
                        }
                    })
                    .collect(),
                cves: a.cves.clone(),
                feed_name: a.feed_name.clone(),
            })
            .collect();

        let currencies = snapshot
            .currencies
            .iter()
            .map(|c| CurrencyRowView {
                cc: c.cc.clone(),
                txt: c.txt.clone(),
                rate: format!("{:.4}", c.rate),
                delta: match c.delta_pct {
                    Some(d) if d > 0.0 => format!("+{d:.4}%"),
                    Some(d) => format!("{d:.4}%"),
                    None => "—".to_string(),
                },
                exchangedate: c.exchangedate.clone(),
            })
            .collect();

        let top_decisions = snapshot
            .inference
            .decisions
            .iter()
            .take(4)
            .map(|dec| DecisionSummaryView {
                rule_id: dec.rule_id.clone(),
                rule_name: dec.rule_name.clone(),
                severity_class: dec.severity.css_class(),
                severity_label: dec.severity.label_uk(),
                cf: format!("{:.2}", dec.cf),
                recommendation: dec.recommendation.clone(),
            })
            .collect();

        let recent_logs = snapshot
            .recent_logs
            .iter()
            .take(10)
            .map(|entry| LogRowView {
                timestamp: format_dt(Some(entry.timestamp)),
                message: entry.message.clone(),
            })
            .collect();

        let operational_mode = snapshot
            .inference
            .inferred_facts
            .get("operational_mode")
            .map(|f| f.value.as_text())
            .unwrap_or_else(|| "аналіз...".to_string());

        Self {
            active_nav: "index",
            poll_count: snapshot.poll_count,
            last_poll_at: format_dt(snapshot.last_poll_at),
            changed_sites,
            all_sites,
            articles,
            total_facts: snapshot.all_facts().len(),
            total_decisions: snapshot.inference.decisions.len(),
            operational_mode,
            disabled_count,
            throttled_count,
            health_alerts,
            currencies,
            top_decisions,
            recent_logs,
        }
    }
}

fn build_health_alert(
    kind_label: &'static str,
    name: &str,
    health: &SourceHealth,
    enable_action: String,
    now: OffsetDateTime,
) -> HealthAlertView {
    let status_text = if health.disabled {
        format!("ВИМКНЕНО після {} спроб", health.consecutive_failures)
    } else if health.is_throttled(now) {
        format!(
            "Backoff (спроба #{}, наступна: {})",
            health.consecutive_failures,
            format_dt(health.next_retry_at)
        )
    } else {
        format!("Остання спроба невдала (#{})", health.consecutive_failures)
    };

    HealthAlertView {
        kind_label,
        name: name.to_string(),
        status_text,
        error_kind: health
            .last_error_kind
            .map(|k| k.label_uk())
            .unwrap_or("Помилка"),
        error_message: health
            .last_error
            .clone()
            .unwrap_or_else(|| "деталі відсутні".to_string()),
        enable_action,
    }
}

pub struct KeywordDetailView {
    pub word: String,
    pub weight: String,
    pub in_title: &'static str,
    pub in_body: &'static str,
    pub contribution: String,
}

#[derive(Template)]
#[template(path = "article.html")]
pub struct ArticleTemplate {
    pub active_nav: &'static str,
    pub title: String,
    pub group: String,
    pub feed_name: String,
    pub published: String,
    pub score: String,
    pub cves: Vec<String>,
    pub summary: String,
    pub link: String,
    pub keywords: Vec<KeywordDetailView>,
}

impl ArticleTemplate {
    pub fn from_article(article: &NewsArticle) -> Self {
        Self {
            active_nav: "index",
            title: article.title.clone(),
            group: article.group.clone(),
            feed_name: article.feed_name.clone(),
            published: article.published.clone().unwrap_or_else(|| "—".to_string()),
            score: format!("{:.2}", article.relevance_score),
            cves: article.cves.clone(),
            summary: article.summary.clone(),
            link: article.link.clone(),
            keywords: article
                .matched_keywords
                .iter()
                .map(|mk| KeywordDetailView {
                    word: mk.word.clone(),
                    weight: format!("{:.2}", mk.weight),
                    in_title: if mk.in_title { "Так (×2)" } else { "Ні" },
                    in_body: if mk.in_body { "Так (×1)" } else { "Ні" },
                    contribution: format!("{:.2}", mk.contribution),
                })
                .collect(),
        }
    }
}

pub struct ConfSiteView {
    pub id: String,
    pub name: String,
    pub category: String,
    pub url: String,
}

pub struct ConfFeedView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub group: String,
    pub keywords_summary: String,
}

#[derive(Template)]
#[template(path = "conf.html")]
pub struct ConfTemplate {
    pub active_nav: &'static str,
    pub bind_addr: String,
    pub poll_interval_secs: u64,
    pub request_timeout_secs: u64,
    pub max_concurrent_requests: usize,
    pub parse_max_retries: u32,
    pub initial_backoff_secs: u64,
    pub max_backoff_secs: u64,
    pub log_dir: String,
    pub log_prefix: String,
    pub log_rotation: &'static str,
    pub log_max_files: usize,
    pub rules_path: String,
    pub state_path: String,
    pub nbu_status: &'static str,
    pub nbu_url: String,
    pub nbu_currencies: String,
    pub sites: Vec<ConfSiteView>,
    pub feeds: Vec<ConfFeedView>,
    pub pretty_toml: String,
}

impl ConfTemplate {
    pub fn from_config(config: &AppConfig) -> Self {
        let pretty_toml = toml::to_string_pretty(config)
            .unwrap_or_else(|e| format!("# Помилка серіалізації TOML: {e}"));

        Self {
            active_nav: "conf",
            bind_addr: config.server.bind_addr.clone(),
            poll_interval_secs: config.server.poll_interval_secs,
            request_timeout_secs: config.server.request_timeout_secs,
            max_concurrent_requests: config.server.max_concurrent_requests,
            parse_max_retries: config.server.retry_policy.parse_max_retries,
            initial_backoff_secs: config.server.retry_policy.initial_backoff_secs,
            max_backoff_secs: config.server.retry_policy.max_backoff_secs,
            log_dir: config.logging.dir.display().to_string(),
            log_prefix: config.logging.file_prefix.clone(),
            log_rotation: config.logging.rotation.as_str(),
            log_max_files: config.logging.max_files,
            rules_path: config.dss.rules_path.display().to_string(),
            state_path: config.dss.state_path.display().to_string(),
            nbu_status: if config.nbu_metrics.enabled {
                "Увімкнено"
            } else {
                "Вимкнено"
            },
            nbu_url: config.nbu_metrics.url.clone(),
            nbu_currencies: config.nbu_metrics.currencies.join(", "),
            sites: config
                .http_sites
                .iter()
                .map(|s| ConfSiteView {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    category: s.category.clone(),
                    url: s.url.clone(),
                })
                .collect(),
            feeds: config
                .news_feeds
                .iter()
                .map(|f| ConfFeedView {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    url: f.url.clone(),
                    group: f.group.clone(),
                    keywords_summary: f
                        .keywords
                        .iter()
                        .map(|k| format!("{} ({:.1})", k.word, k.weight))
                        .collect::<Vec<_>>()
                        .join(", "),
                })
                .collect(),
            pretty_toml,
        }
    }
}

pub struct TraceRowView {
    pub fact_key: String,
    pub actual: String,
    pub op: &'static str,
    pub expected: String,
    pub source: String,
    pub cf: String,
}

pub struct DecisionDetailView {
    pub rule_id: String,
    pub rule_name: String,
    pub priority: i32,
    pub severity_class: &'static str,
    pub severity_label: &'static str,
    pub cf: String,
    pub recommendation: String,
    pub derived_fact: String,
    pub explanation: Vec<TraceRowView>,
}

pub struct FactRowView {
    pub key: String,
    pub value: String,
    pub origin_class: &'static str,
    pub origin_label: &'static str,
    pub source: String,
    pub cf: String,
    pub updated_at: String,
}

pub struct RuleRowView {
    pub id: String,
    pub active: bool,
    pub name: String,
    pub description: String,
    pub conditions_text: String,
    pub derived_text: String,
    pub cf: String,
}

#[derive(Template)]
#[template(path = "dss.html")]
pub struct DssTemplate {
    pub active_nav: &'static str,
    pub iterations: usize,
    pub decisions: Vec<DecisionDetailView>,
    pub manual_facts_count: usize,
    pub facts: Vec<FactRowView>,
    pub rules: Vec<RuleRowView>,
}

impl DssTemplate {
    pub fn from_snapshot(snapshot: &AgentSnapshot, rules: &[Rule]) -> Self {
        let all_facts = snapshot.all_facts();
        let fired_set: HashSet<&str> = snapshot
            .inference
            .fired_rule_ids
            .iter()
            .map(String::as_str)
            .collect();

        let decisions = snapshot
            .inference
            .decisions
            .iter()
            .map(build_decision_view)
            .collect();

        let facts = all_facts
            .values()
            .map(|f| FactRowView {
                key: f.key.clone(),
                value: f.value.as_text(),
                origin_class: match f.origin {
                    FactOrigin::WebAgent => "sev-info",
                    FactOrigin::Inferred => "badge-ok",
                    FactOrigin::Manual => "sev-medium",
                },
                origin_label: f.origin.label_uk(),
                source: f.source.clone(),
                cf: format!("{:.2}", f.cf),
                updated_at: format_dt(Some(f.updated_at)),
            })
            .collect();

        let rule_views = rules
            .iter()
            .map(|r| RuleRowView {
                id: r.id.clone(),
                active: fired_set.contains(r.id.as_str()),
                name: r.name.clone(),
                description: r.description.clone(),
                conditions_text: r
                    .conditions
                    .iter()
                    .map(|c| format!("{} {} {}", c.fact, c.op.symbol(), c.value.as_text()))
                    .collect::<Vec<_>>()
                    .join(" І "),
                derived_text: r
                    .derived_fact
                    .as_ref()
                    .map(|d| format!("{} = {}", d.key, d.value.as_text()))
                    .unwrap_or_else(|| "—".to_string()),
                cf: format!("{:.2}", r.cf),
            })
            .collect();

        Self {
            active_nav: "dss",
            iterations: snapshot.inference.iterations,
            decisions,
            manual_facts_count: snapshot.manual_facts.len(),
            facts,
            rules: rule_views,
        }
    }
}

fn build_decision_view(dec: &Decision) -> DecisionDetailView {
    DecisionDetailView {
        rule_id: dec.rule_id.clone(),
        rule_name: dec.rule_name.clone(),
        priority: dec.priority,
        severity_class: dec.severity.css_class(),
        severity_label: dec.severity.label_uk(),
        cf: format!("{:.2}", dec.cf),
        recommendation: dec.recommendation.clone(),
        derived_fact: dec
            .derived_fact
            .as_ref()
            .map(|d| format!("{} = {}", d.key, d.value.as_text()))
            .unwrap_or_default(),
        explanation: dec
            .explanation
            .iter()
            .map(|tr| TraceRowView {
                fact_key: tr.fact_key.clone(),
                actual: tr.actual.as_text(),
                op: tr.operator.symbol(),
                expected: tr.expected.as_text(),
                source: tr.fact_source.clone(),
                cf: format!("{:.2}", tr.fact_cf),
            })
            .collect(),
    }
}

#[derive(Template)]
#[template(path = "not_found.html")]
pub struct NotFoundTemplate {
    pub active_nav: &'static str,
    pub message: String,
}
