use std::{collections::BTreeSet, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use email_types::{
    BulkAction, BulkRequest, BulkResponse, BulkResult, Message as MessageRow, MessageAttachment,
    PatchMessageRequest,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use chrono;

pub async fn bulk_messages(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkRequest>,
) -> crate::error::Result<Json<BulkResponse>> {
    let mut processed = 0usize;
    let mut results = Vec::with_capacity(req.ids.len());
    let mut affected_folders = BTreeSet::new();

    for id in &req.ids {
        match bulk_message(&state, id, &req.action, &mut affected_folders).await {
            Ok(()) => {
                processed += 1;
                results.push(BulkResult {
                    id: id.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(error) => results.push(BulkResult {
                id: id.clone(),
                success: false,
                error: Some(error.to_string()),
            }),
        }
    }

    for folder_id in affected_folders {
        refresh_folder_counts(&state.pool, &folder_id).await?;
    }

    Ok(Json(BulkResponse {
        processed,
        failed: req.ids.len() - processed,
        results,
    }))
}

use crate::api::webhooks::fire_webhooks;
use crate::error::{AppError, Result};
use crate::providers::factory::provider_for_message;
use crate::providers::{MailProvider, ProviderAttachment};
use crate::state::AppState;

async fn message_folder_id(pool: &sqlx::SqlitePool, message_id: &str) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT folder_id FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

async fn message_event_context(
    pool: &sqlx::SqlitePool,
    message_id: &str,
) -> Result<Option<(String, serde_json::Value)>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT account_id, logical_id FROM messages WHERE id = ?")
            .bind(message_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(account_id, logical_id)| {
        (
            account_id,
            serde_json::json!({
                "logicalId": logical_id,
                "occurrenceId": message_id,
            }),
        )
    }))
}

async fn refresh_folder_counts(pool: &sqlx::SqlitePool, folder_id: &str) -> Result<()> {
    sqlx::query(
        r#"UPDATE folders
           SET total_count = (SELECT COUNT(*) FROM messages WHERE folder_id = ?),
               unread_count = (SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_read = 0)
           WHERE id = ?"#,
    )
    .bind(folder_id)
    .bind(folder_id)
    .bind(folder_id)
    .execute(pool)
    .await?;

    Ok(())
}

async fn load_provider(
    state: &AppState,
    message_id: &str,
) -> Result<(String, u32, Arc<dyn MailProvider>)> {
    provider_for_message(&state.pool, message_id).await
}

async fn delete_remote(state: &AppState, message_id: &str) -> Result<()> {
    let (source, uid, provider) = load_provider(state, message_id).await?;
    let trash: Option<String> = sqlx::query_scalar(
        r#"SELECT f.full_path FROM folders f
           JOIN messages m ON m.account_id = f.account_id
           WHERE m.id = ? AND f.special_use = 'trash' LIMIT 1"#,
    )
    .bind(message_id)
    .fetch_optional(&state.pool)
    .await?;
    match trash {
        Some(destination) if destination != source => {
            provider.move_message(&source, uid, &destination).await
        }
        _ => provider.delete_message(&source, uid).await,
    }
}

async fn bulk_message(
    state: &AppState,
    id: &str,
    action: &BulkAction,
    affected_folders: &mut BTreeSet<String>,
) -> Result<()> {
    let event = message_event_context(&state.pool, id).await?;
    let event_type = match action {
        BulkAction::MarkRead | BulkAction::MarkUnread => "message.updated",
        BulkAction::Delete => "message.deleted",
        BulkAction::Archive => "message.moved",
    };
    let source_folder_id = message_folder_id(&state.pool, id).await?;
    match action {
        BulkAction::MarkRead | BulkAction::MarkUnread => {
            let (folder, uid, provider) = load_provider(state, id).await?;
            let is_read = matches!(action, BulkAction::MarkRead);
            if is_read {
                provider.mark_seen(&folder, uid).await?;
            } else {
                provider.mark_unseen(&folder, uid).await?;
            }
            sqlx::query("UPDATE messages SET is_read = ? WHERE id = ?")
                .bind(is_read)
                .bind(id)
                .execute(&state.pool)
                .await?;
        }
        BulkAction::Delete => {
            delete_remote(state, id).await?;
            sqlx::query("DELETE FROM messages WHERE id = ?")
                .bind(id)
                .execute(&state.pool)
                .await?;
        }
        BulkAction::Archive => {
            let archive_folder: Option<String> = sqlx::query_scalar(
                r#"SELECT f.full_path FROM folders f
                   JOIN messages m ON m.account_id = f.account_id
                   WHERE m.id = ? AND f.special_use = 'archive'
                   LIMIT 1"#,
            )
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
            let (source, uid, provider) = load_provider(state, id).await?;
            let destination = match archive_folder {
                Some(folder) => folder,
                None if provider.provider_id() == "gmail" => "[Gmail]/All Mail".to_string(),
                None => {
                    return Err(AppError::Provider(
                        "account has no archive folder".to_string(),
                    ))
                }
            };
            provider.move_message(&source, uid, &destination).await?;
            // IMAP MOVE assigns a new destination UID. Remove the stale source
            // occurrence and let the next sync discover the destination.
            sqlx::query("DELETE FROM messages WHERE id = ?")
                .bind(id)
                .execute(&state.pool)
                .await?;
        }
    }
    if let Some(folder_id) = source_folder_id {
        affected_folders.insert(folder_id);
    }
    if let Some((account_id, data)) = event {
        fire_webhooks(&state.pool, &account_id, event_type, data).await;
    }
    Ok(())
}

pub async fn patch_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
    Json(req): Json<PatchMessageRequest>,
) -> Result<Json<serde_json::Value>> {
    let event = message_event_context(&state.pool, &message_id).await?;
    if let Some(is_read) = req.is_read {
        let (folder, uid, provider) = load_provider(&state, &message_id).await?;
        if is_read {
            provider.mark_seen(&folder, uid).await?;
        } else {
            provider.mark_unseen(&folder, uid).await?;
        }
        let folder_id = message_folder_id(&state.pool, &message_id).await?;
        sqlx::query("UPDATE messages SET is_read = ? WHERE id = ?")
            .bind(is_read)
            .bind(&message_id)
            .execute(&state.pool)
            .await?;
        if let Some(folder_id) = folder_id {
            refresh_folder_counts(&state.pool, &folder_id).await?;
        }
    }

    if let Some(is_flagged) = req.is_flagged {
        let (folder, uid, provider) = load_provider(&state, &message_id).await?;
        provider.set_flagged(&folder, uid, is_flagged).await?;
        sqlx::query("UPDATE messages SET is_flagged = ? WHERE id = ?")
            .bind(is_flagged)
            .bind(&message_id)
            .execute(&state.pool)
            .await?;
    }

    if let Some((account_id, data)) = event {
        fire_webhooks(&state.pool, &account_id, "message.updated", data).await;
    }

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let event = message_event_context(&state.pool, &message_id).await?;
    let folder_id = message_folder_id(&state.pool, &message_id).await?;

    delete_remote(&state, &message_id).await?;

    // Remove from local DB immediately.
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(&message_id)
        .execute(&state.pool)
        .await?;
    if let Some(folder_id) = folder_id {
        refresh_folder_counts(&state.pool, &folder_id).await?;
    }
    if let Some((account_id, data)) = event {
        fire_webhooks(&state.pool, &account_id, "message.deleted", data).await;
    }

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn archive_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let event = message_event_context(&state.pool, &message_id).await?;
    let source_folder_id = message_folder_id(&state.pool, &message_id).await?;

    // Find the archive folder for this message's account.
    let archive_folder: Option<String> = sqlx::query_scalar(
        r#"SELECT f.full_path FROM folders f
           JOIN messages m ON m.account_id = f.account_id
           WHERE m.id = ? AND f.special_use = 'archive'
           LIMIT 1"#,
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?;

    let (source, uid, provider) = load_provider(&state, &message_id).await?;
    let destination = match archive_folder {
        Some(folder) => folder,
        None if provider.provider_id() == "gmail" => "[Gmail]/All Mail".to_string(),
        None => {
            return Err(AppError::Provider(
                "account has no archive folder".to_string(),
            ))
        }
    };
    provider.move_message(&source, uid, &destination).await?;
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(&message_id)
        .execute(&state.pool)
        .await?;
    if let Some(source_folder_id) = source_folder_id {
        refresh_folder_counts(&state.pool, &source_folder_id).await?;
    }
    if let Some((account_id, mut data)) = event {
        if let Some(object) = data.as_object_mut() {
            object.insert("destination".to_string(), destination.into());
        }
        fire_webhooks(&state.pool, &account_id, "message.moved", data).await;
    }

    Ok(Json(serde_json::json!({ "status": "archived" })))
}

pub async fn list_attachments(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<Vec<MessageAttachment>>> {
    ensure_message_exists(&state.pool, &message_id).await?;
    Ok(Json(
        load_message_attachments(&state.pool, &message_id).await?,
    ))
}

pub async fn download_attachment(
    State(state): State<Arc<AppState>>,
    Path((message_id, attachment_id)): Path<(String, String)>,
) -> Result<Response> {
    let row = sqlx::query_as::<_, AttachmentContentRow>(
        r#"SELECT filename, content_type, content, part_id, size_bytes
           FROM attachments
           WHERE id = ? AND message_id = ?"#,
    )
    .bind(&attachment_id)
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("attachment {} not found", attachment_id)))?;

    let download_limit = attachment_download_limit();
    if row.size_bytes > download_limit as i64 {
        return Err(AppError::PayloadTooLarge(format!(
            "attachment exceeds the configured {} byte download limit",
            download_limit
        )));
    }
    let content = match row.content {
        Some(content) => content,
        None => {
            let (folder, uid, provider) = load_provider(&state, &message_id).await?;
            let content = tokio::time::timeout(
                attachment_provider_timeout(),
                provider.fetch_attachment(&folder, uid, &row.part_id),
            )
            .await
            .map_err(|_| AppError::Provider("attachment provider request timed out".into()))??;
            if content.len() > download_limit {
                return Err(AppError::PayloadTooLarge(format!(
                    "attachment exceeds the configured {} byte download limit",
                    download_limit
                )));
            }
            let cache_limit = attachment_cache_limit();
            if content.len() <= cache_limit {
                sqlx::query("UPDATE attachments SET content = ? WHERE id = ?")
                    .bind(&content)
                    .bind(&attachment_id)
                    .execute(&state.pool)
                    .await?;
            }
            content
        }
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&row.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&attachment_content_disposition(&row.filename))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"attachment\"")),
    );

    Ok((StatusCode::OK, headers, Body::from(content)).into_response())
}

async fn ensure_message_exists(pool: &sqlx::SqlitePool, message_id: &str) -> Result<()> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(pool)
        .await?;
    exists
        .map(|_| ())
        .ok_or_else(|| AppError::NotFound(format!("message {} not found", message_id)))
}

async fn load_message_attachments(
    pool: &sqlx::SqlitePool,
    message_id: &str,
) -> Result<Vec<MessageAttachment>> {
    sqlx::query_as::<_, MessageAttachment>(
        r#"SELECT id, message_id, filename, content_type, size_bytes
           FROM attachments
           WHERE message_id = ?
           ORDER BY filename COLLATE NOCASE, id"#,
    )
    .bind(message_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

#[derive(Debug, sqlx::FromRow)]
struct AttachmentContentRow {
    filename: String,
    content_type: String,
    content: Option<Vec<u8>>,
    part_id: String,
    size_bytes: i64,
}

fn attachment_cache_limit() -> usize {
    std::env::var("ATTACHMENT_CACHE_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(25 * 1024 * 1024)
}

fn attachment_download_limit() -> usize {
    std::env::var("ATTACHMENT_DOWNLOAD_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(50 * 1024 * 1024)
}

fn attachment_provider_timeout() -> std::time::Duration {
    let seconds = std::env::var("ATTACHMENT_PROVIDER_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(30);
    std::time::Duration::from_secs(seconds)
}

pub(crate) async fn cache_provider_attachments(
    pool: &sqlx::SqlitePool,
    message_id: &str,
    attachments: &[ProviderAttachment],
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await?;
    cache_provider_attachments_tx(&mut tx, message_id, attachments).await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn cache_provider_attachments_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    message_id: &str,
    attachments: &[ProviderAttachment],
) -> Result<()> {
    let existing: Vec<String> =
        sqlx::query_scalar("SELECT id FROM attachments WHERE message_id = ?")
            .bind(message_id)
            .fetch_all(&mut **tx)
            .await?;
    let cache_limit = attachment_cache_limit();
    let mut desired = std::collections::HashSet::new();
    for attachment in attachments {
        let id = format!("{message_id}:{}", attachment.part_id);
        desired.insert(id.clone());
        let cached_content = attachment
            .content
            .as_deref()
            .filter(|content| content.len() <= cache_limit);
        sqlx::query(
            r#"INSERT INTO attachments
               (id, message_id, filename, content_type, part_id, content_id, size_bytes, content)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(message_id, part_id) DO UPDATE SET
                   filename = excluded.filename,
                   content_type = excluded.content_type,
                   content_id = excluded.content_id,
                   size_bytes = excluded.size_bytes,
                   content = COALESCE(excluded.content, attachments.content)"#,
        )
        .bind(id)
        .bind(message_id)
        .bind(&attachment.filename)
        .bind(&attachment.content_type)
        .bind(&attachment.part_id)
        .bind(&attachment.content_id)
        .bind(attachment.size_bytes)
        .bind(cached_content)
        .execute(&mut **tx)
        .await?;
    }
    for id in existing {
        if !desired.contains(&id) {
            sqlx::query("DELETE FROM attachments WHERE id = ?")
                .bind(id)
                .execute(&mut **tx)
                .await?;
        }
    }
    sqlx::query("UPDATE messages SET has_attachments = ? WHERE id = ?")
        .bind(!attachments.is_empty())
        .bind(message_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn attachment_content_disposition(filename: &str) -> String {
    let escaped = filename
        .chars()
        .filter(|ch| !ch.is_control())
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect::<String>();
    let filename = if escaped.trim().is_empty() {
        "attachment".to_string()
    } else {
        escaped
    };
    format!("attachment; filename=\"{filename}\"")
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MessageBodyRow {
    pub message_id: String,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub raw_headers: Option<String>,
    pub fetched_at: String,
    #[sqlx(default)]
    pub clean_body: Option<String>,
    #[sqlx(default)]
    pub quoted_history: Option<String>,
    #[sqlx(default)]
    pub signature_stripped: bool,
    #[sqlx(default)]
    pub clean_version: i64,
}

#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub unread_only: Option<bool>,
}

pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
    Query(q): Query<ListMessagesQuery>,
) -> Result<Json<Vec<MessageRow>>> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;
    let unread_only = q.unread_only.unwrap_or(false);

    let sql = if unread_only {
        r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id, thread_id,
                  subject, from_name, from_email, to_json, date,
                  is_read, is_flagged, is_draft, has_attachments, preview, snoozed_until
           FROM messages
           WHERE folder_id = ? AND is_read = 0
             AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
           ORDER BY date DESC
           LIMIT ? OFFSET ?"#
    } else {
        r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id, thread_id,
                  subject, from_name, from_email, to_json, date,
                  is_read, is_flagged, is_draft, has_attachments, preview, snoozed_until
           FROM messages
           WHERE folder_id = ?
             AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
           ORDER BY date DESC
           LIMIT ? OFFSET ?"#
    };

    let rows = sqlx::query_as::<_, MessageRow>(sql)
        .bind(&folder_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(rows))
}

pub async fn get_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let msg = sqlx::query_as::<_, MessageRow>(
        r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id, thread_id,
                  subject, from_name, from_email, to_json, date,
                  is_read, is_flagged, is_draft, has_attachments, preview, snoozed_until
           FROM messages WHERE id = ?"#,
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("message {} not found", message_id)))?;

    // Check body cache.
    let mut body = sqlx::query_as::<_, MessageBodyRow>(
        r#"SELECT message_id, html_body, text_body, raw_headers, fetched_at,
                  clean_body, quoted_history, signature_stripped, clean_version
           FROM message_bodies WHERE message_id = ?"#,
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?;

    let provider = load_provider(&state, &message_id).await.ok();

    // Lazy-fetch body from IMAP on cache miss.
    if body.is_none() {
        if let Some((folder, uid, provider)) = &provider {
            info!("body cache miss for {}, fetching from IMAP", message_id);
            match provider.fetch_message_body(folder, *uid).await {
                Ok(fetched) => {
                    cache_provider_attachments(&state.pool, &message_id, &fetched.attachments)
                        .await?;
                    sqlx::query(
                        r#"INSERT INTO message_bodies (message_id, html_body, text_body, raw_headers)
                           VALUES (?, ?, ?, ?)
                           ON CONFLICT(message_id) DO UPDATE SET
                               html_body  = excluded.html_body,
                               text_body  = excluded.text_body,
                               raw_headers = excluded.raw_headers,
                               fetched_at = datetime('now')"#,
                    )
                    .bind(&message_id)
                    .bind(&fetched.html_body)
                    .bind(&fetched.text_body)
                    .bind(&fetched.raw_headers)
                    .execute(&state.pool)
                    .await?;

                    body = Some(MessageBodyRow {
                        message_id: message_id.clone(),
                        html_body: fetched.html_body,
                        text_body: fetched.text_body,
                        raw_headers: fetched.raw_headers,
                        fetched_at: chrono::Utc::now().to_rfc3339(),
                        clean_body: None,
                        quoted_history: None,
                        signature_stripped: false,
                        clean_version: 0,
                    });
                }
                Err(e) => {
                    tracing::warn!("failed to fetch IMAP body for {}: {}", message_id, e);
                }
            }
        }
    }

    // Clean a body cached before it had a clean form, or under older rules.
    // The startup backfill does the same for bodies nobody reads.
    if let Some(b) = body.as_mut() {
        if b.clean_version < crate::clean::CLEAN_VERSION {
            let cleaned = crate::clean::store(
                &state.pool,
                &message_id,
                b.text_body.as_deref(),
                b.html_body.as_deref(),
            )
            .await?;
            b.clean_body = Some(cleaned.body);
            b.quoted_history = cleaned.quoted_history;
            b.signature_stripped = cleaned.signature_stripped;
            b.clean_version = crate::clean::CLEAN_VERSION;
        }
    }

    if !msg.is_read {
        if let Some((folder, uid, provider)) = &provider {
            if let Err(error) = provider.mark_seen(folder, *uid).await {
                tracing::warn!("mark_seen failed: {}", error);
            }
        }
    }

    // Mark as read in local DB.
    sqlx::query("UPDATE messages SET is_read = 1 WHERE id = ?")
        .bind(&message_id)
        .execute(&state.pool)
        .await?;
    refresh_folder_counts(&state.pool, &msg.folder_id).await?;
    let attachments = load_message_attachments(&state.pool, &message_id).await?;

    let result = serde_json::json!({
        "id": msg.id,
        "occurrenceId": msg.occurrence_id,
        "logicalId": msg.logical_id,
        "accountId": msg.account_id,
        "folderId": msg.folder_id,
        "uid": msg.uid,
        "messageId": msg.message_id,
        "threadId": msg.thread_id,
        "subject": msg.subject,
        "fromName": msg.from_name,
        "fromEmail": msg.from_email,
        "to": msg.to_json,
        "date": msg.date,
        "isRead": true,
        "isFlagged": msg.is_flagged,
        "isDraft": msg.is_draft,
        "hasAttachments": msg.has_attachments,
        "preview": msg.preview,
        "body": body.map(|b| serde_json::json!({
            "htmlBody": b.html_body,
            "textBody": b.text_body,
            "cleanBody": b.clean_body,
            "quotedHistory": b.quoted_history,
            "signatureStripped": b.signature_stripped,
        })),
        "attachments": attachments,
    });

    Ok(Json(result))
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
        .bind("t@example.com")
        .bind("test")
        .bind("test")
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_folder(pool: &SqlitePool, id: &str, account_id: &str, special_use: Option<&str>) {
        sqlx::query(
            "INSERT INTO folders (id, account_id, name, full_path, special_use) VALUES (?,?,?,?,?)",
        )
        // Use `id` as full_path so sibling folders don't collide on the UNIQUE(account_id, full_path) constraint.
        .bind(id)
        .bind(account_id)
        .bind(id)
        .bind(id)
        .bind(special_use)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_message(pool: &SqlitePool, id: &str, account_id: &str, folder_id: &str) {
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
        .bind(format!("Subject of {id}"))
        .bind(false)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_attachment(
        pool: &SqlitePool,
        id: &str,
        message_id: &str,
        filename: &str,
        content_type: &str,
        content: Option<&[u8]>,
    ) {
        sqlx::query(
            "INSERT INTO attachments (id, message_id, filename, content_type, part_id, size_bytes, content) VALUES (?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(message_id)
        .bind(filename)
        .bind(content_type)
        .bind(id)
        .bind(content.map(|bytes| bytes.len() as i64).unwrap_or_default())
        .bind(content)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("UPDATE messages SET has_attachments = 1 WHERE id = ?")
            .bind(message_id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn folder_counts(pool: &SqlitePool, folder_id: &str) -> (i64, i64) {
        sqlx::query_as("SELECT unread_count, total_count FROM folders WHERE id = ?")
            .bind(folder_id)
            .fetch_one(pool)
            .await
            .unwrap()
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

    async fn req_raw(
        router: axum::Router,
        method: &str,
        uri: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(request).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, headers, bytes.to_vec())
    }

    #[tokio::test]
    async fn list_messages_empty_folder() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/folders/fld1/messages",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::json!([]));
    }

    #[tokio::test]
    async fn list_messages_returns_all() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg1", "acc1", "fld1").await;
        seed_message(&state.pool, "msg2", "acc1", "fld1").await;
        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/folders/fld1/messages",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn list_messages_serializes_snoozed_until() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "snoozed-row", "acc1", "fld1").await;
        sqlx::query(
            "UPDATE messages SET snoozed_until = '2000-01-01T09:00:00Z' WHERE id = 'snoozed-row'",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/folders/fld1/messages",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["snoozedUntil"], "2000-01-01T09:00:00Z");
    }

    #[tokio::test]
    async fn list_messages_unread_filter() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "unread-msg", "acc1", "fld1").await;
        seed_message(&state.pool, "read-msg", "acc1", "fld1").await;
        sqlx::query("UPDATE messages SET is_read = 1 WHERE id = 'read-msg'")
            .execute(&state.pool)
            .await
            .unwrap();

        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/folders/fld1/messages?unread_only=true",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "unread-msg");
    }

    #[tokio::test]
    async fn list_messages_pagination() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        for i in 0..5 {
            seed_message(&state.pool, &format!("pg-{}", i), "acc1", "fld1").await;
        }
        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/folders/fld1/messages?per_page=2&page=1",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn list_messages_page_2() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        for i in 0..5 {
            seed_message(&state.pool, &format!("p2-{}", i), "acc1", "fld1").await;
        }
        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/folders/fld1/messages?per_page=3&page=2",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 2); // 5 total, first page has 3, second has 2
    }

    #[tokio::test]
    async fn get_message_returns_content() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-get", "acc1", "fld1").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "GET",
            "/messages/msg-get",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], "msg-get");
        assert_eq!(body["subject"], "Subject of msg-get");
    }

    async fn seed_body(
        pool: &SqlitePool,
        message_id: &str,
        text: Option<&str>,
        html: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO message_bodies (message_id, html_body, text_body) VALUES (?, ?, ?)",
        )
        .bind(message_id)
        .bind(html)
        .bind(text)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn get_message_cleans_a_body_cached_before_cleaning_existed() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-clean", "acc1", "fld1").await;
        seed_body(
            &state.pool,
            "msg-clean",
            Some("Yes, 3pm works.\n\n-- \nAda\n\nOn Mon, 28 Sep 2026 at 17:02, Tom <t@example.com> wrote:\n> Does 3pm work?\n"),
            None,
        )
        .await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "GET",
            "/messages/msg-clean",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["body"]["cleanBody"], "Yes, 3pm works.");
        assert_eq!(body["body"]["signatureStripped"], true);
        assert!(body["body"]["quotedHistory"]
            .as_str()
            .unwrap()
            .contains("Does 3pm work?"));
        assert!(body["body"]["textBody"]
            .as_str()
            .unwrap()
            .contains("Does 3pm work?"));

        let (version, preview): (i64, Option<String>) = sqlx::query_as(
            "SELECT mb.clean_version, m.preview FROM message_bodies mb JOIN messages m ON m.id = mb.message_id WHERE m.id = ?",
        )
        .bind("msg-clean")
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(version, crate::clean::CLEAN_VERSION);
        assert_eq!(preview.as_deref(), Some("Yes, 3pm works."));
    }

    #[tokio::test]
    async fn backfill_cleans_every_stale_body() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        for i in 0..60 {
            let id = format!("msg-{i}");
            seed_message(&state.pool, &id, "acc1", "fld1").await;
            seed_body(
                &state.pool,
                &id,
                None,
                Some("<html><head><style>p{color:red}</style></head><body><p>Hello</p></body></html>"),
            )
            .await;
        }

        assert_eq!(crate::clean::backfill(&state.pool).await.unwrap(), 60);
        let stale: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM message_bodies WHERE clean_version < ? OR clean_body != 'Hello'",
        )
        .bind(crate::clean::CLEAN_VERSION)
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(stale, 0);
        assert_eq!(crate::clean::backfill(&state.pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn get_message_marks_read_in_db() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-read", "acc1", "fld1").await;

        req(
            crate::api::router(Arc::clone(&state)),
            "GET",
            "/messages/msg-read",
            None,
        )
        .await;

        let is_read: bool =
            sqlx::query_scalar("SELECT is_read FROM messages WHERE id = 'msg-read'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(is_read);
    }

    #[tokio::test]
    async fn get_message_refreshes_folder_counts() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-read-count", "acc1", "fld1").await;

        req(
            crate::api::router(Arc::clone(&state)),
            "GET",
            "/messages/msg-read-count",
            None,
        )
        .await;

        assert_eq!(folder_counts(&state.pool, "fld1").await, (0, 1));
    }

    #[tokio::test]
    async fn get_message_response_has_is_read_true() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-ir", "acc1", "fld1").await;

        let (_, body) = req(crate::api::router(state), "GET", "/messages/msg-ir", None).await;
        assert_eq!(body["isRead"], true);
    }

    #[tokio::test]
    async fn get_message_includes_attachment_metadata() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-attach", "acc1", "fld1").await;
        seed_attachment(
            &state.pool,
            "att-2",
            "msg-attach",
            "zeta.txt",
            "text/plain",
            Some(b"zeta"),
        )
        .await;
        seed_attachment(
            &state.pool,
            "att-1",
            "msg-attach",
            "alpha.pdf",
            "application/pdf",
            Some(b"alpha"),
        )
        .await;

        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/messages/msg-attach",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let attachments = body["attachments"].as_array().unwrap();
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0]["id"], "att-1");
        assert_eq!(attachments[0]["filename"], "alpha.pdf");
        assert_eq!(attachments[0]["contentType"], "application/pdf");
        assert_eq!(attachments[0]["sizeBytes"], 5);
    }

    #[tokio::test]
    async fn list_attachments_returns_metadata() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-list-attach", "acc1", "fld1").await;
        seed_attachment(
            &state.pool,
            "att-list",
            "msg-list-attach",
            "invoice.pdf",
            "application/pdf",
            Some(b"pdf"),
        )
        .await;

        let (status, body) = req(
            crate::api::router(state),
            "GET",
            "/messages/msg-list-attach/attachments",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let attachments = body.as_array().unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0]["id"], "att-list");
        assert_eq!(attachments[0]["messageId"], "msg-list-attach");
    }

    #[tokio::test]
    async fn download_attachment_returns_bytes_and_headers() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-download-attach", "acc1", "fld1").await;
        seed_attachment(
            &state.pool,
            "att-download",
            "msg-download-attach",
            "report.txt",
            "text/plain",
            Some(b"report body"),
        )
        .await;

        let (status, headers, bytes) = req_raw(
            crate::api::router(state),
            "GET",
            "/messages/msg-download-attach/attachments/att-download",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(bytes, b"report body");
        assert_eq!(headers["content-type"], "text/plain");
        assert_eq!(
            headers["content-disposition"],
            "attachment; filename=\"report.txt\""
        );
    }

    #[tokio::test]
    async fn download_missing_attachment_content_is_404() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-empty-attach", "acc1", "fld1").await;
        seed_attachment(
            &state.pool,
            "att-empty",
            "msg-empty-attach",
            "empty.bin",
            "application/octet-stream",
            None,
        )
        .await;

        let (status, _, _) = req_raw(
            crate::api::router(state),
            "GET",
            "/messages/msg-empty-attach/attachments/att-empty",
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn declared_oversize_attachment_is_rejected_before_provider_fetch() {
        let state = setup().await;
        seed_account(&state.pool, "acc-limit").await;
        seed_folder(&state.pool, "fld-limit", "acc-limit", None).await;
        seed_message(&state.pool, "msg-limit", "acc-limit", "fld-limit").await;
        seed_attachment(
            &state.pool,
            "att-limit",
            "msg-limit",
            "huge.bin",
            "application/octet-stream",
            None,
        )
        .await;
        sqlx::query("UPDATE attachments SET size_bytes = ? WHERE id='att-limit'")
            .bind(50_i64 * 1024 * 1024 + 1)
            .execute(&state.pool)
            .await
            .unwrap();
        let (status, _, _) = req_raw(
            crate::api::router(state),
            "GET",
            "/messages/msg-limit/attachments/att-limit",
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn get_nonexistent_message_is_404() {
        let state = setup().await;
        let (status, _) = req(crate::api::router(state), "GET", "/messages/ghost", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn patch_message_marks_read() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "pm1", "acc1", "fld1").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/messages/pm1",
            Some(serde_json::json!({"isRead": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");

        let is_read: bool = sqlx::query_scalar("SELECT is_read FROM messages WHERE id = 'pm1'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert!(is_read);
    }

    #[tokio::test]
    async fn patch_message_refreshes_folder_counts() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "pm-count", "acc1", "fld1").await;

        req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/messages/pm-count",
            Some(serde_json::json!({"isRead": true})),
        )
        .await;

        assert_eq!(folder_counts(&state.pool, "fld1").await, (0, 1));
    }

    #[tokio::test]
    async fn patch_message_marks_unread() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "pm2", "acc1", "fld1").await;
        sqlx::query("UPDATE messages SET is_read = 1 WHERE id = 'pm2'")
            .execute(&state.pool)
            .await
            .unwrap();

        req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/messages/pm2",
            Some(serde_json::json!({"isRead": false})),
        )
        .await;

        let is_read: bool = sqlx::query_scalar("SELECT is_read FROM messages WHERE id = 'pm2'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert!(!is_read);
    }

    #[tokio::test]
    async fn patch_message_sets_flagged() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "pm3", "acc1", "fld1").await;

        req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/messages/pm3",
            Some(serde_json::json!({"isFlagged": true})),
        )
        .await;

        let is_flagged: bool =
            sqlx::query_scalar("SELECT is_flagged FROM messages WHERE id = 'pm3'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(is_flagged);
    }

    #[tokio::test]
    async fn patch_message_clears_flagged() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "pm4", "acc1", "fld1").await;
        sqlx::query("UPDATE messages SET is_flagged = 1 WHERE id = 'pm4'")
            .execute(&state.pool)
            .await
            .unwrap();

        req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/messages/pm4",
            Some(serde_json::json!({"isFlagged": false})),
        )
        .await;

        let is_flagged: bool =
            sqlx::query_scalar("SELECT is_flagged FROM messages WHERE id = 'pm4'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(!is_flagged);
    }

    #[tokio::test]
    async fn delete_message_removes_from_db() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "del-msg", "acc1", "fld1").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            "/messages/del-msg",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "response body: {body}");
        assert_eq!(body["status"], "deleted");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'del-msg'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(folder_counts(&state.pool, "fld1").await, (0, 0));
    }

    #[tokio::test]
    async fn archive_without_archive_folder_deletes_message() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld-inbox", "acc1", Some("inbox")).await;
        seed_message(&state.pool, "arch-msg", "acc1", "fld-inbox").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/arch-msg/archive",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "archived");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'arch-msg'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn archive_with_archive_folder_removes_stale_source_occurrence() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld-inbox", "acc1", Some("inbox")).await;
        seed_folder(&state.pool, "fld-archive", "acc1", Some("archive")).await;
        seed_message(&state.pool, "mv-msg", "acc1", "fld-inbox").await;

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/mv-msg/archive",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'mv-msg'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(folder_counts(&state.pool, "fld-inbox").await, (0, 0));
        // IMAP MOVE chooses a new destination UID. The next synchronization
        // creates the destination occurrence with the correct mailbox identity.
        assert_eq!(folder_counts(&state.pool, "fld-archive").await, (0, 0));
    }

    #[tokio::test]
    async fn bulk_mark_read() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "bm1", "acc1", "fld1").await;
        seed_message(&state.pool, "bm2", "acc1", "fld1").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/bulk",
            Some(serde_json::json!({"ids": ["bm1", "bm2"], "action": "mark_read"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["processed"], 2);

        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE id IN ('bm1','bm2') AND is_read = 0",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(unread, 0);
        assert_eq!(folder_counts(&state.pool, "fld1").await, (0, 2));
    }

    #[tokio::test]
    async fn bulk_mark_unread() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "bu1", "acc1", "fld1").await;
        sqlx::query("UPDATE messages SET is_read = 1 WHERE id = 'bu1'")
            .execute(&state.pool)
            .await
            .unwrap();

        req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/bulk",
            Some(serde_json::json!({"ids": ["bu1"], "action": "mark_unread"})),
        )
        .await;

        let is_read: bool = sqlx::query_scalar("SELECT is_read FROM messages WHERE id = 'bu1'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert!(!is_read);
        assert_eq!(folder_counts(&state.pool, "fld1").await, (1, 1));
    }

    #[tokio::test]
    async fn bulk_delete() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "bd1", "acc1", "fld1").await;
        seed_message(&state.pool, "bd2", "acc1", "fld1").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/bulk",
            Some(serde_json::json!({"ids": ["bd1", "bd2"], "action": "delete"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["processed"], 2);

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id IN ('bd1','bd2')")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        assert_eq!(folder_counts(&state.pool, "fld1").await, (0, 0));
    }

    #[tokio::test]
    async fn bulk_archive_without_archive_folder_deletes() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", Some("inbox")).await;
        seed_message(&state.pool, "ba1", "acc1", "fld1").await;
        seed_message(&state.pool, "ba2", "acc1", "fld1").await;

        req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/bulk",
            Some(serde_json::json!({"ids": ["ba1", "ba2"], "action": "archive"})),
        )
        .await;

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id IN ('ba1','ba2')")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        assert_eq!(folder_counts(&state.pool, "fld1").await, (0, 0));
    }

    #[tokio::test]
    async fn bulk_empty_ids_processes_zero() {
        let state = setup().await;
        let (status, body) = req(
            crate::api::router(state),
            "POST",
            "/messages/bulk",
            Some(serde_json::json!({"ids": [], "action": "mark_read"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["processed"], 0);
        assert_eq!(body["failed"], 0);
        assert_eq!(body["results"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn bulk_returns_per_message_success_and_failure() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "bulk-ok", "acc1", "fld1").await;

        let (status, body) = req(
            crate::api::router(state),
            "POST",
            "/messages/bulk",
            Some(serde_json::json!({
                "ids": ["missing", "bulk-ok"],
                "action": "mark_read"
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["processed"], 1);
        assert_eq!(body["failed"], 1);
        assert_eq!(body["results"][0]["id"], "missing");
        assert_eq!(body["results"][0]["success"], false);
        assert!(body["results"][0]["error"].is_string());
        assert_eq!(
            body["results"][1],
            serde_json::json!({
                "id": "bulk-ok",
                "success": true
            })
        );
    }

    #[tokio::test]
    async fn remote_mutation_failures_never_commit_local_read_flag_delete_or_archive() {
        let state = setup().await;
        seed_account(&state.pool, "acc-fail").await;
        sqlx::query("UPDATE accounts SET provider_type='test_fail' WHERE id='acc-fail'")
            .execute(&state.pool)
            .await
            .unwrap();
        seed_folder(&state.pool, "inbox", "acc-fail", Some("inbox")).await;
        seed_folder(&state.pool, "archive", "acc-fail", Some("archive")).await;
        for id in ["read", "flag", "delete", "archive-msg"] {
            seed_message(&state.pool, id, "acc-fail", "inbox").await;
        }

        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/messages/read",
            Some(serde_json::json!({"isRead": true})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "PATCH",
            "/messages/flag",
            Some(serde_json::json!({"isFlagged": true})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "DELETE",
            "/messages/delete",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/archive-msg/archive",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);

        let rows: Vec<(String, bool, bool)> = sqlx::query_as(
            "SELECT id, is_read, is_flagged FROM messages WHERE account_id='acc-fail' ORDER BY id",
        )
        .fetch_all(&state.pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|(_, read, flagged)| !read && !flagged));
    }
}
