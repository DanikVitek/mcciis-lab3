use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use dss::{
    InferenceEngine,
    KnowledgeBase,
};
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::{
    TcpListener,
    TcpStream,
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
use web_agent_server::web::build_router_with_frontend_dir;

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

/// Мінімальний RFC 6455 клієнтський читач текстових кадрів сервера (без зовнішніх залежностей).
struct MinimalWsClient {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl MinimalWsClient {
    async fn connect(addr: std::net::SocketAddr, path: &str) -> Self {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let req = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).await.unwrap();

        let mut buf = Vec::new();
        let header_end = loop {
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "EOF before WebSocket handshake completed");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };

        let header = String::from_utf8_lossy(&buf[..header_end]);
        assert!(
            header.starts_with("HTTP/1.1 101"),
            "Expected HTTP 101 Switching Protocols, got: {header}"
        );
        buf.drain(..header_end);

        Self { stream, buf }
    }

    async fn ensure_bytes(&mut self, needed: usize) {
        while self.buf.len() < needed {
            let mut chunk = [0u8; 1024];
            let n = tokio::time::timeout(Duration::from_secs(5), self.stream.read(&mut chunk))
                .await
                .expect("timed out reading WebSocket frame")
                .unwrap();
            assert!(n > 0, "WebSocket closed unexpectedly");
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    async fn next_text_json(&mut self) -> serde_json::Value {
        loop {
            self.ensure_bytes(2).await;
            let opcode = self.buf[0] & 0x0f;
            let masked = (self.buf[1] & 0x80) != 0;
            assert!(!masked, "Server WebSocket frames must not be masked");
            let mut payload_len = (self.buf[1] & 0x7f) as usize;
            let mut offset = 2;

            if payload_len == 126 {
                self.ensure_bytes(4).await;
                payload_len = u16::from_be_bytes([self.buf[2], self.buf[3]]) as usize;
                offset = 4;
            } else if payload_len == 127 {
                self.ensure_bytes(10).await;
                payload_len = u64::from_be_bytes(self.buf[2..10].try_into().unwrap()) as usize;
                offset = 10;
            }

            self.ensure_bytes(offset + payload_len).await;
            let payload = self.buf[offset..offset + payload_len].to_vec();
            self.buf.drain(..offset + payload_len);

            if opcode == 0x1 {
                return serde_json::from_slice(&payload).unwrap();
            }
        }
    }
}

#[tokio::test]
async fn websocket_pushes_connected_and_poll_lifecycle_events() {
    let mock_base = spawn_mock_sources().await;
    let temp_state = PathBuf::from(format!(
        "/tmp/web_agent_server_ws_test_{}.json",
        std::process::id()
    ));
    let temp_frontend_dir = PathBuf::from(format!(
        "/tmp/web_agent_server_ws_frontend_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp_state);
    let _ = std::fs::remove_dir_all(&temp_frontend_dir);
    std::fs::create_dir_all(&temp_frontend_dir).unwrap();
    std::fs::write(temp_frontend_dir.join("index.html"), "<html>SPA</html>").unwrap();

    let config = AppConfig::new(
        ServerConfig::new("127.0.0.1:0".into(), 300, 5, 4, RetryPolicy::default()).unwrap(),
        LoggingConfig::new(
            PathBuf::from("/tmp/web_agent_ws_logs"),
            "ws-test.log".to_string(),
            LogRotation::Daily,
            3,
        )
        .unwrap(),
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
            id: "ws-site".into(),
            name: "WS Portal".into(),
            url: format!("{mock_base}/page"),
            category: "mock".into(),
        }],
        vec![NewsFeedConfig {
            id: "ws-feed".into(),
            name: "WS Security Feed".into(),
            group: "cybersecurity".into(),
            url: format!("{mock_base}/rss"),
            keywords: vec![KeywordCriterion::new("cve".into(), 2.5).unwrap()],
        }],
    )
    .unwrap();

    let kb = KnowledgeBase::load_from_file(&config.dss.rules_path).unwrap();
    let agent = WebAgent::new(config, InferenceEngine::new(kb)).unwrap();

    let router = build_router_with_frontend_dir(agent.clone(), temp_frontend_dir.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    let mut ws = MinimalWsClient::connect(addr, "/api/ws").await;

    // 1. Початкова подія connected
    let connected = ws.next_text_json().await;
    assert_eq!(connected["kind"], "connected");
    assert_eq!(connected["poll_count"], 0);
    assert_eq!(connected["poll_in_progress"], false);

    // 2. Запускаємо ітерацію агента та перевіряємо миттєві WebSocket-події poll_started -> poll_completed
    let poll_agent = agent.clone();
    let poll_task = tokio::spawn(async move {
        poll_agent.poll_once().await;
    });

    let started = ws.next_text_json().await;
    assert_eq!(started["kind"], "poll_started");
    assert_eq!(started["poll_in_progress"], true);

    let completed = ws.next_text_json().await;
    assert_eq!(completed["kind"], "poll_completed");
    assert_eq!(completed["poll_count"], 1);
    assert_eq!(completed["poll_in_progress"], false);
    assert_eq!(completed["articles_count"], 1);

    poll_task.await.unwrap();

    // 3. Мутація стану (ack) також надсилає state_changed
    assert!(agent.acknowledge_site("ws-site").await);
    let mutated = ws.next_text_json().await;
    assert_eq!(mutated["kind"], "state_changed");
    assert_eq!(mutated["poll_count"], 1);

    server_handle.abort();
    let _ = server_handle.await;
    let _ = std::fs::remove_file(&temp_state);
    let _ = std::fs::remove_dir_all(&temp_frontend_dir);
}
