use std::time::Duration;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

#[derive(sqlx::FromRow)]
struct LogicalIdentityRow {
    id: String,
    account_id: String,
    message_id: Option<String>,
    from_email: Option<String>,
    date: Option<String>,
    subject: Option<String>,
}

/// Returns `(pool, has_fts)` — `has_fts` is false if SQLite was built without FTS5.
pub async fn create_pool(database_url: &str) -> anyhow::Result<(SqlitePool, bool)> {
    // Strip sqlite:// prefix to get the file path, then ensure parent dir exists.
    let file_path = database_url
        .strip_prefix("sqlite:///")
        .map(|p| format!("/{p}"))
        .or_else(|| database_url.strip_prefix("sqlite://").map(String::from))
        .or_else(|| database_url.strip_prefix("sqlite:").map(String::from))
        .unwrap_or_else(|| database_url.to_string());

    if file_path != ":memory:" && !file_path.is_empty() {
        if let Some(parent) = std::path::Path::new(&file_path).parent() {
            if parent != std::path::Path::new("") {
                std::fs::create_dir_all(parent)?;
            }
        }
    }

    // WAL mode is not supported for in-memory databases; use Memory journal instead.
    let journal_mode = if file_path == ":memory:" {
        SqliteJournalMode::Memory
    } else {
        SqliteJournalMode::Wal
    };

    let opts: SqliteConnectOptions = database_url
        .parse::<SqliteConnectOptions>()?
        .create_if_missing(true)
        .journal_mode(journal_mode)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);

    // In-memory databases are per-connection in SQLite: limit to 1 connection so all
    // operations share the same database rather than each getting an empty one.
    let max_conns = if file_path == ":memory:" { 1 } else { 5 };
    let pool = SqlitePoolOptions::new()
        .max_connections(max_conns)
        .connect_with(opts)
        .await?;

    run_migrations(&pool).await?;

    let has_fts = match run_fts_migration(&pool).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("FTS5 unavailable ({e}) — search will use LIKE fallback");
            false
        }
    };

    Ok((pool, has_fts))
}

async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    // Run the initial migration inline to avoid needing DATABASE_URL at compile time.
    let migration_sql = include_str!("migrations/001_initial.sql");

    // Split on statement boundaries and execute each statement.
    // SQLite doesn't support multi-statement execute, so we split manually.
    for statement in migration_sql.split(';') {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(pool).await?;
        }
    }

    // Migration 002: add token_expiry, smtp_host, smtp_port columns.
    // ALTER TABLE fails if the column already exists; ignore "duplicate column name" errors.
    let migration_002 = include_str!("migrations/002_token_expiry.sql");
    for statement in migration_002.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("duplicate column name") {
                // Column already exists from a previous run — safe to skip.
            } else {
                return Err(e.into());
            }
        }
    }

    let migration_003 = include_str!("migrations/003_settings.sql");
    for statement in migration_003.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("duplicate column name") {
                // Column already exists — safe to skip.
            } else {
                return Err(e.into());
            }
        }
    }

    let migration_004 = include_str!("migrations/004_webhooks.sql");
    for statement in migration_004.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate") {
                // Table already exists — safe to skip.
            } else {
                return Err(e.into());
            }
        }
    }

    let migration_006 = include_str!("migrations/006_password.sql");
    for statement in migration_006.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            if e.to_string().contains("duplicate column name") {
                continue;
            }
            return Err(e.into());
        }
    }

    let migration_007 = include_str!("migrations/007_calendar_links.sql");
    for statement in migration_007.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate") {
                continue;
            }
            return Err(e.into());
        }
    }

    let migration_010 = include_str!("migrations/010_labels.sql");
    for statement in migration_010.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate") {
                continue;
            }
            return Err(e.into());
        }
    }

    let migration_009 = include_str!("migrations/009_rules.sql");
    for statement in migration_009.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate") {
                continue;
            }
            return Err(e.into());
        }
    }

    let migration_008 = include_str!("migrations/008_snooze.sql");
    for statement in migration_008.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate column name") {
                continue;
            }
            return Err(e.into());
        }
    }

    let migration_011 = include_str!("migrations/011_accounts_unique.sql");
    for statement in migration_011.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(trimmed).execute(pool).await {
            let msg = e.to_string();
            if msg.contains("already exists") || msg.contains("duplicate") {
                continue;
            }
            return Err(e.into());
        }
    }

    let migration_012 = include_str!("migrations/012_webhook_deliveries.sql");
    for statement in migration_012.split(';') {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(pool).await?;
        }
    }

    run_message_identity_migration(pool).await?;
    run_attachment_metadata_migration(pool).await?;
    run_application_auth_migration(pool).await?;
    run_integration_token_migration(pool).await?;
    run_logical_message_id_migration(pool).await?;

    Ok(())
}

async fn run_logical_message_id_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    let applied: Option<i64> =
        sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 16")
            .fetch_optional(pool)
            .await?;
    if applied.is_some() {
        return Ok(());
    }
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('messages')")
        .fetch_all(pool)
        .await?;
    if !columns.iter().any(|column| column == "logical_id") {
        sqlx::query("ALTER TABLE messages ADD COLUMN logical_id TEXT NOT NULL DEFAULT ''")
            .execute(pool)
            .await?;
    }
    let rows: Vec<LogicalIdentityRow> = sqlx::query_as(
        r#"SELECT id, account_id, message_id, from_email, date, subject
           FROM messages WHERE logical_id = ''"#,
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        let logical_id = logical_message_id(
            &row.account_id,
            row.message_id.as_deref(),
            row.from_email.as_deref(),
            row.date.as_deref(),
            row.subject.as_deref(),
            Some(&row.id),
        );
        sqlx::query("UPDATE messages SET logical_id = ? WHERE id = ?")
            .bind(logical_id)
            .bind(row.id)
            .execute(pool)
            .await?;
    }
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_logical_id ON messages(logical_id)")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO schema_migrations (version) VALUES (16)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Reconcile a logical message across mailbox moves. Exact duplicate copies
/// intentionally share an identifier; the folder/UID occurrence remains distinct.
pub(crate) fn logical_message_id(
    account_id: &str,
    rfc_message_id: Option<&str>,
    from_email: Option<&str>,
    date: Option<&str>,
    subject: Option<&str>,
    fallback_occurrence_id: Option<&str>,
) -> String {
    use sha2::{Digest, Sha256};

    fn normalized(value: Option<&str>) -> String {
        value.unwrap_or_default().trim().to_ascii_lowercase()
    }
    let identifying_metadata = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        normalized(rfc_message_id),
        normalized(from_email),
        normalized(date),
        normalized(subject),
    );
    let identity = if identifying_metadata.chars().all(|ch| ch == '\u{1f}') {
        fallback_occurrence_id.unwrap_or_default().to_string()
    } else {
        identifying_metadata
    };
    let digest = Sha256::digest(format!("{account_id}\u{1f}{identity}").as_bytes());
    format!(
        "lm_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

async fn run_integration_token_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    let applied: Option<i64> =
        sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 15")
            .fetch_optional(pool)
            .await?;
    if applied.is_some() {
        return Ok(());
    }
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS integration_tokens (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES app_users(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            token_prefix TEXT NOT NULL,
            scopes_json TEXT NOT NULL,
            expires_at INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_used_at TEXT,
            revoked_at TEXT
        )"#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_integration_tokens_user ON integration_tokens(user_id, created_at)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_integration_tokens_hash ON integration_tokens(token_hash)",
    )
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO schema_migrations (version) VALUES (15)")
        .execute(pool)
        .await?;
    Ok(())
}

async fn run_application_auth_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    let applied: Option<i64> =
        sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 14")
            .fetch_optional(pool)
            .await?;
    if applied.is_some() {
        return Ok(());
    }
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS app_users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS app_sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES app_users(id) ON DELETE CASCADE,
            token_hash TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL DEFAULT 'session',
            expires_at INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"#,
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_app_sessions_expiry ON app_sessions(expires_at)")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO schema_migrations (version) VALUES (14)")
        .execute(pool)
        .await?;
    Ok(())
}

async fn run_attachment_metadata_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    let applied: Option<i64> =
        sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 13")
            .fetch_optional(pool)
            .await?;
    if applied.is_some() {
        return Ok(());
    }
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('attachments')")
            .fetch_all(pool)
            .await?;
    if !columns.iter().any(|column| column == "part_id") {
        sqlx::query("ALTER TABLE attachments ADD COLUMN part_id TEXT")
            .execute(pool)
            .await?;
        sqlx::query("UPDATE attachments SET part_id = id WHERE part_id IS NULL")
            .execute(pool)
            .await?;
    }
    if !columns.iter().any(|column| column == "content_id") {
        sqlx::query("ALTER TABLE attachments ADD COLUMN content_id TEXT")
            .execute(pool)
            .await?;
    }
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_attachments_message_part ON attachments(message_id, part_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO schema_migrations (version) VALUES (13)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Rebuild the message cache so an IMAP UID is scoped by mailbox UIDVALIDITY.
/// RFC Message-ID remains searchable correlation metadata and is deliberately
/// not unique: the same message may occur in several folders or accounts.
async fn run_message_identity_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT (datetime('now')))",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS migration_backups (
            id TEXT PRIMARY KEY,
            migration_version INTEGER NOT NULL,
            path TEXT NOT NULL UNIQUE,
            status TEXT NOT NULL DEFAULT 'retained',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS account_reconciliation (
            account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
            migration_version INTEGER NOT NULL,
            backup_id TEXT REFERENCES migration_backups(id),
            required_at TEXT NOT NULL DEFAULT (datetime('now')),
            completed_at TEXT
        )"#,
    )
    .execute(pool)
    .await?;
    let applied: Option<i64> =
        sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 12")
            .fetch_optional(pool)
            .await?;
    if applied.is_some() {
        return Ok(());
    }

    let folder_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('folders')")
            .fetch_all(pool)
            .await?;
    if !folder_columns.iter().any(|column| column == "uid_validity") {
        sqlx::query("ALTER TABLE folders ADD COLUMN uid_validity INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }

    let message_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('messages')")
            .fetch_all(pool)
            .await?;
    let table_sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'messages'",
    )
    .fetch_one(pool)
    .await?;
    let already_current = message_columns
        .iter()
        .any(|column| column == "uid_validity")
        && !table_sql
            .to_ascii_lowercase()
            .contains("message_id text unique")
        && !table_sql
            .to_ascii_lowercase()
            .contains("message_id      text unique");

    if !already_current {
        let backup = create_message_identity_backup(pool).await?;

        let mut conn = pool.acquire().await?;
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *conn)
            .await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        let result: std::result::Result<(), sqlx::Error> = async {
            // Old rows without Message-ID could have been inserted repeatedly.
            // Keep the oldest cache row as part of the rebuild transaction.
            sqlx::query(
                "DELETE FROM messages WHERE rowid NOT IN (SELECT MIN(rowid) FROM messages GROUP BY folder_id, uid)",
            )
            .execute(&mut *conn)
            .await?;
            for trigger in ["messages_fts_ai", "messages_fts_ad", "messages_fts_au"] {
                sqlx::query(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                    .execute(&mut *conn)
                    .await?;
            }
            sqlx::query("DROP TABLE IF EXISTS messages_fts")
                .execute(&mut *conn)
                .await?;
            sqlx::query(
                r#"CREATE TABLE messages_new (
                    id TEXT PRIMARY KEY,
                    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                    folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                    uid INTEGER NOT NULL,
                    uid_validity INTEGER NOT NULL DEFAULT 0,
                    message_id TEXT,
                    thread_id TEXT,
                    subject TEXT,
                    from_name TEXT,
                    from_email TEXT,
                    to_json TEXT,
                    cc_json TEXT,
                    date TEXT,
                    is_read BOOLEAN NOT NULL DEFAULT 0,
                    is_flagged BOOLEAN NOT NULL DEFAULT 0,
                    is_draft BOOLEAN NOT NULL DEFAULT 0,
                    has_attachments BOOLEAN NOT NULL DEFAULT 0,
                    preview TEXT,
                    synced_at TEXT NOT NULL DEFAULT (datetime('now')),
                    snoozed_until TEXT,
                    UNIQUE(folder_id, uid_validity, uid)
                )"#,
            )
            .execute(&mut *conn)
            .await?;
            sqlx::query(
                r#"INSERT INTO messages_new (
                    id, account_id, folder_id, uid, uid_validity, message_id, thread_id,
                    subject, from_name, from_email, to_json, cc_json, date, is_read,
                    is_flagged, is_draft, has_attachments, preview, synced_at, snoozed_until
                )
                SELECT id, account_id, folder_id, uid, 0, message_id, thread_id,
                       subject, from_name, from_email, to_json, cc_json, date, is_read,
                       is_flagged, is_draft, has_attachments, preview, synced_at, snoozed_until
                FROM messages"#,
            )
            .execute(&mut *conn)
            .await?;
            sqlx::query("DROP TABLE messages")
                .execute(&mut *conn)
                .await?;
            sqlx::query("ALTER TABLE messages_new RENAME TO messages")
                .execute(&mut *conn)
                .await?;
            for statement in [
                "CREATE INDEX idx_messages_folder ON messages(folder_id)",
                "CREATE INDEX idx_messages_account ON messages(account_id)",
                "CREATE INDEX idx_messages_date ON messages(date DESC)",
                "CREATE INDEX idx_messages_rfc_message_id ON messages(message_id)",
                "CREATE INDEX idx_messages_snoozed ON messages(snoozed_until) WHERE snoozed_until IS NOT NULL",
            ] {
                sqlx::query(statement).execute(&mut *conn).await?;
            }
            sqlx::query("UPDATE folders SET uid_validity = 0, synced_at = NULL")
                .execute(&mut *conn)
                .await?;
            if let Some(backup) = &backup {
                sqlx::query(
                    "INSERT INTO migration_backups (id, migration_version, path) VALUES (?, 12, ?)",
                )
                .bind(&backup.id)
                .bind(&backup.path)
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    r#"INSERT INTO account_reconciliation
                       (account_id, migration_version, backup_id)
                       SELECT id, 12, ? FROM accounts WHERE 1
                       ON CONFLICT(account_id) DO UPDATE SET
                           migration_version = excluded.migration_version,
                           backup_id = excluded.backup_id,
                           required_at = datetime('now'),
                           completed_at = NULL"#,
                )
                .bind(&backup.id)
                .execute(&mut *conn)
                .await?;
            }
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(())
        }
        .await;

        if let Err(error) = result {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            let _ = sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *conn)
                .await;
            return Err(error.into());
        }
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut *conn)
            .await?;
    }

    sqlx::query("INSERT INTO schema_migrations (version) VALUES (12)")
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug)]
struct MigrationBackup {
    id: String,
    path: String,
}

async fn create_message_identity_backup(
    pool: &SqlitePool,
) -> anyhow::Result<Option<MigrationBackup>> {
    let databases: Vec<(i64, String, String)> = sqlx::query_as("PRAGMA database_list")
        .fetch_all(pool)
        .await?;
    let Some((_, _, database_path)) = databases.into_iter().find(|(_, name, _)| name == "main")
    else {
        return Ok(None);
    };
    if database_path.is_empty() || database_path == ":memory:" {
        return Ok(None);
    }
    let source = std::path::Path::new(&database_path);
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("email.db");
    let backup_path = source.with_file_name(format!(
        "{filename}.before-message-identity-{timestamp}-{}.sqlite",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ));
    let escaped = backup_path.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(pool)
        .await?;
    Ok(Some(MigrationBackup {
        id: uuid::Uuid::new_v4().to_string(),
        path: backup_path.to_string_lossy().into_owned(),
    }))
}

/// Record that every folder for an account completed a reconciliation pass.
/// Backups are retained on disk; `releasable` means an operator may remove one
/// after reviewing the migration and synchronization results.
pub async fn mark_account_reconciled(pool: &SqlitePool, account_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE account_reconciliation SET completed_at = datetime('now') WHERE account_id = ?",
    )
    .bind(account_id)
    .execute(pool)
    .await?;
    sqlx::query(
        r#"UPDATE migration_backups SET status = 'releasable'
           WHERE status = 'retained'
             AND NOT EXISTS (
                 SELECT 1 FROM account_reconciliation ar
                 WHERE ar.backup_id = migration_backups.id
                   AND ar.completed_at IS NULL
             )"#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Migration 005: FTS5 full-text search index for messages.
/// Written in Rust rather than a SQL file so we can execute each statement
/// individually — SQLite triggers contain embedded semicolons that confuse
/// a naive split-by-';' runner.
async fn run_fts_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            message_id UNINDEXED,
            subject,
            from_name,
            from_email,
            preview,
            tokenize = 'unicode61'
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS messages_fts_ai
         AFTER INSERT ON messages BEGIN
           INSERT INTO messages_fts(rowid, message_id, subject, from_name, from_email, preview)
           VALUES (new.rowid, new.id,
                   coalesce(new.subject,   ''),
                   coalesce(new.from_name, ''),
                   coalesce(new.from_email,''),
                   coalesce(new.preview,   ''));
         END",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS messages_fts_ad
         AFTER DELETE ON messages BEGIN
           DELETE FROM messages_fts WHERE rowid = old.rowid;
         END",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS messages_fts_au
         AFTER UPDATE OF subject, from_name, from_email, preview ON messages BEGIN
           DELETE FROM messages_fts WHERE rowid = old.rowid;
           INSERT INTO messages_fts(rowid, message_id, subject, from_name, from_email, preview)
           VALUES (new.rowid, new.id,
                   coalesce(new.subject,   ''),
                   coalesce(new.from_name, ''),
                   coalesce(new.from_email,''),
                   coalesce(new.preview,   ''));
         END",
    )
    .execute(pool)
    .await?;

    // Backfill any messages not yet in the index (idempotent).
    sqlx::query(
        "INSERT INTO messages_fts(rowid, message_id, subject, from_name, from_email, preview)
         SELECT rowid,
                id,
                coalesce(subject,   ''),
                coalesce(from_name, ''),
                coalesce(from_email,''),
                coalesce(preview,   '')
         FROM messages
         WHERE id NOT IN (SELECT message_id FROM messages_fts)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;

    #[test]
    fn logical_identity_is_stable_across_occurrences_and_not_rfc_only() {
        let first = logical_message_id(
            "account",
            Some("<duplicate@example>"),
            Some("one@example.test"),
            Some("2026-07-11T00:00:00Z"),
            Some("First"),
            Some("occurrence-a"),
        );
        let moved = logical_message_id(
            "account",
            Some("<duplicate@example>"),
            Some("one@example.test"),
            Some("2026-07-11T00:00:00Z"),
            Some("First"),
            Some("occurrence-b"),
        );
        let distinct_duplicate = logical_message_id(
            "account",
            Some("<duplicate@example>"),
            Some("two@example.test"),
            Some("2026-07-12T00:00:00Z"),
            Some("Second"),
            Some("occurrence-c"),
        );
        assert_eq!(first, moved);
        assert_ne!(first, distinct_duplicate);
    }

    #[tokio::test]
    async fn legacy_identity_migration_creates_backup_and_reconciliation_marker() {
        let database_path = format!("/tmp/email_legacy_{}.db", uuid::Uuid::new_v4());
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE accounts (id TEXT PRIMARY KEY)",
            "CREATE TABLE folders (id TEXT PRIMARY KEY, account_id TEXT NOT NULL, synced_at TEXT)",
            r#"CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                folder_id TEXT NOT NULL,
                uid INTEGER NOT NULL,
                message_id TEXT UNIQUE,
                thread_id TEXT,
                subject TEXT,
                from_name TEXT,
                from_email TEXT,
                to_json TEXT,
                cc_json TEXT,
                date TEXT,
                is_read BOOLEAN NOT NULL DEFAULT 0,
                is_flagged BOOLEAN NOT NULL DEFAULT 0,
                is_draft BOOLEAN NOT NULL DEFAULT 0,
                has_attachments BOOLEAN NOT NULL DEFAULT 0,
                preview TEXT,
                synced_at TEXT NOT NULL DEFAULT (datetime('now')),
                snoozed_until TEXT
            )"#,
            "INSERT INTO accounts (id) VALUES ('account-1')",
            "INSERT INTO folders (id, account_id, synced_at) VALUES ('folder-1', 'account-1', datetime('now'))",
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id) VALUES ('message-1', 'account-1', 'folder-1', 7, '<same@example>')",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        run_message_identity_migration(&pool).await.unwrap();

        let (backup_path, status): (String, String) = sqlx::query_as(
            "SELECT path, status FROM migration_backups WHERE migration_version = 12",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(std::path::Path::new(&backup_path).is_file());
        assert_eq!(status, "retained");
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM account_reconciliation WHERE account_id = 'account-1' AND completed_at IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 1);

        // RFC Message-ID is correlation metadata, so another occurrence in a
        // different mailbox may use the same value after migration.
        sqlx::query(
            "INSERT INTO folders (id, account_id, synced_at, uid_validity) VALUES ('folder-2', 'account-1', NULL, 3)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid, uid_validity, message_id) VALUES ('message-2', 'account-1', 'folder-2', 7, 3, '<same@example>')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let occurrences: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE message_id = '<same@example>'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(occurrences, 2);

        mark_account_reconciled(&pool, "account-1").await.unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM migration_backups WHERE migration_version = 12")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "releasable");

        pool.close().await;
        let _ = std::fs::remove_file(backup_path);
        let _ = std::fs::remove_file(database_path);
    }
}
