use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use email_types::SendRequest;
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

pub async fn enqueue(state: &AppState, req: SendRequest) -> Result<serde_json::Value> {
    let now = Utc::now();
    let scheduled = match req.send_at.as_deref() {
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map_err(|_| AppError::Validation("sendAt must be RFC3339".into()))?
            .with_timezone(&Utc),
        None => now + Duration::seconds(req.send_delay_seconds.unwrap_or(10) as i64),
    };
    if scheduled < now {
        return Err(AppError::Validation("sendAt cannot be in the past".into()));
    }
    let id = Uuid::new_v4().to_string();
    let payload = serde_json::to_string(&req).map_err(|e| AppError::Internal(e.into()))?;
    let cancel_until = now + Duration::seconds(req.send_delay_seconds.unwrap_or(10) as i64);
    sqlx::query("INSERT INTO outbox (id, account_id, payload, scheduled_at, cancel_until, next_attempt_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id).bind(&req.account_id).bind(payload).bind(scheduled.to_rfc3339())
        .bind(cancel_until.to_rfc3339()).bind(scheduled.to_rfc3339()).execute(&state.pool).await?;
    Ok(
        serde_json::json!({"id": id, "status": "pending", "scheduledAt": scheduled, "cancelUntil": cancel_until}),
    )
}

pub async fn list_outbox(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>> {
    let rows = sqlx::query_as::<_, (String, String, String, String, i64, Option<String>)>(
        "SELECT id, status, scheduled_at, cancel_until, attempts, last_error FROM outbox ORDER BY scheduled_at DESC")
        .fetch_all(&state.pool).await?;
    Ok(Json(rows.into_iter().map(|(id,status,scheduled_at,cancel_until,attempts,last_error)| serde_json::json!({"id":id,"status":status,"scheduledAt":scheduled_at,"cancelUntil":cancel_until,"attempts":attempts,"lastError":last_error})).collect()))
}

pub async fn cancel_outbox(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query("UPDATE outbox SET status = 'cancelled' WHERE id = ? AND status = 'pending' AND cancel_until >= ?")
        .bind(&id).bind(now).execute(&state.pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "outbox item is already sent or outside the undo window".into(),
        ));
    }
    Ok(Json(serde_json::json!({"id": id, "status": "cancelled"})))
}
