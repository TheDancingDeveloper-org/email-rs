use std::time::Duration;

use sqlx::SqlitePool;

use crate::config::MaintenanceConfig;

const BATCH_SIZE: i64 = 500;

pub fn spawn(pool: SqlitePool, has_fts: bool, config: MaintenanceConfig) {
    tokio::spawn(async move {
        let cleanup_pool = pool.clone();
        let cleanup =
            run_periodically(cleanup_pool.clone(), config.orphan_cleanup_interval, || {
                let pool = cleanup_pool.clone();
                async move {
                    let removed = cleanup_orphans(&pool).await?;
                    if removed > 0 {
                        tracing::info!(removed, "removed orphaned message bodies and attachments");
                    }
                    Ok::<(), sqlx::Error>(())
                }
            });
        let vacuum_pool = pool.clone();
        let vacuum = run_periodically(vacuum_pool.clone(), config.vacuum_interval, || {
            let pool = vacuum_pool.clone();
            async move {
                sqlx::query("PRAGMA incremental_vacuum(1000)")
                    .execute(&pool)
                    .await?;
                Ok::<(), sqlx::Error>(())
            }
        });
        let fts_pool = pool.clone();
        let fts = async move {
            if has_fts {
                let operation_pool = fts_pool.clone();
                run_periodically(fts_pool, config.fts_optimize_interval, || {
                    let pool = operation_pool.clone();
                    async move {
                        sqlx::query("INSERT INTO messages_fts(messages_fts) VALUES ('optimize')")
                            .execute(&pool)
                            .await?;
                        Ok::<(), sqlx::Error>(())
                    }
                })
                .await;
            }
        };
        tokio::join!(cleanup, vacuum, fts);
    });
}

async fn run_periodically<F, Fut>(pool: SqlitePool, interval: Duration, mut operation: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), sqlx::Error>>,
{
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if let Err(error) = operation().await {
            tracing::warn!(%error, "database maintenance operation failed");
        }
        if pool.is_closed() {
            return;
        }
    }
}

pub async fn cleanup_orphans(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let mut removed = 0;
    loop {
        let mut tx = crate::db::begin_write(pool).await?;
        let bodies = sqlx::query(
            "DELETE FROM message_bodies WHERE message_id IN (
                SELECT b.message_id FROM message_bodies b
                WHERE NOT EXISTS (SELECT 1 FROM messages m WHERE m.id = b.message_id)
                LIMIT ?
            )",
        )
        .bind(BATCH_SIZE)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let attachments = sqlx::query(
            "DELETE FROM attachments WHERE id IN (
                SELECT a.id FROM attachments a
                WHERE NOT EXISTS (SELECT 1 FROM messages m WHERE m.id = a.message_id)
                LIMIT ?
            )",
        )
        .bind(BATCH_SIZE)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        removed += bodies + attachments;
        if bodies + attachments == 0 {
            if let Some(dir) = crate::attachment_store::store_dir(pool).await {
                removed += crate::attachment_store::remove_unreferenced(pool, &dir).await?;
            }
            return Ok(removed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cleanup_removes_orphans_but_keeps_message_data() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE messages (id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE message_bodies (message_id TEXT PRIMARY KEY, text_body TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE attachments (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, filename TEXT NOT NULL, content_type TEXT NOT NULL, part_id TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO messages (id) VALUES ('m')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO message_bodies (message_id, text_body) VALUES ('m', 'kept'), ('orphan', 'removed')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO attachments (id, message_id, filename, content_type, part_id) VALUES ('kept', 'm', 'a', 'text/plain', '1'), ('orphan', 'missing', 'b', 'text/plain', '1')").execute(&pool).await.unwrap();

        assert_eq!(cleanup_orphans(&pool).await.unwrap(), 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM message_bodies")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
}
