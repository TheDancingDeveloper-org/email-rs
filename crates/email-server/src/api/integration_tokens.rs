use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::app::{token_hash, AuthPrincipal, PrincipalKind},
    error::AppError,
    state::AppState,
};

pub const ALLOWED_SCOPES: &[&str] = &[
    "mail.metadata.read",
    "mail.body.read",
    "mail.modify",
    "mail.send",
    "mail.accounts.read",
    "mail.sync",
];

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateIntegrationToken {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationToken {
    pub id: String,
    pub name: String,
    pub token_prefix: String,
    #[sqlx(json)]
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedIntegrationToken {
    #[serde(flatten)]
    pub metadata: IntegrationToken,
    /// Returned exactly once. Only its SHA-256 hash is persisted.
    pub token: String,
}

pub async fn list_tokens(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
) -> Response {
    if principal.kind == PrincipalKind::Integration {
        return forbidden();
    }
    match sqlx::query_as::<_, IntegrationToken>(
        r#"SELECT id, name, token_prefix, scopes_json AS scopes, expires_at,
                  created_at, last_used_at, revoked_at
           FROM integration_tokens WHERE user_id = ? ORDER BY created_at DESC"#,
    )
    .bind(&principal.user_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(tokens) => Json(tokens).into_response(),
        Err(error) => AppError::Database(error).into_response(),
    }
}

pub async fn create_token(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
    Json(mut body): Json<CreateIntegrationToken>,
) -> Response {
    if principal.kind == PrincipalKind::Integration {
        return forbidden();
    }
    body.name = body.name.trim().to_string();
    body.scopes.sort();
    body.scopes.dedup();
    if body.name.is_empty() || body.name.len() > 120 {
        return bad_request("name must contain 1 to 120 characters");
    }
    if body.scopes.is_empty()
        || body
            .scopes
            .iter()
            .any(|scope| !ALLOWED_SCOPES.contains(&scope.as_str()))
    {
        return bad_request("one or more integration-token scopes are invalid");
    }
    if body
        .expires_at
        .is_some_and(|expiry| expiry <= chrono::Utc::now().timestamp())
    {
        return bad_request("expiresAt must be in the future");
    }

    let id = Uuid::new_v4().to_string();
    let token = format!(
        "eirs_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let token_prefix = token.chars().take(13).collect::<String>();
    let scopes_json = match serde_json::to_string(&body.scopes) {
        Ok(value) => value,
        Err(error) => return AppError::Internal(error.into()).into_response(),
    };
    if let Err(error) = sqlx::query(
        r#"INSERT INTO integration_tokens
               (id, user_id, name, token_hash, token_prefix, scopes_json, expires_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&principal.user_id)
    .bind(&body.name)
    .bind(token_hash(&token))
    .bind(&token_prefix)
    .bind(scopes_json)
    .bind(body.expires_at)
    .execute(&state.pool)
    .await
    {
        return AppError::Database(error).into_response();
    }

    let metadata = IntegrationToken {
        id,
        name: body.name,
        token_prefix,
        scopes: body.scopes,
        expires_at: body.expires_at,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        last_used_at: None,
        revoked_at: None,
    };
    (
        StatusCode::CREATED,
        Json(CreatedIntegrationToken { metadata, token }),
    )
        .into_response()
}

pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
    Path(id): Path<String>,
) -> Response {
    if principal.kind == PrincipalKind::Integration {
        return forbidden();
    }
    match sqlx::query(
        "UPDATE integration_tokens SET revoked_at = datetime('now') WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(&principal.user_id)
    .execute(&state.pool)
    .await
    {
        Ok(result) if result.rows_affected() == 0 => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "integration token not found"}))).into_response()
        }
        Ok(_) => Json(serde_json::json!({"status": "revoked"})).into_response(),
        Err(error) => AppError::Database(error).into_response(),
    }
}

fn bad_request(message: &'static str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": message})),
    )
        .into_response()
}

fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": "integration tokens cannot manage credentials"})),
    )
        .into_response()
}
