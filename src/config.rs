use anyhow::Result;
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub redirect_uri: String,
    pub public_base_url: Option<String>,
    pub app_env: String,
    pub project_id: String,
    pub database_url: String,
    pub session_secret: String,
    pub upstream_base_url: String,
    pub upstream_location: String,
}

fn load_secret_json() -> Option<(String, String, Option<String>, Option<String>)> {
    let candidate_paths = [
        "secret.json",
        "/app/secret.json",
        "client_secret.json",
        "/app/client_secret.json",
    ];

    for path in candidate_paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                let client_obj = val.get("web").or_else(|| val.get("installed"));
                if let Some(obj) = client_obj {
                    let client_id = obj
                        .get("client_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let client_secret = obj
                        .get("client_secret")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let redirect_uri = obj
                        .get("redirect_uris")
                        .and_then(|arr| arr.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let project_id = obj
                        .get("project_id")
                        .or_else(|| val.get("project_id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if let (Some(cid), Some(csec)) = (client_id, client_secret) {
                        return Some((cid, csec, redirect_uri, project_id));
                    }
                }
            }
        }
    }
    None
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let port = env::var("PORT")
            .unwrap_or_else(|_| "20128".to_string())
            .parse::<u16>()
            .unwrap_or(20128);

        let secret_file_info = load_secret_json();

        let google_client_id = env::var("GOOGLE_CLIENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| secret_file_info.as_ref().map(|s| s.0.clone()))
            .ok_or_else(|| anyhow::anyhow!("GOOGLE_CLIENT_ID must be configured in .env"))?;

        let google_client_secret = env::var("GOOGLE_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| secret_file_info.as_ref().map(|s| s.1.clone()))
            .ok_or_else(|| anyhow::anyhow!("GOOGLE_CLIENT_SECRET must be configured in .env"))?;

        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "production".to_string());
        let public_base_url = env::var("PUBLIC_BASE_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());

        let redirect_uri = env::var("REDIRECT_URI")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("http://localhost:{}/api/oauth/callback", port));

        let project_id = env::var("PROJECT_ID")
            .or_else(|_| env::var("GOOGLE_PROJECT_ID"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| secret_file_info.as_ref().and_then(|s| s.3.clone()))
            .ok_or_else(|| anyhow::anyhow!("PROJECT_ID must be configured in .env"))?;

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            if std::path::Path::new("/.dockerenv").exists() || std::path::Path::new("/app/data").exists() {
                "sqlite:/app/data/zen-router.db".to_string()
            } else {
                "sqlite:./data/zen-router.db".to_string()
            }
        });

        let session_secret = env::var("SESSION_SECRET")
            .unwrap_or_else(|_| "zen-router-default-secret-change-me-in-production".to_string());

        let upstream_base_url = env::var("UPSTREAM_BASE_URL")
            .unwrap_or_else(|_| "https://daily-cloudcode-pa.googleapis.com".to_string());

        let upstream_location = env::var("UPSTREAM_LOCATION")
            .unwrap_or_else(|_| "us-central1".to_string());

        Ok(Config {
            port,
            google_client_id,
            google_client_secret,
            redirect_uri,
            public_base_url,
            app_env,
            project_id,
            database_url,
            session_secret,
            upstream_base_url,
            upstream_location,
        })
    }

    pub fn upstream_url(&self, is_stream: bool) -> String {
        if is_stream {
            format!("{}/v1internal:streamGenerateContent?alt=sse", self.upstream_base_url)
        } else {
            format!("{}/v1internal:generateContent", self.upstream_base_url)
        }
    }
}
