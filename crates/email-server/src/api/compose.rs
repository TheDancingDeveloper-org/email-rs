use std::sync::Arc;

use axum::{extract::State, Json};
use base64::{engine::general_purpose::STANDARD, Engine};
use email_types::{OutgoingAttachment as RequestAttachment, SendRequest, SendResponse};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
