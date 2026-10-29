#![allow(dead_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ── Domain types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: Uuid,
    pub name: String,
    pub full_path: String,
    pub special_use: Option<String>,
    pub unread_count: u32,
    pub total_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub uid: u32,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: Option<DateTime<Utc>>,
    pub is_read: bool,
    pub is_flagged: bool,
    pub is_draft: bool,
    pub has_attachments: bool,
    pub attachments: Vec<ProviderAttachment>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBody {
    pub message_id: String,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    pub raw_headers: Option<String>,
    pub attachments: Vec<ProviderAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAttachment {
    pub part_id: String,
    pub filename: String,
    pub content_type: String,
    pub content_id: Option<String>,
    pub size_bytes: i64,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attendee {
    pub email: String,
    pub name: Option<String>,
    pub response_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub calendar_id: String,
    pub provider_event_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub location: Option<String>,
    pub is_all_day: bool,
    pub recurrence_rule: Option<String>,
    pub attendees: Vec<Attendee>,
    pub meet_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCalendarEvent {
    pub title: String,
    pub description: Option<String>,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub location: Option<String>,
    pub is_all_day: bool,
    pub attendees: Vec<Attendee>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusySlot {
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

// ── Provider traits ──────────────────────────────────────────────────────────

#[async_trait]
pub trait MailProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn authenticate(&mut self) -> Result<()>;
    async fn list_folders(&self) -> Result<Vec<Folder>>;
    /// Return the mailbox UIDVALIDITY value used to scope IMAP UIDs.
    async fn folder_uid_validity(&self, folder: &str) -> Result<Option<u32>>;
    async fn fetch_messages(
        &self,
        folder: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>>;
    /// Fetch metadata for an explicit set of UIDs. Providers may override this
    /// with one UID FETCH; the default remains correct for contract/test providers.
    async fn fetch_messages_by_uid(&self, folder: &str, uids: &[u32]) -> Result<Vec<Message>> {
        let wanted: std::collections::HashSet<u32> = uids.iter().copied().collect();
        Ok(self
            .fetch_messages(folder, None)
            .await?
            .into_iter()
            .filter(|message| wanted.contains(&message.uid))
            .collect())
    }
    /// Return the complete UID set currently present in a mailbox. Sync uses
    /// this independently of date-windowed metadata fetches to reconcile
    /// messages that were removed or moved by another client.
    async fn list_message_uids(&self, folder: &str) -> Result<Vec<u32>>;
    async fn fetch_message_body(&self, folder: &str, uid: u32) -> Result<MessageBody>;
    async fn fetch_attachment(&self, folder: &str, uid: u32, part_id: &str) -> Result<Vec<u8>> {
        let body = self.fetch_message_body(folder, uid).await?;
        body.attachments
            .into_iter()
            .find(|attachment| attachment.part_id == part_id)
            .and_then(|attachment| attachment.content)
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!(
                    "attachment part {part_id} is unavailable"
                ))
            })
    }
    /// Batch-fetch bodies for multiple UIDs in a single IMAP session.
    /// Returns (uid, body) pairs; silently skips any UIDs that fail to parse.
    async fn fetch_bodies_batch(
        &self,
        folder: &str,
        uids: &[u32],
    ) -> Result<Vec<(u32, MessageBody)>>;
    /// Set the \Seen flag on the server so the read state is durable across clients.
    async fn mark_seen(&self, folder: &str, uid: u32) -> Result<()>;
    /// Remove the \Seen flag on the server.
    async fn mark_unseen(&self, folder: &str, uid: u32) -> Result<()>;
    /// Move a message to the trash folder (permanent delete on providers without trash).
    async fn delete_message(&self, folder: &str, uid: u32) -> Result<()>;
    /// Move a message to a destination folder by IMAP MOVE (or COPY+STORE \Deleted+EXPUNGE).
    async fn move_message(&self, src_folder: &str, uid: u32, dest_folder: &str) -> Result<()>;
    /// Toggle the \Flagged flag.
    async fn set_flagged(&self, folder: &str, uid: u32, flagged: bool) -> Result<()>;
    /// Append a provider-synchronized RFC/MIME draft to the given mailbox.
    async fn append_draft(
        &self,
        _folder: &str,
        _message_id: &str,
        _raw_message: &[u8],
    ) -> Result<u32> {
        Err(crate::error::AppError::Provider(
            "provider does not support synchronized drafts".into(),
        ))
    }
    /// Permanently remove a draft. This deliberately bypasses normal trash behavior.
    async fn delete_draft(&self, folder: &str, uid: u32) -> Result<()> {
        self.delete_message(folder, uid).await
    }
}

#[async_trait]
pub trait CalendarProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn list_calendars(&self) -> Result<Vec<Calendar>>;
    async fn list_events(
        &self,
        calendar_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>>;
    async fn create_event(
        &self,
        calendar_id: &str,
        event: NewCalendarEvent,
    ) -> Result<CalendarEvent>;
    async fn update_event(&self, calendar_id: &str, event: CalendarEvent) -> Result<CalendarEvent>;
    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<()>;
}

/// Optional richer API — Google Calendar, MS Graph etc.
#[async_trait]
pub trait RichCalendarProvider: CalendarProvider {
    async fn get_free_busy(
        &self,
        calendar_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<BusySlot>>;
    async fn create_meeting_link(&self, event: &CalendarEvent) -> Result<Option<String>>;
}

pub mod caldav;
pub mod factory;
pub mod generic_imap;
pub mod gmail;
pub mod google_calendar;

/// Longest provider error text carried into an [`AppError`]. async-imap's
/// parse errors embed the whole unparsed read buffer, up to 512 MiB.
const MAX_IMAP_ERROR_CHARS: usize = 500;

/// Drain an IMAP response stream (FETCH, STORE, LIST, EXPUNGE).
///
/// A malformed message is one item: log it, count it and skip it, so one bad
/// message does not fail the rest of the folder. A connection error is not
/// that. async-imap does not end a response stream after a transport error:
/// once the server drops the connection mid-response, every further poll
/// yields the same error again. `collect()` on such a stream never returns
/// and allocates at memory bandwidth until the host runs out of memory (the
/// server grew to 63–69 GB three times). The connection is unusable after the
/// first repeated error, so stop there.
pub(crate) async fn drain_imap_stream<T, E, S>(stream: S, context: &str) -> Result<Vec<T>>
where
    S: futures::Stream<Item = std::result::Result<T, E>>,
    E: std::fmt::Display,
{
    use futures::StreamExt as _;

    let mut stream = std::pin::pin!(stream);
    let mut items = Vec::new();
    let mut skipped = 0usize;
    let mut last_error: Option<String> = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(item) => {
                last_error = None;
                items.push(item);
            }
            Err(error) => {
                let detail: String = error
                    .to_string()
                    .chars()
                    .take(MAX_IMAP_ERROR_CHARS)
                    .collect();
                // A dropped connection repeats the same error forever. A
                // malformed message does not: the stream moves on.
                if last_error.as_deref() == Some(detail.as_str()) {
                    return Err(crate::error::AppError::Imap(format!(
                        "{context} failed: {detail}"
                    )));
                }
                tracing::warn!("{context}: skipping malformed item: {detail}");
                skipped += 1;
                last_error = Some(detail);
            }
        }
    }
    if skipped > 0 {
        tracing::warn!("{context}: skipped {skipped} malformed item(s)");
    }
    Ok(items)
}

pub(crate) fn parse_message_body(uid: u32, raw: &[u8]) -> Result<MessageBody> {
    use mail_parser::MimeHeaders as _;

    let parsed = mail_parser::MessageParser::default()
        .parse(raw)
        .ok_or_else(|| crate::error::AppError::Imap("failed to parse message".to_string()))?;
    let text_body = parsed.body_text(0).map(|body| body.to_string());
    let html_body = parsed.body_html(0).map(|body| body.to_string());
    let raw_headers = std::str::from_utf8(raw).ok().and_then(|message| {
        message
            .split_once("\r\n\r\n")
            .map(|(headers, _)| headers.to_string())
    });
    let part_paths = mime_part_paths(&parsed);
    let attachments = parsed
        .attachments
        .iter()
        .enumerate()
        .filter_map(|(index, part_index)| {
            parsed
                .parts
                .get(*part_index)
                .map(|part| (index, *part_index, part))
        })
        .map(|(index, part_index, attachment)| {
            let bytes = attachment.contents();
            let content_type = attachment
                .content_type()
                .map(|content_type| {
                    format!(
                        "{}/{}",
                        content_type.ctype(),
                        content_type.subtype().unwrap_or("octet-stream")
                    )
                })
                .unwrap_or_else(|| "application/octet-stream".to_string());
            ProviderAttachment {
                part_id: part_paths
                    .get(&part_index)
                    .cloned()
                    .unwrap_or_else(|| (index + 1).to_string()),
                filename: attachment
                    .attachment_name()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("attachment-{}", index + 1)),
                content_type,
                content_id: attachment.content_id().map(ToString::to_string),
                size_bytes: bytes.len() as i64,
                content: Some(bytes.to_vec()),
            }
        })
        .collect();

    Ok(MessageBody {
        message_id: uid.to_string(),
        html_body,
        text_body,
        raw_headers,
        attachments,
    })
}

fn mime_part_paths(message: &mail_parser::Message<'_>) -> std::collections::HashMap<usize, String> {
    fn visit(
        message: &mail_parser::Message<'_>,
        id: usize,
        path: String,
        paths: &mut std::collections::HashMap<usize, String>,
    ) {
        let Some(part) = message.parts.get(id) else {
            return;
        };
        if let mail_parser::PartType::Multipart(children) = &part.body {
            for (index, child) in children.iter().enumerate() {
                let child_path = if path.is_empty() {
                    (index + 1).to_string()
                } else {
                    format!("{path}.{}", index + 1)
                };
                visit(message, *child, child_path, paths);
            }
        } else {
            paths.insert(id, if path.is_empty() { "1".into() } else { path });
        }
    }
    let mut paths = std::collections::HashMap::new();
    if !message.parts.is_empty() {
        visit(message, 0, String::new(), &mut paths);
    }
    paths
}

pub(crate) fn valid_imap_part_id(part_id: &str) -> bool {
    !part_id.is_empty()
        && part_id.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| byte.is_ascii_digit())
                && segment.parse::<u32>().is_ok_and(|value| value > 0)
        })
}

pub(crate) fn bodystructure_has_attachments(
    structure: &async_imap::imap_proto::types::BodyStructure<'_>,
) -> bool {
    use async_imap::imap_proto::types::BodyStructure;

    match structure {
        BodyStructure::Multipart { bodies, .. } => bodies.iter().any(bodystructure_has_attachments),
        BodyStructure::Basic { common, .. } | BodyStructure::Message { common, .. } => {
            common
                .disposition
                .as_ref()
                .is_some_and(|disposition| disposition.ty.eq_ignore_ascii_case("attachment"))
                || !common.ty.ty.eq_ignore_ascii_case("text")
        }
        BodyStructure::Text { common, .. } => common
            .disposition
            .as_ref()
            .is_some_and(|disposition| disposition.ty.eq_ignore_ascii_case("attachment")),
    }
}

pub(crate) fn bodystructure_attachments(
    structure: &async_imap::imap_proto::types::BodyStructure<'_>,
) -> Vec<ProviderAttachment> {
    use async_imap::imap_proto::types::BodyStructure;
    fn param(params: &async_imap::imap_proto::types::BodyParams<'_>, key: &str) -> Option<String> {
        params
            .as_ref()?
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.to_string())
    }
    fn walk(structure: &BodyStructure<'_>, path: String, out: &mut Vec<ProviderAttachment>) {
        match structure {
            BodyStructure::Multipart { bodies, .. } => {
                for (index, body) in bodies.iter().enumerate() {
                    let child = if path.is_empty() {
                        (index + 1).to_string()
                    } else {
                        format!("{path}.{}", index + 1)
                    };
                    walk(body, child, out);
                }
            }
            BodyStructure::Basic { common, other, .. }
            | BodyStructure::Text { common, other, .. }
            | BodyStructure::Message { common, other, .. } => {
                let disposition_attachment = common.disposition.as_ref().is_some_and(|value| {
                    value.ty.eq_ignore_ascii_case("attachment")
                        || value.ty.eq_ignore_ascii_case("inline")
                });
                if disposition_attachment || !common.ty.ty.eq_ignore_ascii_case("text") {
                    let filename = common
                        .disposition
                        .as_ref()
                        .and_then(|value| param(&value.params, "filename"))
                        .or_else(|| param(&common.ty.params, "name"))
                        .unwrap_or_else(|| format!("attachment-{path}"));
                    out.push(ProviderAttachment {
                        part_id: if path.is_empty() { "1".into() } else { path },
                        filename,
                        content_type: format!(
                            "{}/{}",
                            common.ty.ty.to_ascii_lowercase(),
                            common.ty.subtype.to_ascii_lowercase()
                        ),
                        content_id: other.id.as_deref().map(str::to_string),
                        size_bytes: i64::from(other.octets),
                        content: None,
                    });
                }
            }
        }
    }
    let mut attachments = Vec::new();
    walk(structure, String::new(), &mut attachments);
    attachments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_mime_attachment_with_authoritative_part_path() {
        let raw = concat!(
            "From: a@example.test\r\nMIME-Version: 1.0\r\n",
            "Content-Type: multipart/mixed; boundary=outer\r\n\r\n",
            "--outer\r\nContent-Type: text/plain\r\n\r\nhello\r\n",
            "--outer\r\nContent-Type: multipart/mixed; boundary=inner\r\n\r\n",
            "--inner\r\nContent-Type: application/pdf\r\n",
            "Content-Disposition: attachment; filename=report.pdf\r\n",
            "Content-Transfer-Encoding: base64\r\n\r\ncGRm\r\n--inner--\r\n",
            "--outer--\r\n"
        );
        let body = parse_message_body(1, raw.as_bytes()).unwrap();
        assert_eq!(body.attachments.len(), 1);
        assert_eq!(body.attachments[0].part_id, "2.1");
        assert_eq!(body.attachments[0].filename, "report.pdf");
        assert_eq!(
            body.attachments[0].content.as_deref(),
            Some(b"pdf".as_slice())
        );
    }

    #[test]
    fn malformed_part_identifiers_are_rejected_before_imap_command_construction() {
        for invalid in ["", "0", "1.", ".1", "1 BODY[]", "1.a", "-1"] {
            assert!(!valid_imap_part_id(invalid), "{invalid}");
        }
        for valid in ["1", "2.1", "10.22.3"] {
            assert!(valid_imap_part_id(valid));
        }
    }

    #[test]
    fn missing_and_duplicate_filenames_remain_distinct_by_part_id() {
        let raw = concat!(
            "MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n",
            "--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment\r\n\r\na\r\n",
            "--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=same.bin\r\n\r\nb\r\n",
            "--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=same.bin\r\n\r\nc\r\n--x--\r\n"
        );
        let body = parse_message_body(1, raw.as_bytes()).unwrap();
        assert_eq!(
            body.attachments
                .iter()
                .map(|a| a.part_id.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
        assert_eq!(body.attachments[0].filename, "attachment-1");
        assert_eq!(body.attachments[1].filename, body.attachments[2].filename);
    }

    /// Fail the test, instead of exhausting memory, once a stream has handed
    /// out far more items than the server could have sent.
    fn bounded<S: futures::Stream>(stream: S) -> impl futures::Stream<Item = S::Item> {
        use futures::StreamExt as _;
        let mut polled = 0usize;
        stream.inspect(move |_| {
            polled += 1;
            assert!(
                polled < 10_000,
                "IMAP stream drained past its first error (WI-892)"
            );
        })
    }

    /// An IMAP server that answers UID FETCH with half a literal and then
    /// closes the connection, as Gmail does when it drops a session.
    async fn imap_session_dropped_mid_fetch() -> async_imap::Session<tokio::io::DuplexStream> {
        use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

        let (client, server) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(server);
            let mut reader = tokio::io::BufReader::new(reader);
            writer.write_all(b"* OK ready\r\n").await.unwrap();
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.unwrap() == 0 {
                    return;
                }
                let tag = line
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let reply = if line.contains("LOGIN") {
                    format!("{tag} OK logged in\r\n")
                } else if line.contains("SELECT") {
                    format!(
                        "* 1 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n{tag} OK [READ-WRITE] done\r\n"
                    )
                } else {
                    writer
                        .write_all(b"* 1 FETCH (UID 1 BODY[] {500}\r\nFrom: a@example.com\r\n")
                        .await
                        .unwrap();
                    writer.shutdown().await.unwrap();
                    return;
                };
                writer.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let mut session = async_imap::Client::new(client)
            .login("user", "password")
            .await
            .map_err(|(error, _)| error)
            .unwrap();
        session.select("INBOX").await.unwrap();
        session
    }

    #[tokio::test]
    async fn dropped_connection_mid_fetch_fails_instead_of_growing_forever() {
        let mut session = imap_session_dropped_mid_fetch().await;
        let stream = session.uid_fetch("1", "(UID BODY.PEEK[])").await.unwrap();

        let error = drain_imap_stream(bounded(stream), "UID FETCH")
            .await
            .expect_err("a truncated FETCH response is an error");

        assert!(error.to_string().contains("UID FETCH failed"), "{error}");
    }

    #[tokio::test]
    async fn async_imap_repeats_the_error_after_a_dropped_connection() {
        // The upstream behaviour the helper guards against: the stream never
        // ends once the connection is gone, so `collect()` would never return.
        // If an async-imap upgrade fails this, upstream ends the stream now;
        // keep the helper and drop this test.
        use futures::StreamExt as _;

        let mut session = imap_session_dropped_mid_fetch().await;
        let stream = session.uid_fetch("1", "(UID BODY.PEEK[])").await.unwrap();
        let items: Vec<_> = stream.take(1_000).collect().await;

        assert_eq!(items.len(), 1_000);
        assert!(items.iter().all(|item| item.is_err()));
    }

    #[tokio::test]
    async fn imap_error_text_is_capped() {
        // A dropped connection repeats the same error. The text that reaches
        // the caller is capped even when that error is megabytes long.
        let huge = "x".repeat(10 * 1024 * 1024);
        let stream = futures::stream::iter([Ok(1), Err(huge.clone()), Err(huge)]);

        let error = drain_imap_stream(stream, "UID FETCH").await.unwrap_err();

        assert!(error.to_string().len() < 2 * MAX_IMAP_ERROR_CHARS);
    }

    #[tokio::test]
    async fn complete_imap_stream_is_drained_in_order() {
        let stream = futures::stream::iter([Ok::<_, String>(1), Ok(2), Ok(3)]);

        assert_eq!(
            drain_imap_stream(stream, "UID FETCH").await.unwrap(),
            vec![1, 2, 3]
        );
    }

    #[tokio::test]
    async fn one_malformed_item_does_not_fail_the_rest_of_the_stream() {
        let stream = futures::stream::iter([
            Ok::<_, String>(1),
            Err("unparseable message".to_string()),
            Ok(3),
        ]);

        assert_eq!(
            drain_imap_stream(stream, "UID FETCH").await.unwrap(),
            vec![1, 3]
        );
    }
}
