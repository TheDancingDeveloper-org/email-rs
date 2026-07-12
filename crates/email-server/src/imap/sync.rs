use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use futures::stream::{self, StreamExt};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::api::webhooks::fire_webhooks;
use crate::error::{AppError, Result};
use crate::providers::factory::provider_for_account;
use crate::providers::MailProvider;

static ACCOUNT_SYNC_LOCKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Background IMAP sync engine.
/// Periodically polls all configured accounts and updates the local SQLite cache.
pub struct ImapSyncEngine {
    pool: SqlitePool,
    poll_interval_secs: u64,
}

// ── DB row type for account loading ──────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct AccountRecord {
    #[allow(dead_code)]
    id: String,
    provider_type: String,
    sync_days_limit: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct FolderRecord {
    id: String,
    #[allow(dead_code)]
    name: String,
    full_path: String,
    synced_at: Option<String>,
    is_excluded: bool,
    uid_validity: i64,
}

// ── ImapSyncEngine ────────────────────────────────────────────────────────────

impl ImapSyncEngine {
    pub fn new(pool: SqlitePool, poll_interval_secs: u64) -> Self {
        Self {
            pool,
            poll_interval_secs,
        }
    }

    /// Sync a single account by id.
    pub async fn sync_account(&self, account_id: &str) -> Result<()> {
        self.sync_account_inner(account_id, None).await
    }

    async fn sync_account_inner(
        &self,
        account_id: &str,
        event_tx: Option<&broadcast::Sender<String>>,
    ) -> Result<()> {
        let locks = ACCOUNT_SYNC_LOCKS.get_or_init(|| Mutex::new(HashSet::new()));
        {
            let mut active = locks.lock().await;
            if !active.insert(account_id.to_string()) {
                return Err(AppError::Conflict(format!(
                    "account {account_id} is already synchronizing"
                )));
            }
        }
        let result = self.sync_account_unlocked(account_id, event_tx).await;
        locks.lock().await.remove(account_id);
        if result.is_ok() {
            fire_webhooks(
                &self.pool,
                account_id,
                "sync.completed",
                serde_json::json!({"status": "completed"}),
            )
            .await;
        }
        result
    }

    async fn sync_account_unlocked(
        &self,
        account_id: &str,
        event_tx: Option<&broadcast::Sender<String>>,
    ) -> Result<()> {
        info!("syncing account {}", account_id);

        // 1. Load account from DB.
        let account = sqlx::query_as::<_, AccountRecord>(
            r#"SELECT id, provider_type, sync_days_limit
               FROM accounts WHERE id = ?"#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("account {} not found", account_id)))?;

        let provider = provider_for_account(&self.pool, account_id).await?;
        self.sync_provider(account_id, provider, event_tx, account.sync_days_limit)
            .await?;

        if account.provider_type == "gmail" {
            let pool = self.pool.clone();
            let aid = account_id.to_string();
            tokio::spawn(async move {
                if let Err(error) = crate::calendar::sync::sync_gmail_calendar(&pool, &aid).await {
                    warn!("calendar sync failed for account {}: {}", aid, error);
                }
            });
        }

        Ok(())
    }

    /// Run the sync loop indefinitely.
    #[allow(dead_code)]
    pub async fn run(self) {
        info!(
            "IMAP sync engine starting, interval={}s",
            self.poll_interval_secs
        );

        loop {
            if let Err(e) = self.sync_all_accounts().await {
                error!("sync cycle error: {}", e);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    /// Run the sync loop indefinitely, broadcasting an event after each account sync.
    pub async fn run_with_events(self, event_tx: broadcast::Sender<String>) {
        info!(
            "IMAP sync engine starting (with events), interval={}s",
            self.poll_interval_secs
        );

        loop {
            let account_ids: Vec<String> = match sqlx::query_scalar("SELECT id FROM accounts")
                .fetch_all(&self.pool)
                .await
            {
                Ok(ids) => ids,
                Err(e) => {
                    error!("failed to list accounts: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(self.poll_interval_secs))
                        .await;
                    continue;
                }
            };

            for id in account_ids {
                if let Err(e) = self.sync_account_inner(&id, Some(&event_tx)).await {
                    error!("failed to sync account {}: {}", id, e);
                } else {
                    let _ = event_tx.send(format!(
                        "{{\"type\":\"sync_complete\",\"accountId\":\"{}\"}}",
                        id
                    ));
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    #[allow(dead_code)]
    async fn sync_all_accounts(&self) -> Result<()> {
        let account_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM accounts")
            .fetch_all(&self.pool)
            .await?;

        for id in account_ids {
            if let Err(e) = self.sync_account(&id).await {
                error!("failed to sync account {}: {}", id, e);
            }
        }
        Ok(())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Sync folders and messages for a provider, fetching up to 4 folders in parallel.
    async fn sync_provider(
        &self,
        account_id: &str,
        provider: Arc<dyn MailProvider>,
        event_tx: Option<&broadcast::Sender<String>>,
        sync_days_limit: Option<i64>,
    ) -> Result<()> {
        // List folders and upsert (no synced_at — leave NULL for first-time folders
        // so the first fetch uses "ALL" and pulls full history).
        let folders = provider.list_folders().await?;

        for folder in &folders {
            let folder_id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"INSERT INTO folders (id, account_id, name, full_path, special_use)
                   VALUES (?, ?, ?, ?, ?)
                   ON CONFLICT(account_id, full_path) DO UPDATE SET
                       name        = excluded.name,
                       special_use = excluded.special_use"#,
            )
            .bind(&folder_id)
            .bind(account_id)
            .bind(&folder.name)
            .bind(&folder.full_path)
            .bind(&folder.special_use)
            .execute(&self.pool)
            .await?;
        }

        // Reload folder records so we have the actual DB ids; skip excluded folders.
        let db_folders: Vec<FolderRecord> = sqlx::query_as::<_, FolderRecord>(
            "SELECT id, name, full_path, synced_at, is_excluded, uid_validity FROM folders WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter(|f| !f.is_excluded)
        .collect();

        let total = db_folders.len();
        info!(
            "syncing {} folders for account {} (4 in parallel)",
            total, account_id
        );

        if let Some(tx) = event_tx {
            let _ = tx.send(format!(
                "{{\"type\":\"sync_start\",\"accountId\":\"{}\",\"folderCount\":{}}}",
                account_id, total
            ));
        }

        let pool = self.pool.clone();
        let account_id = account_id.to_string();
        let done_counter = Arc::new(AtomicUsize::new(0));
        let event_tx_owned: Option<broadcast::Sender<String>> = event_tx.cloned();

        let errors: Vec<String> = stream::iter(db_folders)
            .map(|db_folder| {
                let provider = provider.clone();
                let pool = pool.clone();
                let account_id = account_id.clone();
                let done_counter = done_counter.clone();
                let event_tx = event_tx_owned.clone();
                async move {
                    let folder_name = db_folder.full_path.clone();
                    let result = sync_folder(provider, pool, &account_id, db_folder, sync_days_limit).await;
                    let done = done_counter.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Some(tx) = &event_tx {
                        let _ = tx.send(format!(
                            "{{\"type\":\"sync_folder_done\",\"accountId\":\"{}\",\"folder\":\"{}\",\"done\":{},\"total\":{}}}",
                            account_id, folder_name, done, total
                        ));
                    }
                    result.err().map(|e| e.to_string())
                }
            })
            .buffer_unordered(4)
            .filter_map(|r| async move { r })
            .collect()
            .await;

        for e in &errors {
            error!("folder sync error: {}", e);
        }

        if errors.is_empty() {
            if let Err(error) = crate::db::mark_account_reconciled(&pool, &account_id).await {
                warn!(
                    "failed to record reconciliation completion for account {}: {}",
                    account_id, error
                );
            }
        }

        Ok(())
    }
}

/// Fetch and store all messages for a single folder. Runs in parallel with other folders.
async fn sync_folder(
    provider: Arc<dyn MailProvider>,
    pool: SqlitePool,
    account_id: &str,
    db_folder: FolderRecord,
    sync_days_limit: Option<i64>,
) -> crate::error::Result<()> {
    let remote_uid_validity = provider
        .folder_uid_validity(&db_folder.full_path)
        .await?
        .map(i64::from)
        .unwrap_or(db_folder.uid_validity);
    let mailbox_reset = remote_uid_validity != db_folder.uid_validity;
    if mailbox_reset {
        info!(
            "UIDVALIDITY changed for {} ({} -> {}), rebuilding folder cache",
            db_folder.full_path, db_folder.uid_validity, remote_uid_validity
        );
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM messages WHERE folder_id = ?")
            .bind(&db_folder.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE folders SET uid_validity = ?, synced_at = NULL, unread_count = 0, total_count = 0 WHERE id = ?",
        )
        .bind(remote_uid_validity)
        .bind(&db_folder.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }

    let since = db_folder.synced_at.as_deref().and_then(|s| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|ndt| ndt.and_utc())
    });

    // If never synced and a days limit is configured, use that as the floor.
    let since = if mailbox_reset {
        None
    } else {
        since.or_else(|| {
            sync_days_limit.map(|days| chrono::Utc::now() - chrono::Duration::days(days))
        })
    };

    let mut messages = provider.fetch_messages(&db_folder.full_path, since).await?;
    let remote_uids: std::collections::HashSet<u32> = provider
        .list_message_uids(&db_folder.full_path)
        .await?
        .into_iter()
        .collect();

    // SINCE is based on a message's internal date, so an old message moved
    // into this mailbox can have a new UID without appearing in that search.
    // Explicitly fetch every remote UID missing from both the cache and the
    // incremental result so moves are repaired on the next sync.
    let cached_uids: std::collections::HashSet<u32> = sqlx::query_scalar::<_, i64>(
        "SELECT uid FROM messages WHERE folder_id = ? AND uid_validity = ?",
    )
    .bind(&db_folder.id)
    .bind(remote_uid_validity)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .filter_map(|uid| u32::try_from(uid).ok())
    .collect();
    let fetched_uids: std::collections::HashSet<u32> =
        messages.iter().map(|message| message.uid).collect();
    let missing_uids: Vec<u32> = remote_uids
        .difference(&cached_uids)
        .filter(|uid| !fetched_uids.contains(uid))
        .copied()
        .collect();
    if !missing_uids.is_empty() {
        messages.extend(
            provider
                .fetch_messages_by_uid(&db_folder.full_path, &missing_uids)
                .await?,
        );
    }

    let message_count = messages.len() as i64;
    let mut unread_count: i64 = 0;
    let mut new_message_payloads: Vec<serde_json::Value> = Vec::new();

    // Collect new message IDs and their fields for rules evaluation after commit.
    struct NewMsgInfo {
        id: String,
        subject: Option<String>,
        from_name: Option<String>,
        from_email: Option<String>,
        to_json: Option<String>,
        preview: Option<String>,
        is_read: bool,
        is_flagged: bool,
        has_attachments: bool,
        date: Option<String>,
    }
    let mut new_msgs_for_rules: Vec<NewMsgInfo> = Vec::new();

    // Batch all inserts in a single transaction to reduce SQLite write-lock contention
    // when multiple folders are syncing in parallel.
    let mut tx = pool.begin().await?;
    for msg in &messages {
        // The complete UID snapshot is taken after metadata fetching. Do not
        // resurrect a message that disappeared between those two operations.
        if !remote_uids.contains(&msg.uid) {
            continue;
        }
        if !msg.is_read {
            unread_count += 1;
        }
        let msg_id = Uuid::new_v4().to_string();
        let to_json = serde_json::to_string(&msg.to).ok();
        let date_str = msg.date.map(|d| d.to_rfc3339());
        let logical_id = crate::db::logical_message_id(
            account_id,
            msg.message_id.as_deref(),
            msg.from_email.as_deref(),
            date_str.as_deref(),
            msg.subject.as_deref(),
            Some(&msg_id),
        );

        let stored_id: String = sqlx::query_scalar(
            r#"INSERT INTO messages
                   (id, logical_id, account_id, folder_id, uid, uid_validity, message_id, thread_id,
                    subject, from_name, from_email, to_json, date,
                    is_read, is_flagged, is_draft, has_attachments, preview, synced_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
               ON CONFLICT(folder_id, uid_validity, uid) DO UPDATE SET
                   logical_id      = excluded.logical_id,
                   message_id      = excluded.message_id,
                   thread_id       = excluded.thread_id,
                   subject         = excluded.subject,
                   from_name       = excluded.from_name,
                   from_email      = excluded.from_email,
                   to_json         = excluded.to_json,
                   date            = excluded.date,
                   is_read         = excluded.is_read,
                   is_flagged      = excluded.is_flagged,
                   is_draft        = excluded.is_draft,
                   has_attachments = excluded.has_attachments,
                   preview         = excluded.preview,
                   synced_at       = excluded.synced_at
               RETURNING id"#,
        )
        .bind(&msg_id)
        .bind(&logical_id)
        .bind(account_id)
        .bind(&db_folder.id)
        .bind(msg.uid as i64)
        .bind(remote_uid_validity)
        .bind(&msg.message_id)
        .bind(&msg.thread_id)
        .bind(&msg.subject)
        .bind(&msg.from_name)
        .bind(&msg.from_email)
        .bind(&to_json)
        .bind(&date_str)
        .bind(msg.is_read)
        .bind(msg.is_flagged)
        .bind(msg.is_draft)
        .bind(msg.has_attachments)
        .bind(&msg.preview)
        .fetch_one(&mut *tx)
        .await?;

        crate::api::messages::cache_provider_attachments_tx(&mut tx, &stored_id, &msg.attachments)
            .await?;

        // Track genuinely new rows for webhook dispatch and rules evaluation.
        if stored_id == msg_id {
            new_message_payloads.push(serde_json::json!({
                "eventId": Uuid::new_v4(),
                "logicalId": logical_id,
                "occurrenceId": stored_id,
                "subject": msg.subject,
                "from": msg.from_email,
                "fromName": msg.from_name,
                "folder": db_folder.full_path,
                "date": date_str,
            }));

            new_msgs_for_rules.push(NewMsgInfo {
                id: msg_id,
                subject: msg.subject.clone(),
                from_name: msg.from_name.clone(),
                from_email: msg.from_email.clone(),
                to_json: to_json.clone(),
                preview: msg.preview.clone(),
                is_read: msg.is_read,
                is_flagged: msg.is_flagged,
                has_attachments: msg.has_attachments,
                date: date_str,
            });
        }
    }

    let cached_occurrences: Vec<(String, i64)> =
        sqlx::query_as("SELECT id, uid FROM messages WHERE folder_id = ? AND uid_validity = ?")
            .bind(&db_folder.id)
            .bind(remote_uid_validity)
            .fetch_all(&mut *tx)
            .await?;
    for (id, uid) in cached_occurrences {
        if !remote_uids.contains(&(uid as u32)) {
            sqlx::query("DELETE FROM messages WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;

    // Apply rules to newly inserted messages now that the transaction is committed.
    for info in &new_msgs_for_rules {
        let fields = crate::rules::MessageFields {
            subject: info.subject.as_deref(),
            from_name: info.from_name.as_deref(),
            from_email: info.from_email.as_deref(),
            to_json: info.to_json.as_deref(),
            preview: info.preview.as_deref(),
            is_read: info.is_read,
            is_flagged: info.is_flagged,
            has_attachments: info.has_attachments,
            date: info.date.as_deref(),
        };
        if let Err(e) =
            crate::rules::apply_rules_to_message(&pool, account_id, &info.id, &fields).await
        {
            warn!("rules evaluation failed for message {}: {}", info.id, e);
        }
    }

    // Persist message.created events for genuinely inserted messages.
    if !new_message_payloads.is_empty() {
        let pool_ref = pool.clone();
        let account_id_str = account_id.to_string();
        tokio::spawn(async move {
            for payload in new_message_payloads {
                fire_webhooks(&pool_ref, &account_id_str, "message.created", payload).await;
            }
        });
    }

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder_id = ?")
        .bind(&db_folder.id)
        .fetch_one(&pool)
        .await
        .unwrap_or(message_count);

    let unread: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_read = 0")
            .bind(&db_folder.id)
            .fetch_one(&pool)
            .await
            .unwrap_or(unread_count);

    sqlx::query(
        "UPDATE folders SET unread_count = ?, total_count = ?, synced_at = datetime('now') WHERE id = ?",
    )
    .bind(unread)
    .bind(total)
    .bind(&db_folder.id)
    .execute(&pool)
    .await?;

    // Background: pre-fetch bodies for the most recent uncached messages so they open instantly.
    let pf_provider = provider;
    let pf_pool = pool;
    let pf_folder_id = db_folder.id;
    let pf_folder_path = db_folder.full_path;
    tokio::spawn(async move {
        prefetch_uncached_bodies(pf_provider, pf_pool, &pf_folder_id, &pf_folder_path).await;
    });

    Ok(())
}

/// Pre-fetch and cache bodies for up to PREFETCH_LIMIT recent messages that have no cached body.
const PREFETCH_LIMIT: i64 = 50;

async fn prefetch_uncached_bodies(
    provider: Arc<dyn MailProvider>,
    pool: SqlitePool,
    folder_id: &str,
    folder_path: &str,
) {
    #[derive(sqlx::FromRow)]
    struct UncachedMsg {
        id: String,
        uid: i64,
    }

    let uncached = match sqlx::query_as::<_, UncachedMsg>(
        r#"SELECT m.id, m.uid FROM messages m
           LEFT JOIN message_bodies mb ON mb.message_id = m.id
           WHERE m.folder_id = ? AND mb.message_id IS NULL
           ORDER BY m.date DESC
           LIMIT ?"#,
    )
    .bind(folder_id)
    .bind(PREFETCH_LIMIT)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn!("body prefetch query failed for {}: {}", folder_path, e);
            return;
        }
    };

    if uncached.is_empty() {
        return;
    }

    let uids: Vec<u32> = uncached.iter().map(|r| r.uid as u32).collect();
    let id_by_uid: std::collections::HashMap<u32, String> =
        uncached.into_iter().map(|r| (r.uid as u32, r.id)).collect();

    info!(
        "pre-fetching {} bodies for folder {}",
        uids.len(),
        folder_path
    );

    match provider.fetch_bodies_batch(folder_path, &uids).await {
        Ok(bodies) => {
            let fetched = bodies.len();
            for (uid, body) in bodies {
                if let Some(msg_id) = id_by_uid.get(&uid) {
                    if let Err(error) = crate::api::messages::cache_provider_attachments(
                        &pool,
                        msg_id,
                        &body.attachments,
                    )
                    .await
                    {
                        warn!("failed to cache attachments for uid {}: {}", uid, error);
                    }
                    if let Err(e) = sqlx::query(
                        r#"INSERT INTO message_bodies (message_id, html_body, text_body, raw_headers)
                           VALUES (?, ?, ?, ?)
                           ON CONFLICT(message_id) DO NOTHING"#,
                    )
                    .bind(msg_id)
                    .bind(&body.html_body)
                    .bind(&body.text_body)
                    .bind(&body.raw_headers)
                    .execute(&pool)
                    .await
                    {
                        warn!("failed to store prefetched body uid {}: {}", uid, e);
                    }
                }
            }
            info!("pre-fetched {} bodies for folder {}", fetched, folder_path);
        }
        Err(e) => {
            warn!("body pre-fetch failed for folder {}: {}", folder_path, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use chrono::Utc;

    use super::*;
    use crate::providers::{Folder, Message, MessageBody};

    #[derive(Default)]
    struct RemoteMailbox {
        uid_validity: u32,
        messages: Vec<Message>,
        incremental_uids: Vec<u32>,
    }

    struct MockProvider(Arc<RwLock<RemoteMailbox>>);

    #[async_trait::async_trait]
    impl MailProvider for MockProvider {
        fn provider_id(&self) -> &str {
            "mock"
        }
        async fn authenticate(&mut self) -> Result<()> {
            Ok(())
        }
        async fn list_folders(&self) -> Result<Vec<Folder>> {
            Ok(Vec::new())
        }
        async fn folder_uid_validity(&self, _: &str) -> Result<Option<u32>> {
            Ok(Some(self.0.read().unwrap().uid_validity))
        }
        async fn fetch_messages(
            &self,
            _: &str,
            since: Option<chrono::DateTime<Utc>>,
        ) -> Result<Vec<Message>> {
            let state = self.0.read().unwrap();
            if since.is_none() {
                return Ok(state.messages.clone());
            }
            Ok(state
                .messages
                .iter()
                .filter(|m| state.incremental_uids.contains(&m.uid))
                .cloned()
                .collect())
        }
        async fn list_message_uids(&self, _: &str) -> Result<Vec<u32>> {
            Ok(self
                .0
                .read()
                .unwrap()
                .messages
                .iter()
                .map(|m| m.uid)
                .collect())
        }
        async fn fetch_message_body(&self, _: &str, _: u32) -> Result<MessageBody> {
            Ok(MessageBody {
                message_id: "mock".into(),
                html_body: None,
                text_body: None,
                raw_headers: None,
                attachments: Vec::new(),
            })
        }
        async fn fetch_bodies_batch(&self, _: &str, _: &[u32]) -> Result<Vec<(u32, MessageBody)>> {
            Ok(Vec::new())
        }
        async fn mark_seen(&self, _: &str, _: u32) -> Result<()> {
            Ok(())
        }
        async fn mark_unseen(&self, _: &str, _: u32) -> Result<()> {
            Ok(())
        }
        async fn delete_message(&self, _: &str, _: u32) -> Result<()> {
            Ok(())
        }
        async fn move_message(&self, _: &str, _: u32, _: &str) -> Result<()> {
            Ok(())
        }
        async fn set_flagged(&self, _: &str, _: u32, _: bool) -> Result<()> {
            Ok(())
        }
    }

    fn message(uid: u32, message_id: Option<&str>) -> Message {
        Message {
            id: Uuid::new_v4(),
            uid,
            message_id: message_id.map(str::to_string),
            thread_id: None,
            subject: Some(format!("message {uid}")),
            from_name: None,
            from_email: Some("sender@example.test".into()),
            to: vec![],
            cc: vec![],
            date: Some(Utc::now()),
            is_read: false,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            attachments: Vec::new(),
            preview: None,
        }
    }

    async fn setup() -> (SqlitePool, FolderRecord) {
        let path = format!("/tmp/email_sync_contract_{}.db", Uuid::new_v4());
        let (pool, _) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a','A','a@example.test','test','basic')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path, uid_validity, synced_at) VALUES ('f','a','Inbox','INBOX',1,datetime('now'))")
            .execute(&pool).await.unwrap();
        let folder = sqlx::query_as("SELECT id, name, full_path, synced_at, is_excluded, uid_validity FROM folders WHERE id='f'")
            .fetch_one(&pool).await.unwrap();
        (pool, folder)
    }

    async fn folder(pool: &SqlitePool) -> FolderRecord {
        sqlx::query_as("SELECT id, name, full_path, synced_at, is_excluded, uid_validity FROM folders WHERE id='f'")
            .fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn sync_contract_handles_missing_message_id_repeat_and_remote_delete() {
        let (pool, db_folder) = setup().await;
        let remote = Arc::new(RwLock::new(RemoteMailbox {
            uid_validity: 1,
            messages: vec![message(10, None), message(11, Some("same@example.test"))],
            incremental_uids: vec![10, 11],
        }));
        let provider: Arc<dyn MailProvider> = Arc::new(MockProvider(remote.clone()));
        sync_folder(provider.clone(), pool.clone(), "a", db_folder, None)
            .await
            .unwrap();
        sync_folder(
            provider.clone(),
            pool.clone(),
            "a",
            folder(&pool).await,
            None,
        )
        .await
        .unwrap();
        let logical_ids: Vec<String> =
            sqlx::query_scalar("SELECT logical_id FROM messages WHERE folder_id='f' ORDER BY uid")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(logical_ids.len(), 2);
        assert!(logical_ids.iter().all(|id| id.starts_with("lm_")));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder_id='f'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "repeat sync must upsert mailbox identity");
        remote.write().unwrap().messages.remove(0);
        sync_folder(provider, pool.clone(), "a", folder(&pool).await, None)
            .await
            .unwrap();
        let uids: Vec<i64> = sqlx::query_scalar("SELECT uid FROM messages WHERE folder_id='f'")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(uids, vec![11]);
    }

    #[tokio::test]
    async fn sync_contract_discovers_old_message_moved_into_folder_and_resets_uidvalidity() {
        let (pool, db_folder) = setup().await;
        let remote = Arc::new(RwLock::new(RemoteMailbox {
            uid_validity: 1,
            messages: vec![message(20, Some("moved@example.test"))],
            incremental_uids: Vec::new(),
        }));
        let provider: Arc<dyn MailProvider> = Arc::new(MockProvider(remote.clone()));
        sync_folder(provider.clone(), pool.clone(), "a", db_folder, None)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE uid=20")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "missing UID metadata must be fetched independent of SINCE"
        );

        let mut replacement = message(1, Some("replacement@example.test"));
        replacement.subject = Some("replacement".into());
        *remote.write().unwrap() = RemoteMailbox {
            uid_validity: 2,
            messages: vec![replacement],
            incremental_uids: Vec::new(),
        };
        sync_folder(provider, pool.clone(), "a", folder(&pool).await, None)
            .await
            .unwrap();
        let rows: Vec<(i64, i64, Option<String>)> =
            sqlx::query_as("SELECT uid, uid_validity, subject FROM messages WHERE folder_id='f'")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![(1, 2, Some("replacement".into()))]);
    }
}
