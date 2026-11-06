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
    error::{AppError, Result},
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
    let body = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REPLY\r\nPRODID:-//email-rs//Invitation RSVP//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nSEQUENCE:{}\r\nATTENDEE;PARTSTAT={}:mailto:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n", invitation.uid, invitation.sequence.unwrap_or(0), status, from);
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
        serde_json::json!({"status": status, "outbox": response}),
    ))
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
        calendar = sqlx::query_scalar("SELECT CAST(content AS TEXT) FROM attachments WHERE message_id = ? AND lower(content_type) LIKE 'text/calendar%' ORDER BY size_bytes LIMIT 1")
            .bind(message_id).fetch_optional(&state.pool).await?;
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
    let uid = value("UID")
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
    let (head, value) = line.split_once(':')?;
    let mut pieces = head.split(';');
    if pieces.next()?.eq_ignore_ascii_case(wanted) {
        let params = pieces
            .filter_map(|part| {
                part.split_once('=')
                    .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
            })
            .collect();
        Some((
            params,
            value
                .replace("\\n", "\n")
                .replace("\\,", ",")
                .replace("\\;", ";")
                .replace("\\\\", "\\"),
        ))
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
    #[test]
    fn rejects_unbounded_and_malformed_input() {
        assert!(parse_ical(&"x".repeat(MAX_ICAL_BYTES + 1)).is_err());
        assert!(parse_ical("BEGIN:VCALENDAR\r\nEND:VCALENDAR").is_err());
    }
}
