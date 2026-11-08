use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::auth::oauth::OAuthFlow;
use crate::db;

/// Check if a token is expired or will expire within the next 5 minutes
pub fn is_token_expired(expiry: Option<DateTime<Utc>>) -> bool {
    match expiry {
        None => false, // No expiry set, assume still valid
        Some(expiry) => Utc::now() + Duration::minutes(5) >= expiry,
    }
}

/// Attempt to refresh an account's token if needed
pub async fn ensure_fresh_token(
    pool: &SqlitePool,
    account_id: &str,
    refresh_token: &str,
    token_expiry: Option<DateTime<Utc>>,
    oauth_flow: &OAuthFlow,
) -> Result<String> {
    if !is_token_expired(token_expiry) {
        // Token still fresh - get it from DB
        let accounts = db::get_all_accounts(pool).await?;
        if let Some(acc) = accounts.iter().find(|a| a.id == account_id) {
            return Ok(acc.access_token.clone());
        }
    }

    // Token expired, refresh it
    info!("Refreshing token for account {}", account_id);
    let token_resp = oauth_flow.refresh_token(refresh_token).await?;

    let new_expiry = token_resp
        .expires_in
        .map(|secs| Utc::now() + Duration::seconds(secs));

    db::update_account_token(pool, account_id, &token_resp.access_token, new_expiry).await?;

    info!("Token refreshed successfully for account {}", account_id);
    Ok(token_resp.access_token)
}

/// Get a valid access token for an account, refreshing if needed
pub async fn get_valid_token(
    pool: &SqlitePool,
    account_id: &str,
    oauth_flow: &OAuthFlow,
) -> Result<String> {
    let accounts = db::get_all_accounts(pool).await?;
    let account = accounts
        .iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| anyhow::anyhow!("Account {} not found", account_id))?;

    if let Some(ref refresh_token) = account.refresh_token {
        if is_token_expired(account.token_expiry) {
            warn!("Token for account {} is expired, refreshing", account_id);
            return ensure_fresh_token(
                pool,
                account_id,
                refresh_token,
                account.token_expiry,
                oauth_flow,
            )
            .await;
        }
    }

    Ok(account.access_token.clone())
}
