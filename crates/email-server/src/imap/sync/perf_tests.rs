//! Large-mailbox performance fixture (email-rs #75).
//!
//! Drives `ImapSyncEngine` through the in-process mock IMAP server with a
//! lazily generated, deterministic mailbox: nothing is committed as data and
//! the server holds no per-message state until a message is fetched.
//!
//! * `perf_smoke_1k_initial_and_incremental_sync` runs in the normal suite.
//! * `perf_100k_initial_and_incremental_sync` is `#[ignore]`d. Run it in release
//!   mode and print the timings with:
//!
//!   ```text
//!   cargo test -p email-server --release imap::sync::perf_tests -- --ignored --nocapture
//!   ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::imap::mock_server::MockImapServer;

struct Timings {
    initial: Duration,
    incremental: Duration,
}

async fn count_inbox(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages m JOIN folders f ON f.id = m.folder_id WHERE f.full_path = 'INBOX'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Initial sync of `initial` messages, then an incremental sync after
/// `delta` more arrive. Asserts the stored counts and returns the timings.
async fn run(initial: u32, delta: u32) -> Timings {
    let server = MockImapServer::start().await;
    server.populate_lazy("INBOX", initial);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("email.db").display().to_string();
    let (pool, _) = crate::db::create_pool(&format!("sqlite:{path}"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('mock','M','mock@example.test','imap','basic')")
        .execute(&pool)
        .await
        .unwrap();
    let engine = ImapSyncEngine::new(pool.clone(), 60);
    let provider = || -> Arc<dyn MailProvider> { Arc::new(server.provider("mock")) };

    let start = Instant::now();
    engine
        .sync_provider("mock", provider(), None, None)
        .await
        .unwrap();
    let initial_time = start.elapsed();
    assert_eq!(count_inbox(&pool).await, i64::from(initial));

    server.populate_lazy("INBOX", delta);
    let start = Instant::now();
    engine
        .sync_provider("mock", provider(), None, None)
        .await
        .unwrap();
    let incremental_time = start.elapsed();
    assert_eq!(count_inbox(&pool).await, i64::from(initial + delta));

    eprintln!(
        "perf: initial sync of {initial} messages {initial_time:.2?}; incremental sync of {delta} new messages {incremental_time:.2?}"
    );
    Timings {
        initial: initial_time,
        incremental: incremental_time,
    }
}

#[tokio::test]
async fn perf_smoke_1k_initial_and_incremental_sync() {
    let timings = run(1_000, 100).await;
    // Generous bound: only catches pathological (e.g. quadratic) regressions.
    assert!(
        timings.initial < Duration::from_secs(120),
        "{:?}",
        timings.initial
    );
    assert!(
        timings.incremental < Duration::from_secs(60),
        "{:?}",
        timings.incremental
    );
}

#[tokio::test]
#[ignore = "100k-message perf fixture; run with --ignored --release --nocapture"]
async fn perf_100k_initial_and_incremental_sync() {
    run(100_000, 1_000).await;
}
