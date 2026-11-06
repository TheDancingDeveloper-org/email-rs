use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use email_types::{CreateFolder, Folder as FolderRow, PatchFolder, RenameFolder};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::providers::factory::provider_for_account;
use crate::state::AppState;

/// Hierarchy separator used for folder paths the server API creates.
const FOLDER_SEPARATOR: char = '/';
const MAX_FOLDER_NAME_LEN: usize = 128;
const MAX_FOLDER_PATH_LEN: usize = 512;

/// Check a leaf folder name before it reaches an IMAP command. Printable ASCII
/// only: no control characters (CR/LF would end the command), no quote or
/// backslash, no wildcards or hierarchy separators, and no non-ASCII text,
/// which IMAP carries as modified UTF-7 that this API does not yet encode.
fn validate_folder_name(name: &str) -> Result<()> {
    let invalid = |reason: &str| AppError::Validation(format!("invalid folder name: {reason}"));
    if name.is_empty() || name.trim() != name {
        return Err(invalid(
            "must be non-empty with no leading or trailing space",
        ));
    }
    if name.len() > MAX_FOLDER_NAME_LEN {
        return Err(invalid("too long"));
    }
    if matches!(name, "." | "..") {
        return Err(invalid("reserved name"));
    }
    if name.eq_ignore_ascii_case("inbox") {
        return Err(invalid("INBOX is reserved"));
    }
    if let Some(bad) = name.chars().find(|c| {
        !c.is_ascii()
            || c.is_ascii_control()
            || matches!(c, '"' | '\\' | '*' | '%' | '&' | '{' | '}' | '/' | '.')
    }) {
        return Err(invalid(&format!("character {bad:?} is not allowed")));
    }
    Ok(())
}

/// A folder the user may not rename or delete: INBOX and special-use folders.
fn ensure_not_protected(folder: &FolderRow, action: &str) -> Result<()> {
    if folder.full_path.eq_ignore_ascii_case("inbox") || folder.special_use.is_some() {
        return Err(AppError::Validation(format!(
            "cannot {action} {}: it is INBOX or a special-use folder",
            folder.name
        )));
    }
    Ok(())
}

async fn load_folder(state: &AppState, folder_id: &str) -> Result<FolderRow> {
    sqlx::query_as::<_, FolderRow>(
        r#"SELECT id, account_id, name, full_path, special_use,
                  unread_count, total_count, synced_at, is_excluded, is_subscribed
           FROM folders WHERE id = ?"#,
    )
    .bind(folder_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("folder {folder_id} not found")))
}

async fn path_taken(state: &AppState, account_id: &str, full_path: &str) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM folders WHERE account_id = ? AND full_path = ? COLLATE NOCASE",
    )
    .bind(account_id)
    .bind(full_path)
    .fetch_one(&state.pool)
    .await?
        > 0)
}

pub async fn create_folder(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
    Json(body): Json<CreateFolder>,
) -> Result<(axum::http::StatusCode, Json<FolderRow>)> {
    validate_folder_name(&body.name)?;
    let full_path = match body.parent_id.as_deref() {
        Some(parent_id) => {
            let parent = load_folder(&state, parent_id).await?;
            if parent.account_id != account_id {
                return Err(AppError::NotFound(format!("folder {parent_id} not found")));
            }
            format!("{}{FOLDER_SEPARATOR}{}", parent.full_path, body.name)
        }
        None => body.name.clone(),
    };
    if full_path.len() > MAX_FOLDER_PATH_LEN {
        return Err(AppError::Validation("folder path is too long".into()));
    }
    sqlx::query_scalar::<_, String>("SELECT id FROM accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("account {account_id} not found")))?;
    if path_taken(&state, &account_id, &full_path).await? {
        return Err(AppError::Conflict(format!(
            "folder {full_path} already exists"
        )));
    }

    let provider = provider_for_account(&state.pool, &account_id).await?;
    provider.create_folder(&full_path).await?;

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO folders (id, account_id, name, full_path) VALUES (?, ?, ?, ?)
         ON CONFLICT(account_id, full_path) DO NOTHING",
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&body.name)
    .bind(&full_path)
    .execute(&state.pool)
    .await?;
    let folder = sqlx::query_as::<_, FolderRow>(
        r#"SELECT id, account_id, name, full_path, special_use,
                  unread_count, total_count, synced_at, is_excluded, is_subscribed
           FROM folders WHERE account_id = ? AND full_path = ?"#,
    )
    .bind(&account_id)
    .bind(&full_path)
    .fetch_one(&state.pool)
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(folder)))
}

pub async fn rename_folder(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
    Json(body): Json<RenameFolder>,
) -> Result<Json<FolderRow>> {
    validate_folder_name(&body.name)?;
    let folder = load_folder(&state, &folder_id).await?;
    ensure_not_protected(&folder, "rename")?;
    let new_path = match folder.full_path.rsplit_once(FOLDER_SEPARATOR) {
        Some((parent, _)) => format!("{parent}{FOLDER_SEPARATOR}{}", body.name),
        None => body.name.clone(),
    };
    if new_path.len() > MAX_FOLDER_PATH_LEN {
        return Err(AppError::Validation("folder path is too long".into()));
    }
    if new_path == folder.full_path {
        return Ok(Json(folder));
    }
    if path_taken(&state, &folder.account_id, &new_path).await? {
        return Err(AppError::Conflict(format!(
            "folder {new_path} already exists"
        )));
    }

    let provider = provider_for_account(&state.pool, &folder.account_id).await?;
    provider.rename_folder(&folder.full_path, &new_path).await?;

    // The server renames the whole subtree, so the mirror does too. Messages
    // reference folders by id and follow along.
    let old_prefix = format!("{}{FOLDER_SEPARATOR}", folder.full_path);
    let mut tx = crate::db::begin_write(&state.pool).await?;
    sqlx::query(
        "UPDATE folders SET full_path = ? || substr(full_path, ?)
         WHERE account_id = ? AND substr(full_path, 1, ?) = ?",
    )
    .bind(format!("{new_path}{FOLDER_SEPARATOR}"))
    .bind(old_prefix.chars().count() as i64 + 1)
    .bind(&folder.account_id)
    .bind(old_prefix.chars().count() as i64)
    .bind(&old_prefix)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE folders SET name = ?, full_path = ? WHERE id = ?")
        .bind(&body.name)
        .bind(&new_path)
        .bind(&folder.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(load_folder(&state, &folder_id).await?))
}

pub async fn delete_folder(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let folder = load_folder(&state, &folder_id).await?;
    ensure_not_protected(&folder, "delete")?;
    let provider = provider_for_account(&state.pool, &folder.account_id).await?;
    provider.delete_folder(&folder.full_path).await?;

    // Messages in the folder go with it (ON DELETE CASCADE).
    sqlx::query("DELETE FROM folders WHERE id = ?")
        .bind(&folder.id)
        .execute(&state.pool)
        .await?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn set_subscribed(
    state: &AppState,
    folder_id: &str,
    subscribed: bool,
) -> Result<Json<FolderRow>> {
    let folder = load_folder(state, folder_id).await?;
    let provider = provider_for_account(&state.pool, &folder.account_id).await?;
    provider
        .set_folder_subscribed(&folder.full_path, subscribed)
        .await?;
    sqlx::query("UPDATE folders SET is_subscribed = ? WHERE id = ?")
        .bind(subscribed)
        .bind(folder_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(load_folder(state, folder_id).await?))
}

pub async fn subscribe_folder(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
) -> Result<Json<FolderRow>> {
    set_subscribed(&state, &folder_id, true).await
}

pub async fn unsubscribe_folder(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
) -> Result<Json<FolderRow>> {
    set_subscribed(&state, &folder_id, false).await
}

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
                  unread_count, total_count, synced_at, is_excluded, is_subscribed
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

    async fn seed_test_account(pool: &SqlitePool, id: &str, provider_type: &str) {
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
        )
        .bind(id)
        .bind("T")
        .bind(format!("{id}@example.com"))
        .bind(provider_type)
        .bind("password")
        .execute(pool)
        .await
        .unwrap();
    }

    fn calls_for(marker: &str) -> Vec<String> {
        crate::providers::factory::TEST_FOLDER_CALLS
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.contains(marker))
            .cloned()
            .collect()
    }

    async fn folder_paths(pool: &SqlitePool, account_id: &str) -> Vec<String> {
        sqlx::query_scalar("SELECT full_path FROM folders WHERE account_id = ? ORDER BY full_path")
            .bind(account_id)
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn create_folder_calls_the_server_then_mirrors_it() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-create", "test").await;
        seed_folder(&state.pool, "p-create", "acc-create", "Work", "Work", None).await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/accounts/acc-create/folders",
            Some(serde_json::json!({"name": "Invoices", "parentId": "p-create"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["fullPath"], "Work/Invoices");
        assert_eq!(body["name"], "Invoices");
        assert_eq!(body["isSubscribed"], true);
        assert_eq!(calls_for("Work/Invoices"), vec!["create Work/Invoices"]);
        assert_eq!(
            folder_paths(&state.pool, "acc-create").await,
            vec!["Work", "Work/Invoices"]
        );

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/accounts/acc-create/folders",
            Some(serde_json::json!({"name": "invoices", "parentId": "p-create"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn create_folder_refuses_names_that_could_inject_imap_syntax() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-inject", "test").await;
        for name in [
            "a\" b",
            "a\\b",
            "a\r\nA1 DELETE INBOX",
            "{4}",
            "a*",
            "a%",
            "a/b",
            "a.b",
            "..",
            "",
            " pad",
            "INBOX",
            "inbox",
            "Entwürfe",
            "&AGE-",
        ] {
            let (status, body) = req(
                crate::api::router(Arc::clone(&state)),
                "POST",
                "/accounts/acc-inject/folders",
                Some(serde_json::json!({ "name": name })),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{name:?}: {body}");
        }
        assert!(folder_paths(&state.pool, "acc-inject").await.is_empty());
    }

    #[tokio::test]
    async fn create_folder_leaves_the_mirror_alone_when_the_server_refuses() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-fail", "test_fail").await;
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/accounts/acc-fail/folders",
            Some(serde_json::json!({"name": "Nope"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert!(folder_paths(&state.pool, "acc-fail").await.is_empty());
    }

    #[tokio::test]
    async fn create_folder_rejects_a_parent_from_another_account() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-p1", "test").await;
        seed_test_account(&state.pool, "acc-p2", "test").await;
        seed_folder(&state.pool, "p-other", "acc-p2", "Other", "Other", None).await;
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/accounts/acc-p1/folders",
            Some(serde_json::json!({"name": "X", "parentId": "p-other"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rename_folder_updates_the_subtree_in_the_mirror() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-ren", "test").await;
        seed_folder(&state.pool, "r-top", "acc-ren", "Old", "Old", None).await;
        seed_folder(&state.pool, "r-kid", "acc-ren", "Kid", "Old/Kid", None).await;
        seed_folder(&state.pool, "r-sib", "acc-ren", "OldX", "OldX", None).await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/folders/r-top/rename",
            Some(serde_json::json!({"name": "Fresh"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["fullPath"], "Fresh");
        assert_eq!(calls_for("rename Old Fresh"), vec!["rename Old Fresh"]);
        assert_eq!(
            folder_paths(&state.pool, "acc-ren").await,
            vec!["Fresh", "Fresh/Kid", "OldX"]
        );

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/folders/r-top/rename",
            Some(serde_json::json!({"name": "OldX"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn rename_and_delete_refuse_inbox_and_special_use_folders() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-prot", "test").await;
        seed_folder(&state.pool, "x-inbox", "acc-prot", "INBOX", "INBOX", None).await;
        seed_folder(
            &state.pool,
            "x-sent",
            "acc-prot",
            "Sent",
            "Sent",
            Some("sent"),
        )
        .await;
        for id in ["x-inbox", "x-sent"] {
            let (status, _) = req(
                crate::api::router(Arc::clone(&state)),
                "POST",
                &format!("/folders/{id}/rename"),
                Some(serde_json::json!({"name": "Other"})),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "rename {id}");
            let (status, _) = req(
                crate::api::router(Arc::clone(&state)),
                "DELETE",
                &format!("/folders/{id}"),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "delete {id}");
        }
        assert_eq!(
            folder_paths(&state.pool, "acc-prot").await,
            vec!["INBOX", "Sent"]
        );
        assert!(calls_for("INBOX").is_empty());
    }

    #[tokio::test]
    async fn delete_folder_removes_it_and_its_messages_from_the_mirror() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-del", "test").await;
        seed_folder(&state.pool, "d-f", "acc-del", "Scratch", "Scratch", None).await;
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            "/folders/d-f",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(calls_for("delete Scratch"), vec!["delete Scratch"]);
        assert!(folder_paths(&state.pool, "acc-del").await.is_empty());
    }

    #[tokio::test]
    async fn delete_folder_keeps_the_mirror_when_the_server_refuses() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-delf", "test_fail").await;
        seed_folder(&state.pool, "df-f", "acc-delf", "Keep", "Keep", None).await;
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            "/folders/df-f",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(folder_paths(&state.pool, "acc-delf").await, vec!["Keep"]);
    }

    #[tokio::test]
    async fn subscribe_and_unsubscribe_track_the_flag() {
        let state = setup().await;
        seed_test_account(&state.pool, "acc-sub", "test").await;
        seed_folder(&state.pool, "s-f", "acc-sub", "News", "News", None).await;
        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/folders/s-f/unsubscribe",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["isSubscribed"], false);
        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/folders/s-f/subscribe",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["isSubscribed"], true);
        assert_eq!(
            calls_for("News"),
            vec!["subscribed false News", "subscribed true News"]
        );
    }

    #[tokio::test]
    async fn providers_without_folder_management_report_unsupported() {
        use crate::providers::MailProvider;
        struct Bare;
        #[async_trait::async_trait]
        impl MailProvider for Bare {
            fn provider_id(&self) -> &str {
                "bare"
            }
            async fn authenticate(&mut self) -> crate::error::Result<()> {
                Ok(())
            }
            async fn list_folders(&self) -> crate::error::Result<Vec<crate::providers::Folder>> {
                Ok(vec![])
            }
            async fn folder_uid_validity(&self, _: &str) -> crate::error::Result<Option<u32>> {
                Ok(None)
            }
            async fn fetch_messages(
                &self,
                _: &str,
                _: Option<chrono::DateTime<chrono::Utc>>,
            ) -> crate::error::Result<Vec<crate::providers::Message>> {
                Ok(vec![])
            }
            async fn list_message_uids(&self, _: &str) -> crate::error::Result<Vec<u32>> {
                Ok(vec![])
            }
            async fn fetch_message_body(
                &self,
                _: &str,
                _: u32,
            ) -> crate::error::Result<crate::providers::MessageBody> {
                unimplemented!()
            }
            async fn fetch_bodies_batch(
                &self,
                _: &str,
                _: &[u32],
            ) -> crate::error::Result<Vec<(u32, crate::providers::MessageBody)>> {
                Ok(vec![])
            }
            async fn mark_seen(&self, _: &str, _: u32) -> crate::error::Result<()> {
                Ok(())
            }
            async fn mark_unseen(&self, _: &str, _: u32) -> crate::error::Result<()> {
                Ok(())
            }
            async fn delete_message(&self, _: &str, _: u32) -> crate::error::Result<()> {
                Ok(())
            }
            async fn move_message(
                &self,
                _: &str,
                _: u32,
                _: &str,
            ) -> crate::error::Result<crate::providers::MovedMessage> {
                unimplemented!()
            }
            async fn set_flagged(&self, _: &str, _: u32, _: bool) -> crate::error::Result<()> {
                Ok(())
            }
        }
        let provider = Bare;
        for result in [
            provider.create_folder("X").await,
            provider.rename_folder("X", "Y").await,
            provider.delete_folder("X").await,
            provider.set_folder_subscribed("X", true).await,
        ] {
            let error = result.unwrap_err();
            assert!(matches!(error, crate::error::AppError::Unsupported(_)));
            let response = axum::response::IntoResponse::into_response(error);
            assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
        }
    }
}
