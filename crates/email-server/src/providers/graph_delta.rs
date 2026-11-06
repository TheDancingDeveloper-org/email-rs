//! Microsoft Graph mail delta sync (`/me/mailFolders/{id}/messages/delta`).
//!
//! Opt-in (see [`graph_sync_enabled`]); the IMAP path is untouched when it is
//! off. Each folder keeps one persisted `@odata.deltaLink`. A run follows
//! `@odata.nextLink` pages to the end, then stages the result until the sync
//! engine commits its messages; a failed run leaves the old link and replays
//! cleanly. Graph message ids are opaque strings, so they are
//! mapped to stable per-folder UIDs that are never reused.
//!
//! Access tokens are sent only as a bearer header, only to the configured
//! Graph origin, and never logged.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{Folder, Message};
use crate::{
    auth::token_source::AccessTokenSource,
    error::{AppError, Result},
};

pub const DEFAULT_BASE_URL: &str = "https://graph.microsoft.com/v1.0";
const FLAG_ENV: &str = "EMAIL_RS_MICROSOFT_GRAPH_SYNC";
const MESSAGE_SELECT: &str = "id,internetMessageId,subject,from,toRecipients,ccRecipients,receivedDateTime,isRead,isDraft,hasAttachments,flag,bodyPreview";
const MAX_THROTTLE_RETRIES: u32 = 5;
const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);
const MAX_PAGES: usize = 10_000;

/// Is Graph sync switched on? Off unless the variable is `1`, `true` or `yes`.
pub fn graph_sync_enabled() -> bool {
    std::env::var(FLAG_ENV)
        .ok()
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

/// A failed Graph call, kept apart from `AppError` so the delta loop can tell
/// an expired deltaLink from every other failure.
enum GraphError {
    Gone,
    Other(AppError),
}

impl From<AppError> for GraphError {
    fn from(error: AppError) -> Self {
        Self::Other(error)
    }
}

impl From<sqlx::Error> for GraphError {
    fn from(error: sqlx::Error) -> Self {
        Self::Other(error.into())
    }
}

pub struct GraphDelta {
    http: reqwest::Client,
    base_url: String,
    tokens: Arc<AccessTokenSource>,
    pool: SqlitePool,
    account_id: String,
    pending: Mutex<HashMap<String, PendingDelta>>,
}

struct PendingDelta {
    delta_link: Option<String>,
    next_uid: i64,
    upserts: HashMap<String, i64>,
    removed: HashSet<String>,
}

/// What one pass over a folder's delta feed produced.
#[derive(Default)]
struct DeltaBatch {
    upserts: HashMap<String, Value>,
    removed: HashSet<String>,
    delta_link: Option<String>,
}

impl GraphDelta {
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

    /// A link Graph hands back is followed only if it stays on the Graph
    /// origin, so the bearer token cannot be sent elsewhere.
    fn same_origin(&self, link: &str) -> bool {
        match (
            reqwest::Url::parse(&self.base_url),
            reqwest::Url::parse(link),
        ) {
            (Ok(base), Ok(link)) => {
                base.scheme() == link.scheme()
                    && base.host_str() == link.host_str()
                    && base.port_or_known_default() == link.port_or_known_default()
            }
            _ => false,
        }
    }

    /// GET a Graph URL as JSON, waiting out 429/503 `Retry-After`.
    async fn get(&self, url: &str) -> std::result::Result<Value, GraphError> {
        let mut attempts = 0;
        loop {
            let token = self.tokens.access_token().await?;
            let response = self
                .http
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| AppError::Transient("Graph request failed".into()))?;
            let status = response.status();
            if status.as_u16() == 410 {
                return Err(GraphError::Gone);
            }
            if matches!(status.as_u16(), 429 | 503) {
                attempts += 1;
                if attempts > MAX_THROTTLE_RETRIES {
                    return Err(AppError::Transient(format!(
                        "Graph still throttling after {MAX_THROTTLE_RETRIES} retries"
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
                return Err(AppError::Transient(format!("Graph returned {status}")).into());
            }
            if !status.is_success() {
                return Err(AppError::Provider(format!("Graph returned {status}")).into());
            }
            return response
                .json()
                .await
                .map_err(|_| AppError::Provider("invalid Graph response".into()).into());
        }
    }

    pub async fn list_folders(&self) -> Result<Vec<Folder>> {
        let mut url = format!("{}/me/mailFolders?$top=100", self.base_url);
        let mut folders = Vec::new();
        for _ in 0..MAX_PAGES {
            let page = self.get(&url).await.map_err(into_app)?;
            for f in page["value"].as_array().into_iter().flatten() {
                let (Some(id), Some(name)) = (f["id"].as_str(), f["displayName"].as_str()) else {
                    continue;
                };
                folders.push(Folder {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    full_path: id.to_string(),
                    special_use: None,
                    delimiter: None,
                    unread_count: f["unreadItemCount"].as_u64().unwrap_or(0) as u32,
                    total_count: f["totalItemCount"].as_u64().unwrap_or(0) as u32,
                });
            }
            match page["@odata.nextLink"].as_str() {
                Some(next) if self.same_origin(next) => url = next.to_string(),
                Some(_) => return Err(AppError::Provider("Graph nextLink left origin".into())),
                None => return Ok(folders),
            }
        }
        Err(AppError::Provider("Graph folder paging did not end".into()))
    }

    async fn stored_link(&self, folder_id: &str) -> Result<Option<String>> {
        let link: Option<Option<String>> = sqlx::query_scalar(
            "SELECT delta_link FROM graph_delta_links WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(folder_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(link.flatten().filter(|l| self.same_origin(l)))
    }

    /// Follow nextLinks from `start` to the closing deltaLink.
    async fn read_feed(&self, start: String) -> std::result::Result<DeltaBatch, GraphError> {
        let mut batch = DeltaBatch::default();
        let mut url = start;
        for _ in 0..MAX_PAGES {
            let page = self.get(&url).await?;
            for item in page["value"].as_array().into_iter().flatten() {
                let Some(id) = item["id"].as_str() else {
                    continue;
                };
                if item.get("@removed").is_some() {
                    batch.upserts.remove(id);
                    batch.removed.insert(id.to_string());
                } else {
                    batch.removed.remove(id);
                    batch.upserts.insert(id.to_string(), item.clone());
                }
            }
            if let Some(next) = page["@odata.nextLink"].as_str() {
                if !self.same_origin(next) {
                    return Err(AppError::Provider("Graph nextLink left origin".into()).into());
                }
                url = next.to_string();
            } else if let Some(delta) = page["@odata.deltaLink"].as_str() {
                if !self.same_origin(delta) {
                    return Err(AppError::Provider("Graph deltaLink left origin".into()).into());
                }
                batch.delta_link = Some(delta.to_string());
                return Ok(batch);
            } else {
                return Err(
                    AppError::Provider("Graph delta ended without a deltaLink".into()).into(),
                );
            }
        }
        Err(AppError::Provider("Graph delta paging did not end".into()).into())
    }

    /// Apply one delta pass for a folder and return the messages to upsert.
    /// An expired deltaLink (410) is dropped and the folder is fully re-synced.
    pub async fn sync_folder(&self, folder_id: &str) -> Result<Vec<Message>> {
        let result = self.sync_folder_inner(folder_id).await;
        if result.is_err() {
            self.discard_pending(folder_id);
        }
        result
    }

    async fn sync_folder_inner(&self, folder_id: &str) -> Result<Vec<Message>> {
        let initial_url = format!(
            "{}/me/mailFolders/{}/messages/delta?$select={}",
            self.base_url,
            urlencode(folder_id),
            MESSAGE_SELECT
        );
        let mut full = true;
        let mut start = initial_url.clone();
        if let Some(link) = self.stored_link(folder_id).await? {
            full = false;
            start = link;
        }
        let batch = match self.read_feed(start).await {
            Ok(batch) => batch,
            Err(GraphError::Gone) if !full => {
                full = true;
                self.read_feed(initial_url).await.map_err(into_app)?
            }
            Err(e) => return Err(into_app(e)),
        };
        self.apply(folder_id, batch, full).await
    }

    async fn apply(&self, folder_id: &str, batch: DeltaBatch, full: bool) -> Result<Vec<Message>> {
        let mut next_uid: i64 = sqlx::query_scalar(
            "SELECT next_uid FROM graph_delta_links WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(folder_id)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(1);
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT graph_id, uid FROM graph_message_map WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;
        let mut mapped: HashMap<String, i64> = rows.into_iter().collect();
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
        for (graph_id, item) in ordered {
            let uid = if let Some(uid) = mapped.get(graph_id) {
                *uid
            } else {
                let uid = next_uid;
                next_uid += 1;
                mapped.insert(graph_id.clone(), uid);
                uid
            };
            messages.push(parse_message(item, graph_id, uid as u32));
        }
        self.pending.lock().unwrap().insert(
            folder_id.to_string(),
            PendingDelta {
                delta_link: batch.delta_link,
                next_uid,
                upserts: mapped,
                removed,
            },
        );
        Ok(messages)
    }

    pub async fn commit_pending(&self, folder_id: &str) -> Result<()> {
        let Some(pending) = self.pending.lock().unwrap().remove(folder_id) else {
            return Ok(());
        };
        let mut tx = crate::db::begin_write(&self.pool)
            .await
            .map_err(|error| AppError::Transient(error.to_string()))?;
        sqlx::query(
            "INSERT OR IGNORE INTO graph_delta_links (account_id, folder_id) VALUES (?, ?)",
        )
        .bind(&self.account_id)
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;
        for id in pending.removed {
            sqlx::query(
                "DELETE FROM graph_message_map
                 WHERE account_id = ? AND folder_id = ? AND graph_id = ?",
            )
            .bind(&self.account_id)
            .bind(folder_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        for (id, uid) in pending.upserts {
            sqlx::query(
                "INSERT OR REPLACE INTO graph_message_map
                 (account_id, folder_id, graph_id, uid) VALUES (?, ?, ?, ?)",
            )
            .bind(&self.account_id)
            .bind(folder_id)
            .bind(id)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "UPDATE graph_delta_links SET delta_link = ?, next_uid = ?
             WHERE account_id = ? AND folder_id = ?",
        )
        .bind(pending.delta_link)
        .bind(pending.next_uid)
        .bind(&self.account_id)
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub fn discard_pending(&self, folder_id: &str) {
        self.pending.lock().unwrap().remove(folder_id);
    }

    /// Every UID currently mapped in the folder (the post-delta snapshot).
    pub async fn list_uids(&self, folder_id: &str) -> Result<Vec<u32>> {
        if let Some(pending) = self.pending.lock().unwrap().get(folder_id) {
            return Ok(pending
                .upserts
                .values()
                .filter_map(|uid| u32::try_from(*uid).ok())
                .collect());
        }
        let uids: Vec<i64> = sqlx::query_scalar(
            "SELECT uid FROM graph_message_map WHERE account_id = ? AND folder_id = ?",
        )
        .bind(&self.account_id)
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(uids
            .into_iter()
            .filter_map(|u| u32::try_from(u).ok())
            .collect())
    }

    /// Metadata for specific UIDs, fetched individually by Graph id.
    pub async fn fetch_by_uids(&self, folder_id: &str, uids: &[u32]) -> Result<Vec<Message>> {
        let mut out = Vec::new();
        for uid in uids {
            let graph_id: Option<String> = sqlx::query_scalar(
                "SELECT graph_id FROM graph_message_map WHERE account_id = ? AND folder_id = ? AND uid = ?",
            )
            .bind(&self.account_id)
            .bind(folder_id)
            .bind(i64::from(*uid))
            .fetch_optional(&self.pool)
            .await?;
            let Some(graph_id) = graph_id else { continue };
            let url = format!(
                "{}/me/messages/{}?$select={}",
                self.base_url,
                urlencode(&graph_id),
                MESSAGE_SELECT
            );
            match self.get(&url).await {
                Ok(item) => out.push(parse_message(&item, &graph_id, *uid)),
                Err(GraphError::Other(AppError::Provider(_))) => continue,
                Err(e) => return Err(into_app(e)),
            }
        }
        Ok(out)
    }
}

fn into_app(error: GraphError) -> AppError {
    match error {
        GraphError::Gone => AppError::Provider("Graph resource gone".into()),
        GraphError::Other(e) => e,
    }
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

fn recipients(item: &Value, key: &str) -> Vec<String> {
    item[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["emailAddress"]["address"].as_str().map(String::from))
        .collect()
}

fn parse_message(item: &Value, graph_id: &str, uid: u32) -> Message {
    let date = item["receivedDateTime"]
        .as_str()
        .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|d| d.with_timezone(&Utc));
    Message {
        id: Uuid::new_v4(),
        uid,
        message_id: item["internetMessageId"]
            .as_str()
            .or(Some(graph_id))
            .map(String::from),
        thread_id: None,
        subject: item["subject"].as_str().map(String::from),
        from_name: item["from"]["emailAddress"]["name"]
            .as_str()
            .map(String::from),
        from_email: item["from"]["emailAddress"]["address"]
            .as_str()
            .map(String::from),
        to: recipients(item, "toRecipients"),
        cc: recipients(item, "ccRecipients"),
        date,
        is_read: item["isRead"].as_bool().unwrap_or(false),
        is_flagged: item["flag"]["flagStatus"].as_str() == Some("flagged"),
        is_draft: item["isDraft"].as_bool().unwrap_or(false),
        has_attachments: item["hasAttachments"].as_bool().unwrap_or(false),
        attachments: vec![],
        preview: item["bodyPreview"].as_str().map(String::from),
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
            .expect("unscripted Graph request");
        let mut out = (StatusCode::from_u16(status).unwrap(), axum::Json(body)).into_response();
        if let Some(v) = retry_after {
            out.headers_mut().insert("retry-after", v.parse().unwrap());
        }
        out
    }

    struct Harness {
        graph: Arc<GraphDelta>,
        mock: Mock,
        base: String,
        pool: SqlitePool,
    }

    async fn harness(script: Vec<(u16, Option<&'static str>, Value)>) -> Harness {
        let mock = Mock::default();
        mock.script.lock().unwrap().extend(script);
        let app = Router::new().fallback(handler).with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (pool, _) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a','A','a@example.test','microsoft365','oauth2')")
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
            graph: Arc::new(GraphDelta::new(&base, tokens, pool.clone(), "a".into())),
            mock,
            base,
            pool,
        }
    }

    fn msg(id: &str, subject: &str, read: bool) -> Value {
        json!({"id": id, "subject": subject, "isRead": read,
               "from": {"emailAddress": {"name": "N", "address": "n@example.test"}},
               "receivedDateTime": "2026-01-02T03:04:05Z"})
    }

    fn uid_of(messages: &[Message], subject: &str) -> u32 {
        messages
            .iter()
            .find(|m| m.subject.as_deref() == Some(subject))
            .unwrap()
            .uid
    }

    #[tokio::test]
    async fn initial_sync_pages_and_stores_delta_link() {
        let h = harness(vec![]).await;
        let base = h.base.clone();
        h.mock.script.lock().unwrap().extend([
            (
                200,
                None,
                json!({"value": [msg("m1", "one", false)],
                       "@odata.nextLink": format!("{base}/page2")}),
            ),
            (
                200,
                None,
                json!({"value": [msg("m2", "two", true)],
                       "@odata.deltaLink": format!("{base}/delta?token=d1")}),
            ),
        ]);
        let messages = h.graph.sync_folder("inbox").await.unwrap();
        assert_eq!(messages.len(), 2);
        h.graph.commit_pending("inbox").await.unwrap();
        assert_eq!(h.graph.list_uids("inbox").await.unwrap().len(), 2);
        assert_eq!(
            h.graph.stored_link("inbox").await.unwrap().as_deref(),
            Some(format!("{base}/delta?token=d1").as_str())
        );
        let seen = h.mock.seen.lock().unwrap();
        assert!(seen[0]
            .0
            .starts_with("/me/mailFolders/inbox/messages/delta"));
        assert_eq!(seen[0].1.as_deref(), Some("Bearer secret-token"));
    }

    #[tokio::test]
    async fn incremental_sync_adds_updates_and_removes() {
        let h = harness(vec![]).await;
        let base = h.base.clone();
        h.mock.script.lock().unwrap().extend([
            (
                200,
                None,
                json!({"value": [msg("m1", "one", false), msg("m2", "two", false)],
                       "@odata.deltaLink": format!("{base}/delta?token=d1")}),
            ),
            (
                200,
                None,
                json!({"value": [
                        msg("m1", "one edited", true),
                        msg("m3", "three", false),
                        {"id": "m2", "@removed": {"reason": "deleted"}}],
                       "@odata.deltaLink": format!("{base}/delta?token=d2")}),
            ),
        ]);
        let first = h.graph.sync_folder("inbox").await.unwrap();
        let (uid1, uid2) = (uid_of(&first, "one"), uid_of(&first, "two"));
        h.graph.commit_pending("inbox").await.unwrap();
        let second = h.graph.sync_folder("inbox").await.unwrap();
        assert_eq!(second.len(), 2);
        let updated = second.iter().find(|m| m.uid == uid1).unwrap();
        assert_eq!(updated.subject.as_deref(), Some("one edited"));
        assert!(updated.is_read);
        h.graph.commit_pending("inbox").await.unwrap();
        let uids = h.graph.list_uids("inbox").await.unwrap();
        assert!(!uids.contains(&uid2));
        assert!(uids.contains(&uid1) && uids.len() == 2);
        assert!(
            second.iter().any(|m| m.uid > uid2),
            "new uids are never reused"
        );
        // The second run started from the stored deltaLink.
        assert!(h.mock.seen.lock().unwrap()[1]
            .0
            .starts_with("/delta?token=d1"));
        assert_eq!(
            h.graph.stored_link("inbox").await.unwrap().as_deref(),
            Some(format!("{base}/delta?token=d2").as_str())
        );
    }

    #[tokio::test]
    async fn gone_deltalink_is_dropped_and_folder_fully_resynced() {
        let h = harness(vec![]).await;
        let base = h.base.clone();
        h.mock.script.lock().unwrap().extend([
            (
                200,
                None,
                json!({"value": [msg("m1", "one", false), msg("m2", "two", false)],
                       "@odata.deltaLink": format!("{base}/delta?token=old")}),
            ),
            (410, None, json!({"error": {"code": "syncStateNotFound"}})),
            (
                200,
                None,
                json!({"value": [msg("m2", "two", false)],
                       "@odata.deltaLink": format!("{base}/delta?token=new")}),
            ),
        ]);
        let first = h.graph.sync_folder("inbox").await.unwrap();
        let (uid1, uid2) = (uid_of(&first, "one"), uid_of(&first, "two"));
        h.graph.commit_pending("inbox").await.unwrap();
        let resync = h.graph.sync_folder("inbox").await.unwrap();
        assert_eq!(resync.len(), 1);
        assert_eq!(resync[0].uid, uid2, "surviving message keeps its uid");
        h.graph.commit_pending("inbox").await.unwrap();
        let uids = h.graph.list_uids("inbox").await.unwrap();
        assert!(
            !uids.contains(&uid1),
            "message absent from the full pass is removed"
        );
        {
            let seen = h.mock.seen.lock().unwrap();
            assert!(seen[1].0.starts_with("/delta?token=old"));
            assert!(seen[2]
                .0
                .starts_with("/me/mailFolders/inbox/messages/delta"));
        }
        assert_eq!(
            h.graph.stored_link("inbox").await.unwrap().as_deref(),
            Some(format!("{base}/delta?token=new").as_str())
        );
    }

    #[tokio::test]
    async fn throttling_honours_retry_after_then_succeeds() {
        let h = harness(vec![]).await;
        let base = h.base.clone();
        h.mock.script.lock().unwrap().extend([
            (
                429,
                Some("1"),
                json!({"error": {"code": "TooManyRequests"}}),
            ),
            (
                200,
                None,
                json!({"value": [msg("m1", "one", false)],
                       "@odata.deltaLink": format!("{base}/delta?token=d1")}),
            ),
        ]);
        let started = Instant::now();
        let messages = h.graph.sync_folder("inbox").await.unwrap();
        assert_eq!(messages.len(), 1);
        h.graph.commit_pending("inbox").await.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(900));
        assert_eq!(h.mock.seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn persistent_throttling_gives_up_as_transient() {
        let h = harness(vec![]).await;
        for _ in 0..=MAX_THROTTLE_RETRIES {
            h.mock
                .script
                .lock()
                .unwrap()
                .push_back((429, Some("0"), json!({})));
        }
        let error = h.graph.sync_folder("inbox").await.unwrap_err();
        assert!(matches!(error, AppError::Transient(_)));
    }

    #[tokio::test]
    async fn failed_run_keeps_the_old_delta_link() {
        let h = harness(vec![]).await;
        let base = h.base.clone();
        h.mock.script.lock().unwrap().extend([
            (
                200,
                None,
                json!({"value": [msg("m1", "one", false)],
                       "@odata.deltaLink": format!("{base}/delta?token=d1")}),
            ),
            (500, None, json!({})),
        ]);
        h.graph.sync_folder("inbox").await.unwrap();
        h.graph.commit_pending("inbox").await.unwrap();
        assert!(h.graph.sync_folder("inbox").await.is_err());
        assert_eq!(h.graph.list_uids("inbox").await.unwrap(), vec![1]);
        assert_eq!(
            h.graph.stored_link("inbox").await.unwrap().as_deref(),
            Some(format!("{base}/delta?token=d1").as_str())
        );
    }

    #[tokio::test]
    async fn engine_write_failure_replays_old_cursor_and_discards_pending_uids() {
        use crate::providers::gmail::GmailProvider;
        let h = harness(vec![]).await;
        let base = &h.base;
        let initial = json!({"value": [msg("m1", "one", false)],
            "@odata.deltaLink": format!("{base}/delta?token=d1")});
        let update = json!({"value": [msg("m1", "edited", true), msg("m2", "two", false)],
            "@odata.deltaLink": format!("{base}/delta?token=d2")});
        h.mock.script.lock().unwrap().extend([
            (200, None, initial),
            (200, None, update.clone()),
            (200, None, update),
        ]);
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path, uid_validity) VALUES ('f','a','Inbox','inbox',1)")
            .execute(&h.pool).await.unwrap();
        let provider = Arc::new(
            GmailProvider::new_outlook("a".into(), "a@example.test".into(), h.graph.tokens.clone())
                .with_graph_delta(h.graph.clone()),
        );
        crate::imap::sync::sync_graph_test_folder(provider.clone(), h.pool.clone())
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER fail_message_write BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'injected message write failure'); END")
            .execute(&h.pool).await.unwrap();
        let error = crate::imap::sync::sync_graph_test_folder(provider.clone(), h.pool.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("injected message write failure"));
        assert!(h.graph.pending.lock().unwrap().is_empty());
        assert_eq!(h.graph.list_uids("inbox").await.unwrap(), vec![1]);
        assert_eq!(
            h.graph.stored_link("inbox").await.unwrap(),
            Some(format!("{base}/delta?token=d1"))
        );
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT uid, subject FROM messages ORDER BY uid")
                .fetch_all(&h.pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![(1, "one".into())]);
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
        assert_eq!(rows, vec![(1, "edited".into()), (2, "two".into())]);
        assert_eq!(
            h.graph.stored_link("inbox").await.unwrap(),
            Some(format!("{base}/delta?token=d2"))
        );
        let seen = h.mock.seen.lock().unwrap();
        assert_eq!(seen[1].0, "/delta?token=d1");
        assert_eq!(seen[2].0, seen[1].0);
    }

    #[tokio::test]
    async fn failed_fetch_discards_an_uncommitted_batch() {
        let h = harness(vec![]).await;
        h.mock.script.lock().unwrap().extend([
            (
                200,
                None,
                json!({"value": [msg("m1", "one", false)],
                "@odata.deltaLink": format!("{}/delta", h.base)}),
            ),
            (500, None, json!({})),
        ]);
        h.graph.sync_folder("inbox").await.unwrap();
        assert_eq!(h.graph.list_uids("inbox").await.unwrap(), vec![1]);
        assert!(h.graph.sync_folder("inbox").await.is_err());
        assert!(h.graph.list_uids("inbox").await.unwrap().is_empty());
        h.graph.commit_pending("inbox").await.unwrap();
        assert!(h.graph.stored_link("inbox").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn next_link_to_another_origin_is_refused() {
        let h = harness(vec![(
            200,
            None,
            json!({"value": [], "@odata.nextLink": "http://evil.example.test/steal"}),
        )])
        .await;
        assert!(h.graph.sync_folder("inbox").await.is_err());
        assert_eq!(h.mock.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn lists_folders_across_pages() {
        let h = harness(vec![]).await;
        let base = h.base.clone();
        h.mock.script.lock().unwrap().extend([
            (
                200,
                None,
                json!({"value": [{"id": "f1", "displayName": "Inbox", "unreadItemCount": 2, "totalItemCount": 9}],
                       "@odata.nextLink": format!("{base}/more")}),
            ),
            (200, None, json!({"value": [{"id": "f2", "displayName": "Sent"}]})),
        ]);
        let folders = h.graph.list_folders().await.unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].full_path, "f1");
        assert_eq!(folders[0].unread_count, 2);
        let _ = &h.pool;
    }

    #[test]
    fn flag_defaults_off() {
        // The variable is not set in the test environment.
        if std::env::var(FLAG_ENV).is_err() {
            assert!(!graph_sync_enabled());
        }
    }
}
