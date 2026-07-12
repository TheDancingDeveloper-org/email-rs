use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("IMAP error: {0}")]
    Imap(String),

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Auth(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Provider(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Imap(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::Smtp(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    #[test]
    fn not_found_is_404() {
        assert_eq!(
            status_of(AppError::NotFound("x".into())),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn auth_is_401() {
        assert_eq!(
            status_of(AppError::Auth("x".into())),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn provider_is_502() {
        assert_eq!(
            status_of(AppError::Provider("x".into())),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn payload_too_large_is_413() {
        assert_eq!(
            status_of(AppError::PayloadTooLarge("x".into())),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn imap_is_502() {
        assert_eq!(
            status_of(AppError::Imap("x".into())),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn smtp_is_502() {
        assert_eq!(
            status_of(AppError::Smtp("x".into())),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn internal_is_500() {
        assert_eq!(
            status_of(AppError::Internal(anyhow::anyhow!("boom"))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn response_body_has_error_key() {
        use http_body_util::BodyExt;
        let resp = AppError::NotFound("thing not found".into()).into_response();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"], "thing not found");
    }

    #[tokio::test]
    async fn auth_error_body_contains_message() {
        use http_body_util::BodyExt;
        let resp = AppError::Auth("invalid token".into()).into_response();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"], "invalid token");
    }
}
