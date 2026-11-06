//! Needs-reauth lifecycle for OAuth accounts.
//!
//! When the provider refuses an account's refresh token (`invalid_grant`: the
//! user revoked access, changed their password, or the grant expired), retrying
//! cannot help. The account is flagged `needs_reauth`, sync for it pauses, and
//! an `account_needs_reauth` event goes out on the SSE stream (plus an
//! `account.needs_reauth` webhook) so clients can prompt the user to reconnect.
//! A successful reconnect (re-consent) clears the flag and emits
//! `account_reauthed`.

use email_types::SyncEvent;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tracing::warn;

use crate::api::webhooks::fire_webhooks;
use crate::error::{AppError, Result};

/// Whether `error` is the provider refusing the grant itself, as opposed to a
/// transient outage or a rejected access token that a refresh can fix.
pub fn is_revoked_grant(error: &AppError) -> bool {
    matches!(error, AppError::Auth(message) if message.contains("invalid_grant"))
}

/// The error a sync of a flagged account returns without contacting the provider.
pub fn needs_reauth_error(account_id: &str) -> AppError {
    AppError::Auth(format!(
        "account {account_id} needs reauthorization; reconnect the account"
    ))
}

pub async fn needs_reauth(pool: &SqlitePool, account_id: &str) -> Result<bool> {
    let flagged: Option<bool> =
        sqlx::query_scalar("SELECT needs_reauth FROM accounts WHERE id = ?")
            .bind(account_id)
            .fetch_optional(pool)
            .await?;
    Ok(flagged.unwrap_or(false))
}

fn send(event_tx: Option<&broadcast::Sender<String>>, event: &SyncEvent) {
    if let Some(tx) = event_tx {
        if let Ok(json) = serde_json::to_string(event) {
            let _ = tx.send(json);
        }
    }
}

/// Flag the account. Emits the event only on the transition, so parallel
/// workers that all hit the refusal report it once. Returns whether it flagged.
pub async fn mark_needs_reauth(
    pool: &SqlitePool,
    event_tx: Option<&broadcast::Sender<String>>,
    account_id: &str,
    reason: &str,
) -> Result<bool> {
    let reason: String = reason.chars().take(512).collect();
    let flagged = sqlx::query(
        "UPDATE accounts SET needs_reauth = 1, reauth_reason = ? WHERE id = ? AND needs_reauth = 0",
    )
    .bind(&reason)
    .bind(account_id)
    .execute(pool)
    .await?
    .rows_affected()
        > 0;
    if flagged {
        warn!("account {account_id} needs reauthorization: {reason}");
        send(
            event_tx,
            &SyncEvent::AccountNeedsReauth {
                account_id: account_id.to_string(),
                reason: Some(reason.clone()),
            },
        );
        fire_webhooks(
            pool,
            account_id,
            "account.needs_reauth",
            serde_json::json!({ "reason": reason }),
        )
        .await;
    }
    Ok(flagged)
}

/// Clear the flag after a reconnect stored a fresh grant. Emits
/// `account_reauthed` only when the account was flagged.
pub async fn clear_needs_reauth(
    pool: &SqlitePool,
    event_tx: Option<&broadcast::Sender<String>>,
    account_id: &str,
) -> Result<bool> {
    let cleared = sqlx::query(
        "UPDATE accounts SET needs_reauth = 0, reauth_reason = NULL WHERE id = ? AND needs_reauth = 1",
    )
    .bind(account_id)
    .execute(pool)
    .await?
    .rows_affected()
        > 0;
    if cleared {
        send(
            event_tx,
            &SyncEvent::AccountReauthed {
                account_id: account_id.to_string(),
            },
        );
    }
    Ok(cleared)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_account() -> SqlitePool {
        let (pool, _) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a','A','a@example.test','gmail','oauth2')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[test]
    fn only_invalid_grant_counts_as_a_revoked_grant() {
        assert!(is_revoked_grant(&AppError::Auth(
            "OAuth refresh rejected (invalid_grant: Token has been expired or revoked.); reconnect the account".into()
        )));
        assert!(!is_revoked_grant(&AppError::Auth(
            "account has no OAuth token".into()
        )));
        assert!(!is_revoked_grant(&AppError::Provider(
            "invalid_grant".into()
        )));
    }

    #[tokio::test]
    async fn mark_emits_one_event_and_clear_emits_reauthed() {
        let pool = pool_with_account().await;
        let (tx, mut rx) = broadcast::channel(8);
        assert!(mark_needs_reauth(&pool, Some(&tx), "a", "invalid_grant")
            .await
            .unwrap());
        assert!(!mark_needs_reauth(&pool, Some(&tx), "a", "invalid_grant")
            .await
            .unwrap());
        assert!(needs_reauth(&pool, "a").await.unwrap());
        let event: SyncEvent = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(
            event,
            SyncEvent::AccountNeedsReauth {
                account_id: "a".into(),
                reason: Some("invalid_grant".into())
            }
        );
        assert!(rx.try_recv().is_err(), "a repeat refusal emits nothing");

        assert!(clear_needs_reauth(&pool, Some(&tx), "a").await.unwrap());
        assert!(!needs_reauth(&pool, "a").await.unwrap());
        let event: SyncEvent = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(
            event,
            SyncEvent::AccountReauthed {
                account_id: "a".into()
            }
        );
        assert!(!clear_needs_reauth(&pool, Some(&tx), "a").await.unwrap());
        assert!(rx.try_recv().is_err());
    }
}
