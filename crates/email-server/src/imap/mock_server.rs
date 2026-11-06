//! In-process mock IMAP server for end-to-end sync tests (email-rs #75).
//!
//! It speaks enough IMAP over TLS (CAPABILITY, LOGIN, LIST, SELECT, NOOP,
//! UID SEARCH, UID FETCH, CLOSE, LOGOUT) for `GenericImapProvider` to list,
//! select, search and fetch from it, so `ImapSyncEngine` can run against real
//! wire traffic. Mailbox state is shared and mutable, so a test can add or
//! remove messages and change UIDVALIDITY between syncs.
//!
//! The server presents a freshly generated self-signed certificate for
//! `localhost` and registers it in `GenericImapProvider`'s test-only root list,
//! so the provider connects with its normal TLS verification.

use std::sync::{Arc, Mutex};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub const HOST: &str = "localhost";
pub const USER: &str = "mock@example.test";
pub const PASSWORD: &str = "mock-password";

#[derive(Clone, Debug)]
pub struct MockMessage {
    pub uid: u32,
    pub message_id: String,
    pub subject: String,
    pub from_name: String,
    pub from_email: String,
    pub to_email: String,
    pub in_reply_to: Option<String>,
    pub date: chrono::DateTime<chrono::Utc>,
    pub seen: bool,
    pub flagged: bool,
    pub body: String,
}

impl MockMessage {
    pub fn new(uid: u32, subject: &str) -> Self {
        Self {
            uid,
            message_id: format!("m{uid}@mock.example.test"),
            subject: subject.to_string(),
            from_name: "Sender".into(),
            from_email: "sender@example.test".into(),
            to_email: USER.into(),
            in_reply_to: None,
            date: chrono::Utc::now(),
            seen: false,
            flagged: false,
            body: format!("Body of message {uid}."),
        }
    }

    /// Deterministic message for `uid`, used by the lazy perf mailbox: the same
    /// UID always yields the same headers, date and body, so runs are
    /// comparable and nothing needs to be committed as data.
    pub fn generated(uid: u32) -> Self {
        let base = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let mut message = Self::new(uid, &format!("perf message {uid}"));
        message.from_name = format!("Sender {}", uid % 97);
        message.from_email = format!("sender{}@example.test", uid % 97);
        message.date = base + chrono::Duration::minutes(i64::from(uid));
        message.seen = uid.is_multiple_of(3);
        message.flagged = uid.is_multiple_of(50);
        message.in_reply_to =
            (uid.is_multiple_of(5) && uid > 5).then(|| format!("m{}@mock.example.test", uid - 5));
        message.body = format!(
            "Deterministic body of perf message {uid}.\r\nLorem ipsum dolor sit amet, line {}.",
            uid % 1000
        );
        message
    }

    fn raw(&self) -> String {
        let mut raw = format!(
            "Date: {}\r\nFrom: {} <{}>\r\nTo: {}\r\nSubject: {}\r\nMessage-ID: <{}>\r\n",
            self.date.to_rfc2822(),
            self.from_name,
            self.from_email,
            self.to_email,
            self.subject,
            self.message_id
        );
        if let Some(parent) = &self.in_reply_to {
            raw.push_str(&format!(
                "In-Reply-To: <{parent}>\r\nReferences: <{parent}>\r\n"
            ));
        }
        raw.push_str("Content-Type: text/plain; charset=utf-8\r\n\r\n");
        raw.push_str(&self.body);
        raw.push_str("\r\n");
        raw
    }

    fn flags(&self) -> String {
        let mut flags = Vec::new();
        if self.seen {
            flags.push("\\Seen");
        }
        if self.flagged {
            flags.push("\\Flagged");
        }
        flags.join(" ")
    }
}

#[derive(Clone, Debug)]
pub struct MockMailbox {
    pub name: String,
    pub uid_validity: u32,
    pub messages: Vec<MockMessage>,
    /// Inclusive UID range of messages generated on demand by
    /// [`MockMessage::generated`] rather than stored. They come after
    /// `messages`, so a 100k-message mailbox costs no memory until fetched.
    pub lazy: Option<(u32, u32)>,
}

impl MockMailbox {
    fn exists(&self) -> usize {
        self.messages.len() + self.lazy.map_or(0, |(a, b)| (b - a + 1) as usize)
    }

    fn max_uid(&self) -> u32 {
        let stored = self.messages.iter().map(|m| m.uid).max().unwrap_or(0);
        stored.max(self.lazy.map_or(0, |(_, b)| b))
    }
}

#[derive(Default)]
struct State {
    mailboxes: Vec<MockMailbox>,
    /// Every command line received, `LOGIN` redacted, for assertions.
    commands: Vec<String>,
}

pub struct MockImapServer {
    port: u16,
    state: Arc<Mutex<State>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for MockImapServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl MockImapServer {
    /// Start a server on an ephemeral loopback port with an empty `INBOX`.
    pub async fn start() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let cert = rcgen::generate_simple_self_signed(vec![HOST.to_string()]).unwrap();
        let cert_der = CertificateDer::from(cert.cert.der().to_vec());
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()));
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        crate::providers::generic_imap::TEST_EXTRA_ROOTS
            .lock()
            .unwrap()
            .push(cert_der);

        let state = Arc::new(Mutex::new(State {
            mailboxes: vec![MockMailbox {
                name: "INBOX".into(),
                uid_validity: 1,
                messages: Vec::new(),
                lazy: None,
            }],
            commands: Vec::new(),
        }));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                let state = task_state.clone();
                tokio::spawn(async move {
                    if let Ok(tls) = acceptor.accept(tcp).await {
                        let _ = serve(tls, state).await;
                    }
                });
            }
        });
        Self { port, state, task }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// A provider pointed at this server with the fixture credentials.
    pub fn provider(
        &self,
        account_id: &str,
    ) -> crate::providers::generic_imap::GenericImapProvider {
        crate::providers::generic_imap::GenericImapProvider::new(
            account_id.into(),
            USER.into(),
            PASSWORD.into(),
            HOST.into(),
            self.port,
        )
    }

    pub fn add_mailbox(&self, name: &str, uid_validity: u32) {
        self.state.lock().unwrap().mailboxes.push(MockMailbox {
            name: name.into(),
            uid_validity,
            messages: Vec::new(),
            lazy: None,
        });
    }

    pub fn add_message(&self, mailbox: &str, message: MockMessage) {
        self.with_mailbox(mailbox, |mb| mb.messages.push(message));
    }

    /// Add `count` generated messages with consecutive UIDs after the highest
    /// existing one. Cheap enough to scale towards the 100k-message perf
    /// fixture, though syncing that many is not part of the default tests.
    pub fn populate(&self, mailbox: &str, count: u32) {
        self.with_mailbox(mailbox, |mb| {
            let start = mb.messages.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
            for uid in start..start + count {
                mb.messages
                    .push(MockMessage::new(uid, &format!("generated {uid}")));
            }
        });
    }

    /// Append `count` lazily generated, deterministic messages with
    /// consecutive UIDs after the highest existing one. Nothing is stored per
    /// message, so this scales to the 100k-message perf fixture (see
    /// `imap::perf_tests`). Lazy messages cannot be individually removed.
    pub fn populate_lazy(&self, mailbox: &str, count: u32) {
        self.with_mailbox(mailbox, |mb| {
            let next = mb.max_uid() + 1;
            mb.lazy = match mb.lazy {
                Some((a, b)) if b + 1 == next => Some((a, b + count)),
                Some(_) => panic!("lazy range must stay contiguous"),
                None => Some((next, next + count - 1)),
            };
        });
    }

    pub fn remove_message(&self, mailbox: &str, uid: u32) {
        self.with_mailbox(mailbox, |mb| mb.messages.retain(|m| m.uid != uid));
    }

    /// Change UIDVALIDITY and replace the mailbox contents, as a server does
    /// when it renumbers a mailbox.
    pub fn reset_mailbox(&self, mailbox: &str, uid_validity: u32, messages: Vec<MockMessage>) {
        self.with_mailbox(mailbox, |mb| {
            mb.uid_validity = uid_validity;
            mb.messages = messages;
            mb.lazy = None;
        });
    }

    /// Commands received so far (`LOGIN` arguments redacted).
    pub fn commands(&self) -> Vec<String> {
        self.state.lock().unwrap().commands.clone()
    }

    fn with_mailbox<R>(&self, name: &str, f: impl FnOnce(&mut MockMailbox) -> R) -> R {
        let mut state = self.state.lock().unwrap();
        let mailbox = state
            .mailboxes
            .iter_mut()
            .find(|mb| mb.name == name)
            .unwrap_or_else(|| panic!("no mock mailbox {name}"));
        f(mailbox)
    }
}

async fn serve<S>(stream: S, state: Arc<Mutex<State>>) -> std::io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut read = BufReader::new(read);
    write
        .write_all(b"* OK [CAPABILITY IMAP4rev1] mock ready\r\n")
        .await?;
    let mut authenticated = false;
    let mut selected: Option<String> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if read.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let line = line.trim_end();
        let mut parts = line.splitn(3, ' ');
        let tag = parts.next().unwrap_or("*");
        let command = parts.next().unwrap_or("").to_ascii_uppercase();
        let args = parts.next().unwrap_or("");
        {
            let logged = if command == "LOGIN" {
                format!("{tag} LOGIN")
            } else {
                line.to_string()
            };
            state.lock().unwrap().commands.push(logged);
        }
        let mut out: Vec<u8> = Vec::new();
        match command.as_str() {
            "CAPABILITY" => {
                out.extend_from_slice(b"* CAPABILITY IMAP4rev1\r\n");
                ok(&mut out, tag, "CAPABILITY completed");
            }
            "NOOP" => ok(&mut out, tag, "NOOP completed"),
            "LOGIN" => {
                let creds: Vec<String> = args.split(' ').map(unquote).collect();
                if creds == [USER, PASSWORD] {
                    authenticated = true;
                    ok(&mut out, tag, "LOGIN completed");
                } else {
                    out.extend_from_slice(
                        format!("{tag} NO [AUTHENTICATIONFAILED] bad credentials\r\n").as_bytes(),
                    );
                }
            }
            "LOGOUT" => {
                out.extend_from_slice(b"* BYE bye\r\n");
                ok(&mut out, tag, "LOGOUT completed");
                write.write_all(&out).await?;
                return Ok(());
            }
            _ if !authenticated => {
                out.extend_from_slice(format!("{tag} NO not authenticated\r\n").as_bytes());
            }
            "LIST" => {
                for mb in &state.lock().unwrap().mailboxes {
                    out.extend_from_slice(
                        format!("* LIST (\\HasNoChildren) \"/\" {}\r\n", quote(&mb.name))
                            .as_bytes(),
                    );
                }
                ok(&mut out, tag, "LIST completed");
            }
            "SELECT" | "EXAMINE" => {
                let name = unquote(args);
                let state = state.lock().unwrap();
                match state.mailboxes.iter().find(|mb| mb.name == name) {
                    Some(mb) => {
                        let next = mb.max_uid() + 1;
                        out.extend_from_slice(
                            format!(
                                "* {} EXISTS\r\n* 0 RECENT\r\n* FLAGS (\\Seen \\Flagged \\Draft)\r\n* OK [UIDVALIDITY {}] ok\r\n* OK [UIDNEXT {}] ok\r\n",
                                mb.exists(), mb.uid_validity, next
                            )
                            .as_bytes(),
                        );
                        selected = Some(name);
                        ok(&mut out, tag, "[READ-WRITE] SELECT completed");
                    }
                    None => {
                        out.extend_from_slice(format!("{tag} NO no such mailbox\r\n").as_bytes())
                    }
                }
            }
            "CLOSE" => {
                selected = None;
                ok(&mut out, tag, "CLOSE completed");
            }
            "UID" => {
                let Some(mailbox) = selected.clone() else {
                    out.extend_from_slice(format!("{tag} BAD no mailbox selected\r\n").as_bytes());
                    write.write_all(&out).await?;
                    continue;
                };
                let mut sub = args.splitn(2, ' ');
                let sub_command = sub.next().unwrap_or("").to_ascii_uppercase();
                let rest = sub.next().unwrap_or("");
                let state = state.lock().unwrap();
                let mb = state
                    .mailboxes
                    .iter()
                    .find(|mb| mb.name == mailbox)
                    .unwrap();
                match sub_command.as_str() {
                    "SEARCH" => {
                        let uids = search(mb, rest);
                        let list: String = uids.iter().map(|u| format!(" {u}")).collect();
                        out.extend_from_slice(format!("* SEARCH{list}\r\n").as_bytes());
                        ok(&mut out, tag, "UID SEARCH completed");
                    }
                    "FETCH" => {
                        let mut fetch = rest.splitn(2, ' ');
                        let set = fetch.next().unwrap_or("");
                        let items = fetch.next().unwrap_or("").to_ascii_uppercase();
                        let wanted: std::collections::BTreeSet<u32> =
                            parse_set(set).into_iter().collect();
                        for (index, message) in mb.messages.iter().enumerate() {
                            if wanted.contains(&message.uid) {
                                fetch_response(&mut out, index + 1, message, &items);
                            }
                        }
                        if let Some((first, last)) = mb.lazy {
                            for &uid in wanted.range(first..=last) {
                                let seq = mb.messages.len() + (uid - first) as usize + 1;
                                fetch_response(&mut out, seq, &MockMessage::generated(uid), &items);
                            }
                        }
                        ok(&mut out, tag, "UID FETCH completed");
                    }
                    _ => out.extend_from_slice(
                        format!("{tag} BAD unsupported UID command\r\n").as_bytes(),
                    ),
                }
            }
            _ => out.extend_from_slice(format!("{tag} BAD unsupported command\r\n").as_bytes()),
        }
        write.write_all(&out).await?;
    }
}

fn ok(out: &mut Vec<u8>, tag: &str, text: &str) {
    out.extend_from_slice(format!("{tag} OK {text}\r\n").as_bytes());
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(inner) => inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        None => value.to_string(),
    }
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// IMAP string: quoted when plain ASCII, otherwise a literal. `None` is NIL.
fn nstring(value: Option<&str>) -> String {
    match value {
        None => "NIL".into(),
        Some(v) if v.is_ascii() && !v.contains(['\r', '\n']) => quote(v),
        Some(v) => format!("{{{}}}\r\n{}", v.len(), v),
    }
}

fn parse_set(set: &str) -> Vec<u32> {
    let mut uids = Vec::new();
    for part in set.split(',') {
        match part.split_once(':') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (a.parse().unwrap_or(1), b.parse().unwrap_or(u32::MAX));
                uids.extend((a..=b.min(a.saturating_add(1_000_000))).collect::<Vec<_>>());
            }
            None => uids.extend(part.parse::<u32>()),
        }
    }
    uids
}

fn search(mb: &MockMailbox, criteria: &str) -> Vec<u32> {
    let upper = criteria.trim().to_ascii_uppercase();
    let matches: Box<dyn Fn(&MockMessage) -> bool> =
        if let Some(date) = upper.strip_prefix("SINCE ") {
            let since = chrono::NaiveDate::parse_from_str(date.trim(), "%d-%b-%Y").ok();
            Box::new(move |m| since.is_none_or(|s| m.date.date_naive() >= s))
        } else if let Some(text) = upper.strip_prefix("TEXT ") {
            let needle = unquote(text).to_ascii_lowercase();
            Box::new(move |m| m.raw().to_ascii_lowercase().contains(&needle))
        } else {
            // Match-all: list lazy UIDs without generating their messages.
            let mut uids: Vec<u32> = mb.messages.iter().map(|m| m.uid).collect();
            uids.extend(mb.lazy.into_iter().flat_map(|(a, b)| a..=b));
            return uids;
        };
    let lazy = mb
        .lazy
        .into_iter()
        .flat_map(|(a, b)| a..=b)
        .map(MockMessage::generated);
    mb.messages
        .iter()
        .cloned()
        .chain(lazy)
        .filter(|m| matches(m))
        .map(|m| m.uid)
        .collect()
}

fn fetch_response(out: &mut Vec<u8>, seq: usize, message: &MockMessage, items: &str) {
    let raw = message.raw();
    let mut response = format!("* {seq} FETCH (UID {}", message.uid);
    if items.contains("FLAGS") {
        response.push_str(&format!(" FLAGS ({})", message.flags()));
    }
    if items.contains("RFC822.SIZE") {
        response.push_str(&format!(" RFC822.SIZE {}", raw.len()));
    }
    if items.contains("ENVELOPE") {
        let address = format!(
            "(({} NIL {} {}))",
            nstring(Some(&message.from_name)),
            nstring(message.from_email.split('@').next()),
            nstring(message.from_email.split('@').nth(1))
        );
        let to = format!(
            "((NIL NIL {} {}))",
            nstring(message.to_email.split('@').next()),
            nstring(message.to_email.split('@').nth(1))
        );
        response.push_str(&format!(
            " ENVELOPE ({} {} {address} {address} {address} {to} NIL NIL {} {})",
            nstring(Some(&message.date.to_rfc2822())),
            nstring(Some(&message.subject)),
            nstring(
                message
                    .in_reply_to
                    .as_ref()
                    .map(|id| format!("<{id}>"))
                    .as_deref()
            ),
            nstring(Some(&format!("<{}>", message.message_id))),
        ));
    }
    if items.contains("BODYSTRUCTURE") {
        response.push_str(&format!(
            " BODYSTRUCTURE (\"TEXT\" \"PLAIN\" (\"CHARSET\" \"utf-8\") NIL NIL \"7BIT\" {} 1)",
            message.body.len() + 2
        ));
    }
    if items.contains("HEADER.FIELDS") {
        let headers: String = raw
            .split("\r\n\r\n")
            .next()
            .unwrap_or("")
            .split("\r\n")
            .filter(|l| l.starts_with("References:") || l.starts_with("In-Reply-To:"))
            .map(|l| format!("{l}\r\n"))
            .chain(["\r\n".to_string()])
            .collect();
        response.push_str(&format!(
            " BODY[HEADER.FIELDS (REFERENCES IN-REPLY-TO)] {{{}}}\r\n{headers}",
            headers.len()
        ));
    } else if items.trim_matches(['(', ')', ' ']) == "RFC822" {
        response.push_str(&format!(" RFC822 {{{}}}\r\n{raw}", raw.len()));
    }
    response.push_str(")\r\n");
    out.extend_from_slice(response.as_bytes());
}
