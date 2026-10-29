use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use email_types::{CreateWebhook, UpdateWebhook, Webhook as WebhookRow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::state::AppState;

pub async fn list_webhooks(State(state): State<Arc<AppState>>) -> Result<Json<Vec<WebhookRow>>> {
    let rows = sqlx::query_as::<_, WebhookRow>(
        "SELECT id, url, secret, events, account_id, enabled, created_at FROM webhooks ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

pub async fn create_webhook(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWebhook>,
) -> Result<Json<WebhookRow>> {
    let id = Uuid::new_v4().to_string();
    let events = body.events.unwrap_or_else(|| "message.created".to_string());
    validate_events(&events)?;

    sqlx::query(
        r#"INSERT INTO webhooks (id, url, secret, events, account_id)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&body.url)
    .bind(&body.secret)
    .bind(&events)
    .bind(&body.account_id)
    .execute(&state.pool)
    .await?;

    let row = sqlx::query_as::<_, WebhookRow>(
        "SELECT id, url, secret, events, account_id, enabled, created_at FROM webhooks WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

pub async fn update_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWebhook>,
) -> Result<Json<WebhookRow>> {
    if let Some(ref url) = body.url {
        sqlx::query("UPDATE webhooks SET url = ? WHERE id = ?")
            .bind(url)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref secret) = body.secret {
        sqlx::query("UPDATE webhooks SET secret = ? WHERE id = ?")
            .bind(secret)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref events) = body.events {
        validate_events(events)?;
        sqlx::query("UPDATE webhooks SET events = ? WHERE id = ?")
            .bind(events)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(enabled) = body.enabled {
        sqlx::query("UPDATE webhooks SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    let row = sqlx::query_as::<_, WebhookRow>(
        "SELECT id, url, secret, events, account_id, enabled, created_at FROM webhooks WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("webhook {} not found", id)))?;

    Ok(Json(row))
}

pub async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query("DELETE FROM webhooks WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("webhook {} not found", id)));
    }
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

const MAX_DELIVERY_ATTEMPTS: i64 = 8;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationEvent {
    pub id: String,
    pub event_type: String,
    pub account_id: String,
    #[sqlx(json)]
    pub data: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookDelivery {
    pub event_id: String,
    pub webhook_id: String,
    pub status: String,
    pub attempts: i64,
    pub next_attempt_at: String,
    pub last_attempt_at: Option<String>,
    pub response_status: Option<i64>,
    pub last_error: Option<String>,
    pub delivered_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebhookPayload<'a> {
    id: &'a str,
    event: &'a str,
    timestamp: &'a str,
    account_id: &'a str,
    data: &'a serde_json::Value,
}

#[derive(Debug, sqlx::FromRow)]
struct PendingDelivery {
    event_id: String,
    webhook_id: String,
    event_type: String,
    account_id: String,
    data_json: String,
    created_at: String,
    url: String,
    secret: Option<String>,
    attempts: i64,
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<EventQuery>,
) -> Result<Json<Vec<IntegrationEvent>>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let events = if let Some(after) = query.after {
        sqlx::query_as::<_, IntegrationEvent>(
            r#"SELECT id, event_type, account_id, data_json AS data, created_at
               FROM webhook_events
               WHERE (created_at, id) > (
                   SELECT created_at, id FROM webhook_events WHERE id = ?
               )
               ORDER BY created_at, id LIMIT ?"#,
        )
        .bind(after)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, IntegrationEvent>(
            r#"SELECT id, event_type, account_id, data_json AS data, created_at
               FROM webhook_events ORDER BY created_at, id LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(events))
}

pub async fn list_deliveries(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<WebhookDelivery>>> {
    let rows = sqlx::query_as::<_, WebhookDelivery>(
        r#"SELECT event_id, webhook_id, status, attempts, next_attempt_at,
                  last_attempt_at, response_status, last_error, delivered_at
           FROM webhook_deliveries ORDER BY next_attempt_at DESC LIMIT 500"#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

/// Event names a webhook subscription may list, besides the `all` / `*` wildcards.
const KNOWN_EVENTS: &[&str] = &[
    "message.created",
    "message.updated",
    "message.moved",
    "message.deleted",
    "sync.completed",
    "new_message",
];

fn event_matches(configured: &str, event: &str) -> bool {
    configured.split(',').map(str::trim).any(|candidate| {
        candidate == "all"
            || candidate == "*"
            || candidate == event
            || (candidate == "new_message" && event == "message.created")
    })
}

/// Reject subscriptions that could never match an event, so a typo cannot
/// silently disable delivery.
fn validate_events(configured: &str) -> Result<()> {
    let unknown: Vec<&str> = configured
        .split(',')
        .map(str::trim)
        .filter(|c| !(c.is_empty() || *c == "all" || *c == "*" || KNOWN_EVENTS.contains(c)))
        .collect();
    if !unknown.is_empty() {
        return Err(AppError::Validation(format!(
            "unknown webhook event(s): {}; expected all, *, or one of {}",
            unknown.join(", "),
            KNOWN_EVENTS.join(", ")
        )));
    }
    if configured.split(',').all(|c| c.trim().is_empty()) {
        return Err(AppError::Validation(
            "webhook events must not be empty".into(),
        ));
    }
    Ok(())
}

/// Persist an event and enqueue all matching deliveries before returning.
pub async fn fire_webhooks(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    event: &str,
    mut data: serde_json::Value,
) {
    let event_id = data
        .get("eventId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Some(object) = data.as_object_mut() {
        object.remove("eventId");
    }
    let data_json = match serde_json::to_string(&data) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "webhook event serialization failed");
            return;
        }
    };
    if let Err(error) = sqlx::query(
        r#"INSERT OR IGNORE INTO webhook_events (id, event_type, account_id, data_json)
           VALUES (?, ?, ?, ?)"#,
    )
    .bind(&event_id)
    .bind(event)
    .bind(account_id)
    .bind(data_json)
    .execute(pool)
    .await
    {
        tracing::warn!(%error, "failed to persist webhook event");
        return;
    }

    let rows = match sqlx::query_as::<_, WebhookRow>(
        r#"SELECT id, url, secret, events, account_id, enabled, created_at
           FROM webhooks
           WHERE enabled = 1
             AND (account_id IS NULL OR account_id = ?)"#,
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("failed to query webhooks: {}", e);
            return;
        }
    };

    for webhook in rows
        .into_iter()
        .filter(|webhook| event_matches(&webhook.events, event))
    {
        if let Err(error) = sqlx::query(
            r#"INSERT OR IGNORE INTO webhook_deliveries (event_id, webhook_id)
               VALUES (?, ?)"#,
        )
        .bind(&event_id)
        .bind(&webhook.id)
        .execute(pool)
        .await
        {
            tracing::warn!(%error, "failed to enqueue webhook delivery");
        }
    }
}

fn signature(body: &str, secret: &str) -> String {
    use std::fmt::Write as _;
    let key = hmac_sha256::HMAC::mac(body.as_bytes(), secret.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in key {
        let _ = write!(hex, "{byte:02x}");
    }
    format!("sha256={hex}")
}

async fn deliver_one(pool: &sqlx::SqlitePool, row: PendingDelivery) {
    let claimed = sqlx::query(
        r#"UPDATE webhook_deliveries
           SET status = 'delivering', last_attempt_at = datetime('now')
           WHERE event_id = ? AND webhook_id = ?
             AND status IN ('pending', 'retry') AND next_attempt_at <= datetime('now')"#,
    )
    .bind(&row.event_id)
    .bind(&row.webhook_id)
    .execute(pool)
    .await;
    if !matches!(claimed, Ok(result) if result.rows_affected() == 1) {
        return;
    }

    let data = match serde_json::from_str(&row.data_json) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, event_id = %row.event_id, "stored webhook JSON is invalid");
            return;
        }
    };
    let payload = WebhookPayload {
        id: &row.event_id,
        event: &row.event_type,
        timestamp: &row.created_at,
        account_id: &row.account_id,
        data: &data,
    };
    let body = match serde_json::to_string(&payload) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "webhook payload serialization failed");
            return;
        }
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%error, "webhook client creation failed");
            return;
        }
    };
    let mut request = client
        .post(&row.url)
        .header("Content-Type", "application/json")
        .header("X-Email-Event", &row.event_type)
        .header("X-Email-Event-ID", &row.event_id)
        .body(body.clone());
    if let Some(secret) = &row.secret {
        request = request.header("X-Email-Signature", signature(&body, secret));
    }

    let attempt = row.attempts + 1;
    match request.send().await {
        Ok(response) if response.status().is_success() => {
            let _ = sqlx::query(
                r#"UPDATE webhook_deliveries
                   SET status = 'delivered', attempts = ?, response_status = ?,
                       last_error = NULL, delivered_at = datetime('now')
                   WHERE event_id = ? AND webhook_id = ?"#,
            )
            .bind(attempt)
            .bind(i64::from(response.status().as_u16()))
            .bind(&row.event_id)
            .bind(&row.webhook_id)
            .execute(pool)
            .await;
        }
        result => {
            let (response_status, error) = match result {
                Ok(response) => (
                    Some(i64::from(response.status().as_u16())),
                    format!("HTTP {}", response.status()),
                ),
                Err(error) => (None, error.to_string()),
            };
            let dead = attempt >= MAX_DELIVERY_ATTEMPTS;
            let delay = (5_i64 * 2_i64.pow((attempt.min(7) - 1) as u32)).min(300);
            let modifier = format!("+{delay} seconds");
            let _ = sqlx::query(
                r#"UPDATE webhook_deliveries
                   SET status = ?, attempts = ?, response_status = ?, last_error = ?,
                       next_attempt_at = datetime('now', ?)
                   WHERE event_id = ? AND webhook_id = ?"#,
            )
            .bind(if dead { "dead_letter" } else { "retry" })
            .bind(attempt)
            .bind(response_status)
            .bind(error.chars().take(2000).collect::<String>())
            .bind(modifier)
            .bind(&row.event_id)
            .bind(&row.webhook_id)
            .execute(pool)
            .await;
        }
    }
}

pub async fn dispatch_due(pool: &sqlx::SqlitePool) -> Result<usize> {
    let rows = sqlx::query_as::<_, PendingDelivery>(
        r#"SELECT d.event_id, d.webhook_id, e.event_type, e.account_id,
                  e.data_json, e.created_at, w.url, w.secret, d.attempts
           FROM webhook_deliveries d
           JOIN webhook_events e ON e.id = d.event_id
           JOIN webhooks w ON w.id = d.webhook_id
           WHERE d.status IN ('pending', 'retry')
             AND d.next_attempt_at <= datetime('now') AND w.enabled = 1
           ORDER BY d.next_attempt_at, e.created_at LIMIT 50"#,
    )
    .fetch_all(pool)
    .await?;
    let count = rows.len();
    for row in rows {
        deliver_one(pool, row).await;
    }
    Ok(count)
}

pub fn spawn_delivery_worker(pool: sqlx::SqlitePool) {
    tokio::spawn(async move {
        let _ = sqlx::query(
            "UPDATE webhook_deliveries SET status = 'retry' WHERE status = 'delivering'",
        )
        .execute(&pool)
        .await;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(error) = dispatch_due(&pool).await {
                tracing::warn!(%error, "webhook delivery worker failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, routing::post, Router};

    use super::*;

    async fn setup() -> sqlx::SqlitePool {
        let path = format!("/tmp/email_webhook_test_{}.db", Uuid::new_v4());
        crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap()
            .0
    }

    async fn webhook(pool: &sqlx::SqlitePool, url: &str, secret: Option<&str>) {
        sqlx::query(
            r#"INSERT INTO webhooks (id, url, secret, events)
               VALUES ('hook-1', ?, ?, 'new_message,message.updated')"#,
        )
        .bind(url)
        .bind(secret)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn server(status: StatusCode) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/", post(move || async move { status })),
            )
            .await
            .unwrap();
        });
        format!("http://{address}/")
    }

    #[tokio::test]
    async fn event_and_matching_delivery_are_persisted_before_dispatch() {
        let pool = setup().await;
        webhook(&pool, "http://127.0.0.1:1/", Some("secret")).await;

        fire_webhooks(
            &pool,
            "account-1",
            "message.created",
            serde_json::json!({
                "eventId": "event-1",
                "logicalId": "logical-1",
                "occurrenceId": "occurrence-1"
            }),
        )
        .await;

        let event: (String, String, String) = sqlx::query_as(
            "SELECT event_type, account_id, data_json FROM webhook_events WHERE id = 'event-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let delivery: (String, i64) = sqlx::query_as(
            "SELECT status, attempts FROM webhook_deliveries WHERE event_id = 'event-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(event.0, "message.created");
        assert_eq!(event.1, "account-1");
        assert!(!event.2.contains("eventId"));
        assert_eq!(delivery, ("pending".to_string(), 0));
    }

    #[tokio::test]
    async fn dispatcher_records_success_and_retryable_failure() {
        let success_pool = setup().await;
        webhook(&success_pool, &server(StatusCode::NO_CONTENT).await, None).await;
        fire_webhooks(
            &success_pool,
            "account-1",
            "message.updated",
            serde_json::json!({"eventId": "success-1"}),
        )
        .await;
        assert_eq!(dispatch_due(&success_pool).await.unwrap(), 1);
        let success: (String, i64, i64) = sqlx::query_as(
            "SELECT status, attempts, response_status FROM webhook_deliveries WHERE event_id = 'success-1'",
        )
        .fetch_one(&success_pool)
        .await
        .unwrap();
        assert_eq!(success, ("delivered".to_string(), 1, 204));

        let failure_pool = setup().await;
        webhook(
            &failure_pool,
            &server(StatusCode::INTERNAL_SERVER_ERROR).await,
            None,
        )
        .await;
        fire_webhooks(
            &failure_pool,
            "account-1",
            "message.updated",
            serde_json::json!({"eventId": "failure-1"}),
        )
        .await;
        assert_eq!(dispatch_due(&failure_pool).await.unwrap(), 1);
        let failure: (String, i64, i64) = sqlx::query_as(
            "SELECT status, attempts, response_status FROM webhook_deliveries WHERE event_id = 'failure-1'",
        )
        .fetch_one(&failure_pool)
        .await
        .unwrap();
        assert_eq!(failure, ("retry".to_string(), 1, 500));
    }

    #[test]
    fn wildcard_subscriptions_match_every_event() {
        for configured in ["*", "all", " * "] {
            assert!(event_matches(configured, "message.created"));
            assert!(event_matches(configured, "sync.completed"));
        }
        assert!(event_matches("new_message", "message.created"));
        assert!(!event_matches("message.updated", "message.created"));
    }

    #[test]
    fn unknown_event_patterns_are_rejected() {
        assert!(validate_events("*").is_ok());
        assert!(validate_events("all").is_ok());
        assert!(validate_events("new_message, message.updated").is_ok());
        assert!(validate_events("message.*").is_err());
        assert!(validate_events("message.create").is_err());
        assert!(validate_events(" , ").is_err());
    }

    #[tokio::test]
    async fn star_subscription_enqueues_delivery() {
        let pool = setup().await;
        sqlx::query(
            "INSERT INTO webhooks (id, url, secret, events) VALUES ('hook-star', 'http://127.0.0.1:1/', NULL, '*')",
        )
        .execute(&pool)
        .await
        .unwrap();
        fire_webhooks(
            &pool,
            "account-1",
            "message.created",
            serde_json::json!({"eventId": "star-1"}),
        )
        .await;
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM webhook_deliveries WHERE event_id = 'star-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }
}
