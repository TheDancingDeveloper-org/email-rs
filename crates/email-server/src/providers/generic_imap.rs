#![allow(dead_code)]

use std::sync::Arc;

use async_imap::types::{Flag, NameAttribute};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use super::{Folder, MailProvider, Message, MessageBody};
use crate::error::{AppError, Result};
use tracing::info;

#[derive(Clone)]
pub struct GenericImapProvider {
    pub account_id: String,
    pub email: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
}

impl GenericImapProvider {
    pub fn new(
        account_id: String,
        email: String,
        password: String,
        imap_host: String,
        imap_port: u16,
    ) -> Self {
        Self {
            account_id,
            email,
            password,
            imap_host,
            imap_port,
        }
    }

    async fn connect(
        &self,
    ) -> Result<async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>> {
        tokio::time::timeout(std::time::Duration::from_secs(30), self.connect_inner())
            .await
            .map_err(|_| AppError::Imap("IMAP connection timed out after 30s".to_string()))?
    }

    async fn connect_inner(
        &self,
    ) -> Result<async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>> {
        info!("connecting to {}:{}", self.imap_host, self.imap_port);

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

        let mut client = async_imap::Client::new(tls_stream);
        client
            .read_response()
            .await
            .ok_or_else(|| AppError::Imap("IMAP server closed during greeting".to_string()))?
            .map_err(|e| AppError::Imap(format!("IMAP greeting read error: {}", e)))?;

        let session = client
            .login(&self.email, &self.password)
            .await
            .map_err(|(e, _)| AppError::Imap(format!("IMAP login failed: {}", e)))?;

        info!("IMAP authenticated successfully");
        Ok(session)
    }
}

fn special_use_for_name(name: &str, attrs: &[NameAttribute<'_>]) -> Option<String> {
    // Prefer RFC 6154 SPECIAL-USE attributes when present.
    for attr in attrs {
        if let NameAttribute::Extension(ext) = attr {
            match ext.to_lowercase().trim_start_matches('\\') {
                "sent" => return Some("sent".to_string()),
                "drafts" => return Some("drafts".to_string()),
                "trash" => return Some("trash".to_string()),
                "junk" => return Some("spam".to_string()),
                _ => {}
            }
        }
    }
    // Fall back to well-known folder names (Outlook, Exchange, generic).
    match name.to_lowercase().as_str() {
        "inbox" => Some("inbox".to_string()),
        "sent" | "sent items" | "sent mail" => Some("sent".to_string()),
        "drafts" => Some("drafts".to_string()),
        "trash" | "deleted items" | "deleted" => Some("trash".to_string()),
        "junk" | "junk email" | "spam" => Some("spam".to_string()),
        _ => None,
    }
}

fn display_name(full_path: &str) -> String {
    full_path
        .split('/')
        .next_back()
        .unwrap_or(full_path)
        .to_string()
}

fn bytes_to_string(bytes: Option<&[u8]>) -> Option<String> {
    bytes.and_then(|b| std::str::from_utf8(b).ok().map(|s| s.to_string()))
}

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

#[async_trait]
impl MailProvider for GenericImapProvider {
    fn provider_id(&self) -> &str {
        "generic_imap"
    }

    async fn authenticate(&mut self) -> Result<()> {
        if self.password.is_empty() {
            return Err(AppError::Auth("no password configured".to_string()));
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

        let names = super::drain_imap_stream(names_stream, "LIST").await?;

        let mut folders = Vec::new();
        for name in &names {
            if name
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::NoSelect))
            {
                continue;
            }

            let full_path = name.name().to_string();
            let display = display_name(&full_path);
            let attrs = name.attributes().to_vec();
            let special_use = special_use_for_name(&display, &attrs);

            folders.push(Folder {
                id: Uuid::new_v4(),
                name: display,
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

        let search_query = if let Some(since_date) = since {
            format!("SINCE {}", since_date.format("%d-%b-%Y"))
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

        let uid_range = uid_set
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let fetch_stream = session
            .uid_fetch(&uid_range, "(UID FLAGS RFC822.SIZE ENVELOPE BODYSTRUCTURE)")
            .await
            .map_err(|e| AppError::Imap(format!("UID FETCH failed: {}", e)))?;

        let fetches = super::drain_imap_stream(fetch_stream, "UID FETCH").await?;

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

            let (subject, from_name, from_email, to_addresses, date, message_id, thread_id) =
                if let Some(env) = fetch.envelope() {
                    let subject = bytes_to_string(env.subject.as_deref());

                    let (from_name, from_email) =
                        if let Some(addrs) = env.from.as_ref().and_then(|v| v.first()) {
                            let name = bytes_to_string(addrs.name.as_deref());
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

                    // Generic IMAP has no universal provider thread ID. Preserve
                    // the RFC parent identity so the sync layer can reconcile it
                    // to the root thread already stored for that message.
                    let thread_id = env
                        .in_reply_to
                        .as_deref()
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .map(|s| {
                            format!(
                                "rfc:{}",
                                s.trim().trim_matches(|c: char| c == '<' || c == '>')
                            )
                        })
                        .filter(|value| value != "rfc:");

                    (
                        subject,
                        from_name,
                        from_email,
                        to_addresses,
                        date,
                        message_id,
                        thread_id,
                    )
                } else {
                    (None, None, None, vec![], None, None, None)
                };

            let preview = fetch.body().and_then(|b| {
                std::str::from_utf8(b)
                    .ok()
                    .map(|s| s.chars().take(200).collect::<String>())
            });

            let attachments = fetch
                .bodystructure()
                .map(super::bodystructure_attachments)
                .unwrap_or_default();
            let thread_id =
                thread_id.or_else(|| message_id.as_deref().map(|value| format!("rfc:{value}")));
            messages.push(Message {
                id: Uuid::new_v4(),
                uid,
                message_id,
                thread_id,
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

        let fetches = super::drain_imap_stream(fetch_stream, "UID FETCH body").await?;

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
        let fetches = super::drain_imap_stream(stream, "attachment UID FETCH").await?;
        let content = fetches
            .into_iter()
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

            let fetches = super::drain_imap_stream(fetch_stream, "UID FETCH batch").await?;

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
        super::drain_imap_stream(stream, "UID STORE").await?;
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
        super::drain_imap_stream(stream, "UID STORE").await?;
        let _ = session.logout().await;
        Ok(())
    }

    async fn delete_message(&self, folder: &str, uid: u32) -> Result<()> {
        let mut session = self.connect().await?;
        session
            .select(folder)
            .await
            .map_err(|e| AppError::Imap(format!("SELECT failed: {}", e)))?;
        let store_stream = session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
            .await
            .map_err(|e| AppError::Imap(format!("UID STORE \\Deleted failed: {}", e)))?;
        super::drain_imap_stream(store_stream, "UID STORE \\Deleted").await?;
        let expunged = session
            .expunge()
            .await
            .map_err(|e| AppError::Imap(format!("EXPUNGE failed: {}", e)))?;
        super::drain_imap_stream(expunged, "EXPUNGE").await?;
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
        super::drain_imap_stream(store_stream, "UID STORE \\Deleted").await?;
        let expunged = session
            .expunge()
            .await
            .map_err(|e| AppError::Imap(format!("EXPUNGE failed: {}", e)))?;
        super::drain_imap_stream(expunged, "EXPUNGE").await?;
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
        super::drain_imap_stream(stream, "UID STORE").await?;
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
}
