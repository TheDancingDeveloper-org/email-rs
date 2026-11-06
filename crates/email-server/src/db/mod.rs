use std::time::Duration;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqliteConnection, SqlitePool,
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

const LOGICAL_IDENTITY_MIGRATION_BATCH_SIZE: i64 = 1_000;

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
                // Plaintext bodies and attachment blobs live here, so the
                // directory must be private to this user.
                ensure_private_dir(parent)?;
            }
        }
    }

    let in_memory = file_path == ":memory:";

    // WAL mode is not supported for in-memory databases; use Memory journal instead.
    let journal_mode = if in_memory {
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
    if in_memory {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;
        sqlx::query("PRAGMA auto_vacuum = INCREMENTAL")
            .execute(&pool)
            .await?;
        let has_fts = migrate(&pool).await?;
        check_integrity(&pool, false).await?;
        return Ok((pool, has_fts));
    }

    // Migrate a file database through a dedicated single-connection pool and
    // close it before the serving pool opens. Migrating through the
    // serving pool let it open extra connections part-way through the schema
    // build. Such a connection keeps a stale schema, and the first statement it
    // runs that reaches the FTS5 table (for example `DELETE FROM accounts`,
    // which cascades through the `messages_fts` triggers) can fail once with
    // "no such table" instead of being re-prepared.
    let migration_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts.clone())
        .await?;
    sqlx::query("PRAGMA auto_vacuum = INCREMENTAL")
        .execute(&migration_pool)
        .await?;
    let migrated = migrate(&migration_pool).await;
    let integrity = if migrated.is_ok() {
        check_integrity(
            &migration_pool,
            std::env::var("EMAIL_RS_ALLOW_CORRUPT_DATABASE")
                .ok()
                .is_some_and(|value| {
                    matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes")
                }),
        )
        .await
    } else {
        Ok(())
    };
    migration_pool.close().await;
    let has_fts = migrated?;
    integrity?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    // SQLite creates the database, WAL and SHM with the process umask. Restrict
    // the files we own to 0600 after opening so a checkpoint that recreates the
    // WAL cannot leave them readable by other users.
    restrict_database_files(std::path::Path::new(&file_path))?;

    Ok((pool, has_fts))
}

pub(crate) async fn check_integrity(pool: &SqlitePool, allow_corrupt: bool) -> anyhow::Result<()> {
    let result: String = match sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(pool)
        .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error, "database quick_check could not run");
            if allow_corrupt {
                tracing::warn!("continuing after a failed database quick_check because EMAIL_RS_ALLOW_CORRUPT_DATABASE is set");
                return Ok(());
            }
            return Err(error.into());
        }
    };
    handle_integrity_result(&result, allow_corrupt)
}

fn handle_integrity_result(result: &str, allow_corrupt: bool) -> anyhow::Result<()> {
    if result.eq_ignore_ascii_case("ok") {
        tracing::info!("database quick_check passed");
        return Ok(());
    }
    tracing::error!(result = %result, "database quick_check reported corruption");
    if allow_corrupt {
        tracing::warn!(
            "continuing with a corrupt database because EMAIL_RS_ALLOW_CORRUPT_DATABASE is set"
        );
        Ok(())
    } else {
        anyhow::bail!("database integrity check failed: {result}")
    }
}

#[cfg(test)]
mod integrity_tests {
    use super::*;

    #[tokio::test]
    async fn quick_check_refuses_corrupt_schema_without_override() {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE valid (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        assert!(handle_integrity_result("*** in database main ***", false).is_err());
        assert!(handle_integrity_result("*** in database main ***", true).is_ok());
    }
}

/// Run every schema migration; returns whether FTS5 search is available.
async fn migrate(pool: &SqlitePool) -> anyhow::Result<bool> {
    run_migrations(pool).await?;

    Ok(match run_fts_migration(pool).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("FTS5 unavailable ({e}) — search will use LIKE fallback");
            false
        }
    })
}

/// Create `dir` (and missing parents) when they are missing, and set 0700
/// only on directories this process creates. An existing directory is left
/// as it is: group or other access is a warning, and startup refuses only
/// a world-writable directory that has no sticky bit. Shared parents such
/// as `/tmp` or `/var/lib` are never chmodded.
fn ensure_private_dir(dir: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut current = std::path::PathBuf::new();
        for component in dir.components() {
            current.push(component);
            if current.as_os_str().is_empty() {
                continue;
            }
            match std::fs::metadata(&current) {
                Ok(meta) => {
                    if !meta.is_dir() {
                        anyhow::bail!("{} is not a directory", current.display());
                    }
                    let mode = meta.permissions().mode();
                    // World-writable and not sticky: anyone can replace files.
                    if mode & 0o002 != 0 && mode & 0o1000 == 0 {
                        anyhow::bail!(
                            "data directory {} is world-writable without the sticky bit (mode {:o})",
                            current.display(),
                            mode & 0o7777
                        );
                    }
                    // Warn only for the directory the caller named. Parents
                    // such as /var or /data are often 0755 by design.
                    if current == dir && mode & 0o077 != 0 {
                        tracing::warn!(
                            path = %current.display(),
                            mode = format!("{:o}", mode & 0o777),
                            "existing data directory is accessible to group or other users; leaving its permissions unchanged"
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&current)?;
                    let mut perms = std::fs::metadata(&current)?.permissions();
                    perms.set_mode(0o700);
                    std::fs::set_permissions(&current, perms)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Force a freshly created backup to 0600. Unlike the live database files,
/// this copy was just written by us, so a mode wider than 0600 is always tightened.
fn restrict_backup_file(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)?;
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    let _ = path;
    Ok(())
}

/// Restrict the database file and its WAL/SHM sidecars to 0600 when we own them.
fn restrict_database_files(db: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for suffix in ["", "-wal", "-shm"] {
            let path = std::path::PathBuf::from(format!("{}{suffix}", db.display()));
            let meta = match std::fs::metadata(&path) {
                Ok(meta) => meta,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if !is_owned_by_us(&meta) {
                tracing::warn!(
                    path = %path.display(),
                    "database file is not owned by this process; leaving its permissions unchanged"
                );
                continue;
            }
            let mode = meta.permissions().mode();
            if mode & 0o077 != 0 {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                std::fs::set_permissions(&path, perms)?;
            }
        }
    }
    let _ = db;
    Ok(())
}

#[cfg(unix)]
fn is_owned_by_us(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.uid() == current_uid()
}

#[cfg(unix)]
fn current_uid() -> u32 {
    unsafe extern "C" {
        safe fn getuid() -> u32;
    }
    getuid()
}

#[cfg(all(unix, test))]
fn set_umask(mask: u32) -> u32 {
    unsafe extern "C" {
        safe fn umask(mask: u32) -> u32;
    }
    umask(mask)
}

/// Begin a transaction that will write, taking SQLite's write lock up front.
///
/// A deferred `BEGIN` takes the write lock at the first write. If the
/// transaction has read by then, it can't wait for another connection that
/// holds the lock: SQLite returns `SQLITE_BUSY` (or `SQLITE_BUSY_SNAPSHOT`) at
/// once, without calling the busy handler. That read doesn't have to be one of
/// our queries. Preparing a statement that reaches `messages_fts` on a
/// connection that hasn't opened the FTS5 table yet reads its config. With
/// `BEGIN IMMEDIATE` the lock is taken before anything is read, so it waits
/// out `busy_timeout` like any other writer.
pub async fn begin_write(
    pool: &SqlitePool,
) -> std::result::Result<sqlx::Transaction<'static, sqlx::Sqlite>, sqlx::Error> {
    // A reserved lock can still fail immediately (SQLITE_BUSY_SNAPSHOT) when
    // another writer commits between this connection's last read and the
    // upgrade. Retry a few times inside the busy timeout rather than failing
    // the folder and dropping its mail.
    let mut last = None;
    for attempt in 0..8 {
        match pool.begin_with("BEGIN IMMEDIATE").await {
            Ok(tx) => return Ok(tx),
            Err(error) if is_sqlite_locked(&error) && attempt < 7 => {
                last = Some(error);
                tokio::time::sleep(Duration::from_millis(50 * (attempt as u64 + 1))).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last.expect("retry loop records the last lock error"))
}

fn is_sqlite_locked(error: &sqlx::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("database is locked")
        || message.contains("database is busy")
        || message.contains("sqlite_busy")
}

type StepFuture<'c> =
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'c>>;

enum Apply {
    /// Runs inside the transaction the runner opens; the version row is
    /// written in the same transaction, so a failure leaves no trace.
    Transaction(for<'c> fn(&'c mut SqliteConnection) -> StepFuture<'c>),
    /// Rebuilds tables with `PRAGMA foreign_keys = OFF`, which SQLite ignores
    /// inside a transaction, so the step manages its own transaction.
    SelfManaged(for<'c> fn(&'c SqlitePool) -> StepFuture<'c>),
}

struct Migration {
    version: i64,
    name: &'static str,
    apply: Apply,
}

macro_rules! in_transaction {
    ($step:path) => {{
        fn wrapped(conn: &mut SqliteConnection) -> StepFuture<'_> {
            Box::pin($step(conn))
        }
        Apply::Transaction(wrapped)
    }};
}

/// Every schema migration, in the order it is applied. Versions are strictly
/// increasing. Add new steps at the end with the next version number; never
/// reorder or renumber an entry that has shipped.
fn migrations() -> Vec<Migration> {
    fn identity(pool: &SqlitePool) -> StepFuture<'_> {
        Box::pin(run_message_identity_migration(pool))
    }
    fn attachment_files(pool: &SqlitePool) -> StepFuture<'_> {
        Box::pin(run_attachment_file_store_migration(pool))
    }
    vec![
        Migration {
            version: 11,
            name: "legacy_sql_baseline",
            apply: in_transaction!(run_legacy_sql_baseline),
        },
        Migration {
            version: 12,
            name: "message_identity",
            apply: Apply::SelfManaged(identity),
        },
        Migration {
            version: 13,
            name: "attachment_metadata",
            apply: in_transaction!(run_attachment_metadata_migration),
        },
        Migration {
            version: 14,
            name: "application_auth",
            apply: in_transaction!(run_application_auth_migration),
        },
        Migration {
            version: 15,
            name: "integration_tokens",
            apply: in_transaction!(run_integration_token_migration),
        },
        Migration {
            version: 16,
            name: "logical_message_id",
            apply: in_transaction!(run_logical_message_id_migration),
        },
        Migration {
            version: 17,
            name: "clean_body",
            apply: in_transaction!(run_clean_body_migration),
        },
        Migration {
            version: 18,
            name: "search_index",
            apply: in_transaction!(run_search_index_migration),
        },
        Migration {
            version: 19,
            name: "provider_subject",
            apply: in_transaction!(run_provider_subject_migration),
        },
        Migration {
            version: 20,
            name: "outbox",
            apply: in_transaction!(run_outbox_migration),
        },
        Migration {
            version: 21,
            name: "thread_backfill",
            apply: in_transaction!(run_thread_backfill_migration),
        },
        Migration {
            version: 22,
            name: "webhook_secret",
            apply: in_transaction!(run_webhook_secret_migration),
        },
        Migration {
            version: 23,
            name: "message_pagination",
            apply: in_transaction!(run_message_pagination_migration),
        },
        Migration {
            version: 24,
            name: "attachment_file_store",
            apply: Apply::SelfManaged(attachment_files),
        },
        Migration {
            version: 25,
            name: "search_attachment_names",
            apply: in_transaction!(run_search_attachment_names_migration),
        },
        Migration {
            version: 26,
            name: "identities",
            apply: in_transaction!(run_identities_migration),
        },
        Migration {
            version: 27,
            name: "calendar_invitations",
            apply: in_transaction!(run_calendar_invitations_migration),
        },
        Migration {
            version: 28,
            name: "account_sync_health",
            apply: in_transaction!(run_account_sync_health_migration),
        },
        Migration {
            version: 29,
            name: "identity_signatures",
            apply: in_transaction!(run_identity_signatures_migration),
        },
        Migration {
            version: 30,
            name: "folder_delimiters",
            apply: in_transaction!(run_folder_delimiters_migration),
        },
        Migration {
            version: 31,
            name: "calendar_provider_scope",
            apply: in_transaction!(run_calendar_provider_scope_migration),
        },
        Migration {
            version: 32,
            name: "imap_action_queue",
            apply: in_transaction!(run_imap_action_queue_migration),
        },
    ]
}

async fn run_imap_action_queue_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS imap_action_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            action TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS imap_action_queue_account_order ON imap_action_queue(account_id, id)")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn run_account_sync_health_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS account_sync_health (account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE, last_success_at TEXT, last_error_at TEXT, last_error TEXT)")
        .execute(&mut *conn).await?;
    Ok(())
}

async fn run_identity_signatures_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query("ALTER TABLE identities ADD COLUMN signature TEXT")
        .execute(&mut *conn)
        .await?;
    sqlx::query("ALTER TABLE identities ADD COLUMN signature_html TEXT")
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "ALTER TABLE identities ADD COLUMN signature_placement TEXT NOT NULL DEFAULT 'below'",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Scope the provider-id uniqueness to a calendar, so a locally stored
/// invitation and a provider-synced event may share an id in different calendars.
async fn run_calendar_provider_scope_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query("DROP INDEX IF EXISTS idx_calendar_events_provider")
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "CREATE UNIQUE INDEX idx_calendar_events_provider
         ON calendar_events(account_id, calendar_id, provider_event_id)
         WHERE provider_event_id IS NOT NULL",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn run_folder_delimiters_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query("ALTER TABLE folders ADD COLUMN delimiter TEXT")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn run_calendar_invitations_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query(r#"CREATE TABLE IF NOT EXISTS calendar_invitations (
        id TEXT PRIMARY KEY, message_id TEXT NOT NULL UNIQUE REFERENCES messages(id) ON DELETE CASCADE,
        method TEXT NOT NULL, uid TEXT NOT NULL, sequence INTEGER, summary TEXT, description TEXT,
        location TEXT, start_at TEXT NOT NULL, end_at TEXT NOT NULL, organizer TEXT, attendees_json TEXT NOT NULL
    )"#).execute(&mut *conn).await?;
    Ok(())
}

/// Apply every migration this database has not recorded, oldest first.
///
/// Refuses to touch a database whose recorded schema is newer than this
/// binary knows, so an older build never runs against a newer layout.
async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    apply_migrations(pool, &migrations()).await
}

async fn apply_migrations(pool: &SqlitePool, steps: &[Migration]) -> anyhow::Result<()> {
    debug_assert!(steps
        .windows(2)
        .all(|pair| pair[0].version < pair[1].version));
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT (datetime('now')))",
    )
    .execute(pool)
    .await?;

    let latest = steps.last().map_or(0, |step| step.version);
    let recorded: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations")
        .fetch_all(pool)
        .await?;
    if let Some(newest) = recorded.iter().copied().max() {
        if newest > latest {
            anyhow::bail!(
                "database schema version {newest} is newer than this build supports (version {latest}); refusing to start. Upgrade email-rs or restore a matching backup"
            );
        }
    }

    for step in steps {
        if recorded.contains(&step.version) {
            continue;
        }
        tracing::info!(
            version = step.version,
            name = step.name,
            "applying migration"
        );
        let context = || format!("migration {} ({}) failed", step.version, step.name);
        match &step.apply {
            Apply::Transaction(apply) => {
                let mut tx = begin_write(pool).await?;
                match apply(&mut tx).await {
                    Ok(()) => {
                        sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
                            .bind(step.version)
                            .execute(&mut *tx)
                            .await?;
                        tx.commit().await?;
                    }
                    Err(error) => {
                        let _ = tx.rollback().await;
                        return Err(error.context(context()));
                    }
                }
            }
            Apply::SelfManaged(apply) => {
                apply(pool)
                    .await
                    .map_err(|error| error.context(context()))?;
                sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
                    .bind(step.version)
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(())
}

/// Run one statement file. Statements fail with an error naming something that
/// already exists when an earlier build applied them without recording it; the
/// messages in `tolerated` are skipped, anything else aborts.
async fn run_sql_file(
    conn: &mut SqliteConnection,
    sql: &str,
    tolerated: &[&str],
) -> anyhow::Result<()> {
    // SQLite doesn't support multi-statement execute, so we split manually.
    for statement in sql.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Err(error) = sqlx::query(trimmed).execute(&mut *conn).await {
            let message = error.to_string();
            if tolerated.iter().any(|needle| message.contains(needle)) {
                continue;
            }
            return Err(error.into());
        }
    }
    Ok(())
}

/// The numbered SQL files that predate version tracking. They are idempotent
/// and run as one baseline step, in the order the old runner applied them.
async fn run_legacy_sql_baseline(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let dup_column: &[&str] = &["duplicate column name"];
    let exists: &[&str] = &["already exists", "duplicate"];
    let exists_or_column: &[&str] = &["already exists", "duplicate column name"];
    run_sql_file(conn, include_str!("migrations/001_initial.sql"), &[]).await?;
    run_sql_file(
        conn,
        include_str!("migrations/002_token_expiry.sql"),
        dup_column,
    )
    .await?;
    run_sql_file(
        conn,
        include_str!("migrations/003_settings.sql"),
        dup_column,
    )
    .await?;
    run_sql_file(conn, include_str!("migrations/004_webhooks.sql"), exists).await?;
    run_sql_file(
        conn,
        include_str!("migrations/006_password.sql"),
        dup_column,
    )
    .await?;
    run_sql_file(
        conn,
        include_str!("migrations/007_calendar_links.sql"),
        exists,
    )
    .await?;
    run_sql_file(conn, include_str!("migrations/010_labels.sql"), exists).await?;
    run_sql_file(conn, include_str!("migrations/009_rules.sql"), exists).await?;
    run_sql_file(
        conn,
        include_str!("migrations/008_snooze.sql"),
        exists_or_column,
    )
    .await?;
    run_sql_file(
        conn,
        include_str!("migrations/011_accounts_unique.sql"),
        exists,
    )
    .await?;
    run_sql_file(
        conn,
        include_str!("migrations/012_webhook_deliveries.sql"),
        &[],
    )
    .await?;
    Ok(())
}

async fn run_message_pagination_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let migration_sql = include_str!("migrations/023_message_pagination.sql");
    for statement in migration_sql.split(';') {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        sqlx::query(statement).execute(&mut *conn).await?;
    }
    Ok(())
}

async fn run_webhook_secret_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, secret FROM webhooks WHERE secret IS NOT NULL")
            .fetch_all(&mut *conn)
            .await?;
    for (id, secret) in rows {
        let Some(secret) = secret else {
            continue;
        };
        if let Some(sealed) = crate::credentials::reseal(&secret).map_err(anyhow::Error::msg)? {
            sqlx::query("UPDATE webhooks SET secret = ? WHERE id = ?")
                .bind(sealed)
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

async fn run_outbox_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS outbox (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            payload TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            scheduled_at TEXT NOT NULL,
            cancel_until TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            next_attempt_at TEXT NOT NULL,
            last_error TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_outbox_due ON outbox(status, next_attempt_at)")
        .execute(&mut *conn)
        .await?;

    Ok(())
}

/// Send-as aliases. The account's own address is its default identity and is
/// read from `accounts`, so only aliases are stored here.
async fn run_identities_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS identities (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            name TEXT NOT NULL DEFAULT '',
            email TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"#,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_identities_account_email ON identities(account_id, lower(email))",
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn run_thread_backfill_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        account_id: String,
        message_id: Option<String>,
        thread_id: Option<String>,
        subject: Option<String>,
        raw_headers: Option<String>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT m.id, m.account_id, m.message_id, m.thread_id, m.subject, b.raw_headers FROM messages m LEFT JOIN message_bodies b ON b.message_id = m.id"
    ).fetch_all(&mut *conn).await?;
    for row in &rows {
        let ids = row
            .raw_headers
            .as_deref()
            .map(crate::threading::references)
            .unwrap_or_default();
        let parent = ids.iter().find_map(|id| {
            rows.iter().find(|candidate| {
                candidate.account_id == row.account_id
                    && candidate
                        .message_id
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case(id))
            })
        });
        let subject = crate::threading::normalized_subject(row.subject.as_deref());
        let fallback = subject.as_ref().and_then(|subject| {
            rows.iter().find(|candidate| {
                candidate.id != row.id
                    && candidate.account_id == row.account_id
                    && crate::threading::normalized_subject(candidate.subject.as_deref()).as_ref()
                        == Some(subject)
            })
        });
        let thread_id = parent
            .or(fallback)
            .and_then(|candidate| candidate.thread_id.clone())
            .or_else(|| row.thread_id.clone())
            .or_else(|| {
                row.message_id
                    .as_ref()
                    .map(|id| format!("rfc:{}:{}", row.account_id, id))
            });
        if thread_id != row.thread_id {
            sqlx::query("UPDATE messages SET thread_id = ? WHERE id = ?")
                .bind(thread_id)
                .bind(&row.id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}
/// Directory identity of an OAuth account (Microsoft tid + oid).
/// Existing rows stay null until the next successful connect.
async fn run_provider_subject_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('accounts')")
        .fetch_all(&mut *conn)
        .await?;
    for (column, ddl) in [
        (
            "provider_tenant_id",
            "ALTER TABLE accounts ADD COLUMN provider_tenant_id TEXT",
        ),
        (
            "provider_object_id",
            "ALTER TABLE accounts ADD COLUMN provider_object_id TEXT",
        ),
        // Set when the provider refuses the refresh token (`invalid_grant`);
        // cleared by a successful reconnect.
        (
            "needs_reauth",
            "ALTER TABLE accounts ADD COLUMN needs_reauth INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "reauth_reason",
            "ALTER TABLE accounts ADD COLUMN reauth_reason TEXT",
        ),
    ] {
        if !columns.iter().any(|name| name == column) {
            sqlx::query(ddl).execute(&mut *conn).await?;
        }
    }
    Ok(())
}

/// Search-index bookkeeping: `search_version` records which version
/// of the search document the `messages_fts` row of a cached body holds.
/// Rows start at 0 and are indexed by the background reindex or when their
/// body is next cleaned.
async fn run_search_index_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('message_bodies')")
            .fetch_all(&mut *conn)
            .await?;
    if !columns.iter().any(|c| c == "search_version") {
        sqlx::query(
            "ALTER TABLE message_bodies ADD COLUMN search_version INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_message_bodies_search_version ON message_bodies(search_version)",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Cleaned message bodies. Rows start at `clean_version` 0 and are
/// cleaned by the background backfill or on their next read.
async fn run_clean_body_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('message_bodies')")
            .fetch_all(&mut *conn)
            .await?;
    for (column, ddl) in [
        (
            "clean_body",
            "ALTER TABLE message_bodies ADD COLUMN clean_body TEXT",
        ),
        (
            "quoted_history",
            "ALTER TABLE message_bodies ADD COLUMN quoted_history TEXT",
        ),
        (
            "signature_stripped",
            "ALTER TABLE message_bodies ADD COLUMN signature_stripped INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "clean_version",
            "ALTER TABLE message_bodies ADD COLUMN clean_version INTEGER NOT NULL DEFAULT 0",
        ),
    ] {
        if !columns.iter().any(|c| c == column) {
            sqlx::query(ddl).execute(&mut *conn).await?;
        }
    }
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_message_bodies_clean_version ON message_bodies(clean_version)",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn run_logical_message_id_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('messages')")
        .fetch_all(&mut *conn)
        .await?;
    if !columns.iter().any(|column| column == "logical_id") {
        sqlx::query("ALTER TABLE messages ADD COLUMN logical_id TEXT NOT NULL DEFAULT ''")
            .execute(&mut *conn)
            .await?;
    }
    // The runner wraps this step in one transaction, so a failing batch
    // rolls the whole step back. Batches only bound the size of each read.
    loop {
        let rows: Vec<LogicalIdentityRow> = sqlx::query_as(
            r#"SELECT id, account_id, message_id, from_email, date, subject
               FROM messages
               WHERE logical_id = ''
               ORDER BY id
               LIMIT ?"#,
        )
        .bind(LOGICAL_IDENTITY_MIGRATION_BATCH_SIZE)
        .fetch_all(&mut *conn)
        .await?;
        if rows.is_empty() {
            break;
        }
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
                .execute(&mut *conn)
                .await?;
        }
    }
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_logical_id ON messages(logical_id)")
        .execute(&mut *conn)
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

async fn run_integration_token_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
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
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_integration_tokens_user ON integration_tokens(user_id, created_at)",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_integration_tokens_hash ON integration_tokens(token_hash)",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn run_application_auth_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS app_users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"#,
    )
    .execute(&mut *conn)
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
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_app_sessions_expiry ON app_sessions(expires_at)")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

const ATTACHMENT_FILE_MIGRATION_BATCH_SIZE: i64 = 50;

/// Add `attachments.file_sha256` and move existing BLOB content into the
/// content-addressed store in batches. Each batch commits on its own and only
/// clears a BLOB after its file is durably written, so an interrupted run
/// resumes where it stopped (the version is recorded only at the end).
/// Databases without a data directory (in-memory) keep their BLOBs.
async fn run_attachment_file_store_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('attachments')")
            .fetch_all(pool)
            .await?;
    if !columns.iter().any(|column| column == "file_sha256") {
        sqlx::query("ALTER TABLE attachments ADD COLUMN file_sha256 TEXT")
            .execute(pool)
            .await?;
    }
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_attachments_file_sha256 ON attachments(file_sha256) WHERE file_sha256 IS NOT NULL",
    )
    .execute(pool)
    .await?;
    let Some(dir) = crate::attachment_store::store_dir(pool).await else {
        return Ok(());
    };
    loop {
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
            "SELECT id, content FROM attachments WHERE content IS NOT NULL AND file_sha256 IS NULL LIMIT ?",
        )
        .bind(ATTACHMENT_FILE_MIGRATION_BATCH_SIZE)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = begin_write(pool).await?;
        for (id, content) in rows {
            let digest = crate::attachment_store::put(&dir, &content).await?;
            sqlx::query(
                "UPDATE attachments SET file_sha256 = ?, content = NULL WHERE id = ? AND file_sha256 IS NULL",
            )
            .bind(digest)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }
}

async fn run_attachment_metadata_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('attachments')")
            .fetch_all(&mut *conn)
            .await?;
    if !columns.iter().any(|column| column == "part_id") {
        sqlx::query("ALTER TABLE attachments ADD COLUMN part_id TEXT")
            .execute(&mut *conn)
            .await?;
        sqlx::query("UPDATE attachments SET part_id = id WHERE part_id IS NULL")
            .execute(&mut *conn)
            .await?;
    }
    if !columns.iter().any(|column| column == "content_id") {
        sqlx::query("ALTER TABLE attachments ADD COLUMN content_id TEXT")
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_attachments_message_part ON attachments(message_id, part_id)",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Rebuild the message cache so an IMAP UID is scoped by mailbox UIDVALIDITY.
/// RFC Message-ID remains searchable correlation metadata and is deliberately
/// not unique: the same message may occur in several folders or accounts.
async fn run_message_identity_migration(pool: &SqlitePool) -> anyhow::Result<()> {
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
    let folder_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('folders')")
            .fetch_all(pool)
            .await?;
    if !folder_columns.iter().any(|column| column == "uid_validity") {
        sqlx::query("ALTER TABLE folders ADD COLUMN uid_validity INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }

    if !folder_columns
        .iter()
        .any(|column| column == "is_subscribed")
    {
        sqlx::query("ALTER TABLE folders ADD COLUMN is_subscribed BOOLEAN NOT NULL DEFAULT 1")
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
            for trigger in FTS_TRIGGERS {
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

    Ok(())
}

#[derive(Debug)]
struct MigrationBackup {
    id: String,
    path: String,
}

/// Create an empty backup file at `0600`, or refuse if the path exists.
///
/// `create_new` will not follow a symlink: the open fails when the final
/// component is already present. On non-Unix the same refusal is an exists
/// check; the mode is left to the platform.
fn precreate_backup_file(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| {
                anyhow::anyhow!(
                    "refusing to write message-identity backup at {}: {error}",
                    path.display()
                )
            })?;
    }
    #[cfg(not(unix))]
    {
        if path.exists() {
            anyhow::bail!(
                "refusing to write message-identity backup over existing path {}",
                path.display()
            );
        }
    }
    Ok(())
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
    // Pre-create the target owner-only. VACUUM INTO otherwise creates the file
    // with the process umask (0644 under 022) in a directory that may be 0755.
    // create_new also fails when the path already exists, including a planted
    // symlink, so the backup cannot follow a link. SQLite accepts an empty
    // existing file as a VACUUM INTO target. The post-chmod stays as a second
    // layer in case the engine replaces the inode.
    precreate_backup_file(&backup_path)?;
    let escaped = backup_path.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(pool)
        .await?;
    restrict_backup_file(&backup_path)?;
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

/// Every trigger that keeps `messages_fts` in step with `messages` and
/// `attachments`.
const FTS_TRIGGERS: [&str; 7] = [
    "messages_fts_ai",
    "messages_fts_ad",
    "messages_fts_au",
    "messages_fts_au_preview",
    "messages_fts_att_ai",
    "messages_fts_att_ad",
    "messages_fts_att_au",
];

/// Migration 25: the search index gains a `filenames` column (attachment
/// names). An index without it is dropped with its triggers;
/// [`run_fts_migration`] then creates the new one, fills it from `messages`
/// and `attachments`, and marks every body for reindexing. Does nothing when
/// the index is absent (no FTS5) or already current.
async fn run_search_attachment_names_migration(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('messages_fts')")
            .fetch_all(&mut *conn)
            .await?;
    if columns.is_empty() || columns.iter().any(|c| c == "filenames") {
        return Ok(());
    }
    for trigger in FTS_TRIGGERS {
        sqlx::query(&format!("DROP TRIGGER IF EXISTS {trigger}"))
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("DROP TABLE messages_fts")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The attachment names of a message as indexed text, for the message whose
/// id is `message_id` (an SQL expression).
fn filenames_sql(message_id: &str) -> String {
    format!(
        "coalesce((SELECT group_concat(filename, ' ') FROM attachments
                    WHERE message_id = {message_id}), '')"
    )
}

/// The sender of a message as indexed text: display name and address.
fn sender_sql(row: &str) -> String {
    format!("coalesce({row}.from_name, '') || ' ' || coalesce({row}.from_email, '')")
}

/// The recipients of a message (to and cc) as indexed text. The address lists
/// are JSON (`["Name <a@b>", ...]`, or objects with `name`/`email`); only the
/// names and addresses are indexed, not the JSON punctuation or keys. A value
/// that isn't valid JSON is indexed as it is rather than failing the write.
fn recipients_sql(row: &str) -> String {
    let list = |column: &str| {
        format!(
            "CASE WHEN json_valid({row}.{column}) THEN coalesce((
                 SELECT group_concat(CASE type
                          WHEN 'text' THEN value
                          WHEN 'object' THEN coalesce(json_extract(value, '$.name'), '') || ' ' ||
                                             coalesce(json_extract(value, '$.email'), '')
                          ELSE '' END, ' ')
                 FROM json_each({row}.{column})), '')
             ELSE coalesce({row}.{column}, '') END"
        )
    };
    format!("{} || ' ' || {}", list("to_json"), list("cc_json"))
}

/// Migration 005: FTS5 full-text search index for messages.
///
/// One row per message, keyed by the `messages` rowid:
///
/// - `subject`, `sender` (from name and address) and `recipients` (to and cc)
///   come from `messages` and are kept current by triggers;
/// - `body` is the clean current message with link targets removed,
///   written by [`crate::search_index::index_body`] when a body is cleaned.
///   Until then it holds the message preview;
/// - `quoted` is the quoted history split off the body. It stays searchable so
///   a reply quoting mail that isn't synced can still be found, but is weighted
///   far below `body` (see `api::search`), so a message that only quotes a term
///   ranks under the messages that say it.
///
/// Written in Rust rather than a SQL file so we can execute each statement
/// individually — SQLite triggers contain embedded semicolons that confuse
/// a naive split-by-';' runner.
async fn run_fts_migration(pool: &SqlitePool) -> anyhow::Result<()> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('messages_fts')")
            .fetch_all(pool)
            .await?;
    // The first index (subject, from_name, from_email, preview) held raw
    // previews only. Replace it; bodies are indexed again in the background.
    if !columns.is_empty() && !columns.iter().any(|c| c == "quoted") {
        for trigger in FTS_TRIGGERS {
            sqlx::query(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                .execute(pool)
                .await?;
        }
        sqlx::query("DROP TABLE messages_fts").execute(pool).await?;
    }
    let existed: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'messages_fts'",
    )
    .fetch_optional(pool)
    .await?;

    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            message_id UNINDEXED,
            subject,
            sender,
            recipients,
            body,
            quoted,
            filenames,
            tokenize = 'unicode61'
        )",
    )
    .execute(pool)
    .await?;

    if existed.is_none() {
        // A new index holds no clean body yet, whatever an earlier one held.
        sqlx::query("UPDATE message_bodies SET search_version = 0 WHERE search_version <> 0")
            .execute(pool)
            .await?;
    }

    sqlx::query(&format!(
        "CREATE TRIGGER IF NOT EXISTS messages_fts_ai
         AFTER INSERT ON messages BEGIN
           INSERT INTO messages_fts(rowid, message_id, subject, sender, recipients, body, quoted, filenames)
           VALUES (new.rowid, new.id, coalesce(new.subject, ''), {}, {},
                   coalesce(new.preview, ''), '', '');
         END",
        sender_sql("new"),
        recipients_sql("new"),
    ))
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

    sqlx::query(&format!(
        "CREATE TRIGGER IF NOT EXISTS messages_fts_au
         AFTER UPDATE OF subject, from_name, from_email, to_json, cc_json ON messages BEGIN
           UPDATE messages_fts
              SET subject = coalesce(new.subject, ''), sender = {}, recipients = {}
            WHERE rowid = new.rowid;
         END",
        sender_sql("new"),
        recipients_sql("new"),
    ))
    .execute(pool)
    .await?;

    // The preview stands in for the body only until the clean body is indexed.
    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS messages_fts_au_preview
         AFTER UPDATE OF preview ON messages BEGIN
           UPDATE messages_fts SET body = coalesce(new.preview, '')
            WHERE rowid = new.rowid
              AND NOT EXISTS (SELECT 1 FROM message_bodies
                               WHERE message_id = new.id AND search_version > 0);
         END",
    )
    .execute(pool)
    .await?;

    // Attachment names follow the `attachments` rows. A message deleted with
    // its attachments has no index row left to update.
    for (name, event, row, of) in [
        ("messages_fts_att_ai", "INSERT", "new", ""),
        ("messages_fts_att_ad", "DELETE", "old", ""),
        (
            "messages_fts_att_au",
            "UPDATE",
            "new",
            " OF filename, message_id",
        ),
    ] {
        sqlx::query(&format!(
            "CREATE TRIGGER IF NOT EXISTS {name}
             AFTER {event}{of} ON attachments BEGIN
               UPDATE messages_fts SET filenames = {names}
                WHERE rowid = (SELECT rowid FROM messages WHERE id = {row}.message_id);
             END",
            names = filenames_sql(&format!("{row}.message_id")),
        ))
        .execute(pool)
        .await?;
    }

    // Add any message not yet in the index (idempotent). Clean bodies follow
    // in the background (`search_index::spawn_reindex`).
    fill_search_index(pool).await
}

/// Insert an index row for every message that has none: subject, sender,
/// recipients, preview and attachment names. Clean bodies are written by
/// `search_index`.
pub(crate) async fn fill_search_index<'e, E>(executor: E) -> anyhow::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(&format!(
        "INSERT INTO messages_fts(rowid, message_id, subject, sender, recipients, body, quoted, filenames)
         SELECT m.rowid, m.id, coalesce(m.subject, ''), {}, {}, coalesce(m.preview, ''), '', {}
         FROM messages m
         WHERE m.rowid NOT IN (SELECT rowid FROM messages_fts)",
        sender_sql("m"),
        recipients_sql("m"),
        filenames_sql("m.id"),
    ))
    .execute(executor)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn calendar_provider_scope_migration_rebuilds_the_unique_index() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();

        // Put the database back at version 30: the index without calendar_id.
        sqlx::query("DROP INDEX idx_calendar_events_provider")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE UNIQUE INDEX idx_calendar_events_provider ON calendar_events(account_id, provider_event_id) WHERE provider_event_id IS NOT NULL",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM schema_migrations WHERE version = 31")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a', 'A', 'a@example.test', 'imap', 'password')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO calendar_events (id, account_id, calendar_id, provider_event_id, title, start_at, end_at) VALUES ('e1', 'a', 'primary', 'p', 'T', 's', 'e')")
            .execute(&pool)
            .await
            .unwrap();

        run_migrations(&pool).await.unwrap();

        // The same provider id in another calendar is now allowed...
        sqlx::query("INSERT INTO calendar_events (id, account_id, calendar_id, provider_event_id, title, start_at, end_at) VALUES ('e2', 'a', 'default', 'p', 'T', 's', 'e')")
            .execute(&pool)
            .await
            .unwrap();
        // ...but a duplicate within one calendar is still refused.
        assert!(sqlx::query("INSERT INTO calendar_events (id, account_id, calendar_id, provider_event_id, title, start_at, end_at) VALUES ('e3', 'a', 'default', 'p', 'T', 's', 'e')")
            .execute(&pool)
            .await
            .is_err());
    }
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
    #[cfg(unix)]
    async fn message_identity_backup_is_owner_only_under_umask_022() {
        use std::os::unix::fs::PermissionsExt;
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let database_path = _db_dir.path().join("email.db").display().to_string();
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
                message_id TEXT UNIQUE
            )"#,
            "INSERT INTO accounts (id) VALUES ('account-1')",
            "INSERT INTO folders (id, account_id, synced_at) VALUES ('folder-1', 'account-1', datetime('now'))",
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id) VALUES ('message-1', 'account-1', 'folder-1', 7, '<same@example>')",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        let previous = set_umask(0o022);
        let backup = create_message_identity_backup(&pool).await.unwrap();
        set_umask(previous);
        let backup = backup.expect("file database produces a backup");
        let mode = std::fs::metadata(&backup.path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "backup mode was {mode:o}");
        pool.close().await;
        let _ = std::fs::remove_file(backup.path);
        let _ = std::fs::remove_file(database_path);
    }

    #[test]
    #[cfg(unix)]
    fn precreated_backup_file_is_owner_only_before_vacuum() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("email.db.before-message-identity-test.sqlite");
        let previous = set_umask(0o022);
        let created = precreate_backup_file(&path);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        set_umask(previous);
        created.unwrap();
        assert_eq!(mode, 0o600, "backup mode at creation was {mode:o}");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    }

    #[test]
    #[cfg(unix)]
    fn precreated_backup_refuses_existing_file_and_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("already.sqlite");
        std::fs::write(&existing, b"planted").unwrap();
        let error = precreate_backup_file(&existing).unwrap_err();
        assert!(
            error.to_string().contains("refusing"),
            "existing file error was {error}"
        );
        assert_eq!(std::fs::read(&existing).unwrap(), b"planted");

        let target = dir.path().join("secret.sqlite");
        std::fs::write(&target, b"secret").unwrap();
        let link = dir.path().join("backup-link.sqlite");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let error = precreate_backup_file(&link).unwrap_err();
        assert!(
            error.to_string().contains("refusing"),
            "symlink error was {error}"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"secret");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[tokio::test]
    async fn legacy_identity_migration_creates_backup_and_reconciliation_marker() {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let database_path = _db_dir.path().join("email.db").display().to_string();
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

    #[tokio::test]
    async fn full_migrations_upgrade_messages_without_logical_id() {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let database_path = _db_dir.path().join("email.db").display().to_string();
        let options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        for statement in include_str!("migrations/001_initial.sql").split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&pool).await.unwrap();
            }
        }
        for statement in [
            "ALTER TABLE messages DROP COLUMN logical_id",
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('account-1', 'Legacy', 'legacy@example.test', 'imap', 'password')",
            "INSERT INTO folders (id, account_id, name, full_path, synced_at) VALUES ('folder-1', 'account-1', 'Inbox', 'INBOX', datetime('now'))",
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id, from_email, date, subject) VALUES ('message-1', 'account-1', 'folder-1', 7, '<legacy@example>', 'sender@example.test', '2026-07-12T00:00:00Z', 'Legacy')",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        run_migrations(&pool).await.unwrap();

        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('messages')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(columns.iter().any(|column| column == "logical_id"));
        let logical_id: String =
            sqlx::query_scalar("SELECT logical_id FROM messages WHERE id = 'message-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(logical_id.starts_with("lm_"));
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 16")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(version, 16);

        pool.close().await;
        let _ = std::fs::remove_file(database_path);
    }

    #[tokio::test]
    async fn logical_identity_migration_updates_large_legacy_database() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in include_str!("migrations/001_initial.sql").split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&pool).await.unwrap();
            }
        }
        for statement in [
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY)",
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('account-1', 'Legacy', 'legacy@example.test', 'imap', 'password')",
            "INSERT INTO folders (id, account_id, name, full_path) VALUES ('folder-1', 'account-1', 'Inbox', 'INBOX')",
            r#"WITH RECURSIVE sequence(value) AS (
                   SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 2005
               )
               INSERT INTO messages (id, account_id, folder_id, uid, subject)
               SELECT printf('message-%04d', value), 'account-1', 'folder-1', value, 'Legacy'
               FROM sequence"#,
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        apply_migrations(
            &pool,
            &[Migration {
                version: 16,
                name: "logical_message_id",
                apply: in_transaction!(run_logical_message_id_migration),
            }],
        )
        .await
        .unwrap();

        let migrated: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE logical_id LIKE 'lm_%'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(migrated, 2005);
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 16")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(version, 16);
    }

    #[tokio::test]
    async fn logical_identity_migration_rolls_back_failing_batch() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in include_str!("migrations/001_initial.sql").split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&pool).await.unwrap();
            }
        }
        for statement in [
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY)",
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('account-1', 'Legacy', 'legacy@example.test', 'imap', 'password')",
            "INSERT INTO folders (id, account_id, name, full_path) VALUES ('folder-1', 'account-1', 'Inbox', 'INBOX')",
            r#"WITH RECURSIVE sequence(value) AS (
                   SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 10
               )
               INSERT INTO messages (id, account_id, folder_id, uid, subject)
               SELECT printf('message-%04d', value), 'account-1', 'folder-1', value, 'Legacy'
               FROM sequence"#,
            r#"CREATE TRIGGER reject_logical_identity
               BEFORE UPDATE OF logical_id ON messages
               WHEN old.id = 'message-0005'
               BEGIN SELECT RAISE(ABORT, 'injected migration failure'); END"#,
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        let error = apply_migrations(
            &pool,
            &[Migration {
                version: 16,
                name: "logical_message_id",
                apply: in_transaction!(run_logical_message_id_migration),
            }],
        )
        .await
        .expect_err("the injected update failure should abort the migration");
        assert!(format!("{error:#}").contains("injected migration failure"));
        let unchanged: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE logical_id = ''")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, 10, "the failing step must be rolled back");
        let applied: Option<i64> =
            sqlx::query_scalar("SELECT version FROM schema_migrations WHERE version = 16")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(applied, None);
    }

    #[tokio::test]
    async fn file_database_is_private_to_the_owner() {
        let dir = std::env::temp_dir().join(format!("email-rs-perms-{}", uuid::Uuid::new_v4()));
        let database_path = dir.join("email.db");
        let url = format!("sqlite://{}", database_path.display());

        let (pool, _) = super::create_pool(&url).await.unwrap();
        pool.close().await;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700, "data dir mode {dir_mode:o}");
            for suffix in ["", "-wal", "-shm"] {
                let path = std::path::PathBuf::from(format!("{}{suffix}", database_path.display()));
                if path.exists() {
                    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
                    assert_eq!(mode, 0o600, "{} mode {mode:o}", path.display());
                }
                let _ = std::fs::remove_file(&path);
            }
        }
        let _ = std::fs::remove_dir(&dir);
    }

    #[tokio::test]
    async fn existing_shared_parent_is_left_alone() {
        let parent = std::env::temp_dir().join(format!("email-rs-shared-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&parent).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dir = parent.join("data");
        let database_path = dir.join("email.db");
        let url = format!("sqlite://{}", database_path.display());
        let (pool, _) = super::create_pool(&url).await.unwrap();
        pool.close().await;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let parent_mode = std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                parent_mode, 0o755,
                "pre-existing parent must not be chmodded"
            );
            let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700, "created directory mode {dir_mode:o}");
        }
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", database_path.display()));
        }
        let _ = std::fs::remove_dir(&dir);
        let _ = std::fs::remove_dir(&parent);
    }

    #[tokio::test]
    async fn webhook_secret_migration_seals_plaintext_and_is_idempotent() {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
        let pool = super::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap()
            .0;
        sqlx::query(
            "INSERT INTO webhooks (id, url, secret, events) VALUES ('legacy-hook', 'https://example.test', 'legacy-secret', 'all')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM schema_migrations WHERE version = 22")
            .execute(&pool)
            .await
            .unwrap();

        run_migrations(&pool).await.unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT secret FROM webhooks WHERE id = 'legacy-hook'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_ne!(stored, "legacy-secret");
        assert_eq!(crate::credentials::open(&stored).unwrap(), "legacy-secret");

        run_migrations(&pool).await.unwrap();
        let applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations WHERE version = 22")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(applied, 1);
    }

    #[tokio::test]
    async fn world_writable_unsticky_directory_is_refused() {
        let dir = std::env::temp_dir().join(format!("email-rs-ww-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
            let url = format!("sqlite://{}/email.db", dir.display());
            let error = super::create_pool(&url).await.unwrap_err();
            assert!(error.to_string().contains("world-writable"), "{error}");
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o777, "refused directory must not be chmodded");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unwritable_data_dir_fails() {
        let blocker = std::env::temp_dir().join(format!("email-rs-file-{}", uuid::Uuid::new_v4()));
        std::fs::write(&blocker, b"not a directory").unwrap();
        let url = format!("sqlite://{}/email.db", blocker.display());
        let error = super::create_pool(&url).await.unwrap_err();
        assert!(!error.to_string().is_empty());
        let _ = std::fs::remove_file(&blocker);
    }

    async fn memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                "sqlite::memory:"
                    .parse::<SqliteConnectOptions>()
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn attachment_blob_migration_moves_content_to_files_and_resumes() {
        let tmp = tempfile::tempdir().unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(tmp.path().join("email.db"))
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        for statement in include_str!("migrations/001_initial.sql").split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&pool).await.unwrap();
            }
        }
        for statement in [
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('account-1', 'Legacy', 'legacy@example.test', 'imap', 'password')",
            "INSERT INTO folders (id, account_id, name, full_path) VALUES ('folder-1', 'account-1', 'Inbox', 'INBOX')",
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id, from_email, date, subject) VALUES ('message-1', 'account-1', 'folder-1', 7, '<legacy@example>', 'sender@example.test', '2026-07-12T00:00:00Z', 'Legacy')",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        // More rows than one batch, with duplicate content to share a file.
        let total = ATTACHMENT_FILE_MIGRATION_BATCH_SIZE + 5;
        for i in 0..total {
            sqlx::query(
                "INSERT INTO attachments (id, message_id, filename, content_type, part_id, size_bytes, content) VALUES (?, 'message-1', 'f', 'text/plain', ?, 1, ?)",
            )
            .bind(format!("att-{i}"))
            .bind(format!("{i}"))
            .bind(format!("content-{}", i % 10).into_bytes())
            .execute(&pool)
            .await
            .unwrap();
        }

        // Simulate an interrupted run: schema exists, first batch moved only.
        run_migrations(&pool).await.unwrap();
        sqlx::query("DELETE FROM schema_migrations WHERE version = 24")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE attachments SET file_sha256 = NULL, content = CAST('content-3' AS BLOB) WHERE id = 'att-3'")
            .execute(&pool)
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();

        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attachments WHERE content IS NOT NULL OR file_sha256 IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0);
        let digest: String =
            sqlx::query_scalar("SELECT file_sha256 FROM attachments WHERE id = 'att-3'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let dir = tmp.path().join("attachments");
        let stored = std::fs::read(dir.join(&digest[..2]).join(&digest)).unwrap();
        assert_eq!(stored, b"content-3");
        let files = std::fs::read_dir(&dir)
            .unwrap()
            .flat_map(|shard| std::fs::read_dir(shard.unwrap().path()).unwrap())
            .count();
        assert_eq!(files, 10);
    }

    #[tokio::test]
    async fn migrations_are_strictly_ordered() {
        let versions: Vec<i64> = migrations().iter().map(|step| step.version).collect();
        assert!(versions.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[tokio::test]
    async fn upgrade_from_001_schema_reaches_latest_with_data_intact() {
        let pool = memory_pool().await;
        // A version-001 database: only the initial schema, no version table.
        for statement in include_str!("migrations/001_initial.sql").split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&pool).await.unwrap();
            }
        }
        for statement in [
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('account-1', 'Legacy', 'legacy@example.test', 'imap', 'password')",
            "INSERT INTO folders (id, account_id, name, full_path) VALUES ('folder-1', 'account-1', 'Inbox', 'INBOX')",
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id, from_email, date, subject) VALUES ('message-1', 'account-1', 'folder-1', 7, '<legacy@example>', 'sender@example.test', '2026-07-12T00:00:00Z', 'Legacy')",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        run_migrations(&pool).await.unwrap();

        let latest = migrations().last().unwrap().version;
        let applied: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
                .fetch_all(&pool)
                .await
                .unwrap();
        let expected: Vec<i64> = migrations().iter().map(|step| step.version).collect();
        assert_eq!(applied, expected);
        assert_eq!(applied.last(), Some(&latest));

        async fn columns(pool: &SqlitePool, table: &str) -> Vec<String> {
            sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(pool)
                .await
                .unwrap()
        }
        for (table, column) in [
            ("messages", "uid_validity"),
            ("messages", "logical_id"),
            ("accounts", "provider_tenant_id"),
            ("accounts", "provider_object_id"),
            ("attachments", "part_id"),
            ("message_bodies", "clean_body"),
            ("message_bodies", "search_version"),
        ] {
            assert!(
                columns(&pool, table)
                    .await
                    .iter()
                    .any(|name| name == column),
                "{table}.{column} missing after upgrade"
            );
        }
        for table in [
            "webhooks",
            "app_users",
            "app_sessions",
            "integration_tokens",
            "outbox",
            "migration_backups",
            "account_reconciliation",
        ] {
            let found: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(found, 1, "table {table} missing after upgrade");
        }

        let (subject, message_id, logical_id): (String, String, String) = sqlx::query_as(
            "SELECT subject, message_id, logical_id FROM messages WHERE id = 'message-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(subject, "Legacy");
        assert_eq!(message_id, "<legacy@example>");
        assert!(logical_id.starts_with("lm_"));
        let account: String =
            sqlx::query_scalar("SELECT email FROM accounts WHERE id = 'account-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(account, "legacy@example.test");

        // A second start applies nothing and keeps the data.
        run_migrations(&pool).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count as usize, expected.len());
    }

    #[tokio::test]
    async fn newer_schema_than_the_binary_knows_is_refused() {
        let pool = memory_pool().await;
        run_migrations(&pool).await.unwrap();
        let latest = migrations().last().unwrap().version;
        sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
            .bind(latest + 1)
            .execute(&pool)
            .await
            .unwrap();

        let error = run_migrations(&pool).await.unwrap_err();
        assert!(
            error.to_string().contains("newer than this build"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn failing_step_rolls_back_and_is_not_recorded() {
        fn failing(conn: &mut SqliteConnection) -> StepFuture<'_> {
            Box::pin(async move {
                sqlx::query("CREATE TABLE half_applied (id INTEGER)")
                    .execute(&mut *conn)
                    .await?;
                anyhow::bail!("injected step failure")
            })
        }
        let pool = memory_pool().await;
        let steps = [Migration {
            version: 1,
            name: "failing",
            apply: Apply::Transaction(failing),
        }];

        let error = apply_migrations(&pool, &steps).await.unwrap_err();
        assert!(format!("{error:#}").contains("injected step failure"));
        let table: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = 'half_applied'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(table, 0, "the step's DDL must be rolled back");
        let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(recorded, 0);
    }

    #[tokio::test]
    async fn attachment_name_migration_upgrades_an_index_without_filenames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("email.db").display().to_string();
        let (pool, has_fts) = create_pool(&format!("sqlite:{path}")).await.unwrap();
        assert!(has_fts);
        sqlx::query(
            "INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('a','A','a@x','generic_imap','password')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO folders (id, account_id, name, full_path) VALUES ('f','a','INBOX','INBOX')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO messages (id, account_id, folder_id, uid, subject, message_id) VALUES ('m','a','f',1,'Hi','<m>')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO attachments (id, message_id, filename, content_type, part_id) VALUES ('t','m','oldlayout.pdf','application/pdf','1')")
            .execute(&pool)
            .await
            .unwrap();

        // Put back the index as release 0.7.x left it: no `filenames`.
        for trigger in FTS_TRIGGERS {
            sqlx::query(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("DROP TABLE messages_fts")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE VIRTUAL TABLE messages_fts USING fts5(message_id UNINDEXED, subject, sender, recipients, body, quoted, tokenize = 'unicode61')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM schema_migrations WHERE version = 25")
            .execute(&pool)
            .await
            .unwrap();

        assert!(migrate(&pool).await.unwrap());
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('messages_fts')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(columns.iter().any(|c| c == "filenames"), "{columns:?}");
        let hits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'oldlayout'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(hits, 1);
    }
}
