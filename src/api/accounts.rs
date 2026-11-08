use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::error;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub email: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: Option<i64>,
}

pub async fn create_account(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateAccountRequest>,
) -> impl IntoResponse {
    if payload.email.trim().is_empty() || payload.access_token.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "email and access_token are required"})),
        )
            .into_response();
    }

    let token_expiry = payload
        .expires_in_seconds
        .map(|s| chrono::Utc::now() + chrono::Duration::seconds(s))
        .or_else(|| Some(chrono::Utc::now() + chrono::Duration::hours(1)));

    match crate::db::upsert_account(
        &state.db,
        &payload.email,
        &payload.access_token,
        payload.refresh_token.as_deref(),
        token_expiry,
    )
    .await
    {
        Ok(account) => (
            StatusCode::CREATED,
            Json(json!({
                "success": true,
                "message": "Account created or updated successfully",
                "account": {
                    "id": account.id,
                    "email": account.email,
                    "is_active": account.is_active,
                    "health_score": account.health_score,
                }
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountResponse {
    pub id: String,
    pub email: String,
    pub is_active: bool,
    pub request_count: i64,
    pub error_count: i64,
    pub health_score: f64,
    pub last_used: Option<String>,
    pub created_at: String,
    pub token_expires_in: Option<i64>,
    pub cooldown_remaining_seconds: Option<i64>,
}

pub async fn list_accounts(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match crate::db::get_all_accounts(&state.db).await {
        Ok(accounts) => {
            let now = chrono::Utc::now();
            let responses: Vec<AccountResponse> = accounts
                .iter()
                .map(|a| {
                    let cooldown_remaining = a.cooldown_until.and_then(|until| {
                        let secs = (until - now).num_seconds();
                        if secs > 0 {
                            Some(secs)
                        } else {
                            None
                        }
                    });

                    AccountResponse {
                        id: a.id.clone(),
                        email: a.email.clone(),
                        is_active: a.is_active,
                        request_count: a.request_count,
                        error_count: a.error_count,
                        health_score: a.health_score,
                        last_used: a.last_used.map(|d| d.to_rfc3339()),
                        created_at: a.created_at.to_rfc3339(),
                        token_expires_in: a.token_expiry.map(|exp| (exp - now).num_seconds()),
                        cooldown_remaining_seconds: cooldown_remaining,
                    }
                })
                .collect();

            Json(json!({ "accounts": responses, "total": responses.len() })).into_response()
        }
        Err(e) => {
            error!("Failed to list accounts: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn delete_account(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> impl IntoResponse {
    match crate::db::delete_account(&state.db, &account_id).await {
        Ok(_) => Json(json!({"success": true, "message": "Account deleted"})).into_response(),
        Err(e) => {
            error!("Failed to delete account {}: {}", account_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

pub async fn toggle_account(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> impl IntoResponse {
    // Get current state
    match crate::db::get_all_accounts(&state.db).await {
        Ok(accounts) => {
            if let Some(account) = accounts.iter().find(|a| a.id == account_id) {
                let new_state = !account.is_active;
                let result = sqlx::query(
                    "UPDATE accounts SET is_active = ? WHERE id = ?"
                )
                .bind(new_state as i64)
                .bind(&account_id)
                .execute(&state.db)
                .await;

                match result {
                    Ok(_) => Json(json!({
                        "success": true,
                        "is_active": new_state,
                        "message": if new_state { "Account activated" } else { "Account deactivated" }
                    }))
                    .into_response(),
                    Err(e) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": e.to_string()})),
                    )
                        .into_response(),
                }
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "Account not found"})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
