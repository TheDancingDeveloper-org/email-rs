#![allow(dead_code)]

use std::{collections::HashMap, sync::Arc};

use async_imap::types::{Flag, NameAttribute};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use super::{Folder, MailProvider, Message, MessageBody};
use crate::error::{AppError, Result};
use tracing::{info, warn};

fn gmail_thread_identity(
    attributes: &[async_imap::imap_proto::types::AttributeValue<'_>],
) -> Option<(u32, u64)> {
    use async_imap::imap_proto::types::AttributeValue;

    let mut uid = None;
    let mut thread_id = None;
    for attribute in attributes {
        match attribute {
            AttributeValue::Uid(value) => uid = Some(*value),
            AttributeValue::GmailThrId(value) => thread_id = Some(*value),
            _ => {}
        }
    }
    uid.zip(thread_id)
}

// ── XOAUTH2 SASL authenticator ────────────────────────────────────────────────

struct XOAuth2 {
    /// Raw (not base64-encoded) SASL XOAUTH2 string.
    /// async-imap base64-encodes the bytes returned by process() before sending,
    /// so we must NOT pre-encode here.
    raw: String,
}

impl XOAuth2 {
    fn new(email: &str, access_token: &str) -> Self {
        let raw = format!("user={}\x01auth=Bearer {}\x01\x01", email, access_token);
        Self { raw }
    }
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = Vec<u8>;

    fn process(&mut self, challenge: &[u8]) -> Self::Response {
        if challenge.is_empty() {
            // Initial exchange: return raw bytes; async-imap will base64-encode them.
            self.raw.as_bytes().to_vec()
        } else {
            // Non-empty challenge = Gmail sent an error JSON (token rejected).
            // Respond with empty bytes to abort the SASL exchange.
            if let Ok(s) = std::str::from_utf8(challenge) {
                warn!("XOAUTH2 auth error from Gmail: {}", s);
            }
            Vec::new()
        }
    }
}

// ── GmailProvider ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct GmailProvider {
    pub account_id: String,
    pub email: String,
    pub access_token: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub provider_type: String,
}

impl GmailProvider {
    pub fn new(account_id: String, email: String, access_token: String) -> Self {
        Self {
            account_id,
            email,
            access_token,
            imap_host: "imap.gmail.com".to_string(),
            imap_port: 993,
            provider_type: "gmail".to_string(),
        }
    }

    pub fn new_outlook(account_id: String, email: String, access_token: String) -> Self {
        Self {
            account_id,
            email,
            access_token,
            imap_host: "outlook.office365.com".to_string(),
            imap_port: 993,
            provider_type: "microsoft365".to_string(),
        }
    }

    /// Establish an authenticated IMAP session over TLS, with a 30-second timeout.
    async fn connect(
        &self,
    ) -> Result<async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>> {
        tokio::time::timeout(std::time::Duration::from_secs(30), self.connect_inner())
            .await
            .map_err(|_| AppError::Imap("IMAP connection timed out after 30s".to_string()))?
    }

    /// Inner connection logic (no timeout; called by `connect`).
    async fn connect_inner(
        &self,
    ) -> Result<async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>> {
        info!("connecting to {}:{}", self.imap_host, self.imap_port);

        // Build rustls config with Mozilla trust roots.
        let mut root_store = RootCertStore::empty();
        root_store.roots = webpki_roots::TLS_SERVER_ROOTS.to_vec();
        let tls_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(tls_config));

        let tcp = tokio::net::TcpStream::connect((self.imap_host.as_str(), self.imap_port))
            .await
            .map_err(|e| AppError::Imap(format!("TCP connect failed: {}", e)))?;

        let domain = rustls::pki_types::ServerName::try_from(self.imap_host.as_str())
            .map_err(|e| AppError::Imap(format!("invalid hostname: {}", e)))?
            .to_owned();

        let tls_stream = connector
            .connect(domain, tcp)
            .await
            .map_err(|e| AppError::Imap(format!("TLS handshake failed: {}", e)))?;

        info!("TLS connected, authenticating with XOAUTH2");

        let mut client = async_imap::Client::new(tls_stream);

        // async-imap does NOT auto-read the server greeting — we must consume it
        // before issuing any command, otherwise the greeting line shifts the
        // response parsing and the AUTHENTICATE exchange deadlocks.
        client
            .read_response()
            .await
            .ok_or_else(|| AppError::Imap("IMAP server closed during greeting".to_string()))?
            .map_err(|e| AppError::Imap(format!("IMAP greeting read error: {}", e)))?;

        let authenticator = XOAuth2::new(&self.email, &self.access_token);

        let session = client
            .authenticate("XOAUTH2", authenticator)
            .await
            .map_err(|(e, _)| AppError::Imap(format!("IMAP auth failed: {}", e)))?;

        info!("IMAP authenticated successfully");

        Ok(session)
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Map a Gmail IMAP mailbox name to a `special_use` slug.
/// Returns `None` for mailboxes that should be skipped (e.g., All Mail).
fn special_use_for(name: &str) -> Option<Option<String>> {
    match name {
        "INBOX" => Some(Some("inbox".to_string())),
        "[Gmail]/Sent Mail" => Some(Some("sent".to_string())),
        "[Gmail]/Drafts" => Some(Some("drafts".to_string())),
        "[Gmail]/Trash" => Some(Some("trash".to_string())),
        "[Gmail]/Spam" => Some(Some("spam".to_string())),
        // Skip virtual / aggregate folders
        "[Gmail]/All Mail" | "[Gmail]/Important" | "[Gmail]/Starred" => None,
        _ => Some(None),
    }
}

fn generic_special_use(name: &str, attrs: &[NameAttribute<'_>]) -> Option<String> {
    for attr in attrs {
        if let NameAttribute::Extension(extension) = attr {
            match extension.to_ascii_lowercase().trim_start_matches('\\') {
                "sent" => return Some("sent".to_string()),
                "drafts" => return Some("drafts".to_string()),
                "trash" => return Some("trash".to_string()),
                "junk" => return Some("spam".to_string()),
                "archive" => return Some("archive".to_string()),
                _ => {}
            }
        }
    }
    match name.to_ascii_lowercase().as_str() {
        "inbox" => Some("inbox".to_string()),
        "sent" | "sent items" | "sent mail" => Some("sent".to_string()),
        "drafts" => Some("drafts".to_string()),
        "trash" | "deleted" | "deleted items" => Some("trash".to_string()),
        "junk" | "junk email" | "spam" => Some("spam".to_string()),
        "archive" => Some("archive".to_string()),
        _ => None,
    }
}

/// Derive a human-readable folder name from the IMAP full path.
fn display_name(full_path: &str) -> String {
    let without_prefix = full_path.strip_prefix("[Gmail]/").unwrap_or(full_path);
    without_prefix
        .split('/')
        .next_back()
        .unwrap_or(without_prefix)
        .to_string()
}

/// Decode optional raw bytes from an IMAP envelope field into a UTF-8 string.
fn bytes_to_string(bytes: Option<&[u8]>) -> Option<String> {
    bytes.and_then(|b| std::str::from_utf8(b).ok().map(|s| s.to_string()))
}

/// Format an IMAP Address as "Name <email>" or just "email".
fn format_address(addr: &async_imap::imap_proto::types::Address<'_>) -> String {
    let mailbox = addr
        .mailbox
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let host = addr
        .host
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let name = addr
        .name
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok());

    if let Some(n) = name {
        format!("{} <{}@{}>", n, mailbox, host)
    } else {
        format!("{}@{}", mailbox, host)
    }
}

// ── MailProvider implementation ───────────────────────────────────────────────

#[async_trait]
impl MailProvider for GmailProvider {
    fn provider_id(&self) -> &str {
        &self.provider_type
    }

    async fn authenticate(&mut self) -> Result<()> {
        // Authentication is validated at connection time; here we just verify
        // the token is non-empty.
        if self.access_token.is_empty() {
            return Err(AppError::Auth("no access token available".to_string()));
        }
        Ok(())
    }

    async fn list_folders(&self) -> Result<Vec<Folder>> {
        info!("listing folders for account {}", self.account_id);
        let mut session = self.connect().await?;

        let names_stream = session
            .list(None, Some("*"))
            .await
            .map_err(|e| AppError::Imap(format!("LIST failed: {}", e)))?;

        let names: Vec<_> = names_stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let mut folders = Vec::new();
        for name in &names {
            // Skip non-selectable mailboxes.
            if name
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::NoSelect))
            {
                continue;
            }

            let full_path = name.name().to_string();

            let special_use = if self.provider_type == "gmail" {
                match special_use_for(&full_path) {
                    None => continue,
                    Some(su) => su,
                }
            } else {
                generic_special_use(&full_path, name.attributes())
            };

            folders.push(Folder {
                id: Uuid::new_v4(),
                name: display_name(&full_path),
                full_path,
                special_use,
                unread_count: 0,
                total_count: 0,
            });
        }

        let _ = session.logout().await;
        Ok(folders)
    }

    async fn folder_uid_validity(&self, folder: &str) -> Result<Option<u32>> {
        let mut session = self.connect().await?;
        let mailbox = session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;
        let uid_validity = mailbox.uid_validity;
        let _ = session.logout().await;
        Ok(uid_validity)
    }

    async fn fetch_messages(
        &self,
        folder: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>> {
        info!("fetching messages for folder {}", folder);
        let mut session = self.connect().await?;

        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;

        // Build UID SEARCH query.
        let search_query = if let Some(since_date) = since {
            // IMAP SINCE uses DD-Mon-YYYY format.
            let formatted = since_date.format("%d-%b-%Y").to_string();
            format!("SINCE {}", formatted)
        } else {
            "ALL".to_string()
        };

        let uid_set = session
            .uid_search(&search_query)
            .await
            .map_err(|e| AppError::Imap(format!("UID SEARCH failed: {}", e)))?;

        if uid_set.is_empty() {
            let _ = session.logout().await;
            return Ok(vec![]);
        }

        // Build a compact sequence set from the UIDs.
        let uids: Vec<String> = uid_set.iter().map(|u| u.to_string()).collect();
        let uid_range = uids.join(",");

        // Gmail's stable thread identifier is only available through its IMAP
        // extension. Fetch it separately so Outlook (which shares this provider
        // implementation for XOAUTH2) never receives an unsupported attribute.
        let mut thread_ids = HashMap::new();
        if self.provider_type == "gmail" {
            use async_imap::imap_proto::{types::Response, Status};

            let command_id = session
                .run_command(format!("UID FETCH {uid_range} (UID X-GM-THRID)"))
                .await
                .map_err(|e| AppError::Imap(format!("Gmail thread UID FETCH failed: {e}")))?;
            loop {
                let response = session
                    .read_response()
                    .await
                    .ok_or_else(|| {
                        AppError::Imap(
                            "Gmail closed the connection while fetching thread IDs".to_string(),
                        )
                    })?
                    .map_err(|e| AppError::Imap(format!("Gmail thread response failed: {e}")))?;
                match response.parsed() {
                    Response::Fetch(_, attributes) => {
                        if let Some((uid, thread_id)) = gmail_thread_identity(attributes) {
                            thread_ids
                                .insert(uid, format!("gmail:{}:{thread_id}", self.account_id));
                        }
                    }
                    Response::Done { tag, status, .. } if tag == &command_id => {
                        if *status != Status::Ok {
                            return Err(AppError::Imap(format!(
                                "Gmail thread UID FETCH completed with {status:?}"
                            )));
                        }
                        break;
                    }
                    _ => {}
                }
            }
        }

        let fetch_stream = session
            .uid_fetch(&uid_range, "(UID FLAGS RFC822.SIZE ENVELOPE BODYSTRUCTURE)")
            .await
            .map_err(|e| AppError::Imap(format!("UID FETCH failed: {}", e)))?;

        let fetches: Vec<_> = fetch_stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let mut messages = Vec::new();
        for fetch in &fetches {
            let uid = match fetch.uid {
                Some(u) => u,
                None => continue,
            };

            let flags: Vec<Flag<'_>> = fetch.flags().collect();
            let is_read = flags.iter().any(|f| matches!(f, Flag::Seen));
            let is_flagged = flags.iter().any(|f| matches!(f, Flag::Flagged));
            let is_draft = flags.iter().any(|f| matches!(f, Flag::Draft));

            let (subject, from_name, from_email, to_addresses, date, message_id) =
                if let Some(env) = fetch.envelope() {
                    let subject = env
                        .subject
                        .as_deref()
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .map(|s| s.to_string());

                    let (from_name, from_email) =
                        if let Some(addrs) = env.from.as_ref().and_then(|v| v.first()) {
                            let name = addrs
                                .name
                                .as_deref()
                                .and_then(|b| std::str::from_utf8(b).ok())
                                .map(|s| s.to_string());
                            let mailbox = addrs
                                .mailbox
                                .as_deref()
                                .and_then(|b| std::str::from_utf8(b).ok())
                                .unwrap_or("");
                            let host = addrs
                                .host
                                .as_deref()
                                .and_then(|b| std::str::from_utf8(b).ok())
                                .unwrap_or("");
                            let email = if host.is_empty() {
                                mailbox.to_string()
                            } else {
                                format!("{}@{}", mailbox, host)
                            };
                            (name, Some(email))
                        } else {
                            (None, None)
                        };

                    let to_addresses: Vec<String> = env
                        .to
                        .as_ref()
                        .map(|addrs| addrs.iter().map(|a| format_address(a)).collect())
                        .unwrap_or_default();

                    let date = env
                        .date
                        .as_deref()
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .and_then(|s| {
                            chrono::DateTime::parse_from_rfc2822(s)
                                .ok()
                                .map(|dt| dt.with_timezone(&Utc))
                        });

                    let message_id = env
                        .message_id
                        .as_deref()
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .map(|s| s.trim_matches(|c: char| c == '<' || c == '>').to_string());

                    (
                        subject,
                        from_name,
                        from_email,
                        to_addresses,
                        date,
                        message_id,
                    )
                } else {
                    (None, None, None, vec![], None, None)
                };

            // Build a short preview from the message body bytes if available.
            let preview = fetch.body().and_then(|b| {
                std::str::from_utf8(b)
                    .ok()
                    .map(|s| s.chars().take(200).collect::<String>())
            });

            let attachments = fetch
                .bodystructure()
                .map(super::bodystructure_attachments)
                .unwrap_or_default();
            messages.push(Message {
                id: Uuid::new_v4(),
                uid,
                message_id,
                thread_id: thread_ids.get(&uid).cloned(),
                subject,
                from_name,
                from_email,
                to: to_addresses,
                cc: vec![],
                date,
                is_read,
                is_flagged,
                is_draft,
                has_attachments: !attachments.is_empty(),
                attachments,
                preview,
            });
        }

        let _ = session.logout().await;
        Ok(messages)
    }

    async fn list_message_uids(&self, folder: &str) -> Result<Vec<u32>> {
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {e}")))?;
        let uids = session
            .uid_search("ALL")
            .await
            .map_err(|e| AppError::Imap(format!("UID SEARCH ALL failed: {e}")))?;
        let _ = session.logout().await;
        Ok(uids.into_iter().collect())
    }

    async fn fetch_message_body(&self, folder: &str, uid: u32) -> Result<MessageBody> {
        let mut session = self.connect().await?;

        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;

        let fetch_stream = session
            .uid_fetch(uid.to_string(), "RFC822")
            .await
            .map_err(|e| AppError::Imap(format!("UID FETCH body failed: {}", e)))?;

        let fetches: Vec<_> = fetch_stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let fetch = fetches
            .first()
            .ok_or_else(|| AppError::Imap(format!("no message found with uid {}", uid)))?;

        let raw = fetch
            .body()
            .ok_or_else(|| AppError::Imap("message had no RFC822 body".to_string()))?;

        let body = super::parse_message_body(uid, raw)?;
        let _ = session.logout().await;
        Ok(body)
    }

    async fn fetch_attachment(&self, folder: &str, uid: u32, part_id: &str) -> Result<Vec<u8>> {
        if !super::valid_imap_part_id(part_id) {
            return Err(AppError::Imap("invalid MIME part identifier".into()));
        }
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {e}")))?;
        let stream = session
            .uid_fetch(uid.to_string(), format!("(BODY.PEEK[{part_id}])"))
            .await
            .map_err(|e| AppError::Imap(format!("attachment UID FETCH failed: {e}")))?;
        let fetches = stream.collect::<Vec<_>>().await;
        let content = fetches
            .into_iter()
            .filter_map(|item| item.ok())
            .find_map(|fetch| fetch.body().map(ToOwned::to_owned));
        let _ = session.logout().await;
        content
            .ok_or_else(|| AppError::NotFound(format!("attachment part {part_id} is unavailable")))
    }

    async fn fetch_bodies_batch(
        &self,
        folder: &str,
        uids: &[u32],
    ) -> Result<Vec<(u32, MessageBody)>> {
        if uids.is_empty() {
            return Ok(vec![]);
        }
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;

        let mut results = Vec::new();

        // Chunk UIDs to keep individual IMAP commands reasonable in size.
        for chunk in uids.chunks(500) {
            let uid_range = chunk
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let fetch_stream = session
                .uid_fetch(&uid_range, "RFC822")
                .await
                .map_err(|e| AppError::Imap(format!("UID FETCH batch failed: {}", e)))?;

            let fetches: Vec<_> = fetch_stream
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

            for fetch in &fetches {
                let uid = match fetch.uid {
                    Some(u) => u,
                    None => continue,
                };
                let raw = match fetch.body() {
                    Some(b) => b,
                    None => continue,
                };
                let Ok(body) = super::parse_message_body(uid, raw) else {
                    continue;
                };
                results.push((uid, body));
            }
        }

        let _ = session.logout().await;
        Ok(results)
    }

    async fn mark_seen(&self, folder: &str, uid: u32) -> Result<()> {
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;
        let stream = session
            .uid_store(uid.to_string(), "+FLAGS (\\Seen)")
            .await
            .map_err(|e| AppError::Imap(format!("UID STORE failed: {}", e)))?;
        stream.collect::<Vec<_>>().await;
        let _ = session.logout().await;
        Ok(())
    }

    async fn mark_unseen(&self, folder: &str, uid: u32) -> Result<()> {
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;
        let stream = session
            .uid_store(uid.to_string(), "-FLAGS (\\Seen)")
            .await
            .map_err(|e| AppError::Imap(format!("UID STORE failed: {}", e)))?;
        stream.collect::<Vec<_>>().await;
        let _ = session.logout().await;
        Ok(())
    }

    async fn delete_message(&self, folder: &str, uid: u32) -> Result<()> {
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;
        // Gmail: COPY to Trash, then \Deleted + EXPUNGE from source.
        session
            .uid_copy(uid.to_string(), "[Gmail]/Trash")
            .await
            .map_err(|e| AppError::Imap(format!("UID COPY to Trash failed: {}", e)))?;
        let store_stream = session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
            .await
            .map_err(|e| AppError::Imap(format!("UID STORE \\Deleted failed: {}", e)))?;
        store_stream.collect::<Vec<_>>().await;
        session
            .expunge()
            .await
            .map_err(|e| AppError::Imap(format!("EXPUNGE failed: {}", e)))?
            .collect::<Vec<_>>()
            .await;
        let _ = session.logout().await;
        Ok(())
    }

    async fn move_message(&self, src_folder: &str, uid: u32, dest_folder: &str) -> Result<()> {
        let mut session = self.connect().await?;
        session
            .select(src_folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;
        session
            .uid_copy(uid.to_string(), dest_folder)
            .await
            .map_err(|e| AppError::Imap(format!("UID COPY failed: {}", e)))?;
        let store_stream = session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
            .await
            .map_err(|e| AppError::Imap(format!("UID STORE \\Deleted failed: {}", e)))?;
        store_stream.collect::<Vec<_>>().await;
        session
            .expunge()
            .await
            .map_err(|e| AppError::Imap(format!("EXPUNGE failed: {}", e)))?
            .collect::<Vec<_>>()
            .await;
        let _ = session.logout().await;
        Ok(())
    }

    async fn set_flagged(&self, folder: &str, uid: u32, flagged: bool) -> Result<()> {
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;
        let flag_cmd = if flagged {
            "+FLAGS (\\Flagged)"
        } else {
            "-FLAGS (\\Flagged)"
        };
        let stream = session
            .uid_store(uid.to_string(), flag_cmd)
            .await
            .map_err(|e| AppError::Imap(format!("UID STORE failed: {}", e)))?;
        stream.collect::<Vec<_>>().await;
        let _ = session.logout().await;
        Ok(())
    }

    async fn append_draft(
        &self,
        folder: &str,
        message_id: &str,
        raw_message: &[u8],
    ) -> Result<u32> {
        let mut session = self.connect().await?;
        session
            .append(folder, Some("(\\Draft)"), None, raw_message)
            .await
            .map_err(|e| AppError::Imap(format!("APPEND draft failed: {e}")))?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT drafts failed: {e}")))?;
        let uids = session
            .uid_search(format!("HEADER Message-ID \"<{message_id}>\""))
            .await
            .map_err(|e| AppError::Imap(format!("draft UID lookup failed: {e}")))?;
        let uid = uids.into_iter().max().ok_or_else(|| {
            AppError::Imap("provider did not return the appended draft UID".into())
        })?;
        let _ = session.logout().await;
        Ok(uid)
    }

    async fn delete_draft(&self, folder: &str, uid: u32) -> Result<()> {
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {e}")))?;
        let stream = session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
            .await
            .map_err(|e| AppError::Imap(format!("UID STORE \\Deleted failed: {e}")))?;
        stream.collect::<Vec<_>>().await;
        session
            .expunge()
            .await
            .map_err(|e| AppError::Imap(format!("EXPUNGE failed: {e}")))?
            .collect::<Vec<_>>()
            .await;
        let _ = session.logout().await;
        Ok(())
    }
}

#[cfg(test)]
mod thread_tests {
    use super::*;
    use async_imap::imap_proto::types::AttributeValue;

    #[test]
    fn extracts_stable_gmail_thread_identity() {
        let attributes = vec![
            AttributeValue::Uid(42),
            AttributeValue::GmailThrId(9_876_543),
        ];
        assert_eq!(gmail_thread_identity(&attributes), Some((42, 9_876_543)));
        assert_eq!(gmail_thread_identity(&[AttributeValue::Uid(42)]), None);
    }
}
