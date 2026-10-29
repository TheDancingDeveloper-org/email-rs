//! Full-text search over the clean message.
//!
//! `messages_fts` (see `db::run_fts_migration`) indexes the subject, sender and
//! recipients of every message through triggers on `messages`. Its `body` and
//! `quoted` columns come from the clean body and are written here:
//! when a body is cleaned ([`crate::clean::store`]) and, for bodies cleaned
//! before this index existed, by a bounded background reindex.

use sqlx::{SqliteConnection, SqlitePool};

/// Bump when the indexed text of a body changes; rows below it are reindexed.
pub const SEARCH_VERSION: i64 = 1;

/// Rows reindexed per write transaction.
const BATCH: i64 = 100;

/// Write the clean body of one message into its search-index row and record
/// the index version on the body. Runs inside the caller's write transaction
/// (`db::begin_write`). Without an FTS5 index only the version is recorded;
/// a new index resets it.
pub async fn index_body(
    conn: &mut SqliteConnection,
    message_id: &str,
    body: &str,
    quoted_history: Option<&str>,
) -> sqlx::Result<()> {
    let has_fts: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'messages_fts'",
    )
    .fetch_optional(&mut *conn)
    .await?;
    if has_fts.is_some() {
        sqlx::query(
            r#"UPDATE messages_fts SET body = ?, quoted = ?
               WHERE rowid = (SELECT rowid FROM messages WHERE id = ?)"#,
        )
        .bind(crate::clean::search_text(body))
        .bind(
            quoted_history
                .map(crate::clean::search_text)
                .unwrap_or_default(),
        )
        .bind(message_id)
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query("UPDATE message_bodies SET search_version = ? WHERE message_id = ?")
        .bind(SEARCH_VERSION)
        .bind(message_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct PendingIndex {
    message_id: String,
    clean_body: Option<String>,
    quoted_history: Option<String>,
}

/// Index every clean body whose search-index row is older than this version,
/// in small write transactions so sync and API writers keep getting the write
/// lock. Bodies not cleaned yet are left to `clean::backfill`, which indexes
/// them as it cleans them.
pub async fn reindex(pool: &SqlitePool) -> sqlx::Result<usize> {
    let mut done = 0;
    loop {
        let batch: Vec<PendingIndex> = sqlx::query_as(
            r#"SELECT message_id, clean_body, quoted_history FROM message_bodies
               WHERE search_version < ? AND clean_version > 0 LIMIT ?"#,
        )
        .bind(SEARCH_VERSION)
        .bind(BATCH)
        .fetch_all(pool)
        .await?;
        if batch.is_empty() {
            return Ok(done);
        }
        let mut tx = crate::db::begin_write(pool).await?;
        for row in &batch {
            index_body(
                &mut tx,
                &row.message_id,
                row.clean_body.as_deref().unwrap_or_default(),
                row.quoted_history.as_deref(),
            )
            .await?;
        }
        tx.commit().await?;
        done += batch.len();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Run [`reindex`] in the background after startup.
pub fn spawn_reindex(pool: SqlitePool) {
    tokio::spawn(async move {
        match reindex(&pool).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("indexed {n} clean message bodies for search"),
            Err(e) => tracing::warn!("search reindex stopped: {e}"),
        }
    });
}
