use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use email_types::{
    DraftRequest, DraftResponse, OutgoingAttachment as RequestAttachment, SendRequest, SendResponse,
};

use crate::auth::oauth2::StoredToken;
use crate::error::{AppError, Result};
use crate::smtp::{self, OutboundAttachment, OutboundMessage, SmtpConfig};
use crate::state::AppState;

pub async fn send_message(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendRequest>,
) -> Result<Json<SendResponse>> {
    let max_attachment_bytes = std::env::var("EMAIL_RS_MAX_OUTBOUND_ATTACHMENT_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(25 * 1024 * 1024);
    let attachments = decode_attachments(req.attachments, max_attachment_bytes)?;

    let account =
        crate::providers::factory::load_account_config(&state.pool, &req.account_id).await?;

    let config = match account.provider_type.as_str() {
        "gmail" | "microsoft365" if account.auth_type == "oauth2" => {
            let token_json = account
                .oauth_token_json
                .as_deref()
                .ok_or_else(|| AppError::Auth("account has no OAuth token".to_string()))?;

            let stored: StoredToken = serde_json::from_str(token_json)
                .map_err(|e| AppError::Auth(format!("parse token: {e}")))?;
            SmtpConfig {
                host: if account.provider_type == "gmail" {
                    "smtp.gmail.com"
                } else {
                    "smtp.office365.com"
                }
                .to_string(),
                port: 587,
                username: account.email.clone(),
                password: stored.access_token,
                use_tls: false,
                xoauth2: true,
            }
        }
        "microsoft365" if account.auth_type == "app_password" || account.auth_type == "basic" => {
            let password = account
                .smtp_password
                .or(account.password)
                .ok_or_else(|| AppError::Auth("no password configured for account".to_string()))?;
            let host = account
                .smtp_host
                .unwrap_or_else(|| "smtp.office365.com".to_string());
            let port = account.smtp_port.unwrap_or(587) as u16;
            SmtpConfig {
                host,
                port,
                username: account.email.clone(),
                password,
                use_tls: false,
                xoauth2: false,
            }
        }
        _ => {
            let password = account
                .smtp_password
                .or(account.password)
                .unwrap_or_default();
            let host = account
                .smtp_host
                .or(account.host)
                .ok_or_else(|| AppError::Smtp("no SMTP host configured for account".to_string()))?;
            SmtpConfig {
                host,
                port: account.smtp_port.or(account.port).unwrap_or(587) as u16,
                username: account.email.clone(),
                password,
                use_tls: true,
                xoauth2: false,
            }
        }
    };

    let msg = OutboundMessage {
        from: account.email,
        message_id: None,
        to: req.to,
        cc: req.cc,
        bcc: req.bcc,
        subject: req.subject,
        text_body: req.text_body,
        html_body: req.html_body,
        in_reply_to: req.in_reply_to,
        attachments,
    };

    smtp::send_message(&config, msg).await?;

    Ok(Json(SendResponse {
        status: "sent".to_string(),
    }))
}

fn decode_attachments(
    request: Vec<RequestAttachment>,
    max_attachment_bytes: usize,
) -> Result<Vec<OutboundAttachment>> {
    let mut attachment_bytes = 0usize;
    let mut attachments = Vec::with_capacity(request.len());
    for attachment in request {
        if attachment.filename.trim().is_empty() {
            return Err(AppError::Validation(
                "attachment filename is required".to_string(),
            ));
        }
        let content = STANDARD.decode(&attachment.content_base64).map_err(|_| {
            AppError::Validation("attachment content is not valid base64".to_string())
        })?;
        attachment_bytes = attachment_bytes.saturating_add(content.len());
        if attachment_bytes > max_attachment_bytes {
            return Err(AppError::PayloadTooLarge(format!(
                "outgoing attachments exceed the configured {max_attachment_bytes} byte limit"
            )));
        }
        attachments.push(OutboundAttachment {
            filename: attachment.filename,
            content_type: attachment.content_type,
            content,
        });
    }
    Ok(attachments)
}

pub async fn create_draft(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DraftRequest>,
) -> Result<Json<DraftResponse>> {
    save_draft(&state, None, req).await.map(Json)
}

pub async fn update_draft(
    State(state): State<Arc<AppState>>,
    Path(draft_id): Path<String>,
    Json(req): Json<DraftRequest>,
) -> Result<Json<DraftResponse>> {
    save_draft(&state, Some(&draft_id), req).await.map(Json)
}

pub async fn delete_draft(
    State(state): State<Arc<AppState>>,
    Path(draft_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (account_id, folder, uid, logical_id, provider) = draft_provider(&state, &draft_id).await?;
    provider.delete_draft(&folder, uid).await?;
    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(&draft_id)
        .execute(&state.pool)
        .await?;
    crate::api::webhooks::fire_webhooks(
        &state.pool,
        &account_id,
        "message.deleted",
        serde_json::json!({"occurrenceId": draft_id, "logicalId": logical_id}),
    )
    .await;
    Ok(Json(serde_json::json!({"status": "deleted"})))
}

async fn save_draft(
    state: &AppState,
    existing_id: Option<&str>,
    req: DraftRequest,
) -> Result<DraftResponse> {
    let max_attachment_bytes = std::env::var("EMAIL_RS_MAX_OUTBOUND_ATTACHMENT_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(25 * 1024 * 1024);
    let attachments = decode_attachments(req.attachments, max_attachment_bytes)?;
    let account =
        crate::providers::factory::load_account_config(&state.pool, &req.account_id).await?;
    let (message_id, existing_logical_id, old_remote) = if let Some(id) = existing_id {
        let (account_id, folder, uid, _, provider) = draft_provider(state, id).await?;
        if account_id != req.account_id {
            return Err(AppError::Validation(
                "draft account cannot be changed".into(),
            ));
        }
        let (message_id, logical_id): (Option<String>, String) =
            sqlx::query_as("SELECT message_id, logical_id FROM messages WHERE id = ?")
                .bind(id)
                .fetch_one(&state.pool)
                .await?;
        (
            message_id.unwrap_or_else(new_draft_message_id),
            Some(logical_id),
            Some((id.to_string(), folder, uid, provider)),
        )
    } else {
        (new_draft_message_id(), None, None)
    };
    let (draft_folder_id, draft_folder, cached_uid_validity): (String, String, i64) = sqlx::query_as(
        "SELECT id, full_path, uid_validity FROM folders WHERE account_id = ? AND special_use = 'drafts' LIMIT 1",
    )
    .bind(&req.account_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| {
        AppError::Provider("account has no drafts folder; synchronize folders first".into())
    })?;
    let provider =
        crate::providers::factory::provider_for_account(&state.pool, &req.account_id).await?;
    let (text_body, html_body) = match (req.text_body, req.html_body) {
        (None, None) => (Some(String::new()), None),
        bodies => bodies,
    };
    let to = req.to;
    let cc = req.cc;
    let bcc = req.bcc;
    let subject = req.subject;
    let in_reply_to = req.in_reply_to;
    let message = smtp::build_message(&OutboundMessage {
        from: account.email.clone(),
        message_id: Some(message_id.clone()),
        to: to.clone(),
        cc: cc.clone(),
        bcc,
        subject: subject.clone(),
        text_body: text_body.clone(),
        html_body: html_body.clone(),
        in_reply_to: in_reply_to.clone(),
        attachments: attachments.clone(),
    })?;
    let raw = message.formatted();
    let uid = provider
        .append_draft(&draft_folder, &message_id, &raw)
        .await?;
    if let Some((old_id, old_folder, old_uid, old_provider)) = old_remote {
        if let Err(error) = old_provider.delete_draft(&old_folder, old_uid).await {
            tracing::warn!(
                "new draft saved but superseded draft {old_id} could not be removed: {error}"
            );
            return Err(error);
        }
        sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(old_id)
            .execute(&state.pool)
            .await?;
    }
    let uid_validity = provider
        .folder_uid_validity(&draft_folder)
        .await?
        .map(i64::from)
        .unwrap_or(cached_uid_validity);
    let id = uuid::Uuid::new_v4().to_string();
    let date = chrono::Utc::now().to_rfc3339();
    let logical_id = existing_logical_id.unwrap_or_else(|| {
        crate::db::logical_message_id(
            &req.account_id,
            Some(&message_id),
            Some(&account.email),
            Some(&date),
            Some(&subject),
            Some(&id),
        )
    });
    let to_json = serde_json::to_string(&to).expect("string address list must serialize");
    let cc_json = serde_json::to_string(&cc).expect("string address list must serialize");
    let preview = text_body
        .as_deref()
        .or(html_body.as_deref())
        .map(|body| body.chars().take(200).collect::<String>());
    sqlx::query(
        r#"INSERT INTO messages
           (id, logical_id, account_id, folder_id, uid, uid_validity, message_id, thread_id,
            subject, from_email, to_json, cc_json, date, is_read, is_draft,
            has_attachments, preview, synced_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 1, ?, ?, datetime('now'))"#,
    )
    .bind(&id)
    .bind(&logical_id)
    .bind(&req.account_id)
    .bind(&draft_folder_id)
    .bind(i64::from(uid))
    .bind(uid_validity)
    .bind(&message_id)
    .bind(
        in_reply_to
            .as_ref()
            .map(|parent| format!("rfc:{}", parent.trim_matches(['<', '>']))),
    )
    .bind(&subject)
    .bind(&account.email)
    .bind(to_json)
    .bind(cc_json)
    .bind(&date)
    .bind(!attachments.is_empty())
    .bind(&preview)
    .execute(&state.pool)
    .await?;
    sqlx::query(
        "INSERT INTO message_bodies (message_id, html_body, text_body, raw_headers) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&html_body)
    .bind(&text_body)
    .bind(String::from_utf8_lossy(&raw).split("\r\n\r\n").next())
    .execute(&state.pool)
    .await?;
    let provider_attachments = attachments
        .into_iter()
        .enumerate()
        .map(|(index, attachment)| crate::providers::ProviderAttachment {
            part_id: format!("draft-{}", index + 1),
            filename: attachment.filename,
            content_type: attachment.content_type,
            content_id: None,
            size_bytes: attachment.content.len() as i64,
            content: Some(attachment.content),
        })
        .collect::<Vec<_>>();
    crate::api::messages::cache_provider_attachments(&state.pool, &id, &provider_attachments)
        .await?;
    sqlx::query(
        r#"UPDATE folders
           SET total_count = (SELECT COUNT(*) FROM messages WHERE folder_id = ?),
               unread_count = (SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_read = 0)
           WHERE id = ?"#,
    )
    .bind(&draft_folder_id)
    .bind(&draft_folder_id)
    .bind(&draft_folder_id)
    .execute(&state.pool)
    .await?;
    crate::api::webhooks::fire_webhooks(
        &state.pool,
        &req.account_id,
        if existing_id.is_some() {
            "message.updated"
        } else {
            "message.created"
        },
        serde_json::json!({"occurrenceId": id, "logicalId": logical_id}),
    )
    .await;
    Ok(DraftResponse {
        occurrence_id: id.clone(),
        id,
        logical_id,
        message_id,
        status: if existing_id.is_some() {
            "updated"
        } else {
            "created"
        }
        .into(),
    })
}

async fn draft_provider(
    state: &AppState,
    draft_id: &str,
) -> Result<(
    String,
    String,
    u32,
    String,
    Arc<dyn crate::providers::MailProvider>,
)> {
    let row: Option<(String, String, i64, bool, String)> = sqlx::query_as(
        "SELECT m.account_id, f.full_path, m.uid, m.is_draft, m.logical_id FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
    )
    .bind(draft_id)
    .fetch_optional(&state.pool)
    .await?;
    let (account_id, folder, uid, is_draft, logical_id) =
        row.ok_or_else(|| AppError::NotFound(format!("draft {draft_id} not found")))?;
    if !is_draft {
        return Err(AppError::Validation(format!(
            "message {draft_id} is not a draft"
        )));
    }
    let provider =
        crate::providers::factory::provider_for_account(&state.pool, &account_id).await?;
    Ok((account_id, folder, uid as u32, logical_id, provider))
}

fn new_draft_message_id() -> String {
    format!("draft-{}@email-rs.local", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_draft_state() -> Arc<AppState> {
        let path = format!("/tmp/email_draft_test_{}.db", uuid::Uuid::new_v4());
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('account', 'Test', 'me@example.test', 'test', 'test')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO folders (id, account_id, name, full_path, special_use, uid_validity) VALUES ('drafts', 'account', 'Drafts', 'Drafts', 'drafts', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        Arc::new(AppState::new(pool, has_fts))
    }

    fn draft_request(subject: &str) -> DraftRequest {
        DraftRequest {
            account_id: "account".into(),
            to: vec!["you@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: subject.into(),
            text_body: None,
            html_body: None,
            in_reply_to: None,
            attachments: vec![],
        }
    }

    #[test]
    fn outgoing_attachments_decode_and_enforce_total_limit() {
        let attachment = RequestAttachment {
            filename: "report.txt".into(),
            content_type: "text/plain".into(),
            content_base64: "aGVsbG8=".into(),
        };
        let decoded = decode_attachments(vec![attachment.clone()], 5).unwrap();
        assert_eq!(decoded[0].content, b"hello");
        assert!(matches!(
            decode_attachments(vec![attachment], 4),
            Err(AppError::PayloadTooLarge(_))
        ));
    }

    #[test]
    fn outgoing_attachments_reject_invalid_base64_and_blank_names() {
        assert!(matches!(
            decode_attachments(
                vec![RequestAttachment {
                    filename: "bad.bin".into(),
                    content_type: "application/octet-stream".into(),
                    content_base64: "not base64".into(),
                }],
                100,
            ),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            decode_attachments(
                vec![RequestAttachment {
                    filename: "  ".into(),
                    content_type: "text/plain".into(),
                    content_base64: "aA==".into(),
                }],
                100,
            ),
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn synchronized_draft_create_update_delete_preserves_logical_identity() {
        let state = setup_draft_state().await;
        let created = create_draft(State(Arc::clone(&state)), Json(draft_request("First")))
            .await
            .unwrap()
            .0;
        assert_eq!(created.status, "created");
        let row: (String, bool, String) =
            sqlx::query_as("SELECT subject, is_draft, logical_id FROM messages WHERE id = ?")
                .bind(&created.id)
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(row.0, "First");
        assert!(row.1);
        assert_eq!(row.2, created.logical_id);

        let updated = update_draft(
            State(Arc::clone(&state)),
            Path(created.id.clone()),
            Json(draft_request("Second")),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(updated.status, "updated");
        assert_ne!(updated.id, created.id);
        assert_eq!(updated.message_id, created.message_id);
        assert_eq!(updated.logical_id, created.logical_id);
        let old_exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM messages WHERE id = ?")
            .bind(&created.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap();
        assert!(old_exists.is_none());

        let _ = delete_draft(State(Arc::clone(&state)), Path(updated.id.clone()))
            .await
            .unwrap();
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
