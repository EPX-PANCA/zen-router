use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use tracing::error;

use crate::AppState;

pub async fn get_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match crate::db::get_stats(&state.db).await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => {
            error!("Failed to get stats: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn get_logs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match crate::db::get_recent_logs(&state.db, 100).await {
        Ok(logs) => Json(json!({ "logs": logs })).into_response(),
        Err(e) => {
            error!("Failed to get logs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn get_models(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    let models = vec![
        ("gemini-3.8-flash-high", "google-antigravity"),
        ("gemini-3.8-flash-medium", "google-antigravity"),
        ("gemini-3.8-flash-low", "google-antigravity"),
        ("gemini-3.8-flash", "google-antigravity"),
        ("gemini-3.8-flash-tiered", "google-antigravity"),
        ("gemini-3-flash", "google-antigravity"),
        ("gemini-3.5-flash-lite", "google-antigravity"),
        ("gemini-3.1-flash-lite", "google-antigravity"),
        ("gemini-2.5-flash", "google-antigravity"),
        ("gemini-2.5-pro", "google-antigravity"),
        ("claude-sonnet-4-6", "anthropic-antigravity"),
        ("claude-opus-4-6-thinking", "anthropic-antigravity"),
        ("gpt-oss-120b-medium", "openai-antigravity"),
        ("gpt-4o", "openai-compatible"),
        ("gpt-4o-mini", "openai-compatible"),
        ("claude-3-5-sonnet-20241022", "anthropic-compatible"),
        ("claude-3-5-haiku-20241022", "anthropic-compatible"),
    ];

    let data: Vec<serde_json::Value> = models
        .into_iter()
        .map(|(id, owner)| {
            json!({
                "id": id,
                "object": "model",
                "created": 1700000000,
                "owned_by": owner,
                "permission": [],
                "root": id,
                "parent": null
            })
        })
        .collect();

    Json(json!({
        "object": "list",
        "data": data
    }))
    .into_response()
}

pub async fn get_quota(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let (account, token) = match state.account_pool.get_next_account().await {
        Ok(Some(pair)) => pair,
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "No active account available to check quota"})),
            )
                .into_response();
        }
    };

    let client = &state.http_client;
    let url = format!("{}/v1internal:fetchAvailableModels", state.config.upstream_base_url);

    let resp = match client
        .post(&url)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .header("User-Agent", "antigravity/1.104.0 darwin/arm64")
        .header("X-Goog-Api-Client", "gl-rust/1.0")
        .json(&json!({}))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Failed to fetch quota from upstream: {}", e)})),
            )
                .into_response();
        }
    };

    if !resp.status().is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Upstream quota error: {}", err_text)})),
        )
            .into_response();
    }

    let upstream_data: serde_json::Value = match resp.json().await {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Failed to parse upstream quota json: {}", e)})),
            )
                .into_response();
        }
    };

    let raw_models = upstream_data.get("models").and_then(|m| m.as_object());

    // Extract default Gemini quota
    let gemini_quota = raw_models
        .and_then(|m| m.get("gemini-2.5-flash").or_else(|| m.get("gemini-3.8-flash-tiered")))
        .and_then(|m| m.get("quotaInfo"));

    let remaining_fraction = gemini_quota
        .and_then(|q| q.get("remainingFraction"))
        .and_then(|f| f.as_f64())
        .unwrap_or(1.0);
    let reset_time = gemini_quota
        .and_then(|q| q.get("resetTime"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let claude_reset_time = raw_models
        .and_then(|m| m.get("claude-sonnet-4-6"))
        .and_then(|m| m.get("quotaInfo"))
        .and_then(|q| q.get("resetTime"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let remaining_percent = (remaining_fraction * 1000.0).round() / 10.0;

    Json(json!({
        "success": true,
        "account": account.email,
        "remaining_fraction": remaining_fraction,
        "remaining_percent": remaining_percent,
        "reset_time": reset_time,
        "claude_reset_time": claude_reset_time,
        "models": [
            {
                "id": "gemini-2.5-flash",
                "name": "Gemini 2.5 Flash",
                "speed": "⚡ Ultra Fast (~650ms)",
                "thinking": "None",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": true
            },
            {
                "id": "gemini-3.8-flash-low",
                "name": "Gemini 3.8 Flash (Low Thinking)",
                "speed": "🚀 Fast (~1.5s)",
                "thinking": "LOW",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": false
            },
            {
                "id": "gemini-3.8-flash-medium",
                "name": "Gemini 3.8 Flash (Medium Thinking)",
                "speed": "⚖️ Balanced (~2.0s)",
                "thinking": "MEDIUM",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": false
            },
            {
                "id": "gemini-3.8-flash-high",
                "name": "Gemini 3.8 Flash (High Thinking)",
                "speed": "🧠 Deep Reasoning (~2.4s)",
                "thinking": "HIGH",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": false
            },
            {
                "id": "gemini-2.5-pro",
                "name": "Gemini 2.5 Pro",
                "speed": "🌟 High Capability (~1.2s)",
                "thinking": "None",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": false
            },
            {
                "id": "gemini-3.5-flash-lite",
                "name": "Gemini 3.5 Flash Lite",
                "speed": "⚡ Fast Lite (~800ms)",
                "thinking": "None",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": false
            },
            {
                "id": "gpt-4o",
                "name": "GPT-4o (OpenAI Compatible Alias)",
                "speed": "🧠 Deep Reasoning (~2.4s)",
                "thinking": "HIGH",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": false
            },
            {
                "id": "claude-3-5-sonnet-20241022",
                "name": "Claude 3.5 Sonnet (Anthropic Alias)",
                "speed": "🧠 Deep Reasoning (~2.4s)",
                "thinking": "HIGH",
                "remaining_percent": remaining_percent,
                "reset_time": reset_time,
                "available": true,
                "is_recommended": false
            }
        ]
    }))
    .into_response()
}
