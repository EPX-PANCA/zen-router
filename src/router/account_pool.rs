use anyhow::Result;
use chrono::Utc;
use parking_lot::RwLock;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::auth::oauth::OAuthFlow;
use crate::auth::token::get_valid_token;
use crate::db::{self, Account};

pub struct AccountPool {
    pool: SqlitePool,
    oauth_flow: Arc<OAuthFlow>,
    current_index: Arc<RwLock<usize>>,
    // Anti-Ban Rate Limiter: tracks sliding 60-second request timestamps per account
    request_history: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

impl AccountPool {
    pub fn new(pool: SqlitePool, oauth_flow: Arc<OAuthFlow>) -> Self {
        Self {
            pool,
            oauth_flow,
            current_index: Arc::new(RwLock::new(0)),
            request_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Anti-Ban Smart Selection:
    /// 1. Exclude accounts currently in cooldown (e.g. from 429 rate limits)
    /// 2. Prioritize healthy accounts (health_score > 15.0)
    /// 3. Rate-limit check: avoid accounts that exceeded 25 requests in the last 60 seconds
    /// 4. Fair weighted round-robin distribution to prevent hotspotting
    pub async fn get_next_account(&self) -> Result<Option<(Account, String)>> {
        let accounts = db::get_active_accounts(&self.pool).await?;

        if accounts.is_empty() {
            return Ok(None);
        }

        let now_utc = Utc::now();

        // 1. Filter out accounts currently in cooldown
        let available_accounts: Vec<Account> = accounts
            .into_iter()
            .filter(|a| {
                if let Some(cooldown) = a.cooldown_until {
                    cooldown <= now_utc
                } else {
                    true
                }
            })
            .collect();

        if available_accounts.is_empty() {
            warn!("[Anti-Ban] All active accounts are currently in cooldown! Quotas cooling down.");
            return Ok(None);
        }

        // 2. Filter healthy accounts (health > 15.0)
        let mut healthy_accounts: Vec<Account> = available_accounts
            .iter()
            .filter(|a| a.health_score > 15.0)
            .cloned()
            .collect();

        if healthy_accounts.is_empty() {
            healthy_accounts = available_accounts;
        }

        // 3. Sliding-window rate check: count requests in last 60 seconds per account
        let now_instant = Instant::now();
        let sixty_secs_ago = Duration::from_secs(60);

        let mut candidate_accounts = Vec::new();
        let mut high_load_accounts = Vec::new();

        {
            let mut history = self.request_history.write();
            for acc in healthy_accounts {
                let timestamps = history.entry(acc.id.clone()).or_default();
                // Retain only requests within the last 60 seconds
                timestamps.retain(|t| now_instant.duration_since(*t) < sixty_secs_ago);

                // Anti-Ban threshold: max 25 RPM per account to stay comfortably below Google's quota
                if timestamps.len() < 25 {
                    candidate_accounts.push(acc);
                } else {
                    high_load_accounts.push(acc);
                }
            }
        }

        // If candidates with < 25 RPM exist, use them; otherwise fallback to least busy high-load accounts
        let candidates = if !candidate_accounts.is_empty() {
            candidate_accounts
        } else {
            high_load_accounts
        };

        // 4. Round-robin selection among candidates
        let idx = {
            let mut idx = self.current_index.write();
            let current = *idx % candidates.len();
            *idx = (*idx + 1) % candidates.len();
            current
        };

        let account = candidates[idx].clone();

        // Record request timestamp for anti-ban leaky bucket tracking
        {
            let mut history = self.request_history.write();
            history
                .entry(account.id.clone())
                .or_default()
                .push(now_instant);
        }

        // Get valid token (refreshing if needed)
        match get_valid_token(&self.pool, &account.id, &self.oauth_flow).await {
            Ok(token) => Ok(Some((account, token))),
            Err(e) => {
                warn!(
                    "Failed to get valid token for account {}: {}",
                    account.email, e
                );
                // Deactivate broken account and retry with next
                db::deactivate_account(&self.pool, &account.id).await.ok();
                Box::pin(self.get_next_account()).await
            }
        }
    }

    /// Anti-Ban Rate Limit Handler:
    /// Put rate-limited account into cooldown for 60 seconds so it is NEVER spammed while throttled,
    /// and immediately rotate to a healthy non-cooldown alternative account.
    pub async fn handle_rate_limit(&self, account_id: &str) -> Result<Option<(Account, String)>> {
        warn!(
            "[Anti-Ban] Rate limit (429) hit for account {}. Entering 60s cooldown and rotating.",
            account_id
        );

        // Put account on 60-second cooldown in database
        db::set_account_cooldown(&self.pool, account_id, 60).await.ok();
        db::increment_account_requests(&self.pool, account_id, true).await.ok();

        // Get alternative account that is NOT in cooldown
        let now_utc = Utc::now();
        let accounts = db::get_active_accounts(&self.pool).await?;
        let alternatives: Vec<&Account> = accounts
            .iter()
            .filter(|a| {
                a.id != account_id
                    && a.health_score > 10.0
                    && a.cooldown_until.map(|c| c <= now_utc).unwrap_or(true)
            })
            .collect();

        if let Some(alt) = alternatives.first() {
            match get_valid_token(&self.pool, &alt.id, &self.oauth_flow).await {
                Ok(token) => {
                    info!(
                        "[Anti-Ban] Successfully failed over to account {}",
                        alt.email
                    );
                    return Ok(Some(((*alt).clone(), token)));
                }
                Err(e) => {
                    warn!(
                        "Failed to get token for alternative account {}: {}",
                        alt.email, e
                    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cooldown_filter_logic() {
        let now = Utc::now();
        let active_account = Account {
            id: "acc_1".to_string(),
            email: "acc1@gmail.com".to_string(),
            access_token: "token1".to_string(),
            refresh_token: None,
            token_expiry: None,
            is_active: true,
            request_count: 5,
            error_count: 0,
            last_used: None,
            created_at: now,
            health_score: 100.0,
            cooldown_until: None,
        };

        let cooled_account = Account {
            id: "acc_2".to_string(),
            email: "acc2@gmail.com".to_string(),
            access_token: "token2".to_string(),
            refresh_token: None,
            token_expiry: None,
            is_active: true,
            request_count: 20,
            error_count: 3,
            last_used: None,
            created_at: now,
            health_score: 50.0,
            cooldown_until: Some(now + chrono::Duration::seconds(60)),
        };

        let expired_cooldown_account = Account {
            id: "acc_3".to_string(),
            email: "acc3@gmail.com".to_string(),
            access_token: "token3".to_string(),
            refresh_token: None,
            token_expiry: None,
            is_active: true,
            request_count: 10,
            error_count: 1,
            last_used: None,
            created_at: now,
            health_score: 90.0,
            cooldown_until: Some(now - chrono::Duration::seconds(10)),
        };

        let accounts = vec![active_account, cooled_account, expired_cooldown_account];

        let available: Vec<&Account> = accounts
            .iter()
            .filter(|a| {
                if let Some(c) = a.cooldown_until {
                    c <= now
                } else {
                    true
                }
            })
            .collect();

        assert_eq!(available.len(), 2);
        assert_eq!(available[0].id, "acc_1");
        assert_eq!(available[1].id, "acc_3");
    }
}
