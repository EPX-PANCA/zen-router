use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::Config;

pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

pub const OAUTH_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
    "openid",
];

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
    pub id_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub email: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

pub struct OAuthFlow {
    config: Config,
    http_client: reqwest::Client,
}

impl OAuthFlow {
    pub fn new(config: Config, http_client: reqwest::Client) -> Self {
        Self { config, http_client }
    }

    /// Generate a random state parameter
    pub fn generate_state() -> String {
        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Generate PKCE code verifier and challenge
    pub fn generate_pkce() -> (String, String) {
        let mut rng = rand::thread_rng();
        let verifier_bytes: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
        let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let code_challenge = URL_SAFE_NO_PAD.encode(hash.as_slice());

        (code_verifier, code_challenge)
    }

    /// Build the authorization URL
    pub fn authorization_url(&self, state: &str, code_challenge: &str) -> String {
        let scopes = OAUTH_SCOPES.join(" ");
        let params = vec![
            ("client_id", self.config.google_client_id.as_str()),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", &scopes),
            ("state", state),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            ("prompt", "consent select_account"),
            ("include_granted_scopes", "true"),
        ];

        let query: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlenccode(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!("{}?{}", GOOGLE_AUTH_URL, query)
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<TokenResponse> {
        let params = [
            ("client_id", self.config.google_client_id.as_str()),
            ("client_secret", self.config.google_client_secret.as_str()),
            ("code", code),
            ("code_verifier", code_verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ];

        let response = self
            .http_client
            .post(GOOGLE_TOKEN_URL)
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Token exchange failed with status {}: {}", status, body);
        }

        let token_response: TokenResponse = response.json().await?;
        Ok(token_response)
    }

    /// Refresh an access token using a refresh token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        let params = [
            ("client_id", self.config.google_client_id.as_str()),
            ("client_secret", self.config.google_client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];

        let response = self
            .http_client
            .post(GOOGLE_TOKEN_URL)
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Token refresh failed with status {}: {}", status, body);
        }

        let token_response: TokenResponse = response.json().await?;
        Ok(token_response)
    }

    /// Get user info from Google
    pub async fn get_user_info(&self, access_token: &str) -> Result<UserInfo> {
        let response = self
            .http_client
            .get(GOOGLE_USERINFO_URL)
            .bearer_auth(access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("UserInfo request failed with status {}: {}", status, body);
        }

        let user_info: UserInfo = response.json().await?;
        Ok(user_info)
    }
}

/// Simple URL percent-encoding for query parameters
fn urlenccode(s: &str) -> String {
    let mut encoded = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push_str("%20"),
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{:02X}", byte));
            }
        }
    }
    encoded
}
