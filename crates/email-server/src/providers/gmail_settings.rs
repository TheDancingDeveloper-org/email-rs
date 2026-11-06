//! Gmail account settings that the IMAP connection cannot reach. Only the
//! vacation responder lives here; sync and mutations stay in `gmail.rs`.

use chrono::{Duration, NaiveDate};
use email_types::VacationSettings;
use serde_json::json;

use crate::error::{AppError, Result};

/// The only host the bearer token is ever sent to. Tests pass their own base
/// to [`apply_vacation`]; there is no runtime override.
pub const GMAIL_API_BASE: &str = "https://gmail.googleapis.com";

const UNAUTHORIZED: &str = "Gmail settings: unauthorized";

/// Gmail answered 401: the access token was rejected (as opposed to a 403
/// permission failure, which a refresh cannot fix).
pub fn is_unauthorized(error: &AppError) -> bool {
    matches!(error, AppError::Auth(message) if message.starts_with(UNAUTHORIZED))
}

fn midnight_utc_millis(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_utc()
        .timestamp_millis()
}

/// The `users.settings.updateVacation` request body. The end date is
/// inclusive, so the window closes at the last millisecond of that day.
pub(crate) fn vacation_request_body(settings: &VacationSettings) -> serde_json::Value {
    let parse = |value: &Option<String>| {
        value
            .as_deref()
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
    };
    let mut body = json!({
        "enableAutoReply": settings.enabled,
        "responseSubject": settings.subject,
        "responseBodyPlainText": settings.body,
        "restrictToContacts": false,
        "restrictToDomain": false,
    });
    if let Some(start) = parse(&settings.start_date) {
        body["startTime"] = json!(midnight_utc_millis(start).to_string());
    }
    if let Some(end) = parse(&settings.end_date) {
        body["endTime"] = json!((midnight_utc_millis(end + Duration::days(1)) - 1).to_string());
    }
    body
}

/// Apply the vacation responder to the signed-in Gmail account.
pub async fn apply_vacation(
    api_base: &str,
    access_token: &str,
    settings: &VacationSettings,
) -> Result<()> {
    let response = reqwest::Client::new()
        .put(format!(
            "{}/gmail/v1/users/me/settings/vacation",
            api_base.trim_end_matches('/')
        ))
        .bearer_auth(access_token)
        .json(&vacation_request_body(settings))
        .send()
        .await
        .map_err(|error| AppError::Transient(format!("Gmail settings: {error}")))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let detail = response.text().await.unwrap_or_default();
    let detail: String = detail.chars().take(300).collect();
    Err(match status.as_u16() {
        401 => AppError::Auth(format!("{UNAUTHORIZED} — {detail}")),
        403 => AppError::Auth(format!(
            "Gmail settings: permission denied; reconnect the account — {detail}"
        )),
        400 => AppError::Validation(format!("Gmail rejected the vacation reply: {detail}")),
        code => AppError::Provider(format!("Gmail settings error {code}: {detail}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_maps_fields_and_an_inclusive_utc_window() {
        let body = vacation_request_body(&VacationSettings {
            enabled: true,
            subject: "Away".into(),
            body: "Back soon".into(),
            start_date: Some("2026-12-24".into()),
            end_date: Some("2026-12-25".into()),
        });
        assert_eq!(body["enableAutoReply"], true);
        assert_eq!(body["responseSubject"], "Away");
        assert_eq!(body["responseBodyPlainText"], "Back soon");
        assert_eq!(body["startTime"], "1798070400000");
        // 2026-12-26T00:00:00Z minus one millisecond.
        assert_eq!(body["endTime"], "1798243199999");
    }

    #[test]
    fn request_body_omits_an_unset_window() {
        let body = vacation_request_body(&VacationSettings::default());
        assert_eq!(body["enableAutoReply"], false);
        assert!(body.get("startTime").is_none());
        assert!(body.get("endTime").is_none());
    }
}
