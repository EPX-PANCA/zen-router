use anyhow::Result;
use bytes::Bytes;
use futures::Stream;
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

impl UpstreamClient {
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client");

        Self { client, config }
    }

    /// Send a non-streaming request to the upstream API
    pub async fn send_request(
        &self,
        body: &Value,
        access_token: &str,
    ) -> Result<UpstreamResponse> {
        let url = self.config.upstream_url(false);
        debug!("Sending request to upstream: {}", url);

        let response = self
            .client
            .post(&url)
            .bearer_auth(access_token)
            .header("Content-Type", "application/json")
            .header("User-Agent", "antigravity/1.104.0 darwin/arm64")
            .header("X-Goog-Api-Client", "gl-rust/1.0")
            .json(body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if status == 429 {
            let error_body = response.text().await.unwrap_or_default();
            warn!("Upstream rate limit (429): {}", error_body);
            return Err(anyhow::anyhow!("RATE_LIMIT:429: {}", error_body));
        }

        if !response.status().is_success() {
            let error_body = response.text().await.unwrap_or_default();
            warn!("Upstream error {}: {}", status, error_body);
            return Err(anyhow::anyhow!("Upstream error {}: {}", status, error_body));
        }

        let body: Value = response.json().await?;
        Ok(UpstreamResponse { status, body })
    }

    /// Send a streaming request and return the raw byte stream
    pub async fn send_streaming_request(
        &self,
        body: &Value,
        access_token: &str,
    ) -> Result<(u16, Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>)> {
        let url = self.config.upstream_url(true);
        debug!("Sending streaming request to upstream: {}", url);

        let response = self
            .client
            .post(&url)
            .bearer_auth(access_token)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("User-Agent", "antigravity/1.104.0 darwin/arm64")
            .header("X-Goog-Api-Client", "gl-rust/1.0")
            .json(body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if status == 429 {
            let error_body = response.text().await.unwrap_or_default();
            warn!("Upstream streaming rate limit (429): {}", error_body);
            return Err(anyhow::anyhow!("RATE_LIMIT:429: {}", error_body));
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
