use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering,
};

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use dss::{
    FactValue,
    InferenceEngine,
    KnowledgeBase,
};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use web_agent::WebAgent;
use web_agent::config::{
    AppConfig,
    DssConfig,
    HttpSiteConfig,
    KeywordCriterion,
    LoggingConfig,
    NbuMetricsConfig,
    NewsFeedConfig,
    ServerConfig,
};
use web_agent::feeds::{
    extract_cves,
    filter_and_rank_items,
    parse_feed_bytes,
};
use web_agent::health::{
    ErrorKind,
    FailureTransition,
    RetryPolicy,
    SourceHealth,
};

#[test]
fn config_deserialization_rejects_invalid_values() {
    let bad_poll = r#"
[server]
bind_addr = "127.0.0.1:8080"
poll_interval_secs = 0

[dss]
rules_path = "knowledge/rules.toml"
state_path = "data/state.json"
"#;
    assert!(toml::from_str::<AppConfig>(bad_poll).is_err());

    let bad_retry = r#"
[server]
bind_addr = "127.0.0.1:8080"

[server.retry_policy]
parse_max_retries = 3
initial_backoff_secs = 60
max_backoff_secs = 10

[dss]
rules_path = "knowledge/rules.toml"
state_path = "data/state.json"
"#;
    assert!(toml::from_str::<AppConfig>(bad_retry).is_err());

    let bad_keyword = r#"
[server]
bind_addr = "127.0.0.1:8080"

[dss]
rules_path = "knowledge/rules.toml"
state_path = "data/state.json"

[[news_feeds]]
id = "f1"
name = "Feed"
group = "cybersecurity"
url = "http://localhost/rss"
keywords = [{ word = "rust", weight = -2.0 }]
"#;
    assert!(toml::from_str::<AppConfig>(bad_keyword).is_err());

    let bad_log_max_files = r#"
[server]
bind_addr = "127.0.0.1:8080"

[logging]
dir = "logs"
file_prefix = "server.log"
rotation = "daily"
max_files = 0

[dss]
rules_path = "knowledge/rules.toml"
state_path = "data/state.json"
"#;
    assert!(toml::from_str::<AppConfig>(bad_log_max_files).is_err());

    let bad_log_prefix = r#"
[server]
bind_addr = "127.0.0.1:8080"

[logging]
dir = "logs"
file_prefix = "   "
rotation = "hourly"
max_files = 5

[dss]
rules_path = "knowledge/rules.toml"
state_path = "data/state.json"
"#;
    assert!(toml::from_str::<AppConfig>(bad_log_prefix).is_err());
}

#[test]
fn cve_scanner_parses_valid_ids_without_regex() {
    let text = "Alert: cve-2026-12345, (CVE-2025-9999), duplicate CVE-2026-12345, bad CVE-26-123, bad CVE-2026-12.";
    let cves = extract_cves(text);
    assert_eq!(cves, vec!["CVE-2026-12345", "CVE-2025-9999"]);
}

#[test]
fn rss_parsing_and_relevance_ranking() {
    let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Test</title>
  <item>
    <title>Minor update notes</title>
    <link>https://example.org/1</link>
    <description>&lt;p&gt;Mentions a small vulnerability in passing.&lt;/p&gt;</description>
  </item>
  <item>
    <title>Critical zero-day vulnerability CVE-2026-44123</title>
    <link>https://example.org/2</link>
    <description>Active exploit targeting servers with CVE-2026-44123.</description>
  </item>
  <item>
    <title>Unrelated cooking recipe</title>
    <link>https://example.org/3</link>
    <description>Pasta and tomatoes.</description>
  </item>
</channel></rss>"#;

    let raw_items = parse_feed_bytes(rss.as_bytes()).expect("RSS must parse");
    assert_eq!(raw_items.len(), 3);

    let feed_cfg = NewsFeedConfig {
        id: "test-sec".into(),
        name: "Test Security".into(),
        group: "cybersecurity".into(),
        url: "http://localhost/rss".into(),
        keywords: vec![
            KeywordCriterion::new("critical".into(), 3.0).unwrap(),
            KeywordCriterion::new("zero-day".into(), 3.5).unwrap(),
            KeywordCriterion::new("vulnerability".into(), 2.0).unwrap(),
            KeywordCriterion::new("exploit".into(), 2.5).unwrap(),
        ],
    };

    let ranked = filter_and_rank_items(&feed_cfg, raw_items);
    assert_eq!(ranked.len(), 2);
    assert!(ranked[0].title.contains("Critical"));
    assert_eq!(ranked[0].cves, vec!["CVE-2026-44123"]);
    assert!(ranked[0].relevance_score > ranked[1].relevance_score);
}

#[test]
fn source_health_throttles_with_backoff_then_disables_on_parse_errors() {
    let policy = RetryPolicy::new(3, 10, 60).unwrap();
    let t0 = OffsetDateTime::now_utc();
    let mut health = SourceHealth::default();

    // 1-ша помилка парсингу: не вимикає, backoff 10 с
    let tr1 = health.record_failure(t0, ErrorKind::Parse, "bad xml".into(), &policy);
    assert_eq!(
        tr1,
        FailureTransition::Throttled {
            attempt: 1,
            backoff_secs: 10
        }
    );
    assert!(!health.disabled);
    assert!(!health.should_poll(t0 + time::Duration::seconds(5)));
    assert!(health.should_poll(t0 + time::Duration::seconds(11)));

    // 2-га помилка парсингу: backoff подвоюється до 20 с
    let t1 = t0 + time::Duration::seconds(11);
    let tr2 = health.record_failure(t1, ErrorKind::Parse, "bad xml".into(), &policy);
    assert_eq!(
        tr2,
        FailureTransition::Throttled {
            attempt: 2,
            backoff_secs: 20
        }
    );
    assert!(!health.disabled);

    // 3-тя помилка парсингу: вимикає джерело
    let t2 = t1 + time::Duration::seconds(21);
    let tr3 = health.record_failure(t2, ErrorKind::Parse, "bad xml".into(), &policy);
    assert_eq!(tr3, FailureTransition::Disabled { attempts: 3 });
    assert!(health.disabled);
    assert!(!health.should_poll(t2 + time::Duration::hours(24)));

    // Ручне скидання відновлює опитування
    health.reset_manual();
    assert!(!health.disabled);
    assert!(health.should_poll(t2));

    // Тимчасова помилка НЕ вимикає джерело назавжди навіть після 5 спроб, а обмежується max_backoff
    for i in 1..=5 {
        let tr = health.record_failure(
            t2 + time::Duration::seconds(i * 100),
            ErrorKind::Transient,
            "timeout".into(),
            &policy,
        );
        assert!(matches!(tr, FailureTransition::Throttled { .. }));
        assert!(!health.disabled);
    }
}

#[derive(Clone)]
struct MockServerState {
    page_body: Arc<RwLock<String>>,
    rss_body: Arc<RwLock<String>>,
    nbu_body: Arc<RwLock<String>>,
    broken_feed_is_broken: Arc<AtomicBool>,
    broken_feed_hits: Arc<AtomicUsize>,
}

async fn spawn_mock_web_environment(state: MockServerState) -> String {
    let app = Router::new()
        .route(
            "/page",
            get(|State(s): State<MockServerState>| async move { s.page_body.read().await.clone() }),
        )
        .route(
            "/rss",
            get(|State(s): State<MockServerState>| async move { s.rss_body.read().await.clone() }),
        )
        .route(
            "/broken-rss",
            get(|State(s): State<MockServerState>| async move {
                s.broken_feed_hits.fetch_add(1, Ordering::SeqCst);
                if s.broken_feed_is_broken.load(Ordering::SeqCst) {
                    "<<<NOT VALID XML OR FEED AT ALL>>>".to_string()
                } else {
                    r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Fixed</title>
                    <item><title>Rust release update</title><link>http://example/fixed</link><description>Recovered</description></item>
                    </channel></rss>"#
                        .to_string()
                }
            }),
        )
        .route(
            "/nbu",
            get(|State(s): State<MockServerState>| async move { s.nbu_body.read().await.clone() }),
        )
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn end_to_end_dynamic_kb_population_and_fault_resilience() {
    let mock_state = MockServerState {
        page_body: Arc::new(RwLock::new("Initial news portal content v1".into())),
        rss_body: Arc::new(RwLock::new(
            r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Sec</title>
            <item><title>Quiet day in IT</title><link>http://example/0</link><description>Routine maintenance completed.</description></item>
            </channel></rss>"#
                .into(),
        )),
        nbu_body: Arc::new(RwLock::new(
            r#"[{"cc":"USD","txt":"Долар США","rate":39.50,"exchangedate":"23.11.2026"},
                {"cc":"EUR","txt":"Євро","rate":42.10,"exchangedate":"23.11.2026"}]"#
                .into(),
        )),
        broken_feed_is_broken: Arc::new(AtomicBool::new(true)),
        broken_feed_hits: Arc::new(AtomicUsize::new(0)),
    };

    let base_url = spawn_mock_web_environment(mock_state.clone()).await;

    let temp_state = PathBuf::from(format!(
        "/tmp/web_agent_dss_test_state_{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp_state);

    // Використовуємо initial_backoff_secs = 0 для миттєвого прогону повторних спроб у тесті
    let retry_policy = RetryPolicy::new(2, 0, 0).unwrap();
    let config = AppConfig::new(
        ServerConfig::new("127.0.0.1:0".into(), 60, 5, 4, retry_policy).unwrap(),
        LoggingConfig::default(),
        DssConfig {
            rules_path: PathBuf::from("../../knowledge/rules.toml"),
            state_path: temp_state.clone(),
        },
        NbuMetricsConfig {
            enabled: true,
            url: format!("{base_url}/nbu"),
            currencies: vec!["USD".into(), "EUR".into()],
        },
        vec![HttpSiteConfig {
            id: "dyn-page".into(),
            name: "Dynamic Portal".into(),
            url: format!("{base_url}/page"),
            category: "news".into(),
        }],
        vec![
            NewsFeedConfig {
                id: "sec-feed".into(),
                name: "Security Feed".into(),
                group: "cybersecurity".into(),
                url: format!("{base_url}/rss"),
                keywords: vec![
                    KeywordCriterion::new("critical".into(), 3.0).unwrap(),
                    KeywordCriterion::new("cve".into(), 2.5).unwrap(),
                ],
            },
            NewsFeedConfig {
                id: "broken-feed".into(),
                name: "Broken Feed".into(),
                group: "rust-ecosystem".into(),
                url: format!("{base_url}/broken-rss"),
                keywords: vec![KeywordCriterion::new("rust".into(), 2.0).unwrap()],
            },
        ],
    )
    .unwrap();

    let kb = KnowledgeBase::load_from_file(&config.dss.rules_path).unwrap();
    let agent = WebAgent::new(config, InferenceEngine::new(kb)).unwrap();

    // Цикл 1: спокійний стан; broken-feed має 1-шу помилку парсингу (ще НЕ вимкнений!)
    agent.poll_once().await;
    let s1 = agent.snapshot().await;
    assert!(s1.sites[0].is_visible_in_changed_table());
    assert_eq!(
        s1.inference.inferred_facts["cyber_threat_level"].value,
        FactValue::Text("normal".into())
    );
    let broken_f1 = s1.feeds.iter().find(|f| f.id == "broken-feed").unwrap();
    assert!(!broken_f1.health.disabled);
    assert_eq!(broken_f1.health.consecutive_failures, 1);

    // Позначаємо HTTP-сайт як переглянутий — він зникає з таблиці змін
    assert!(agent.acknowledge_site("dyn-page").await);
    let s_ack = agent.snapshot().await;
    assert!(!s_ack.sites[0].is_visible_in_changed_table());

    // Динамічно змінюємо веб-сторінку, публікуємо критичну CVE-новину та піднімаємо курси НБУ (п. 10)
    *mock_state.page_body.write().await =
        "Updated breaking portal content v2 with significantly larger payload!".into();
    *mock_state.rss_body.write().await =
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Sec</title>
        <item><title>Critical vulnerability CVE-2026-77777</title><link>http://example/cve</link>
        <description>Critical patch required immediately for CVE-2026-77777.</description></item>
        </channel></rss>"#
            .into();
    *mock_state.nbu_body.write().await =
        r#"[{"cc":"USD","txt":"Долар США","rate":42.80,"exchangedate":"24.11.2026"},
            {"cc":"EUR","txt":"Євро","rate":46.20,"exchangedate":"24.11.2026"}]"#
            .into();

    // Цикл 2: Web-агент виявляє всі зміни; broken-feed досягає parse_max_retries=2 і вимикається
    agent.poll_once().await;
    let s2 = agent.snapshot().await;

    // HTTP-сайт знову у таблиці змін, бо його розмір реально змінився
    assert!(s2.sites[0].is_visible_in_changed_table());
    assert!(s2.sites[0].size_delta().unwrap() > 0);

    // Зламана стрічка вимкнена, але інші джерела відпрацювали коректно
    let broken_f2 = s2.feeds.iter().find(|f| f.id == "broken-feed").unwrap();
    assert!(broken_f2.health.disabled);
    assert!(broken_f2.health.last_error.is_some());
    assert_eq!(
        s2.agent_facts["disabled_sources_count"].value,
        FactValue::Number(1.0)
    );

    // ІСППР перейшла у кризовий режим на основі автоматично поповнених фактів БЗ
    assert_eq!(
        s2.inference.inferred_facts["cyber_threat_level"].value,
        FactValue::Text("critical".into())
    );
    assert_eq!(
        s2.inference.inferred_facts["operational_mode"].value,
        FactValue::Text("crisis_response".into())
    );

    // Цикл 3: вимкнена стрічка НЕ опитується повторно
    let hits_before = mock_state.broken_feed_hits.load(Ordering::SeqCst);
    agent.poll_once().await;
    assert_eq!(
        mock_state.broken_feed_hits.load(Ordering::SeqCst),
        hits_before
    );

    // «Виправляємо» парсинг стрічки та повторно вмикаємо її — агент знову отримує її статті
    mock_state
        .broken_feed_is_broken
        .store(false, Ordering::SeqCst);
    assert!(agent.enable_feed("broken-feed").await);
    agent.poll_once().await;
    let s4 = agent.snapshot().await;
    let broken_f4 = s4.feeds.iter().find(|f| f.id == "broken-feed").unwrap();
    assert!(!broken_f4.health.disabled);
    assert_eq!(broken_f4.matched_count, 1);

    let _ = std::fs::remove_file(&temp_state);
}
