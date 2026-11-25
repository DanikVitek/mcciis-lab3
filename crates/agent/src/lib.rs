//! Автономний інтелектуальний Web-агент для моніторингу HTTP/HTTPS-сайтів,
//! ранжування новинних стрічок RSS/Atom, збору метрик НБУ та автоматичного
//! поповнення бази знань ІСППР.

pub mod config;
pub mod error;
pub mod extractor;
pub mod feeds;
pub mod health;
pub mod http_monitor;
pub mod nbu;

use core::time::Duration;
use std::collections::{
    BTreeMap,
    VecDeque,
};
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

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
use parking_lot::RwLock;
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use time::OffsetDateTime;
use tokio::sync::{
    Mutex,
    Notify,
    Semaphore,
    broadcast,
};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{
    error,
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

/// Тип push-події Web-агента, що транслюється клієнтам через WebSocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentUpdateKind {
    Connected,
    PollStarted,
    PollCompleted,
    StateChanged,
}

/// Компактна подія життєвого циклу Web-агента для push-оновлень інтерфейсу.
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
    /// Формує подію оновлення на основі поточного знімка стану агента.
    #[must_use]
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

/// Запис журналу подій Web-агента для відображення в інтерфейсі.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentLogEntry {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub message: String,
}

/// Стан моніторингу окремої стрічки новин.
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
    /// Ініціалізує початковий стан стрічки новин із конфігурації.
    #[must_use]
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

/// Повний знімок стану автономного Web-агента та результатів виводу ІСППР.
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
    pub recent_logs: VecDeque<AgentLogEntry>,
}

impl AgentSnapshot {
    /// Об'єднана робоча пам'ять ІСППР: факти Web-агента + ручні факти користувача (з пріоритетом) + виведені факти.
    #[must_use]
    pub fn all_facts(&self) -> BTreeMap<String, Fact> {
        let mut combined = self.base_facts();
        for (k, v) in &self.inference.inferred_facts {
            combined.entry(k.clone()).or_insert_with(|| v.clone());
        }
        combined
    }

    /// Базова робоча пам'ять: факти Web-агента з пріоритетним перекриттям ручними фактами.
    #[must_use]
    pub fn base_facts(&self) -> BTreeMap<String, Fact> {
        let mut base = self.agent_facts.clone();
        for (k, v) in &self.manual_facts {
            base.insert(k.clone(), v.clone());
        }
        base
    }

    fn push_log(&mut self, message: impl Into<String>) {
        self.recent_logs.push_front(AgentLogEntry {
            timestamp: OffsetDateTime::now_utc(),
            message: message.into(),
        });
        self.recent_logs.truncate(MAX_RECENT_LOGS);
    }
}

/// RAII-запобіжник, що гарантує скидання `poll_in_progress` навіть при скасуванні або паніці.
struct PollProgressGuard {
    state: Arc<RwLock<Arc<AgentSnapshot>>>,
    active: bool,
}

impl PollProgressGuard {
    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for PollProgressGuard {
    fn drop(&mut self) {
        if self.active {
            let mut guard = self.state.write();
            let snap = Arc::make_mut(&mut *guard);
            snap.poll_in_progress = false;
        }
    }
}

/// Автономний Web-агент із фоновим опитуванням, персистентністю стану та push-подіями.
#[derive(Clone)]
pub struct WebAgent {
    config: Arc<AppConfig>,
    client: Client,
    engine: Arc<InferenceEngine>,
    state: Arc<RwLock<Arc<AgentSnapshot>>>,
    poll_lock: Arc<Mutex<()>>,
    persist_lock: Arc<Mutex<()>>,
    poll_trigger: Arc<Notify>,
    updates_tx: broadcast::Sender<AgentUpdateEvent>,
}

impl fmt::Debug for WebAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebAgent")
            .field("bind_addr", &self.config.server.bind_addr)
            .field("sites_count", &self.config.http_sites.len())
            .field("feeds_count", &self.config.news_feeds.len())
            .field("rules_count", &self.engine.rules().len())
            .finish_non_exhaustive()
    }
}

impl WebAgent {
    /// Ініціалізує Web-агент, HTTP-клієнт і відновлює збережений стан із диску (якщо він існує).
    ///
    /// # Errors
    ///
    /// Повертає [`AgentError::Http`], якщо не вдалося сконструювати `reqwest::Client`.
    pub fn new(config: AppConfig, engine: InferenceEngine) -> AgentResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.server.request_timeout_secs))
            .user_agent("WebAgentDSS/0.1 (Academic Research Lab 3; Rust/reqwest)")
            .build()?;

        let mut snapshot = load_persisted_snapshot(&config.dss.state_path);
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
            state: Arc::new(RwLock::new(Arc::new(snapshot))),
            poll_lock: Arc::new(Mutex::new(())),
            persist_lock: Arc::new(Mutex::new(())),
            poll_trigger: Arc::new(Notify::new()),
            updates_tx,
        })
    }

    /// Повертає розділене посилання на незмінну конфігурацію агента.
    #[must_use]
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Повертає `Arc` на незмінну конфігурацію агента без клонування даних.
    #[must_use]
    pub fn config_arc(&self) -> Arc<AppConfig> {
        Arc::clone(&self.config)
    }

    /// Повертає посилання на машину логічного виводу ІСППР.
    #[must_use]
    pub fn engine(&self) -> &InferenceEngine {
        &self.engine
    }

    /// Повертає поточний знімок стану за `O(1)` (лічильник посилань `Arc` без глибокого копіювання).
    #[must_use]
    pub fn snapshot(&self) -> Arc<AgentSnapshot> {
        Arc::clone(&self.state.read())
    }

    /// Знаходить одну статтю за ідентифікатором без клонування всього знімка.
    #[must_use]
    pub fn article_by_id(&self, id: &str) -> Option<NewsArticle> {
        self.state
            .read()
            .articles
            .iter()
            .find(|a| a.id == id)
            .cloned()
    }

    /// Підписується на потік push-подій Web-агента.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<AgentUpdateEvent> {
        self.updates_tx.subscribe()
    }

    /// Ініціює позачерговий цикл опитування у фоновому циклі.
    pub fn request_immediate_poll(&self) {
        self.poll_trigger.notify_one();
    }

    fn emit_update(&self, kind: AgentUpdateKind, snapshot: &AgentSnapshot) {
        let _ = self
            .updates_tx
            .send(AgentUpdateEvent::from_snapshot(kind, snapshot));
    }

    async fn persist_state_checked(&self, snapshot: Arc<AgentSnapshot>) {
        let _guard = self.persist_lock.lock().await;
        if let Err(err) = persist_snapshot(&self.config.dss.state_path, snapshot).await {
            warn!(
                path = %self.config.dss.state_path.display(),
                error = %err,
                "Не вдалося зберегти стан агента на диск"
            );
        }
    }

    /// Фоновий неблокуючий цикл опитування (п. 6 методички) із кооперативним завершенням.
    pub async fn run_background_loop(self, shutdown: CancellationToken) {
        let interval = Duration::from_secs(self.config.server.poll_interval_secs);

        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = self.poll_once() => {}
        }

        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => {
                    info!("Фоновий цикл Web-агента отримав сигнал завершення");
                    break;
                }
                () = tokio::time::sleep(interval) => {
                    self.poll_once().await;
                }
                () = self.poll_trigger.notified() => {
                    self.poll_once().await;
                }
            }
        }
    }

    /// Один відмовостійкий цикл збору даних Web-агента та оновлення БЗ ІСППР.
    /// Помилка окремого джерела переводить його в режим експоненційного backoff,
    /// а після серії помилок парсингу вимикає лише проблемне джерело без зупинки агента.
    #[tracing::instrument(skip(self))]
    pub async fn poll_once(&self) {
        let Ok(_poll_guard) = self.poll_lock.try_lock() else {
            return;
        };

        let poll_start_utc = OffsetDateTime::now_utc();

        let (eligible_sites, eligible_feeds, nbu_eligible, previous_currencies, started_event) = {
            let mut guard = self.state.write();
            let snap = Arc::make_mut(&mut *guard);
            if snap.poll_in_progress {
                return;
            }
            snap.poll_in_progress = true;

            let eligible_sites: HashSet<String> = snap
                .sites
                .iter()
                .filter(|s| s.health.should_poll(poll_start_utc))
                .map(|s| s.id.clone())
                .collect();
            let eligible_feeds: HashSet<String> = snap
                .feeds
                .iter()
                .filter(|f| f.health.should_poll(poll_start_utc))
                .map(|f| f.id.clone())
                .collect();
            let nbu_eligible =
                self.config.nbu_metrics.enabled && snap.nbu_health.should_poll(poll_start_utc);

            let started_event = AgentUpdateEvent::from_snapshot(AgentUpdateKind::PollStarted, snap);

            (
                eligible_sites,
                eligible_feeds,
                nbu_eligible,
                snap.currencies.clone(),
                started_event,
            )
        };

        let mut progress_guard = PollProgressGuard {
            state: Arc::clone(&self.state),
            active: true,
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
            let sem = Arc::clone(&semaphore);
            let id = site.id.clone();
            let url = site.url.clone();
            site_tasks.spawn(async move {
                let Ok(_permit) = sem.acquire_owned().await else {
                    return (
                        id,
                        Err(crate::http_monitor::SiteProbeError {
                            kind: ErrorKind::Transient,
                            message: "семафор обмеження запитів закрито".to_string(),
                        }),
                    );
                };
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
            let sem = Arc::clone(&semaphore);
            let feed_cfg = feed.clone();
            feed_tasks.spawn(async move {
                let Ok(_permit) = sem.acquire_owned().await else {
                    return (
                        feed_cfg.id,
                        feed_cfg.name,
                        Err(AgentError::Config(
                            "семафор обмеження запитів закрито".to_string(),
                        )),
                    );
                };
                let res = fetch_and_rank_feed(&client, &feed_cfg).await;
                (feed_cfg.id, feed_cfg.name, res)
            });
        }

        // 3. Опитування API НБУ з урахуванням глобального ліміту паралельних запитів
        let nbu_res = if nbu_eligible {
            let _permit = semaphore.acquire().await.ok();
            Some(
                fetch_nbu_rates(&self.client, &self.config.nbu_metrics, &previous_currencies).await,
            )
        } else {
            None
        };

        let mut site_results = HashMap::with_capacity(eligible_sites.len());
        while let Some(joined) = site_tasks.join_next().await {
            match joined {
                Ok((id, probe)) => {
                    site_results.insert(id, probe);
                }
                Err(join_err) => {
                    error!(error = %join_err, "Внутрішній збій завдання моніторингу HTTP-сайту");
                }
            }
        }

        let mut updated_feed_articles: HashMap<String, Vec<NewsArticle>> =
            HashMap::with_capacity(eligible_feeds.len());
        let mut feed_outcomes: HashMap<String, (String, Result<usize, AgentError>)> =
            HashMap::with_capacity(eligible_feeds.len());
        while let Some(joined) = feed_tasks.join_next().await {
            match joined {
                Ok((feed_id, feed_name, res)) => match res {
                    Ok(items) => {
                        let count = items.len();
                        updated_feed_articles.insert(feed_id.clone(), items);
                        feed_outcomes.insert(feed_id, (feed_name, Ok(count)));
                    }
                    Err(err) => {
                        warn!(feed = %feed_name, error = %err, "Помилка збору/парсингу стрічки новин");
                        feed_outcomes.insert(feed_id, (feed_name, Err(err)));
                    }
                },
                Err(join_err) => {
                    error!(error = %join_err, "Внутрішній збій завдання збору стрічки новин");
                }
            }
        }

        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let now = OffsetDateTime::now_utc();
        let policy = &self.config.server.retry_policy;

        let snapshot_to_save = {
            let mut guard = self.state.write();
            let snap = Arc::make_mut(&mut *guard);
            let mut new_logs = Vec::new();

            // Оновлення стану HTTP-сайтів із backoff / авто-вимкненням
            for site_state in &mut snap.sites {
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
            let mut feeds_to_replace = HashSet::with_capacity(feed_outcomes.len());
            for feed_state in &mut snap.feeds {
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

            snap.articles
                .retain(|a| !feeds_to_replace.contains(&a.feed_id));
            for (_, mut items) in updated_feed_articles {
                snap.articles.append(&mut items);
            }
            snap.articles.sort_by(NewsArticle::cmp_relevance);

            // Оновлення стану API НБУ із backoff / авто-вимкненням
            if let Some(res) = nbu_res {
                match res {
                    Ok(rates) => {
                        snap.nbu_health.record_success(now);
                        snap.currencies = rates;
                    }
                    Err(err) => {
                        warn!(error = %err, "Помилка отримання/парсингу курсів НБУ");
                        let kind = classify_error(&err);
                        let transition =
                            snap.nbu_health
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
                snap.push_log(msg);
            }

            self.recompute_dss_locked(snap);

            snap.last_poll_at = Some(now);
            snap.poll_count = snap.poll_count.saturating_add(1);
            snap.last_poll_duration_ms = Some(duration_ms);
            snap.poll_in_progress = false;
            progress_guard.disarm();

            let poll_num = snap.poll_count;
            let changed_count = snap
                .sites
                .iter()
                .filter(|s| s.is_visible_in_changed_table())
                .count();
            let articles_count = snap.articles.len();
            let decisions_count = snap.inference.decisions.len();
            let facts_count = snap.agent_facts.len();

            snap.push_log(format!(
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

            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::PollCompleted, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
    }

    /// Позначає змінений HTTP-сайт як переглянутий.
    pub async fn acknowledge_site(&self, site_id: &str) -> bool {
        let snapshot_to_save = {
            let mut guard = self.state.write();
            let found = guard.sites.iter().any(|s| s.id == site_id);
            if !found {
                return false;
            }
            let snap = Arc::make_mut(&mut *guard);
            if let Some(site) = snap.sites.iter_mut().find(|s| s.id == site_id) {
                site.acknowledged = true;
                let site_name = site.name.clone();
                self.recompute_dss_locked(snap);
                snap.push_log(format!("Сайт `{site_name}` позначено як переглянутий."));
            }
            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
        true
    }

    /// Повертає HTTP-сайт до списку непереглянутих змін.
    pub async fn unacknowledge_site(&self, site_id: &str) -> bool {
        let snapshot_to_save = {
            let mut guard = self.state.write();
            let found = guard.sites.iter().any(|s| s.id == site_id);
            if !found {
                return false;
            }
            let snap = Arc::make_mut(&mut *guard);
            if let Some(site) = snap.sites.iter_mut().find(|s| s.id == site_id) {
                site.acknowledged = false;
                site.changed = true;
                self.recompute_dss_locked(snap);
            }
            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
        true
    }

    /// Повторно увімкнути моніторинг HTTP-сайту та скинути backoff/лічильник помилок.
    pub async fn enable_site(&self, site_id: &str) -> bool {
        let snapshot_to_save = {
            let mut guard = self.state.write();
            let found = guard.sites.iter().any(|s| s.id == site_id);
            if !found {
                return false;
            }
            let snap = Arc::make_mut(&mut *guard);
            if let Some(site) = snap.sites.iter_mut().find(|s| s.id == site_id) {
                site.health.reset_manual();
                let site_name = site.name.clone();
                self.recompute_dss_locked(snap);
                snap.push_log(format!(
                    "Моніторинг сайту `{site_name}` повторно увімкнено."
                ));
            }
            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
        true
    }

    /// Повторно увімкнути моніторинг стрічки новин та скинути backoff/лічильник помилок.
    pub async fn enable_feed(&self, feed_id: &str) -> bool {
        let snapshot_to_save = {
            let mut guard = self.state.write();
            let found = guard.feeds.iter().any(|f| f.id == feed_id);
            if !found {
                return false;
            }
            let snap = Arc::make_mut(&mut *guard);
            if let Some(feed) = snap.feeds.iter_mut().find(|f| f.id == feed_id) {
                feed.health.reset_manual();
                let feed_name = feed.name.clone();
                self.recompute_dss_locked(snap);
                snap.push_log(format!(
                    "Моніторинг стрічки `{feed_name}` повторно увімкнено."
                ));
            }
            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
        true
    }

    /// Повторно увімкнути моніторинг API НБУ та скинути backoff/лічильник помилок.
    pub async fn enable_nbu(&self) {
        let snapshot_to_save = {
            let mut guard = self.state.write();
            let snap = Arc::make_mut(&mut *guard);
            snap.nbu_health.reset_manual();
            self.recompute_dss_locked(snap);
            snap.push_log("Моніторинг API НБУ повторно увімкнено.");
            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
    }

    /// Додає або оновлює ручний факт користувача та перераховує вивід ІСППР.
    pub async fn upsert_manual_fact(&self, key: impl Into<String>, value: FactValue, cf: f64) {
        let key = key.into();
        let snapshot_to_save = {
            let mut guard = self.state.write();
            let snap = Arc::make_mut(&mut *guard);
            let fact = Fact::new(
                key.clone(),
                value.clone(),
                "Ручне введення користувача",
                cf,
                FactOrigin::Manual,
            );
            let effective_cf = fact.cf;
            snap.manual_facts.insert(key.clone(), fact);

            let base_facts = snap.base_facts();
            snap.inference = self.engine.evaluate(&base_facts);
            snap.push_log(format!(
                "Користувач додав факт `{key} = {value}` (CF={effective_cf:.2}); ІСППР перерахувала вивід."
            ));
            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
    }

    /// Очищає всі ручні факти та відновлює вивід ІСППР лише на основі даних Web-агента.
    pub async fn clear_manual_facts(&self) {
        let snapshot_to_save = {
            let mut guard = self.state.write();
            let snap = Arc::make_mut(&mut *guard);
            snap.manual_facts.clear();
            let base_facts = snap.base_facts();
            snap.inference = self.engine.evaluate(&base_facts);
            snap.push_log("Ручні факти очищено; відновлено вивід лише на основі даних Web-агента.");
            Arc::clone(&*guard)
        };

        self.emit_update(AgentUpdateKind::StateChanged, &snapshot_to_save);
        self.persist_state_checked(snapshot_to_save).await;
    }

    fn recompute_dss_locked(&self, snap: &mut AgentSnapshot) {
        snap.agent_facts = extract_facts(
            &snap.sites,
            &snap.feeds,
            &snap.articles,
            &snap.currencies,
            &snap.nbu_health,
        );
        let base_facts = snap.base_facts();
        snap.inference = self.engine.evaluate(&base_facts);
    }
}

fn classify_error(err: &AgentError) -> ErrorKind {
    match err {
        AgentError::FeedParse(_)
        | AgentError::Json(_)
        | AgentError::TomlDe(_)
        | AgentError::ResponseTooLarge { .. } => ErrorKind::Parse,
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

fn load_persisted_snapshot(path: &Path) -> AgentSnapshot {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return AgentSnapshot::default(),
        Err(err) => {
            warn!(
                path = %path.display(),
                error = %err,
                "Не вдалося прочитати збережений стан агента; використано порожній стан"
            );
            return AgentSnapshot::default();
        }
    };

    match serde_json::from_str(&content) {
        Ok(snap) => snap,
        Err(err) => {
            warn!(
                path = %path.display(),
                error = %err,
                "Пошкоджений файл стану агента; використано порожній стан"
            );
            AgentSnapshot::default()
        }
    }
}

async fn persist_snapshot(path: &Path, snapshot: Arc<AgentSnapshot>) -> AgentResult<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await?;
    }

    let json = tokio::task::spawn_blocking(move || serde_json::to_string_pretty(snapshot.as_ref()))
        .await??;

    let mut tmp_name = path
        .file_name()
        .map_or_else(|| std::ffi::OsString::from("state.json"), ToOwned::to_owned);
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);

    tokio::fs::write(&tmp_path, json).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}
