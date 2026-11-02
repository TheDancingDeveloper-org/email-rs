# Backup and restore (SQLite)

email-rs keeps all state in one SQLite database (`DATABASE_URL`, `/data/email.db`
in the container image) in WAL mode. Do not copy `email.db` with `cp` while the
server runs: the live state is split across `email.db`, `email.db-wal` and
`email.db-shm`, and a plain copy can be torn or missing recent writes. Use one
of the two online methods below. Both are safe while the server is serving.

## Back up

### Online backup API (recommended)

```sh
sqlite3 /data/email.db ".backup '/backups/email-$(date +%F).db'"
```

`.backup` uses SQLite's online backup API. It copies a consistent snapshot even
while writers are active, and the result is a single standalone file (no WAL).

### `VACUUM INTO`

```sh
sqlite3 /data/email.db "VACUUM INTO '/backups/email-$(date +%F).db'"
```

This writes a compacted, consistent copy. The target file must not already
exist. It is slower than `.backup` on large databases but also defragments.

In Docker: `docker compose exec email-rs sqlite3 ...` if the image ships the
CLI, otherwise run `sqlite3` on the host against the mounted data volume.

### Protect the backup

The database holds plaintext mail and the encrypted account credentials. Create
backups with mode `0600` (`umask 077`), store them encrypted, and keep the
credential key material (the secrets that unlock stored account credentials,
as configured for your deployment) separate from, but retrievable alongside,
the backup: a database backup without it cannot decrypt account passwords or
tokens, and the accounts must be re-authorised.

### Verify every backup

```sh
sqlite3 /backups/email-2026-01-01.db "PRAGMA integrity_check;"
```

The output must be exactly `ok`. Run this after each backup, and periodically
test a full restore (below) into a scratch directory.

## Restore

1. Stop the server (`docker compose stop email-rs`). Never restore under a
   running server.
2. Move the damaged database and its sidecar files aside:
   `mv email.db email.db.bad; rm -f email.db-wal email.db-shm` (the stale WAL
   must not be replayed onto the restored file).
3. Copy the backup into place as `email.db`, owned by the server user, mode
   `0600`.
4. Start the server. Migrations run on startup and bring an older backup up to
   the current schema. A backup from a *newer* release than the running binary
   is not supported.
5. Check `GET /api/v1/ready`, then the startup log for
   `database quick_check passed`.

After a restore, the next sync fetches anything newer than the backup from the
mail servers, so only local-only state (rules, webhooks, drafts and outbox
items created after the backup) is lost.

## Startup integrity check

On every start the server runs `PRAGMA quick_check` on the database after
migrations (before serving). Outcomes:

- `database quick_check passed` (info): the database is healthy.
- `database quick_check reported corruption` (error): startup aborts with
  `database integrity check failed: ...`. Restore from the latest good backup.
- If you must inspect a damaged database anyway, set
  `EMAIL_RS_ALLOW_CORRUPT_DATABASE=true`. The server then logs a warning and
  continues. Use it only to export data, then restore a clean backup; it is not
  a normal operating mode.

`quick_check` is a fast structural check. For a full check, including index
consistency, run `PRAGMA integrity_check;` against a backup copy.

## Observability

- **Logs.** Text by default. Set `EMAIL_RS_LOG_FORMAT=json` for one JSON object
  per line (for Loki, Elastic, CloudWatch). `RUST_LOG` still sets the level.
- **Metrics.** `GET /api/v1/metrics` returns Prometheus text format:
  `email_rs_sync_runs_total`, `email_rs_sync_errors_total`,
  `email_rs_messages_synced_total`, `email_rs_webhook_deliveries_total`,
  `email_rs_webhook_failures_total`, `email_rs_http_requests_total{method,route,status}`
  and the histogram `email_rs_http_request_duration_seconds`. HTTP series use the
  route template, not the raw path, so cardinality stays bounded. Counters reset
  when the process restarts.

  **Access choice:** the endpoint sits behind the same authentication as the rest
  of the API (session cookie or bearer token), so it is never exposed
  unauthenticated. A Prometheus scraper therefore needs a bearer token
  (`authorization: Bearer ...` in `scrape_config`). When auth is disabled with
  `EMAIL_RS_DISABLE_AUTH`, the server is already restricted to a loopback bind
  unless explicitly opened, and `/metrics` is as reachable as the rest of the
  API. Do not publish `/api/v1/metrics` through a public reverse proxy route
  without auth; restrict it in Caddy to your scraper's address if you prefer.
