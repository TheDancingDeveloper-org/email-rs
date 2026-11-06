use std::{collections::BTreeSet, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE},
        HeaderMap, HeaderName, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use email_types::{
    AuthenticationResultStatus, BulkAction, BulkRequest, BulkResponse, BulkResult,
    Message as MessageRow, MessageAttachment, MessageAuthenticationResults, MoveMessageRequest,
    MoveMessageResponse, PatchMessageRequest,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use chrono;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CursorMessagesResponse {
    pub messages: Vec<MessageRow>,
    pub next_cursor: Option<String>,
}

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
use crate::providers::{MailProvider, MovedMessage, ProviderAttachment};
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
        Some(destination) if destination != source => provider
            .move_message(&source, uid, &destination)
            .await
            .map(|_| ()),
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

pub async fn move_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
    Json(body): Json<MoveMessageRequest>,
) -> Result<Json<MoveMessageResponse>> {
    transfer_message(state, message_id, body, false).await
}

pub async fn copy_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
    Json(body): Json<MoveMessageRequest>,
) -> Result<Json<MoveMessageResponse>> {
    transfer_message(state, message_id, body, true).await
}

async fn transfer_message(
    state: Arc<AppState>,
    message_id: String,
    body: MoveMessageRequest,
    copy: bool,
) -> Result<Json<MoveMessageResponse>> {
    ensure_message_exists(&state.pool, &message_id).await?;
    let event = message_event_context(&state.pool, &message_id).await?;
    let source_folder_id = message_folder_id(&state.pool, &message_id).await?;
    let destination: Option<(String,)> = sqlx::query_as(
        r#"SELECT f.full_path FROM folders f
           JOIN messages m ON m.account_id = f.account_id
           WHERE m.id = ? AND f.id = ?"#,
    )
    .bind(&message_id)
    .bind(&body.folder_id)
    .fetch_optional(&state.pool)
    .await?;
    let (destination,) = destination.ok_or_else(|| {
        AppError::Validation("destination folder not found in the message's account".to_string())
    })?;
    if source_folder_id.as_deref() == Some(body.folder_id.as_str()) {
        return Err(AppError::Validation(
            "message is already in that folder".to_string(),
        ));
    }

    let (source, uid, provider) = load_provider(&state, &message_id).await?;
    let moved = if copy {
        provider.copy_message(&source, uid, &destination).await?
    } else {
        provider.move_message(&source, uid, &destination).await?
    };
    // The server chose the destination UID. Record the destination occurrence
    // under a new id so the caller can move it back; the next sync upserts
    // onto the same row.
    let new_message_id =
        record_moved_message(&state.pool, &message_id, &body.folder_id, moved, copy).await?;
    if new_message_id.is_some() {
        refresh_folder_counts(&state.pool, &body.folder_id).await?;
    }
    if let Some(source_folder_id) = source_folder_id {
        refresh_folder_counts(&state.pool, &source_folder_id).await?;
    }
    if let Some((account_id, mut data)) = event {
        if let Some(object) = data.as_object_mut() {
            object.insert("destination".to_string(), destination.into());
        }
        let event_name = if copy {
            "message.copied"
        } else {
            "message.moved"
        };
        fire_webhooks(&state.pool, &account_id, event_name, data).await;
    }

    Ok(Json(MoveMessageResponse {
        status: if copy { "copied" } else { "moved" }.to_string(),
        message_id: new_message_id,
    }))
}

/// Replace the source occurrence with the destination occurrence the server
/// reported, copying its cached metadata and labels. Without a reported UID the
/// source row is only removed and the next sync discovers the destination.
async fn record_moved_message(
    pool: &sqlx::SqlitePool,
    source_id: &str,
    destination_folder_id: &str,
    moved: MovedMessage,
    copy: bool,
) -> Result<Option<String>> {
    let mut tx = pool.begin().await?;
    let mut new_id = None;
    if let (Some(uid), Some(uid_validity)) = (moved.uid, moved.uid_validity) {
        sqlx::query(
            r#"INSERT INTO messages
                   (id, logical_id, account_id, folder_id, uid, uid_validity, message_id, thread_id,
                    subject, from_name, from_email, to_json, cc_json, date,
                    is_read, is_flagged, is_draft, has_attachments, preview, synced_at)
               SELECT ?, logical_id, account_id, ?, ?, ?, message_id, thread_id,
                      subject, from_name, from_email, to_json, cc_json, date,
                      is_read, is_flagged, is_draft, has_attachments, preview, synced_at
               FROM messages WHERE id = ?
               ON CONFLICT(folder_id, uid_validity, uid) DO NOTHING"#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(destination_folder_id)
        .bind(uid as i64)
        .bind(uid_validity as i64)
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
        let id: String = sqlx::query_scalar(
            "SELECT id FROM messages WHERE folder_id = ? AND uid_validity = ? AND uid = ?",
        )
        .bind(destination_folder_id)
        .bind(uid_validity as i64)
        .bind(uid as i64)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT OR IGNORE INTO message_labels (id, message_id, label_id)
               SELECT lower(hex(randomblob(16))), ?, label_id FROM message_labels WHERE message_id = ?"#,
        )
        .bind(&id)
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
        new_id = Some(id);
    }
    if !copy {
        sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(new_id)
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
        r#"SELECT filename, content_type, content, part_id, size_bytes, file_sha256
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
    let store = crate::attachment_store::store_dir(&state.pool).await;
    let mut body = None;
    if let (Some(dir), Some(digest)) = (&store, &row.file_sha256) {
        body = crate::attachment_store::open_body(dir, digest).await;
    }
    let (body, content_length) = match (body, row.content) {
        (Some((body, len)), _) => (body, len),
        (None, Some(content)) => {
            let len = content.len() as u64;
            (Body::from(content), len)
        }
        (None, None) => {
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
            if content.len() <= attachment_cache_limit() {
                cache_attachment_content(&state.pool, store.as_deref(), &attachment_id, &content)
                    .await?;
            }
            let len = content.len() as u64;
            (Body::from(content), len)
        }
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&safe_attachment_content_type(&row.content_type))
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static("sandbox; default-src 'none'"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&attachment_content_disposition(&row.filename))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"attachment\"")),
    );

    headers.insert(CONTENT_LENGTH, HeaderValue::from(content_length));

    Ok((StatusCode::OK, headers, body).into_response())
}

/// Return the original RFC822 message without parsing or rewriting it.
pub async fn download_source(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Response> {
    let (folder, uid, provider) = load_provider(&state, &message_id).await?;
    let raw = provider.fetch_raw_message(&folder, uid).await?;
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("message/rfc822"));
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=message.eml"),
    );
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::try_from(raw.len().to_string()).expect("content length is valid"),
    );
    Ok((headers, Body::from(raw)).into_response())
}

/// Keep fetched attachment bytes: in the content-addressed store when the
/// database has a data directory, otherwise as a BLOB.
async fn cache_attachment_content(
    pool: &sqlx::SqlitePool,
    store: Option<&std::path::Path>,
    attachment_id: &str,
    content: &[u8],
) -> Result<()> {
    if let Some(dir) = store {
        match crate::attachment_store::put(dir, content).await {
            Ok(digest) => {
                sqlx::query("UPDATE attachments SET file_sha256 = ?, content = NULL WHERE id = ?")
                    .bind(digest)
                    .bind(attachment_id)
                    .execute(pool)
                    .await?;
                return Ok(());
            }
            Err(error) => {
                tracing::warn!(%error, "attachment store write failed; not caching");
                return Ok(());
            }
        }
    }
    sqlx::query("UPDATE attachments SET content = ? WHERE id = ?")
        .bind(content)
        .bind(attachment_id)
        .execute(pool)
        .await?;
    Ok(())
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
    let mut attachments = sqlx::query_as::<_, MessageAttachment>(
        r#"SELECT id, message_id, filename, content_type, size_bytes,
                  0 AS dangerous_or_double_extension
           FROM attachments
           WHERE message_id = ?
           ORDER BY filename COLLATE NOCASE, id"#,
    )
    .bind(message_id)
    .fetch_all(pool)
    .await?;
    for attachment in &mut attachments {
        attachment.dangerous_or_double_extension =
            dangerous_or_double_extension(&attachment.filename);
    }
    Ok(attachments)
}

const DANGEROUS_ATTACHMENT_EXTENSIONS: &[&str] = &[
    "app", "bat", "cmd", "com", "cpl", "exe", "hta", "jar", "js", "jse", "lnk", "msi", "pif",
    "ps1", "reg", "scr", "vbe", "vbs", "wsf",
];

fn dangerous_or_double_extension(filename: &str) -> bool {
    let filename = filename.trim().trim_end_matches('.').to_ascii_lowercase();
    let extensions = filename
        .split('.')
        .skip(1)
        .filter(|extension| !extension.is_empty())
        .collect::<Vec<_>>();
    let Some(extension) = extensions.last() else {
        return false;
    };
    DANGEROUS_ATTACHMENT_EXTENSIONS.contains(extension)
        || (extensions.len() > 1
            && !matches!(
                (extensions[extensions.len() - 2], *extension),
                ("tar", "gz" | "bz2" | "xz")
            ))
}

#[derive(Debug, Default, PartialEq, Eq)]
struct MessageTrustSignals {
    authentication_results: MessageAuthenticationResults,
    from_reply_to_domain_mismatch: bool,
    display_name_spoofing: bool,
}

fn message_trust_signals(
    raw_headers: Option<&str>,
    fallback_from: Option<&str>,
) -> MessageTrustSignals {
    let Some(raw_headers) = raw_headers else {
        return MessageTrustSignals::default();
    };
    let headers = unfolded_headers(raw_headers);
    let authentication_results = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("Authentication-Results"))
        .fold(
            MessageAuthenticationResults::default(),
            |mut results, (_, value)| {
                results.spf = authentication_status(value, "spf", results.spf);
                results.dkim = authentication_status(value, "dkim", results.dkim);
                results.dmarc = authentication_status(value, "dmarc", results.dmarc);
                results
            },
        );
    let from = header_value(&headers, "From")
        .and_then(parse_address)
        .or_else(|| fallback_from.map(|email| (String::new(), email.to_ascii_lowercase())));
    let reply_to = header_value(&headers, "Reply-To").and_then(parse_address);
    let from_reply_to_domain_mismatch = from.as_ref().zip(reply_to.as_ref()).is_some_and(
        |((_, from_email), (_, reply_to_email))| {
            email_domain(from_email) != email_domain(reply_to_email)
        },
    );
    let display_name_spoofing = from.as_ref().is_some_and(|(display_name, from_email)| {
        embedded_email(display_name)
            .is_some_and(|display_email| !display_email.eq_ignore_ascii_case(from_email))
    });

    MessageTrustSignals {
        authentication_results,
        from_reply_to_domain_mismatch,
        display_name_spoofing,
    }
}

pub(crate) fn unfolded_headers(raw_headers: &str) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in raw_headers.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    headers
}

pub(crate) fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn authentication_status(
    value: &str,
    method: &str,
    current: AuthenticationResultStatus,
) -> AuthenticationResultStatus {
    value
        .split(|character: char| character == ';' || character.is_whitespace())
        .find_map(|token| {
            let (name, result) = token.split_once('=')?;
            name.eq_ignore_ascii_case(method)
                .then(|| match result.to_ascii_lowercase().as_str() {
                    "pass" => AuthenticationResultStatus::Pass,
                    "none" => AuthenticationResultStatus::None,
                    _ => AuthenticationResultStatus::Fail,
                })
        })
        .unwrap_or(current)
}

fn parse_address(value: &str) -> Option<(String, String)> {
    let (display_name, address) = match (value.rfind('<'), value.rfind('>')) {
        (Some(start), Some(end)) if start < end => (&value[..start], &value[start + 1..end]),
        _ => ("", value.split(',').next()?),
    };
    let address = address.trim().to_ascii_lowercase();
    address.contains('@').then(|| {
        (
            display_name.trim().trim_matches('"').trim().to_string(),
            address,
        )
    })
}

fn embedded_email(value: &str) -> Option<&str> {
    value
        .split(|character: char| {
            character.is_whitespace() || matches!(character, '<' | '>' | '(' | ')' | '"')
        })
        .map(|token| {
            token.trim_matches(|character: char| matches!(character, ',' | ';' | ':' | '[' | ']'))
        })
        .find(|token| {
            token.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
            })
        })
}

fn email_domain(email: &str) -> Option<&str> {
    email
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim_end_matches('.'))
}

#[derive(Debug, sqlx::FromRow)]
struct AttachmentContentRow {
    filename: String,
    content_type: String,
    content: Option<Vec<u8>>,
    part_id: String,
    size_bytes: i64,
    file_sha256: Option<String>,
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
    let store = crate::attachment_store::store_dir_conn(tx).await;
    let mut desired = std::collections::HashSet::new();
    for attachment in attachments {
        let id = format!("{message_id}:{}", attachment.part_id);
        desired.insert(id.clone());
        let cached_content = attachment
            .content
            .as_deref()
            .filter(|content| content.len() <= cache_limit);
        let mut file_sha256 = None;
        let mut blob = cached_content;
        if let (Some(dir), Some(content)) = (&store, cached_content) {
            match crate::attachment_store::put(dir, content).await {
                Ok(digest) => {
                    file_sha256 = Some(digest);
                    blob = None;
                }
                Err(error) => {
                    tracing::warn!(%error, "attachment store write failed; keeping BLOB")
                }
            }
        }
        sqlx::query(
            r#"INSERT INTO attachments
               (id, message_id, filename, content_type, part_id, content_id, size_bytes, content, file_sha256)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(message_id, part_id) DO UPDATE SET
                   filename = excluded.filename,
                   content_type = excluded.content_type,
                   content_id = excluded.content_id,
                   size_bytes = excluded.size_bytes,
                   content = CASE WHEN excluded.file_sha256 IS NOT NULL THEN NULL
                                  ELSE COALESCE(excluded.content, attachments.content) END,
                   file_sha256 = COALESCE(excluded.file_sha256, attachments.file_sha256)"#,
        )
        .bind(id)
        .bind(message_id)
        .bind(&attachment.filename)
        .bind(&attachment.content_type)
        .bind(&attachment.part_id)
        .bind(&attachment.content_id)
        .bind(attachment.size_bytes)
        .bind(blob)
        .bind(file_sha256)
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

/// Content types the sender may choose that are inert when rendered inline.
/// Anything else (html, svg, xml, js, unknown) is served as an opaque download.
const SAFE_ATTACHMENT_TYPES: &[&str] = &[
    "application/pdf",
    "text/plain",
    "text/csv",
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
    "audio/mpeg",
    "audio/ogg",
    "audio/wav",
    "video/mp4",
    "video/webm",
];

fn safe_attachment_content_type(content_type: &str) -> String {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if SAFE_ATTACHMENT_TYPES.contains(&essence.as_str()) {
        essence
    } else {
        "application/octet-stream".to_string()
    }
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
    /// An empty cursor selects the first cursor page; subsequent values are opaque.
    pub cursor: Option<String>,
}

pub(crate) fn decode_message_cursor(
    cursor: &str,
) -> crate::error::Result<Option<(String, String)>> {
    if cursor.is_empty() {
        return Ok(None);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| AppError::Validation("invalid message cursor".to_string()))?;
    serde_json::from_slice(&decoded)
        .map(Some)
        .map_err(|_| AppError::Validation("invalid message cursor".to_string()))
}

pub(crate) fn encode_message_cursor(date: Option<&str>, id: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&(date.unwrap_or_default(), id)).expect("cursor tuple serializes"),
    )
}

pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    Path(folder_id): Path<String>,
    Query(q): Query<ListMessagesQuery>,
) -> Result<axum::response::Response> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;
    let unread_only = q.unread_only.unwrap_or(false);

    if let Some(cursor) = q.cursor.as_deref() {
        let after = decode_message_cursor(cursor)?;
        let rows = if unread_only {
            sqlx::query_as::<_, MessageRow>(
                r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id, thread_id,
                          subject, from_name, from_email, to_json, date, is_read, is_flagged, is_draft,
                          has_attachments, preview, snoozed_until
                   FROM messages
                   WHERE folder_id = ? AND is_read = 0
                     AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                     AND (? IS NULL OR date < ? OR (date = ? AND id < ?))
                   ORDER BY date DESC, id DESC LIMIT ?"#,
            )
            .bind(&folder_id)
            .bind(after.as_ref().map(|(date, _)| date))
            .bind(after.as_ref().map(|(date, _)| date))
            .bind(after.as_ref().map(|(date, _)| date))
            .bind(after.as_ref().map(|(_, id)| id))
            .bind(per_page + 1)
            .fetch_all(&state.pool)
            .await?
        } else {
            sqlx::query_as::<_, MessageRow>(
                r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id, thread_id,
                          subject, from_name, from_email, to_json, date, is_read, is_flagged, is_draft,
                          has_attachments, preview, snoozed_until
                   FROM messages
                   WHERE folder_id = ?
                     AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                     AND (? IS NULL OR date < ? OR (date = ? AND id < ?))
                   ORDER BY date DESC, id DESC LIMIT ?"#,
            )
            .bind(&folder_id)
            .bind(after.as_ref().map(|(date, _)| date))
            .bind(after.as_ref().map(|(date, _)| date))
            .bind(after.as_ref().map(|(date, _)| date))
            .bind(after.as_ref().map(|(_, id)| id))
            .bind(per_page + 1)
            .fetch_all(&state.pool)
            .await?
        };
        let mut messages = rows;
        let next_cursor = if messages.len() > per_page as usize {
            messages.truncate(per_page as usize);
            messages
                .last()
                .map(|message| encode_message_cursor(message.date.as_deref(), &message.id))
        } else {
            None
        };
        return Ok(Json(CursorMessagesResponse {
            messages,
            next_cursor,
        })
        .into_response());
    }

    let sql = if unread_only {
        r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id, thread_id,
                  subject, from_name, from_email, to_json, date,
                  is_read, is_flagged, is_draft, has_attachments, preview, snoozed_until
           FROM messages
           WHERE folder_id = ? AND is_read = 0
             AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
           ORDER BY date DESC, id DESC
           LIMIT ? OFFSET ?"#
    } else {
        r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id, thread_id,
                  subject, from_name, from_email, to_json, date,
                  is_read, is_flagged, is_draft, has_attachments, preview, snoozed_until
           FROM messages
           WHERE folder_id = ?
             AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
           ORDER BY date DESC, id DESC
           LIMIT ? OFFSET ?"#
    };

    let rows = sqlx::query_as::<_, MessageRow>(sql)
        .bind(&folder_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(rows).into_response())
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
    let body_headers = body.as_ref().and_then(|body| body.raw_headers.clone());
    let trust_signals = message_trust_signals(body_headers.as_deref(), msg.from_email.as_deref());

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
            "htmlBody": b.html_body.as_deref().map(crate::sanitize::sanitize_html),
            "textBody": b.text_body,
            "rawHeaders": b.raw_headers,
            "cleanBody": b.clean_body,
            "quotedHistory": b.quoted_history,
            "signatureStripped": b.signature_stripped,
        })),
        "authenticationResults": trust_signals.authentication_results,
        "fromReplyToDomainMismatch": trust_signals.from_reply_to_domain_mismatch,
        "displayNameSpoofing": trust_signals.display_name_spoofing,
        "listUnsubscribe": body_headers
            .as_deref()
            .and_then(super::unsubscribe::parse_list_unsubscribe),
        "attachments": attachments,
    });

    Ok(Json(result))
}

#[cfg(test)]
mod attachment_type_tests {
    use super::{dangerous_or_double_extension as dangerous, safe_attachment_content_type as safe};

    #[test]
    fn active_types_are_forced_to_octet_stream() {
        for t in [
            "text/html",
            "TEXT/HTML; charset=utf-8",
            "image/svg+xml",
            "application/xml",
            "text/xml",
            "application/javascript",
            "application/xhtml+xml",
            "",
        ] {
            assert_eq!(safe(t), "application/octet-stream", "{t}");
        }
    }

    #[test]
    fn allowlisted_types_pass() {
        assert_eq!(safe("Image/PNG"), "image/png");
        assert_eq!(safe("application/pdf; name=a.pdf"), "application/pdf");
    }

    #[test]
    fn dangerous_and_double_extensions_are_flagged() {
        for filename in [
            "invoice.exe",
            "invoice.pdf.exe",
            "scan.pdf.js",
            "photo.jpg.zip",
        ] {
            assert!(dangerous(filename), "{filename}");
        }
        for filename in ["invoice.pdf", "archive.tar.gz", "README"] {
            assert!(!dangerous(filename), "{filename}");
        }
    }
}

#[cfg(test)]
mod trust_signal_tests {
    use super::message_trust_signals;
    use email_types::AuthenticationResultStatus::{Fail, None as NoResult, Pass};

    #[test]
    fn parses_realistic_folded_authentication_results() {
        let headers = concat!(
            "From: Example Billing <billing@example.com>\r\n",
            "Reply-To: Support <support.example@payments.test>\r\n",
            "Authentication-Results: mx.example.net;\r\n",
            " spf=pass smtp.mailfrom=example.com;\r\n",
            " dkim=fail header.d=example.com; dmarc=none header.from=example.com\r\n",
        );

        let signals = message_trust_signals(Some(headers), None);

        assert_eq!(signals.authentication_results.spf, Pass);
        assert_eq!(signals.authentication_results.dkim, Fail);
        assert_eq!(signals.authentication_results.dmarc, NoResult);
        assert!(signals.from_reply_to_domain_mismatch);
        assert!(!signals.display_name_spoofing);
    }

    #[test]
    fn detects_display_name_address_spoofing() {
        let headers = concat!(
            "From: \"accounts@example.com\" <attacker@evil.test>\r\n",
            "Reply-To: attacker@evil.test\r\n",
            "Authentication-Results: gateway.test; spf=fail; dkim=pass; dmarc=fail\r\n",
        );

        let signals = message_trust_signals(Some(headers), None);

        assert_eq!(signals.authentication_results.spf, Fail);
        assert_eq!(signals.authentication_results.dkim, Pass);
        assert_eq!(signals.authentication_results.dmarc, Fail);
        assert!(!signals.from_reply_to_domain_mismatch);
        assert!(signals.display_name_spoofing);
    }
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
    async fn list_messages_cursor_paginates_tied_dates() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        for id in ["cursor-a", "cursor-b", "cursor-c", "cursor-d", "cursor-e"] {
            seed_message(&state.pool, id, "acc1", "fld1").await;
        }
        sqlx::query("UPDATE messages SET date = '2026-01-01T00:00:00Z' WHERE folder_id = 'fld1'")
            .execute(&state.pool)
            .await
            .unwrap();

        let (status, first) = req(
            crate::api::router(Arc::clone(&state)),
            "GET",
            "/folders/fld1/messages?cursor=&per_page=2",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let first_ids: Vec<&str> = first["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["id"].as_str().unwrap())
            .collect();
        assert_eq!(first_ids.len(), 2);
        let cursor = first["nextCursor"].as_str().unwrap();

        let (status, second) = req(
            crate::api::router(state),
            "GET",
            &format!("/folders/fld1/messages?cursor={cursor}&per_page=3"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let second_ids: Vec<&str> = second["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["id"].as_str().unwrap())
            .collect();
        assert_eq!(second_ids.len(), 3);
        assert!(first_ids.iter().all(|id| !second_ids.contains(id)));
        assert!(second["nextCursor"].is_null());
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
        assert_eq!(body["authenticationResults"]["spf"], "none");
        assert_eq!(body["authenticationResults"]["dkim"], "none");
        assert_eq!(body["authenticationResults"]["dmarc"], "none");
        assert_eq!(body["fromReplyToDomainMismatch"], false);
        assert_eq!(body["displayNameSpoofing"], false);
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
    async fn get_message_serves_sanitized_html() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-hostile", "acc1", "fld1").await;
        seed_body(
            &state.pool,
            "msg-hostile",
            None,
            Some(r#"<p onclick="x()">Hi</p><script>alert(1)</script><img src="https://t.example/p.gif"><a href="javascript:alert(1)">l</a>"#),
        )
        .await;

        let (_, body) = req(
            crate::api::router(Arc::clone(&state)),
            "GET",
            "/messages/msg-hostile",
            None,
        )
        .await;
        let html = body["body"]["htmlBody"].as_str().unwrap();
        assert!(html.contains("Hi"));
        for bad in ["script", "onclick", "t.example", "javascript:"] {
            assert!(!html.contains(bad), "{bad} survived: {html}");
        }
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
    async fn download_source_returns_original_rfc822_and_headers() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld1", "acc1", None).await;
        seed_message(&state.pool, "msg-source", "acc1", "fld1").await;

        let (status, headers, bytes) = req_raw(
            crate::api::router(state),
            "GET",
            "/messages/msg-source/source",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            bytes,
            b"From: sender@example.com\r\nSubject: Test\r\n\r\nraw body\r\n"
        );
        assert_eq!(headers["content-type"], "message/rfc822");
        assert_eq!(
            headers["content-disposition"],
            "attachment; filename=message.eml"
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
    async fn copy_without_copyuid_keeps_source_until_destination_sync() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "inbox", "acc1", Some("inbox")).await;
        seed_folder(&state.pool, "projects", "acc1", None).await;
        seed_message(&state.pool, "original", "acc1", "inbox").await;
        let result = super::record_moved_message(
            &state.pool,
            "original",
            "projects",
            crate::providers::MovedMessage {
                uid: None,
                uid_validity: None,
            },
            true,
        )
        .await
        .unwrap();
        assert_eq!(result, None);
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'original'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1);
    }

    #[tokio::test]
    async fn copy_to_folder_preserves_source_occurrence_and_refreshes_counts() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld-inbox", "acc1", Some("inbox")).await;
        seed_folder(&state.pool, "fld-projects", "acc1", None).await;
        seed_message(&state.pool, "move-msg", "acc1", "fld-inbox").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/move-msg/copy",
            Some(serde_json::json!({"folderId": "fld-projects"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "copied");
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'move-msg'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(folder_counts(&state.pool, "fld-inbox").await, (1, 1));

        // The destination occurrence exists under the id the API returned.
        let new_id = body["messageId"].as_str().expect("messageId").to_string();
        assert_ne!(new_id, "move-msg");
        let (folder, subject): (String, Option<String>) =
            sqlx::query_as("SELECT folder_id, subject FROM messages WHERE id = ?")
                .bind(&new_id)
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(folder, "fld-projects");
        assert_eq!(subject.as_deref(), Some("Subject of move-msg"));
        assert_eq!(folder_counts(&state.pool, "fld-projects").await.1, 1);
    }

    #[tokio::test]
    async fn move_to_folder_removes_source_occurrence_and_refreshes_counts() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld-inbox", "acc1", Some("inbox")).await;
        seed_folder(&state.pool, "fld-projects", "acc1", None).await;
        seed_message(&state.pool, "move-msg", "acc1", "fld-inbox").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/move-msg/move",
            Some(serde_json::json!({"folderId": "fld-projects"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "moved");
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'move-msg'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(folder_counts(&state.pool, "fld-inbox").await, (0, 0));

        // The destination occurrence exists under the id the API returned.
        let new_id = body["messageId"].as_str().expect("messageId").to_string();
        assert_ne!(new_id, "move-msg");
        let (folder, subject): (String, Option<String>) =
            sqlx::query_as("SELECT folder_id, subject FROM messages WHERE id = ?")
                .bind(&new_id)
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(folder, "fld-projects");
        assert_eq!(subject.as_deref(), Some("Subject of move-msg"));
        assert_eq!(folder_counts(&state.pool, "fld-projects").await.1, 1);
    }

    #[tokio::test]
    async fn move_then_move_back_with_the_returned_id_round_trips() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        seed_folder(&state.pool, "fld-inbox", "acc1", Some("inbox")).await;
        seed_folder(&state.pool, "fld-projects", "acc1", None).await;
        seed_message(&state.pool, "rt-msg", "acc1", "fld-inbox").await;

        let (status, body) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/rt-msg/move",
            Some(serde_json::json!({"folderId": "fld-projects"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let moved_id = body["messageId"].as_str().unwrap().to_string();

        let (status, back) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            &format!("/messages/{moved_id}/move"),
            Some(serde_json::json!({"folderId": "fld-inbox"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{back}");
        let restored_id = back["messageId"].as_str().unwrap();
        let folder: String = sqlx::query_scalar("SELECT folder_id FROM messages WHERE id = ?")
            .bind(restored_id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(folder, "fld-inbox");
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(folder_counts(&state.pool, "fld-projects").await.1, 0);
    }

    #[tokio::test]
    async fn copy_rejects_same_folder_and_foreign_account_folder() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('acc2','T','u2@example.com','test','test')",
        )
        .execute(&state.pool)
        .await
        .unwrap();
        seed_folder(&state.pool, "fld-inbox", "acc1", Some("inbox")).await;
        seed_folder(&state.pool, "fld-other", "acc2", None).await;
        seed_message(&state.pool, "stay-msg", "acc1", "fld-inbox").await;

        for folder in ["fld-inbox", "fld-other", "missing"] {
            let (status, _) = req(
                crate::api::router(Arc::clone(&state)),
                "POST",
                "/messages/stay-msg/copy",
                Some(serde_json::json!({ "folderId": folder })),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{folder}");
        }
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/nope/copy",
            Some(serde_json::json!({"folderId": "fld-inbox"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'stay-msg'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1);
    }

    #[tokio::test]
    async fn move_rejects_same_folder_and_foreign_account_folder() {
        let state = setup().await;
        seed_account(&state.pool, "acc1").await;
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('acc2','T','u2@example.com','test','test')",
        )
        .execute(&state.pool)
        .await
        .unwrap();
        seed_folder(&state.pool, "fld-inbox", "acc1", Some("inbox")).await;
        seed_folder(&state.pool, "fld-other", "acc2", None).await;
        seed_message(&state.pool, "stay-msg", "acc1", "fld-inbox").await;

        for folder in ["fld-inbox", "fld-other", "missing"] {
            let (status, _) = req(
                crate::api::router(Arc::clone(&state)),
                "POST",
                "/messages/stay-msg/move",
                Some(serde_json::json!({ "folderId": folder })),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{folder}");
        }
        let (status, _) = req(
            crate::api::router(Arc::clone(&state)),
            "POST",
            "/messages/nope/move",
            Some(serde_json::json!({"folderId": "fld-inbox"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE id = 'stay-msg'")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1);
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
