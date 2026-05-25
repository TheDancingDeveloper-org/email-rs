use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

const CREATED_AT: &str = "2026-05-25T09:00:00Z";
const SYNCED_AT: &str = "2026-05-25T09:30:00Z";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let out_dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures/fluentgui-phase0"));
    fs::create_dir_all(&out_dir)?;

    create_fixture(&out_dir.join("no-accounts.sqlite"), FixtureKind::NoAccounts).await?;
    create_fixture(&out_dir.join("one-account.sqlite"), FixtureKind::OneAccount).await?;
    create_fixture(
        &out_dir.join("two-accounts.sqlite"),
        FixtureKind::TwoAccounts,
    )
    .await?;

    println!("wrote FluentGUI Phase 0 fixtures to {}", out_dir.display());
    Ok(())
}

#[derive(Clone, Copy)]
enum FixtureKind {
    NoAccounts,
    OneAccount,
    TwoAccounts,
}

async fn create_fixture(path: &Path, kind: FixtureKind) -> anyhow::Result<()> {
    remove_existing(path)?;

    let pool = create_pool(path).await?;
    match kind {
        FixtureKind::NoAccounts => {}
        FixtureKind::OneAccount => {
            seed_personal_account(&pool).await?;
        }
        FixtureKind::TwoAccounts => {
            seed_personal_account(&pool).await?;
            seed_work_account(&pool).await?;
        }
    }

    refresh_folder_counts(&pool).await?;
    let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await;
    pool.close().await;
    println!("wrote {}", path.display());
    Ok(())
}

fn remove_existing(path: &Path) -> anyhow::Result<()> {
    for candidate in [
        path.to_path_buf(),
        sidecar(path, "-wal"),
        sidecar(path, "-shm"),
    ] {
        match fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("fixture.sqlite")
        .to_string();
    name.push_str(suffix);
    path.with_file_name(name)
}

async fn create_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let url = format!("sqlite:{}", path.to_string_lossy());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;

    run_migrations(&pool).await?;
    if let Err(err) = run_fts_migration(&pool).await {
        eprintln!("warning: FTS5 unavailable for {}: {err}", path.display());
    }

    Ok(pool)
}

async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    for migration in [
        include_str!("../db/migrations/001_initial.sql"),
        include_str!("../db/migrations/002_token_expiry.sql"),
        include_str!("../db/migrations/003_settings.sql"),
        include_str!("../db/migrations/004_webhooks.sql"),
        include_str!("../db/migrations/006_password.sql"),
        include_str!("../db/migrations/007_calendar_links.sql"),
        include_str!("../db/migrations/010_labels.sql"),
        include_str!("../db/migrations/009_rules.sql"),
        include_str!("../db/migrations/008_snooze.sql"),
        include_str!("../db/migrations/011_accounts_unique.sql"),
    ] {
        execute_statements(pool, migration).await?;
    }
    Ok(())
}

async fn execute_statements(pool: &SqlitePool, sql: &str) -> anyhow::Result<()> {
    for statement in sql.split(';') {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(pool).await?;
        }
    }
    Ok(())
}

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

    Ok(())
}

async fn seed_personal_account(pool: &SqlitePool) -> anyhow::Result<()> {
    insert_account(
        pool,
        "fx-a1",
        "Personal Gmail Fixture",
        "personal.gmail@example.test",
        "generic_imap",
        "basic",
        "imap.example.test",
        993,
        "smtp.example.test",
        587,
        Some(30),
        Some("Best,\nPersonal Fixture"),
    )
    .await?;

    seed_standard_folders(pool, "fx-a1", "personal").await?;

    insert_label(
        pool,
        "fx-a1-label-important",
        "fx-a1",
        "Important",
        "#b42318",
    )
    .await?;
    insert_label(pool, "fx-a1-label-project", "fx-a1", "FluentGUI", "#2563eb").await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-html-unread",
            account_id: "fx-a1",
            folder_id: "fx-a1-inbox",
            uid: 101,
            message_id: "<fx-a1-msg-html-unread@example.test>",
            thread_id: Some("fx-a1-thread-welcome"),
            subject: "HTML rendering fixture for FluentGUI",
            from_name: Some("Alex Sender"),
            from_email: Some("alex.sender@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-25T14:10:00Z",
            is_read: false,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some("HTML message with headings, links, tables, and inline styling."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-html-unread",
        Some(
            r#"<!doctype html><html><body><h1>FluentGUI HTML Fixture</h1><p>This message verifies <strong>rich HTML</strong>, links, tables, and safe styling.</p><table border="1" cellpadding="4"><tr><th>Area</th><th>Status</th></tr><tr><td>Rendering</td><td>Fixture</td></tr></table><p><a href="https://example.test/fluentgui">Example link</a></p></body></html>"#,
        ),
        Some("FluentGUI HTML Fixture\nThis message verifies rich HTML, links, tables, and safe styling."),
    )
    .await?;
    link_label(pool, "fx-a1-msg-html-unread", "fx-a1-label-important").await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-thread-older",
            account_id: "fx-a1",
            folder_id: "fx-a1-inbox",
            uid: 102,
            message_id: "<fx-a1-msg-thread-older@example.test>",
            thread_id: Some("fx-a1-thread-alpha"),
            subject: "Threaded migration discussion",
            from_name: Some("Casey Thread"),
            from_email: Some("casey.thread@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-24T11:00:00Z",
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some("Older thread item used to verify expand and collapse."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-thread-older",
        None,
        Some("Older thread item used to verify expand and collapse."),
    )
    .await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-thread-latest",
            account_id: "fx-a1",
            folder_id: "fx-a1-inbox",
            uid: 103,
            message_id: "<fx-a1-msg-thread-latest@example.test>",
            thread_id: Some("fx-a1-thread-alpha"),
            subject: "Re: Threaded migration discussion",
            from_name: Some("Casey Thread"),
            from_email: Some("casey.thread@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-25T10:45:00Z",
            is_read: false,
            is_flagged: true,
            is_draft: false,
            has_attachments: false,
            preview: Some("Latest thread item should appear first and show the thread count."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-thread-latest",
        None,
        Some("Latest thread item should appear first and show the thread count."),
    )
    .await?;
    link_label(pool, "fx-a1-msg-thread-latest", "fx-a1-label-project").await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-attachment",
            account_id: "fx-a1",
            folder_id: "fx-a1-inbox",
            uid: 104,
            message_id: "<fx-a1-msg-attachment@example.test>",
            thread_id: None,
            subject: "Attachment indicator fixture",
            from_name: Some("Dana Attachments"),
            from_email: Some("dana.attachments@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-23T16:20:00Z",
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: true,
            preview: Some("This row has a stored attachment record and hasAttachments set."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-attachment",
        None,
        Some("This row has a stored attachment record and hasAttachments set."),
    )
    .await?;
    insert_attachment(
        pool,
        "fx-a1-att-1",
        "fx-a1-msg-attachment",
        "migration-plan.txt",
        "text/plain",
        b"fixture attachment",
    )
    .await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-snoozed",
            account_id: "fx-a1",
            folder_id: "fx-a1-inbox",
            uid: 105,
            message_id: "<fx-a1-msg-snoozed@example.test>",
            thread_id: None,
            subject: "Snoozed future fixture",
            from_name: Some("Morgan Later"),
            from_email: Some("morgan.later@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-22T09:00:00Z",
            is_read: false,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some("Future snoozed message for smart-folder verification."),
            snoozed_until: Some("2099-01-08T09:00:00Z"),
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-snoozed",
        None,
        Some("Future snoozed message for smart-folder verification."),
    )
    .await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-text-only",
            account_id: "fx-a1",
            folder_id: "fx-a1-projects-fluent",
            uid: 201,
            message_id: "<fx-a1-msg-text-only@example.test>",
            thread_id: None,
            subject: "Text-only fixture in nested folder",
            from_name: Some("Taylor Text"),
            from_email: Some("taylor.text@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"["observer@example.test"]"#,
            date: "2026-05-21T12:30:00Z",
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some("Plain text body verifies fallback rendering."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-text-only",
        None,
        Some("Plain text body verifies fallback rendering.\nSecond paragraph for wrapping."),
    )
    .await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-archive",
            account_id: "fx-a1",
            folder_id: "fx-a1-archive",
            uid: 301,
            message_id: "<fx-a1-msg-archive@example.test>",
            thread_id: None,
            subject: "Archived message fixture",
            from_name: Some("Archive Sender"),
            from_email: Some("archive.sender@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-20T08:15:00Z",
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some("Message starts in Archive."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-archive",
        None,
        Some("Message starts in Archive."),
    )
    .await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-trash",
            account_id: "fx-a1",
            folder_id: "fx-a1-trash",
            uid: 401,
            message_id: "<fx-a1-msg-trash@example.test>",
            thread_id: None,
            subject: "Trash message fixture",
            from_name: Some("Trash Sender"),
            from_email: Some("trash.sender@example.test"),
            to_json: r#"["personal.gmail@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-19T08:15:00Z",
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some("Message starts in Trash."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-trash",
        None,
        Some("Message starts in Trash."),
    )
    .await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a1-msg-draft",
            account_id: "fx-a1",
            folder_id: "fx-a1-drafts",
            uid: 501,
            message_id: "<fx-a1-msg-draft@example.test>",
            thread_id: None,
            subject: "Draft fixture",
            from_name: Some("Personal Gmail Fixture"),
            from_email: Some("personal.gmail@example.test"),
            to_json: r#"["draft.recipient@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-18T08:15:00Z",
            is_read: true,
            is_flagged: false,
            is_draft: true,
            has_attachments: false,
            preview: Some("Draft-state row for list parity."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a1-msg-draft",
        None,
        Some("Draft-state row for list parity."),
    )
    .await?;

    seed_calendar(pool, "fx-a1", "fx-a1-msg-html-unread").await?;
    seed_rules(pool, "fx-a1", "fx-a1-projects-fluent").await?;
    seed_webhooks(pool, "fx-a1").await?;
    Ok(())
}

async fn seed_work_account(pool: &SqlitePool) -> anyhow::Result<()> {
    insert_account(
        pool,
        "fx-a2",
        "Work Microsoft Fixture",
        "work.microsoft@example.test",
        "microsoft365",
        "app_password",
        "outlook.office365.com",
        993,
        "smtp.office365.com",
        587,
        None,
        Some("Regards,\nWork Fixture"),
    )
    .await?;

    seed_standard_folders(pool, "fx-a2", "work").await?;

    insert_label(pool, "fx-a2-label-vip", "fx-a2", "VIP", "#7c3aed").await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a2-msg-unread",
            account_id: "fx-a2",
            folder_id: "fx-a2-inbox",
            uid: 101,
            message_id: "<fx-a2-msg-unread@example.test>",
            thread_id: None,
            subject: "Second account unread fixture",
            from_name: Some("Work Sender"),
            from_email: Some("work.sender@example.test"),
            to_json: r#"["work.microsoft@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-25T13:00:00Z",
            is_read: false,
            is_flagged: true,
            is_draft: false,
            has_attachments: false,
            preview: Some("Verifies multi-account sidebar, search, unread, and flagged views."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a2-msg-unread",
        None,
        Some("Verifies multi-account sidebar, search, unread, and flagged views."),
    )
    .await?;
    link_label(pool, "fx-a2-msg-unread", "fx-a2-label-vip").await?;

    insert_message(
        pool,
        MessageSeed {
            id: "fx-a2-msg-sent",
            account_id: "fx-a2",
            folder_id: "fx-a2-sent",
            uid: 201,
            message_id: "<fx-a2-msg-sent@example.test>",
            thread_id: None,
            subject: "Sent message fixture",
            from_name: Some("Work Microsoft Fixture"),
            from_email: Some("work.microsoft@example.test"),
            to_json: r#"["teammate@example.test"]"#,
            cc_json: r#"[]"#,
            date: "2026-05-24T17:00:00Z",
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some("Second account sent folder message."),
            snoozed_until: None,
        },
    )
    .await?;
    insert_body(
        pool,
        "fx-a2-msg-sent",
        None,
        Some("Second account sent folder message."),
    )
    .await?;

    seed_rules(pool, "fx-a2", "fx-a2-archive").await?;
    seed_webhooks(pool, "fx-a2").await?;
    Ok(())
}

async fn insert_account(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    email: &str,
    provider_type: &str,
    auth_type: &str,
    host: &str,
    port: i64,
    smtp_host: &str,
    smtp_port: i64,
    sync_days_limit: Option<i64>,
    signature: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO accounts
           (id, name, email, provider_type, auth_type, host, port, use_ssl,
            password, smtp_host, smtp_port, smtp_password, sync_days_limit,
            signature, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(id)
    .bind(name)
    .bind(email)
    .bind(provider_type)
    .bind(auth_type)
    .bind(host)
    .bind(port)
    .bind("fixture-password")
    .bind(smtp_host)
    .bind(smtp_port)
    .bind("fixture-password")
    .bind(sync_days_limit)
    .bind(signature)
    .bind(CREATED_AT)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_standard_folders(
    pool: &SqlitePool,
    account_id: &str,
    prefix: &str,
) -> anyhow::Result<()> {
    for (suffix, name, full_path, special_use, excluded) in [
        ("inbox", "Inbox", "INBOX", Some("inbox"), false),
        ("sent", "Sent", "Sent", Some("sent"), false),
        ("drafts", "Drafts", "Drafts", Some("drafts"), false),
        ("archive", "Archive", "Archive", Some("archive"), false),
        ("trash", "Trash", "Trash", Some("trash"), false),
        ("spam", "Spam", "Spam", Some("spam"), false),
        ("projects", "Projects", "Projects", None, false),
        (
            "projects-fluent",
            "FluentGUI",
            "Projects/FluentGUI",
            None,
            false,
        ),
        ("receipts", "Receipts", "Receipts", None, true),
    ] {
        insert_folder(
            pool,
            &format!("{account_id}-{suffix}"),
            account_id,
            name,
            full_path,
            special_use,
            excluded,
            prefix,
        )
        .await?;
    }
    Ok(())
}

async fn insert_folder(
    pool: &SqlitePool,
    id: &str,
    account_id: &str,
    name: &str,
    full_path: &str,
    special_use: Option<&str>,
    excluded: bool,
    _prefix: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO folders
           (id, account_id, name, full_path, special_use, unread_count, total_count, synced_at, is_excluded)
           VALUES (?, ?, ?, ?, ?, 0, 0, ?, ?)"#,
    )
    .bind(id)
    .bind(account_id)
    .bind(name)
    .bind(full_path)
    .bind(special_use)
    .bind(SYNCED_AT)
    .bind(excluded)
    .execute(pool)
    .await?;
    Ok(())
}

struct MessageSeed<'a> {
    id: &'a str,
    account_id: &'a str,
    folder_id: &'a str,
    uid: i64,
    message_id: &'a str,
    thread_id: Option<&'a str>,
    subject: &'a str,
    from_name: Option<&'a str>,
    from_email: Option<&'a str>,
    to_json: &'a str,
    cc_json: &'a str,
    date: &'a str,
    is_read: bool,
    is_flagged: bool,
    is_draft: bool,
    has_attachments: bool,
    preview: Option<&'a str>,
    snoozed_until: Option<&'a str>,
}

async fn insert_message(pool: &SqlitePool, msg: MessageSeed<'_>) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO messages
           (id, account_id, folder_id, uid, message_id, thread_id, subject,
            from_name, from_email, to_json, cc_json, date, is_read, is_flagged,
            is_draft, has_attachments, preview, snoozed_until, synced_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(msg.id)
    .bind(msg.account_id)
    .bind(msg.folder_id)
    .bind(msg.uid)
    .bind(msg.message_id)
    .bind(msg.thread_id)
    .bind(msg.subject)
    .bind(msg.from_name)
    .bind(msg.from_email)
    .bind(msg.to_json)
    .bind(msg.cc_json)
    .bind(msg.date)
    .bind(msg.is_read)
    .bind(msg.is_flagged)
    .bind(msg.is_draft)
    .bind(msg.has_attachments)
    .bind(msg.preview)
    .bind(msg.snoozed_until)
    .bind(SYNCED_AT)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_body(
    pool: &SqlitePool,
    message_id: &str,
    html_body: Option<&str>,
    text_body: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO message_bodies (message_id, html_body, text_body, raw_headers, fetched_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(message_id)
    .bind(html_body)
    .bind(text_body)
    .bind("From: fixture@example.test\r\nTo: fixture@example.test\r\n")
    .bind(SYNCED_AT)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_attachment(
    pool: &SqlitePool,
    id: &str,
    message_id: &str,
    filename: &str,
    content_type: &str,
    content: &[u8],
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO attachments (id, message_id, filename, content_type, size_bytes, content)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(message_id)
    .bind(filename)
    .bind(content_type)
    .bind(content.len() as i64)
    .bind(content)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_label(
    pool: &SqlitePool,
    id: &str,
    account_id: &str,
    name: &str,
    color: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO labels (id, account_id, name, color, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(account_id)
    .bind(name)
    .bind(color)
    .bind(CREATED_AT)
    .execute(pool)
    .await?;
    Ok(())
}

async fn link_label(pool: &SqlitePool, message_id: &str, label_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO message_labels (id, message_id, label_id, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(format!("{message_id}-{label_id}"))
    .bind(message_id)
    .bind(label_id)
    .bind(CREATED_AT)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_calendar(
    pool: &SqlitePool,
    account_id: &str,
    linked_message_id: &str,
) -> anyhow::Result<()> {
    insert_event(
        pool,
        EventSeed {
            id: "fx-cal-all-day",
            account_id,
            calendar_id: "primary",
            provider_event_id: Some("fixture-all-day"),
            title: "All-day FluentGUI audit",
            description: Some("All-day fixture event for the calendar row."),
            start_at: "2026-05-25T00:00:00Z",
            end_at: "2026-05-26T00:00:00Z",
            location: Some("Remote"),
            is_all_day: true,
            attendees_json: r#"[{"email":"alex.sender@example.test","name":"Alex Sender","responseStatus":"accepted"}]"#,
            meet_link: Some("https://meet.example.test/fluentgui-audit"),
        },
    )
    .await?;
    insert_event(
        pool,
        EventSeed {
            id: "fx-cal-timed",
            account_id,
            calendar_id: "primary",
            provider_event_id: Some("fixture-timed"),
            title: "Native mail parity review",
            description: Some("Timed event with attendees, location, meet link, and linked email."),
            start_at: "2026-05-25T15:00:00Z",
            end_at: "2026-05-25T16:30:00Z",
            location: Some("Conference Room 3"),
            is_all_day: false,
            attendees_json: r#"[{"email":"casey.thread@example.test","name":"Casey Thread","responseStatus":"tentative"},{"email":"personal.gmail@example.test","name":"Personal Fixture","responseStatus":"accepted"}]"#,
            meet_link: Some("https://meet.example.test/native-mail-parity"),
        },
    )
    .await?;

    sqlx::query(
        "INSERT INTO message_calendar_links (id, message_id, event_id, linked_at) VALUES (?, ?, ?, ?)",
    )
    .bind("fx-link-cal-email")
    .bind(linked_message_id)
    .bind("fx-cal-timed")
    .bind("2026-05-25T09:45:00Z")
    .execute(pool)
    .await?;
    Ok(())
}

struct EventSeed<'a> {
    id: &'a str,
    account_id: &'a str,
    calendar_id: &'a str,
    provider_event_id: Option<&'a str>,
    title: &'a str,
    description: Option<&'a str>,
    start_at: &'a str,
    end_at: &'a str,
    location: Option<&'a str>,
    is_all_day: bool,
    attendees_json: &'a str,
    meet_link: Option<&'a str>,
}

async fn insert_event(pool: &SqlitePool, event: EventSeed<'_>) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO calendar_events
           (id, account_id, calendar_id, provider_event_id, title, description,
            start_at, end_at, location, is_all_day, recurrence_rule, attendees_json,
            meet_link, synced_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)"#,
    )
    .bind(event.id)
    .bind(event.account_id)
    .bind(event.calendar_id)
    .bind(event.provider_event_id)
    .bind(event.title)
    .bind(event.description)
    .bind(event.start_at)
    .bind(event.end_at)
    .bind(event.location)
    .bind(event.is_all_day)
    .bind(event.attendees_json)
    .bind(event.meet_link)
    .bind(SYNCED_AT)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_rules(
    pool: &SqlitePool,
    account_id: &str,
    move_folder_id: &str,
) -> anyhow::Result<()> {
    let rule_id = format!("{account_id}-rule-priority");
    sqlx::query(
        "INSERT INTO rules (id, account_id, name, is_active, match_mode, priority, created_at)
         VALUES (?, ?, ?, 1, 'all', 10, ?)",
    )
    .bind(&rule_id)
    .bind(account_id)
    .bind("Move FluentGUI mail")
    .bind(CREATED_AT)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO rule_conditions (id, rule_id, field, operator, value, position)
         VALUES (?, ?, 'subject', 'contains', 'FluentGUI', 0)",
    )
    .bind(format!("{rule_id}-condition-subject"))
    .bind(&rule_id)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO rule_actions (id, rule_id, action_type, action_value, position)
         VALUES (?, ?, 'move_to_folder', ?, 0)",
    )
    .bind(format!("{rule_id}-action-move"))
    .bind(&rule_id)
    .bind(move_folder_id)
    .execute(pool)
    .await?;

    let inactive_rule_id = format!("{account_id}-rule-inactive");
    sqlx::query(
        "INSERT INTO rules (id, account_id, name, is_active, match_mode, priority, created_at)
         VALUES (?, ?, ?, 0, 'any', 20, ?)",
    )
    .bind(&inactive_rule_id)
    .bind(account_id)
    .bind("Flag attachment mail")
    .bind(CREATED_AT)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO rule_conditions (id, rule_id, field, operator, value, position)
         VALUES (?, ?, 'has_attachment', 'equals', 'true', 0)",
    )
    .bind(format!("{inactive_rule_id}-condition-attachment"))
    .bind(&inactive_rule_id)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO rule_actions (id, rule_id, action_type, action_value, position)
         VALUES (?, ?, 'flag', NULL, 0)",
    )
    .bind(format!("{inactive_rule_id}-action-flag"))
    .bind(&inactive_rule_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_webhooks(pool: &SqlitePool, account_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO webhooks (id, url, secret, events, account_id, enabled, created_at)
         VALUES (?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(format!("{account_id}-webhook-signed"))
    .bind("https://hooks.example.test/email-rs")
    .bind("fixture-secret")
    .bind("new_message,rule_matched")
    .bind(account_id)
    .bind(CREATED_AT)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO webhooks (id, url, secret, events, account_id, enabled, created_at)
         VALUES (?, ?, NULL, 'all', NULL, 0, ?)",
    )
    .bind(format!("{account_id}-webhook-disabled"))
    .bind("https://hooks.example.test/disabled")
    .bind(CREATED_AT)
    .execute(pool)
    .await?;
    Ok(())
}

async fn refresh_folder_counts(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE folders
         SET total_count = (SELECT COUNT(*) FROM messages WHERE messages.folder_id = folders.id),
             unread_count = (SELECT COUNT(*) FROM messages WHERE messages.folder_id = folders.id AND is_read = 0)",
    )
    .execute(pool)
    .await?;
    Ok(())
}
