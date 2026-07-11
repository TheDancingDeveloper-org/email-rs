use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use email_types::{CreateLabelBody, Label, Message as MessageRow, UpdateLabelBody};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::state::AppState;

// ── Query / body types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListLabelsQuery {
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
pub struct LabelMessagesQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /labels?account_id=xxx
pub async fn list_labels(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListLabelsQuery>,
) -> Result<Json<Vec<Label>>> {
    let rows = sqlx::query_as::<_, Label>(
        "SELECT id, account_id, name, color FROM labels WHERE account_id = ? ORDER BY name ASC",
    )
    .bind(&q.account_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

/// POST /labels
pub async fn create_label(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateLabelBody>,
) -> Result<Json<Label>> {
    let id = uuid::Uuid::new_v4().to_string();
    let color = body.color.unwrap_or_else(|| "#6b7280".to_string());

    let result =
        sqlx::query("INSERT INTO labels (id, account_id, name, color) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(&body.account_id)
            .bind(&body.name)
            .bind(&color)
            .execute(&state.pool)
            .await;

    match result {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed") {
                return Err(AppError::Provider("label already exists".into()));
            }
            return Err(e.into());
        }
    }

    let label = Label {
        id,
        account_id: body.account_id,
        name: body.name,
        color,
    };

    Ok(Json(label))
}

/// PUT /labels/{id}
pub async fn update_label(
    State(state): State<Arc<AppState>>,
    Path(label_id): Path<String>,
    Json(body): Json<UpdateLabelBody>,
) -> Result<Json<Label>> {
    // Fetch current label first.
    let current =
        sqlx::query_as::<_, Label>("SELECT id, account_id, name, color FROM labels WHERE id = ?")
            .bind(&label_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("label {} not found", label_id)))?;

    let new_name = body.name.unwrap_or_else(|| current.name.clone());
    let new_color = body.color.unwrap_or_else(|| current.color.clone());

    sqlx::query("UPDATE labels SET name = ?, color = ? WHERE id = ?")
        .bind(&new_name)
        .bind(&new_color)
        .bind(&label_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(Label {
        id: current.id,
        account_id: current.account_id,
        name: new_name,
        color: new_color,
    }))
}

/// DELETE /labels/{id}
pub async fn delete_label(
    State(state): State<Arc<AppState>>,
    Path(label_id): Path<String>,
) -> Result<StatusCode> {
    sqlx::query("DELETE FROM labels WHERE id = ?")
        .bind(&label_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /messages/{id}/labels
pub async fn get_message_labels(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<Vec<Label>>> {
    let rows = sqlx::query_as::<_, Label>(
        r#"SELECT l.id, l.account_id, l.name, l.color
           FROM labels l
           JOIN message_labels ml ON ml.label_id = l.id
           WHERE ml.message_id = ?
           ORDER BY l.name ASC"#,
    )
    .bind(&message_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

/// POST /messages/{id}/labels/{label_id}
pub async fn add_message_label(
    State(state): State<Arc<AppState>>,
    Path((message_id, label_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT OR IGNORE INTO message_labels (id, message_id, label_id) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(&message_id)
        .bind(&label_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /messages/{id}/labels/{label_id}
pub async fn remove_message_label(
    State(state): State<Arc<AppState>>,
    Path((message_id, label_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    sqlx::query("DELETE FROM message_labels WHERE message_id = ? AND label_id = ?")
        .bind(&message_id)
        .bind(&label_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /labels/{id}/messages
pub async fn list_label_messages(
    State(state): State<Arc<AppState>>,
    Path(label_id): Path<String>,
    Query(q): Query<LabelMessagesQuery>,
) -> Result<Json<Vec<MessageRow>>> {
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let rows = sqlx::query_as::<_, MessageRow>(
        r#"SELECT m.id, m.account_id, m.folder_id, m.uid, m.message_id, m.thread_id,
                  m.subject, m.from_name, m.from_email, m.to_json, m.date,
                  m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview, m.snoozed_until
           FROM messages m
           JOIN message_labels ml ON ml.message_id = m.id
           WHERE ml.label_id = ?
           ORDER BY m.date DESC
           LIMIT ? OFFSET ?"#,
    )
    .bind(&label_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn setup() -> Arc<crate::state::AppState> {
        let path = format!("/tmp/email_labels_test_{}.db", uuid::Uuid::new_v4());
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
        .bind("Test")
        .bind("test@test.com")
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
        let uid: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(uid), 0) + 1 FROM messages WHERE folder_id = ?",
        )
        .bind(folder_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id, subject, is_read) VALUES (?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(folder_id)
        .bind(uid)
        .bind(format!("<{id}>"))
        .bind(format!("Subject {id}"))
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
    async fn list_labels_empty() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/labels?account_id=acc1",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::json!([]));
    }

    #[tokio::test]
    async fn create_and_list_label() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "Important", "color": "#ff0000"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "Important");
        assert_eq!(body["color"], "#ff0000");
        assert_eq!(body["accountId"], "acc1");

        let (list_status, list_body) = req(
            crate::api::router(state),
            "GET",
            "/labels?account_id=acc1",
            None,
        )
        .await;
        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(list_body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_duplicate_label_returns_error() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "Dup", "color": "#aaa"})),
        )
        .await;
        let (status, _) = req(
            crate::api::router(state),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "Dup", "color": "#bbb"})),
        )
        .await;
        // Provider error maps to 502
        assert_eq!(status, StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn update_label() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        let (_, create_body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "Old", "color": "#111"})),
        )
        .await;
        let label_id = create_body["id"].as_str().unwrap().to_string();

        let (status, body) = req(
            crate::api::router(state),
            "PUT",
            &format!("/labels/{label_id}"),
            Some(serde_json::json!({"name": "New", "color": "#222"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "New");
        assert_eq!(body["color"], "#222");
    }

    #[tokio::test]
    async fn delete_label() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        let (_, create_body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "ToDelete", "color": "#333"})),
        )
        .await;
        let label_id = create_body["id"].as_str().unwrap().to_string();

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            &format!("/labels/{label_id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (list_status, list_body) = req(
            crate::api::router(state),
            "GET",
            "/labels?account_id=acc1",
            None,
        )
        .await;
        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(list_body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn add_and_get_message_labels() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1").await;
        seed_message(&state.pool, "msg1", "acc1", "fld1").await;

        let (_, label_body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "Work", "color": "#0000ff"})),
        )
        .await;
        let label_id = label_body["id"].as_str().unwrap().to_string();

        let (add_status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            &format!("/messages/msg1/labels/{label_id}"),
            None,
        )
        .await;
        assert_eq!(add_status, StatusCode::NO_CONTENT);

        let (get_status, get_body) = req(
            crate::api::router(state),
            "GET",
            "/messages/msg1/labels",
            None,
        )
        .await;
        assert_eq!(get_status, StatusCode::OK);
        let arr = get_body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "Work");
    }

    #[tokio::test]
    async fn remove_message_label() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1").await;
        seed_message(&state.pool, "msg1", "acc1", "fld1").await;

        let (_, label_body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "Remove", "color": "#999"})),
        )
        .await;
        let label_id = label_body["id"].as_str().unwrap().to_string();

        req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            &format!("/messages/msg1/labels/{label_id}"),
            None,
        )
        .await;

        let (del_status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            &format!("/messages/msg1/labels/{label_id}"),
            None,
        )
        .await;
        assert_eq!(del_status, StatusCode::NO_CONTENT);

        let (get_status, get_body) = req(
            crate::api::router(state),
            "GET",
            "/messages/msg1/labels",
            None,
        )
        .await;
        assert_eq!(get_status, StatusCode::OK);
        assert_eq!(get_body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn list_label_messages() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1").await;
        seed_message(&state.pool, "msg1", "acc1", "fld1").await;
        seed_message(&state.pool, "msg2", "acc1", "fld1").await;

        let (_, label_body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/labels",
            Some(serde_json::json!({"accountId": "acc1", "name": "Tag", "color": "#abc"})),
        )
        .await;
        let label_id = label_body["id"].as_str().unwrap().to_string();

        req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            &format!("/messages/msg1/labels/{label_id}"),
            None,
        )
        .await;

        let (list_status, list_body) = req(
            crate::api::router(state),
            "GET",
            &format!("/labels/{label_id}/messages"),
            None,
        )
        .await;
        assert_eq!(list_status, StatusCode::OK);
        let arr = list_body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "msg1");
    }
}
