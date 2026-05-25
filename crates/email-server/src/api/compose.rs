use std::sync::Arc;

use axum::{extract::State, Json};
use email_types::{SendRequest, SendResponse};

use crate::auth::oauth2::{OAuthConfig, StoredToken};
use crate::error::{AppError, Result};
use crate::smtp::{self, OutboundMessage, SmtpConfig};
use crate::state::AppState;

#[derive(Debug, sqlx::FromRow)]
struct AccountRow {
    email: String,
    provider_type: String,
    auth_type: String,
    oauth_token_json: Option<String>,
    host: Option<String>,
    port: Option<i64>,
    password: Option<String>,
    smtp_host: Option<String>,
    smtp_port: Option<i64>,
    smtp_password: Option<String>,
}

pub async fn send_message(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendRequest>,
) -> Result<Json<SendResponse>> {
    let account = sqlx::query_as::<_, AccountRow>(
        "SELECT email, provider_type, auth_type, oauth_token_json, host, port, password, smtp_host, smtp_port, smtp_password FROM accounts WHERE id = ?",
    )
    .bind(&req.account_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("account {} not found", req.account_id)))?;

    let config = match account.provider_type.as_str() {
        "gmail" => {
            let token_json = account
                .oauth_token_json
                .as_deref()
                .ok_or_else(|| AppError::Auth("account has no OAuth token".to_string()))?;

            let mut stored: StoredToken = serde_json::from_str(token_json)
                .map_err(|e| AppError::Auth(format!("parse token: {}", e)))?;

            if stored.is_expired() {
                let client_id = crate::auth::google_client_id()
                    .ok_or_else(|| AppError::Auth("GOOGLE_CLIENT_ID not configured".to_string()))?;
                let client_secret = crate::auth::google_client_secret().ok_or_else(|| {
                    AppError::Auth("GOOGLE_CLIENT_SECRET not configured".to_string())
                })?;
                let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI").unwrap_or_else(|_| {
                    "http://localhost:3000/api/v1/auth/gmail/callback".to_string()
                });
                let oauth = OAuthConfig::gmail(client_id, client_secret, redirect_uri);
                let refresh = stored
                    .refresh_token
                    .as_deref()
                    .ok_or_else(|| AppError::Auth("no refresh token".to_string()))?;
                stored = StoredToken::from_token_response(oauth.refresh_token(refresh).await?);
            }

            SmtpConfig {
                host: "smtp.gmail.com".to_string(),
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
