use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures::stream::{self, StreamExt};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::api::webhooks::fire_webhooks;
use crate::error::{AppError, Result};
use crate::providers::factory::provider_for_account;
use crate::providers::{IdleEvent, MailProvider};

static ACCOUNT_SYNC_LOCKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

const MAX_SYNC_CONCURRENCY: usize = 4;
const ACCOUNT_SYNC_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_BACKOFF: u64 = 30 * 60;

#[derive(Debug, Clone, Copy, Default)]
struct AccountSchedule {
    failures: u32,
    next_attempt: u64,
}

#[derive(Debug, Default)]
struct SyncSchedule {
    accounts: HashMap<String, AccountSchedule>,
}

impl SyncSchedule {
    fn due(&mut self, account_id: &str, now: u64) -> bool {
        self.accounts
            .entry(account_id.to_string())
            .or_default()
            .next_attempt
            <= now
    }

    fn success(&mut self, account_id: &str, now: u64, interval: u64) {
        self.accounts.insert(
            account_id.to_string(),
            AccountSchedule {
                failures: 0,
                next_attempt: now.saturating_add(interval),
            },
        );
    }

    fn failure(&mut self, account_id: &str, now: u64, retry_after: Option<u64>, salt: u64) -> u64 {
        let state = self.accounts.entry(account_id.to_string()).or_default();
        state.failures = state.failures.saturating_add(1);
        let exponential = 2u64.saturating_pow(state.failures.min(10)).min(MAX_BACKOFF);
        let jitter = deterministic_jitter(account_id, state.failures, salt, exponential / 4);
        let delay = retry_after
            .unwrap_or(exponential.saturating_add(jitter))
            .min(MAX_BACKOFF);
        state.next_attempt = now.saturating_add(delay);
        delay
    }
}

fn deterministic_jitter(account_id: &str, failures: u32, salt: u64, bound: u64) -> u64 {
    if bound == 0 {
        return 0;
    }
    let hash = account_id
        .bytes()
        .fold(salt ^ failures as u64, |value, byte| {
            value.wrapping_mul(31).wrapping_add(byte as u64)
        });
    hash % (bound + 1)
}

fn retry_after(error: &AppError) -> Option<u64> {
    if let AppError::RateLimited {
        retry_after_secs, ..
    } = error
    {
        return Some(*retry_after_secs);
    }
    let text = error.to_string().to_ascii_lowercase();
    let marker = "retry-after";
    let start = text.find(marker)? + marker.len();
    text[start..]
        .trim_start_matches(|character: char| character == ':' || character.is_whitespace())
        .split_whitespace()
        .next()?
        .trim_matches(|character: char| !character.is_ascii_digit())
        .parse()
        .ok()
}

/// Marks an account as synchronizing for as long as it lives. Dropping it,
/// whether the sync finished, its future was cancelled or it panicked,
/// releases the account (#19).
struct AccountSyncGuard {
    account_id: String,
}

impl AccountSyncGuard {
    fn acquire(account_id: &str) -> Result<Self> {
        let locks = ACCOUNT_SYNC_LOCKS.get_or_init(Default::default);
        let mut active = locks.lock().unwrap_or_else(|e| e.into_inner());
        if !active.insert(account_id.to_string()) {
            return Err(AppError::Conflict(format!(
                "account {account_id} is already synchronizing"
            )));
        }
        Ok(Self {
            account_id: account_id.to_string(),
        })
    }
}

impl Drop for AccountSyncGuard {
    fn drop(&mut self) {
        if let Some(locks) = ACCOUNT_SYNC_LOCKS.get() {
            locks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.account_id);
        }
    }
}

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
        let guard = AccountSyncGuard::acquire(account_id)?;
        let result = self.sync_account_unlocked(account_id, event_tx).await;
        drop(guard);
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

        let engine = Arc::new(self);
        engine.spawn_idle_watchers(None).await;
        engine.run_scheduled(None).await;
    }

    /// Run the sync loop indefinitely, broadcasting an event after each account sync.
    pub async fn run_with_events(self, event_tx: broadcast::Sender<String>) {
        info!(
            "IMAP sync engine starting (with events), interval={}s",
            self.poll_interval_secs
        );

        let engine = Arc::new(self);
        engine.spawn_idle_watchers(Some(event_tx.clone())).await;
        engine.run_scheduled(Some(event_tx)).await;
    }

    async fn spawn_idle_watchers(self: &Arc<Self>, event_tx: Option<broadcast::Sender<String>>) {
        let account_ids: Vec<String> = match sqlx::query_scalar("SELECT id FROM accounts")
            .fetch_all(&self.pool)
            .await
        {
            Ok(ids) => ids,
            Err(error) => {
                warn!("failed to list accounts for IMAP IDLE: {}", error);
                return;
            }
        };

        for account_id in account_ids {
            let engine = Arc::clone(self);
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                let provider = match provider_for_account(&engine.pool, &account_id).await {
                    Ok(provider) => provider,
                    Err(error) => {
                        warn!("IMAP IDLE unavailable for {}: {}", account_id, error);
                        return;
                    }
                };
                loop {
                    match provider.wait_for_idle(Duration::from_secs(25 * 60)).await {
                        Ok(IdleEvent::Changed) => {
                            if let Err(error) = engine
                                .sync_account_inner(&account_id, event_tx.as_ref())
                                .await
                            {
                                warn!("IDLE-triggered sync failed for {}: {}", account_id, error);
                            }
                        }
                        Ok(IdleEvent::Timeout) => {
                            if let Err(error) = engine
                                .sync_account_inner(&account_id, event_tx.as_ref())
                                .await
                            {
                                warn!("IDLE timeout sync failed for {}: {}", account_id, error);
                            }
                        }
                        Err(error) => {
                            info!("falling back to polling for {}: {}", account_id, error);
                            return;
                        }
                    }
                }
            });
        }
    }

    async fn run_scheduled(&self, event_tx: Option<broadcast::Sender<String>>) {
        let mut schedule = SyncSchedule::default();
        loop {
            let account_ids: Vec<String> = match sqlx::query_scalar("SELECT id FROM accounts")
                .fetch_all(&self.pool)
                .await
            {
                Ok(ids) => ids,
                Err(error) => {
                    error!("failed to list accounts: {}", error);
                    Vec::new()
                }
            };
            let now = monotonic_seconds();
            let due: Vec<String> = account_ids
                .into_iter()
                .filter(|id| schedule.due(id, now))
                .collect();
            let results = stream::iter(due.into_iter().map(|id| {
                let event_tx = event_tx.clone();
                async move {
                    let result = tokio::time::timeout(
                        ACCOUNT_SYNC_TIMEOUT,
                        self.sync_account_inner(&id, event_tx.as_ref()),
                    )
                    .await;
                    (id, result)
                }
            }))
            .buffer_unordered(MAX_SYNC_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
            for (id, result) in results {
                match result {
                    Ok(Ok(())) => {
                        schedule.success(&id, now, self.poll_interval_secs);
                        if let Some(tx) = &event_tx {
                            let _ = tx.send(format!(
                                "{{\"type\":\"sync_complete\",\"accountId\":\"{}\"}}",
                                id
                            ));
                        }
                    }
                    Ok(Err(error)) => {
                        let delay = schedule.failure(&id, now, retry_after(&error), now);
                        error!(
                            "failed to sync account {} (retry in {}s): {}",
                            id, delay, error
                        );
                    }
                    Err(_) => {
                        let delay = schedule.failure(&id, now, None, now);
                        error!("sync timed out for account {} (retry in {}s)", id, delay);
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(self.poll_interval_secs.max(1))).await;
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

        // One immediate transaction. A deferred upsert per folder overlaps
        // the workers and returns SQLITE_BUSY without waiting.
        let mut tx = crate::db::begin_write(&self.pool).await?;
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
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

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
        // Set once the account's credentials are refused for good (for example
        // a revoked OAuth grant). Remaining folders are skipped rather than
        // each failing against the provider, and the run reports it.
        let auth_failure: Arc<OnceLock<String>> = Arc::new(OnceLock::new());

        let errors: Vec<String> = stream::iter(db_folders)
            .map(|db_folder| {
                let provider = provider.clone();
                let pool = pool.clone();
                let account_id = account_id.clone();
                let done_counter = done_counter.clone();
                let event_tx = event_tx_owned.clone();
                let auth_failure = auth_failure.clone();
                async move {
                    let folder_name = db_folder.full_path.clone();
                    let result = if auth_failure.get().is_some() {
                        Ok(()) // skipped; the run as a whole reports the auth failure
                    } else {
                        let result = sync_folder(provider, pool, &account_id, db_folder, sync_days_limit).await;
                        if let Err(AppError::Auth(message)) = &result {
                            let _ = auth_failure.set(message.clone());
                        }
                        result
                    };
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
        if let Some(message) = auth_failure.get() {
            return Err(AppError::Auth(message.clone()));
        }
        // A folder that failed to write is missing mail, not a successful
        // sync. Dropping these errors left the account looking complete
        // while folders were lost under writer contention.
        if let Some(message) = errors.first() {
            return Err(AppError::Provider(format!(
                "{} folder(s) failed to sync: {message}",
                errors.len()
            )));
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

fn monotonic_seconds() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs()
}

#[cfg(test)]
mod schedule_tests {
    use super::*;

    struct FakeClock(u64);
    impl FakeClock {
        fn advance(&mut self, seconds: u64) {
            self.0 += seconds;
        }
    }

    #[test]
    fn failed_account_waits_for_backoff_and_fake_clock() {
        let mut clock = FakeClock(0);
        let mut schedule = SyncSchedule::default();
        assert!(schedule.due("a", clock.0));
        let delay = schedule.failure("a", clock.0, None, 0);
        assert!(delay >= 2);
        assert!(!schedule.due("a", clock.0));
        clock.advance(delay);
        assert!(schedule.due("a", clock.0));
    }

    #[test]
    fn retry_after_overrides_exponential_backoff() {
        let mut schedule = SyncSchedule::default();
        assert_eq!(schedule.failure("a", 10, Some(47), 0), 47);
        assert_eq!(schedule.accounts["a"].next_attempt, 57);
    }

    #[test]
    fn idle_events_rearm_and_timeout_still_syncs() {
        let events = [IdleEvent::Changed, IdleEvent::Timeout, IdleEvent::Changed];
        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .all(|event| matches!(event, IdleEvent::Changed | IdleEvent::Timeout)));
    }

    #[test]
    fn idle_unsupported_is_a_poll_fallback() {
        let error = AppError::Provider("IMAP server does not support IDLE".into());
        assert!(error.to_string().contains("does not support IDLE"));
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
        let mut tx = crate::db::begin_write(&pool).await?;
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

    let since = (!mailbox_reset)
        .then(|| {
            db_folder.synced_at.as_deref().and_then(|s| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|ndt| ndt.and_utc())
            })
        })
        .flatten();

    // If never synced (including after UIDVALIDITY reset) and a days limit is
    // configured, use that as the floor rather than downloading the mailbox.
    let since = since
        .or_else(|| sync_days_limit.map(|days| chrono::Utc::now() - chrono::Duration::days(days)));

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
    // With a bounded history window, only repair UIDs allocated after the
    // cache's high-water mark. An empty cache has no watermark, so its initial
    // date-windowed fetch is authoritative; otherwise this repair would fetch
    // every older UID and defeat sync_days_limit. Moves receive a new UID, so
    // subsequent move repair remains covered without scanning old history.
    let repair_watermark = sync_days_limit
        .is_some()
        .then(|| cached_uids.iter().copied().max())
        .flatten();
    let missing_uids: Vec<u32> = remote_uids
        .difference(&cached_uids)
        .filter(|uid| !fetched_uids.contains(uid))
        .filter(|uid| {
            sync_days_limit.is_none() || repair_watermark.is_some_and(|mark| **uid > mark)
        })
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
    // when multiple folders are syncing in parallel. The write lock is taken up
    // front so a concurrent writer is waited out, not reported as "database is
    // locked" (see `db::begin_write`).
    let mut tx = crate::db::begin_write(&pool).await?;
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
        let subject_thread_id = if msg.thread_id.is_none() {
            let subject = crate::threading::normalized_subject(msg.subject.as_deref());
            if let Some(subject) = subject {
                let candidates: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
                    "SELECT thread_id, subject FROM messages WHERE account_id = ? AND thread_id IS NOT NULL",
                )
                .bind(account_id)
                .fetch_all(&mut *tx)
                .await?;
                candidates
                    .into_iter()
                    .find_map(|(thread_id, candidate_subject)| {
                        (crate::threading::normalized_subject(candidate_subject.as_deref())
                            .as_ref()
                            == Some(&subject))
                        .then_some(thread_id)
                        .flatten()
                    })
            } else {
                None
            }
        } else {
            None
        };
        let thread_id = if let Some(parent_message_id) = msg
            .thread_id
            .as_deref()
            .and_then(|value| value.strip_prefix("rfc:"))
        {
            sqlx::query_scalar::<_, String>(
                r#"SELECT thread_id
                   FROM messages
                   WHERE account_id = ? AND message_id = ? AND thread_id IS NOT NULL
                   ORDER BY date, id
                   LIMIT 1"#,
            )
            .bind(account_id)
            .bind(parent_message_id)
            .fetch_optional(&mut *tx)
            .await?
            .or_else(|| Some(format!("rfc:{account_id}:{parent_message_id}")))
        } else {
            msg.thread_id.clone().or(subject_thread_id).or_else(|| {
                msg.message_id
                    .as_deref()
                    .map(|message_id| format!("rfc:{account_id}:{message_id}"))
            })
        };

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
        .bind(&thread_id)
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
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder_id = ?")
        .bind(&db_folder.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or(message_count);

    let unread: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_read = 0")
            .bind(&db_folder.id)
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(unread_count);

    sqlx::query(
        "UPDATE folders SET unread_count = ?, total_count = ?, synced_at = datetime('now') WHERE id = ?",
    )
    .bind(unread)
    .bind(total)
    .bind(&db_folder.id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Load the rules and their conditions/actions once per folder sync, then
    // reuse that snapshot for every newly inserted message.
    if !new_msgs_for_rules.is_empty() {
        let rules = crate::rules::RulesService::new(pool.clone())
            .list_rules(account_id)
            .await;
        match rules {
            Ok(rules) => {
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
                    if let Err(e) = crate::rules::apply_preloaded_rules_to_message(
                        &pool, account_id, &info.id, &fields, &rules,
                    )
                    .await
                    {
                        warn!("rules evaluation failed for message {}: {}", info.id, e);
                    }
                }
            }
            Err(e) => warn!(
                "rules loading failed for folder {}: {}",
                db_folder.full_path, e
            ),
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
            // One immediate transaction for the folder's body rows. A deferred
            // write per body races other folders' writers and returns
            // SQLITE_BUSY without waiting out busy_timeout.
            let mut tx = match crate::db::begin_write(&pool).await {
                Ok(tx) => tx,
                Err(e) => {
                    warn!(
                        "body prefetch transaction failed for {}: {}",
                        folder_path, e
                    );
                    return;
                }
            };
            let mut stored: Vec<(u32, String, Option<String>, Option<String>)> = Vec::new();
            for (uid, body) in bodies {
                if let Some(msg_id) = id_by_uid.get(&uid) {
                    if let Err(error) = crate::api::messages::cache_provider_attachments_tx(
                        &mut tx,
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
                    .execute(&mut *tx)
                    .await
                    {
                        warn!("failed to store prefetched body uid {}: {}", uid, e);
                    } else {
                        stored.push((
                            uid,
                            msg_id.clone(),
                            body.text_body.clone(),
                            body.html_body.clone(),
                        ));
                    }
                }
            }
            if let Err(e) = tx.commit().await {
                warn!("body prefetch commit failed for {}: {}", folder_path, e);
                return;
            }
            for (uid, msg_id, text_body, html_body) in stored {
                if let Err(e) =
                    crate::clean::store(&pool, &msg_id, text_body.as_deref(), html_body.as_deref())
                        .await
                {
                    warn!("failed to clean prefetched body uid {}: {}", uid, e);
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

    #[tokio::test]
    async fn cancelled_sync_releases_account_lock() {
        let account_id = "sync-lock-cancel-test";
        let (held_tx, held_rx) = tokio::sync::oneshot::channel();
        let sync = tokio::spawn(async move {
            let _guard = AccountSyncGuard::acquire(account_id).unwrap();
            held_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        held_rx.await.unwrap();
        assert!(matches!(
            AccountSyncGuard::acquire(account_id),
            Err(AppError::Conflict(_))
        ));

        sync.abort();
        assert!(sync.await.unwrap_err().is_cancelled());
        AccountSyncGuard::acquire(account_id).expect("lock released after cancellation");
    }

    #[tokio::test]
    async fn panicked_sync_releases_account_lock() {
        let account_id = "sync-lock-panic-test";
        let sync = tokio::spawn(async move {
            let _guard = AccountSyncGuard::acquire(account_id).unwrap();
            panic!("sync failed mid-flight");
        });
        assert!(sync.await.unwrap_err().is_panic());
        AccountSyncGuard::acquire(account_id).expect("lock released after panic");
    }

    #[derive(Default)]
    struct RemoteMailbox {
        uid_validity: u32,
        messages: Vec<Message>,
        incremental_uids: Vec<u32>,
        fetched_by_uid: Vec<Vec<u32>>,
        fetched_since: Vec<Option<chrono::DateTime<Utc>>>,
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
            let mut state = self.0.write().unwrap();
            state.fetched_since.push(since);
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
        async fn fetch_messages_by_uid(&self, _: &str, uids: &[u32]) -> Result<Vec<Message>> {
            let mut state = self.0.write().unwrap();
            state.fetched_by_uid.push(uids.to_vec());
            Ok(state
                .messages
                .iter()
                .filter(|message| uids.contains(&message.uid))
                .cloned()
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
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
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
    async fn fresh_folder_days_limit_does_not_repair_fetch_out_of_window_uids() {
        let (pool, _) = setup().await;
        sqlx::query("UPDATE folders SET synced_at = NULL WHERE id = 'f'")
            .execute(&pool)
            .await
            .unwrap();
        let remote = Arc::new(RwLock::new(RemoteMailbox {
            uid_validity: 1,
            messages: vec![
                message(10, Some("old@example.test")),
                message(20, Some("recent@example.test")),
            ],
            incremental_uids: vec![20],
            fetched_by_uid: Vec::new(),
            fetched_since: Vec::new(),
        }));

        sync_folder(
            Arc::new(MockProvider(remote.clone())),
            pool.clone(),
            "a",
            folder(&pool).await,
            Some(30),
        )
        .await
        .unwrap();

        assert!(
            remote.read().unwrap().fetched_by_uid.is_empty(),
            "the repair path must not bypass the initial date window"
        );
        let uids: Vec<i64> = sqlx::query_scalar("SELECT uid FROM messages WHERE folder_id='f'")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(uids, vec![20]);

        remote
            .write()
            .unwrap()
            .messages
            .push(message(30, Some("moved@example.test")));
        sync_folder(
            Arc::new(MockProvider(remote.clone())),
            pool.clone(),
            "a",
            folder(&pool).await,
            Some(30),
        )
        .await
        .unwrap();

        assert_eq!(remote.read().unwrap().fetched_by_uid, vec![vec![30]]);
        let uids: Vec<i64> = sqlx::query_scalar("SELECT uid FROM messages WHERE folder_id='f'")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(uids, vec![20, 30]);
    }

    #[tokio::test]
    async fn sync_contract_handles_missing_message_id_repeat_and_remote_delete() {
        let (pool, db_folder) = setup().await;
        let remote = Arc::new(RwLock::new(RemoteMailbox {
            uid_validity: 1,
            messages: vec![message(10, None), message(11, Some("same@example.test"))],
            incremental_uids: vec![10, 11],
            fetched_by_uid: Vec::new(),
            fetched_since: Vec::new(),
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
            fetched_by_uid: Vec::new(),
            fetched_since: Vec::new(),
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
            fetched_by_uid: Vec::new(),
            fetched_since: Vec::new(),
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

    #[tokio::test]
    async fn uidvalidity_reset_uses_days_limit_instead_of_previous_sync_time() {
        let (pool, db_folder) = setup().await;
        let remote = Arc::new(RwLock::new(RemoteMailbox {
            uid_validity: 2,
            messages: vec![
                message(1, Some("old@example.test")),
                message(2, Some("recent@example.test")),
            ],
            incremental_uids: vec![2],
            fetched_by_uid: Vec::new(),
            fetched_since: Vec::new(),
        }));

        sync_folder(
            Arc::new(MockProvider(remote.clone())),
            pool.clone(),
            "a",
            db_folder,
            Some(30),
        )
        .await
        .unwrap();

        let state = remote.read().unwrap();
        let since = state.fetched_since[0].expect("days limit supplies a fetch floor");
        assert!(
            since <= Utc::now() - chrono::Duration::days(29),
            "reset must not reuse the previous synced_at"
        );
        assert!(
            state.fetched_by_uid.is_empty(),
            "reset must not repair-fetch messages outside the days limit"
        );
        drop(state);
        let uids: Vec<i64> = sqlx::query_scalar("SELECT uid FROM messages WHERE folder_id='f'")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(uids, vec![2]);
    }

    // ── OAuth token lifetime across a long run ───────────────────────────────

    use crate::auth::token_source::testing::{FakeClock, FakeOAuth};
    use crate::auth::token_source::{with_fresh_token, AccessTokenSource};
    use crate::providers::gmail::is_credential_rejection;
    use std::sync::atomic::AtomicUsize;

    /// Something the fake server does part-way through a run.
    type ServerAction = Box<dyn Fn(&FakeOAuth) + Send + Sync>;

    /// A mailbox of empty folders behind an XOAUTH2 server. Like
    /// `GmailProvider`, every operation opens its own connection, which takes
    /// its token from the shared source; each connection costs `step` seconds
    /// of fake time.
    struct OAuthMailbox {
        oauth: Arc<FakeOAuth>,
        tokens: AccessTokenSource,
        folders: usize,
        step: i64,
        connects: AtomicUsize,
        /// Before connection number N, run this against the fake server.
        on_connect: Option<(usize, ServerAction)>,
    }

    impl OAuthMailbox {
        async fn connect(&self) -> Result<()> {
            let n = self.connects.fetch_add(1, Ordering::SeqCst);
            if let Some((at, action)) = &self.on_connect {
                if n == *at {
                    action(self.oauth.as_ref());
                }
            }
            with_fresh_token(&self.tokens, is_credential_rejection, |token| {
                let result = self.oauth.authenticate(&token);
                async move { result }
            })
            .await?;
            self.oauth.clock.advance(self.step);
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl MailProvider for OAuthMailbox {
        fn provider_id(&self) -> &str {
            "gmail"
        }
        async fn authenticate(&mut self) -> Result<()> {
            Ok(())
        }
        async fn list_folders(&self) -> Result<Vec<Folder>> {
            self.connect().await?;
            Ok((0..self.folders)
                .map(|i| Folder {
                    id: Uuid::new_v4(),
                    name: format!("Label {i}"),
                    full_path: format!("Label {i}"),
                    special_use: None,
                    unread_count: 0,
                    total_count: 0,
                })
                .collect())
        }
        async fn folder_uid_validity(&self, _: &str) -> Result<Option<u32>> {
            self.connect().await?;
            Ok(Some(1))
        }
        async fn fetch_messages(
            &self,
            _: &str,
            _: Option<chrono::DateTime<Utc>>,
        ) -> Result<Vec<Message>> {
            self.connect().await?;
            Ok(Vec::new())
        }
        async fn list_message_uids(&self, _: &str) -> Result<Vec<u32>> {
            self.connect().await?;
            Ok(Vec::new())
        }
        async fn fetch_message_body(&self, _: &str, _: u32) -> Result<MessageBody> {
            Err(AppError::NotFound("no bodies".into()))
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

    /// 202 folders like the prod account, each costing three connections of
    /// `step` seconds, against tokens that live 3600s. The cache is an
    /// in-memory database: with instant fake IMAP the run is purely
    /// write-bound, and these tests are about tokens, not SQLite contention.
    async fn oauth_run(
        step: i64,
        on_connect: Option<(usize, ServerAction)>,
    ) -> (SqlitePool, Arc<FakeOAuth>, Result<()>) {
        let (pool, _) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a','A','a@example.test','gmail','oauth2')")
            .execute(&pool).await.unwrap();
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock, 3600);
        let provider = Arc::new(OAuthMailbox {
            tokens: oauth.source(&format!("a-{}", Uuid::new_v4())),
            oauth: oauth.clone(),
            folders: 202,
            step,
            connects: AtomicUsize::new(0),
            on_connect,
        });
        let result = ImapSyncEngine::new(pool.clone(), 60)
            .sync_provider("a", provider, None, None)
            .await;
        (pool, oauth, result)
    }

    async fn synced_labels(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM folders WHERE full_path LIKE 'Label %' AND synced_at IS NOT NULL",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn oauth_sync_run_that_outlives_its_token_keeps_succeeding() {
        // 1 + 202 * 3 connections at 10s each: ~6070s, well past the first
        // token's 3600s. Before the per-connection refresh, every folder after ~3600s failed.
        let (pool, oauth, result) = oauth_run(10, None).await;
        result.unwrap();
        assert_eq!(synced_labels(&pool).await, 202);
        assert_eq!(
            oauth.rejections.load(Ordering::SeqCst),
            0,
            "the token is refreshed before it expires, not after a rejection"
        );
        assert_eq!(
            oauth.refresh_count(),
            1,
            "one refresh for the one expiry crossed, shared by the four workers"
        );
    }

    #[tokio::test]
    async fn oauth_sync_auth_rejection_refreshes_and_retries_once() {
        // Google invalidates the live token early, part-way through the run.
        let revoke: ServerAction =
            Box::new(|oauth: &FakeOAuth| oauth.revoke_token(&oauth.current().access_token));
        let (pool, oauth, result) = oauth_run(1, Some((100, revoke))).await;
        result.unwrap();
        assert_eq!(
            synced_labels(&pool).await,
            202,
            "no folder records an error"
        );
        assert_eq!(oauth.refresh_count(), 1, "exactly one forced refresh");
        let rejections = oauth.rejections.load(Ordering::SeqCst);
        assert!(rejections >= 1);
        assert_eq!(
            oauth.auth_calls.load(Ordering::SeqCst),
            1 + 202 * 3 + rejections,
            "each rejected connection is retried exactly once"
        );
    }

    #[tokio::test]
    async fn oauth_sync_revoked_grant_reports_reconnect_needed() {
        let revoke: ServerAction = Box::new(|oauth: &FakeOAuth| {
            oauth.revoke_token(&oauth.current().access_token);
            oauth.revoked_grant.store(true, Ordering::SeqCst);
        });
        let (pool, oauth, result) = oauth_run(1, Some((100, revoke))).await;
        let error = result.unwrap_err();
        assert!(
            matches!(&error, AppError::Auth(message) if message.contains("invalid_grant")),
            "a revoked grant must surface as the reconnect-needed auth error, got {error}"
        );
        assert_eq!(
            oauth.refresh_count(),
            1,
            "the refused refresh is not repeated by every folder"
        );
        assert!(synced_labels(&pool).await < 202);
    }

    /// File-backed WAL, 202 folders, 4 workers, instant fake IMAP. This is
    /// the case that returned "database is locked" about one run in three
    /// when writers used a deferred BEGIN.
    #[tokio::test]
    async fn parallel_folder_sync_on_a_file_db_never_reports_database_is_locked() {
        let path = std::env::temp_dir().join(format!("email-rs-wi587-{}.db", Uuid::new_v4()));
        let url = format!("sqlite://{}", path.display());
        let (pool, _) = crate::db::create_pool(&url).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a','A','a@example.test','gmail','oauth2')")
            .execute(&pool).await.unwrap();
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock, 3600);
        let provider = Arc::new(OAuthMailbox {
            tokens: oauth.source(&format!("a-{}", Uuid::new_v4())),
            oauth,
            folders: 202,
            step: 0,
            connects: AtomicUsize::new(0),
            on_connect: None,
        });
        let result = ImapSyncEngine::new(pool.clone(), 60)
            .sync_provider("a", provider, None, None)
            .await;
        let error = result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        assert!(
            !error.to_lowercase().contains("database is locked"),
            "file-backed parallel sync must not report a lock: {error}"
        );
        assert!(error.is_empty(), "sync failed: {error}");
        assert_eq!(synced_labels(&pool).await, 202);
    }
}
