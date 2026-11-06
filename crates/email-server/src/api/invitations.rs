use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use email_types::{OutgoingAttachment, SendRequest};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::outbox,
    calendar::CalendarService,
    error::{AppError, Result},
    providers::{Attendee, CalendarEvent},
    state::AppState,
};

const MAX_ICAL_BYTES: usize = 256 * 1024;
const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    pub message_id: String,
    pub method: String,
    pub uid: String,
    pub sequence: Option<i64>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub organizer: Option<String>,
    pub attendees: Vec<InvitationAttendee>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationAttendee {
    pub email: String,
    pub name: Option<String>,
    pub response_status: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RsvpRequest {
    pub response_status: String,
}

pub async fn get_invitation(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<Invitation>> {
    let invitation = load_or_parse(&state, &message_id).await?;
    Ok(Json(invitation))
}

pub async fn rsvp_invitation(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
    Json(request): Json<RsvpRequest>,
) -> Result<Json<serde_json::Value>> {
    let invitation = load_or_parse(&state, &message_id).await?;
    let status = normalize_status(&request.response_status)?;
    let organizer = invitation
        .organizer
        .clone()
        .ok_or_else(|| AppError::Validation("invitation has no organizer".into()))?;
    let (account_id, from) = sqlx::query_as::<_, (String, String)>(
        "SELECT m.account_id, COALESCE(a.email, '') FROM messages m JOIN accounts a ON a.id = m.account_id WHERE m.id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("message {message_id} not found")))?;
    if from.is_empty() {
        return Err(AppError::Validation(
            "message has no sender identity".into(),
        ));
    }
    if invitation.method != "REQUEST" {
        return Err(AppError::Validation(
            "only REQUEST invitations can be answered".into(),
        ));
    }
    if !invitation
        .attendees
        .iter()
        .any(|attendee| attendee.email.eq_ignore_ascii_case(&from))
    {
        return Err(AppError::Validation(
            "account address is not an attendee of this invitation".into(),
        ));
    }
    let body = build_reply(&invitation, &status, &from)?;
    // Local calendar change first: the reply is only queued once it succeeded,
    // so a retry after a failure cannot send a duplicate REPLY.
    let calendar_event_id =
        apply_to_calendar(&state.pool, &account_id, &invitation, &status, &from).await?;
    let request = SendRequest {
        account_id,
        to: vec![organizer],
        cc: vec![],
        bcc: vec![],
        subject: format!(
            "{}: {}",
            status.to_ascii_lowercase(),
            invitation.summary.as_deref().unwrap_or("invitation")
        ),
        text_body: Some(format!("RSVP: {status}")),
        html_body: None,
        in_reply_to: None,
        attachments: vec![OutgoingAttachment {
            filename: "invite.ics".into(),
            content_type: "text/calendar; method=REPLY".into(),
            content_base64: STANDARD.encode(body.as_bytes()),
        }],
        send_at: None,
        send_delay_seconds: None,
        identity_id: None,
    };
    let response = outbox::enqueue(&state, request).await?;
    Ok(Json(
        serde_json::json!({"status": status, "outbox": response, "calendarEventId": calendar_event_id}),
    ))
}

/// Calendar the invitation events are stored in.
const INVITATION_CALENDAR_ID: &str = "default";
const MAX_UID_LEN: usize = 255;

/// A UID is used as the event's provider id and may become a CalDAV object
/// name (`<calendar>/<uid>.ics`), so only a conservative set of characters is
/// accepted: no path separators, dots-only names, percent escapes, or control
/// characters.
fn valid_event_uid(uid: &str) -> bool {
    !uid.is_empty()
        && uid.len() <= MAX_UID_LEN
        && !uid.chars().all(|c| c == '.')
        && uid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '+' | '='))
}

/// ACCEPTED/TENTATIVE create or update the event for the invitation UID;
/// DECLINED removes it. Returns the stored event id, if any remains.
async fn apply_to_calendar(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    invitation: &Invitation,
    status: &str,
    from: &str,
) -> Result<Option<String>> {
    if !valid_event_uid(&invitation.uid) {
        return Err(AppError::Validation(
            "invitation UID is not valid for a calendar event".into(),
        ));
    }
    let service = CalendarService::new(pool.clone());
    if status == "DECLINED" {
        service
            .delete_event_by_uid(account_id, &invitation.uid)
            .await?;
        return Ok(None);
    }
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|_| AppError::Validation("invitation has an invalid time".into()))
    };
    let attendees = invitation
        .attendees
        .iter()
        .map(|a| Attendee {
            email: a.email.clone(),
            name: a.name.clone(),
            response_status: if a.email.eq_ignore_ascii_case(from) {
                Some(status.to_string())
            } else {
                a.response_status.clone()
            },
        })
        .collect();
    let event = CalendarEvent {
        id: String::new(),
        calendar_id: INVITATION_CALENDAR_ID.into(),
        provider_event_id: Some(invitation.uid.clone()),
        title: invitation
            .summary
            .clone()
            .unwrap_or_else(|| "(no title)".into()),
        description: invitation.description.clone(),
        start_at: parse(&invitation.start_at)?,
        end_at: parse(&invitation.end_at)?,
        location: invitation.location.clone(),
        is_all_day: false,
        recurrence_rule: None,
        attendees,
        meet_link: None,
    };
    let event_id = service.upsert_event(account_id, &event).await?;
    service
        .add_event_link(&event_id, &invitation.message_id)
        .await?;
    Ok(Some(event_id))
}

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

/// Fold a content line at 75 octets (RFC 5545 3.1) on char boundaries.
fn fold_line(line: &str) -> String {
    let mut out = String::new();
    let mut width = 0;
    for ch in line.chars() {
        let len = ch.len_utf8();
        if width + len > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(ch);
        width += len;
    }
    out
}

fn build_reply(invitation: &Invitation, status: &str, from: &str) -> Result<String> {
    // The UID is kept in its escaped wire form (never unescaped), so it is
    // inserted verbatim; anything that could break out of the line is refused.
    if invitation.uid.is_empty() || has_control(&invitation.uid) {
        return Err(AppError::Validation(
            "invitation UID contains control characters".into(),
        ));
    }
    if has_control(from) || from.contains([':', ';', ',', '"', '<', '>', ' ']) {
        return Err(AppError::Validation(
            "account address is not valid for a calendar reply".into(),
        ));
    }
    let lines = [
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "METHOD:REPLY".into(),
        "PRODID:-//email-rs//Invitation RSVP//EN".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}", invitation.uid),
        format!("SEQUENCE:{}", invitation.sequence.unwrap_or(0)),
        format!("ATTENDEE;PARTSTAT={status}:mailto:{from}"),
        "END:VEVENT".into(),
        "END:VCALENDAR".into(),
    ];
    let mut body = String::new();
    for line in lines {
        body.push_str(&fold_line(&line));
        body.push_str("\r\n");
    }
    Ok(body)
}

async fn load_or_parse(state: &AppState, message_id: &str) -> Result<Invitation> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!(
            "message {message_id} not found"
        )));
    }
    let body: Option<String> =
        sqlx::query_scalar("SELECT text_body FROM message_bodies WHERE message_id = ?")
            .bind(message_id)
            .fetch_optional(&state.pool)
            .await?;
    let mut calendar = body.filter(|value| {
        value.len() <= MAX_ICAL_BYTES
            && value
                .lines()
                .any(|line| line.eq_ignore_ascii_case("BEGIN:VCALENDAR"))
    });
    if calendar.is_none() {
        calendar = sqlx::query_scalar("SELECT CAST(content AS TEXT) FROM attachments WHERE message_id = ? AND lower(content_type) LIKE 'text/calendar%' AND length(content) <= ? ORDER BY size_bytes LIMIT 1")
            .bind(message_id).bind(MAX_ICAL_BYTES as i64).fetch_optional(&state.pool).await?;
    }
    let source =
        calendar.ok_or_else(|| AppError::NotFound("message has no calendar part".into()))?;
    let parsed = parse_ical(&source)?;
    let invitation = Invitation {
        message_id: message_id.into(),
        method: parsed.method,
        uid: parsed.uid,
        sequence: parsed.sequence,
        summary: parsed.summary,
        description: parsed.description,
        location: parsed.location,
        start_at: parsed.start_at,
        end_at: parsed.end_at,
        organizer: parsed.organizer,
        attendees: parsed.attendees,
    };
    sqlx::query("INSERT INTO calendar_invitations (id, message_id, method, uid, sequence, summary, description, location, start_at, end_at, organizer, attendees_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET method=excluded.method, uid=excluded.uid, sequence=excluded.sequence, summary=excluded.summary, description=excluded.description, location=excluded.location, start_at=excluded.start_at, end_at=excluded.end_at, organizer=excluded.organizer, attendees_json=excluded.attendees_json")
        .bind(Uuid::new_v4().to_string()).bind(message_id).bind(&invitation.method).bind(&invitation.uid).bind(invitation.sequence).bind(&invitation.summary).bind(&invitation.description).bind(&invitation.location).bind(&invitation.start_at).bind(&invitation.end_at).bind(&invitation.organizer).bind(serde_json::to_string(&invitation.attendees).expect("attendees serialize")).execute(&state.pool).await?;
    Ok(invitation)
}

struct ParsedInvitation {
    method: String,
    uid: String,
    sequence: Option<i64>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    start_at: String,
    end_at: String,
    organizer: Option<String>,
    attendees: Vec<InvitationAttendee>,
}

fn parse_ical(input: &str) -> Result<ParsedInvitation> {
    if input.len() > MAX_ICAL_BYTES {
        return Err(AppError::PayloadTooLarge(
            "calendar part exceeds 256 KiB".into(),
        ));
    }
    let mut lines: Vec<String> = Vec::new();
    for raw in input.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if raw.len() > MAX_LINE_BYTES {
            return Err(AppError::Validation("calendar line is too long".into()));
        }
        if raw.starts_with(' ') || raw.starts_with('\t') {
            if let Some(last) = lines.last_mut() {
                last.push_str(raw.trim_start());
            } else {
                return Err(AppError::Validation(
                    "calendar continuation without a property".into(),
                ));
            }
        } else if !raw.is_empty() {
            lines.push(raw.to_string());
        }
    }
    if !lines.iter().any(|line| line == "BEGIN:VCALENDAR")
        || !lines.iter().any(|line| line == "END:VCALENDAR")
    {
        return Err(AppError::Validation("malformed iCalendar envelope".into()));
    }
    let value = |name: &str| {
        lines
            .iter()
            .find_map(|line| property(line, name).map(|(_, value)| value))
    };
    let method = value("METHOD")
        .unwrap_or_else(|| "REQUEST".into())
        .to_ascii_uppercase();
    if !["REQUEST", "CANCEL", "REPLY"].contains(&method.as_str()) {
        return Err(AppError::Validation("unsupported iTIP method".into()));
    }
    let uid = lines
        .iter()
        .find_map(|line| property_raw(line, "UID").map(|(_, value)| value))
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::Validation("calendar event is missing UID".into()))?;
    let start = value("DTSTART")
        .ok_or_else(|| AppError::Validation("calendar event is missing DTSTART".into()))
        .and_then(|v| parse_datetime(&v))?;
    let end = value("DTEND")
        .ok_or_else(|| AppError::Validation("calendar event is missing DTEND".into()))
        .and_then(|v| parse_datetime(&v))?;
    if end <= start {
        return Err(AppError::Validation(
            "calendar event end must follow start".into(),
        ));
    }
    let organizer = value("ORGANIZER").and_then(|v| {
        v.strip_prefix("mailto:")
            .or_else(|| v.strip_prefix("MAILTO:"))
            .map(str::to_ascii_lowercase)
    });
    let attendees = lines
        .iter()
        .filter_map(|line| {
            let (params, value) = property(line, "ATTENDEE")?;
            let email = value
                .strip_prefix("mailto:")
                .or_else(|| value.strip_prefix("MAILTO:"))
                .unwrap_or(&value)
                .to_ascii_lowercase();
            if !email.contains('@') {
                return None;
            }
            Some(InvitationAttendee {
                email,
                name: params
                    .iter()
                    .find_map(|(key, value)| (key == "CN").then(|| value.clone())),
                response_status: params
                    .iter()
                    .find_map(|(key, value)| (key == "PARTSTAT").then(|| value.clone())),
            })
        })
        .collect();
    Ok(ParsedInvitation {
        method,
        uid,
        sequence: value("SEQUENCE").and_then(|v| v.parse().ok()),
        summary: value("SUMMARY"),
        description: value("DESCRIPTION"),
        location: value("LOCATION"),
        start_at: start.to_rfc3339(),
        end_at: end.to_rfc3339(),
        organizer,
        attendees,
    })
}

fn property(line: &str, wanted: &str) -> Option<(Vec<(String, String)>, String)> {
    property_raw(line, wanted).map(|(params, value)| {
        (
            params,
            value
                .replace("\\n", "\n")
                .replace("\\,", ",")
                .replace("\\;", ";")
                .replace("\\\\", "\\"),
        )
    })
}

/// Like `property` but leaves the value in its escaped wire form.
fn property_raw(line: &str, wanted: &str) -> Option<(Vec<(String, String)>, String)> {
    let (head, value) = line.split_once(':')?;
    let mut pieces = head.split(';');
    if pieces.next()?.eq_ignore_ascii_case(wanted) {
        let params = pieces
            .filter_map(|part| {
                part.split_once('=')
                    .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
            })
            .collect();
        Some((params, value.to_string()))
    } else {
        None
    }
}
fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    if value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")
            .map_err(|_| AppError::Validation("invalid calendar date".into()))?;
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(0, 0, 0).unwrap(),
            Utc,
        ));
    }
    let trimmed = value.strip_suffix('Z').unwrap_or(value);
    let date = NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%S")
        .map_err(|_| AppError::Validation("invalid calendar datetime".into()))?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(date, Utc))
}
fn normalize_status(status: &str) -> Result<String> {
    let status = status.trim().to_ascii_uppercase();
    if ["ACCEPTED", "DECLINED", "TENTATIVE"].contains(&status.as_str()) {
        Ok(status)
    } else {
        Err(AppError::Validation(
            "responseStatus must be ACCEPTED, DECLINED, or TENTATIVE".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_request_and_unfolds_lines() {
        let parsed = parse_ical("BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:abc\r\nDTSTART:20261106T120000Z\r\nDTEND:20261106T130000Z\r\nSUMMARY:Team\r\nORGANIZER:mailto:Host@EXAMPLE.COM\r\nATTENDEE;CN=Me:mailto:me@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n").unwrap();
        assert_eq!(parsed.uid, "abc");
        assert_eq!(parsed.organizer.as_deref(), Some("host@example.com"));
        assert_eq!(parsed.attendees[0].name.as_deref(), Some("Me"));
    }
    fn invitation_with_uid(uid: &str) -> Invitation {
        Invitation {
            message_id: "m".into(),
            method: "REQUEST".into(),
            uid: uid.into(),
            sequence: None,
            summary: None,
            description: None,
            location: None,
            start_at: String::new(),
            end_at: String::new(),
            organizer: None,
            attendees: vec![],
        }
    }
    #[test]
    fn escaped_newline_in_uid_cannot_inject_reply_lines() {
        let parsed = parse_ical("BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:x\\nATTENDEE;PARTSTAT=ACCEPTED:mailto:boss@corp\r\nDTSTART:20261106T120000Z\r\nDTEND:20261106T130000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n").unwrap();
        let invitation = invitation_with_uid(&parsed.uid);
        let body = build_reply(&invitation, "ACCEPTED", "me@example.com").unwrap();
        let attendees = body.lines().filter(|l| l.starts_with("ATTENDEE")).count();
        assert_eq!(attendees, 1, "{body}");
        assert!(
            !body
                .lines()
                .any(|l| l.contains("boss@corp") && !l.starts_with("UID:")),
            "{body}"
        );
    }
    #[test]
    fn reply_rejects_control_characters_and_folds_long_lines() {
        for uid in ["a\r\nATTENDEE:mailto:x@y", "a\nb", "a\rb", "a\0b", ""] {
            assert!(build_reply(&invitation_with_uid(uid), "ACCEPTED", "me@example.com").is_err());
        }
        assert!(build_reply(&invitation_with_uid("u"), "ACCEPTED", "a\r\nb@x").is_err());
        let long = "u".repeat(200);
        let body = build_reply(&invitation_with_uid(&long), "ACCEPTED", "me@example.com").unwrap();
        assert!(body.split("\r\n").all(|l| l.len() <= 75));
    }
    #[test]
    fn rejects_unbounded_and_malformed_input() {
        assert!(parse_ical(&"x".repeat(MAX_ICAL_BYTES + 1)).is_err());
        assert!(parse_ical("BEGIN:VCALENDAR\r\nEND:VCALENDAR").is_err());
    }
    #[test]
    fn event_uid_rejects_path_and_control_injection() {
        for uid in [
            "a/b", "../x", "..", ".", "a\\b", "a?b", "a#b", "a%2fb", "a b", "a\nb", "", "é",
        ] {
            assert!(!valid_event_uid(uid), "{uid:?}");
        }
        assert!(!valid_event_uid(&"u".repeat(MAX_UID_LEN + 1)));
        for uid in ["abc", "040000008200E000", "a-b_c.d+e=f@example.com"] {
            assert!(valid_event_uid(uid), "{uid}");
        }
    }

    async fn setup() -> (sqlx::SqlitePool, Invitation) {
        let (pool, _) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('acc', 'Me', 'me@example.com', 'imap', 'password')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path) VALUES ('f', 'acc', 'INBOX', 'INBOX')")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid) VALUES ('m', 'acc', 'f', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut invitation = invitation_with_uid("evt-1@example.com");
        invitation.summary = Some("Team".into());
        invitation.start_at = "2026-11-06T12:00:00+00:00".into();
        invitation.end_at = "2026-11-06T13:00:00+00:00".into();
        invitation.attendees = vec![InvitationAttendee {
            email: "me@example.com".into(),
            name: None,
            response_status: Some("NEEDS-ACTION".into()),
        }];
        (pool, invitation)
    }

    async fn event_count(pool: &sqlx::SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM calendar_events")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn accept_creates_then_updates_event_by_uid_and_decline_removes_it() {
        let (pool, mut invitation) = setup().await;
        let id = apply_to_calendar(&pool, "acc", &invitation, "ACCEPTED", "me@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event_count(&pool).await, 1);
        invitation.summary = Some("Team renamed".into());
        let again = apply_to_calendar(&pool, "acc", &invitation, "TENTATIVE", "me@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(id, again);
        assert_eq!(event_count(&pool).await, 1);
        let (title, attendees): (String, String) =
            sqlx::query_as("SELECT title, attendees_json FROM calendar_events WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(title, "Team renamed");
        assert!(attendees.contains("TENTATIVE"), "{attendees}");
        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_calendar_links")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(links, 1);
        let gone = apply_to_calendar(&pool, "acc", &invitation, "DECLINED", "me@example.com")
            .await
            .unwrap();
        assert!(gone.is_none());
        assert_eq!(event_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn accept_does_not_update_synced_event_in_another_calendar() {
        let (pool, invitation) = setup().await;
        sqlx::query(
            "INSERT INTO calendar_events (id, account_id, calendar_id, provider_event_id, title, start_at, end_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("synced")
        .bind("acc")
        .bind("google")
        .bind(&invitation.uid)
        .bind("Synced event")
        .bind(&invitation.start_at)
        .bind(&invitation.end_at)
        .execute(&pool)
        .await
        .unwrap();

        let invitation_id =
            apply_to_calendar(&pool, "acc", &invitation, "ACCEPTED", "me@example.com")
                .await
                .unwrap()
                .unwrap();
        assert_ne!(invitation_id, "synced");
        let (title, calendar_id): (String, String) =
            sqlx::query_as("SELECT title, calendar_id FROM calendar_events WHERE id = 'synced'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(title, "Synced event");
        assert_eq!(calendar_id, "google");
    }

    #[tokio::test]
    async fn decline_does_not_delete_synced_event_in_another_calendar() {
        let (pool, invitation) = setup().await;
        sqlx::query(
            "INSERT INTO calendar_events (id, account_id, calendar_id, provider_event_id, title, start_at, end_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("synced")
        .bind("acc")
        .bind("google")
        .bind(&invitation.uid)
        .bind("Synced event")
        .bind(&invitation.start_at)
        .bind(&invitation.end_at)
        .execute(&pool)
        .await
        .unwrap();

        assert!(
            apply_to_calendar(&pool, "acc", &invitation, "DECLINED", "me@example.com")
                .await
                .unwrap()
                .is_none()
        );
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM calendar_events WHERE id = 'synced'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn unsafe_uid_is_refused_before_touching_the_calendar() {
        let (pool, mut invitation) = setup().await;
        invitation.uid = "../../etc/passwd".into();
        for status in ["ACCEPTED", "DECLINED"] {
            assert!(
                apply_to_calendar(&pool, "acc", &invitation, status, "me@example.com")
                    .await
                    .is_err()
            );
        }
        assert_eq!(event_count(&pool).await, 0);
    }
}
