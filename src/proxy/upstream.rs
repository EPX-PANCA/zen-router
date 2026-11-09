use anyhow::Result;
use bytes::Bytes;
use futures::Stream;
use rand::Rng;
use reqwest::Client;
use serde_json::Value;
use std::pin::Pin;
use tracing::{debug, warn};

use crate::config::Config;

pub struct UpstreamClient {
    client: Client,
    config: Config,
}

pub struct UpstreamResponse {
    pub status: u16,
    pub body: Value,
}

/// Anti-Ban: Subtly jitter request timings (5-20ms) to break microsecond machine burst patterns without affecting latency
async fn apply_anti_burst_jitter() {
    if std::env::var("DISABLE_JITTER").map(|v| v == "1" || v == "true").unwrap_or(false) {
        return;
    }
    let jitter_ms = rand::thread_rng().gen_range(5..=20);
    tokio::time::sleep(std::time::Duration::from_millis(jitter_ms)).await;
}

/// Anti-Ban: Generate realistic Google Antigravity extension client headers
/// Replaces `gl-rust/1.0` with authentic Electron/Node IDE client signatures
fn get_antiban_headers() -> (String, String, String, String) {
    let session_id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let user_agent = "antigravity/1.104.0 darwin/arm64".to_string();
    let api_client = "gl-node/20.18.0 gccl/1.104.0".to_string();
    (user_agent, api_client, session_id, request_id)
}

impl UpstreamClient {
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(15))
            // Connection Pooling & Persistent Warm Sockets
            .pool_max_idle_per_host(32)
            .pool_idle_timeout(Some(std::time::Duration::from_secs(120)))
            .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
            .tcp_nodelay(true)
            // HTTP/2 Multiplexing & Warm Keep-Alive
            .http2_adaptive_window(true)
            .http2_keep_alive_interval(Some(std::time::Duration::from_secs(20)))
            .http2_keep_alive_timeout(std::time::Duration::from_secs(10))
            .http2_keep_alive_while_idle(true)
            .build()
            .expect("Failed to build HTTP client");

        Self { client, config }
    }

    /// Send a non-streaming request to the upstream API with Anti-Ban protection
    pub async fn send_request(
        &self,
        body: &Value,
        access_token: &str,
    ) -> Result<UpstreamResponse> {
        let url = self.config.upstream_url(false);
        debug!("Sending request to upstream: {}", url);

        // Anti-ban: smooth out rapid-fire bursts
        apply_anti_burst_jitter().await;

        let (user_agent, api_client, session_id, request_id) = get_antiban_headers();

        let response = self
            .client
            .post(&url)
            .bearer_auth(access_token)
            .header("Content-Type", "application/json")
            .header("User-Agent", user_agent)
            .header("X-Goog-Api-Client", api_client)
            .header("X-Client-Session-Id", session_id)
            .header("X-Request-Id", request_id)
            .header("X-Client-Version", "1.104.0")
            .json(body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if status == 429 {
            let error_body = response.text().await.unwrap_or_default();
            warn!("[Anti-Ban] Upstream rate limit (429): {}", error_body);
            return Err(anyhow::anyhow!("RATE_LIMIT:429: {}", error_body));
        }

        if status == 401 {
            let error_body = response.text().await.unwrap_or_default();
            warn!("[Auth Error] Upstream 401 Unauthorized: {}", error_body);
            return Err(anyhow::anyhow!("AUTH_ERROR:401: {}", error_body));
        }

        if (500..=599).contains(&status) {
            let error_body = response.text().await.unwrap_or_default();
            warn!("[Server Error] Upstream {}: {}", status, error_body);
            return Err(anyhow::anyhow!("SERVER_ERROR:{}: {}", status, error_body));
        }

        if !response.status().is_success() {
            let error_body = response.text().await.unwrap_or_default();
            warn!("Upstream error {}: {}", status, error_body);
            return Err(anyhow::anyhow!("Upstream error {}: {}", status, error_body));
        }

        let body: Value = response.json().await?;
        Ok(UpstreamResponse { status, body })
    }

    /// Send a streaming request with Anti-Ban protection
    pub async fn send_streaming_request(
        &self,
        body: &Value,
        access_token: &str,
    ) -> Result<(u16, Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>)> {
        let url = self.config.upstream_url(true);
        debug!("Sending streaming request to upstream: {}", url);

        // Anti-ban: smooth out rapid-fire bursts
        apply_anti_burst_jitter().await;

        let (user_agent, api_client, session_id, request_id) = get_antiban_headers();

        let response = self
            .client
            .post(&url)
            .bearer_auth(access_token)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("User-Agent", user_agent)
            .header("X-Goog-Api-Client", api_client)
            .header("X-Client-Session-Id", session_id)
            .header("X-Request-Id", request_id)
            .header("X-Client-Version", "1.104.0")
            .json(body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if status == 429 {
            let error_body = response.text().await.unwrap_or_default();
            warn!("[Anti-Ban] Upstream streaming rate limit (429): {}", error_body);
            return Err(anyhow::anyhow!("RATE_LIMIT:429: {}", error_body));
        }

        if status == 401 {
            let error_body = response.text().await.unwrap_or_default();
            warn!("[Auth Error] Upstream streaming 401 Unauthorized: {}", error_body);
            return Err(anyhow::anyhow!("AUTH_ERROR:401: {}", error_body));
        }

        if (500..=599).contains(&status) {
            let error_body = response.text().await.unwrap_or_default();
            warn!("[Server Error] Upstream streaming {}: {}", status, error_body);
            return Err(anyhow::anyhow!("SERVER_ERROR:{}: {}", status, error_body));
        }

        if !response.status().is_success() {
            let error_body = response.text().await.unwrap_or_default();
            warn!("Upstream streaming error {}: {}", status, error_body);
            return Err(anyhow::anyhow!("Upstream error {}: {}", status, error_body));
        }

        let stream = response.bytes_stream();
        Ok((status, Box::pin(stream)))
    }
}
