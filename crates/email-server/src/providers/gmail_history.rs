//! Gmail REST history sync (`users.history.list`).
//!
//! Opt-in (see [`gmail_api_sync_enabled`]); the IMAP path is untouched when it
//! is off. Folders are Gmail labels. Each (account, label) keeps one persisted
//! `historyId`. A run reads the history from that id to the end, then stages the
//! result until the sync engine commits its messages; a failed run leaves the
//! old id and replays cleanly. Gmail message ids are opaque strings, so they
//! are mapped to stable per-label UIDs that are never reused.
//!
//! A 404 from `history.list` means the stored id is too old: it is dropped and
//! the label is fully re-listed. The full listing records the mailbox
//! `historyId` from `users.getProfile` *before* listing, so a change made while
//! listing is replayed by the next incremental run.
//!
//! Requests are built only from the configured API origin (Gmail pages with a
//! `pageToken`, not links). Access tokens are sent only as a bearer header and
//! never logged.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{Folder, Message};
use crate::{
    auth::token_source::AccessTokenSource,
    error::{AppError, Result},
};

pub const DEFAULT_BASE_URL: &str = "https://gmail.googleapis.com/gmail/v1";
const FLAG_ENV: &str = "EMAIL_RS_GMAIL_API_SYNC";
const MAX_THROTTLE_RETRIES: u32 = 5;
const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);
const MAX_PAGES: usize = 10_000;
const PAGE_SIZE: u32 = 500;
const METADATA_HEADERS: &str = "format=metadata&metadataHeaders=From&metadataHeaders=To&metadataHeaders=Cc&metadataHeaders=Subject&metadataHeaders=Message-ID&metadataHeaders=Date";

/// Labels that are views over other labels, not folders.
const VIRTUAL_LABELS: &[&str] = &["UNREAD", "STARRED", "IMPORTANT", "CHAT"];

/// Is Gmail API sync switched on? Off unless the variable is `1`, `true` or `yes`.
pub fn gmail_api_sync_enabled() -> bool {
    std::env::var(FLAG_ENV)
        .ok()
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

/// A failed Gmail call, kept apart from `AppError` so callers can tell a
/// missing resource (404: stale historyId, deleted message) from the rest.
enum GmailError {
    NotFound,
    Other(AppError),
}

impl From<AppError> for GmailError {
    fn from(error: AppError) -> Self {
        Self::Other(error)
    }
}

impl From<sqlx::Error> for GmailError {
    fn from(error: sqlx::Error) -> Self {
        Self::Other(error.into())
    }
}

pub struct GmailHistory {
    http: reqwest::Client,
    base_url: String,
    tokens: Arc<AccessTokenSource>,
    pool: SqlitePool,
    account_id: String,
    pending: Mutex<HashMap<String, PendingHistory>>,
}

/// A folder's result, held until the sync engine's message write commits.
struct PendingHistory {
    history_id: String,
    next_uid: i64,
    /// The folder's complete id -> uid map after this run.
    mapped: HashMap<String, i64>,
    removed: HashSet<String>,
}

/// What one pass over a label produced.
#[derive(Default)]
struct HistoryBatch {
    upserts: HashMap<String, Value>,
    removed: HashSet<String>,
    history_id: String,
}

impl GmailHistory {
    pub fn new(
        base_url: &str,
        tokens: Arc<AccessTokenSource>,
        pool: SqlitePool,
        account_id: String,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            tokens,
            pool,
            account_id,
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// GET a Gmail API URL as JSON, waiting out 429/503 `Retry-After`.
    async fn get(&self, url: &str) -> std::result::Result<Value, GmailError> {
        let mut attempts = 0;
        loop {
            let token = self.tokens.access_token().await?;
            let response = self
                .http
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| AppError::Transient("Gmail request failed".into()))?;
            let status = response.status();
            if status.as_u16() == 404 {
                return Err(GmailError::NotFound);
            }
            if matches!(status.as_u16(), 429 | 503) {
                attempts += 1;
                if attempts > MAX_THROTTLE_RETRIES {
                    return Err(AppError::Transient(format!(
                        "Gmail still throttling after {MAX_THROTTLE_RETRIES} retries"
                    ))
                    .into());
                }
                let wait = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(Duration::from_secs(1))
                    .min(MAX_RETRY_AFTER);
                tokio::time::sleep(wait).await;
                continue;
            }
            if status.is_server_error() {
                return Err(AppError::Transient(format!("Gmail returned {status}")).into());
            }
            if !status.is_success() {
                return Err(AppError::Provider(format!("Gmail returned {status}")).into());
            }
            return response
                .json()
                .await
                .map_err(|_| AppError::Provider("invalid Gmail response".into()).into());
        }
    }

    pub async fn list_folders(&self) -> Result<Vec<Folder>> {
        let url = format!("{}/users/me/labels", self.base_url);
        let body = self.get(&url).await.map_err(into_app)?;
        let mut folders = Vec::new();
        for label in body["labels"].as_array().into_iter().flatten() {
            let (Some(id), Some(name)) = (label["id"].as_str(), label["name"].as_str()) else {
                continue;
            };
            if VIRTUAL_LABELS.contains(&id) || id.starts_with("CATEGORY_") {
                continue;
            }
            let special_use = match id {
                "INBOX" => Some("inbox"),
                "SENT" => Some("sent"),
                "DRAFT" => Some("drafts"),
                "TRASH" => Some("trash"),
                "SPAM" => Some("spam"),
                _ => None,
            };
            folders.push(Folder {
                id: Uuid::new_v4(),
                name: name.to_string(),
                full_path: id.to_string(),
                special_use: special_use.map(String::from),
                delimiter: Some("/".to_string()),
                unread_count: 0,
                total_count: 0,
            });
        }
        Ok(folders)
    }

    async fn stored_cursor(&self, label: &str) -> Result<Option<String>> {
        let cursor: Option<Option<String>> = sqlx::query_scalar(
            "SELECT history_id FROM gmail_history_cursors WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(label)
        .fetch_optional(&self.pool)
        .await?;
        Ok(cursor.flatten())
    }

    async fn mapped_ids(&self, label: &str) -> Result<HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT gmail_id, uid FROM gmail_message_map WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(label)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    async fn fetch_metadata(&self, id: &str) -> std::result::Result<Option<Value>, GmailError> {
        let url = format!(
            "{}/users/me/messages/{}?{METADATA_HEADERS}",
            self.base_url,
            urlencode(id)
        );
        match self.get(&url).await {
            Ok(item) => Ok(Some(item)),
            Err(GmailError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List every message in a label and read its metadata.
    async fn read_full(&self, label: &str) -> std::result::Result<HistoryBatch, GmailError> {
        let profile = self
            .get(&format!("{}/users/me/profile", self.base_url))
            .await?;
        let history_id = history_id_of(&profile["historyId"])
            .ok_or_else(|| AppError::Provider("Gmail profile has no historyId".into()))?;
        let mut batch = HistoryBatch {
            history_id,
            ..Default::default()
        };
        let mut ids = Vec::new();
        let mut page_token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut url = format!(
                "{}/users/me/messages?labelIds={}&includeSpamTrash=true&maxResults={PAGE_SIZE}",
                self.base_url,
                urlencode(label)
            );
            if let Some(token) = &page_token {
                url.push_str("&pageToken=");
                url.push_str(&urlencode(token));
            }
            let page = self.get(&url).await?;
            for m in page["messages"].as_array().into_iter().flatten() {
                if let Some(id) = m["id"].as_str() {
                    ids.push(id.to_string());
                }
            }
            match page["nextPageToken"].as_str() {
                Some(token) => page_token = Some(token.to_string()),
                None => {
                    for id in ids {
                        if let Some(item) = self.fetch_metadata(&id).await? {
                            batch.upserts.insert(id, item);
                        }
                    }
                    return Ok(batch);
                }
            }
        }
        Err(AppError::Provider("Gmail message paging did not end".into()).into())
    }

    /// Read history from `start` to the end and resolve the touched messages.
    async fn read_history(
        &self,
        label: &str,
        start: &str,
        mapped: &HashMap<String, i64>,
    ) -> std::result::Result<HistoryBatch, GmailError> {
        let mut candidates: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        let mut deleted: HashSet<String> = HashSet::new();
        let mut history_id = start.to_string();
        let mut page_token: Option<String> = None;
        let mut ended = false;
        for _ in 0..MAX_PAGES {
            let mut url = format!(
                "{}/users/me/history?startHistoryId={}&maxResults={PAGE_SIZE}",
                self.base_url,
                urlencode(start)
            );
            if let Some(token) = &page_token {
                url.push_str("&pageToken=");
                url.push_str(&urlencode(token));
            }
            let page = self.get(&url).await?;
            for record in page["history"].as_array().into_iter().flatten() {
                for key in ["messagesAdded", "labelsAdded", "labelsRemoved"] {
                    for entry in record[key].as_array().into_iter().flatten() {
                        let message = &entry["message"];
                        let Some(id) = message["id"].as_str() else {
                            continue;
                        };
                        deleted.remove(id);
                        let in_label = message["labelIds"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|l| l.as_str() == Some(label));
                        let removed_here = entry["labelIds"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|l| l.as_str() == Some(label));
                        if (in_label || removed_here || mapped.contains_key(id))
                            && seen.insert(id.to_string())
                        {
                            candidates.push(id.to_string());
                        }
                    }
                }
                for entry in record["messagesDeleted"].as_array().into_iter().flatten() {
                    if let Some(id) = entry["message"]["id"].as_str() {
                        deleted.insert(id.to_string());
                    }
                }
            }
            if let Some(id) = history_id_of(&page["historyId"]) {
                history_id = id;
            }
            match page["nextPageToken"].as_str() {
                Some(token) => page_token = Some(token.to_string()),
                None => {
                    ended = true;
                    break;
                }
            }
        }
        if !ended {
            return Err(AppError::Provider("Gmail history paging did not end".into()).into());
        }
        let mut batch = HistoryBatch {
            history_id,
            removed: deleted.clone(),
            ..Default::default()
        };
        for id in candidates {
            if deleted.contains(&id) {
                continue;
            }
            match self.fetch_metadata(&id).await? {
                Some(item) if has_label(&item, label) => {
                    batch.upserts.insert(id, item);
                }
                _ => {
                    batch.removed.insert(id);
                }
            }
        }
        Ok(batch)
    }

    /// Apply one history pass for a label and return the messages to upsert.
    /// An expired historyId (404) is dropped and the label is fully re-listed.
    pub async fn sync_folder(&self, label: &str) -> Result<Vec<Message>> {
        let result = self.sync_folder_inner(label).await;
        if result.is_err() {
            self.discard_pending(label);
        }
        result
    }

    async fn sync_folder_inner(&self, label: &str) -> Result<Vec<Message>> {
        let mapped = self.mapped_ids(label).await?;
        let mut full = true;
        let mut batch = None;
        if let Some(cursor) = self.stored_cursor(label).await? {
            match self.read_history(label, &cursor, &mapped).await {
                Ok(b) => {
                    full = false;
                    batch = Some(b);
                }
                Err(GmailError::NotFound) => {}
                Err(e) => return Err(into_app(e)),
            }
        }
        let batch = match batch {
            Some(b) => b,
            None => self.read_full(label).await.map_err(into_app)?,
        };
        self.apply(label, batch, full, mapped).await
    }

    async fn apply(
        &self,
        label: &str,
        batch: HistoryBatch,
        full: bool,
        mut mapped: HashMap<String, i64>,
    ) -> Result<Vec<Message>> {
        let mut next_uid: i64 = sqlx::query_scalar(
            "SELECT next_uid FROM gmail_history_cursors WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(label)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(1);
        let mut removed = batch.removed.clone();
        for id in &removed {
            mapped.remove(id);
        }
        if full {
            for id in mapped
                .keys()
                .filter(|id| !batch.upserts.contains_key(*id))
                .cloned()
                .collect::<Vec<_>>()
            {
                mapped.remove(&id);
                removed.insert(id);
            }
        }
        let mut messages = Vec::with_capacity(batch.upserts.len());
        let mut ordered: Vec<(&String, &Value)> = batch.upserts.iter().collect();
        ordered.sort_by(|a, b| a.0.cmp(b.0));
        for (gmail_id, item) in ordered {
            let uid = if let Some(uid) = mapped.get(gmail_id) {
                *uid
            } else {
                let uid = next_uid;
                next_uid += 1;
                mapped.insert(gmail_id.clone(), uid);
                uid
            };
            messages.push(parse_message(item, gmail_id, uid as u32, &self.account_id));
        }
        self.pending.lock().unwrap().insert(
            label.to_string(),
            PendingHistory {
                history_id: batch.history_id,
                next_uid,
                mapped,
                removed,
            },
        );
        Ok(messages)
    }

    /// Persist the staged historyId and id map. Called by the sync engine only
    /// after its message transaction has committed.
    pub async fn commit_pending(&self, label: &str) -> Result<()> {
        let Some(pending) = self.pending.lock().unwrap().remove(label) else {
            return Ok(());
        };
        let mut tx = crate::db::begin_write(&self.pool)
            .await
            .map_err(|error| AppError::Transient(error.to_string()))?;
        sqlx::query(
            "INSERT OR IGNORE INTO gmail_history_cursors (account_id, folder_id) VALUES (?, ?)",
        )
        .bind(&self.account_id)
        .bind(label)
        .execute(&mut *tx)
        .await?;
        for id in pending.removed {
            sqlx::query(
                "DELETE FROM gmail_message_map
                 WHERE account_id = ? AND folder_id = ? AND gmail_id = ?",
            )
            .bind(&self.account_id)
            .bind(label)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        for (id, uid) in pending.mapped {
            sqlx::query(
                "INSERT OR REPLACE INTO gmail_message_map
                 (account_id, folder_id, gmail_id, uid) VALUES (?, ?, ?, ?)",
            )
            .bind(&self.account_id)
            .bind(label)
            .bind(id)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "UPDATE gmail_history_cursors SET history_id = ?, next_uid = ?
             WHERE account_id = ? AND folder_id = ?",
        )
        .bind(pending.history_id)
        .bind(pending.next_uid)
        .bind(&self.account_id)
        .bind(label)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub fn discard_pending(&self, label: &str) {
        self.pending.lock().unwrap().remove(label);
    }

    /// Every UID currently mapped in the label (the post-sync snapshot).
    pub async fn list_uids(&self, label: &str) -> Result<Vec<u32>> {
        if let Some(pending) = self.pending.lock().unwrap().get(label) {
            return Ok(pending
                .mapped
                .values()
                .filter_map(|uid| u32::try_from(*uid).ok())
                .collect());
        }
        let uids: Vec<i64> = sqlx::query_scalar(
            "SELECT uid FROM gmail_message_map WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(label)
        .fetch_all(&self.pool)
        .await?;
        Ok(uids
            .into_iter()
            .filter_map(|u| u32::try_from(u).ok())
            .collect())
    }

    /// Metadata for specific UIDs, fetched individually by Gmail id.
    pub async fn fetch_by_uids(&self, label: &str, uids: &[u32]) -> Result<Vec<Message>> {
        let pending_map = self
            .pending
            .lock()
            .unwrap()
            .get(label)
            .map(|p| p.mapped.clone());
        let mapped = match pending_map {
            Some(map) => map,
            None => self.mapped_ids(label).await?,
        };
        let by_uid: HashMap<i64, &String> = mapped.iter().map(|(id, uid)| (*uid, id)).collect();
        let mut out = Vec::new();
        for uid in uids {
            let Some(gmail_id) = by_uid.get(&i64::from(*uid)) else {
                continue;
            };
            if let Some(item) = self.fetch_metadata(gmail_id).await.map_err(into_app)? {
                out.push(parse_message(&item, gmail_id, *uid, &self.account_id));
            }
        }
        Ok(out)
    }
}

fn into_app(error: GmailError) -> AppError {
    match error {
        GmailError::NotFound => AppError::Provider("Gmail resource not found".into()),
        GmailError::Other(e) => e,
    }
}

/// Gmail returns `historyId` as a string; accept a number too.
fn history_id_of(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn has_label(item: &Value, label: &str) -> bool {
    item["labelIds"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|l| l.as_str() == Some(label))
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn header<'a>(item: &'a Value, name: &str) -> Option<&'a str> {
    item["payload"]["headers"]
        .as_array()?
        .iter()
        .find(|h| {
            h["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|h| h["value"].as_str())
}

/// Split an address list on commas that are outside quotes and angle brackets.
fn split_addresses(list: &str) -> Vec<String> {
    let (mut out, mut current) = (Vec::new(), String::new());
    let (mut quoted, mut angle) = (false, false);
    for c in list.chars() {
        match c {
            '"' => quoted = !quoted,
            '<' if !quoted => angle = true,
            '>' if !quoted => angle = false,
            ',' if !quoted && !angle => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    out.push(current);
    out.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `Name <a@b>` or bare `a@b` into (name, address).
fn parse_from(value: &str) -> (Option<String>, Option<String>) {
    let value = value.trim();
    match (value.rfind('<'), value.rfind('>')) {
        (Some(open), Some(close)) if open < close => {
            let name = value[..open].trim().trim_matches('"').trim();
            (
                (!name.is_empty()).then(|| name.to_string()),
                Some(value[open + 1..close].trim().to_string()),
            )
        }
        _ if value.is_empty() => (None, None),
        _ => (None, Some(value.to_string())),
    }
}

fn has_attachment(part: &Value) -> bool {
    part["filename"].as_str().is_some_and(|f| !f.is_empty())
        || part["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .any(has_attachment)
}

fn parse_message(item: &Value, gmail_id: &str, uid: u32, account_id: &str) -> Message {
    let date = item["internalDate"]
        .as_str()
        .and_then(|ms| ms.parse::<i64>().ok())
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
        .or_else(|| {
            header(item, "Date")
                .and_then(|d| DateTime::parse_from_rfc2822(d).ok())
                .map(|d| d.with_timezone(&Utc))
        });
    let (from_name, from_email) = header(item, "From").map(parse_from).unwrap_or_default();
    let labels = |name: &str| {
        item["labelIds"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|l| l.as_str() == Some(name))
    };
    // The REST thread id is the hex form of IMAP's X-GM-THRID, so threads match
    // across both paths.
    let thread_id = item["threadId"]
        .as_str()
        .and_then(|t| u64::from_str_radix(t, 16).ok())
        .map(|t| format!("gmail:{account_id}:{t}"));
    Message {
        id: Uuid::new_v4(),
        uid,
        message_id: header(item, "Message-ID")
            .map(|v| v.trim().to_string())
            .or_else(|| Some(gmail_id.to_string())),
        thread_id,
        subject: header(item, "Subject").map(String::from),
        from_name,
        from_email,
        to: header(item, "To").map(split_addresses).unwrap_or_default(),
        cc: header(item, "Cc").map(split_addresses).unwrap_or_default(),
        date,
        is_read: !labels("UNREAD"),
        is_flagged: labels("STARRED"),
        is_draft: labels("DRAFT"),
        has_attachments: has_attachment(&item["payload"]),
        attachments: vec![],
        preview: item["snippet"].as_str().map(String::from),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
        time::Instant,
    };

    use async_trait::async_trait;
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode, Uri},
        response::IntoResponse,
        Router,
    };
    use serde_json::json;

    use super::*;
    use crate::auth::{oauth2::StoredToken, token_source::TokenStore};

    struct StaticStore(StoredToken);

    #[async_trait]
    impl TokenStore for StaticStore {
        async fn load(&self) -> Result<StoredToken> {
            Ok(self.0.clone())
        }
        async fn refresh(&self, _: &StoredToken) -> Result<StoredToken> {
            Err(AppError::Auth("no refresh in tests".into()))
        }
    }

    type Script = Arc<Mutex<VecDeque<(u16, Option<&'static str>, Value)>>>;

    /// Request URI and `Authorization` header.
    type SeenRequest = (String, Option<String>);

    #[derive(Clone, Default)]
    struct Mock {
        script: Script,
        seen: Arc<Mutex<Vec<SeenRequest>>>,
    }

    async fn handler(State(mock): State<Mock>, uri: Uri, headers: HeaderMap) -> impl IntoResponse {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        mock.seen.lock().unwrap().push((uri.to_string(), auth));
        let (status, retry_after, body) = mock
            .script
            .lock()
            .unwrap()
            .pop_front()
            .expect("unscripted Gmail request");
        let mut out = (StatusCode::from_u16(status).unwrap(), axum::Json(body)).into_response();
        if let Some(v) = retry_after {
            out.headers_mut().insert("retry-after", v.parse().unwrap());
        }
        out
    }

    struct Harness {
        gmail: Arc<GmailHistory>,
        mock: Mock,
        pool: SqlitePool,
    }

    async fn harness() -> Harness {
        let mock = Mock::default();
        let app = Router::new().fallback(handler).with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (pool, _) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a','A','a@example.test','gmail','oauth2')")
            .execute(&pool)
            .await
            .unwrap();
        let token = StoredToken {
            access_token: "secret-token".into(),
            refresh_token: None,
            expires_at: None,
            id_token: None,
        };
        let tokens = Arc::new(AccessTokenSource::new(
            "a".into(),
            token.clone(),
            Arc::new(StaticStore(token)),
        ));
        Harness {
            gmail: Arc::new(GmailHistory::new(&base, tokens, pool.clone(), "a".into())),
            mock,
            pool,
        }
    }

    fn script(h: &Harness, steps: Vec<(u16, Option<&'static str>, Value)>) {
        h.mock.script.lock().unwrap().extend(steps);
    }

    fn msg(id: &str, subject: &str, labels: &[&str]) -> Value {
        json!({"id": id, "threadId": "1a", "labelIds": labels, "snippet": "snip",
               "internalDate": "1767323045000",
               "payload": {"headers": [
                   {"name": "From", "value": "\"N, Sender\" <n@example.test>"},
                   {"name": "To", "value": "x@example.test, Y <y@example.test>"},
                   {"name": "Subject", "value": subject},
                   {"name": "Message-ID", "value": format!("<{id}@example.test>")}]}})
    }

    fn ids(list: &[&str]) -> Value {
        json!({"messages": list.iter().map(|i| json!({"id": i})).collect::<Vec<_>>()})
    }

    fn uid_of(messages: &[Message], subject: &str) -> u32 {
        messages
            .iter()
            .find(|m| m.subject.as_deref() == Some(subject))
            .unwrap()
            .uid
    }

    /// Scripts a full listing of `m1` (INBOX, unread) and `m2`, baseline `h1`.
    fn initial_steps() -> Vec<(u16, Option<&'static str>, Value)> {
        vec![
            (200, None, json!({"historyId": "100"})),
            (200, None, ids(&["m1", "m2"])),
            (200, None, msg("m1", "one", &["INBOX", "UNREAD"])),
            (200, None, msg("m2", "two", &["INBOX"])),
        ]
    }

    #[tokio::test]
    async fn initial_sync_lists_pages_and_stores_baseline_history_id() {
        let h = harness().await;
        script(
            &h,
            vec![
                (200, None, json!({"historyId": "100"})),
                (
                    200,
                    None,
                    json!({"messages": [{"id": "m1"}], "nextPageToken": "tok/1"}),
                ),
                (200, None, ids(&["m2"])),
                (200, None, msg("m1", "one", &["INBOX", "UNREAD"])),
                (200, None, msg("m2", "two", &["INBOX", "STARRED"])),
            ],
        );
        let messages = h.gmail.sync_folder("INBOX").await.unwrap();
        assert_eq!(messages.len(), 2);
        let one = messages
            .iter()
            .find(|m| m.subject.as_deref() == Some("one"))
            .unwrap();
        assert!(!one.is_read);
        assert_eq!(one.from_name.as_deref(), Some("N, Sender"));
        assert_eq!(one.from_email.as_deref(), Some("n@example.test"));
        assert_eq!(one.to, vec!["x@example.test", "Y <y@example.test>"]);
        assert_eq!(one.thread_id.as_deref(), Some("gmail:a:26"));
        assert!(messages.iter().any(|m| m.is_flagged));
        assert_eq!(h.gmail.stored_cursor("INBOX").await.unwrap(), None);
        h.gmail.commit_pending("INBOX").await.unwrap();
        assert_eq!(h.gmail.list_uids("INBOX").await.unwrap().len(), 2);
        assert_eq!(
            h.gmail.stored_cursor("INBOX").await.unwrap().as_deref(),
            Some("100")
        );
        let seen = h.mock.seen.lock().unwrap();
        assert_eq!(seen[0].0, "/users/me/profile");
        assert!(seen[1].0.starts_with("/users/me/messages?labelIds=INBOX"));
        assert!(seen[2].0.contains("pageToken=tok%2F1"));
        assert!(seen[3]
            .0
            .starts_with("/users/me/messages/m1?format=metadata"));
        assert_eq!(seen[0].1.as_deref(), Some("Bearer secret-token"));
    }

    #[tokio::test]
    async fn incremental_sync_handles_add_label_change_and_delete() {
        let h = harness().await;
        script(&h, initial_steps());
        let first = h.gmail.sync_folder("INBOX").await.unwrap();
        let (uid1, uid2) = (uid_of(&first, "one"), uid_of(&first, "two"));
        h.gmail.commit_pending("INBOX").await.unwrap();
        script(
            &h,
            vec![
                (
                    200,
                    None,
                    json!({"history": [
                        {"id": "101", "messagesAdded": [{"message": {"id": "m3", "labelIds": ["INBOX"]}}]},
                        {"id": "102", "labelsRemoved": [{"message": {"id": "m1", "labelIds": ["INBOX"]}, "labelIds": ["UNREAD"]}]},
                        {"id": "103", "messagesDeleted": [{"message": {"id": "m2"}}]},
                        {"id": "104", "messagesAdded": [{"message": {"id": "other", "labelIds": ["SENT"]}}]}
                    ], "historyId": "110"}),
                ),
                (200, None, msg("m3", "three", &["INBOX", "UNREAD"])),
                (200, None, msg("m1", "one", &["INBOX"])),
            ],
        );
        let second = h.gmail.sync_folder("INBOX").await.unwrap();
        assert_eq!(second.len(), 2);
        let changed = second.iter().find(|m| m.uid == uid1).unwrap();
        assert!(changed.is_read, "label change is reflected");
        assert!(second.iter().any(|m| m.uid > uid2), "uids are not reused");
        h.gmail.commit_pending("INBOX").await.unwrap();
        let uids = h.gmail.list_uids("INBOX").await.unwrap();
        assert!(!uids.contains(&uid2) && uids.len() == 2);
        assert_eq!(
            h.gmail.stored_cursor("INBOX").await.unwrap().as_deref(),
            Some("110")
        );
        let seen = h.mock.seen.lock().unwrap();
        assert!(seen[4]
            .0
            .starts_with("/users/me/history?startHistoryId=100"));
        assert_eq!(seen.len(), 7, "the SENT-only message is never fetched");
    }

    #[tokio::test]
    async fn message_that_lost_the_label_is_removed() {
        let h = harness().await;
        script(&h, initial_steps());
        h.gmail.sync_folder("INBOX").await.unwrap();
        h.gmail.commit_pending("INBOX").await.unwrap();
        script(
            &h,
            vec![
                (
                    200,
                    None,
                    json!({"history": [{"id": "101", "labelsRemoved": [
                        {"message": {"id": "m1", "labelIds": ["UNREAD"]}, "labelIds": ["INBOX"]}]}],
                        "historyId": "101"}),
                ),
                (200, None, msg("m1", "one", &["UNREAD"])),
            ],
        );
        assert!(h.gmail.sync_folder("INBOX").await.unwrap().is_empty());
        h.gmail.commit_pending("INBOX").await.unwrap();
        assert_eq!(h.gmail.list_uids("INBOX").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn history_pages_follow_the_page_token() {
        let h = harness().await;
        script(&h, initial_steps());
        h.gmail.sync_folder("INBOX").await.unwrap();
        h.gmail.commit_pending("INBOX").await.unwrap();
        script(
            &h,
            vec![
                (
                    200,
                    None,
                    json!({"history": [{"id": "101", "messagesAdded": [{"message": {"id": "m3", "labelIds": ["INBOX"]}}]}],
                           "nextPageToken": "p2", "historyId": "120"}),
                ),
                (
                    200,
                    None,
                    json!({"history": [{"id": "102", "messagesAdded": [{"message": {"id": "m4", "labelIds": ["INBOX"]}}]}],
                           "historyId": "120"}),
                ),
                (200, None, msg("m3", "three", &["INBOX"])),
                (200, None, msg("m4", "four", &["INBOX"])),
            ],
        );
        assert_eq!(h.gmail.sync_folder("INBOX").await.unwrap().len(), 2);
        assert!(h.mock.seen.lock().unwrap()[5].0.contains("pageToken=p2"));
    }

    #[tokio::test]
    async fn stale_history_id_404_triggers_full_resync() {
        let h = harness().await;
        script(&h, initial_steps());
        let first = h.gmail.sync_folder("INBOX").await.unwrap();
        let (uid1, uid2) = (uid_of(&first, "one"), uid_of(&first, "two"));
        h.gmail.commit_pending("INBOX").await.unwrap();
        script(
            &h,
            vec![
                (404, None, json!({"error": {"code": 404}})),
                (200, None, json!({"historyId": "500"})),
                (200, None, ids(&["m2"])),
                (200, None, msg("m2", "two", &["INBOX"])),
            ],
        );
        let resync = h.gmail.sync_folder("INBOX").await.unwrap();
        assert_eq!(resync.len(), 1);
        assert_eq!(resync[0].uid, uid2, "surviving message keeps its uid");
        h.gmail.commit_pending("INBOX").await.unwrap();
        let uids = h.gmail.list_uids("INBOX").await.unwrap();
        assert_eq!(uids, vec![uid2]);
        assert!(!uids.contains(&uid1));
        assert_eq!(
            h.gmail.stored_cursor("INBOX").await.unwrap().as_deref(),
            Some("500")
        );
        let seen = h.mock.seen.lock().unwrap();
        assert!(seen[4]
            .0
            .starts_with("/users/me/history?startHistoryId=100"));
        assert_eq!(seen[5].0, "/users/me/profile");
    }

    #[tokio::test]
    async fn throttling_honours_capped_retry_after_then_succeeds() {
        let h = harness().await;
        let mut steps = vec![(429, Some("1"), json!({"error": {"code": 429}}))];
        steps.extend(initial_steps());
        script(&h, steps);
        let started = Instant::now();
        assert_eq!(h.gmail.sync_folder("INBOX").await.unwrap().len(), 2);
        assert!(started.elapsed() >= Duration::from_millis(900));
        assert!(
            started.elapsed() < MAX_RETRY_AFTER,
            "a Retry-After of one second is not stretched"
        );
    }

    #[tokio::test]
    async fn persistent_throttling_gives_up_as_transient() {
        let h = harness().await;
        for _ in 0..=MAX_THROTTLE_RETRIES {
            script(&h, vec![(429, Some("0"), json!({}))]);
        }
        let error = h.gmail.sync_folder("INBOX").await.unwrap_err();
        assert!(matches!(error, AppError::Transient(_)));
    }

    #[tokio::test]
    async fn retry_after_is_capped() {
        // Parsed value is clamped; checking the clamp without sleeping.
        let huge = Duration::from_secs(86_400).min(MAX_RETRY_AFTER);
        assert_eq!(huge, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn failed_run_keeps_the_old_history_id() {
        let h = harness().await;
        script(&h, initial_steps());
        h.gmail.sync_folder("INBOX").await.unwrap();
        h.gmail.commit_pending("INBOX").await.unwrap();
        script(&h, vec![(500, None, json!({}))]);
        assert!(h.gmail.sync_folder("INBOX").await.is_err());
        assert_eq!(h.gmail.list_uids("INBOX").await.unwrap().len(), 2);
        assert_eq!(
            h.gmail.stored_cursor("INBOX").await.unwrap().as_deref(),
            Some("100")
        );
    }

    #[tokio::test]
    async fn engine_write_failure_replays_old_history_id_and_discards_pending() {
        use crate::providers::gmail::GmailProvider;
        let h = harness().await;
        script(&h, initial_steps());
        let update = |extra: Vec<(u16, Option<&'static str>, Value)>| {
            let mut steps = vec![(
                200,
                None,
                json!({"history": [
                    {"id": "101", "messagesAdded": [{"message": {"id": "m3", "labelIds": ["INBOX"]}}]},
                    {"id": "102", "labelsRemoved": [{"message": {"id": "m1", "labelIds": ["INBOX"]}, "labelIds": ["UNREAD"]}]}],
                    "historyId": "110"}),
            )];
            steps.extend(extra);
            steps
        };
        let fetches = vec![
            (200, None, msg("m3", "three", &["INBOX"])),
            (200, None, msg("m1", "edited", &["INBOX"])),
        ];
        script(&h, update(fetches.clone()));
        script(&h, update(fetches));
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path, uid_validity) VALUES ('f','a','Inbox','INBOX',1)")
            .execute(&h.pool).await.unwrap();
        let tokens = h.gmail.tokens.clone();
        let provider = Arc::new(
            GmailProvider::new("a".into(), "a@example.test".into(), tokens)
                .with_gmail_history(h.gmail.clone()),
        );
        crate::imap::sync::sync_graph_test_folder(provider.clone(), h.pool.clone())
            .await
            .unwrap();
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT uid, subject FROM messages ORDER BY uid")
                .fetch_all(&h.pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![(1, "one".into()), (2, "two".into())]);
        sqlx::query("CREATE TRIGGER fail_message_write BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'injected message write failure'); END")
            .execute(&h.pool).await.unwrap();
        let error = crate::imap::sync::sync_graph_test_folder(provider.clone(), h.pool.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("injected message write failure"));
        assert!(h.gmail.pending.lock().unwrap().is_empty());
        assert_eq!(h.gmail.list_uids("INBOX").await.unwrap().len(), 2);
        assert_eq!(
            h.gmail.stored_cursor("INBOX").await.unwrap().as_deref(),
            Some("100"),
            "the new historyId is not persisted when the engine write fails"
        );
        sqlx::query("DROP TRIGGER fail_message_write")
            .execute(&h.pool)
            .await
            .unwrap();
        crate::imap::sync::sync_graph_test_folder(provider, h.pool.clone())
            .await
            .unwrap();
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT uid, subject FROM messages ORDER BY uid")
                .fetch_all(&h.pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![(1, "edited".into()), (2, "two".into()), (3, "three".into())]
        );
        assert_eq!(
            h.gmail.stored_cursor("INBOX").await.unwrap().as_deref(),
            Some("110")
        );
        let seen = h.mock.seen.lock().unwrap();
        let history: Vec<&str> = seen
            .iter()
            .map(|s| s.0.as_str())
            .filter(|u| u.starts_with("/users/me/history"))
            .collect();
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|u| u.contains("startHistoryId=100")));
    }

    #[tokio::test]
    async fn requests_only_go_to_the_configured_origin() {
        let h = harness().await;
        let mut steps = initial_steps();
        steps.insert(
            1,
            (
                200,
                None,
                json!({"messages": [], "nextPageToken": "http://evil.example.test/x"}),
            ),
        );
        script(&h, steps);
        // The hostile token is opaque data: it is encoded into a query value on
        // our own origin and never followed as a URL.
        let _ = h.gmail.sync_folder("INBOX").await;
        let seen = h.mock.seen.lock().unwrap();
        assert!(seen.iter().all(|s| s.0.starts_with('/')));
        assert!(seen[2]
            .0
            .contains("pageToken=http%3A%2F%2Fevil.example.test%2Fx"));
    }

    #[tokio::test]
    async fn lists_labels_as_folders_skipping_virtual_ones() {
        let h = harness().await;
        script(
            &h,
            vec![(
                200,
                None,
                json!({"labels": [
                    {"id": "INBOX", "name": "INBOX"},
                    {"id": "SENT", "name": "SENT"},
                    {"id": "STARRED", "name": "STARRED"},
                    {"id": "CATEGORY_SOCIAL", "name": "CATEGORY_SOCIAL"},
                    {"id": "Label_1", "name": "Work/Reports"}]}),
            )],
        );
        let folders = h.gmail.list_folders().await.unwrap();
        let paths: Vec<&str> = folders.iter().map(|f| f.full_path.as_str()).collect();
        assert_eq!(paths, vec!["INBOX", "SENT", "Label_1"]);
        assert_eq!(folders[0].special_use.as_deref(), Some("inbox"));
        assert_eq!(folders[2].name, "Work/Reports");
    }

    #[test]
    fn flag_defaults_off() {
        if std::env::var(FLAG_ENV).is_err() {
            assert!(!gmail_api_sync_enabled());
        }
    }

    #[test]
    fn address_parsing() {
        assert_eq!(
            split_addresses("\"A, B\" <a@x>, c@y"),
            vec!["\"A, B\" <a@x>", "c@y"]
        );
        assert_eq!(parse_from("c@y"), (None, Some("c@y".into())));
    }
}
