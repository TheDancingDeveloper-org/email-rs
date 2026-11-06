//! Private operational state. Public liveness never discloses account information.
use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{error::Result, state::AppState};

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AccountHealth {
    pub account_id: String,
    /// NULL until a complete mail sync succeeds; calendar sync is separate.
    pub last_success_at: Option<String>,
    pub last_error_at: Option<String>,
    /// Safe error category, cleared on recovery. Never raw provider diagnostics.
    pub last_error: Option<String>,
    pub needs_reauth: bool,
    /// Outgoing messages pending or currently sending, including scheduled sends.
    pub queue_depth: i64,
    pub offline_queue_depth: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthDetails {
    pub database: String,
    pub accounts: Vec<AccountHealth>,
}

pub async fn details(State(state): State<Arc<AppState>>) -> Result<Json<HealthDetails>> {
    // The query itself probes the migrated database. Fail closed through the normal
    // sanitized error response, rather than reporting an empty healthy estate.
    let accounts = sqlx::query_as::<_, AccountHealth>(
        "SELECT a.id AS account_id, h.last_success_at, h.last_error_at, h.last_error, a.needs_reauth,
         (SELECT COUNT(*) FROM outbox o WHERE o.account_id = a.id AND o.status IN ('pending', 'sending')) AS queue_depth,
         (SELECT COUNT(*) FROM imap_action_queue q WHERE q.account_id = a.id) AS offline_queue_depth
         FROM accounts a LEFT JOIN account_sync_health h ON h.account_id = a.id ORDER BY a.id",
    ).fetch_all(&state.pool).await?;
    Ok(Json(HealthDetails {
        database: "ok".into(),
        accounts,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_reports_unknown_history_reauth_and_account_scoped_queue() {
        let (pool, has_fts) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        let state = Arc::new(AppState::new(pool.clone(), has_fts));
        assert!(details(State(state.clone()))
            .await
            .unwrap()
            .0
            .accounts
            .is_empty());
        for id in ["a", "b"] {
            sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?, 'name', ?, 'imap', 'basic')").bind(id).bind(format!("{id}@example.test")).execute(&pool).await.unwrap();
        }
        sqlx::query("UPDATE accounts SET needs_reauth = 1 WHERE id = 'b'")
            .execute(&pool)
            .await
            .unwrap();
        for (id, account, status) in [
            ("1", "a", "pending"),
            ("2", "a", "sending"),
            ("3", "a", "sent"),
            ("4", "a", "failed"),
            ("5", "b", "pending"),
        ] {
            sqlx::query("INSERT INTO outbox (id, account_id, payload, status, scheduled_at, cancel_until, next_attempt_at) VALUES (?, ?, '{}', ?, 'future', 'future', 'future')").bind(id).bind(account).bind(status).execute(&pool).await.unwrap();
        }
        let response = crate::api::router(state)
            .oneshot(
                Request::builder()
                    .uri("/health/details")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["database"], "ok");
        assert_eq!(body["accounts"][0]["queueDepth"], 2);
        assert!(body["accounts"][0]["lastSuccessAt"].is_null());
        assert!(body["accounts"][0]["lastError"].is_null());
        assert_eq!(body["accounts"][1]["queueDepth"], 1);
        assert_eq!(body["accounts"][1]["needsReauth"], true);
        assert!(body["accounts"][0].get("email").is_none());
    }

    #[tokio::test]
    async fn health_history_survives_database_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("health.db").display());
        let (pool, _) = crate::db::create_pool(&url).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('persistent', 'A', 'a@example.test', 'imap', 'basic')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO account_sync_health (account_id, last_success_at, last_error_at, last_error) VALUES ('persistent', '2026-11-01 01:00:00', '2026-11-02 01:00:00', 'sync failed')").execute(&pool).await.unwrap();
        pool.close().await;
        let (pool, has_fts) = crate::db::create_pool(&url).await.unwrap();
        let response = details(State(Arc::new(AppState::new(pool.clone(), has_fts))))
            .await
            .unwrap()
            .0;
        assert_eq!(
            response.accounts[0].last_success_at.as_deref(),
            Some("2026-11-01 01:00:00")
        );
        assert_eq!(
            response.accounts[0].last_error.as_deref(),
            Some("sync failed")
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn database_failure_is_not_healthy() {
        let (pool, has_fts) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        let state = Arc::new(AppState::new(pool.clone(), has_fts));
        pool.close().await;
        assert!(details(State(state)).await.is_err());
    }
}
