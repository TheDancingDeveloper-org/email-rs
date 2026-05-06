#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::oauth2::{OAuthConfig, StoredToken};
use crate::error::{AppError, Result};
use crate::state::AppState;
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

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuthorizeResponse {
    pub url: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AccountRow {
    pub id: String,
    pub name: String,
    pub email: String,
    pub provider_type: String,
    pub auth_type: String,
    pub created_at: String,
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
) -> std::result::Result<Option<String>, Response> {
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

#[derive(Debug, Deserialize)]
struct FastmailSession {
    pub username: Option<String>,
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

async fn fetch_oidc_userinfo(access_token: &str, endpoint: &str) -> Result<(String, String)> {
    let info = reqwest::Client::new()
        .get(endpoint)
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

async fn fetch_fastmail_userinfo(access_token: &str) -> Result<(String, String)> {
    let session = reqwest::Client::new()
        .get("https://api.fastmail.com/jmap/session")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Auth(format!("Fastmail session request failed: {e}")))?
        .json::<FastmailSession>()
        .await
        .map_err(|e| AppError::Auth(format!("Fastmail session parse failed: {e}")))?;
    let email = session.username.unwrap_or_default();
    Ok((email.clone(), email))
}

// ── DB helpers ────────────────────────────────────────────────────────────────

async fn upsert_account(
    pool: &sqlx::SqlitePool,
    email: &str,
    name: &str,
    provider_type: &str,
    token_json: &str,
    token_expiry: Option<String>,
) -> std::result::Result<String, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
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
pub async fn gmail_authorize(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
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

    state.oauth_states.lock().await.insert(state_val, verifier);
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

    let verifier = match consume_state(&app_state, state_val).await {
        Ok(v) => v,
        Err(r) => return r,
    };

    let client_id = match crate::auth::google_client_id() {
        Some(id) => id,
        None => return not_configured("Google", "GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"),
    };
    let client_secret = match crate::auth::google_client_secret() {
        Some(s) => s,
        None => return not_configured("Google", "GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"),
    };

    let oauth = OAuthConfig::gmail(client_id, client_secret, gmail_redirect_uri(&headers));
    let token_resp = match oauth.exchange_code(code, verifier.as_deref()).await {
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

    if let Some(account_id) = q.account_id {
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
    headers: HeaderMap,
) -> Response {
    let client_id = match crate::auth::microsoft_client_id() {
        Some(id) => id,
        None => return not_configured("Microsoft", "MICROSOFT_CLIENT_ID"),
    };

    let oauth = OAuthConfig::outlook(client_id, outlook_redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();
    let (url, verifier) = oauth.authorization_url(&state_val);

    state.oauth_states.lock().await.insert(state_val, verifier);
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

    let verifier = match consume_state(&app_state, state_val).await {
        Ok(v) => v,
        Err(r) => return r,
    };

    let client_id = match crate::auth::microsoft_client_id() {
        Some(id) => id,
        None => return not_configured("Microsoft", "MICROSOFT_CLIENT_ID"),
    };

    let oauth = OAuthConfig::outlook(client_id, outlook_redirect_uri(&headers));
    let token_resp = match oauth.exchange_code(code, verifier.as_deref()).await {
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

    if let Some(account_id) = q.account_id {
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

// ── Yahoo ─────────────────────────────────────────────────────────────────────

fn yahoo_redirect_uri(headers: &HeaderMap) -> String {
    std::env::var("YAHOO_REDIRECT_URI").unwrap_or_else(|_| {
        format!(
            "{}/api/v1/auth/yahoo/callback",
            request_origin(headers).unwrap_or_else(|| "http://localhost:3000".into())
        )
    })
}

/// GET /api/v1/auth/yahoo/authorize
pub async fn yahoo_authorize(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let client_id = match crate::auth::yahoo_client_id() {
        Some(id) => id,
        None => return not_configured("Yahoo", "YAHOO_CLIENT_ID"),
    };

    let oauth = OAuthConfig::yahoo(client_id, yahoo_redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();
    let (url, verifier) = oauth.authorization_url(&state_val);

    state.oauth_states.lock().await.insert(state_val, verifier);
    Json(AuthorizeResponse { url }).into_response()
}

/// GET /api/v1/auth/yahoo/callback
pub async fn yahoo_callback(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    pkce_callback(
        &app_state,
        &headers,
        q,
        "yahoo",
        yahoo_redirect_uri,
        OAuthConfig::yahoo,
        |token: String| async move {
            fetch_oidc_userinfo(&token, "https://api.login.yahoo.com/openid/v1/userinfo").await
        },
    )
    .await
}

// ── AOL ───────────────────────────────────────────────────────────────────────

fn aol_redirect_uri(headers: &HeaderMap) -> String {
    std::env::var("AOL_REDIRECT_URI").unwrap_or_else(|_| {
        format!(
            "{}/api/v1/auth/aol/callback",
            request_origin(headers).unwrap_or_else(|| "http://localhost:3000".into())
        )
    })
}

/// GET /api/v1/auth/aol/authorize
pub async fn aol_authorize(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let client_id = match crate::auth::aol_client_id() {
        Some(id) => id,
        None => return not_configured("AOL", "AOL_CLIENT_ID"),
    };

    let oauth = OAuthConfig::aol(client_id, aol_redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();
    let (url, verifier) = oauth.authorization_url(&state_val);

    state.oauth_states.lock().await.insert(state_val, verifier);
    Json(AuthorizeResponse { url }).into_response()
}

/// GET /api/v1/auth/aol/callback
pub async fn aol_callback(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    pkce_callback(
        &app_state,
        &headers,
        q,
        "aol",
        aol_redirect_uri,
        OAuthConfig::aol,
        |token: String| async move {
            fetch_oidc_userinfo(&token, "https://api.login.aol.com/openid/v1/userinfo").await
        },
    )
    .await
}

// ── Fastmail ──────────────────────────────────────────────────────────────────

fn fastmail_redirect_uri(headers: &HeaderMap) -> String {
    std::env::var("FASTMAIL_REDIRECT_URI").unwrap_or_else(|_| {
        format!(
            "{}/api/v1/auth/fastmail/callback",
            request_origin(headers).unwrap_or_else(|| "http://localhost:3000".into())
        )
    })
}

/// GET /api/v1/auth/fastmail/authorize
pub async fn fastmail_authorize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let client_id = match crate::auth::fastmail_client_id() {
        Some(id) => id,
        None => return not_configured("Fastmail", "FASTMAIL_CLIENT_ID"),
    };

    let oauth = OAuthConfig::fastmail(client_id, fastmail_redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();
    let (url, verifier) = oauth.authorization_url(&state_val);

    state.oauth_states.lock().await.insert(state_val, verifier);
    Json(AuthorizeResponse { url }).into_response()
}

/// GET /api/v1/auth/fastmail/callback
pub async fn fastmail_callback(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    pkce_callback(
        &app_state,
        &headers,
        q,
        "fastmail",
        fastmail_redirect_uri,
        OAuthConfig::fastmail,
        |token: String| async move { fetch_fastmail_userinfo(&token).await },
    )
    .await
}

// ── Shared PKCE callback driver ───────────────────────────────────────────────

/// Shared callback handler for PKCE-only providers (no client secret).
/// `get_client_id` returns the client_id from env; `build_config` constructs
/// the OAuthConfig; `fetch_user` returns `(email, name)` from the access token.
async fn pkce_callback<F, B, Fut>(
    app_state: &Arc<AppState>,
    headers: &HeaderMap,
    q: CallbackQuery,
    provider_type: &str,
    build_redirect_uri: fn(&HeaderMap) -> String,
    build_config: F,
    fetch_user: B,
) -> Response
where
    F: Fn(String, String) -> OAuthConfig,
    B: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<(String, String)>>,
{
    if let Some(err) = q.error.as_deref() {
        return Redirect::to(&format!("/?oauth=error&msg={}", urlencoding::encode(err)))
            .into_response();
    }

    let (code, state_val) = match extract_code_and_state(&q) {
        Ok(v) => v,
        Err(r) => return r,
    };

    let verifier = match consume_state(app_state, state_val).await {
        Ok(v) => v,
        Err(r) => return r,
    };

    // Look up client_id from env (provider-specific var already resolved by caller).
    let client_id_var = match provider_type {
        "yahoo" => crate::auth::yahoo_client_id(),
        "aol" => crate::auth::aol_client_id(),
        "fastmail" => crate::auth::fastmail_client_id(),
        _ => None,
    };
    let client_id = match client_id_var {
        Some(id) => id,
        None => return (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({ "error": format!("{provider_type} OAuth2 not configured") })),
        )
            .into_response(),
    };

    let redirect_uri = build_redirect_uri(headers);
    let oauth = build_config(client_id, redirect_uri);

    let token_resp = match oauth.exchange_code(code, verifier.as_deref()).await {
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

    if let Some(account_id) = q.account_id {
        return update_account_token(app_state, &account_id, &token_json, token_expiry).await;
    }

    let (email, name) = match fetch_user(stored.access_token.clone()).await {
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
        provider_type,
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
