# email-rs

A self-hosted email and calendar client. Rust/Axum backend, React/TypeScript frontend, SQLite database. Runs as a single Docker container and is accessible from any browser.

Built as a clean-UI replacement for Thunderbird/Outlook.

---

## Features

**Email**
- Gmail (OAuth2) and Outlook/Exchange (OAuth2 + app-password) accounts
- Generic IMAP/SMTP accounts
- Multi-account support — sync all accounts in one view
- Background IMAP sync with pre-fetching
- Full-text search (SQLite FTS5, BM25 ranked)
- Advanced search with condition builder (AND/OR groups, all message fields)
- Compose with To/Cc/Bcc, reply, and forward
- Recipient autocomplete from contacts/history
- Labels, snooze, smart folders
- Rules engine — condition-based automatic actions on incoming mail
- Mark read/unread, bulk mark-all-read

**Calendar**
- Google Calendar synchronization with week and event-detail views
- Week view with event detail
- Calendar events appear in search results

**UI**
- 3-pane layout (sidebar / message list / reading pane)
- Light and dark themes, compact/comfortable density
- Command palette (Ctrl+K) — compose, navigate, search, toggle calendar
- Unread count in browser tab title

**Linux**
- Native FluentGUI desktop app backed by the same in-process Axum server
- Falls back to browser-only server mode when running headless (no `$DISPLAY`/`$WAYLAND_DISPLAY`)
- Set `EMAIL_RS_HEADLESS=1` to force server-only mode on a machine with a display

**Windows**
- Native Windows app via WebView2 (single-instance, system tray, MSI installer)

**Mobile**
- Companion Capacitor app (`mobile/`) with inbox, search, compose, and folder navigation; secure server onboarding remains in progress

Current implementation status and known gaps are tracked in
[`docs/recovery-audit-2026-07-10.md`](docs/recovery-audit-2026-07-10.md).

---

## Quick start (Docker)

```bash
cp .env.example .env
# Set EMAIL_RS_BOOTSTRAP_PASSWORD to a new 12+ character secret.
# Add OAuth credentials if you use Gmail or Microsoft accounts.
docker compose up -d
```

Open **http://localhost:8585**, sign in with the bootstrap credentials, then go
to Settings → Add Account. Remove `EMAIL_RS_BOOTSTRAP_PASSWORD` from `.env`
after the first successful startup; the stored credential is an Argon2id hash.

The backend hot-reloads via `cargo-watch`; the frontend via Vite HMR. The first start takes ~60s while Rust compiles inside the container.

---

## Configuration

All configuration is via environment variables (`.env` or `docker-compose.yml`).

| Variable | Description | Default |
|---|---|---|
| `GOOGLE_CLIENT_ID` | Google OAuth2 client ID | — |
| `GOOGLE_CLIENT_SECRET` | Google OAuth2 client secret | — |
| `MICROSOFT_CLIENT_ID` | Microsoft OAuth2 client ID | — |
| `MICROSOFT_CLIENT_SECRET` | Microsoft OAuth2 client secret | — |
| `DATABASE_URL` | SQLite connection string | `~/.local/share/email-rs/email.db` (Linux) / `%LOCALAPPDATA%\email-rs\email.db` (Windows) |
| `HOST` | Bind address | `127.0.0.1` |
| `PORT` | Bind port | `8585` |
| `FRONTEND_DIST` | Path to built frontend assets | auto-detected (see below) |
| `EMAIL_RS_HEADLESS` | Set to `1` to disable the desktop window on Linux | unset |
| `EMAIL_RS_BOOTSTRAP_USERNAME` | Initial single-user login name | `admin` |
| `EMAIL_RS_BOOTSTRAP_PASSWORD` | Initial password secret (12+ characters; used only when no app user exists) | — |
| `EMAIL_RS_CREDENTIAL_KEY` | Stable base64-encoded 32-byte AES key for provider credentials | — |
| `EMAIL_RS_PUBLIC_URL` | Required HTTPS URL for a non-loopback deployment | — |
| `EMAIL_RS_ALLOWED_ORIGINS` | Comma-separated Capacitor/development origins allowed by CORS | — |
| `EMAIL_RS_SESSION_TTL_SECS` | Browser/desktop cookie lifetime | `43200` (12 hours) |
| `EMAIL_RS_BEARER_TTL_SECS` | Mobile bearer-token lifetime | `2592000` (30 days) |
| `EMAIL_RS_ALLOW_INSECURE_HTTP` | Explicit non-loopback HTTP development override | unset |
| `EMAIL_RS_DISABLE_AUTH` | Disable application auth for isolated development/tests only | unset |

**Frontend asset resolution order (Linux/macOS):** executable directory → `/usr/local/share/email-rs/static` → `/usr/share/email-rs/static` → `crates/email-server/static` relative to the working directory.

### Setting up OAuth credentials

**Google (Gmail + Calendar)**
1. Go to [Google Cloud Console](https://console.cloud.google.com) → APIs & Services → Credentials
2. Create an OAuth 2.0 Client ID (Web application)
3. Add `http://localhost:8585/api/v1/auth/gmail/callback` as an authorised redirect URI
4. Enable the Gmail API and Google Calendar API for your project

**Microsoft (Outlook / Exchange)**
1. Go to [Azure portal](https://portal.azure.com) → App registrations → New registration
2. Add `http://localhost:8585/api/v1/auth/microsoft/callback` as a redirect URI
3. Under API permissions add: `Mail.ReadWrite`, `Mail.Send`, `Calendars.ReadWrite`, `offline_access`

Generic IMAP/SMTP accounts and app-password accounts do not require OAuth setup.

---

## Running natively on Linux

**Install native FluentGUI system dependencies (Ubuntu / Debian):**

```bash
sudo apt install pkg-config libssl-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev
```

**Build and run:**

```bash
# Build and run the native FluentGUI desktop (it starts the server in-process)
cargo build --release --locked -p email-desktop-fluent
./target/release/email-desktop-fluent
```

The native FluentGUI dependencies are pinned to an immutable upstream Git
revision, so a sibling FluentGUI checkout is not required. Developers working
on both repositories can use an untracked Cargo configuration patch locally;
committed manifests must continue to use the pinned revision so clean clones,
CI, and Docker builds remain reproducible.

For browser/headless operation, build the frontend and run `cargo run -p email-server`.

The database is created automatically at `~/.local/share/email-rs/email.db` on first run.

**Browser development mode (hot reload):**

```bash
# Terminal 1 — backend
EMAIL_RS_ALLOW_INSECURE_HTTP=true cargo run -p email-server

# Terminal 2 — frontend with HMR
cd frontend && npm run dev
```

The Vite dev server runs on port 5173 and proxies `/api/*` to the backend.
Set `EMAIL_RS_ALLOWED_ORIGINS=http://localhost:5173` when accessing the backend
directly rather than through Vite's same-origin proxy.

---

## Building a production image

```bash
docker build -t email-rs .
docker run -d \
  -e EMAIL_RS_BOOTSTRAP_USERNAME=admin \
  -e EMAIL_RS_BOOTSTRAP_PASSWORD=... \
  -e EMAIL_RS_PUBLIC_URL=https://mail.example.com \
  -e GOOGLE_CLIENT_ID=... \
  -e GOOGLE_CLIENT_SECRET=... \
  -e MICROSOFT_CLIENT_ID=... \
  -e MICROSOFT_CLIENT_SECRET=... \
  -e DATABASE_URL=sqlite:///data/email.db \
  -v email-data:/data \
  -p 127.0.0.1:3000:3000 \
  email-rs
```

Put Caddy or another TLS reverse proxy in front of the loopback-published
container port. Non-loopback production deployments must set
`EMAIL_RS_PUBLIC_URL` to an `https://` URL. See
[`docs/deployment-security.md`](docs/deployment-security.md) for Caddy,
authentication bootstrap, CORS, and migration-backup guidance.

Additional operational and developer documentation is indexed in
[`docs/README.md`](docs/README.md).

---

## Linux CI release build

The GitHub and Woodpecker release workflows build the `email-server` browser/headless
binary and `email-desktop-fluent` native binary explicitly, then package them
with the browser assets. The Docker image builds only `email-server`.

---

## Windows installer

Pre-built MSI installers are available on the [Releases](../../releases) page.

To build from source you need: Rust (MSVC toolchain), Node.js LTS, .NET SDK 8, and WiX v5.

```powershell
cd frontend && npm ci
npx vite build --outDir ..\staging\static --emptyOutDir
cd ..
cargo build --release --target x86_64-pc-windows-msvc -p email-server
# Then build the MSI with wix (see .woodpecker/windows/installer.wxs)
```

---

## Stack

| Layer | Technology |
|---|---|
| Backend | Rust, Axum, SQLx, SQLite |
| Frontend | React, TypeScript, Vite, Zustand |
| Mobile | React, Capacitor |
| Application auth | Argon2id single-user login; hashed cookie/bearer sessions |
| Provider auth | OAuth2 (Google, Microsoft), IMAP basic/app-password |
| Mail transport | async-imap, lettre |
| Search | SQLite FTS5 with BM25 ranking |
| Dev proxy | Caddy |

---

## Service integrations and Python SDK

Machine consumers authenticate with revocable integration tokens created by an
email-rs application user. Tokens are stored only as SHA-256 hashes and can be
limited to account, metadata, body, mutation, send, and sync scopes.

The complete OpenAPI 3.1 contract is available at `/api/v1/openapi.json` and via
the `export_openapi` binary. `clients/python` is the generated, version-matched
`email-rs-client` package; tagged releases publish it to the Forgejo Python
package registry. Consumers should pin the exact SDK and email-rs image version.

OAuth token JSON and IMAP/SMTP passwords are stored as randomized AES-256-GCM
envelopes. Generate `EMAIL_RS_CREDENTIAL_KEY` once with
`openssl rand -base64 32`, keep it in secrets management, and back it up
separately from SQLite. Startup migrates legacy plaintext credential rows in
place and fails closed if credential rows exist but the key is missing or wrong.

Integration events are persisted before delivery. Webhook payloads contain a
top-level `id`, `event`, `timestamp`, `accountId`, and `data`; failed deliveries
use bounded exponential retry and their status is visible at
`/api/v1/webhook-deliveries`. A consumer with `mail.metadata.read` can recover
missed events from `/api/v1/integration-events?after=<event-id>`. Stable event
names include `message.created`, `message.updated`, `message.moved`,
`message.deleted`, and `sync.completed`. The legacy webhook subscription name
`new_message` continues to match `message.created` during migration.

---

## License

MIT
