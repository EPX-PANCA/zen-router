mod api;
mod auth;
mod config;
mod dashboard;
mod db;
mod proxy;
mod router;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use config::Config;
use proxy::upstream::UpstreamClient;
use router::account_pool::AccountPool;

pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub config: Config,
    pub account_pool: AccountPool,
    pub upstream: UpstreamClient,
    pub http_client: reqwest::Client,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("zen_router=info".parse()?)
                .add_directive("tower_http=info".parse()?),
        )
        .init();

    let config = Config::from_env()?;
    info!("Starting ZEN ROUTER on port {}", config.port);
    info!("OAuth Client ID: {}", config.google_client_id);
    info!("OAuth Redirect URI: {}", config.redirect_uri);

    // Ensure data directory exists
    if config.database_url.starts_with("sqlite:") {
        let db_path = config.database_url.strip_prefix("sqlite:").unwrap_or("zen-router.db");
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    // Initialize SQLite pool
    let db_url = &config.database_url;
    let connect_options = SqliteConnectOptions::from_str(db_url)?
        .create_if_missing(true)
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL");

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(connect_options)
        .await?;

    // Run database migrations
    db::init_db(&pool).await?;
    info!("Database initialized");

    // Anti-Ban: Periodic health rehabilitation background task (runs every 60s)
    let rehab_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = db::rehabilitate_accounts(&rehab_pool).await {
                tracing::warn!("[Anti-Ban] Account rehabilitation error: {}", e);
            }
        }
    });

    // Build HTTP client
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Build OAuth flow
    let oauth_flow = Arc::new(auth::oauth::OAuthFlow::new(
        config.clone(),
        http_client.clone(),
    ));

    // Build account pool
    let account_pool = AccountPool::new(pool.clone(), oauth_flow);

    // Build upstream client
    let upstream = UpstreamClient::new(config.clone());

    let state = Arc::new(AppState {
        db: pool,
        config: config.clone(),
        account_pool,
        upstream,
        http_client,
    });

    // CORS middleware
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build routes
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Dashboard
        .route("/", get(serve_dashboard))
        .route("/dashboard", get(serve_dashboard))
        // Static files
        .route("/static/style.css", get(serve_css))
        .route("/static/app.js", get(serve_js))
        // OpenAI-compatible endpoints
        .route("/v1/chat/completions", post(api::chat::openai_chat_completions))
        .route("/v1/models", get(api::stats::get_models))
        // Anthropic-compatible endpoints
        .route("/v1/messages", post(api::chat::anthropic_messages))
        // OAuth endpoints
        .route("/api/oauth/login", get(dashboard::oauth_login))
        .route("/api/oauth/url", get(dashboard::oauth_get_url))
        .route("/api/oauth/callback", get(dashboard::oauth_callback))
        .route("/api/oauth/exchange-code", post(dashboard::oauth_exchange_code))
        // Account management
        .route("/api/accounts", get(api::accounts::list_accounts).post(api::accounts::create_account))
        .route("/api/accounts/:id", delete(api::accounts::delete_account))
        .route("/api/accounts/:id/toggle", post(api::accounts::toggle_account))
        // API Key management
        .route("/api/keys", get(api::keys::list_keys).post(api::keys::create_key))
        .route("/api/keys/:id", delete(api::keys::delete_key))
        // Stats & Quota
        .route("/api/stats", get(api::stats::get_stats))
        .route("/api/logs", get(api::stats::get_logs))
        .route("/api/quota", get(api::stats::get_quota))
        .route("/api/debug/token", get(debug_token))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("ZEN ROUTER listening on 0.0.0.0:{}", config.port);
    info!("Local Dashboard: http://localhost:{}/", config.port);
    if let Some(ref pub_url) = config.public_base_url {
        info!("Public Domain:   {}", pub_url);
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "zen-router",
        "version": "0.1.0"
    }))
}

async fn serve_dashboard(
    State(state): State<Arc<AppState>>,
    Query(params): Query<dashboard::OAuthCallbackParams>,
) -> impl IntoResponse {
    if params.code.is_some() || params.error.is_some() {
        return dashboard::oauth_callback(State(state), Query(params))
            .await
            .into_response();
    }
    let html = include_str!("../static/index.html");
    Html(html).into_response()
}

async fn serve_css() -> impl IntoResponse {
    let css = include_str!("../static/style.css");
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        css,
    )
}

async fn serve_js() -> impl IntoResponse {
    let js = include_str!("../static/app.js");
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript")],
        js,
    )
}

async fn debug_token(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    if let Ok(Some((acc, token))) = state.account_pool.get_next_account().await {
        let client = reqwest::Client::new();
        let res = client
            .get(format!("https://www.googleapis.com/oauth2/v1/tokeninfo?access_token={}", token))
            .send()
            .await;
        if let Ok(r) = res {
            let j: serde_json::Value = r.json().await.unwrap_or_default();
            return Json(json!({"email": acc.email, "tokeninfo": j}));
        }
    }
    Json(json!({"error": "no account"}))
}
