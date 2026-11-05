use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use email_types::{CancelOutboxResponse, OutboxItem, SendRequest, SendResponse};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

const DEFAULT_DELAY_SECONDS: u64 = 10;
const MAX_DELAY_SECONDS: u64 = 3600;

pub async fn enqueue(state: &AppState, req: SendRequest) -> Result<SendResponse> {
    enqueue_at(&state.pool, req, Utc::now()).await
}

/// Queues `req`. The undo window closes when the send is due, so a message
/// scheduled with `sendAt` stays cancellable until then.
pub(crate) async fn enqueue_at(
    pool: &SqlitePool,
    req: SendRequest,
    now: DateTime<Utc>,
) -> Result<SendResponse> {
    let scheduled = match req.send_at.as_deref() {
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map_err(|_| AppError::Validation("sendAt must be RFC3339".into()))?
            .with_timezone(&Utc),
        None => {
            let delay = req.send_delay_seconds.unwrap_or(DEFAULT_DELAY_SECONDS);
            if delay > MAX_DELAY_SECONDS {
                return Err(AppError::Validation(format!(
                    "sendDelaySeconds cannot exceed {MAX_DELAY_SECONDS}"
                )));
            }
            now + Duration::seconds(delay as i64)
        }
    };
    if scheduled < now {
        return Err(AppError::Validation("sendAt cannot be in the past".into()));
    }
    let id = Uuid::new_v4().to_string();
    let payload = serde_json::to_string(&req).map_err(|e| AppError::Internal(e.into()))?;
    let scheduled_at = scheduled.to_rfc3339();
    sqlx::query("INSERT INTO outbox (id, account_id, payload, scheduled_at, cancel_until, next_attempt_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id).bind(&req.account_id).bind(payload).bind(&scheduled_at)
        .bind(&scheduled_at).bind(&scheduled_at).execute(pool).await?;
    Ok(SendResponse {
        status: "queued".to_string(),
        id: Some(id),
        scheduled_at: Some(scheduled_at.clone()),
        cancel_until: Some(scheduled_at),
    })
}

pub async fn list_outbox(State(state): State<Arc<AppState>>) -> Result<Json<Vec<OutboxItem>>> {
    let rows = sqlx::query_as::<_, (String, String, String, String, i64, Option<String>)>(
        "SELECT id, status, scheduled_at, cancel_until, attempts, last_error FROM outbox ORDER BY scheduled_at DESC")
        .fetch_all(&state.pool).await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, status, scheduled_at, cancel_until, attempts, last_error)| OutboxItem {
                    id,
                    status,
                    scheduled_at,
                    cancel_until,
                    attempts,
                    last_error,
                },
            )
            .collect(),
    ))
}

pub async fn cancel_outbox(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<CancelOutboxResponse>> {
    cancel_at(&state.pool, id, Utc::now()).await.map(Json)
}

pub(crate) async fn cancel_at(
    pool: &SqlitePool,
    id: String,
    now: DateTime<Utc>,
) -> Result<CancelOutboxResponse> {
    let result = sqlx::query("UPDATE outbox SET status = 'cancelled' WHERE id = ? AND status = 'pending' AND cancel_until >= ?")
        .bind(&id).bind(now.to_rfc3339()).execute(pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "outbox item is already sent or outside the undo window".into(),
        ));
    }
    Ok(CancelOutboxResponse {
        id,
        status: "cancelled".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(delay: Option<u64>, send_at: Option<String>) -> SendRequest {
        SendRequest {
            account_id: "a1".into(),
            to: vec!["to@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "hi".into(),
            text_body: Some("body".into()),
            html_body: None,
            in_reply_to: None,
            attachments: vec![],
            send_at,
            send_delay_seconds: delay,
        }
    }

    async fn pool() -> SqlitePool {
        crate::db::create_pool("sqlite::memory:").await.unwrap().0
    }

    async fn status(pool: &SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT status FROM outbox WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn enqueue_returns_id_and_window_and_cancel_inside_it_succeeds() {
        let pool = pool().await;
        let now = Utc::now();
        let queued = enqueue_at(&pool, request(Some(10), None), now)
            .await
            .unwrap();
        let id = queued.id.clone().unwrap();
        assert_eq!(queued.status, "queued");
        assert_eq!(queued.scheduled_at, queued.cancel_until);
        let due = DateTime::parse_from_rfc3339(queued.scheduled_at.as_deref().unwrap()).unwrap();
        assert_eq!(due.with_timezone(&Utc), now + Duration::seconds(10));

        let cancelled = cancel_at(&pool, id.clone(), now + Duration::seconds(9))
            .await
            .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(status(&pool, &id).await, "cancelled");
    }

    #[tokio::test]
    async fn cancel_after_window_or_twice_or_once_sending_conflicts() {
        let pool = pool().await;
        let now = Utc::now();
        let late = enqueue_at(&pool, request(Some(10), None), now)
            .await
            .unwrap();
        let late_id = late.id.unwrap();
        let err = cancel_at(&pool, late_id.clone(), now + Duration::seconds(11))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        assert_eq!(status(&pool, &late_id).await, "pending");

        let twice = enqueue_at(&pool, request(Some(10), None), now)
            .await
            .unwrap();
        let twice_id = twice.id.unwrap();
        cancel_at(&pool, twice_id.clone(), now).await.unwrap();
        assert!(matches!(
            cancel_at(&pool, twice_id, now).await.unwrap_err(),
            AppError::Conflict(_)
        ));

        let sending = enqueue_at(&pool, request(Some(10), None), now)
            .await
            .unwrap();
        let sending_id = sending.id.unwrap();
        sqlx::query("UPDATE outbox SET status = 'sending' WHERE id = ?")
            .bind(&sending_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            cancel_at(&pool, sending_id, now).await.unwrap_err(),
            AppError::Conflict(_)
        ));
        assert!(matches!(
            cancel_at(&pool, "missing".into(), now).await.unwrap_err(),
            AppError::Conflict(_)
        ));
    }

    #[tokio::test]
    async fn send_at_keeps_the_item_cancellable_until_it_is_due() {
        let pool = pool().await;
        let now = Utc::now();
        let at = (now + Duration::hours(2)).to_rfc3339();
        let queued = enqueue_at(&pool, request(Some(10), Some(at)), now)
            .await
            .unwrap();
        cancel_at(&pool, queued.id.unwrap(), now + Duration::hours(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn invalid_schedules_are_rejected_without_queueing() {
        let pool = pool().await;
        let now = Utc::now();
        for req in [
            request(None, Some("tomorrow".into())),
            request(None, Some((now - Duration::seconds(5)).to_rfc3339())),
            request(Some(MAX_DELAY_SECONDS + 1), None),
        ] {
            assert!(matches!(
                enqueue_at(&pool, req, now).await.unwrap_err(),
                AppError::Validation(_)
            ));
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        // zero delay is allowed: send immediately, no undo.
        enqueue_at(&pool, request(Some(0), None), now)
            .await
            .unwrap();
    }
}
