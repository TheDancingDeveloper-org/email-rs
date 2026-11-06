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

pub fn is_transient(error: &AppError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    [
        "connection",
        "timed out",
        "timeout",
        "broken pipe",
        "reset by peer",
        "unreachable",
        "temporarily unavailable",
        "closed",
    ]
    .iter()
    .any(|s| text.contains(s))
}

pub async fn enqueue(pool: &SqlitePool, account_id: &str, action: &Action) -> Result<()> {
    let payload = serde_json::to_string(action).map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query("INSERT INTO imap_action_queue (account_id, action) VALUES (?, ?)")
        .bind(account_id)
        .bind(payload)
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
}

/// Replay one account's actions in insertion order. A transient failure stops the
/// account so later actions cannot overtake it; permanent failures are discarded.
pub async fn replay(pool: &SqlitePool, account_id: &str) -> Result<()> {
    let rows: Vec<Row> = sqlx::query_as("SELECT id, action, attempts FROM imap_action_queue WHERE account_id = ? ORDER BY id LIMIT 32")
        .bind(account_id).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let provider = provider_for_account(pool, account_id).await?;
    for row in rows {
        let action: Action =
            serde_json::from_str(&row.action).map_err(|e| AppError::Internal(e.into()))?;
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
            Ok(()) => {
                sqlx::query("DELETE FROM imap_action_queue WHERE id = ?")
                    .bind(row.id)
                    .execute(pool)
                    .await?;
            }
            Err(error) if is_transient(&error) && row.attempts + 1 < MAX_ATTEMPTS => {
                sqlx::query("UPDATE imap_action_queue SET attempts = attempts + 1, last_error = ? WHERE id = ?")
                    .bind("provider unavailable").bind(row.id).execute(pool).await?;
                break;
            }
            Err(error) if is_transient(&error) => {
                sqlx::query("DELETE FROM imap_action_queue WHERE id = ?")
                    .bind(row.id)
                    .execute(pool)
                    .await?;
                record_queue_error(pool, account_id, "offline action retries exhausted").await?;
                break;
            }
            Err(_) => {
                sqlx::query("DELETE FROM imap_action_queue WHERE id = ?")
                    .bind(row.id)
                    .execute(pool)
                    .await?;
                record_queue_error(pool, account_id, "offline action rejected by provider").await?;
            }
        }
    }
    Ok(())
}

async fn record_queue_error(pool: &SqlitePool, account_id: &str, message: &str) -> Result<()> {
    sqlx::query("INSERT INTO account_sync_health (account_id, last_error_at, last_error) VALUES (?, datetime('now'), ?) ON CONFLICT(account_id) DO UPDATE SET last_error_at = excluded.last_error_at, last_error = excluded.last_error")
        .bind(account_id).bind(message).execute(pool).await?;
    Ok(())
}
