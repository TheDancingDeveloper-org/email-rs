//! Per-account vacation auto-reply. The settings are applied by the mail
//! provider itself (Gmail `settings.vacation`) and stored here once it accepts
//! them. There is no local auto-responder, so providers without a server-side
//! responder refuse writes.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDate;
use email_types::{VacationSettings, VacationStatus};

use crate::{
    auth::token_source::{with_fresh_token, AccessTokenSource},
    error::{AppError, Result},
    providers::{factory, gmail_settings},
    state::AppState,
};

const MAX_SUBJECT_CHARS: usize = 255;
const MAX_BODY_CHARS: usize = 20_000;

#[derive(sqlx::FromRow)]
struct AccountKind {
    provider_type: String,
    auth_type: String,
}

fn gmail_supported(account: &AccountKind) -> bool {
    account.provider_type == "gmail" && account.auth_type == "oauth2"
}

async fn account_kind(state: &AppState, id: &str) -> Result<AccountKind> {
    sqlx::query_as("SELECT provider_type, auth_type FROM accounts WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("account {id} not found")))
}

fn parse_date(label: &str, value: &Option<String>) -> Result<Option<NaiveDate>> {
    value
        .as_deref()
        .map(|text| {
            NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|_| AppError::Validation(format!("{label} must be a YYYY-MM-DD date")))
        })
        .transpose()
}

fn validate(settings: &VacationSettings) -> Result<()> {
    if settings.subject.chars().count() > MAX_SUBJECT_CHARS {
        return Err(AppError::Validation(format!(
            "subject is limited to {MAX_SUBJECT_CHARS} characters"
        )));
    }
    // The subject becomes a mail header line, so a line break would let the
    // caller inject headers.
    if settings.subject.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "subject must not contain line breaks or other control characters".to_string(),
        ));
    }
    if settings.body.chars().count() > MAX_BODY_CHARS {
        return Err(AppError::Validation(format!(
            "body is limited to {MAX_BODY_CHARS} characters"
        )));
    }
    let start = parse_date("startDate", &settings.start_date)?;
    let end = parse_date("endDate", &settings.end_date)?;
    if let (Some(start), Some(end)) = (start, end) {
        if end < start {
            return Err(AppError::Validation(
                "endDate must not be before startDate".to_string(),
            ));
        }
    }
    if settings.enabled && settings.body.trim().is_empty() {
        return Err(AppError::Validation(
            "body is required to enable the auto-reply".to_string(),
        ));
    }
    Ok(())
}

async fn stored(state: &AppState, id: &str) -> Result<VacationSettings> {
    #[derive(sqlx::FromRow)]
    struct Row {
        enabled: bool,
        subject: String,
        body: String,
        start_date: Option<String>,
        end_date: Option<String>,
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT enabled, subject, body, start_date, end_date FROM account_vacation WHERE account_id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row
        .map(|row| VacationSettings {
            enabled: row.enabled,
            subject: row.subject,
            body: row.body,
            start_date: row.start_date,
            end_date: row.end_date,
        })
        .unwrap_or_default())
}

pub async fn get_vacation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<VacationStatus>> {
    let account = account_kind(&state, &id).await?;
    Ok(Json(VacationStatus {
        supported: gmail_supported(&account),
        settings: stored(&state, &id).await?,
    }))
}

/// Send the settings to Gmail with a token that is not about to expire,
/// refreshing and retrying once if Gmail answers 401.
async fn apply_with_refresh(
    tokens: &AccessTokenSource,
    api_base: &str,
    settings: &VacationSettings,
) -> Result<()> {
    with_fresh_token(
        tokens,
        gmail_settings::is_unauthorized,
        |token| async move { gmail_settings::apply_vacation(api_base, &token, settings).await },
    )
    .await
}

pub async fn put_vacation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(settings): Json<VacationSettings>,
) -> Result<Json<VacationStatus>> {
    let account = account_kind(&state, &id).await?;
    validate(&settings)?;
    if !gmail_supported(&account) {
        return Err(AppError::Unsupported(
            "vacation auto-reply is only supported for Gmail accounts; \
             this account's provider has no server-side responder (ManageSieve is not supported)"
                .to_string(),
        ));
    }
    let config = factory::load_account_config(&state.pool, &id).await?;
    let tokens = factory::oauth_token_source(&state.pool, &config)?;
    apply_with_refresh(&tokens, gmail_settings::GMAIL_API_BASE, &settings).await?;
    sqlx::query(
        r#"INSERT INTO account_vacation (account_id, enabled, subject, body, start_date, end_date, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
           ON CONFLICT(account_id) DO UPDATE SET
               enabled = excluded.enabled, subject = excluded.subject, body = excluded.body,
               start_date = excluded.start_date, end_date = excluded.end_date,
               updated_at = excluded.updated_at"#,
    )
    .bind(&id)
    .bind(settings.enabled)
    .bind(&settings.subject)
    .bind(&settings.body)
    .bind(&settings.start_date)
    .bind(&settings.end_date)
    .execute(&state.pool)
    .await?;
    Ok(Json(VacationStatus {
        supported: true,
        settings,
    }))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn setup() -> Arc<AppState> {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = dir.path().join("email.db").display().to_string();
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        Arc::new(AppState::new(pool, has_fts))
    }

    async fn seed(state: &AppState, id: &str, provider: &str, auth: &str, token: Option<&str>) {
        let sealed = token.map(|t| crate::credentials::seal(t).unwrap());
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type, oauth_token_json) VALUES (?,?,?,?,?,?)",
        )
        .bind(id)
        .bind("T")
        .bind("t@example.com")
        .bind(provider)
        .bind(auth)
        .bind(sealed)
        .execute(&state.pool)
        .await
        .unwrap();
    }

    async fn call(
        state: &Arc<AppState>,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(match body {
                Some(value) => Body::from(serde_json::to_vec(&value).unwrap()),
                None => Body::empty(),
            })
            .unwrap();
        let response = crate::api::router(Arc::clone(state))
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    fn away() -> serde_json::Value {
        serde_json::json!({
            "enabled": true, "subject": "Away", "body": "Back on the 5th",
            "startDate": "2026-12-24", "endDate": "2026-12-31"
        })
    }

    #[tokio::test]
    async fn imap_accounts_report_unsupported_and_refuse_writes_without_storing() {
        let state = setup().await;
        seed(&state, "a1", "generic", "basic", None).await;
        let (status, body) = call(&state, "GET", "/accounts/a1/vacation", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["supported"], false);
        assert_eq!(body["settings"]["enabled"], false);

        let (status, body) = call(&state, "PUT", "/accounts/a1/vacation", Some(away())).await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{body}");
        assert!(body["error"].as_str().unwrap().contains("Gmail"));
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM account_vacation")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn unknown_accounts_and_invalid_settings_are_rejected() {
        let state = setup().await;
        seed(&state, "a1", "gmail", "oauth2", None).await;
        let (status, _) = call(&state, "GET", "/accounts/nope/vacation", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        for body in [
            serde_json::json!({"enabled": true, "subject": "s", "body": "  "}),
            serde_json::json!({"enabled": false, "startDate": "2026-13-40"}),
            serde_json::json!({"enabled": false, "startDate": "2026-12-31", "endDate": "2026-12-24"}),
            serde_json::json!({"enabled": false, "subject": "x".repeat(256)}),
        ] {
            let (status, response) = call(&state, "PUT", "/accounts/a1/vacation", Some(body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        }
    }

    /// Serve one canned HTTP response per entry, returning the raw requests.
    fn serve(responses: Vec<&'static str>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 2048];
                loop {
                    let n = stream.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    if String::from_utf8_lossy(&buf).contains("enableAutoReply") || n == 0 {
                        break;
                    }
                }
                stream.write_all(response.as_bytes()).unwrap();
                requests.push(String::from_utf8_lossy(&buf).to_string());
            }
            requests
        });
        (base, handle)
    }

    const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    const UNAUTHORIZED: &str =
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    const FORBIDDEN: &str =
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";

    fn settings() -> VacationSettings {
        serde_json::from_value(away()).unwrap()
    }

    #[tokio::test]
    async fn control_characters_in_the_subject_are_rejected() {
        let state = setup().await;
        seed(&state, "a1", "gmail", "oauth2", None).await;
        for subject in [
            "Away\r\nBcc: x@example.com",
            "Away\nnow",
            "Away\r",
            "A\u{0}b",
            "A\tb",
        ] {
            let body = serde_json::json!({"enabled": false, "subject": subject});
            let (status, response) = call(&state, "PUT", "/accounts/a1/vacation", Some(body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{subject:?}: {response}");
        }
    }

    #[tokio::test]
    async fn an_expired_stored_token_is_refreshed_before_the_settings_call() {
        use crate::auth::token_source::testing::{FakeClock, FakeOAuth};
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock.clone(), 3600);
        let tokens = oauth.source("acct-vacation-expired");
        clock.advance(7200); // token-0 is now expired
        let (base, server) = serve(vec![OK]);

        apply_with_refresh(&tokens, &base, &settings())
            .await
            .unwrap();

        let requests = server.join().unwrap();
        assert_eq!(oauth.refresh_count(), 1);
        assert_eq!(requests.len(), 1);
        let request = requests[0].to_ascii_lowercase();
        assert!(request.contains("bearer token-1"), "{request}");
        assert!(!request.contains("token-0"), "{request}");
    }

    #[tokio::test]
    async fn a_401_refreshes_the_token_and_retries_once() {
        use crate::auth::token_source::testing::{FakeClock, FakeOAuth};
        let oauth = FakeOAuth::new(FakeClock::new(1_000_000), 3600);
        let tokens = oauth.source("acct-vacation-401");
        let (base, server) = serve(vec![UNAUTHORIZED, OK]);

        apply_with_refresh(&tokens, &base, &settings())
            .await
            .unwrap();

        let requests = server.join().unwrap();
        assert_eq!(oauth.refresh_count(), 1);
        assert_eq!(requests.len(), 2);
        assert!(requests[0].to_ascii_lowercase().contains("bearer token-0"));
        assert!(requests[1].to_ascii_lowercase().contains("bearer token-1"));
    }

    #[tokio::test]
    async fn a_second_401_reports_a_reconnect_and_a_403_is_not_retried() {
        use crate::auth::token_source::testing::{FakeClock, FakeOAuth};
        let oauth = FakeOAuth::new(FakeClock::new(1_000_000), 3600);
        let tokens = oauth.source("acct-vacation-401x2");
        let (base, server) = serve(vec![UNAUTHORIZED, UNAUTHORIZED]);
        let error = apply_with_refresh(&tokens, &base, &settings())
            .await
            .unwrap_err();
        assert!(
            matches!(error, AppError::Auth(ref m) if m.contains("reconnect")),
            "{error}"
        );
        assert_eq!(server.join().unwrap().len(), 2);

        let oauth = FakeOAuth::new(FakeClock::new(1_000_000), 3600);
        let tokens = oauth.source("acct-vacation-403");
        let (base, server) = serve(vec![FORBIDDEN]);
        let error = apply_with_refresh(&tokens, &base, &settings())
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::Auth(_)));
        assert_eq!(server.join().unwrap().len(), 1);
        assert_eq!(oauth.refresh_count(), 0);
    }
}
