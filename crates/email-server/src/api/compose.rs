use std::sync::Arc;

use axum::{extract::State, Json};
use email_types::{SendRequest, SendResponse};

use crate::auth::oauth2::StoredToken;
use crate::error::{AppError, Result};
use crate::smtp::{self, OutboundMessage, SmtpConfig};
use crate::state::AppState;

pub async fn send_message(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendRequest>,
) -> Result<Json<SendResponse>> {
    let account =
        crate::providers::factory::load_account_config(&state.pool, &req.account_id).await?;

    let config = match account.provider_type.as_str() {
        "gmail" | "microsoft365" if account.auth_type == "oauth2" => {
            let token_json = account
                .oauth_token_json
                .as_deref()
                .ok_or_else(|| AppError::Auth("account has no OAuth token".to_string()))?;

            let stored: StoredToken = serde_json::from_str(token_json)
                .map_err(|e| AppError::Auth(format!("parse token: {}", e)))?;
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
                use_tls: false, // STARTTLS on 587
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
                use_tls: false, // STARTTLS on 587
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
    };

    smtp::send_message(&config, msg).await?;

    Ok(Json(SendResponse {
        status: "sent".to_string(),
    }))
}
