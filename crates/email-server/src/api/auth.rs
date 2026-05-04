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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn get_oauth_config() -> Option<(String, String)> {
    let client_id = crate::auth::google_client_id()?;
    let client_secret = crate::auth::google_client_secret()?;
    Some((client_id, client_secret))
}

/// Resolve the OAuth callback URL. Env var wins if set (production deployments
/// behind a reverse proxy need to pin this). Otherwise derive from the request's
/// Host header so it works for both the docker dev stack (:3000) and the
/// desktop build (:8585) without extra configuration.
fn redirect_uri(headers: &HeaderMap) -> String {
    if let Ok(v) = std::env::var("GOOGLE_REDIRECT_URI") {
        return v;
    }
    format!(
        "{}/api/v1/auth/gmail/callback",
        request_origin(headers).unwrap_or_else(|| "http://localhost:3000".into())
    )
}

fn not_configured() -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Google OAuth2 not configured — set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"
        })),
    )
        .into_response()
}

// ── Microsoft helpers ─────────────────────────────────────────────────────────

fn get_microsoft_oauth_config() -> Option<(String, String)> {
    let client_id = crate::auth::microsoft_client_id()?;
    let client_secret = crate::auth::microsoft_client_secret()?;
    Some((client_id, client_secret))
}

fn microsoft_redirect_uri(headers: &HeaderMap) -> String {
    if let Ok(v) = std::env::var("MICROSOFT_REDIRECT_URI") {
        return v;
    }
    format!(
        "{}/api/v1/auth/microsoft/callback",
        request_origin(headers).unwrap_or_else(|| "http://localhost:3000".into())
    )
}

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

fn microsoft_not_configured() -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Microsoft OAuth2 not configured — set MICROSOFT_CLIENT_ID and MICROSOFT_CLIENT_SECRET"
        })),
    )
        .into_response()
}

// ── Google userinfo ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
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

async fn fetch_microsoft_userinfo(access_token: &str) -> Result<MicrosoftUserInfo> {
    let client = reqwest::Client::new();
    let info = client
        .get("https://graph.microsoft.com/v1.0/me")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Auth(format!("Microsoft userinfo request failed: {}", e)))?
        .json::<MicrosoftUserInfo>()
        .await
        .map_err(|e| AppError::Auth(format!("Microsoft userinfo parse failed: {}", e)))?;
    Ok(info)
}

async fn fetch_userinfo(access_token: &str) -> Result<GoogleUserInfo> {
    let client = reqwest::Client::new();
    let info = client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Auth(format!("userinfo request failed: {}", e)))?
        .json::<GoogleUserInfo>()
        .await
        .map_err(|e| AppError::Auth(format!("userinfo parse failed: {}", e)))?;
    Ok(info)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

// ── Route handlers ────────────────────────────────────────────────────────────

/// GET /api/v1/auth/gmail/authorize
/// Returns a JSON body with the Google authorization URL.
pub async fn gmail_authorize(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let (client_id, client_secret) = match get_oauth_config() {
        Some(c) => c,
        None => return not_configured(),
    };

    let oauth = OAuthConfig::gmail(client_id, client_secret, redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();

    // Store the pending state.
    {
        let mut map = state.oauth_states.lock().await;
        map.insert(state_val.clone(), ());
    }

    let url = oauth.authorization_url(&state_val);
    Json(AuthorizeResponse { url }).into_response()
}

/// GET /api/v1/auth/gmail/callback?code=...&state=...
pub async fn gmail_callback(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    // Propagate provider-reported errors as a redirect.
    if let Some(err) = q.error {
        let encoded = urlencoding::encode(&err);
        return Redirect::to(&format!("/?oauth=error&msg={}", encoded)).into_response();
    }

    let code = match q.code {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing code parameter" })),
            )
                .into_response()
        }
    };

    let state_val = match q.state {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing state parameter" })),
            )
                .into_response()
        }
    };

    // Validate and consume the state token.
    {
        let mut map = app_state.oauth_states.lock().await;
        if map.remove(&state_val).is_none() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid or expired state" })),
            )
                .into_response();
        }
    }

    let (client_id, client_secret) = match get_oauth_config() {
        Some(c) => c,
        None => return not_configured(),
    };

    let oauth = OAuthConfig::gmail(client_id, client_secret, redirect_uri(&headers));

    // Exchange authorization code for tokens.
    let token_resp = match oauth.exchange_code(&code).await {
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
    let token_json = match serde_json::to_string(&stored) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let token_expiry = stored.expires_at.map(|t| t.to_string());

    if let Some(account_id) = q.account_id {
        // Update existing account's token.
        let result =
            sqlx::query("UPDATE accounts SET oauth_token_json = ?, token_expiry = ? WHERE id = ?")
                .bind(&token_json)
                .bind(&token_expiry)
                .bind(&account_id)
                .execute(&app_state.pool)
                .await;

        match result {
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response()
            }
            Ok(r) if r.rows_affected() == 0 => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "account not found" })),
                )
                    .into_response()
            }
            _ => {}
        }

        let row = sqlx::query_as::<_, AccountRow>(
            "SELECT id, name, email, provider_type, auth_type, created_at FROM accounts WHERE id = ?",
        )
        .bind(&account_id)
        .fetch_one(&app_state.pool)
        .await;

        match row {
            Ok(_) => {
                spawn_sync(
                    app_state.pool.clone(),
                    app_state.event_tx.clone(),
                    account_id,
                );
                Redirect::to("/?oauth=success").into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    } else {
        // Fetch userinfo to get email + name for the new account.
        let userinfo = match fetch_userinfo(&stored.access_token).await {
            Ok(u) => u,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response()
            }
        };

        let email = userinfo.email.unwrap_or_default();
        let name = userinfo.name.unwrap_or_else(|| email.clone());
        let id = Uuid::new_v4().to_string();

        // Upsert: create a new account or update the existing one's tokens if
        // the same email+provider was already registered (e.g. after redeploy).
        let insert = sqlx::query(
            r#"INSERT INTO accounts (id, name, email, provider_type, auth_type, oauth_token_json, token_expiry, sync_days_limit)
               VALUES (?, ?, ?, 'gmail', 'oauth2', ?, ?, 30)
               ON CONFLICT(email, provider_type) DO UPDATE SET
                   name = excluded.name,
                   oauth_token_json = excluded.oauth_token_json,
                   token_expiry = excluded.token_expiry"#,
        )
        .bind(&id)
        .bind(&name)
        .bind(&email)
        .bind(&token_json)
        .bind(&token_expiry)
        .execute(&app_state.pool)
        .await;

        match insert {
            Ok(_) => {
                // Find the actual account ID (ON CONFLICT may have kept the original ID).
                let actual_id: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM accounts WHERE email = ? AND provider_type = 'gmail'",
                )
                .bind(&email)
                .fetch_optional(&app_state.pool)
                .await
                .ok()
                .flatten();

                if let Some(aid) = actual_id {
                    spawn_sync(app_state.pool.clone(), app_state.event_tx.clone(), aid);
                }
                Redirect::to("/?oauth=success").into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    }
}

/// GET /api/v1/auth/microsoft/authorize
/// Returns a JSON body with the Microsoft authorization URL.
pub async fn microsoft_authorize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let (client_id, client_secret) = match get_microsoft_oauth_config() {
        Some(c) => c,
        None => return microsoft_not_configured(),
    };

    let oauth = OAuthConfig::microsoft(client_id, client_secret, microsoft_redirect_uri(&headers));
    let state_val = Uuid::new_v4().to_string();

    {
        let mut map = state.oauth_states.lock().await;
        map.insert(state_val.clone(), ());
    }

    let url = oauth.authorization_url_microsoft(&state_val);
    Json(AuthorizeResponse { url }).into_response()
}

/// GET /api/v1/auth/microsoft/callback?code=...&state=...
pub async fn microsoft_callback(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if let Some(err) = q.error {
        let encoded = urlencoding::encode(&err);
        return Redirect::to(&format!("/?oauth=error&msg={}", encoded)).into_response();
    }

    let code = match q.code {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing code parameter" })),
            )
                .into_response()
        }
    };

    let state_val = match q.state {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing state parameter" })),
            )
                .into_response()
        }
    };

    {
        let mut map = app_state.oauth_states.lock().await;
        if map.remove(&state_val).is_none() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid or expired state" })),
            )
                .into_response();
        }
    }

    let (client_id, client_secret) = match get_microsoft_oauth_config() {
        Some(c) => c,
        None => return microsoft_not_configured(),
    };

    let oauth = OAuthConfig::microsoft(client_id, client_secret, microsoft_redirect_uri(&headers));

    let token_resp = match oauth.exchange_code(&code).await {
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
    let token_json = match serde_json::to_string(&stored) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let token_expiry = stored.expires_at.map(|t| t.to_string());

    if let Some(account_id) = q.account_id {
        let result =
            sqlx::query("UPDATE accounts SET oauth_token_json = ?, token_expiry = ? WHERE id = ?")
                .bind(&token_json)
                .bind(&token_expiry)
                .bind(&account_id)
                .execute(&app_state.pool)
                .await;

        match result {
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response()
            }
            Ok(r) if r.rows_affected() == 0 => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "account not found" })),
                )
                    .into_response()
            }
            _ => {}
        }

        spawn_sync(
            app_state.pool.clone(),
            app_state.event_tx.clone(),
            account_id,
        );
        return Redirect::to("/?oauth=success").into_response();
    }

    let userinfo = match fetch_microsoft_userinfo(&stored.access_token).await {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let email = userinfo
        .mail
        .or(userinfo.user_principal_name)
        .unwrap_or_default();
    let name = userinfo.display_name.unwrap_or_else(|| email.clone());
    let id = Uuid::new_v4().to_string();

    // Upsert: create a new account or update the existing one's tokens if
    // the same email+provider was already registered (e.g. after redeploy).
    let insert = sqlx::query(
        r#"INSERT INTO accounts (id, name, email, provider_type, auth_type, oauth_token_json, token_expiry, sync_days_limit)
           VALUES (?, ?, ?, 'microsoft365', 'oauth2', ?, ?, 30)
           ON CONFLICT(email, provider_type) DO UPDATE SET
               name = excluded.name,
               oauth_token_json = excluded.oauth_token_json,
               token_expiry = excluded.token_expiry"#,
    )
    .bind(&id)
    .bind(&name)
    .bind(&email)
    .bind(&token_json)
    .bind(&token_expiry)
    .execute(&app_state.pool)
    .await;

    match insert {
        Ok(_) => {
            let actual_id: Option<String> = sqlx::query_scalar(
                "SELECT id FROM accounts WHERE email = ? AND provider_type = 'microsoft365'",
            )
            .bind(&email)
            .fetch_optional(&app_state.pool)
            .await
            .ok()
            .flatten();

            if let Some(aid) = actual_id {
                spawn_sync(app_state.pool.clone(), app_state.event_tx.clone(), aid);
            }
            Redirect::to("/?oauth=success").into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
