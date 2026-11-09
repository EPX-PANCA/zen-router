use anyhow::Result;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::auth::oauth::OAuthFlow;
use crate::auth::token::{ensure_fresh_token, is_token_expired, is_token_expiring_soon};
use crate::db::{self, Account};

#[derive(Clone)]
pub struct AccountPool {
    pool: SqlitePool,
    oauth_flow: Arc<OAuthFlow>,
    current_index: Arc<RwLock<usize>>,
    // Anti-Ban Rate Limiter: tracks sliding 60-second request timestamps per account
    request_history: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    // IN-MEMORY CACHE: completely eliminates SQLite queries on the chat routing path
    accounts_cache: Arc<RwLock<Vec<Account>>>,
}

impl AccountPool {
    pub fn new(pool: SqlitePool, oauth_flow: Arc<OAuthFlow>) -> Self {
        Self {
            pool,
            oauth_flow,
            current_index: Arc::new(RwLock::new(0)),
            request_history: Arc::new(RwLock::new(HashMap::new())),
            accounts_cache: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Reload in-memory cache directly from SQLite
    pub async fn reload_cache(&self) -> Result<()> {
        let accounts = db::get_all_accounts(&self.pool).await?;
        *self.accounts_cache.write() = accounts;
        Ok(())
    }

    /// Update cached token & expiry in-memory
    pub fn update_cached_token(
        &self,
        account_id: &str,
        new_token: &str,
        expiry: Option<DateTime<Utc>>,
    ) {
        let mut cache = self.accounts_cache.write();
        if let Some(acc) = cache.iter_mut().find(|a| a.id == account_id) {
            acc.access_token = new_token.to_string();
            acc.token_expiry = expiry;
        }
    }

    /// Set cached cooldown in-memory
    pub fn set_cached_cooldown(&self, account_id: &str, cooldown_secs: i64) {
        let cooldown_until = Utc::now() + chrono::Duration::seconds(cooldown_secs);
        let mut cache = self.accounts_cache.write();
        if let Some(acc) = cache.iter_mut().find(|a| a.id == account_id) {
            acc.cooldown_until = Some(cooldown_until);
        }
    }

    /// Update cached health in-memory
    pub fn update_cached_health(&self, account_id: &str, is_error: bool) {
        let mut cache = self.accounts_cache.write();
        if let Some(acc) = cache.iter_mut().find(|a| a.id == account_id) {
            if is_error {
                acc.error_count += 1;
                acc.health_score = (acc.health_score - 10.0).max(0.0);
            } else {
                acc.request_count += 1;
                acc.health_score = (acc.health_score + 0.5).min(100.0);
            }
        }
    }

    /// Proactively refreshes any active account token that will expire in the next 10 minutes.
    /// Runs in a background task so incoming user requests NEVER wait on an OAuth roundtrip!
    pub async fn refresh_expiring_tokens(&self) -> Result<usize> {
        let expiring: Vec<(String, String, Option<DateTime<Utc>>)> = {
            let cache = self.accounts_cache.read();
            cache
                .iter()
                .filter(|a| a.is_active && a.refresh_token.is_some())
                .filter(|a| is_token_expiring_soon(a.token_expiry))
                .map(|a| (a.id.clone(), a.refresh_token.clone().unwrap(), a.token_expiry))
                .collect()
        };

        let mut count = 0;
        for (id, refresh_token, expiry) in expiring {
            info!(
                "[Proactive-Refresh] Account {} token expiring soon. Refreshing in background...",
                id
            );
            match ensure_fresh_token(&self.pool, &id, &refresh_token, expiry, &self.oauth_flow).await
            {
                Ok(new_token) => {
                    let new_expiry = Some(Utc::now() + chrono::Duration::hours(1));
                    self.update_cached_token(&id, &new_token, new_expiry);
                    count += 1;
                }
                Err(e) => {
                    warn!(
                        "[Proactive-Refresh] Failed refreshing token for account {}: {}",
                        id, e
                    );
                }
            }
        }
        Ok(count)
    }

    /// Anti-Ban Smart Selection using IN-MEMORY cache:
    /// 1. Exclude accounts currently in cooldown (e.g. from 429 rate limits)
    /// 2. Prioritize healthy accounts (health_score > 15.0)
    /// 3. Rate-limit check: avoid accounts that exceeded 25 requests in the last 60 seconds
    /// 4. Fair weighted round-robin distribution to prevent hotspotting
    pub async fn get_next_account(&self) -> Result<Option<(Account, String)>> {
        // 1. Read from in-memory cache
        let mut active_accounts: Vec<Account> = {
            let cache = self.accounts_cache.read();
            cache.iter().filter(|a| a.is_active).cloned().collect()
        };

        // If cache is empty (e.g. at startup before first reload), populate it
        if active_accounts.is_empty() {
            self.reload_cache().await.ok();
            active_accounts = {
                let cache = self.accounts_cache.read();
                cache.iter().filter(|a| a.is_active).cloned().collect()
            };
        }

        if active_accounts.is_empty() {
            return Ok(None);
        }

        let now_utc = Utc::now();

        // 1. Filter out accounts currently in cooldown
        let available_accounts: Vec<Account> = active_accounts
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
                timestamps.retain(|t| now_instant.duration_since(*t) < sixty_secs_ago);

                // Anti-Ban threshold: max 25 RPM per account to stay comfortably below Google's quota
                if timestamps.len() < 25 {
                    candidate_accounts.push(acc);
                } else {
                    high_load_accounts.push(acc);
                }
            }
        }

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

        // Get valid token (using cached token or refreshing with anti-stampede if expired)
        let token = if is_token_expired(account.token_expiry) {
            if let Some(ref rt) = account.refresh_token {
                match ensure_fresh_token(
                    &self.pool,
                    &account.id,
                    rt,
                    account.token_expiry,
                    &self.oauth_flow,
                )
                .await
                {
                    Ok(tok) => {
                        let new_expiry = Some(Utc::now() + chrono::Duration::hours(1));
                        self.update_cached_token(&account.id, &tok, new_expiry);
                        tok
                    }
                    Err(e) => {
                        warn!(
                            "Failed to refresh token for account {}: {}",
                            account.email, e
                        );
                        db::deactivate_account(&self.pool, &account.id).await.ok();
                        self.reload_cache().await.ok();
                        return Box::pin(self.get_next_account()).await;
                    }
                }
            } else {
                account.access_token.clone()
            }
        } else {
            account.access_token.clone()
        };

        Ok(Some((account, token)))
    }

    /// Full Transparent Failover (429 Rate Limit, 401 Auth Error, 5xx Server Error):
    /// Intelligently penalizes the failing account and rotates to a healthy alternative account.
    pub async fn handle_failover(
        &self,
        account_id: &str,
        error_msg: &str,
    ) -> Result<Option<(Account, String)>> {
        let now_utc = Utc::now();

        if error_msg.starts_with("RATE_LIMIT:") {
            warn!(
                "[Failover] Rate limit (429) hit for account {}. Entering 60s cooldown.",
                account_id
            );
            self.set_cached_cooldown(account_id, 60);
            db::set_account_cooldown(&self.pool, account_id, 60).await.ok();
            self.record_error(account_id).await.ok();
        } else if error_msg.starts_with("AUTH_ERROR:") {
            warn!(
                "[Failover] 401 Unauthorized for account {}. Token invalidated.",
                account_id
            );
            self.update_cached_token(account_id, "", Some(now_utc - chrono::Duration::minutes(1)));
            self.record_error(account_id).await.ok();
        } else if error_msg.starts_with("SERVER_ERROR:") {
            warn!(
                "[Failover] Upstream 5xx server error for account {}. Rotating.",
                account_id
            );
            self.record_error(account_id).await.ok();
        }

        // Find alternative active account that is NOT in cooldown
        let alternatives: Vec<Account> = {
            let cache = self.accounts_cache.read();
            cache
                .iter()
                .filter(|a| {
                    a.id != account_id
                        && a.is_active
                        && a.health_score > 10.0
                        && a.cooldown_until.map(|c| c <= now_utc).unwrap_or(true)
                })
                .cloned()
                .collect()
        };

        if let Some(alt) = alternatives.first() {
            let token = if is_token_expired(alt.token_expiry) {
                if let Some(ref rt) = alt.refresh_token {
                    match ensure_fresh_token(
                        &self.pool,
                        &alt.id,
                        rt,
                        alt.token_expiry,
                        &self.oauth_flow,
                    )
                    .await
                    {
                        Ok(tok) => {
                            let new_expiry = Some(Utc::now() + chrono::Duration::hours(1));
                            self.update_cached_token(&alt.id, &tok, new_expiry);
                            tok
                        }
                        Err(_) => alt.access_token.clone(),
                    }
                } else {
                    alt.access_token.clone()
                }
            } else {
                alt.access_token.clone()
            };

            info!(
                "[Failover] Seamlessly failed over to alternative account: {}",
                alt.email
            );
            return Ok(Some((alt.clone(), token)));
        }

        Ok(None)
    }

    /// Anti-Ban Rate Limit Handler (legacy alias)
    pub async fn handle_rate_limit(&self, account_id: &str) -> Result<Option<(Account, String)>> {
        self.handle_failover(account_id, "RATE_LIMIT:429").await
    }

    /// Mark an account request as complete
    pub async fn record_success(&self, account_id: &str) -> Result<()> {
        self.update_cached_health(account_id, false);
        db::increment_account_requests(&self.pool, account_id, false).await
    }

    /// Mark an account request as error
    pub async fn record_error(&self, account_id: &str) -> Result<()> {
        self.update_cached_health(account_id, true);
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
