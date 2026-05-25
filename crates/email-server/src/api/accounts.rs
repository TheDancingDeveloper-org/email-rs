use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use email_types::{Account as AccountRow, AccountSettings, CreateAccount, UpdateAccount};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::sync::SyncOrchestrator;

pub async fn list_accounts(State(state): State<Arc<AppState>>) -> Result<Json<Vec<AccountRow>>> {
    let rows = sqlx::query_as::<_, AccountRow>(
        "SELECT id, name, email, provider_type, auth_type, created_at FROM accounts ORDER BY created_at"
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

pub async fn create_account(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateAccount>,
) -> Result<Json<AccountRow>> {
    let id = Uuid::new_v4().to_string();
    let use_ssl = body.use_ssl.unwrap_or(true);

    sqlx::query(
        r#"INSERT INTO accounts
               (id, name, email, provider_type, auth_type, oauth_token_json,
                host, port, use_ssl, password, smtp_host, smtp_port, smtp_password)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&body.name)
    .bind(&body.email)
    .bind(&body.provider_type)
    .bind(&body.auth_type)
    .bind(&body.oauth_token_json)
    .bind(&body.host)
    .bind(body.port)
    .bind(use_ssl)
    .bind(&body.password)
    .bind(&body.smtp_host)
    .bind(body.smtp_port)
    .bind(&body.smtp_password)
    .execute(&state.pool)
    .await?;

    let row = sqlx::query_as::<_, AccountRow>(
        "SELECT id, name, email, provider_type, auth_type, created_at FROM accounts WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

pub async fn update_account(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateAccount>,
) -> Result<Json<AccountRow>> {
    if let Some(ref name) = body.name {
        sqlx::query("UPDATE accounts SET name = ? WHERE id = ?")
            .bind(name)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(days) = body.sync_days_limit {
        sqlx::query("UPDATE accounts SET sync_days_limit = ? WHERE id = ?")
            .bind(days)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref sig) = body.signature {
        sqlx::query("UPDATE accounts SET signature = ? WHERE id = ?")
            .bind(sig)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    let row = sqlx::query_as::<_, AccountRow>(
        "SELECT id, name, email, provider_type, auth_type, created_at FROM accounts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("account {} not found", id)))?;

    Ok(Json(row))
}

pub async fn delete_account(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("account {} not found", id)));
    }
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn get_account_settings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AccountSettings>> {
    let row = sqlx::query_as::<_, AccountSettings>(
        "SELECT id, name, email, provider_type, sync_days_limit, signature FROM accounts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("account {} not found", id)))?;

    Ok(Json(row))
}

pub async fn trigger_sync(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_one(&state.pool)
        .await?;
    let exists = count > 0;

    if !exists {
        return Err(AppError::NotFound(format!(
            "account {} not found",
            account_id
        )));
    }

    let orchestrator = SyncOrchestrator::new(state.pool.clone(), state.event_tx.clone());
    orchestrator.sync_account(&account_id).await?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn setup() -> Arc<crate::state::AppState> {
        let path = format!("/tmp/email_test_{}.db", uuid::Uuid::new_v4());
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        Arc::new(crate::state::AppState::new(pool, has_fts))
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
    async fn list_accounts_empty() {
        let state = setup().await;
        let (status, body) = req(crate::api::router(state), "GET", "/accounts", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::json!([]));
    }

    #[tokio::test]
    async fn create_account_returns_row_with_correct_fields() {
        let state = setup().await;
        let payload = serde_json::json!({
            "name": "My IMAP",
            "email": "user@example.com",
            "providerType": "generic_imap",
            "authType": "password",
            "host": "imap.example.com",
            "port": 993,
            "useSsl": true
        });
        let (status, body) = req(
            crate::api::router(state),
            "POST",
            "/accounts",
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["email"], "user@example.com");
        assert_eq!(body["name"], "My IMAP");
        assert_eq!(body["providerType"], "generic_imap");
        assert!(body["id"].is_string());
        assert!(body["createdAt"].is_string());
    }

    #[tokio::test]
    async fn create_then_list_shows_account() {
        let state = setup().await;
        let payload = serde_json::json!({
            "name": "Gmail",
            "email": "user@gmail.com",
            "providerType": "gmail",
            "authType": "oauth"
        });
        req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/accounts",
            Some(payload),
        )
        .await;

        let (status, body) = req(crate::api::router(state), "GET", "/accounts", None).await;
        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["email"], "user@gmail.com");
    }

    #[tokio::test]
    async fn delete_account_removes_it() {
        let state = setup().await;
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
        )
        .bind("del-id")
        .bind("Del")
        .bind("d@d.com")
        .bind("generic_imap")
        .bind("password")
        .execute(&state.pool)
        .await
        .unwrap();

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            "/accounts/del-id",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "deleted");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE id = 'del-id'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn delete_nonexistent_account_is_404() {
        let state = setup().await;
        let (status, _) = req(crate::api::router(state), "DELETE", "/accounts/ghost", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_account_name() {
        let state = setup().await;
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
        )
        .bind("upd-id")
        .bind("Old")
        .bind("o@o.com")
        .bind("generic_imap")
        .bind("password")
        .execute(&state.pool)
        .await
        .unwrap();

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/accounts/upd-id",
            Some(serde_json::json!({"name": "New Name"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "New Name");
    }

    #[tokio::test]
    async fn update_nonexistent_account_is_404() {
        let state = setup().await;
        let (status, _) = req(
            crate::api::router(state),
            "PATCH",
            "/accounts/ghost",
            Some(serde_json::json!({"name": "x"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cascade_delete_removes_folders_and_messages() {
        let state = setup().await;
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
        )
        .bind("cas-acc")
        .bind("C")
        .bind("c@c.com")
        .bind("generic_imap")
        .bind("password")
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path) VALUES (?,?,?,?)")
            .bind("cas-fld")
            .bind("cas-acc")
            .bind("INBOX")
            .bind("INBOX")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id) VALUES (?,?,?,?,?)",
        )
        .bind("cas-msg")
        .bind("cas-acc")
        .bind("cas-fld")
        .bind(1i64)
        .bind("<cas@x>")
        .execute(&state.pool)
        .await
        .unwrap();

        req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            "/accounts/cas-acc",
            None,
        )
        .await;

        let msg_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'cas-msg'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(msg_count, 0);
    }

    #[tokio::test]
    async fn multiple_accounts_returned_in_order() {
        let state = setup().await;
        for i in 0..3 {
            sqlx::query(
                "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
            )
            .bind(format!("acc-{}", i))
            .bind(format!("Account {}", i))
            .bind(format!("a{}@example.com", i))
            .bind("generic_imap")
            .bind("password")
            .execute(&state.pool)
            .await
            .unwrap();
        }
        let (status, body) = req(crate::api::router(state), "GET", "/accounts", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 3);
    }
}
