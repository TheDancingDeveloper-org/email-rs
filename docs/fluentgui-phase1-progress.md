# FluentGUI Phase 1 Progress

Last updated: 2026-05-25

## Scope Completed

- Added `crates/email-types` for shared Rust DTOs that mirror the current
  `/api/v1` JSON casing.
- Added `crates/email-client` as the native app's typed HTTP client for the
  current API surface.
- Refactored `crates/email-server` into a reusable library plus the existing
  binary entrypoint.
- Exposed `email_server::run_server` and `email_server::build_app` so a native
  desktop crate can start Axum in-process without duplicating server startup.
- Reused shared DTOs in low-risk Axum handlers where the server structs matched
  the API contract exactly.
- Added shared `Message.snoozed_until` support and selected `snoozed_until` in
  message list/search/smart-folder responses so the existing frontend
  `snoozedUntil` field is populated.

## Assumptions

- Phase 1 keeps HTTP as the native UI integration seam. Direct service calls can
  be reconsidered only after native feature parity is working against the
  existing API.
- Existing platform desktop behavior stays in `email-server/src/main.rs` until
  the FluentGUI shell exists. The library seam is intentionally limited to
  backend startup and app construction.
- DTO replacement is limited to structs that are serde/sqlx-compatible with the
  current API. Rules, calendar provider objects, and full-message body assembly
  keep local conversion code for now to avoid accidental product behavior
  changes.
- `email-client::advanced_search` preserves the current frontend URL shape, but
  the backend still only honors `q` and optional `account_id`. The Phase 0
  backend gap remains open for a later milestone.
- `email-client::open_sync_events` returns the raw SSE response and
  `parse_sync_event` parses event payloads. Native UI code owns cancellation,
  reconnection, and request lifecycle policy.

## Verification

- `cargo fmt --all --check`
- `cargo test --workspace`
- `npm run build` from `frontend/`
- `cargo run -p email-server --bin seed_fixtures -- fixtures/fluentgui-phase0`
- Headless local server smoke on `127.0.0.1:8593` using
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/rules?account_id=fx-a1`
  - `/api/v1/search?q=FluentGUI`
  - `/api/v1/search/suggest?q=FluentGUI`
  - `/api/v1/smart-folders/snoozed/messages`

The headless smoke still attempts fixture IMAP sync and logs a DNS failure for
`imap.example.test`, which is expected for offline-safe fixture credentials.
