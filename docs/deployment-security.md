# Secure deployment

email-rs is a single-user application, but every mail, settings, webhook, sync, search, calendar, and event endpoint still requires an application session. Only login/bootstrap, OAuth callbacks, and `GET /api/v1/health` are public.

Provider secrets require `EMAIL_RS_CREDENTIAL_KEY`, a stable base64-encoded
32-byte key generated with `openssl rand -base64 32`. OAuth token JSON and
IMAP/SMTP passwords are encrypted with AES-256-GCM before SQLite writes.
Existing plaintext rows are migrated at startup. Store and back up the key in
secrets management; losing it makes provider credentials unrecoverable.

## First user

Supply the initial credential as deployment secrets on the first start:

```text
EMAIL_RS_BOOTSTRAP_USERNAME=admin
EMAIL_RS_BOOTSTRAP_PASSWORD=<a secret of at least 12 characters>
```

The password is stored only as an Argon2id hash. Once startup succeeds, remove `EMAIL_RS_BOOTSTRAP_PASSWORD` from the running environment; it is ignored after a user exists. An empty variable is also ignored. Alternatively, the browser first-run form calls the bootstrap endpoint, which accepts connections only from a direct loopback peer. Setup through a container-network reverse proxy is not loopback, so Docker deployments should use the environment secret.

The server supports an HTTP-only, SameSite=Strict cookie for browser/desktop clients and bearer tokens for native mobile clients; only SHA-256 token hashes are stored in SQLite. The browser login flow is integrated. Fluent desktop login and secure mobile token persistence are still release blockers. Logout and session revocation delete the server-side session row immediately.

## HTTPS with Caddy

The server refuses a non-loopback bind unless `EMAIL_RS_PUBLIC_URL` starts with `https://`. Caddy should be the only public listener and forward the original scheme and host:

```caddyfile
mail.example.com {
    encode zstd gzip
    reverse_proxy email-rs:3000
}
```

Run the application container on a private network with:

```text
HOST=0.0.0.0
PORT=3000
EMAIL_RS_PUBLIC_URL=https://mail.example.com
```

Caddy obtains and renews the certificate automatically when DNS points at the host and ports 80/443 are reachable. `EMAIL_RS_ALLOW_INSECURE_HTTP=true` bypasses the non-loopback check and is intended only for the checked-in local development Compose stack.

Same-origin web requests need no CORS headers. Add only native mobile or explicit development origins when needed, for example:

```text
EMAIL_RS_ALLOWED_ORIGINS=capacitor://localhost,http://localhost:5173
```

Cookie-authenticated POST/PUT/PATCH/DELETE requests must also carry an Origin matching the request host, `EMAIL_RS_PUBLIC_URL`, or this allowlist. Bearer credentials do not enable an origin in CORS; a hostile website therefore cannot read API responses.

## Message-identity migration backups

Before rebuilding a legacy `messages` table, startup creates a timestamped sibling SQLite file named like:

```text
email.db.before-message-identity-20260710T120000Z-a1b2c3d4.sqlite
```

The path and state are recorded in `migration_backups`; affected accounts are recorded in `account_reconciliation`. The backup remains `retained` until every account completes a successful folder reconciliation, after which it becomes `releasable`. email-rs never deletes the backup automatically. Confirm the migrated application, message counts, and a completed synchronization before removing it manually.
