pub mod config;
pub mod error;
pub mod extractor;
pub mod feeds;
pub mod health;
pub mod http_monitor;
pub mod nbu;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use dss::{
    Fact,
    FactOrigin,
    FactValue,
    InferenceEngine,
    InferenceResult,
};
use hashbrown::{
    HashMap,
    HashSet,
};
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use time::OffsetDateTime;
use tokio::sync::{
    Notify,
    RwLock,
    Semaphore,
    broadcast,
};
use tokio::task::JoinSet;
use tracing::{
    info,
    warn,
};

use crate::config::{
    AppConfig,
    NewsFeedConfig,
};
use crate::error::{
    AgentError,
    AgentResult,
};
use crate::extractor::extract_facts;
use crate::feeds::{
    NewsArticle,
    fetch_and_rank_feed,
};
use crate::health::{
    ErrorKind,
    FailureTransition,
    SourceHealth,
};
use crate::http_monitor::{
    HttpSiteState,
    apply_probe,
    probe_site,
};
use crate::nbu::{
    CurrencyRate,
    fetch_nbu_rates,
};

const MAX_RECENT_LOGS: usize = 30;
const UPDATE_CHANNEL_CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentUpdateKind {
    Connected,
    PollStarted,
    PollCompleted,
    StateChanged,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentUpdateEvent {
    pub kind: AgentUpdateKind,
    pub poll_count: u64,
    pub poll_in_progress: bool,
    pub changed_count: usize,
    pub articles_count: usize,
    pub decisions_count: usize,
    pub last_poll_duration_ms: Option<u64>,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
}

impl AgentUpdateEvent {
    pub fn from_snapshot(kind: AgentUpdateKind, snapshot: &AgentSnapshot) -> Self {
        let changed_count = snapshot
            .sites
            .iter()
            .filter(|s| s.is_visible_in_changed_table())
            .count();
        Self {
            kind,
            poll_count: snapshot.poll_count,
            poll_in_progress: snapshot.poll_in_progress,
            changed_count,
            articles_count: snapshot.articles.len(),
            decisions_count: snapshot.inference.decisions.len(),
            last_poll_duration_ms: snapshot.last_poll_duration_ms,
            timestamp: OffsetDateTime::now_utc(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentLogEntry {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedMonitorState {
    pub id: String,
    pub name: String,
    pub group: String,
    pub url: String,
    pub matched_count: usize,
    #[serde(default)]
    pub health: SourceHealth,
}

impl FeedMonitorState {
    pub fn from_config(cfg: &NewsFeedConfig) -> Self {
        Self {
            id: cfg.id.clone(),
            name: cfg.name.clone(),
            group: cfg.group.clone(),
            url: cfg.url.clone(),
            matched_count: 0,
            health: SourceHealth::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentSnapshot {
    pub sites: Vec<HttpSiteState>,
    #[serde(default)]
    pub feeds: Vec<FeedMonitorState>,
    pub articles: Vec<NewsArticle>,
    pub currencies: Vec<CurrencyRate>,
    #[serde(default)]
    pub nbu_health: SourceHealth,
    pub agent_facts: BTreeMap<String, Fact>,
    pub manual_facts: BTreeMap<String, Fact>,
    pub inference: InferenceResult,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_poll_at: Option<OffsetDateTime>,
    pub poll_count: u64,
    pub last_poll_duration_ms: Option<u64>,
    #[serde(default)]
    pub poll_in_progress: bool,
    pub recent_logs: Vec<AgentLogEntry>,
}

impl AgentSnapshot {
    /// Об'єднана робоча пам'ять ІСППР: факти Web-агента + ручні факти користувача (з пріоритетом) + виведені факти.
    pub fn all_facts(&self) -> BTreeMap<String, Fact> {
        let mut combined = self.base_facts();
        for (k, v) in &self.inference.inferred_facts {
            combined.entry(k.clone()).or_insert_with(|| v.clone());
        }
        combined
    }

    pub fn base_facts(&self) -> BTreeMap<String, Fact> {
        let mut base = self.agent_facts.clone();
        for (k, v) in &self.manual_facts {
            base.insert(k.clone(), v.clone());
        }
        base
    }

    fn push_log(&mut self, message: impl Into<String>) {
        self.recent_logs.insert(
            0,
            AgentLogEntry {
                timestamp: OffsetDateTime::now_utc(),
                message: message.into(),
            },
        );
        self.recent_logs.truncate(MAX_RECENT_LOGS);
    }
}

#[derive(Clone)]
pub struct WebAgent {
    config: Arc<AppConfig>,
    client: Client,
    engine: Arc<InferenceEngine>,
    state: Arc<RwLock<AgentSnapshot>>,
    poll_trigger: Arc<Notify>,
    updates_tx: broadcast::Sender<AgentUpdateEvent>,
}

impl WebAgent {
    pub fn new(config: AppConfig, engine: InferenceEngine) -> AgentResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.server.request_timeout_secs))
            .user_agent("WebAgentDSS/0.1 (Academic Research Lab 3; Rust/reqwest)")
            .build()?;

        let mut snapshot = load_persisted_snapshot(&config.dss.state_path).unwrap_or_default();
        snapshot.poll_in_progress = false;
        reconcile_sites(&mut snapshot.sites, &config);
        reconcile_feeds(&mut snapshot.feeds, &config);

        if !snapshot.agent_facts.is_empty() || !snapshot.manual_facts.is_empty() {
            let base = snapshot.base_facts();
            snapshot.inference = engine.evaluate(&base);
        }

        let (updates_tx, _) = broadcast::channel(UPDATE_CHANNEL_CAPACITY);

        Ok(Self {
            config: Arc::new(config),
            client,
            engine: Arc::new(engine),
            state: Arc::new(RwLock::new(snapshot)),
            poll_trigger: Arc::new(Notify::new()),
            updates_tx,
        })
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn engine(&self) -> &InferenceEngine {
        &self.engine
    }

    pub async fn snapshot(&self) -> AgentSnapshot {
        self.state.read().await.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentUpdateEvent> {
        self.updates_tx.subscribe()
    }

    pub fn request_immediate_poll(&self) {
        self.poll_trigger.notify_one();
    }

    fn emit_update(&self, kind: AgentUpdateKind, snapshot: &AgentSnapshot) {
        let _ = self
            .updates_tx
            .send(AgentUpdateEvent::from_snapshot(kind, snapshot));
    }

    /// Фоновий неблокуючий цикл опитування (п. 6 методички).
    pub async fn run_background_loop(self) {
        let interval = Duration::from_secs(self.config.server.poll_interval_secs);

        self.poll_once().await;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    self.poll_once().await;
                }
                _ = self.poll_trigger.notified() => {
                    self.poll_once().await;
                }
            }
        }
    }

    /// Один відмовостійкий цикл збору даних Web-агента та оновлення БЗ ІСППР.
    /// Помилка окремого джерела переводить його в режим експоненційного backoff,
    /// а після серії помилок парсингу вимикає лише проблемне джерело без зупинки агента.
    pub async fn poll_once(&self) {
        let poll_start_utc = OffsetDateTime::now_utc();

        let (eligible_sites, eligible_feeds, nbu_eligible, previous_currencies, started_event) = {
            let mut guard = self.state.write().await;
            if guard.poll_in_progress {
                return;
            }
            guard.poll_in_progress = true;

            let eligible_sites: HashSet<String> = guard
                .sites
                .iter()
                .filter(|s| s.health.should_poll(poll_start_utc))
                .map(|s| s.id.clone())
                .collect();
            let eligible_feeds: HashSet<String> = guard
                .feeds
                .iter()
                .filter(|f| f.health.should_poll(poll_start_utc))
                .map(|f| f.id.clone())
                .collect();
            let nbu_eligible =
                self.config.nbu_metrics.enabled && guard.nbu_health.should_poll(poll_start_utc);

            let started_event =
                AgentUpdateEvent::from_snapshot(AgentUpdateKind::PollStarted, &guard);

            (
                eligible_sites,
                eligible_feeds,
                nbu_eligible,
                guard.currencies.clone(),
                started_event,
            )
        };

        let _ = self.updates_tx.send(started_event);

        let started = Instant::now();
        info!("Web-агент: початок циклу збору інформації");

        let semaphore = Arc::new(Semaphore::new(self.config.server.max_concurrent_requests));

        // 1. Опитування активних (не вимкнених і не в backoff) HTTP/HTTPS-сайтів
        let mut site_tasks = JoinSet::new();
        for site in &self.config.http_sites {
            if !eligible_sites.contains(&site.id) {
                continue;
            }
            let client = self.client.clone();
            let sem = semaphore.clone();
            let id = site.id.clone();
            let url = site.url.clone();
            site_tasks.spawn(async move {
                let _permit = sem.acquire_owned().await.ok();
                let probe = probe_site(&client, &url).await;
                (id, probe)
            });
        }

        // 2. Опитування активних (не вимкнених і не в backoff) стрічок новин
        let mut feed_tasks = JoinSet::new();
        for feed in &self.config.news_feeds {
            if !eligible_feeds.contains(&feed.id) {
                continue;
            }
            let client = self.client.clone();
            let sem = semaphore.clone();
            let feed_cfg = feed.clone();
            feed_tasks.spawn(async move {
                let _permit = sem.acquire_owned().await.ok();
                let res = fetch_and_rank_feed(&client, &feed_cfg).await;
                (feed_cfg.id, feed_cfg.name, res)
            });
        }

        // 3. Опитування API НБУ (якщо увімкнено, не вимкнено та не в backoff)
        let nbu_res = if nbu_eligible {
            Some(
                fetch_nbu_rates(&self.client, &self.config.nbu_metrics, &previous_currencies).await,
            )
        } else {
            None
        };

        let mut site_results = HashMap::new();
        while let Some(joined) = site_tasks.join_next().await {
            if let Ok((id, probe)) = joined {
                site_results.insert(id, probe);
            }
        }

        let mut updated_feed_articles: HashMap<String, Vec<NewsArticle>> = HashMap::new();
        let mut feed_outcomes: HashMap<String, (String, Result<usize, AgentError>)> =
            HashMap::new();
        while let Some(joined) = feed_tasks.join_next().await {
            if let Ok((feed_id, feed_name, res)) = joined {
                match res {
                    Ok(items) => {
                        let count = items.len();
                        updated_feed_articles.insert(feed_id.clone(), items);
                        feed_outcomes.insert(feed_id, (feed_name, Ok(count)));
                    }
                    Err(err) => {
                        warn!(feed = %feed_name, error = %err, "Помилка збору/парсингу стрічки новин");
                        feed_outcomes.insert(feed_id, (feed_name, Err(err)));
                    }
                }
            }
        }

        let duration_ms = started.elapsed().as_millis() as u64;
        let now = OffsetDateTime::now_utc();
        let policy = &self.config.server.retry_policy;

        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            let mut new_logs = Vec::new();

            // Оновлення стану HTTP-сайтів із backoff / авто-вимкненням
            for site_state in &mut guard.sites {
                if let Some(probe_res) = site_results.remove(&site_state.id)
                    && let Some(transition) = apply_probe(site_state, probe_res, policy)
                {
                    let err_text = site_state
                        .health
                        .last_error
                        .as_deref()
                        .unwrap_or("невідома помилка");
                    match transition {
                        FailureTransition::Disabled { attempts } => {
                            new_logs.push(format!(
                                "Моніторинг сайту `{}` вимкнено після {} невдалих спроб парсингу: {}",
                                site_state.name, attempts, err_text
                            ));
                        }
                        FailureTransition::Throttled {
                            attempt,
                            backoff_secs,
                        } => {
                            new_logs.push(format!(
                                "Сайт `{}`: спроба #{attempt} невдала (повтор через {backoff_secs} с): {err_text}",
                                site_state.name
                            ));
                        }
                    }
                }
            }

            // Оновлення стану стрічок новин із backoff та вимкненням після серії помилок парсингу
            let mut feeds_to_replace = HashSet::new();
            for feed_state in &mut guard.feeds {
                if let Some((feed_name, outcome)) = feed_outcomes.remove(&feed_state.id) {
                    match outcome {
                        Ok(count) => {
                            feed_state.health.record_success(now);
                            feed_state.matched_count = count;
                            feeds_to_replace.insert(feed_state.id.clone());
                        }
                        Err(err) => {
                            let kind = classify_error(&err);
                            let transition = feed_state.health.record_failure(
                                now,
                                kind,
                                err.to_string(),
                                policy,
                            );
                            match transition {
                                FailureTransition::Disabled { attempts } => {
                                    feed_state.matched_count = 0;
                                    feeds_to_replace.insert(feed_state.id.clone());
                                    new_logs.push(format!(
                                        "Моніторинг стрічки `{feed_name}` вимкнено після {attempts} невдалих спроб парсингу: {err}"
                                    ));
                                }
                                FailureTransition::Throttled {
                                    attempt,
                                    backoff_secs,
                                } => {
                                    new_logs.push(format!(
                                        "Стрічка `{feed_name}` ({}): спроба #{attempt} невдала, повтор через {backoff_secs} с — {err}",
                                        kind.label_uk()
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            guard
                .articles
                .retain(|a| !feeds_to_replace.contains(&a.feed_id));
            for (_, mut items) in updated_feed_articles {
                guard.articles.append(&mut items);
            }
            guard.articles.sort_by(|a, b| {
                b.relevance_score
                    .total_cmp(&a.relevance_score)
                    .then_with(|| b.matched_keywords.len().cmp(&a.matched_keywords.len()))
                    .then_with(|| a.title.cmp(&b.title))
            });

            // Оновлення стану API НБУ із backoff / авто-вимкненням
            if let Some(res) = nbu_res {
                match res {
                    Ok(rates) => {
                        guard.nbu_health.record_success(now);
                        guard.currencies = rates;
                    }
                    Err(err) => {
                        warn!(error = %err, "Помилка отримання/парсингу курсів НБУ");
                        let kind = classify_error(&err);
                        let transition =
                            guard
                                .nbu_health
                                .record_failure(now, kind, err.to_string(), policy);
                        match transition {
                            FailureTransition::Disabled { attempts } => {
                                new_logs.push(format!(
                                    "Моніторинг API НБУ вимкнено після {attempts} невдалих спроб парсингу JSON: {err}"
                                ));
                            }
                            FailureTransition::Throttled {
                                attempt,
                                backoff_secs,
                            } => {
                                new_logs.push(format!(
                                    "API НБУ ({}): спроба #{attempt} невдала, повтор через {backoff_secs} с — {err}",
                                    kind.label_uk()
                                ));
                            }
                        }
                    }
                }
            }

            for msg in new_logs {
                guard.push_log(msg);
            }

            self.recompute_dss_locked(&mut guard);

            guard.last_poll_at = Some(now);
            guard.poll_count = guard.poll_count.saturating_add(1);
            guard.last_poll_duration_ms = Some(duration_ms);
            guard.poll_in_progress = false;

            let poll_num = guard.poll_count;
            let changed_count = guard
                .sites
                .iter()
                .filter(|s| s.is_visible_in_changed_table())
                .count();
            let articles_count = guard.articles.len();
            let decisions_count = guard.inference.decisions.len();
            let facts_count = guard.agent_facts.len();

            guard.push_log(format!(
                "Цикл #{poll_num} завершено за {duration_ms} мс: змінених сайтів — {changed_count}, новин — {articles_count}, фактів БЗ — {facts_count}, рішень ІСППР — {decisions_count}.",
            ));

            info!(
                poll = poll_num,
                duration_ms,
                changed_count,
                articles_count,
                facts_count,
                decisions_count,
                "Цикл Web-агента завершено"
            );

            guard.clone()
        };

        self.emit_update(AgentUpdateKind::PollCompleted, &snapshot_to_save);

        if let Err(err) = persist_snapshot(&self.config.dss.state_path, &snapshot_to_save) {
            warn!(error = %err, "Не вдалося зберегти стан агента на диск");
        }
    }

    pub async fn acknowledge_site(&self, site_id: &str) -> bool {
        let mut updated = false;
        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            if let Some(site) = guard.sites.iter_mut().find(|s| s.id == site_id) {
                site.acknowledged = true;
                let site_name = site.name.clone();
                updated = true;

                self.recompute_dss_locked(&mut guard);
                guard.push_log(format!("Сайт `{site_name}` позначено як переглянутий."));
            }
            updated.then(|| guard.clone())
        };

        if let Some(snap) = snapshot_to_save {
            self.emit_update(AgentUpdateKind::StateChanged, &snap);
            let _ = persist_snapshot(&self.config.dss.state_path, &snap);
        }
        updated
    }

    pub async fn unacknowledge_site(&self, site_id: &str) -> bool {
        let mut updated = false;
        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            if let Some(site) = guard.sites.iter_mut().find(|s| s.id == site_id) {
                site.acknowledged = false;
                site.changed = true;
                updated = true;

                self.recompute_dss_locked(&mut guard);
            }
            updated.then(|| guard.clone())
        };

        if let Some(snap) = snapshot_to_save {
            self.emit_update(AgentUpdateKind::StateChanged, &snap);
            let _ = persist_snapshot(&self.config.dss.state_path, &snap);
        }
        updated
    }

    /// Повторно увімкнути моніторинг HTTP-сайту та скинути backoff/лічильник помилок.
    pub async fn enable_site(&self, site_id: &str) -> bool {
        let mut updated = false;
        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            if let Some(site) = guard.sites.iter_mut().find(|s| s.id == site_id) {
                site.health.reset_manual();
                let site_name = site.name.clone();
                updated = true;

                self.recompute_dss_locked(&mut guard);
                guard.push_log(format!(
                    "Моніторинг сайту `{site_name}` повторно увімкнено."
                ));
            }
            updated.then(|| guard.clone())
        };

        if let Some(snap) = snapshot_to_save {
            self.emit_update(AgentUpdateKind::StateChanged, &snap);
            let _ = persist_snapshot(&self.config.dss.state_path, &snap);
        }
        updated
    }

    /// Повторно увімкнути моніторинг стрічки новин та скинути backoff/лічильник помилок.
    pub async fn enable_feed(&self, feed_id: &str) -> bool {
        let mut updated = false;
        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            if let Some(feed) = guard.feeds.iter_mut().find(|f| f.id == feed_id) {
                feed.health.reset_manual();
                let feed_name = feed.name.clone();
                updated = true;

                self.recompute_dss_locked(&mut guard);
                guard.push_log(format!(
                    "Моніторинг стрічки `{feed_name}` повторно увімкнено."
                ));
            }
            updated.then(|| guard.clone())
        };

        if let Some(snap) = snapshot_to_save {
            self.emit_update(AgentUpdateKind::StateChanged, &snap);
            let _ = persist_snapshot(&self.config.dss.state_path, &snap);
        }
        updated
    }

    /// Повторно увімкнути моніторинг API НБУ та скинути backoff/лічильник помилок.
    pub async fn enable_nbu(&self) {
        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            guard.nbu_health.reset_manual();
            self.recompute_dss_locked(&mut guard);
            guard.push_log("Моніторинг API НБУ повторно увімкнено.");
            guard.clone()
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        let _ = persist_snapshot(&self.config.dss.state_path, &snapshot_to_save);
    }

    pub async fn upsert_manual_fact(&self, key: String, value: FactValue, cf: f64) {
        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            let fact = Fact::new(
                key.clone(),
                value.clone(),
                "Ручне введення користувача",
                cf,
                FactOrigin::Manual,
            );
            guard.manual_facts.insert(key.clone(), fact);

            let base_facts = guard.base_facts();
            guard.inference = self.engine.evaluate(&base_facts);
            guard.push_log(format!(
                "Користувач додав факт `{key} = {value}` (CF={:.2}); ІСППР перерахувала вивід.",
                cf.clamp(0.0, 1.0)
            ));
            guard.clone()
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        let _ = persist_snapshot(&self.config.dss.state_path, &snapshot_to_save);
    }

    pub async fn clear_manual_facts(&self) {
        let snapshot_to_save = {
            let mut guard = self.state.write().await;
            guard.manual_facts.clear();
            let base_facts = guard.base_facts();
            guard.inference = self.engine.evaluate(&base_facts);
            guard
                .push_log("Ручні факти очищено; відновлено вивід лише на основі даних Web-агента.");
            guard.clone()
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        let _ = persist_snapshot(&self.config.dss.state_path, &snapshot_to_save);
    }

    fn recompute_dss_locked(&self, guard: &mut AgentSnapshot) {
        guard.agent_facts = extract_facts(
            &guard.sites,
            &guard.feeds,
            &guard.articles,
            &guard.currencies,
            &guard.nbu_health,
        );
        let base_facts = guard.base_facts();
        guard.inference = self.engine.evaluate(&base_facts);
    }
}

fn classify_error(err: &AgentError) -> ErrorKind {
    match err {
        AgentError::FeedParse(_) | AgentError::Json(_) | AgentError::TomlDe(_) => ErrorKind::Parse,
        AgentError::Http(req_err) if req_err.is_decode() || req_err.is_builder() => {
            ErrorKind::Parse
        }
        _ => ErrorKind::Transient,
    }
}

fn reconcile_sites(existing: &mut Vec<HttpSiteState>, config: &AppConfig) {
    let mut prev_by_id: HashMap<String, HttpSiteState> =
        existing.drain(..).map(|s| (s.id.clone(), s)).collect();

    for cfg in &config.http_sites {
        if let Some(mut prev) = prev_by_id.remove(&cfg.id) {
            prev.name = cfg.name.clone();
            prev.url = cfg.url.clone();
            prev.category = cfg.category.clone();
            existing.push(prev);
        } else {
            existing.push(HttpSiteState::from_config(cfg));
        }
    }
}

fn reconcile_feeds(existing: &mut Vec<FeedMonitorState>, config: &AppConfig) {
    let mut prev_by_id: HashMap<String, FeedMonitorState> =
        existing.drain(..).map(|f| (f.id.clone(), f)).collect();

    for cfg in &config.news_feeds {
        if let Some(mut prev) = prev_by_id.remove(&cfg.id) {
            prev.name = cfg.name.clone();
            prev.group = cfg.group.clone();
            prev.url = cfg.url.clone();
            existing.push(prev);
        } else {
            existing.push(FeedMonitorState::from_config(cfg));
        }
    }
}

fn load_persisted_snapshot(path: &Path) -> Option<AgentSnapshot> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn persist_snapshot(path: &Path, snapshot: &AgentSnapshot) -> AgentResult<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(snapshot)?;
    std::fs::write(path, json)?;
    Ok(())
}
