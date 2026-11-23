use std::io::Write;
use std::path::PathBuf;

use axum::Router;
use axum::routing::get;
use dss::{
    InferenceEngine,
    KnowledgeBase,
};
use tokio::net::TcpListener;
use tracing::Dispatch;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{
    EnvFilter,
    fmt,
};
use web_agent::WebAgent;
use web_agent::config::{
    AppConfig,
    DssConfig,
    HttpSiteConfig,
    KeywordCriterion,
    LogRotation,
    LoggingConfig,
    NbuMetricsConfig,
    NewsFeedConfig,
    ServerConfig,
};
use web_agent::health::RetryPolicy;
use web_agent_server::logging::build_rotating_appender;
use web_agent_server::web::build_router;

async fn spawn_mock_sources() -> String {
    let app = Router::new()
        .route("/page", get(|| async { "Mock monitored portal content" }))
        .route(
            "/rss",
            get(|| async {
                r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Mock Security</title>
  <item>
    <title>Critical vulnerability CVE-2026-55555</title>
    <link>http://mock.local/cve</link>
    <description>Patch critical vulnerability CVE-2026-55555.</description>
  </item>
</channel></rss>"#
            }),
        )
        .route(
            "/nbu",
            get(|| async {
                r#"[{"cc":"USD","txt":"Долар США","rate":41.25,"exchangedate":"23.11.2026"}]"#
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn web_interface_serves_required_routes_and_logs_to_rotating_file() {
    let mock_base = spawn_mock_sources().await;

    let temp_state = PathBuf::from(format!(
        "/tmp/web_agent_server_ui_test_{}.json",
        std::process::id()
    ));
    let temp_log_dir = PathBuf::from(format!(
        "/tmp/web_agent_server_e2e_logs_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp_state);
    let _ = std::fs::remove_dir_all(&temp_log_dir);

    let log_cfg = LoggingConfig::new(
        temp_log_dir.clone(),
        "web-agent-server.log".to_string(),
        LogRotation::Daily,
        7,
    )
    .unwrap();

    let config = AppConfig::new(
        ServerConfig::new("127.0.0.1:0".into(), 300, 5, 4, RetryPolicy::default()).unwrap(),
        log_cfg.clone(),
        DssConfig {
            rules_path: PathBuf::from("../../knowledge/rules.toml"),
            state_path: temp_state.clone(),
        },
        NbuMetricsConfig {
            enabled: true,
            url: format!("{mock_base}/nbu"),
            currencies: vec!["USD".into()],
        },
        vec![HttpSiteConfig {
            id: "mock-site".into(),
            name: "Mock Portal".into(),
            url: format!("{mock_base}/page"),
            category: "mock".into(),
        }],
        vec![NewsFeedConfig {
            id: "mock-feed".into(),
            name: "Mock Security Feed".into(),
            group: "cybersecurity".into(),
            url: format!("{mock_base}/rss"),
            keywords: vec![
                KeywordCriterion::new("critical".into(), 3.0).unwrap(),
                KeywordCriterion::new("cve".into(), 2.5).unwrap(),
            ],
        }],
    )
    .unwrap();

    let (file_writer, guard) = build_rotating_appender(&log_cfg).unwrap();
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::new(
            "info,web_agent=debug,web_agent_server=debug,tower_http=debug",
        ))
        .with(
            fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_target(true),
        );
    let dispatch = Dispatch::new(subscriber);

    let kb = KnowledgeBase::load_from_file(&config.dss.rules_path).unwrap();
    let agent = WebAgent::new(config, InferenceEngine::new(kb)).unwrap();

    // 1. Виконуємо повний цикл опитування мок-джерел під ротованим файловим логером
    agent.poll_once().with_subscriber(dispatch.clone()).await;

    let router = build_router(agent);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(
        async move {
            let _ = axum::serve(listener, router).await;
        }
        .with_subscriber(dispatch.clone()),
    );

    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    // 2. /index.html
    let idx = client
        .get(format!("{base}/index.html"))
        .send()
        .await
        .unwrap();
    assert!(idx.status().is_success());
    let idx_html = idx.text().await.unwrap();
    assert!(idx_html.contains("HTTP/HTTPS-сайти"));
    assert!(idx_html.contains("Відібрані повідомлення"));
    assert!(idx_html.contains("Mock Portal"));
    assert!(idx_html.contains("CVE-2026-55555"));

    // 3. /conf.html
    let conf = client
        .get(format!("{base}/conf.html"))
        .send()
        .await
        .unwrap();
    assert!(conf.status().is_success());
    let conf_html = conf.text().await.unwrap();
    assert!(conf_html.contains("poll_interval_secs"));
    assert!(conf_html.contains("web-agent-server.log"));
    assert!(conf_html.contains("append-only файл з ротацією"));

    // 4. POST /dss/fact → ручне додавання факту та перевірка на /dss.html і /api/state
    let resp = client
        .post(format!("{base}/dss/fact"))
        .form(&[("key", "nbu_rate_usd"), ("value", "43.25"), ("cf", "1.0")])
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let dss_html = resp.text().await.unwrap();
    assert!(dss_html.contains("R_FX_USD_HIGH"));

    let state_json: serde_json::Value = client
        .get(format!("{base}/api/state"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(state_json["manual_facts"]["nbu_rate_usd"]["value"], 43.25);

    // Завершуємо мок-сервер і скидаємо буфер неблокуючого файлового логера
    server_handle.abort();
    let _ = server_handle.await;
    drop(dispatch);
    drop(guard);

    // 5. Перевіряємо ротований append-only лог-файл
    let entries: Vec<PathBuf> = std::fs::read_dir(&temp_log_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("web-agent-server.log")
    );

    let log_content = std::fs::read_to_string(&entries[0]).unwrap();
    assert!(!log_content.contains('\x1b'));
    assert!(log_content.contains("web_agent"));
    assert!(log_content.contains("Цикл Web-агента завершено"));
    assert!(log_content.contains("poll=1"));

    let _ = std::fs::remove_file(&temp_state);
    let _ = std::fs::remove_dir_all(&temp_log_dir);
}

#[test]
fn rotating_file_appender_creates_and_appends_log_file() {
    let temp_dir = PathBuf::from(format!("/tmp/web_agent_log_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);

    let cfg = LoggingConfig::new(
        temp_dir.clone(),
        "server-test.log".to_string(),
        LogRotation::Daily,
        3,
    )
    .unwrap();

    {
        let (mut writer, _guard) = build_rotating_appender(&cfg).unwrap();
        writeln!(writer, "first log line").unwrap();
    }

    {
        let (mut writer, _guard) = build_rotating_appender(&cfg).unwrap();
        writeln!(writer, "second log line").unwrap();
    }

    let entries: Vec<PathBuf> = std::fs::read_dir(&temp_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("server-test.log")
    );

    let content = std::fs::read_to_string(&entries[0]).unwrap();
    assert!(content.contains("first log line"));
    assert!(content.contains("second log line"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
