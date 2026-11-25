//! HTTP JSON API, WebSocket push-потік і роздача статичного `SvelteKit` SPA.

use core::time::Duration;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::error_handling::HandleErrorLayer;
use axum::extract::ws::{
    Message,
    WebSocket,
    WebSocketUpgrade,
};
use axum::extract::{
    Path,
    State,
};
use axum::http::StatusCode;
#[cfg(debug_assertions)]
use axum::http::Uri;
use axum::response::IntoResponse;
#[cfg(debug_assertions)]
use axum::response::Redirect;
use axum::routing::{
    any,
    get,
    post,
};
use axum::{
    Json,
    Router,
};
use dss::{
    FactValue,
    Rule,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::broadcast::error::RecvError;
use tower::limit::ConcurrencyLimitLayer;
use tower::timeout::TimeoutLayer;
use tower::{
    BoxError,
    ServiceBuilder,
};
use tower_http::compression::CompressionLayer;
use tower_http::services::{
    ServeDir,
    ServeFile,
};
use tower_http::trace::TraceLayer;
use tracing::warn;
use web_agent::config::AppConfig;
use web_agent::{
    AgentSnapshot,
    AgentUpdateEvent,
    AgentUpdateKind,
    WebAgent,
};

/// Режим обслуговування клієнтського фронтенду.
#[derive(Debug, Clone)]
pub enum FrontendMode {
    /// Роздача зібраного статичного `SvelteKit` SPA з диску (основний / release-режим).
    StaticDir(PathBuf),
    /// Режим розробки (лише debug-збірка): API на Rust + перенаправлення UI-маршрутів на Vite dev server.
    #[cfg(debug_assertions)]
    DevRedirect(String),
}

/// Агрегований стан інтерфейсу, який повертається клієнту в `/api/ui-state` і мутаційних ендпоїнтах.
#[derive(Debug, Serialize)]
pub struct UiStatePayload {
    pub snapshot: Arc<AgentSnapshot>,
    pub rules: Arc<[Rule]>,
    pub config: Arc<AppConfig>,
    pub pretty_toml: Arc<str>,
}

/// Запит на додавання або оновлення ручного факту бази знань ІСППР.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualFactRequest {
    pub key: String,
    pub value: String,
    pub cf: Option<f64>,
}

#[derive(Clone)]
struct AppState {
    agent: WebAgent,
    config: Arc<AppConfig>,
    rules: Arc<[Rule]>,
    pretty_toml: Arc<str>,
}

impl AppState {
    fn new(agent: WebAgent) -> Self {
        let config = agent.config_arc();
        let rules: Arc<[Rule]> = Arc::from(agent.engine().rules());
        let pretty_toml: Arc<str> = Arc::from(
            toml::to_string_pretty(config.as_ref())
                .unwrap_or_else(|e| format!("# Помилка серіалізації TOML: {e}")),
        );
        Self {
            agent,
            config,
            rules,
            pretty_toml,
        }
    }

    fn ui_state(&self) -> UiStatePayload {
        UiStatePayload {
            snapshot: self.agent.snapshot(),
            rules: Arc::clone(&self.rules),
            config: Arc::clone(&self.config),
            pretty_toml: Arc::clone(&self.pretty_toml),
        }
    }
}

/// Повертає каталог зібраного фронтенду з `FRONTEND_DIR` або типовий `frontend/build`.
#[must_use]
pub fn default_frontend_dir() -> PathBuf {
    std::env::var("FRONTEND_DIR").map_or_else(|_| PathBuf::from("frontend/build"), PathBuf::from)
}

/// Визначає режим роздачі фронтенду залежно від змінних оточення та профілю збірки.
#[must_use]
pub fn default_frontend_mode() -> FrontendMode {
    #[cfg(debug_assertions)]
    if let Ok(dev_url) = std::env::var("DEV_FRONTEND_URL") {
        let trimmed = dev_url.trim().trim_end_matches('/').to_string();
        if !trimmed.is_empty() {
            return FrontendMode::DevRedirect(trimmed);
        }
    }
    FrontendMode::StaticDir(default_frontend_dir())
}

/// Будує маршрутизатор `axum` із типовим режимом фронтенду.
pub fn build_router(agent: WebAgent) -> Router {
    build_router_with_frontend_mode(agent, default_frontend_mode())
}

/// Будує маршрутизатор `axum` із роздачею статичного SPA із вказаного каталогу.
pub fn build_router_with_frontend_dir(agent: WebAgent, frontend_dir: PathBuf) -> Router {
    build_router_with_frontend_mode(agent, FrontendMode::StaticDir(frontend_dir))
}

/// Будує маршрутизатор `axum` із заданим [`FrontendMode`].
pub fn build_router_with_frontend_mode(agent: WebAgent, mode: FrontendMode) -> Router {
    let state = AppState::new(agent);
    let middleware = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|err: BoxError| async move {
            if err.is::<tower::timeout::error::Elapsed>() {
                (
                    StatusCode::REQUEST_TIMEOUT,
                    "Перевищено час очікування обробки запиту.".to_string(),
                )
            } else {
                warn!(error = %err, "Помилка проміжного шару HTTP-сервера");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("Сервіс тимчасово недоступний: {err}"),
                )
            }
        }))
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(ConcurrencyLimitLayer::new(64))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let api_router = Router::new()
        .route("/api/health", get(api_health_handler))
        .route("/api/state", get(api_state_handler))
        .route("/api/ui-state", get(api_ui_state_handler))
        .route("/api/articles/{id}", get(api_article_handler))
        .route("/api/poll", post(api_poll_handler))
        .route("/api/sites/{id}/ack", post(api_ack_site_handler))
        .route("/api/sites/{id}/unack", post(api_unack_site_handler))
        .route("/api/sites/{id}/enable", post(api_enable_site_handler))
        .route("/api/feeds/{id}/enable", post(api_enable_feed_handler))
        .route("/api/nbu/enable", post(api_enable_nbu_handler))
        .route("/api/dss/fact", post(api_upsert_fact_handler))
        .route("/api/dss/fact/clear", post(api_clear_facts_handler))
        .route("/api/{*path}", any(api_not_found_handler));

    let http_router = match mode {
        FrontendMode::StaticDir(frontend_dir) => {
            let spa_index = frontend_dir.join("index.html");
            let static_service = ServeDir::new(frontend_dir).fallback(ServeFile::new(spa_index));
            api_router.fallback_service(static_service)
        }
        #[cfg(debug_assertions)]
        FrontendMode::DevRedirect(dev_base) => api_router.fallback(move |uri: Uri| {
            let dev_base = dev_base.clone();
            async move {
                let path_and_query = uri
                    .path_and_query()
                    .map_or("/", axum::http::uri::PathAndQuery::as_str);
                Redirect::temporary(&format!("{dev_base}{path_and_query}"))
            }
        }),
    }
    .layer(middleware);

    // WebSocket-маршрут винесено окремо від CompressionLayer та TimeoutLayer,
    // щоб забезпечити коректний HTTP 101 Upgrade та довготривалу push-сесію.
    let ws_router = Router::new()
        .route("/api/ws", get(api_ws_handler))
        .layer(TraceLayer::new_for_http());

    ws_router.merge(http_router).with_state(state)
}

async fn api_ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state.agent))
}

#[tracing::instrument(skip(socket, agent))]
async fn handle_ws_connection(mut socket: WebSocket, agent: WebAgent) {
    let mut rx = agent.subscribe();
    let initial_snap = agent.snapshot();
    let connected_event =
        AgentUpdateEvent::from_snapshot(AgentUpdateKind::Connected, &initial_snap);

    if send_ws_event(&mut socket, &connected_event).await.is_err() {
        return;
    }

    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;

    loop {
        tokio::select! {
            recv_res = rx.recv() => {
                match recv_res {
                    Ok(event) => {
                        if send_ws_event(&mut socket, &event).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        warn!(skipped, "WebSocket-підписник відстав від каналу подій; надсилається знімок catch-up");
                        let snap = agent.snapshot();
                        let catchup = AgentUpdateEvent::from_snapshot(AgentUpdateKind::StateChanged, &snap);
                        if send_ws_event(&mut socket, &catchup).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            client_msg = socket.recv() => {
                match client_msg {
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
            _ = heartbeat.tick() => {
                if socket.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn send_ws_event(socket: &mut WebSocket, event: &AgentUpdateEvent) -> Result<(), ()> {
    let payload = match serde_json::to_string(event) {
        Ok(payload) => payload,
        Err(err) => {
            warn!(error = %err, "Не вдалося серіалізувати WebSocket-подію в JSON");
            return Ok(());
        }
    };
    socket
        .send(Message::Text(payload.into()))
        .await
        .map_err(|_| ())
}

async fn api_not_found_handler() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "Невідомий маршрут API." })),
    )
}

async fn api_health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn api_state_handler(State(state): State<AppState>) -> Json<Arc<AgentSnapshot>> {
    Json(state.agent.snapshot())
}

async fn api_ui_state_handler(State(state): State<AppState>) -> Json<UiStatePayload> {
    Json(state.ui_state())
}

async fn api_article_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.agent.article_by_id(&id) {
        Some(article) => (StatusCode::OK, Json(article)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Новину з таким ідентифікатором не знайдено."
            })),
        )
            .into_response(),
    }
}

async fn api_poll_handler(State(state): State<AppState>) -> Json<UiStatePayload> {
    state.agent.request_immediate_poll();
    Json(state.ui_state())
}

async fn api_ack_site_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<UiStatePayload> {
    state.agent.acknowledge_site(&id).await;
    Json(state.ui_state())
}

async fn api_unack_site_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<UiStatePayload> {
    state.agent.unacknowledge_site(&id).await;
    Json(state.ui_state())
}

async fn api_enable_site_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<UiStatePayload> {
    state.agent.enable_site(&id).await;
    state.agent.request_immediate_poll();
    Json(state.ui_state())
}

async fn api_enable_feed_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<UiStatePayload> {
    state.agent.enable_feed(&id).await;
    state.agent.request_immediate_poll();
    Json(state.ui_state())
}

async fn api_enable_nbu_handler(State(state): State<AppState>) -> Json<UiStatePayload> {
    state.agent.enable_nbu().await;
    state.agent.request_immediate_poll();
    Json(state.ui_state())
}

async fn api_upsert_fact_handler(
    State(state): State<AppState>,
    Json(form): Json<ManualFactRequest>,
) -> impl IntoResponse {
    let key = form.key.trim().to_string();
    if key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Ключ факту не може бути порожнім." })),
        )
            .into_response();
    }
    let cf = form.cf.unwrap_or(1.0);
    if !cf.is_finite() || !(0.0..=1.0).contains(&cf) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Коефіцієнт впевненості (CF) має бути скінченним числом у межах [0.0, 1.0]."
            })),
        )
            .into_response();
    }
    let value = FactValue::parse_user_input(&form.value);
    state.agent.upsert_manual_fact(key, value, cf).await;
    (StatusCode::OK, Json(state.ui_state())).into_response()
}

async fn api_clear_facts_handler(State(state): State<AppState>) -> Json<UiStatePayload> {
    state.agent.clear_manual_facts().await;
    Json(state.ui_state())
}
