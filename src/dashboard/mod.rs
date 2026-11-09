use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Json,
};
use chrono::Duration;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};

use crate::auth::oauth::OAuthFlow;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

pub async fn oauth_login(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let oauth = OAuthFlow::new(state.config.clone(), state.http_client.clone());

    let oauth_state = OAuthFlow::generate_state();
    let (code_verifier, code_challenge) = OAuthFlow::generate_pkce();

    // Save state to DB
    if let Err(e) = crate::db::save_oauth_state(&state.db, &oauth_state, &code_verifier).await {
        error!("Failed to save OAuth state: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to initiate OAuth flow"})),
        )
            .into_response();
    }

    let auth_url = oauth.authorization_url(&oauth_state, &code_challenge);
    info!("Redirecting to OAuth: {}", auth_url);

    Redirect::temporary(&auth_url).into_response()
}

pub async fn oauth_get_url(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let oauth = OAuthFlow::new(state.config.clone(), state.http_client.clone());

    let oauth_state = OAuthFlow::generate_state();
    let (code_verifier, code_challenge) = OAuthFlow::generate_pkce();

    if let Err(e) = crate::db::save_oauth_state(&state.db, &oauth_state, &code_verifier).await {
        error!("Failed to save OAuth state: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to initiate OAuth flow"})),
        )
            .into_response();
    }

    let auth_url = oauth.authorization_url(&oauth_state, &code_challenge);
    Json(json!({ "url": auth_url, "state": oauth_state })).into_response()
}

pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OAuthCallbackParams>,
) -> impl IntoResponse {
    // Handle OAuth errors
    if let Some(error) = params.error {
        error!("OAuth error: {}", error);
        return Html(format!(
            r#"<html><body><h1>OAuth Error</h1><p>{}</p><a href="/">Back to Dashboard</a></body></html>"#,
            error
        ))
        .into_response();
    }

    let code = match params.code {
        Some(c) => c,
        None => {
            return Html(
                "<html><body><h1>Error</h1><p>No authorization code received</p><a href='/'>Back</a></body></html>".to_string()
            )
            .into_response();
        }
    };

    let oauth_state = match params.state {
        Some(s) => s,
        None => {
            return Html(
                "<html><body><h1>Error</h1><p>No state parameter received</p><a href='/'>Back</a></body></html>".to_string()
            )
            .into_response();
        }
    };

    // Verify state and get code verifier
    let state_record = match crate::db::get_oauth_state(&state.db, &oauth_state).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            error!("OAuth state not found or expired: {}", oauth_state);
            return Html(
                "<html><body><h1>Error</h1><p>OAuth state invalid or expired. Please try again.</p><a href='/'>Back</a></body></html>".to_string()
            )
            .into_response();
        }
        Err(e) => {
            error!("Failed to get OAuth state: {}", e);
            return Html(format!(
                "<html><body><h1>Error</h1><p>Internal error: {}</p><a href='/'>Back</a></body></html>",
                e
            ))
            .into_response();
        }
    };

    // Delete used state
    crate::db::delete_oauth_state(&state.db, &oauth_state).await.ok();

    let oauth = OAuthFlow::new(state.config.clone(), state.http_client.clone());

    // Exchange code for tokens
    let token_resp = match oauth.exchange_code(&code, &state_record.code_verifier).await {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to exchange OAuth code: {}", e);
            return Html(format!(
                "<html><body><h1>Error</h1><p>Token exchange failed: {}</p><a href='/'>Back</a></body></html>",
                e
            ))
            .into_response();
        }
    };

    // Get user info
    let user_info = match oauth.get_user_info(&token_resp.access_token).await {
        Ok(u) => u,
        Err(e) => {
            error!("Failed to get user info: {}", e);
            return Html(format!(
                "<html><body><h1>Error</h1><p>Failed to get user info: {}</p><a href='/'>Back</a></body></html>",
                e
            ))
            .into_response();
        }
    };

    let token_expiry = token_resp
        .expires_in
        .map(|secs| chrono::Utc::now() + Duration::seconds(secs));

    // Save account
    match crate::db::upsert_account(
        &state.db,
        &user_info.email,
        &token_resp.access_token,
        token_resp.refresh_token.as_deref(),
        token_expiry,
    )
    .await
    {
        Ok(account) => {
            state.account_pool.reload_cache().await.ok();
            info!("Successfully added/updated account: {}", account.email);
            let redirect_target = state.config.public_base_url.as_deref().unwrap_or("/");
            Html(format!(
                r#"<!DOCTYPE html>
<html>
<head>
    <title>OAuth Success - ZEN ROUTER</title>
    <style>
        body {{ font-family: system-ui; background: #0f0f1a; color: #e2e8f0; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; }}
        .card {{ background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); border-radius: 16px; padding: 2rem; text-align: center; max-width: 400px; }}
        h1 {{ color: #22c55e; margin-bottom: 0.5rem; }}
        p {{ color: #94a3b8; }}
        a {{ display: inline-block; margin-top: 1rem; padding: 0.75rem 1.5rem; background: linear-gradient(135deg, #667eea, #764ba2); color: white; text-decoration: none; border-radius: 8px; font-weight: 600; }}
    </style>
</head>
<body>
    <div class="card">
        <h1>✅ Success!</h1>
        <p>Account <strong>{}</strong> has been connected to ZEN ROUTER.</p>
        <a href="{}">Go to Dashboard</a>
    </div>
    <script>setTimeout(() => window.location.href = '{}', 2000);</script>
</body>
</html>"#,
                account.email,
                redirect_target,
                redirect_target
            ))
            .into_response()
        }
        Err(e) => {
            error!("Failed to save account: {}", e);
            Html(format!(
                "<html><body><h1>Error</h1><p>Failed to save account: {}</p><a href='/'>Back</a></body></html>",
                e
            ))
            .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ExchangeCodeRequest {
    pub code: Option<String>,
    pub state: Option<String>,
    pub url: Option<String>,
}

/// Exchange OAuth code manually (for remote devices / domain users where localhost redirect is not reachable)
pub async fn oauth_exchange_code(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExchangeCodeRequest>,
) -> impl IntoResponse {
    let (code, oauth_state) = if let Some(ref raw_url) = payload.url {
        if let Ok(parsed) = reqwest::Url::parse(raw_url) {
            let mut extracted_code = None;
            let mut extracted_state = None;
            for (k, v) in parsed.query_pairs() {
                if k == "code" {
                    extracted_code = Some(v.into_owned());
                } else if k == "state" {
                    extracted_state = Some(v.into_owned());
                }
            }
            (extracted_code, extracted_state)
        } else {
            (payload.code, payload.state)
        }
    } else {
        (payload.code, payload.state)
    };

    let code = match code {
        Some(c) if !c.trim().is_empty() => c,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "No authorization code found in payload or URL"})),
            )
                .into_response();
        }
    };

    // If state is given, verify it
    let code_verifier = if let Some(ref st) = oauth_state {
        match crate::db::get_oauth_state(&state.db, st).await {
            Ok(Some(s)) => {
                crate::db::delete_oauth_state(&state.db, st).await.ok();
                s.code_verifier
            }
            _ => {
                match sqlx::query_as::<_, (String,)>("SELECT code_verifier FROM oauth_states ORDER BY created_at DESC LIMIT 1")
                    .fetch_optional(&state.db)
                    .await
                {
                    Ok(Some((cv,))) => cv,
                    _ => "".to_string(),
                }
            }
        }
    } else {
        match sqlx::query_as::<_, (String,)>("SELECT code_verifier FROM oauth_states ORDER BY created_at DESC LIMIT 1")
            .fetch_optional(&state.db)
            .await
        {
            Ok(Some((cv,))) => cv,
            _ => "".to_string(),
        }
    };

    let oauth = OAuthFlow::new(state.config.clone(), state.http_client.clone());

    let token_resp = match oauth.exchange_code(&code, &code_verifier).await {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to exchange manual OAuth code: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Token exchange failed: {}", e)})),
            )
                .into_response();
        }
    };

    let user_info = match oauth.get_user_info(&token_resp.access_token).await {
        Ok(u) => u,
        Err(e) => {
            error!("Failed to get user info: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Failed to get user info: {}", e)})),
            )
                .into_response();
        }
    };

    let token_expiry = token_resp
        .expires_in
        .map(|secs| chrono::Utc::now() + Duration::seconds(secs));

    match crate::db::upsert_account(
        &state.db,
        &user_info.email,
        &token_resp.access_token,
        token_resp.refresh_token.as_deref(),
        token_expiry,
    )
    .await
    {
        Ok(account) => {
            state.account_pool.reload_cache().await.ok();
            info!("Successfully added/updated account via manual code exchange: {}", account.email);
            Json(json!({
                "success": true,
                "email": account.email,
                "message": format!("Akun {} berhasil terhubung ke ZEN ROUTER!", account.email)
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to save account: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to save account: {}", e)})),
            )
                .into_response()
        }
    }
}
