use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;
use uuid::Uuid;

const INTERNAL_ERROR_MESSAGE: &str = "An internal error occurred";

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

    #[error("rate limited ({retry_after_secs}s): {message}")]
    RateLimited {
        retry_after_secs: u64,
        message: String,
    },

    #[error("payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("IMAP error: {0}")]
    Imap(String),

    /// The provider could not be reached or the connection dropped; retrying
    /// later may succeed. Mutations that hit this are queued for replay.
    #[error("provider unreachable: {0}")]
    Transient(String),

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    /// True when the failure is a connectivity problem, not a rejection.
    pub fn is_transient(&self) -> bool {
        matches!(self, AppError::Transient(_))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message, correlation_id) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg, None),
            AppError::Auth(msg) => (StatusCode::UNAUTHORIZED, msg, None),
            AppError::Database(error) => internal_error_response("database", &error),
            AppError::Provider(msg) => (StatusCode::BAD_GATEWAY, msg, None),
            AppError::RateLimited { message, .. } => (StatusCode::TOO_MANY_REQUESTS, message, None),
            AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg, None),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, msg, None),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg, None),
            AppError::Unsupported(msg) => (StatusCode::NOT_IMPLEMENTED, msg, None),
            AppError::Imap(msg) => (StatusCode::BAD_GATEWAY, msg, None),
            AppError::Smtp(msg) => (StatusCode::BAD_GATEWAY, msg, None),
            AppError::Transient(msg) => (StatusCode::BAD_GATEWAY, msg, None),
            AppError::Internal(error) => internal_error_response("internal", &error),
        };

        let body = match correlation_id {
            Some(correlation_id) => json!({
                "error": message,
                "correlation_id": correlation_id,
            }),
            None => json!({ "error": message }),
        };

        (status, Json(body)).into_response()
    }
}

fn internal_error_response(
    error_kind: &'static str,
    error: &dyn std::fmt::Debug,
) -> (StatusCode, String, Option<String>) {
    let correlation_id = Uuid::new_v4().to_string();
    tracing::error!(
        correlation_id = %correlation_id,
        error_kind,
        error = ?error,
        "request failed"
    );
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        INTERNAL_ERROR_MESSAGE.to_owned(),
        Some(correlation_id),
    )
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    async fn response_json(err: AppError) -> serde_json::Value {
        let response = err.into_response();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
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
    fn validation_is_400() {
        assert_eq!(
            status_of(AppError::Validation("x".into())),
            StatusCode::BAD_REQUEST
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
        let json = response_json(AppError::NotFound("thing not found".into())).await;
        assert_eq!(json["error"], "thing not found");
    }

    #[tokio::test]
    async fn auth_error_body_contains_message() {
        let json = response_json(AppError::Auth("invalid token".into())).await;
        assert_eq!(json["error"], "invalid token");
    }

    #[tokio::test]
    async fn database_error_body_hides_sql_and_has_correlation_id() {
        let sensitive = "SELECT password FROM users";
        let json = response_json(AppError::Database(sqlx::Error::Protocol(
            sensitive.to_owned(),
        )))
        .await;

        assert_eq!(json["error"], INTERNAL_ERROR_MESSAGE);
        assert!(!json.to_string().contains(sensitive));
        assert!(Uuid::parse_str(json["correlation_id"].as_str().unwrap()).is_ok());
    }

    #[tokio::test]
    async fn internal_error_body_hides_detail_and_has_correlation_id() {
        let sensitive = "credential decryption failed for account 42";
        let json = response_json(AppError::Internal(anyhow::anyhow!(sensitive))).await;

        assert_eq!(json["error"], INTERNAL_ERROR_MESSAGE);
        assert!(!json.to_string().contains(sensitive));
        assert!(Uuid::parse_str(json["correlation_id"].as_str().unwrap()).is_ok());
    }

    #[tokio::test]
    async fn validation_error_body_keeps_user_facing_message() {
        let message = "email address is required";
        let json = response_json(AppError::Validation(message.into())).await;

        assert_eq!(json["error"], message);
        assert!(json.get("correlation_id").is_none());
    }
}
