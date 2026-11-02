//! Content-addressed on-disk attachment store.
//!
//! Attachment bytes live in files named by their SHA-256 under an
//! `attachments/` directory beside the SQLite file (`<ab>/<sha256>`), created
//! owner-only. SQLite keeps only metadata and the digest, so the main database
//! stays small and downloads stream from disk instead of loading whole blobs.
//! In-memory databases have no directory; callers then keep the legacy BLOB.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use axum::body::Body;
use futures::stream;
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};
use tokio::io::AsyncReadExt;

const STORE_DIR: &str = "attachments";
const READ_CHUNK: usize = 64 * 1024;
/// Files younger than this are never removed by cleanup: a file is written
/// before the transaction that references it commits.
const ORPHAN_GRACE: Duration = Duration::from_secs(3600);

/// Directory of the store for the database behind `conn`, or `None` for an
/// in-memory or temporary database.
pub async fn store_dir_conn(conn: &mut SqliteConnection) -> Option<PathBuf> {
    let file: Option<String> =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_optional(&mut *conn)
            .await
            .ok()
            .flatten();
    let file = file.filter(|file| !file.is_empty())?;
    Path::new(&file).parent().map(|parent| {
        if parent.as_os_str().is_empty() {
            PathBuf::from(STORE_DIR)
        } else {
            parent.join(STORE_DIR)
        }
    })
}

pub async fn store_dir(pool: &SqlitePool) -> Option<PathBuf> {
    let mut conn = pool.acquire().await.ok()?;
    store_dir_conn(&mut conn).await
}

pub fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn path_for(dir: &Path, digest: &str) -> PathBuf {
    dir.join(&digest[..2]).join(digest)
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// Write `content` to the store and return its SHA-256 hex digest. Identical
/// content is stored once. The write goes to a temporary file, is synced and
/// renamed into place, so a reader never sees a partial file.
pub async fn put(dir: &Path, content: &[u8]) -> std::io::Result<String> {
    let digest = hex(&Sha256::digest(content));
    let dir = dir.to_path_buf();
    let content = content.to_vec();
    let name = digest.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        use std::io::Write;
        let target = path_for(&dir, &name);
        if target.is_file() {
            std::fs::File::options()
                .write(true)
                .open(&target)?
                .set_modified(SystemTime::now())?;
            return Ok(());
        }
        let shard = target.parent().expect("sharded path has a parent");
        create_private_dir(shard)?;
        let temp = shard.join(format!(".tmp-{}-{}", std::process::id(), nanos()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        let written = file.write_all(&content).and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = written.and_then(|()| std::fs::rename(&temp, &target)) {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
        #[cfg(unix)]
        std::fs::File::open(shard)?.sync_all()?;
        Ok(())
    })
    .await
    .map_err(std::io::Error::other)??;
    Ok(digest)
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Stream a stored file as a response body. Returns `None` if the digest is
/// malformed or the file is missing.
pub async fn open_body(dir: &Path, digest: &str) -> Option<(Body, u64)> {
    if !is_digest(digest) {
        return None;
    }
    let file = tokio::fs::File::open(path_for(dir, digest)).await.ok()?;
    let len = file.metadata().await.ok()?.len();
    let body = Body::from_stream(stream::unfold(file, |mut file| async move {
        let mut buf = vec![0u8; READ_CHUNK];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok::<_, std::io::Error>(axum::body::Bytes::from(buf)), file))
            }
            Err(error) => Some((Err(error), file)),
        }
    }));
    Some((body, len))
}

/// Remove store files no attachment row references (and abandoned temporary
/// files), skipping anything younger than the grace period. Returns the count.
pub async fn remove_unreferenced(pool: &SqlitePool, dir: &Path) -> Result<u64, sqlx::Error> {
    remove_unreferenced_older_than(pool, dir, ORPHAN_GRACE).await
}

pub(crate) async fn remove_unreferenced_older_than(
    pool: &SqlitePool,
    dir: &Path,
    grace: Duration,
) -> Result<u64, sqlx::Error> {
    let mut removed = 0;
    let Ok(mut shards) = tokio::fs::read_dir(dir).await else {
        return Ok(0);
    };
    while let Ok(Some(shard)) = shards.next_entry().await {
        let Ok(mut files) = tokio::fs::read_dir(shard.path()).await else {
            continue;
        };
        while let Ok(Some(entry)) = files.next_entry().await {
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            let old_enough = meta
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age >= grace);
            if !meta.is_file() || !old_enough {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let referenced = is_digest(&name)
                && sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS (SELECT 1 FROM attachments WHERE file_sha256 = ?)",
                )
                .bind(&name)
                .fetch_one(pool)
                .await?
                    != 0;
            if !referenced && tokio::fs::remove_file(entry.path()).await.is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_is_content_addressed_private_and_streams_back() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("attachments");
        let digest = put(&dir, b"hello").await.unwrap();
        assert_eq!(
            digest,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(put(&dir, b"hello").await.unwrap(), digest);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = std::fs::metadata(path_for(&dir, &digest)).unwrap();
            assert_eq!(file.permissions().mode() & 0o777, 0o600);
            let shard = std::fs::metadata(dir.join(&digest[..2])).unwrap();
            assert_eq!(shard.permissions().mode() & 0o777, 0o700);
        }
        let (body, len) = open_body(&dir, &digest).await.unwrap();
        assert_eq!(len, 5);
        let bytes = axum::body::to_bytes(body, 1024).await.unwrap();
        assert_eq!(&bytes[..], b"hello");
        assert!(open_body(&dir, "../../etc/passwd").await.is_none());
    }

    #[tokio::test]
    async fn cleanup_removes_only_old_unreferenced_files() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("email.db");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&db)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE attachments (id TEXT PRIMARY KEY, file_sha256 TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        let dir = store_dir(&pool).await.unwrap();
        assert_eq!(dir, tmp.path().join("attachments"));
        let kept = put(&dir, b"kept").await.unwrap();
        let orphan = put(&dir, b"orphan").await.unwrap();
        sqlx::query("INSERT INTO attachments VALUES ('a', ?)")
            .bind(&kept)
            .execute(&pool)
            .await
            .unwrap();

        // Fresh files are protected by the grace period.
        assert_eq!(remove_unreferenced(&pool, &dir).await.unwrap(), 0);
        assert_eq!(
            remove_unreferenced_older_than(&pool, &dir, Duration::ZERO)
                .await
                .unwrap(),
            1
        );
        assert!(path_for(&dir, &kept).is_file());
        assert!(!path_for(&dir, &orphan).exists());
    }

    #[tokio::test]
    async fn dedup_hit_refreshes_mtime_and_protects_file_from_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("email.db");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&db)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE attachments (id TEXT PRIMARY KEY, file_sha256 TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        let dir = store_dir(&pool).await.unwrap();
        let digest = put(&dir, b"content").await.unwrap();
        let target = path_for(&dir, &digest);
        std::fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .unwrap();

        assert_eq!(put(&dir, b"content").await.unwrap(), digest);
        assert_eq!(
            remove_unreferenced_older_than(&pool, &dir, Duration::from_secs(1))
                .await
                .unwrap(),
            0
        );
        assert!(target.is_file());
    }
}
