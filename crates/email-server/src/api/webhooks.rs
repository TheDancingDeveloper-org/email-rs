use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use email_types::{CreateWebhook, UpdateWebhook, Webhook as WebhookRow};
use serde::Serialize;
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
    let events = body.events.unwrap_or_else(|| "new_message".to_string());

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

#[derive(Debug, Serialize)]
struct WebhookPayload<'a> {
    event: &'a str,
    account_id: &'a str,
    data: &'a serde_json::Value,
}

/// Fire all enabled webhooks matching the given event for an account.
/// Runs in the background; errors are logged but don't block the caller.
pub async fn fire_webhooks(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    event: &str,
    data: serde_json::Value,
) {
    let rows = match sqlx::query_as::<_, WebhookRow>(
        r#"SELECT id, url, secret, events, account_id, enabled, created_at
           FROM webhooks
           WHERE enabled = 1
             AND (account_id IS NULL OR account_id = ?)
             AND (events = ? OR events LIKE ? OR events = 'all')"#,
    )
    .bind(account_id)
    .bind(event)
    .bind(format!("%{}%", event))
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("failed to query webhooks: {}", e);
            return;
        }
    };

    let client = reqwest::Client::new();
    for webhook in rows {
        let payload = WebhookPayload {
            event,
            account_id,
            data: &data,
        };
        let body = match serde_json::to_string(&payload) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("webhook payload serialize error: {}", e);
                continue;
            }
        };

        let mut req = client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-Email-Event", event)
            .body(body.clone());

        if let Some(secret) = &webhook.secret {
            // Simple HMAC-SHA256 signature: X-Email-Signature: sha256=<hex>
            use std::fmt::Write as FmtWrite;
            let key = hmac_sha256::HMAC::mac(body.as_bytes(), secret.as_bytes());
            let mut hex = String::with_capacity(64);
            for b in key {
                let _ = write!(hex, "{:02x}", b);
            }
            req = req.header("X-Email-Signature", format!("sha256={}", hex));
        }

        let url = webhook.url.clone();
        let req = req;
        tokio::spawn(async move {
            match req.send().await {
                Ok(resp) if !resp.status().is_success() => {
                    tracing::warn!("webhook {} returned {}", url, resp.status());
                }
                Err(e) => {
                    tracing::warn!("webhook {} failed: {}", url, e);
                }
                _ => {}
            }
        });
    }
}
