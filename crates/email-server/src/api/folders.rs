use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use email_types::{Folder as FolderRow, PatchFolder};

use crate::error::{AppError, Result};
use crate::state::AppState;

pub async fn patch_folder(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
    Json(body): Json<PatchFolder>,
) -> Result<Json<serde_json::Value>> {
    if let Some(excluded) = body.is_excluded {
        let rows = sqlx::query("UPDATE folders SET is_excluded = ? WHERE id = ?")
            .bind(excluded)
            .bind(&folder_id)
            .execute(&state.pool)
            .await?;
        if rows.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "folder {} not found",
                folder_id
            )));
        }
    }
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn mark_folder_read(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let mut tx = crate::db::begin_write(&state.pool).await?;
    let result = sqlx::query("UPDATE messages SET is_read = 1 WHERE folder_id = ?")
        .bind(&folder_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE folders SET unread_count = 0 WHERE id = ?")
        .bind(&folder_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(
        serde_json::json!({ "updated": result.rows_affected() }),
    ))
}

pub async fn list_folders(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Result<Json<Vec<FolderRow>>> {
    let rows = sqlx::query_as::<_, FolderRow>(
        r#"SELECT id, account_id, name, full_path, special_use,
                  unread_count, total_count, synced_at, is_excluded
           FROM folders
           WHERE account_id = ?
           ORDER BY CASE special_use
               WHEN 'inbox'   THEN 0
               WHEN 'sent'    THEN 1
               WHEN 'drafts'  THEN 2
               WHEN 'archive' THEN 3
               WHEN 'trash'   THEN 4
               WHEN 'spam'    THEN 5
               ELSE 6
           END"#,
    )
    .bind(&account_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use sqlx::SqlitePool;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn setup() -> Arc<crate::state::AppState> {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        Arc::new(crate::state::AppState::new(pool, has_fts))
    }

    async fn seed_account(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
        )
        .bind(id)
        .bind("T")
        .bind(format!("{id}@example.com"))
        .bind("generic_imap")
        .bind("password")
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_folder(
        pool: &SqlitePool,
        id: &str,
        account_id: &str,
        name: &str,
        full_path: &str,
        special_use: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO folders (id, account_id, name, full_path, special_use) VALUES (?,?,?,?,?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(name)
        .bind(full_path)
        .bind(special_use)
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
    async fn list_folders_empty() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/accounts/acc1/folders",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::json!([]));
    }

    #[tokio::test]
    async fn list_folders_ordered_by_special_use() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(
            &state.pool,
            "fld-trash",
            "acc1",
            "Trash",
            "Trash",
            Some("trash"),
        )
        .await;
        seed_folder(
            &state.pool,
            "fld-inbox",
            "acc1",
            "INBOX",
            "INBOX",
            Some("inbox"),
        )
        .await;
        seed_folder(
            &state.pool,
            "fld-sent",
            "acc1",
            "Sent",
            "Sent",
            Some("sent"),
        )
        .await;

        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/accounts/acc1/folders",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["id"], "fld-inbox");
        assert_eq!(arr[1]["id"], "fld-sent");
        assert_eq!(arr[2]["id"], "fld-trash");
    }

    #[tokio::test]
    async fn list_folders_null_special_use_comes_last() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld-custom", "acc1", "Lists", "Lists", None).await;
        seed_folder(
            &state.pool,
            "fld-inbox",
            "acc1",
            "INBOX",
            "INBOX",
            Some("inbox"),
        )
        .await;

        let (_, body) = req(
            crate::api::router(state),
            "GET",
            "/accounts/acc1/folders",
            None,
        )
        .await;
        let arr = body.as_array().unwrap();
        assert_eq!(arr[0]["id"], "fld-inbox");
        assert_eq!(arr[1]["id"], "fld-custom");
    }

    #[tokio::test]
    async fn mark_folder_read_updates_all_messages() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", "INBOX", "INBOX", None).await;
        for i in 0..3 {
            sqlx::query(
                "INSERT INTO messages (id, account_id, folder_id, uid, message_id, is_read) VALUES (?,?,?,?,?,?)",
            )
            .bind(format!("mr-msg-{}", i))
            .bind("acc1")
            .bind("fld1")
            .bind(i as i64)
            .bind(format!("<id{}>", i))
            .bind(false)
            .execute(&state.pool)
            .await
            .unwrap();
        }
        sqlx::query("UPDATE folders SET unread_count = 3 WHERE id = 'fld1'")
            .execute(&state.pool)
            .await
            .unwrap();

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/folders/fld1/mark-read",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["updated"], 3);

        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE folder_id = 'fld1' AND is_read = 0",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(unread, 0);

        let folder_unread: i64 =
            sqlx::query_scalar("SELECT unread_count FROM folders WHERE id = 'fld1'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(folder_unread, 0);
    }

    #[tokio::test]
    async fn mark_folder_read_empty_folder_returns_zero() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", "INBOX", "INBOX", None).await;
        sqlx::query("UPDATE folders SET unread_count = 2 WHERE id = 'fld1'")
            .execute(&state.pool)
            .await
            .unwrap();

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/folders/fld1/mark-read",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["updated"], 0);

        let folder_unread: i64 =
            sqlx::query_scalar("SELECT unread_count FROM folders WHERE id = 'fld1'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(folder_unread, 0);
    }

    #[tokio::test]
    async fn patch_folder_sets_excluded() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", "Promos", "Promos", None).await;

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/folders/fld1",
            Some(serde_json::json!({"isExcluded": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let excluded: bool =
            sqlx::query_scalar("SELECT is_excluded FROM folders WHERE id = 'fld1'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(excluded);
    }

    #[tokio::test]
    async fn patch_folder_clears_excluded() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", "Spam", "Spam", None).await;
        sqlx::query("UPDATE folders SET is_excluded = 1 WHERE id = 'fld1'")
            .execute(&state.pool)
            .await
            .unwrap();

        req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/folders/fld1",
            Some(serde_json::json!({"isExcluded": false})),
        )
        .await;

        let excluded: bool =
            sqlx::query_scalar("SELECT is_excluded FROM folders WHERE id = 'fld1'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(!excluded);
    }

    #[tokio::test]
    async fn patch_nonexistent_folder_is_404() {
        let state = setup().await;
        let (status, _) = req(
            crate::api::router(state),
            "PATCH",
            "/folders/ghost",
            Some(serde_json::json!({"isExcluded": true})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_folders_only_shows_account_folders() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_account(&state.pool, "acc2").await;
        seed_folder(&state.pool, "f1", "acc1", "INBOX", "INBOX", Some("inbox")).await;
        seed_folder(&state.pool, "f2", "acc2", "INBOX", "INBOX", Some("inbox")).await;

        let (_, body) = req(
            crate::api::router(state),
            "GET",
            "/accounts/acc1/folders",
            None,
        )
        .await;
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "f1");
    }
}
