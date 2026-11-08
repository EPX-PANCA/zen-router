use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::error;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub name: Option<String>,
}

pub async fn list_keys(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match crate::db::list_api_keys(&state.db).await {
        Ok(keys) => Json(json!({ "keys": keys, "total": keys.len() })).into_response(),
        Err(e) => {
            error!("Failed to list API keys: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

pub async fn create_key(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateKeyRequest>,
) -> impl IntoResponse {
    let name = payload.name.unwrap_or_else(|| "Default Key".to_string());
    let clean_name = if name.trim().is_empty() {
        "Default Key".to_string()
    } else {
        name.trim().to_string()
    };

    match crate::db::create_api_key(&state.db, &clean_name).await {
        Ok(key) => (
            StatusCode::CREATED,
            Json(json!({
                "success": true,
                "message": "API key generated successfully",
                "key": key
            })),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to create API key: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

pub async fn delete_key(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match crate::db::delete_api_key(&state.db, &id).await {
        Ok(_) => Json(json!({ "success": true, "message": "API key revoked" })).into_response(),
        Err(e) => {
            error!("Failed to delete API key {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

