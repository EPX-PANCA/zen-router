use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::Instant;
use tracing::{error, info, warn};

use crate::proxy::transform::{
    anthropic_to_turns, build_google_payload, extract_delta_from_google_chunk,
    extract_text_from_google_response, google_chunk_to_anthropic_sse, google_chunk_to_openai_sse,
    google_to_anthropic_response, google_to_openai_response, openai_to_turns, AnthropicRequest,
    OpenAiRequest,
};
use crate::AppState;

async fn verify_api_key(state: &Arc<AppState>, headers: &HeaderMap) -> Result<(), Response> {
    // If no keys exist in the system yet, allow open access
    let has_keys = crate::db::has_api_keys(&state.db).await.unwrap_or(false);
    if !has_keys {
        return Ok(());
    }

    // Extract Bearer token from Authorization or x-api-key header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .or_else(|| headers.get("x-api-key").and_then(|h| h.to_str().ok()));

    let token = match auth_header {
        Some(h) if h.to_lowercase().starts_with("bearer ") => h[7..].trim(),
        Some(h) => h.trim(),
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": {
                        "message": "Missing API key in Authorization header. Please pass 'Authorization: Bearer <your_key>' generated from ZEN ROUTER dashboard (/api/keys).",
                        "type": "authentication_error"
                    }
                })),
            )
                .into_response());
        }
    };

    let is_valid = crate::db::validate_and_touch_api_key(&state.db, token)
        .await
        .unwrap_or(false);
    if !is_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "message": "Invalid API key. Please generate a valid key from ZEN ROUTER dashboard (/api/keys).",
                    "type": "authentication_error"
                }
            })),
        )
            .into_response());
    }

    Ok(())
}

/// Handle OpenAI-compatible chat completions endpoint
pub async fn openai_chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<OpenAiRequest>,
) -> Response {
    if let Err(resp) = verify_api_key(&state, &headers).await {
        return resp;
    }

    let start = Instant::now();
    let model = req.model.clone();
    let stream = req.stream.unwrap_or(false);

    let (system_prompt, turns) = openai_to_turns(&req);
    let (backend_model, google_payload) = build_google_payload(
        &state.config.project_id,
        &model,
        system_prompt.as_deref(),
        &turns,
        req.temperature,
        req.max_tokens,
    );

    proxy_request(state, google_payload, &backend_model, &model, stream, true, start).await
}

/// Handle Anthropic-compatible messages endpoint
pub async fn anthropic_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AnthropicRequest>,
) -> Response {
    if let Err(resp) = verify_api_key(&state, &headers).await {
        return resp;
    }

    let start = Instant::now();
    let model = req.model.clone();
    let stream = req.stream.unwrap_or(false);

    let (system_prompt, turns) = anthropic_to_turns(&req);
    let (backend_model, google_payload) = build_google_payload(
        &state.config.project_id,
        &model,
        system_prompt.as_deref(),
        &turns,
        req.temperature,
        Some(req.max_tokens),
    );

    proxy_request(state, google_payload, &backend_model, &model, stream, false, start).await
}

fn estimate_prompt_tokens(google_payload: &Value) -> i64 {
    let mut total_chars = 0usize;
    if let Some(contents) = google_payload
        .get("request")
        .and_then(|r| r.get("contents"))
        .and_then(|c| c.as_array())
    {
        for content in contents {
            if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        total_chars += text.len();
                    }
                }
            }
        }
    }
    ((total_chars / 4) + 1).max(1) as i64
}

fn extract_usage_tokens(val: &Value) -> Option<(i64, i64)> {
    let usage = val
        .get("usageMetadata")
        .or_else(|| val.get("response").and_then(|r| r.get("usageMetadata")));

    let meta = usage?;
    let prompt = meta.get("promptTokenCount").and_then(|t| t.as_i64()).unwrap_or(0);
    let candidates = meta.get("candidatesTokenCount").and_then(|t| t.as_i64()).unwrap_or(0);
    if prompt > 0 || candidates > 0 {
        Some((prompt, candidates))
    } else {
        None
    }
}

async fn proxy_request(
    state: Arc<AppState>,
    google_payload: Value,
    backend_model: &str,
    original_model: &str,
    stream: bool,
    openai_format: bool,
    start: Instant,
) -> Response {
    // Get account from pool
    let (account, access_token) = match state.account_pool.get_next_account().await {
        Ok(Some(acc)) => acc,
        Ok(None) => {
            error!("No active accounts available");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": {
                        "message": "No active accounts available. Please add a Google account via OAuth.",
                        "type": "service_unavailable"
                    }
                })),
            )
                .into_response();
        }
        Err(e) => {
            error!("Error getting account: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": e.to_string(), "type": "internal_error"}})),
            )
                .into_response();
        }
    };

    info!(
        "Routing request: model={} (backend={}) via account {}",
        original_model, backend_model, account.email
    );

    if stream {
        handle_streaming(
            state,
            account.id.clone(),
            backend_model.to_string(),
            original_model.to_string(),
            google_payload,
            access_token,
            openai_format,
            start,
        )
        .await
    } else {
        handle_non_streaming(
            state,
            account.id.clone(),
            backend_model.to_string(),
            original_model.to_string(),
            google_payload,
            access_token,
            openai_format,
            start,
        )
        .await
    }
}

async fn handle_non_streaming(
    state: Arc<AppState>,
    account_id: String,
    backend_model: String,
    original_model: String,
    google_payload: Value,
    access_token: String,
    openai_format: bool,
    start: Instant,
) -> Response {
    let result = state
        .upstream
        .send_request(&google_payload, &access_token)
        .await;

    let latency_ms = start.elapsed().as_millis() as i64;

    match result {
        Ok(resp) => {
            let estimated_prompt = estimate_prompt_tokens(&google_payload);
            let (mut prompt_tokens, mut candidates_tokens) = extract_usage_tokens(&resp.body)
                .unwrap_or((estimated_prompt, 0));

            if prompt_tokens == 0 {
                prompt_tokens = estimated_prompt;
            }
            if candidates_tokens == 0 {
                let (text, _, _) = extract_text_from_google_response(&resp.body);
                candidates_tokens = ((text.len() / 4) + 1).max(1) as i64;
            }

            state.account_pool.record_success(&account_id).await.ok();
            crate::db::record_request(
                &state.db,
                Some(&account_id),
                &original_model,
                prompt_tokens,
                candidates_tokens,
                latency_ms,
                "success",
                None,
            )
            .await
            .ok();

            if openai_format {
                let openai_resp = google_to_openai_response(&resp.body, &original_model);
                Json(openai_resp).into_response()
            } else {
                let anthropic_resp = google_to_anthropic_response(&resp.body, &original_model);
                Json(anthropic_resp).into_response()
            }
        }
        Err(e) => {
            let error_msg = e.to_string();

            if error_msg.starts_with("RATE_LIMIT:") {
                warn!("Rate limit hit, rotating account");
                match state.account_pool.handle_rate_limit(&account_id).await {
                    Ok(Some((alt_account, alt_token))) => {
                        return Box::pin(handle_non_streaming(
                            state,
                            alt_account.id,
                            backend_model,
                            original_model,
                            google_payload,
                            alt_token,
                            openai_format,
                            start,
                        ))
                        .await;
                    }
                    _ => {}
                }
                let detail = error_msg.trim_start_matches("RATE_LIMIT:429:").trim();
                let msg = if detail.is_empty() {
                    "Rate limit exceeded on all accounts".to_string()
                } else {
                    format!("Rate limit exceeded: {}", detail)
                };
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": {"message": msg, "type": "rate_limit_error"}})),
                )
                    .into_response();
            }

            state.account_pool.record_error(&account_id).await.ok();
            crate::db::record_request(
                &state.db,
                Some(&account_id),
                &original_model,
                0,
                0,
                latency_ms,
                "error",
                Some(&error_msg),
            )
            .await
            .ok();

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": error_msg, "type": "upstream_error"}})),
            )
                .into_response()
        }
    }
}

async fn handle_streaming(
    state: Arc<AppState>,
    account_id: String,
    backend_model: String,
    original_model: String,
    google_payload: Value,
    access_token: String,
    openai_format: bool,
    start: Instant,
) -> Response {
    let result = state
        .upstream
        .send_streaming_request(&google_payload, &access_token)
        .await;

    match result {
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.starts_with("RATE_LIMIT:") {
                match state.account_pool.handle_rate_limit(&account_id).await {
                    Ok(Some((alt_account, alt_token))) => {
                        return Box::pin(handle_streaming(
                            state,
                            alt_account.id,
                            backend_model,
                            original_model,
                            google_payload,
                            alt_token,
                            openai_format,
                            start,
                        ))
                        .await;
                    }
                    _ => {}
                }
                let detail = error_msg.trim_start_matches("RATE_LIMIT:429:").trim();
                let msg = if detail.is_empty() {
                    "Rate limit exceeded on all accounts".to_string()
                } else {
                    format!("Rate limit exceeded: {}", detail)
                };
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": {"message": msg, "type": "rate_limit_error"}})),
                )
                    .into_response();
            }

            state.account_pool.record_error(&account_id).await.ok();
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": error_msg}})),
            )
                .into_response()
        }
        Ok((_status, byte_stream)) => {
            let estimated_prompt = estimate_prompt_tokens(&google_payload);
            let model_clone = original_model.clone();
            let state_clone = state.clone();
            let account_id_clone = account_id.clone();

            let sse_stream = async_stream::stream! {
                let mut buffer = String::new();
                let msg_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
                let mut prompt_tokens = 0i64;
                let mut candidate_tokens = 0i64;
                let mut streamed_chars = 0usize;

                futures::pin_mut!(byte_stream);

                if !openai_format {
                    // Anthropic message_start
                    let start_event = json!({
                        "type": "message_start",
                        "message": {
                            "id": format!("msg_{}", uuid::Uuid::new_v4()),
                            "type": "message",
                            "role": "assistant",
                            "content": [],
                            "model": model_clone,
                            "usage": {"input_tokens": 0, "output_tokens": 0}
                        }
                    });
                    yield Ok::<_, std::convert::Infallible>(format!(
                        "event: message_start\ndata: {}\n\n",
                        serde_json::to_string(&start_event).unwrap()
                    ));
                    yield Ok::<_, std::convert::Infallible>(format!(
                        "event: content_block_start\ndata: {}\n\n",
                        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})
                    ));
                }

                while let Some(chunk_result) = byte_stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            buffer.push_str(&String::from_utf8_lossy(&bytes));

                            while let Some((event_end, delimiter_len)) = {
                                if let Some(pos) = buffer.find("\r\n\r\n") {
                                    Some((pos, 4))
                                } else {
                                    buffer.find("\n\n").map(|pos| (pos, 2))
                                }
                            } {
                                let event_str = buffer[..event_end].to_string();
                                buffer = buffer[event_end + delimiter_len..].to_string();

                                for line in event_str.lines() {
                                    let trimmed = line.trim();
                                    if let Some(rest) = trimmed.strip_prefix("data:") {
                                        let rest = rest.trim();
                                        if rest.is_empty() || rest == "[DONE]" {
                                            continue;
                                        }

                                        if let Ok(val) = serde_json::from_str::<Value>(rest) {
                                            if let Some((p, c)) = extract_usage_tokens(&val) {
                                                if p > 0 { prompt_tokens = p; }
                                                if c > 0 { candidate_tokens = c; }
                                            }

                                            if let Some(delta) = extract_delta_from_google_chunk(&val) {
                                                if !delta.is_empty() {
                                                    streamed_chars += delta.len();
                                                    if openai_format {
                                                        let sse_line = google_chunk_to_openai_sse(&delta, &model_clone, &msg_id);
                                                        yield Ok::<_, std::convert::Infallible>(sse_line);
                                                    } else {
                                                        let sse_line = google_chunk_to_anthropic_sse(&delta);
                                                        yield Ok::<_, std::convert::Infallible>(sse_line);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Stream error: {}", e);
                            break;
                        }
                    }
                }

                if !buffer.trim().is_empty() {
                    for line in buffer.lines() {
                        let trimmed = line.trim();
                        if let Some(rest) = trimmed.strip_prefix("data:") {
                            let rest = rest.trim();
                            if !rest.is_empty() && rest != "[DONE]" {
                                if let Ok(val) = serde_json::from_str::<Value>(rest) {
                                    if let Some((p, c)) = extract_usage_tokens(&val) {
                                        if p > 0 { prompt_tokens = p; }
                                        if c > 0 { candidate_tokens = c; }
                                    }
                                    if let Some(delta) = extract_delta_from_google_chunk(&val) {
                                        if !delta.is_empty() {
                                            streamed_chars += delta.len();
                                            if openai_format {
                                                let sse_line = google_chunk_to_openai_sse(&delta, &model_clone, &msg_id);
                                                yield Ok::<_, std::convert::Infallible>(sse_line);
                                            } else {
                                                let sse_line = google_chunk_to_anthropic_sse(&delta);
                                                yield Ok::<_, std::convert::Infallible>(sse_line);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if prompt_tokens == 0 {
                    prompt_tokens = estimated_prompt;
                }
                if candidate_tokens == 0 {
                    candidate_tokens = ((streamed_chars / 4) + 1).max(1) as i64;
                }

                if openai_format {
                    yield Ok::<_, std::convert::Infallible>("data: [DONE]\n\n".to_string());
                } else {
                    yield Ok::<_, std::convert::Infallible>(format!(
                        "event: content_block_stop\ndata: {}\n\n",
                        json!({"type": "content_block_stop", "index": 0})
                    ));
                    yield Ok::<_, std::convert::Infallible>(format!(
                        "event: message_delta\ndata: {}\n\n",
                        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": candidate_tokens}})
                    ));
                    yield Ok::<_, std::convert::Infallible>(format!(
                        "event: message_stop\ndata: {}\n\n",
                        json!({"type": "message_stop"})
                    ));
                }

                let latency_ms = start.elapsed().as_millis() as i64;
                state_clone.account_pool.record_success(&account_id_clone).await.ok();
                crate::db::record_request(
                    &state_clone.db,
                    Some(&account_id_clone),
                    &model_clone,
                    prompt_tokens,
                    candidate_tokens,
                    latency_ms,
                    "success",
                    None,
                )
                .await
                .ok();
            };

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(axum::body::Body::from_stream(sse_stream))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}
