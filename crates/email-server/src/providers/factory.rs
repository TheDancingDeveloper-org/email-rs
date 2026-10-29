use std::sync::Arc;

use sqlx::SqlitePool;

use super::{generic_imap::GenericImapProvider, gmail::GmailProvider, MailProvider};
use crate::auth::oauth2::{OAuthConfig, StoredToken};
use crate::auth::token_source::{lock_account_refresh, AccessTokenSource, TokenStore};
use crate::error::{AppError, Result};

#[derive(Debug, sqlx::FromRow)]
pub struct AccountConfig {
    pub id: String,
    pub email: String,
    pub provider_type: String,
    pub auth_type: String,
    pub oauth_token_json: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub password: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<i64>,
    pub smtp_password: Option<String>,
}

pub async fn load_account_config(pool: &SqlitePool, account_id: &str) -> Result<AccountConfig> {
    let mut account = sqlx::query_as::<_, AccountConfig>(
        r#"SELECT id, email, provider_type, auth_type, oauth_token_json, host, port, password,
                  smtp_host, smtp_port, smtp_password FROM accounts WHERE id = ?"#,
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("account {account_id} not found")))?;
    account.oauth_token_json = crate::credentials::open_optional(account.oauth_token_json)?;
    account.password = crate::credentials::open_optional(account.password)?;
    account.smtp_password = crate::credentials::open_optional(account.smtp_password)?;

    if account.auth_type == "oauth2" {
        let raw = account
            .oauth_token_json
            .as_deref()
            .ok_or_else(|| AppError::Auth("account has no OAuth token".into()))?;
        let token: StoredToken = serde_json::from_str(raw)
            .map_err(|error| AppError::Auth(format!("parse OAuth token: {error}")))?;
        if token.is_expired() {
            // Same per-account lock as the per-connection refresh, so a send or
            // calendar sync never refreshes alongside a running mail sync.
            let _guard = lock_account_refresh(&account.id).await;
            let persisted = load_stored_token(pool, &account.id).await?;
            let token = if persisted.is_expired() {
                refresh_oauth_token(pool, &account.id, &account.provider_type, persisted).await?
            } else {
                persisted
            };
            account.oauth_token_json = Some(
                serde_json::to_string(&token)
                    .map_err(|error| AppError::Auth(format!("serialize OAuth token: {error}")))?,
            );
        }
    }
    Ok(account)
}

/// Construct the mail provider used by sync, reads, and mutations. Token
/// refresh is centralized here so every API path persists the same credentials.
pub async fn provider_for_account(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Arc<dyn MailProvider>> {
    let account = load_account_config(pool, account_id).await?;

    #[cfg(test)]
    if matches!(account.provider_type.as_str(), "test" | "test_fail") {
        return Ok(Arc::new(TestMailProvider {
            fail: account.provider_type == "test_fail",
        }));
    }

    if matches!(account.auth_type.as_str(), "basic" | "app_password") {
        let password = account
            .password
            .ok_or_else(|| AppError::Auth("account has no password configured".to_string()))?;
        let default_host = match account.provider_type.as_str() {
            "microsoft365" => Some("outlook.office365.com"),
            _ => None,
        };
        let host = account
            .host
            .or_else(|| default_host.map(ToString::to_string))
            .ok_or_else(|| AppError::Auth("IMAP host is not configured".to_string()))?;
        return Ok(Arc::new(GenericImapProvider::new(
            account.id,
            account.email,
            password,
            host,
            account.port.unwrap_or(993) as u16,
        )));
    }

    let raw_token = account
        .oauth_token_json
        .as_deref()
        .ok_or_else(|| AppError::Auth("account has no OAuth token".to_string()))?;
    let token: StoredToken = serde_json::from_str(raw_token)
        .map_err(|error| AppError::Auth(format!("parse OAuth token: {error}")))?;
    if !matches!(account.provider_type.as_str(), "gmail" | "microsoft365") {
        return Err(AppError::Provider(format!(
            "unsupported provider {}",
            account.provider_type
        )));
    }
    // Every IMAP connection the provider opens asks this source for a token,
    // so a long sync refreshes mid-run instead of outliving its first token.
    let tokens = Arc::new(AccessTokenSource::new(
        account.id.clone(),
        token,
        Arc::new(DbTokenStore {
            pool: pool.clone(),
            account_id: account.id.clone(),
            provider_type: account.provider_type.clone(),
        }),
    ));
    if account.provider_type == "gmail" {
        Ok(Arc::new(GmailProvider::new(
            account.id,
            account.email,
            tokens,
        )))
    } else {
        Ok(Arc::new(GmailProvider::new_outlook(
            account.id,
            account.email,
            tokens,
        )))
    }
}

/// The account's OAuth token as persisted in `accounts.oauth_token_json`.
struct DbTokenStore {
    pool: SqlitePool,
    account_id: String,
    provider_type: String,
}

#[async_trait::async_trait]
impl TokenStore for DbTokenStore {
    async fn load(&self) -> Result<StoredToken> {
        load_stored_token(&self.pool, &self.account_id).await
    }

    async fn refresh(&self, current: &StoredToken) -> Result<StoredToken> {
        refresh_oauth_token(
            &self.pool,
            &self.account_id,
            &self.provider_type,
            current.clone(),
        )
        .await
    }
}

async fn load_stored_token(pool: &SqlitePool, account_id: &str) -> Result<StoredToken> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT oauth_token_json FROM accounts WHERE id = ?")
            .bind(account_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let raw = crate::credentials::open_optional(raw)?
        .ok_or_else(|| AppError::Auth("account has no OAuth token".to_string()))?;
    serde_json::from_str(&raw)
        .map_err(|error| AppError::Auth(format!("parse OAuth token: {error}")))
}

/// Exchange the refresh token and persist the new token. Callers hold
/// `lock_account_refresh` for the account.
async fn refresh_oauth_token(
    pool: &SqlitePool,
    account_id: &str,
    provider_type: &str,
    token: StoredToken,
) -> Result<StoredToken> {
    let refresh_token = token
        .refresh_token
        .clone()
        .ok_or_else(|| AppError::Auth("OAuth refresh token is missing".to_string()))?;
    let oauth = match provider_type {
        "gmail" => OAuthConfig::gmail(
            crate::auth::google_client_id()
                .ok_or_else(|| AppError::Auth("GOOGLE_CLIENT_ID is not configured".to_string()))?,
            crate::auth::google_client_secret().ok_or_else(|| {
                AppError::Auth("GOOGLE_CLIENT_SECRET is not configured".to_string())
            })?,
            std::env::var("GOOGLE_REDIRECT_URI")
                .unwrap_or_else(|_| "http://localhost:8585/api/v1/auth/gmail/callback".to_string()),
        ),
        "microsoft365" => OAuthConfig::outlook(
            crate::auth::microsoft_client_id().ok_or_else(|| {
                AppError::Auth("MICROSOFT_CLIENT_ID is not configured".to_string())
            })?,
            std::env::var("MICROSOFT_REDIRECT_URI").unwrap_or_else(|_| {
                "http://localhost:8585/api/v1/auth/microsoft/callback".to_string()
            }),
        ),
        provider => {
            return Err(AppError::Provider(format!(
                "unsupported OAuth provider {provider}"
            )))
        }
    };
    let response = oauth.refresh_token(&refresh_token).await?;
    let mut refreshed = StoredToken::from_token_response(response);
    if refreshed.refresh_token.is_none() {
        refreshed.refresh_token = Some(refresh_token);
    }
    let json = serde_json::to_string(&refreshed)
        .map_err(|error| AppError::Auth(format!("serialize OAuth token: {error}")))?;
    sqlx::query("UPDATE accounts SET oauth_token_json = ?, token_expiry = ? WHERE id = ?")
        .bind(crate::credentials::seal(&json)?)
        .bind(refreshed.expires_at.map(|value| value.to_string()))
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(refreshed)
}

#[cfg(test)]
struct TestMailProvider {
    fail: bool,
}

#[cfg(test)]
impl TestMailProvider {
    fn mutation(&self) -> Result<()> {
        if self.fail {
            Err(AppError::Provider(
                "injected remote mutation failure".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl MailProvider for TestMailProvider {
    fn provider_id(&self) -> &str {
        "gmail"
    }

    async fn authenticate(&mut self) -> Result<()> {
        Ok(())
    }

    async fn list_folders(&self) -> Result<Vec<super::Folder>> {
        Ok(Vec::new())
    }

    async fn folder_uid_validity(&self, _folder: &str) -> Result<Option<u32>> {
        Ok(Some(1))
    }

    async fn fetch_messages(
        &self,
        _folder: &str,
        _since: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<super::Message>> {
        Ok(Vec::new())
    }

    async fn list_message_uids(&self, _folder: &str) -> Result<Vec<u32>> {
        Ok(Vec::new())
    }

    async fn fetch_message_body(&self, _folder: &str, _uid: u32) -> Result<super::MessageBody> {
        Err(AppError::NotFound("message body is not cached".to_string()))
    }

    async fn fetch_bodies_batch(
        &self,
        _folder: &str,
        _uids: &[u32],
    ) -> Result<Vec<(u32, super::MessageBody)>> {
        Ok(Vec::new())
    }

    async fn mark_seen(&self, _folder: &str, _uid: u32) -> Result<()> {
        self.mutation()
    }

    async fn mark_unseen(&self, _folder: &str, _uid: u32) -> Result<()> {
        self.mutation()
    }

    async fn delete_message(&self, _folder: &str, _uid: u32) -> Result<()> {
        self.mutation()
    }

    async fn move_message(&self, _src: &str, _uid: u32, _dest: &str) -> Result<()> {
        self.mutation()
    }

    async fn set_flagged(&self, _folder: &str, _uid: u32, _flagged: bool) -> Result<()> {
        self.mutation()
    }

    async fn append_draft(
        &self,
        _folder: &str,
        _message_id: &str,
        _raw_message: &[u8],
    ) -> Result<u32> {
        self.mutation().map(|()| 1)
    }
}

pub async fn provider_for_message(
    pool: &SqlitePool,
    message_id: &str,
) -> Result<(String, u32, Arc<dyn MailProvider>)> {
    let row: Option<(String, String, i64)> = sqlx::query_as(
        r#"SELECT m.account_id, f.full_path, m.uid
           FROM messages m JOIN folders f ON f.id = m.folder_id
           WHERE m.id = ?"#,
    )
    .bind(message_id)
    .fetch_optional(pool)
    .await?;
    let (account_id, folder, uid) =
        row.ok_or_else(|| AppError::NotFound(format!("message {message_id} not found")))?;
    let provider = provider_for_account(pool, &account_id).await?;
    Ok((folder, uid as u32, provider))
}
