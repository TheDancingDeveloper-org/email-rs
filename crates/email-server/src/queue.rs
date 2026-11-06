//! Durable, ordered queue for IMAP mutations made while an account is offline.
use crate::error::{AppError, Result};
use crate::providers::factory::provider_for_account;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

const MAX_ATTEMPTS: i64 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Flag {
        folder: String,
        uid: u32,
        flagged: bool,
    },
    Move {
        folder: String,
        uid: u32,
        destination: String,
    },
    Delete {
        folder: String,
        uid: u32,
    },
}

impl Action {
    fn folder(&self) -> &str {
        match self {
            Action::Flag { folder, .. }
            | Action::Move { folder, .. }
            | Action::Delete { folder, .. } => folder,
        }
    }

    fn uid(&self) -> u32 {
        match self {
            Action::Flag { uid, .. } | Action::Move { uid, .. } | Action::Delete { uid, .. } => {
                *uid
            }
        }
    }
}

/// Queue an action, recording the UIDVALIDITY its UID belongs to (the cached
/// value for the folder) so a replay after a mailbox reset can tell.
pub async fn enqueue(pool: &SqlitePool, account_id: &str, action: &Action) -> Result<()> {
    let payload = serde_json::to_string(action).map_err(|e| AppError::Internal(e.into()))?;
    let uid_validity: Option<i64> = sqlx::query_scalar(
        "SELECT uid_validity FROM folders WHERE account_id = ? AND full_path = ?",
    )
    .bind(account_id)
    .bind(action.folder())
    .fetch_optional(pool)
    .await?;
    sqlx::query(
        "INSERT INTO imap_action_queue (account_id, action, uid_validity) VALUES (?, ?, ?)",
    )
    .bind(account_id)
    .bind(payload)
    .bind(uid_validity)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn pending_count(pool: &SqlitePool, account_id: &str) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM imap_action_queue WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(pool)
            .await?,
    )
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    action: String,
    attempts: i64,
    uid_validity: Option<i64>,
}

/// Replay one account's actions in insertion order. A transient failure stops the
/// account so later actions cannot overtake it; permanent failures are discarded.
/// An unreachable provider stops replay without consuming any attempt: only
/// attempts made while a connection could be opened count against the budget.
pub async fn replay(pool: &SqlitePool, account_id: &str) -> Result<()> {
    let rows: Vec<Row> = sqlx::query_as("SELECT id, action, attempts, uid_validity FROM imap_action_queue WHERE account_id = ? ORDER BY id LIMIT 32")
        .bind(account_id).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let provider = provider_for_account(pool, account_id).await?;
    if let Err(error) = provider.check_connection().await {
        if error.is_transient() {
            return Ok(());
        }
        return Err(error);
    }
    for row in rows {
        let action: Action =
            serde_json::from_str(&row.action).map_err(|e| AppError::Internal(e.into()))?;
        if let Some(queued) = row.uid_validity {
            match provider.folder_uid_validity(action.folder()).await {
                Ok(Some(current)) if current as i64 != queued => {
                    drop_action(pool, row.id).await?;
                    tracing::warn!(
                        account_id,
                        folder = action.folder(),
                        uid = action.uid(),
                        queued,
                        current,
                        "dropped offline action: UIDVALIDITY changed"
                    );
                    record_queue_error(
                        pool,
                        account_id,
                        "offline action dropped: mailbox UIDVALIDITY changed",
                    )
                    .await?;
                    continue;
                }
                Ok(_) => {}
                Err(error) if error.is_transient() => {
                    note_transient(pool, &row, &error).await?;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        let result = match action {
            Action::Flag {
                folder,
                uid,
                flagged,
            } => provider
                .set_flagged(&folder, uid, flagged)
                .await
                .map(|_| ()),
            Action::Move {
                folder,
                uid,
                destination,
            } => provider
                .move_message(&folder, uid, &destination)
                .await
                .map(|_| ()),
            Action::Delete { folder, uid } => provider.delete_message(&folder, uid).await,
        };
        match result {
            Ok(()) => drop_action(pool, row.id).await?,
            Err(error) if error.is_transient() => {
                if row.attempts + 1 < MAX_ATTEMPTS {
                    note_transient(pool, &row, &error).await?;
                } else {
                    drop_action(pool, row.id).await?;
                    tracing::warn!(account_id, %error, "dropped offline action: retries exhausted");
                    record_queue_error(pool, account_id, "offline action retries exhausted")
                        .await?;
                }
                break;
            }
            Err(error) => {
                drop_action(pool, row.id).await?;
                tracing::warn!(account_id, %error, "dropped offline action: rejected by provider");
                record_queue_error(pool, account_id, "offline action rejected by provider").await?;
            }
        }
    }
    Ok(())
}

async fn drop_action(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM imap_action_queue WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn note_transient(pool: &SqlitePool, row: &Row, error: &AppError) -> Result<()> {
    sqlx::query(
        "UPDATE imap_action_queue SET attempts = attempts + 1, last_error = ? WHERE id = ?",
    )
    .bind(error.to_string())
    .bind(row.id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn record_queue_error(pool: &SqlitePool, account_id: &str, message: &str) -> Result<()> {
    sqlx::query("INSERT INTO account_sync_health (account_id, last_error_at, last_error) VALUES (?, datetime('now'), ?) ON CONFLICT(account_id) DO UPDATE SET last_error_at = excluded.last_error_at, last_error = excluded.last_error")
        .bind(account_id).bind(message).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::factory::TEST_FOLDER_CALLS;

    async fn pool() -> SqlitePool {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = dir.path().join("email.db").display().to_string();
        crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap()
            .0
    }

    async fn account(pool: &SqlitePool, id: &str, provider_type: &str) {
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?, 'T', ?, ?, 'test')",
        )
        .bind(id)
        .bind(format!("{id}@example.com"))
        .bind(provider_type)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn folder(pool: &SqlitePool, account_id: &str, path: &str, uid_validity: i64) {
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path, uid_validity) VALUES (?, ?, ?, ?, ?)")
            .bind(format!("{account_id}-{path}"))
            .bind(account_id)
            .bind(path)
            .bind(path)
            .bind(uid_validity)
            .execute(pool)
            .await
            .unwrap();
    }

    fn flag(folder: &str, uid: u32) -> Action {
        Action::Flag {
            folder: folder.to_string(),
            uid,
            flagged: true,
        }
    }

    fn calls(marker: &str) -> Vec<String> {
        TEST_FOLDER_CALLS
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.contains(marker))
            .cloned()
            .collect()
    }

    async fn remaining(pool: &SqlitePool, account_id: &str) -> Vec<(i64, Option<String>)> {
        sqlx::query_as(
            "SELECT attempts, last_error FROM imap_action_queue WHERE account_id = ? ORDER BY id",
        )
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    async fn health_error(pool: &SqlitePool, account_id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT last_error FROM account_sync_health WHERE account_id = ?")
            .bind(account_id)
            .fetch_optional(pool)
            .await
            .unwrap()
            .flatten()
    }

    #[tokio::test]
    async fn replay_runs_actions_in_insertion_order_and_empties_the_queue() {
        let pool = pool().await;
        account(&pool, "ordered", "test").await;
        for uid in [30, 10, 20] {
            enqueue(&pool, "ordered", &flag("ordered-box", uid))
                .await
                .unwrap();
        }
        replay(&pool, "ordered").await.unwrap();
        assert_eq!(
            calls("ordered-box"),
            [
                "flag ordered-box 30 true",
                "flag ordered-box 10 true",
                "flag ordered-box 20 true"
            ]
        );
        assert_eq!(pending_count(&pool, "ordered").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn transient_failure_mid_replay_stops_so_later_actions_cannot_overtake() {
        let pool = pool().await;
        account(&pool, "midway", "test_flaky").await;
        for uid in [12, 13, 14] {
            enqueue(&pool, "midway", &flag("midway-box", uid))
                .await
                .unwrap();
        }
        replay(&pool, "midway").await.unwrap();
        assert_eq!(
            calls("midway-box"),
            ["flag midway-box 12 true", "flag midway-box 13 true"]
        );
        let left = remaining(&pool, "midway").await;
        assert_eq!(left.len(), 2);
        assert_eq!(left[0].0, 1);
        assert!(left[0]
            .1
            .as_deref()
            .unwrap()
            .contains("injected network failure"));
        assert_eq!(left[1].0, 0);
    }

    #[tokio::test]
    async fn retry_budget_is_not_consumed_while_the_provider_is_unreachable() {
        let pool = pool().await;
        account(&pool, "offline", "test_offline").await;
        enqueue(&pool, "offline", &flag("offline-box", 1))
            .await
            .unwrap();
        for _ in 0..(MAX_ATTEMPTS + 2) {
            replay(&pool, "offline").await.unwrap();
        }
        assert_eq!(remaining(&pool, "offline").await, [(0, None)]);
        assert!(calls("offline-box").is_empty());
    }

    #[tokio::test]
    async fn retry_budget_counts_attempts_made_while_reachable_then_drops_the_action() {
        let pool = pool().await;
        account(&pool, "budget", "test_flaky").await;
        enqueue(&pool, "budget", &flag("budget-box", 13))
            .await
            .unwrap();
        for _ in 0..(MAX_ATTEMPTS - 1) {
            replay(&pool, "budget").await.unwrap();
        }
        assert_eq!(pending_count(&pool, "budget").await.unwrap(), 1);
        replay(&pool, "budget").await.unwrap();
        assert_eq!(pending_count(&pool, "budget").await.unwrap(), 0);
        let error = health_error(&pool, "budget").await.unwrap();
        // The health row holds a safe category, never the provider's text.
        assert_eq!(error, "offline action retries exhausted");
    }

    #[tokio::test]
    async fn permanent_failure_drops_the_action_and_records_the_provider_error() {
        let pool = pool().await;
        account(&pool, "rejected", "test_fail").await;
        for uid in [1, 2] {
            enqueue(&pool, "rejected", &flag("rejected-box", uid))
                .await
                .unwrap();
        }
        replay(&pool, "rejected").await.unwrap();
        // Both are rejected and discarded; a permanent error does not stop replay.
        assert_eq!(calls("rejected-box").len(), 2);
        assert_eq!(pending_count(&pool, "rejected").await.unwrap(), 0);
        let error = health_error(&pool, "rejected").await.unwrap();
        assert_eq!(error, "offline action rejected by provider");
    }

    #[tokio::test]
    async fn uidvalidity_change_drops_the_action_instead_of_acting_on_the_new_uid() {
        let pool = pool().await;
        account(&pool, "reset", "test").await;
        // The test provider reports UIDVALIDITY 1; the action was queued under 5.
        folder(&pool, "reset", "reset-box", 5).await;
        enqueue(&pool, "reset", &flag("reset-box", 7))
            .await
            .unwrap();
        let stored: Option<i64> = sqlx::query_scalar(
            "SELECT uid_validity FROM imap_action_queue WHERE account_id = 'reset'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored, Some(5));
        replay(&pool, "reset").await.unwrap();
        assert!(calls("reset-box").is_empty());
        assert_eq!(pending_count(&pool, "reset").await.unwrap(), 0);
        let error = health_error(&pool, "reset").await.unwrap();
        assert_eq!(error, "offline action dropped: mailbox UIDVALIDITY changed");
    }

    #[tokio::test]
    async fn matching_uidvalidity_replays_the_action() {
        let pool = pool().await;
        account(&pool, "same", "test").await;
        folder(&pool, "same", "same-box", 1).await;
        enqueue(&pool, "same", &flag("same-box", 7)).await.unwrap();
        replay(&pool, "same").await.unwrap();
        assert_eq!(calls("same-box"), ["flag same-box 7 true"]);
    }

    #[tokio::test]
    async fn deleting_the_account_cascades_its_queue() {
        let pool = pool().await;
        account(&pool, "gone", "test").await;
        account(&pool, "kept", "test").await;
        enqueue(&pool, "gone", &flag("gone-box", 1)).await.unwrap();
        enqueue(&pool, "kept", &flag("kept-box", 1)).await.unwrap();
        sqlx::query("DELETE FROM accounts WHERE id = 'gone'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(pending_count(&pool, "gone").await.unwrap(), 0);
        assert_eq!(pending_count(&pool, "kept").await.unwrap(), 1);
    }

    #[test]
    fn only_the_typed_transient_variant_is_transient() {
        assert!(AppError::Transient("x".into()).is_transient());
        // Error text that merely mentions a connection is a rejection.
        assert!(!AppError::Imap("connection refused by policy".into()).is_transient());
        assert!(!AppError::Provider("timeout".into()).is_transient());
    }
}
