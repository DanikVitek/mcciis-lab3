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
use web_agent_server::web::build_router_with_frontend_dir;
#[cfg(debug_assertions)]
use web_agent_server::web::{
    FrontendMode,
    build_router_with_frontend_mode,
};

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
async fn api_serves_ui_state_spa_static_files_and_logs_to_rotating_file() {
    let mock_base = spawn_mock_sources().await;

    let temp_state = PathBuf::from(format!(
        "/tmp/web_agent_server_api_test_{}.json",
        std::process::id()
    ));
    let temp_log_dir = PathBuf::from(format!(
        "/tmp/web_agent_server_e2e_logs_{}",
        std::process::id()
    ));
    let temp_frontend_dir = PathBuf::from(format!(
        "/tmp/web_agent_server_e2e_frontend_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp_state);
    let _ = std::fs::remove_dir_all(&temp_log_dir);
    let _ = std::fs::remove_dir_all(&temp_frontend_dir);

    std::fs::create_dir_all(temp_frontend_dir.join("_app")).unwrap();
    std::fs::write(
        temp_frontend_dir.join("index.html"),
        "<!doctype html><html><body><div id=\"spa-root\">SPA OK</div></body></html>",
    )
    .unwrap();
    std::fs::write(
        temp_frontend_dir.join("_app/bundle.js"),
        "console.log('static asset ok');",
    )
    .unwrap();

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

    // 1. Повний цикл опитування мок-джерел під ротованим файловим логером
    agent.poll_once().with_subscriber(dispatch.clone()).await;

    let router = build_router_with_frontend_dir(agent, temp_frontend_dir.clone());
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

    // 2. Статичний фронтенд SPA (корінь, асети та client-side fallback 200 OK для /dss, /conf, /guide, /article/*)
    for route in ["/", "/dss", "/conf", "/guide", "/article/demo"] {
        let resp = client.get(format!("{base}{route}")).send().await.unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::OK,
            "route {route} did not return 200 OK"
        );
        let html = resp.text().await.unwrap();
        assert!(
            html.contains("SPA OK"),
            "route {route} did not serve index.html"
        );
    }

    let js = client
        .get(format!("{base}/_app/bundle.js"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(js.contains("static asset ok"));

    let unknown_api = client
        .get(format!("{base}/api/unknown-route"))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown_api.status(), reqwest::StatusCode::NOT_FOUND);

    // 3. GET /api/health та /api/ui-state
    let health = client
        .get(format!("{base}/api/health"))
        .send()
        .await
        .unwrap();
    assert!(health.status().is_success());

    let ui_state: serde_json::Value = client
        .get(format!("{base}/api/ui-state"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ui_state["snapshot"]["sites"][0]["name"], "Mock Portal");
    assert_eq!(
        ui_state["snapshot"]["articles"][0]["cves"][0],
        "CVE-2026-55555"
    );
    assert!(ui_state["rules"].is_array());
    assert!(
        ui_state["pretty_toml"]
            .as_str()
            .unwrap()
            .contains("poll_interval_secs")
    );
    assert!(
        ui_state["pretty_toml"]
            .as_str()
            .unwrap()
            .contains("web-agent-server.log")
    );

    // 4. GET /api/articles/{id} та 404 для невідомого ID
    let art_id = ui_state["snapshot"]["articles"][0]["id"].as_str().unwrap();
    let art_resp = client
        .get(format!("{base}/api/articles/{art_id}"))
        .send()
        .await
        .unwrap();
    assert!(art_resp.status().is_success());
    let art_json: serde_json::Value = art_resp.json().await.unwrap();
    assert_eq!(art_json["id"], art_id);

    let missing_resp = client
        .get(format!("{base}/api/articles/nonexistent"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_resp.status(), reqwest::StatusCode::NOT_FOUND);

    // 5. POST /api/sites/{id}/ack і /unack
    let acked: serde_json::Value = client
        .post(format!("{base}/api/sites/mock-site/ack"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(acked["snapshot"]["sites"][0]["acknowledged"], true);

    let unacked: serde_json::Value = client
        .post(format!("{base}/api/sites/mock-site/unack"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unacked["snapshot"]["sites"][0]["acknowledged"], false);

    // 6. POST /api/dss/fact → ручне додавання факту та перевірка перерахунку ІСППР і /api/state
    let updated: serde_json::Value = client
        .post(format!("{base}/api/dss/fact"))
        .json(&serde_json::json!({
            "key": "nbu_rate_usd",
            "value": "43.25",
            "cf": 1.0
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        updated["snapshot"]["manual_facts"]["nbu_rate_usd"]["value"],
        43.25
    );
    let fired = updated["snapshot"]["inference"]["fired_rule_ids"]
        .as_array()
        .unwrap();
    assert!(fired.iter().any(|r| r == "R_FX_USD_HIGH"));

    let state_json: serde_json::Value = client
        .get(format!("{base}/api/state"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(state_json["manual_facts"]["nbu_rate_usd"]["value"], 43.25);

    // 7. POST /api/dss/fact/clear
    let cleared: serde_json::Value = client
        .post(format!("{base}/api/dss/fact/clear"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        cleared["snapshot"]["manual_facts"]
            .as_object()
            .unwrap()
            .is_empty()
    );

    // Завершуємо мок-сервер і скидаємо буфер неблокуючого файлового логера
    server_handle.abort();
    let _ = server_handle.await;
    drop(dispatch);
    drop(guard);

    // 8. Перевіряємо ротований append-only лог-файл
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
    let _ = std::fs::remove_dir_all(&temp_frontend_dir);
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn dev_redirect_mode_redirects_ui_routes_and_keeps_api_routes() {
    let temp_state = PathBuf::from(format!(
        "/tmp/web_agent_server_dev_redirect_{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp_state);

    let config = AppConfig::new(
        ServerConfig::new("127.0.0.1:0".into(), 300, 5, 4, RetryPolicy::default()).unwrap(),
        LoggingConfig::new(
            PathBuf::from("/tmp/web_agent_dev_redirect_logs"),
            "dev-test.log".to_string(),
            LogRotation::Daily,
            3,
        )
        .unwrap(),
        DssConfig {
            rules_path: PathBuf::from("../../knowledge/rules.toml"),
            state_path: temp_state.clone(),
        },
        NbuMetricsConfig {
            enabled: false,
            url: "http://127.0.0.1:9/nbu".into(),
            currencies: vec![],
        },
        vec![],
        vec![],
    )
    .unwrap();

    let kb = KnowledgeBase::load_from_file(&config.dss.rules_path).unwrap();
    let agent = WebAgent::new(config, InferenceEngine::new(kb)).unwrap();

    let router = build_router_with_frontend_mode(
        agent,
        FrontendMode::DevRedirect("http://127.0.0.1:5173".to_string()),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let base = format!("http://{addr}");

    let redirect_resp = client
        .get(format!("{base}/dss?filter=critical"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        redirect_resp.status(),
        reqwest::StatusCode::TEMPORARY_REDIRECT
    );
    assert_eq!(
        redirect_resp.headers().get("location").unwrap(),
        "http://127.0.0.1:5173/dss?filter=critical"
    );

    let health = client
        .get(format!("{base}/api/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), reqwest::StatusCode::OK);

    let missing_api = client
        .get(format!("{base}/api/nonexistent"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_api.status(), reqwest::StatusCode::NOT_FOUND);

    server_handle.abort();
    let _ = server_handle.await;
    let _ = std::fs::remove_file(&temp_state);
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
