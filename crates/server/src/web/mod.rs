pub mod templates;

use std::time::Duration;

use axum::error_handling::HandleErrorLayer;
use axum::extract::{
    Path,
    State,
};
use axum::http::StatusCode;
use axum::response::{
    IntoResponse,
    Redirect,
};
use axum::routing::{
    get,
    post,
};
use axum::{
    Form,
    Json,
    Router,
};
use dss::FactValue;
use serde::Deserialize;
use tower::limit::ConcurrencyLimitLayer;
use tower::timeout::TimeoutLayer;
use tower::{
    BoxError,
    ServiceBuilder,
};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use web_agent::{
    AgentSnapshot,
    WebAgent,
};

use crate::web::templates::{
    ArticleTemplate,
    ConfTemplate,
    DssTemplate,
    HtmlTemplate,
    IndexTemplate,
    NotFoundTemplate,
};

pub fn build_router(agent: WebAgent) -> Router {
    let middleware = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|err: BoxError| async move {
            (
                StatusCode::REQUEST_TIMEOUT,
                format!("Сервіс перевантажений або перевищено час очікування: {err}"),
            )
        }))
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(ConcurrencyLimitLayer::new(64))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    Router::new()
        .route("/", get(index_handler))
        .route("/index.html", get(index_handler))
        .route("/conf.html", get(conf_handler))
        .route("/dss.html", get(dss_handler))
        .route("/article/{id}", get(article_handler))
        .route("/api/state", get(api_state_handler))
        .route("/poll", post(trigger_poll_handler))
        .route("/sites/{id}/ack", post(ack_site_handler))
        .route("/sites/{id}/unack", post(unack_site_handler))
        .route("/sites/{id}/enable", post(enable_site_handler))
        .route("/feeds/{id}/enable", post(enable_feed_handler))
        .route("/nbu/enable", post(enable_nbu_handler))
        .route("/dss/fact", post(upsert_fact_handler))
        .route("/dss/fact/clear", post(clear_facts_handler))
        .layer(middleware)
        .with_state(agent)
}

async fn index_handler(State(agent): State<WebAgent>) -> impl IntoResponse {
    let snap = agent.snapshot().await;
    HtmlTemplate(IndexTemplate::from_snapshot(&snap))
}

async fn conf_handler(State(agent): State<WebAgent>) -> impl IntoResponse {
    HtmlTemplate(ConfTemplate::from_config(agent.config()))
}

async fn dss_handler(State(agent): State<WebAgent>) -> impl IntoResponse {
    let snap = agent.snapshot().await;
    HtmlTemplate(DssTemplate::from_snapshot(&snap, agent.engine().rules()))
}

async fn article_handler(
    State(agent): State<WebAgent>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let snap = agent.snapshot().await;
    match snap.articles.iter().find(|a| a.id == id) {
        Some(article) => (
            StatusCode::OK,
            HtmlTemplate(ArticleTemplate::from_article(article)),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            HtmlTemplate(NotFoundTemplate {
                active_nav: "index",
                message: "Новину з таким ідентифікатором не знайдено.".to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_state_handler(State(agent): State<WebAgent>) -> Json<AgentSnapshot> {
    Json(agent.snapshot().await)
}

async fn trigger_poll_handler(State(agent): State<WebAgent>) -> Redirect {
    agent.request_immediate_poll();
    Redirect::to("/index.html")
}

async fn ack_site_handler(State(agent): State<WebAgent>, Path(id): Path<String>) -> Redirect {
    agent.acknowledge_site(&id).await;
    Redirect::to("/index.html")
}

async fn unack_site_handler(State(agent): State<WebAgent>, Path(id): Path<String>) -> Redirect {
    agent.unacknowledge_site(&id).await;
    Redirect::to("/index.html")
}

async fn enable_site_handler(State(agent): State<WebAgent>, Path(id): Path<String>) -> Redirect {
    agent.enable_site(&id).await;
    agent.request_immediate_poll();
    Redirect::to("/index.html")
}

async fn enable_feed_handler(State(agent): State<WebAgent>, Path(id): Path<String>) -> Redirect {
    agent.enable_feed(&id).await;
    agent.request_immediate_poll();
    Redirect::to("/index.html")
}

async fn enable_nbu_handler(State(agent): State<WebAgent>) -> Redirect {
    agent.enable_nbu().await;
    agent.request_immediate_poll();
    Redirect::to("/index.html")
}

#[derive(Debug, Deserialize)]
struct ManualFactForm {
    key: String,
    value: String,
    cf: Option<f64>,
}

async fn upsert_fact_handler(
    State(agent): State<WebAgent>,
    Form(form): Form<ManualFactForm>,
) -> Redirect {
    let key = form.key.trim().to_string();
    if !key.is_empty() {
        let value = FactValue::parse_user_input(&form.value);
        let cf = form.cf.unwrap_or(1.0);
        agent.upsert_manual_fact(key, value, cf).await;
    }
    Redirect::to("/dss.html")
}

async fn clear_facts_handler(State(agent): State<WebAgent>) -> Redirect {
    agent.clear_manual_facts().await;
    Redirect::to("/dss.html")
}
