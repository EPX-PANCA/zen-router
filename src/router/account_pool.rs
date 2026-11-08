use anyhow::Result;
use parking_lot::RwLock;
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{info, warn};

use crate::auth::oauth::OAuthFlow;
use crate::auth::token::get_valid_token;
use crate::db::{self, Account};

pub struct AccountPool {
    pool: SqlitePool,
    oauth_flow: Arc<OAuthFlow>,
    current_index: Arc<RwLock<usize>>,
}

impl AccountPool {
    pub fn new(pool: SqlitePool, oauth_flow: Arc<OAuthFlow>) -> Self {
        Self {
            pool,
            oauth_flow,
            current_index: Arc::new(RwLock::new(0)),
        }
    }

    /// Get the next available account with a valid token (round-robin with health scoring)
    pub async fn get_next_account(&self) -> Result<Option<(Account, String)>> {
        let accounts = db::get_active_accounts(&self.pool).await?;

        if accounts.is_empty() {
            return Ok(None);
        }

        // Sort by health score (descending) for weighted round-robin
        let healthy_accounts: Vec<&Account> = accounts
            .iter()
            .filter(|a| a.health_score > 20.0)
            .collect();

        let candidates = if healthy_accounts.is_empty() {
            &accounts[..]
        } else {
            &accounts[..healthy_accounts.len()]
        };

        // Round-robin among candidates
        let idx = {
            let mut idx = self.current_index.write();
            let current = *idx % candidates.len();
            *idx = (*idx + 1) % candidates.len();
            current
        };

        let account = candidates[idx].clone();

        // Get valid token (refreshing if needed)
        match get_valid_token(&self.pool, &account.id, &self.oauth_flow).await {
            Ok(token) => Ok(Some((account, token))),
            Err(e) => {
                warn!("Failed to get valid token for account {}: {}", account.email, e);
                // Try to deactivate and try next
                db::deactivate_account(&self.pool, &account.id).await.ok();
                // Recurse with remaining accounts (simple retry)
                Box::pin(self.get_next_account()).await
            }
        }
    }

    /// Handle a 429 (rate limit) response - penalize the account and try another
    pub async fn handle_rate_limit(&self, account_id: &str) -> Result<Option<(Account, String)>> {
        warn!("Rate limit hit for account {}, rotating to next account", account_id);

        // Heavily penalize the rate-limited account
        sqlx::query(
            "UPDATE accounts SET health_score = MAX(0.0, health_score - 50.0) WHERE id = ?"
        )
        .bind(account_id)
        .execute(&self.pool)
        .await?;

        db::increment_account_requests(&self.pool, account_id, true).await.ok();

        // Get a different account
        let accounts = db::get_active_accounts(&self.pool).await?;
        let alternatives: Vec<&Account> = accounts
            .iter()
            .filter(|a| a.id != account_id && a.health_score > 10.0)
            .collect();

        if let Some(alt) = alternatives.first() {
            match get_valid_token(&self.pool, &alt.id, &self.oauth_flow).await {
                Ok(token) => {
                    info!("Rotated to account {} after rate limit", alt.email);
                    return Ok(Some(((*alt).clone(), token)));
                }
                Err(e) => {
                    warn!("Failed to get token for alternative account {}: {}", alt.email, e);
                }
            }
        }

        Ok(None)
    }

    /// Mark an account request as complete
    pub async fn record_success(&self, account_id: &str) -> Result<()> {
        db::increment_account_requests(&self.pool, account_id, false).await
    }

    /// Mark an account request as error
    pub async fn record_error(&self, account_id: &str) -> Result<()> {
        db::increment_account_requests(&self.pool, account_id, true).await
    }
}
