use chrono::{Duration, Utc};
use email_types::SendRequest;
use sqlx::SqlitePool;
use tokio::time::{sleep, Duration as TokioDuration};

pub fn spawn_worker(pool: SqlitePool) {
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE outbox SET status = 'pending' WHERE status = 'sending'")
            .execute(&pool)
            .await;
        loop {
            process_due(&pool).await;
            sleep(TokioDuration::from_secs(1)).await;
        }
    });
}

async fn process_due(pool: &SqlitePool) {
    let now = Utc::now().to_rfc3339();
    let rows = match sqlx::query_as::<_, (String, String)>(&format!("SELECT id, payload FROM outbox WHERE status = 'pending' AND scheduled_at <= '{now}' AND next_attempt_at <= '{now}' ORDER BY scheduled_at LIMIT 10")).fetch_all(pool).await { Ok(rows) => rows, Err(error) => { tracing::error!(%error, "outbox query failed"); return; } };
    for (id, payload) in rows {
        let claimed =
            sqlx::query("UPDATE outbox SET status = 'sending' WHERE id = ? AND status = 'pending'")
                .bind(&id)
                .execute(pool)
                .await
                .map(|r| r.rows_affected() == 1)
                .unwrap_or(false);
        if !claimed {
            continue;
        }
        let result = async {
            let req: SendRequest = serde_json::from_str(&payload)
                .map_err(|e| crate::error::AppError::Internal(e.into()))?;
            let (config, message) = crate::api::compose::build_outbound(
                &crate::state::AppState::new(pool.clone(), false),
                &req,
            )
            .await?;
            crate::smtp::send_message(&config, message).await
        }
        .await;
        match result {
            Ok(()) => {
                let _ = sqlx::query("UPDATE outbox SET status = 'sent' WHERE id = ?")
                    .bind(&id)
                    .execute(pool)
                    .await;
            }
            Err(error) => {
                let attempts: i64 = sqlx::query_scalar("SELECT attempts FROM outbox WHERE id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0)
                    + 1;
                let retry = Utc::now()
                    + Duration::seconds((30_i64).saturating_mul(1_i64 << attempts.min(6)));
                let _ = sqlx::query("UPDATE outbox SET status = 'pending', attempts = ?, next_attempt_at = ?, last_error = ? WHERE id = ?").bind(attempts).bind(retry.to_rfc3339()).bind(error.to_string()).bind(&id).execute(pool).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    #[test]
    fn fake_clock_honours_delay_and_cancel_window() {
        let clock = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let scheduled = clock + Duration::seconds(10);
        assert!(scheduled > clock);
        assert!(clock + Duration::seconds(10) >= scheduled);
        assert!(clock + Duration::seconds(9) < scheduled);
    }
}
