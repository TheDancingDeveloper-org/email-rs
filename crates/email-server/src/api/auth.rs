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

pub(crate) async fn oauth_owner_is_active(app_state: &AppState, pending: &OAuthState) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM app_sessions WHERE id = ? AND user_id = ? AND expires_at > ?",
    )
    .bind(&pending.session_id)
    .bind(&pending.user_id)
    .bind(chrono::Utc::now().timestamp())
    .fetch_optional(&app_state.pool)
    .await
    .ok()
    .flatten()
    .is_some()
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
}

#[derive(Debug, Deserialize)]
struct MicrosoftUserInfo {
    pub mail: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

async fn fetch_google_userinfo(access_token: &str) -> Result<(String, String)> {
    let info = reqwest::Client::new()
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Auth(format!("userinfo request failed: {e}")))?
        .json::<OidcUserInfo>()
        .await
        .map_err(|e| AppError::Auth(format!("userinfo parse failed: {e}")))?;
    let email = info.email.unwrap_or_default();
    let name = info.name.unwrap_or_else(|| email.clone());
    Ok((email, name))
}

async fn fetch_microsoft_userinfo(access_token: &str) -> Result<(String, String)> {
    let info = reqwest::Client::new()
        .get("https://graph.microsoft.com/v1.0/me")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Auth(format!("Microsoft userinfo request failed: {e}")))?
        .json::<MicrosoftUserInfo>()
        .await
        .map_err(|e| AppError::Auth(format!("Microsoft userinfo parse failed: {e}")))?;
    let email = info.mail.or(info.user_principal_name).unwrap_or_default();
    let name = info.display_name.unwrap_or_else(|| email.clone());
    Ok((email, name))
}

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn upsert_account(
    pool: &sqlx::SqlitePool,
    email: &str,
    name: &str,
    provider_type: &str,
    token_json: &str,
    token_expiry: Option<String>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let token_json = crate::credentials::seal(token_json)?;
    sqlx::query(
        r#"INSERT INTO accounts (id, name, email, provider_type, auth_type, oauth_token_json, token_expiry, sync_days_limit)
           VALUES (?, ?, ?, ?, 'oauth2', ?, ?, 30)
           ON CONFLICT(email, provider_type) DO UPDATE SET
               name = excluded.name,
               oauth_token_json = excluded.oauth_token_json,
               token_expiry = excluded.token_expiry"#,
    )
    .bind(&id)
    .bind(name)
    .bind(email)
    .bind(provider_type)
    .bind(token_json)
    .bind(token_expiry)
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
    if let Some(account_id) = pending.account_id {
        return update_account_token(&app_state, &account_id, &token_json, token_expiry).await;
    }

    let (email, name) = match fetch_google_userinfo(&stored.access_token).await {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    match upsert_account(
        &app_state.pool,
        &email,
        &name,
        "gmail",
        &token_json,
        token_expiry,
    )
    .await
    {
        Ok(aid) => {
            spawn_sync(app_state.pool.clone(), app_state.event_tx.clone(), aid);
            Redirect::to("/?oauth=success").into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
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
    if let Some(account_id) = pending.account_id {
        return update_account_token(&app_state, &account_id, &token_json, token_expiry).await;
    }

    let (email, name) = match fetch_microsoft_userinfo(&stored.access_token).await {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    match upsert_account(
        &app_state.pool,
        &email,
        &name,
        "microsoft365",
        &token_json,
        token_expiry,
    )
    .await
    {
        Ok(aid) => {
            spawn_sync(app_state.pool.clone(), app_state.event_tx.clone(), aid);
            Redirect::to("/?oauth=success").into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ── Token-refresh helper (used by re-auth flows) ──────────────────────────────

async fn update_account_token(
    app_state: &Arc<AppState>,
    account_id: &str,
    token_json: &str,
    token_expiry: Option<String>,
) -> Response {
    let token_json = match crate::credentials::seal(token_json) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let result =
        sqlx::query("UPDATE accounts SET oauth_token_json = ?, token_expiry = ? WHERE id = ?")
            .bind(token_json)
            .bind(&token_expiry)
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
