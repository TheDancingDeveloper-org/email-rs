#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Extension, Json,
};
use email_types::AuthorizeResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::app::AuthPrincipal;
use crate::auth::oauth2::{OAuthConfig, StoredToken};
use crate::error::{AppError, Result};
use crate::state::{AppState, OAuthState};
use crate::sync::SyncOrchestrator;

// ── Query parameter structs ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    /// If provided, update this account's token instead of creating a new one.
    pub account_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeQuery {
    pub account_id: Option<String>,
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Build `scheme://host` from the request. Trusts `X-Forwarded-Proto` when
/// present (reverse proxy). Falls back to `http` since the desktop/dev server
/// isn't TLS-terminated.
fn request_origin(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(axum::http::header::HOST)?.to_str().ok()?;
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    Some(format!("{scheme}://{host}"))
}

fn not_configured(provider: &str, vars: &str) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": format!("{provider} OAuth2 not configured — set {vars}")
        })),
    )
        .into_response()
}

fn spawn_sync(
    pool: sqlx::SqlitePool,
    event_tx: tokio::sync::broadcast::Sender<String>,
    account_id: String,
) {
    tokio::spawn(async move {
        let orchestrator = SyncOrchestrator::new(pool, event_tx);
        if let Err(e) = orchestrator.sync_account(&account_id).await {
            tracing::warn!("post-auth sync failed for account {}: {}", account_id, e);
        }
    });
}

/// Validate and consume the OAuth state token. Returns the stored PKCE verifier
/// (None for non-PKCE flows) or an error response.
#[allow(clippy::result_large_err)]
async fn consume_state(
    app_state: &Arc<AppState>,
    state_val: &str,
) -> std::result::Result<OAuthState, Response> {
    let mut map = app_state.oauth_states.lock().await;
    match map.remove(state_val) {
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid or expired state" })),
        )
            .into_response()),
        Some(verifier) => Ok(verifier),
    }
}

/// The principal that started an OAuth flow must still be valid at callback time:
/// an unexpired application session, or an unrevoked, unexpired integration token
/// holding `mail.accounts.connect` (its id is recorded as the session id).
pub(crate) async fn oauth_owner_is_active(app_state: &AppState, pending: &OAuthState) -> bool {
    let now = chrono::Utc::now().timestamp();
    let session = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM app_sessions WHERE id = ? AND user_id = ? AND expires_at > ?",
    )
    .bind(&pending.session_id)
    .bind(&pending.user_id)
    .bind(now)
    .fetch_optional(&app_state.pool)
    .await
    .ok()
    .flatten()
    .is_some();
    if session {
        return true;
    }
    sqlx::query_scalar::<_, String>(
        r#"SELECT scopes_json FROM integration_tokens
           WHERE id = ? AND user_id = ? AND revoked_at IS NULL
             AND (expires_at IS NULL OR expires_at > ?)"#,
    )
    .bind(&pending.session_id)
    .bind(&pending.user_id)
    .bind(now)
    .fetch_optional(&app_state.pool)
    .await
    .ok()
    .flatten()
    .and_then(|scopes| serde_json::from_str::<Vec<String>>(&scopes).ok())
    .is_some_and(|scopes| scopes.iter().any(|scope| scope == "mail.accounts.connect"))
}

#[allow(clippy::result_large_err)]
fn extract_code_and_state(q: &CallbackQuery) -> std::result::Result<(&str, &str), Response> {
    let code = match q.code.as_deref() {
        Some(c) => c,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing code parameter" })),
            )
                .into_response())
        }
    };
    let state = match q.state.as_deref() {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing state parameter" })),
            )
                .into_response())
        }
    };
    Ok((code, state))
}

// ── Userinfo fetchers ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OidcUserInfo {
    pub email: Option<String>,
    pub name: Option<String>,
    pub email_verified: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftUserInfo {
    /// Admin-settable and not verified. Display only — never the identity.
    pub mail: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    /// Immutable directory object id.
    pub id: Option<String>,
}

/// Who signed in. Microsoft identity is the immutable tenant + object id;
/// `email` is the mailbox shown to the user, never the binding key.
struct VerifiedIdentity {
    email: String,
    name: String,
    provider_tenant_id: Option<String>,
    provider_object_id: Option<String>,
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

fn google_email_verified(value: &Option<serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(serde_json::Value::String(text)) => text.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

async fn fetch_google_userinfo(access_token: &str) -> Result<VerifiedIdentity> {
    let info = reqwest::Client::new()
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Auth(format!("userinfo request failed: {e}")))?
        .json::<OidcUserInfo>()
        .await
        .map_err(|e| AppError::Auth(format!("userinfo parse failed: {e}")))?;
    if !google_email_verified(&info.email_verified) {
        return Err(AppError::Auth(
            "Google account email is not verified".into(),
        ));
    }
    let Some(email) = nonempty(info.email) else {
        return Err(AppError::Auth("OAuth identity is empty".into()));
    };
    let name = nonempty(info.name).unwrap_or_else(|| email.clone());
    Ok(VerifiedIdentity {
        email,
        name,
        provider_tenant_id: None,
        provider_object_id: None,
    })
}

async fn fetch_microsoft_userinfo(
    access_token: &str,
    id_token: Option<&str>,
) -> Result<VerifiedIdentity> {
    let info = reqwest::Client::new()
        .get("https://graph.microsoft.com/v1.0/me?$select=id,userPrincipalName,displayName")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Auth(format!("Microsoft userinfo request failed: {e}")))?
        .json::<MicrosoftUserInfo>()
        .await
        .map_err(|e| AppError::Auth(format!("Microsoft userinfo parse failed: {e}")))?;
    // Graph `/me` has no tenant id, and `mail` is admin-settable. The binding
    // is the ID token's tid + oid, checked against Graph's object id.
    let Some((tenant_id, object_id)) = id_token.and_then(crate::auth::oauth2::microsoft_tid_oid)
    else {
        return Err(AppError::Auth(
            "Microsoft identity is missing the tenant and object id".into(),
        ));
    };
    // Graph `mail` is admin-settable and unverified. The mailbox we store is
    // the user principal name; the binding itself is tid + oid.
    let Some(email) = nonempty(info.user_principal_name) else {
        return Err(AppError::Auth("OAuth identity is empty".into()));
    };
    let name = nonempty(info.display_name).unwrap_or_else(|| email.clone());
    object_id_matches(&object_id, info.id.as_deref())?;
    Ok(VerifiedIdentity {
        email,
        name,
        provider_tenant_id: Some(tenant_id),
        provider_object_id: Some(object_id),
    })
}

/// Graph `id` and the ID token `oid` name the same directory object.
fn object_id_matches(oid: &str, graph_id: Option<&str>) -> Result<()> {
    if let Some(graph_id) = graph_id.filter(|id| !id.is_empty()) {
        if !graph_id.eq_ignore_ascii_case(oid) {
            return Err(AppError::Auth(
                "Microsoft identity does not match the ID token".into(),
            ));
        }
    }
    Ok(())
}

// ── DB helpers ────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn upsert_account(
    pool: &sqlx::SqlitePool,
    email: &str,
    name: &str,
    provider_type: &str,
    token_json: &str,
    token_expiry: Option<String>,
    provider_tenant_id: Option<&str>,
    provider_object_id: Option<&str>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let token_json = crate::credentials::seal(token_json)?;
    // Same mailbox, different directory object: do not hand the stored token
    // over. ON CONFLICT would otherwise replace it.
    if let Some(object_id) = provider_object_id {
        let stored: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT provider_tenant_id, provider_object_id FROM accounts WHERE email = ? AND provider_type = ?",
        )
        .bind(email)
        .bind(provider_type)
        .fetch_optional(pool)
        .await?;
        if let Some((stored_tid, stored_oid)) = stored {
            if let Some(stored_oid) = stored_oid.as_deref().filter(|id| !id.is_empty()) {
                let same_oid = stored_oid.eq_ignore_ascii_case(object_id);
                let same_tid = match (stored_tid.as_deref(), provider_tenant_id) {
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    // A legacy row has no tenant yet; the object id is enough
                    // to recognise it, and this connect fills the tenant in.
                    (None, _) => true,
                    _ => false,
                };
                if !same_oid || !same_tid {
                    return Err(AppError::Conflict(
                        "OAuth identity does not match this account".into(),
                    ));
                }
            }
        }
    }
    sqlx::query(
        r#"INSERT INTO accounts (id, name, email, provider_type, auth_type, oauth_token_json, token_expiry, sync_days_limit, provider_tenant_id, provider_object_id)
           VALUES (?, ?, ?, ?, 'oauth2', ?, ?, 30, ?, ?)
           ON CONFLICT(email, provider_type) DO UPDATE SET
               name = excluded.name,
               oauth_token_json = excluded.oauth_token_json,
               token_expiry = excluded.token_expiry,
               provider_tenant_id = COALESCE(excluded.provider_tenant_id, accounts.provider_tenant_id),
               provider_object_id = COALESCE(excluded.provider_object_id, accounts.provider_object_id)"#,
    )
    .bind(&id)
    .bind(name)
    .bind(email)
    .bind(provider_type)
    .bind(token_json)
    .bind(token_expiry)
    .bind(provider_tenant_id)
    .bind(provider_object_id)
    .execute(pool)
    .await?;

    let actual_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE email = ? AND provider_type = ?")
            .bind(email)
            .bind(provider_type)
            .fetch_optional(pool)
            .await?;

    Ok(actual_id.unwrap_or(id))
}

// ── Gmail ─────────────────────────────────────────────────────────────────────

fn gmail_redirect_uri(headers: &HeaderMap) -> String {
    std::env::var("GOOGLE_REDIRECT_URI").unwrap_or_else(|_| {
        format!(
            "{}/api/v1/auth/gmail/callback",
            request_origin(headers).unwrap_or_else(|| "http://localhost:3000".into())
        )
    })
}

/// GET /api/v1/auth/gmail/authorize
pub async fn gmail_authorize(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
    Query(query): Query<AuthorizeQuery>,
    headers: HeaderMap,
) -> Response {
    let client_id = match crate::auth::google_client_id() {
        Some(id) => id,
        None => return not_configured("Google", "GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"),
    };
    let client_secret = match crate::auth::google_client_secret() {
        Some(s) => s,
        None => return not_configured("Google", "GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"),
    };

    let oauth = OAuthConfig::gmail(client_id, client_secret, gmail_redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();
    let (url, verifier) = oauth.authorization_url(&state_val);

    state.oauth_states.lock().await.insert(
        state_val,
        OAuthState {
            user_id: principal.user_id,
            session_id: principal.session_id,
            verifier,
            account_id: query.account_id,
        },
    );
    Json(AuthorizeResponse { url }).into_response()
}

/// GET /api/v1/auth/gmail/callback
pub async fn gmail_callback(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if let Some(err) = q.error.as_deref() {
        return Redirect::to(&format!("/?oauth=error&msg={}", urlencoding::encode(err)))
            .into_response();
    }

    let (code, state_val) = match extract_code_and_state(&q) {
        Ok(v) => v,
        Err(r) => return r,
    };

    let pending = match consume_state(&app_state, state_val).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if !oauth_owner_is_active(&app_state, &pending).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "OAuth session expired or was revoked" })),
        )
            .into_response();
    }

    let client_id = match crate::auth::google_client_id() {
        Some(id) => id,
        None => return not_configured("Google", "GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"),
    };
    let client_secret = match crate::auth::google_client_secret() {
        Some(s) => s,
        None => return not_configured("Google", "GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"),
    };

    let oauth = OAuthConfig::gmail(client_id, client_secret, gmail_redirect_uri(&headers));
    let token_resp = match oauth.exchange_code(code, pending.verifier.as_deref()).await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let stored = StoredToken::from_token_response(token_resp);
    let token_json = serde_json::to_string(&stored).unwrap();
    let token_expiry = stored.expires_at.map(|t| t.to_string());

    if q.account_id.is_some() && q.account_id != pending.account_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "OAuth account does not match initiating session" })),
        )
            .into_response();
    }

    let identity = match fetch_google_userinfo(&stored.access_token).await {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    finish_oauth(
        &app_state,
        pending.account_id.as_deref(),
        &identity,
        "gmail",
        &token_json,
        token_expiry,
    )
    .await
}

// ── Microsoft / Outlook ───────────────────────────────────────────────────────

fn outlook_redirect_uri(headers: &HeaderMap) -> String {
    std::env::var("MICROSOFT_REDIRECT_URI").unwrap_or_else(|_| {
        format!(
            "{}/api/v1/auth/microsoft/callback",
            request_origin(headers).unwrap_or_else(|| "http://localhost:3000".into())
        )
    })
}

/// GET /api/v1/auth/microsoft/authorize
pub async fn microsoft_authorize(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
    Query(query): Query<AuthorizeQuery>,
    headers: HeaderMap,
) -> Response {
    let client_id = match crate::auth::microsoft_client_id() {
        Some(id) => id,
        None => return not_configured("Microsoft", "MICROSOFT_CLIENT_ID"),
    };

    let oauth = OAuthConfig::outlook(client_id, outlook_redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();
    let (url, verifier) = oauth.authorization_url(&state_val);

    state.oauth_states.lock().await.insert(
        state_val,
        OAuthState {
            user_id: principal.user_id,
            session_id: principal.session_id,
            verifier,
            account_id: query.account_id,
        },
    );
    Json(AuthorizeResponse { url }).into_response()
}

/// GET /api/v1/auth/microsoft/callback
pub async fn microsoft_callback(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if let Some(err) = q.error.as_deref() {
        return Redirect::to(&format!("/?oauth=error&msg={}", urlencoding::encode(err)))
            .into_response();
    }

    let (code, state_val) = match extract_code_and_state(&q) {
        Ok(v) => v,
        Err(r) => return r,
    };

    let pending = match consume_state(&app_state, state_val).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if !oauth_owner_is_active(&app_state, &pending).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "OAuth session expired or was revoked" })),
        )
            .into_response();
    }

    let client_id = match crate::auth::microsoft_client_id() {
        Some(id) => id,
        None => return not_configured("Microsoft", "MICROSOFT_CLIENT_ID"),
    };

    let oauth = OAuthConfig::outlook(client_id, outlook_redirect_uri(&headers));
    let token_resp = match oauth.exchange_code(code, pending.verifier.as_deref()).await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let stored = StoredToken::from_token_response(token_resp);
    let token_json = serde_json::to_string(&stored).unwrap();
    let token_expiry = stored.expires_at.map(|t| t.to_string());

    if q.account_id.is_some() && q.account_id != pending.account_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "OAuth account does not match initiating session" })),
        )
            .into_response();
    }

    let identity =
        match fetch_microsoft_userinfo(&stored.access_token, stored.id_token.as_deref()).await {
            Ok(u) => u,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response()
            }
        };

    finish_oauth(
        &app_state,
        pending.account_id.as_deref(),
        &identity,
        "microsoft365",
        &token_json,
        token_expiry,
    )
    .await
}

/// Bind a completed OAuth grant. A reconnect (`account_id` set) may only
/// refresh the token of the account it named: the verified mailbox and the
/// provider must already be that account's. A different identity is refused
/// and the stored token is left untouched, so a `mail.accounts.connect`
/// holder cannot silently re-point an account at another mailbox or provider.
/// A first connect (no account id) still upserts by verified email and provider.
async fn finish_oauth(
    app_state: &Arc<AppState>,
    account_id: Option<&str>,
    identity: &VerifiedIdentity,
    provider_type: &str,
    token_json: &str,
    token_expiry: Option<String>,
) -> Response {
    if identity.email.trim().is_empty() {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": "OAuth identity is empty" })),
        )
            .into_response();
    }
    if let Some(account_id) = account_id {
        let existing: Option<(String, String, Option<String>, Option<String>)> = match sqlx::query_as(
            "SELECT email, provider_type, provider_tenant_id, provider_object_id FROM accounts WHERE id = ?",
        )
        .bind(account_id)
        .fetch_optional(&app_state.pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response()
            }
        };
        let Some((account_email, account_provider, stored_tid, stored_oid)) = existing else {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "account not found" })),
            )
                .into_response();
        };
        let email_ok = account_email.eq_ignore_ascii_case(&identity.email);
        // A legacy row has no tid+oid yet. The verified mailbox still has to
        // match, and this reconnect records the directory identity.
        let subject_ok = match (
            stored_oid.as_deref().filter(|id| !id.is_empty()),
            identity.provider_object_id.as_deref(),
        ) {
            (None, _) => true,
            (Some(stored), Some(current)) => {
                let oid_ok = stored.eq_ignore_ascii_case(current);
                let tid_ok = match (
                    stored_tid.as_deref(),
                    identity.provider_tenant_id.as_deref(),
                ) {
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    (None, _) => true,
                    _ => false,
                };
                oid_ok && tid_ok
            }
            (Some(_), None) => false,
        };
        if !email_ok || account_provider != provider_type || !subject_ok {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "OAuth identity does not match this account"
                })),
            )
                .into_response();
        }
        return update_account_token(
            app_state,
            account_id,
            token_json,
            token_expiry,
            identity.provider_tenant_id.as_deref(),
            identity.provider_object_id.as_deref(),
        )
        .await;
    }

    match upsert_account(
        &app_state.pool,
        &identity.email,
        &identity.name,
        provider_type,
        token_json,
        token_expiry,
        identity.provider_tenant_id.as_deref(),
        identity.provider_object_id.as_deref(),
    )
    .await
    {
        Ok(aid) => {
            spawn_sync(app_state.pool.clone(), app_state.event_tx.clone(), aid);
            Redirect::to("/?oauth=success").into_response()
        }
        Err(AppError::Conflict(message)) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": message })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ── Token-refresh helper (used by re-auth flows) ──────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn update_account_token(
    app_state: &Arc<AppState>,
    account_id: &str,
    token_json: &str,
    token_expiry: Option<String>,
    provider_tenant_id: Option<&str>,
    provider_object_id: Option<&str>,
) -> Response {
    let token_json = match crate::credentials::seal(token_json) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    // Fill a legacy row's directory identity; never replace one that is set.
    let result = sqlx::query(
        r#"UPDATE accounts SET
               oauth_token_json = ?,
               token_expiry = ?,
               provider_tenant_id = COALESCE(provider_tenant_id, ?),
               provider_object_id = COALESCE(provider_object_id, ?)
           WHERE id = ?"#,
    )
    .bind(token_json)
    .bind(&token_expiry)
    .bind(provider_tenant_id)
    .bind(provider_object_id)
    .bind(account_id)
    .execute(&app_state.pool)
    .await;

    match result {
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
        Ok(r) if r.rows_affected() == 0 => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "account not found" })),
        )
            .into_response(),
        Ok(_) => {
            spawn_sync(
                app_state.pool.clone(),
                app_state.event_tx.clone(),
                account_id.to_string(),
            );
            Redirect::to("/?oauth=success").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{finish_oauth, upsert_account, VerifiedIdentity};

    fn identity(email: &str, name: &str) -> VerifiedIdentity {
        VerifiedIdentity {
            email: email.to_string(),
            name: name.to_string(),
            provider_tenant_id: None,
            provider_object_id: None,
        }
    }

    fn microsoft(email: &str, tid: &str, oid: &str) -> VerifiedIdentity {
        VerifiedIdentity {
            email: email.to_string(),
            name: email.to_string(),
            provider_tenant_id: Some(tid.to_string()),
            provider_object_id: Some(oid.to_string()),
        }
    }
    use axum::body::to_bytes;
    use axum::http::StatusCode;

    async fn pool() -> sqlx::SqlitePool {
        let path = format!("/tmp/email_oauth_identity_{}.db", uuid::Uuid::new_v4());
        let (pool, _) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        pool
    }

    async fn body_error(response: axum::response::Response) -> (StatusCode, String) {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (status, value["error"].as_str().unwrap().to_string())
    }

    /// Reconnecting an existing account with a token whose verified identity is
    /// a different mailbox (or a different provider) must be rejected, and the
    /// account's stored token must be left as it was.
    #[tokio::test]
    async fn reconnect_rejects_a_different_identity_and_keeps_the_token() {
        let pool = pool().await;
        let state = std::sync::Arc::new(crate::state::AppState::new(pool.clone(), false));
        let account_id = upsert_account(
            &pool,
            "owner@example.com",
            "Owner",
            "gmail",
            r#"{"access_token":"original"}"#,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let response = finish_oauth(
            &state,
            Some(&account_id),
            &identity("attacker@example.com", "Attacker"),
            "gmail",
            r#"{"access_token":"stolen"}"#,
            None,
        )
        .await;
        let (status, error) = body_error(response).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(error, "OAuth identity does not match this account");

        // Same mailbox, different provider: still a re-point, still rejected.
        let response = finish_oauth(
            &state,
            Some(&account_id),
            &identity("owner@example.com", "Owner"),
            "microsoft365",
            r#"{"access_token":"other-provider"}"#,
            None,
        )
        .await;
        let (status, _) = body_error(response).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let stored: String =
            sqlx::query_scalar("SELECT oauth_token_json FROM accounts WHERE id = ?")
                .bind(&account_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let opened = crate::credentials::open(&stored).unwrap();
        assert!(opened.contains("original"), "{opened}");
        assert!(!opened.contains("stolen"));
        assert!(!opened.contains("other-provider"));
    }

    #[tokio::test]
    async fn reconnect_accepts_the_same_identity() {
        let pool = pool().await;
        let state = std::sync::Arc::new(crate::state::AppState::new(pool.clone(), false));
        let account_id = upsert_account(
            &pool,
            "owner@example.com",
            "Owner",
            "gmail",
            r#"{"access_token":"original"}"#,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let response = finish_oauth(
            &state,
            Some(&account_id),
            &identity("Owner@Example.com", "Owner"),
            "gmail",
            r#"{"access_token":"refreshed"}"#,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let stored: String =
            sqlx::query_scalar("SELECT oauth_token_json FROM accounts WHERE id = ?")
                .bind(&account_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let opened = crate::credentials::open(&stored).unwrap();
        assert!(opened.contains("refreshed"), "{opened}");
    }

    #[tokio::test]
    async fn reconnect_of_an_unknown_account_is_not_found() {
        let pool = pool().await;
        let state = std::sync::Arc::new(crate::state::AppState::new(pool, false));
        let response = finish_oauth(
            &state,
            Some("does-not-exist"),
            &identity("owner@example.com", "Owner"),
            "gmail",
            r#"{"access_token":"x"}"#,
            None,
        )
        .await;
        let (status, error) = body_error(response).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error, "account not found");
    }

    /// Same mailbox, different Microsoft directory: the first account's token
    /// stays put. `mail` is not an identity.
    #[tokio::test]
    async fn microsoft_reconnect_rejects_a_different_directory_identity() {
        let pool = pool().await;
        let state = std::sync::Arc::new(crate::state::AppState::new(pool.clone(), false));
        let first = finish_oauth(
            &state,
            None,
            &microsoft("user@contoso.com", "tenant-a", "object-a"),
            "microsoft365",
            r#"{"access_token":"original"}"#,
            None,
        )
        .await;
        assert_eq!(first.status(), StatusCode::SEE_OTHER);

        let spoofed = finish_oauth(
            &state,
            None,
            &microsoft("user@contoso.com", "tenant-b", "object-b"),
            "microsoft365",
            r#"{"access_token":"spoofed"}"#,
            None,
        )
        .await;
        let (status, error) = body_error(spoofed).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(error, "OAuth identity does not match this account");

        let stored: String = sqlx::query_scalar(
            "SELECT oauth_token_json FROM accounts WHERE email = ? AND provider_type = ?",
        )
        .bind("user@contoso.com")
        .bind("microsoft365")
        .fetch_one(&pool)
        .await
        .unwrap();
        let opened = crate::credentials::open(&stored).unwrap();
        assert!(opened.contains("original"), "{opened}");
        assert!(!opened.contains("spoofed"));
    }

    /// A row from before directory identity existed is filled in on the next
    /// successful connect, and that binding is what a later reconnect checks.
    #[tokio::test]
    async fn legacy_row_records_directory_identity_on_reconnect() {
        let pool = pool().await;
        let state = std::sync::Arc::new(crate::state::AppState::new(pool.clone(), false));
        let account_id = upsert_account(
            &pool,
            "user@contoso.com",
            "User",
            "microsoft365",
            r#"{"access_token":"legacy"}"#,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let response = finish_oauth(
            &state,
            Some(&account_id),
            &microsoft("user@contoso.com", "tenant-a", "object-a"),
            "microsoft365",
            r#"{"access_token":"upgraded"}"#,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let row: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT provider_tenant_id, provider_object_id FROM accounts WHERE id = ?",
        )
        .bind(&account_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0.as_deref(), Some("tenant-a"));
        assert_eq!(row.1.as_deref(), Some("object-a"));

        let spoofed = finish_oauth(
            &state,
            Some(&account_id),
            &microsoft("user@contoso.com", "tenant-b", "object-b"),
            "microsoft365",
            r#"{"access_token":"spoofed"}"#,
            None,
        )
        .await;
        assert_eq!(body_error(spoofed).await.0, StatusCode::CONFLICT);
    }

    #[test]
    fn google_requires_a_verified_email() {
        assert!(super::google_email_verified(&Some(
            serde_json::Value::Bool(true)
        )));
        assert!(!super::google_email_verified(&Some(
            serde_json::Value::Bool(false)
        )));
        assert!(!super::google_email_verified(&Some(serde_json::json!(
            "false"
        ))));
        assert!(super::google_email_verified(&Some(serde_json::json!(
            "true"
        ))));
        assert!(!super::google_email_verified(&None));
    }

    #[test]
    fn empty_identity_is_not_an_address() {
        assert!(super::nonempty(None).is_none());
        assert!(super::nonempty(Some("  ".into())).is_none());
        assert_eq!(
            super::nonempty(Some("a@b.c".into())).as_deref(),
            Some("a@b.c")
        );
    }

    #[test]
    fn microsoft_subject_comes_from_the_id_token() {
        use base64::Engine as _;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            br#"{"tid":"tenant-a","oid":"object-a","preferred_username":"user@contoso.com"}"#,
        );
        let token = format!("header.{payload}.sig");
        assert_eq!(
            crate::auth::oauth2::microsoft_tid_oid(&token)
                .as_ref()
                .map(|(t, o)| (t.as_str(), o.as_str())),
            Some(("tenant-a", "object-a"))
        );
        assert!(crate::auth::oauth2::microsoft_tid_oid("not-a-token").is_none());
    }
}
