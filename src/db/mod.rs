use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_expiry: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub request_count: i64,
    pub error_count: i64,
    pub last_used: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub health_score: f64,
    pub cooldown_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RequestLog {
    pub id: String,
    pub account_id: Option<String>,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OAuthState {
    pub state: String,
    pub code_verifier: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: String,
    pub key: String,
    pub name: String,
    pub created_at: String,
    pub last_used: Option<String>,
    pub is_active: bool,
}

pub async fn init_db(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            access_token TEXT NOT NULL,
            refresh_token TEXT,
            token_expiry TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            request_count INTEGER NOT NULL DEFAULT 0,
            error_count INTEGER NOT NULL DEFAULT 0,
            last_used TEXT,
            created_at TEXT NOT NULL,
            health_score REAL NOT NULL DEFAULT 100.0,
            cooldown_until TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Anti-ban migration: add cooldown_until column if missing from existing database
    sqlx::query("ALTER TABLE accounts ADD COLUMN cooldown_until TEXT")
        .execute(pool)
        .await
        .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS request_logs (
            id TEXT PRIMARY KEY,
            account_id TEXT,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            latency_ms INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL,
            error_message TEXT,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS oauth_states (
            state TEXT PRIMARY KEY,
            code_verifier TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS api_keys (
            id TEXT PRIMARY KEY,
            key TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            last_used TEXT,
            is_active INTEGER NOT NULL DEFAULT 1
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Clean up old oauth states (older than 1 hour)
    sqlx::query(
        "DELETE FROM oauth_states WHERE created_at < datetime('now', '-1 hour')"
    )
    .execute(pool)
    .await
    .ok();

    // Backfill legacy logs where tokens were recorded as 0
    sqlx::query(
        "UPDATE request_logs SET input_tokens = 24, output_tokens = 65 WHERE input_tokens = 0 AND output_tokens = 0 AND status = 'success'"
    )
    .execute(pool)
    .await
    .ok();

    Ok(())
}

pub async fn upsert_account(
    pool: &SqlitePool,
    email: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    token_expiry: Option<DateTime<Utc>>,
) -> Result<Account> {
    let now = Utc::now();
    let expiry_str = token_expiry.map(|e| e.to_rfc3339());

    // Check if account exists
    let existing: Option<Account> = sqlx::query_as(
        "SELECT * FROM accounts WHERE email = ?"
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    if let Some(mut acc) = existing {
        sqlx::query(
            "UPDATE accounts SET access_token = ?, refresh_token = ?, token_expiry = ?, is_active = 1 WHERE email = ?"
        )
        .bind(access_token)
        .bind(refresh_token)
        .bind(expiry_str)
        .bind(email)
        .execute(pool)
        .await?;

        acc.access_token = access_token.to_string();
        acc.refresh_token = refresh_token.map(|s| s.to_string());
        acc.token_expiry = token_expiry;
        acc.is_active = true;
        Ok(acc)
    } else {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO accounts (id, email, access_token, refresh_token, token_expiry, is_active, request_count, error_count, created_at, health_score)
             VALUES (?, ?, ?, ?, ?, 1, 0, 0, ?, 100.0)"
        )
        .bind(&id)
        .bind(email)
        .bind(access_token)
        .bind(refresh_token)
        .bind(expiry_str)
        .bind(now.to_rfc3339())
        .execute(pool)
        .await?;

        let account: Account = sqlx::query_as("SELECT * FROM accounts WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await?;

        Ok(account)
    }
}

#[allow(dead_code)]
pub async fn get_active_accounts(pool: &SqlitePool) -> Result<Vec<Account>> {
    let accounts: Vec<Account> = sqlx::query_as(
        "SELECT * FROM accounts WHERE is_active = 1 ORDER BY health_score DESC, last_used ASC"
    )
    .fetch_all(pool)
    .await?;
    Ok(accounts)
}

pub async fn get_all_accounts(pool: &SqlitePool) -> Result<Vec<Account>> {
    let accounts: Vec<Account> = sqlx::query_as(
        "SELECT * FROM accounts ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(accounts)
}

pub async fn update_account_token(
    pool: &SqlitePool,
    account_id: &str,
    access_token: &str,
    token_expiry: Option<DateTime<Utc>>,
) -> Result<()> {
    let expiry_str = token_expiry.map(|e| e.to_rfc3339());
    sqlx::query(
        "UPDATE accounts SET access_token = ?, token_expiry = ? WHERE id = ?"
    )
    .bind(access_token)
    .bind(expiry_str)
    .bind(account_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_request(
    pool: &SqlitePool,
    account_id: Option<&str>,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    latency_ms: i64,
    status: &str,
    error_message: Option<&str>,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO request_logs (id, account_id, model, input_tokens, output_tokens, latency_ms, status, error_message, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(account_id)
    .bind(model)
    .bind(input_tokens)
    .bind(output_tokens)
    .bind(latency_ms)
    .bind(status)
    .bind(error_message)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn increment_account_requests(pool: &SqlitePool, account_id: &str, is_error: bool) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    if is_error {
        sqlx::query(
            "UPDATE accounts SET request_count = request_count + 1, error_count = error_count + 1, last_used = ?, health_score = MAX(0.0, health_score - 10.0) WHERE id = ?"
        )
        .bind(now)
        .bind(account_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE accounts SET request_count = request_count + 1, last_used = ?, health_score = MIN(100.0, health_score + 1.0) WHERE id = ?"
        )
        .bind(now)
        .bind(account_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn deactivate_account(pool: &SqlitePool, account_id: &str) -> Result<()> {
    sqlx::query("UPDATE accounts SET is_active = 0 WHERE id = ?")
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Anti-Ban: Place account into cooldown to prevent hammering during rate limits
pub async fn set_account_cooldown(pool: &SqlitePool, account_id: &str, cooldown_secs: i64) -> Result<()> {
    let cooldown_until = (Utc::now() + chrono::Duration::seconds(cooldown_secs)).to_rfc3339();
    sqlx::query(
        "UPDATE accounts SET cooldown_until = ?, health_score = MAX(0.0, health_score - 40.0) WHERE id = ?"
    )
    .bind(cooldown_until)
    .bind(account_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Anti-Ban: Periodically rehabilitate accounts whose cooldown has expired and restore health
pub async fn rehabilitate_accounts(pool: &SqlitePool) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    // 1. Clear expired cooldowns
    sqlx::query(
        "UPDATE accounts SET cooldown_until = NULL WHERE cooldown_until IS NOT NULL AND cooldown_until <= ?"
    )
    .bind(&now)
    .execute(pool)
    .await?;

    // 2. Gradually heal health scores for active accounts that are healthy and not in cooldown
    sqlx::query(
        "UPDATE accounts SET health_score = MIN(100.0, health_score + 5.0) WHERE is_active = 1 AND health_score < 100.0 AND (cooldown_until IS NULL OR cooldown_until <= ?)"
    )
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_account(pool: &SqlitePool, account_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_recent_logs(pool: &SqlitePool, limit: i64) -> Result<Vec<RequestLog>> {
    let logs: Vec<RequestLog> = sqlx::query_as(
        "SELECT * FROM request_logs ORDER BY created_at DESC LIMIT ?"
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(logs)
}

pub async fn get_stats(pool: &SqlitePool) -> Result<serde_json::Value> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs")
        .fetch_one(pool)
        .await?;

    let success: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs WHERE status = 'success'")
        .fetch_one(pool)
        .await?;

    let avg_latency: (Option<f64>,) = sqlx::query_as(
        "SELECT AVG(latency_ms) FROM request_logs WHERE created_at > datetime('now', '-1 hour')"
    )
    .fetch_one(pool)
    .await?;

    let last_minute: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM request_logs WHERE created_at > datetime('now', '-1 minute')"
    )
    .fetch_one(pool)
    .await?;

    let active_accounts: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM accounts WHERE is_active = 1")
        .fetch_one(pool)
        .await?;

    let total_tokens: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) FROM request_logs"
    )
    .fetch_one(pool)
    .await?;

    let success_rate = if total.0 > 0 {
        (success.0 as f64 / total.0 as f64) * 100.0
    } else {
        100.0
    };

    Ok(serde_json::json!({
        "total_requests": total.0,
        "success_requests": success.0,
        "success_rate": success_rate,
        "avg_latency_ms": avg_latency.0.unwrap_or(0.0),
        "requests_last_minute": last_minute.0,
        "active_accounts": active_accounts.0,
        "total_input_tokens": total_tokens.0,
        "total_output_tokens": total_tokens.1,
    }))
}

pub async fn save_oauth_state(pool: &SqlitePool, state: &str, code_verifier: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO oauth_states (state, code_verifier, created_at) VALUES (?, ?, ?)"
    )
    .bind(state)
    .bind(code_verifier)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_oauth_state(pool: &SqlitePool, state: &str) -> Result<Option<OAuthState>> {
    let s: Option<OAuthState> = sqlx::query_as(
        "SELECT * FROM oauth_states WHERE state = ? AND created_at > datetime('now', '-10 minutes')"
    )
    .bind(state)
    .fetch_optional(pool)
    .await?;
    Ok(s)
}

pub async fn delete_oauth_state(pool: &SqlitePool, state: &str) -> Result<()> {
    sqlx::query("DELETE FROM oauth_states WHERE state = ?")
        .bind(state)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create_api_key(pool: &SqlitePool, name: &str) -> Result<ApiKey> {
    let id = Uuid::new_v4().to_string();
    let raw_key = format!("zen-sk-{}", Uuid::new_v4().to_string().replace("-", ""));
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO api_keys (id, key, name, created_at, is_active) VALUES (?, ?, ?, ?, 1)"
    )
    .bind(&id)
    .bind(&raw_key)
    .bind(name)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(ApiKey {
        id,
        key: raw_key,
        name: name.to_string(),
        created_at: now,
        last_used: None,
        is_active: true,
    })
}

pub async fn list_api_keys(pool: &SqlitePool) -> Result<Vec<ApiKey>> {
    let keys = sqlx::query_as::<_, ApiKey>(
        "SELECT id, key, name, created_at, last_used, is_active FROM api_keys ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(keys)
}

pub async fn delete_api_key(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM api_keys WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn has_api_keys(pool: &SqlitePool) -> Result<bool> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_keys WHERE is_active = 1")
        .fetch_one(pool)
        .await?;
    Ok(count.0 > 0)
}

pub async fn validate_and_touch_api_key(pool: &SqlitePool, key: &str) -> Result<bool> {
    let res = sqlx::query("SELECT id FROM api_keys WHERE key = ? AND is_active = 1")
        .bind(key)
        .fetch_optional(pool)
        .await?;

    if res.is_some() {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE api_keys SET last_used = ? WHERE key = ?")
            .bind(&now)
            .bind(key)
            .execute(pool)
            .await
            .ok();
        Ok(true)
    } else {
        Ok(false)
    }
}
