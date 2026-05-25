use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use email_types::SnoozeRequest;

use crate::error::Result;
use crate::state::AppState;

/// POST /messages/{id}/snooze
/// Body: {"until": "2026-04-25T09:00:00Z"}
pub async fn snooze_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
    Json(req): Json<SnoozeRequest>,
) -> Result<StatusCode> {
    sqlx::query("UPDATE messages SET snoozed_until = ? WHERE id = ?")
        .bind(&req.until)
        .bind(&message_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /messages/{id}/snooze
pub async fn unsnooze_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<StatusCode> {
    sqlx::query("UPDATE messages SET snoozed_until = NULL WHERE id = ?")
        .bind(&message_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn setup() -> Arc<crate::state::AppState> {
        let path = format!("/tmp/email_snooze_test_{}.db", uuid::Uuid::new_v4());
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        Arc::new(crate::state::AppState::new(pool, has_fts))
    }

    async fn seed_account(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
        )
        .bind(id)
        .bind("T")
        .bind("t@t.com")
        .bind("generic_imap")
        .bind("password")
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_folder(pool: &sqlx::SqlitePool, id: &str, account_id: &str) {
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path) VALUES (?,?,?,?)")
            .bind(id)
            .bind(account_id)
            .bind(id)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn seed_message(pool: &sqlx::SqlitePool, id: &str, account_id: &str, folder_id: &str) {
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id, subject, is_read) VALUES (?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(folder_id)
        .bind(1i64)
        .bind(format!("<{id}>"))
        .bind(format!("Subject of {id}"))
        .bind(false)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn req(
        router: axum::Router,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let body = match body {
            Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
            None => Body::empty(),
        };
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(body)
            .unwrap();
        let resp = router.oneshot(request).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn snooze_sets_snoozed_until() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1").await;
        seed_message(&state.pool, "msg1", "acc1", "fld1").await;

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/msg1/snooze",
            Some(serde_json::json!({"until": "2099-01-01T09:00:00Z"})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let snoozed: Option<String> =
            sqlx::query_scalar("SELECT snoozed_until FROM messages WHERE id = 'msg1'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(snoozed.as_deref(), Some("2099-01-01T09:00:00Z"));
    }

    #[tokio::test]
    async fn unsnooze_clears_snoozed_until() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1").await;
        seed_message(&state.pool, "msg2", "acc1", "fld1").await;

        sqlx::query("UPDATE messages SET snoozed_until = '2099-01-01T09:00:00Z' WHERE id = 'msg2'")
            .execute(&state.pool)
            .await
            .unwrap();

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            "/messages/msg2/snooze",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let snoozed: Option<String> =
            sqlx::query_scalar("SELECT snoozed_until FROM messages WHERE id = 'msg2'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(snoozed.is_none());
    }

    #[tokio::test]
    async fn snoozed_messages_appear_in_smart_folder() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1").await;
        seed_message(&state.pool, "sn1", "acc1", "fld1").await;
        seed_message(&state.pool, "sn2", "acc1", "fld1").await;

        // Snooze sn1 far in the future, sn2 in the past (already expired)
        sqlx::query("UPDATE messages SET snoozed_until = '2099-01-01T09:00:00Z' WHERE id = 'sn1'")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE messages SET snoozed_until = '2000-01-01T09:00:00Z' WHERE id = 'sn2'")
            .execute(&state.pool)
            .await
            .unwrap();

        let (status, json) = req(
            crate::api::router(Arc::clone(&state)),
            "GET",
            "/smart-folders/snoozed/messages",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "sn1");
        assert_eq!(arr[0]["snoozedUntil"], "2099-01-01T09:00:00Z");
    }
}
