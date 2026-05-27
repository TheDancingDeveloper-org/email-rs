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
    BulkAction, BulkRequest, Message as MessageRow, MessageAttachment, PatchMessageRequest,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use chrono;

pub async fn bulk_messages(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkRequest>,
) -> crate::error::Result<Json<serde_json::Value>> {
    let mut processed = 0usize;
    let mut affected_folders = BTreeSet::new();

    for id in &req.ids {
        match req.action {
            BulkAction::MarkRead => {
                if let Some(folder_id) = message_folder_id(&state.pool, id).await? {
                    affected_folders.insert(folder_id);
                }
                sqlx::query("UPDATE messages SET is_read = 1 WHERE id = ?")
                    .bind(id)
                    .execute(&state.pool)
                    .await?;

                if let Ok(Some((info, provider))) = load_provider(&state, id).await {
                    let uid = info.uid as u32;
                    let folder = info.full_path.clone();
                    let p = provider.clone();
                    tokio::spawn(async move {
                        if let Err(e) = p.mark_seen(&folder, uid).await {
                            tracing::warn!("bulk mark_seen failed: {}", e);
                        }
                    });
                }
            }
            BulkAction::MarkUnread => {
                if let Some(folder_id) = message_folder_id(&state.pool, id).await? {
                    affected_folders.insert(folder_id);
                }
                sqlx::query("UPDATE messages SET is_read = 0 WHERE id = ?")
                    .bind(id)
                    .execute(&state.pool)
                    .await?;

                if let Ok(Some((info, provider))) = load_provider(&state, id).await {
                    let uid = info.uid as u32;
                    let folder = info.full_path.clone();
                    let p = provider.clone();
                    tokio::spawn(async move {
                        if let Err(e) = p.mark_unseen(&folder, uid).await {
                            tracing::warn!("bulk mark_unseen failed: {}", e);
                        }
                    });
                }
            }
            BulkAction::Delete => {
                if let Some(folder_id) = message_folder_id(&state.pool, id).await? {
                    affected_folders.insert(folder_id);
                }
                if let Ok(Some((info, provider))) = load_provider(&state, id).await {
                    let uid = info.uid as u32;
                    let folder = info.full_path.clone();
                    tokio::spawn(async move {
                        if let Err(e) = provider.delete_message(&folder, uid).await {
                            tracing::warn!("bulk IMAP delete failed: {}", e);
                        }
                    });
                }
                sqlx::query("DELETE FROM messages WHERE id = ?")
                    .bind(id)
                    .execute(&state.pool)
                    .await?;
            }
            BulkAction::Archive => {
                let source_folder_id = message_folder_id(&state.pool, id).await?;
                let archive_folder: Option<String> = sqlx::query_scalar(
                    r#"SELECT f.full_path FROM folders f
                       JOIN messages m ON m.account_id = f.account_id
                       WHERE m.id = ? AND f.special_use = 'archive'
                       LIMIT 1"#,
                )
                .bind(id)
                .fetch_optional(&state.pool)
                .await?;

                let archive_full_path =
                    archive_folder.unwrap_or_else(|| "[Gmail]/All Mail".to_string());

                let archive_folder_id: Option<String> = sqlx::query_scalar(
                    r#"SELECT f.id FROM folders f
                       JOIN messages m ON m.account_id = f.account_id
                       WHERE m.id = ? AND f.special_use = 'archive'
                       LIMIT 1"#,
                )
                .bind(id)
                .fetch_optional(&state.pool)
                .await?;

                if let Ok(Some((info, provider))) = load_provider(&state, id).await {
                    let uid = info.uid as u32;
                    let src = info.full_path.clone();
                    let dest = archive_full_path.clone();
                    tokio::spawn(async move {
                        if let Err(e) = provider.move_message(&src, uid, &dest).await {
                            tracing::warn!("bulk IMAP archive failed: {}", e);
                        }
                    });
                }

                if let Some(folder_id) = archive_folder_id {
                    sqlx::query("UPDATE messages SET folder_id = ? WHERE id = ?")
                        .bind(&folder_id)
                        .bind(id)
                        .execute(&state.pool)
                        .await?;
                    if let Some(source_folder_id) = source_folder_id {
                        affected_folders.insert(source_folder_id);
                    }
                    affected_folders.insert(folder_id);
                } else {
                    sqlx::query("DELETE FROM messages WHERE id = ?")
                        .bind(id)
                        .execute(&state.pool)
                        .await?;
                    if let Some(source_folder_id) = source_folder_id {
                        affected_folders.insert(source_folder_id);
                    }
                }
            }
        }
        processed += 1;
    }

    for folder_id in affected_folders {
        refresh_folder_counts(&state.pool, &folder_id).await?;
    }

    Ok(Json(serde_json::json!({ "processed": processed })))
}

use crate::auth::oauth2::{OAuthConfig, StoredToken};
use crate::error::{AppError, Result};
use crate::providers::gmail::GmailProvider;
use crate::providers::MailProvider;
use crate::state::AppState;

async fn message_folder_id(pool: &sqlx::SqlitePool, message_id: &str) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT folder_id FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
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

#[derive(Debug, sqlx::FromRow)]
struct MessageActionInfo {
    uid: i64,
    full_path: String,
    email: String,
    provider_type: String,
    oauth_token_json: Option<String>,
}

async fn load_provider(
    state: &AppState,
    message_id: &str,
) -> Result<Option<(MessageActionInfo, GmailProvider)>> {
    let info = sqlx::query_as::<_, MessageActionInfo>(
        r#"SELECT m.uid, f.full_path, a.email, a.provider_type, a.oauth_token_json
           FROM messages m
           JOIN folders f ON m.folder_id = f.id
           JOIN accounts a ON m.account_id = a.id
           WHERE m.id = ?"#,
    )
    .bind(message_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(info) = info else { return Ok(None) };
    if info.provider_type != "gmail" {
        return Ok(None);
    }

    let token_json = info
        .oauth_token_json
        .as_deref()
        .ok_or_else(|| AppError::Auth("account has no OAuth token".to_string()))?;
    let mut stored: StoredToken = serde_json::from_str(token_json)
        .map_err(|e| AppError::Auth(format!("parse token: {}", e)))?;
    if stored.is_expired() {
        let client_id = std::env::var("GOOGLE_CLIENT_ID")
            .map_err(|_| AppError::Auth("GOOGLE_CLIENT_ID not set".to_string()))?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
            .map_err(|_| AppError::Auth("GOOGLE_CLIENT_SECRET not set".to_string()))?;
        let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:3000/api/v1/auth/gmail/callback".to_string());
        let oauth = OAuthConfig::gmail(client_id, client_secret, redirect_uri);
        let refresh = stored
            .refresh_token
            .as_deref()
            .ok_or_else(|| AppError::Auth("no refresh token".to_string()))?;
        stored = StoredToken::from_token_response(oauth.refresh_token(refresh).await?);
    }

    let account_id: String = sqlx::query_scalar("SELECT account_id FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_one(&state.pool)
        .await?;

    let provider = GmailProvider::new(account_id, info.email.clone(), stored.access_token.clone());
    Ok(Some((info, provider)))
}

pub async fn patch_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
    Json(req): Json<PatchMessageRequest>,
) -> Result<Json<serde_json::Value>> {
    if let Some(is_read) = req.is_read {
        let folder_id = message_folder_id(&state.pool, &message_id).await?;
        sqlx::query("UPDATE messages SET is_read = ? WHERE id = ?")
            .bind(is_read)
            .bind(&message_id)
            .execute(&state.pool)
            .await?;
        if let Some(folder_id) = folder_id {
            refresh_folder_counts(&state.pool, &folder_id).await?;
        }

        if let Ok(Some((info, provider))) = load_provider(&state, &message_id).await {
            let uid = info.uid as u32;
            let folder = info.full_path.clone();
            let p = provider.clone();
            tokio::spawn(async move {
                let result = if is_read {
                    p.mark_seen(&folder, uid).await
                } else {
                    p.mark_unseen(&folder, uid).await
                };
                if let Err(e) = result {
                    tracing::warn!("mark_seen/unseen failed: {}", e);
                }
            });
        }
    }

    if let Some(is_flagged) = req.is_flagged {
        sqlx::query("UPDATE messages SET is_flagged = ? WHERE id = ?")
            .bind(is_flagged)
            .bind(&message_id)
            .execute(&state.pool)
            .await?;

        if let Ok(Some((info, provider))) = load_provider(&state, &message_id).await {
            let uid = info.uid as u32;
            let folder = info.full_path.clone();
            tokio::spawn(async move {
                if let Err(e) = provider.set_flagged(&folder, uid, is_flagged).await {
                    tracing::warn!("set_flagged failed: {}", e);
                }
            });
        }
    }

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let folder_id = message_folder_id(&state.pool, &message_id).await?;

    // Fire IMAP delete in background.
    if let Ok(Some((info, provider))) = load_provider(&state, &message_id).await {
        let uid = info.uid as u32;
        let folder = info.full_path.clone();
        tokio::spawn(async move {
            if let Err(e) = provider.delete_message(&folder, uid).await {
                tracing::warn!("IMAP delete_message failed: {}", e);
            }
        });
    }

    // Remove from local DB immediately.
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(&message_id)
        .execute(&state.pool)
        .await?;
    if let Some(folder_id) = folder_id {
        refresh_folder_counts(&state.pool, &folder_id).await?;
    }

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn archive_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
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

    let archive_full_path = archive_folder.unwrap_or_else(|| "[Gmail]/All Mail".to_string());

    // Find the new folder_id for the archive folder in the DB.
    let archive_folder_id: Option<String> = sqlx::query_scalar(
        r#"SELECT f.id FROM folders f
           JOIN messages m ON m.account_id = f.account_id
           WHERE m.id = ? AND f.special_use = 'archive'
           LIMIT 1"#,
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?;

    if let Ok(Some((info, provider))) = load_provider(&state, &message_id).await {
        let uid = info.uid as u32;
        let src_folder = info.full_path.clone();
        let dest = archive_full_path.clone();
        tokio::spawn(async move {
            if let Err(e) = provider.move_message(&src_folder, uid, &dest).await {
                tracing::warn!("IMAP archive failed: {}", e);
            }
        });
    }

    if let Some(folder_id) = archive_folder_id {
        sqlx::query("UPDATE messages SET folder_id = ? WHERE id = ?")
            .bind(&folder_id)
            .bind(&message_id)
            .execute(&state.pool)
            .await?;
        if let Some(source_folder_id) = source_folder_id {
            refresh_folder_counts(&state.pool, &source_folder_id).await?;
        }
        refresh_folder_counts(&state.pool, &folder_id).await?;
    } else {
        sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(&message_id)
            .execute(&state.pool)
            .await?;
        if let Some(source_folder_id) = source_folder_id {
            refresh_folder_counts(&state.pool, &source_folder_id).await?;
        }
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
        r#"SELECT filename, content_type, content
           FROM attachments
           WHERE id = ? AND message_id = ?"#,
    )
    .bind(&attachment_id)
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("attachment {} not found", attachment_id)))?;

    let content = row.content.ok_or_else(|| {
        AppError::NotFound(format!("attachment {} content not cached", attachment_id))
    })?;
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
        r#"SELECT id, account_id, folder_id, uid, message_id, thread_id,
                  subject, from_name, from_email, to_json, date,
                  is_read, is_flagged, is_draft, has_attachments, preview, snoozed_until
           FROM messages
           WHERE folder_id = ? AND is_read = 0
             AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
           ORDER BY date DESC
           LIMIT ? OFFSET ?"#
    } else {
        r#"SELECT id, account_id, folder_id, uid, message_id, thread_id,
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

/// Row type for the join query used during lazy body fetch.
#[derive(Debug, sqlx::FromRow)]
struct MessageFetchInfo {
    uid: i64,
    full_path: String,
    email: String,
    provider_type: String,
    oauth_token_json: Option<String>,
}

pub async fn get_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let msg = sqlx::query_as::<_, MessageRow>(
        r#"SELECT id, account_id, folder_id, uid, message_id, thread_id,
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
        "SELECT message_id, html_body, text_body, raw_headers, fetched_at FROM message_bodies WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?;

    // Always load provider info — needed for both lazy body fetch and mark_seen.
    let fetch_info = sqlx::query_as::<_, MessageFetchInfo>(
        r#"SELECT m.uid, f.full_path, a.email, a.provider_type, a.oauth_token_json
           FROM messages m
           JOIN folders f ON m.folder_id = f.id
           JOIN accounts a ON m.account_id = a.id
           WHERE m.id = ?"#,
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?;

    // Build provider from fetch_info (shared by body fetch + mark_seen).
    let provider = if let Some(ref info) = fetch_info {
        if info.provider_type == "gmail" {
            let token_json = info
                .oauth_token_json
                .as_deref()
                .ok_or_else(|| AppError::Auth("account has no OAuth token".to_string()))?;

            let mut stored: StoredToken = serde_json::from_str(token_json)
                .map_err(|e| AppError::Auth(format!("parse token: {}", e)))?;

            if stored.is_expired() {
                let client_id = std::env::var("GOOGLE_CLIENT_ID")
                    .map_err(|_| AppError::Auth("GOOGLE_CLIENT_ID not set".to_string()))?;
                let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
                    .map_err(|_| AppError::Auth("GOOGLE_CLIENT_SECRET not set".to_string()))?;
                let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI").unwrap_or_else(|_| {
                    "http://localhost:3000/api/v1/auth/gmail/callback".to_string()
                });
                let oauth = OAuthConfig::gmail(client_id, client_secret, redirect_uri);
                let refresh = stored
                    .refresh_token
                    .as_deref()
                    .ok_or_else(|| AppError::Auth("no refresh token".to_string()))?;
                let new_resp = oauth.refresh_token(refresh).await?;
                stored = StoredToken::from_token_response(new_resp);
            }

            Some(GmailProvider::new(
                msg.account_id.clone(),
                info.email.clone(),
                stored.access_token.clone(),
            ))
        } else {
            None
        }
    } else {
        None
    };

    // Lazy-fetch body from IMAP on cache miss.
    if body.is_none() {
        if let (Some(ref info), Some(ref p)) = (&fetch_info, &provider) {
            info!("body cache miss for {}, fetching from IMAP", message_id);
            match p.fetch_message_body(&info.full_path, info.uid as u32).await {
                Ok(fetched) => {
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
                    });
                }
                Err(e) => {
                    tracing::warn!("failed to fetch IMAP body for {}: {}", message_id, e);
                }
            }
        }
    }

    // Fire-and-forget: set \Seen on the IMAP server (doesn't block the response).
    if !msg.is_read {
        if let (Some(info), Some(p)) = (fetch_info, provider) {
            tokio::spawn(async move {
                if let Err(e) = p.mark_seen(&info.full_path, info.uid as u32).await {
                    tracing::warn!("mark_seen failed: {}", e);
                }
            });
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
        let path = format!("/tmp/email_test_{}.db", uuid::Uuid::new_v4());
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
        .bind("t@t.com")
        .bind("generic_imap")
        .bind("password")
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

    async fn seed_attachment(
        pool: &SqlitePool,
        id: &str,
        message_id: &str,
        filename: &str,
        content_type: &str,
        content: Option<&[u8]>,
    ) {
        sqlx::query(
            "INSERT INTO attachments (id, message_id, filename, content_type, size_bytes, content) VALUES (?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(message_id)
        .bind(filename)
        .bind(content_type)
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
    async fn archive_with_archive_folder_moves_message() {
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

        let folder_id: String =
            sqlx::query_scalar("SELECT folder_id FROM messages WHERE id = 'mv-msg'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(folder_id, "fld-archive");
        assert_eq!(folder_counts(&state.pool, "fld-inbox").await, (0, 0));
        assert_eq!(folder_counts(&state.pool, "fld-archive").await, (1, 1));
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
    }
}
