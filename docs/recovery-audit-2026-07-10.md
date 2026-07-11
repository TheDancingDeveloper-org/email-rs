# Recovery audit and implementation status — updated 2026-07-11

This audit supersedes the interrupted-session checkpoint from 2026-07-10. The recovered implementation is present and its local acceptance suite passes. The tree was not reset during recovery; all recovered changes were preserved for the final source-control handoff.

## Completed implementation

### Reproducible workspace and release builds

- The workspace contains `email-types`, `email-client`, `email-server`, and `email-desktop-fluent` with a canonical lockfile.
- FluentGUI dependencies use one pinned Git revision and CI rejects sibling-repository paths.
- CI runs locked metadata, formatting, check, clippy, Rust tests, mobile tests/build, frontend build, and the production Docker build.
- Linux releases package the server and Fluent desktop explicitly.
- The Windows MSI now builds and launches `email-desktop-fluent`; it no longer packages the legacy server/WebView desktop executable.

### Application authentication

- Migration 14 creates Argon2id application users and hashed cookie/bearer sessions.
- Server login, first-run bootstrap, refresh, identity, logout, revocation, OAuth ownership, Origin validation, CORS restrictions, HTTPS deployment checks, and secret-safe webhook serialization are implemented.
- Browser, Fluent desktop, and mobile all have login/first-run gates.
- The shared Rust `EmailClient` attaches bearer credentials to API, SSE, and attachment requests. Fluent stores its session in a user-only file and supports logout/recovery from an expired token.
- Mobile validates HTTPS except for loopback development, tests the session before opening mail, stores native sessions in iOS Keychain or Android Keystore-backed encrypted storage, and supports logout/server switching. Web preview tokens remain memory-only.
- A canonical privileged-route inventory drives the OpenAPI security document and an anonymous-access regression test for every protected method/path.

### Message identity and synchronization

- Mailbox identity is `(folder_id, uid_validity, uid)`; RFC Message-ID is nullable, non-unique, and indexed.
- Migration backup, retention, reconciliation state, FTS rebuild/backfill, and duplicate Message-ID coverage are implemented.
- Sync upserts mailbox occurrences without changing local IDs, fires new-message effects only for inserts, handles UIDVALIDITY resets transactionally, and removes remotely deleted occurrences.
- Incremental sync compares the complete UID set and explicitly fetches newly appearing UIDs independent of IMAP `SINCE`, repairing old messages moved into a folder.
- Contract tests cover missing Message-ID, repeat sync, remote deletion, remote move discovery, and UIDVALIDITY replacement.

### Providers and remote mutations

- `AccountConfig` is the single account connection value. Mail sync, reads, attachments, mutations, compose, and calendar use the centralized OAuth refresh/persistence path.
- Gmail OAuth, Microsoft OAuth, Microsoft app-password, and generic IMAP accounts are supported.
- Read/unread, flag, archive, trash, and permanent-delete operations complete remotely before local state changes.
- Archive and Trash use provider-discovered special-use folders. Deleting from Trash performs permanent deletion; deleting elsewhere moves to Trash when available.
- Bulk mutations return typed per-message results. Web and Fluent clients surface partial failures while refreshing successful changes.
- Mock-provider contract tests prove remote failures do not commit local read, flag, delete, or archive changes.

### Attachments

- BODYSTRUCTURE synchronization persists attachment filename, MIME type, content ID, size, and authoritative nested part path without downloading content.
- Parsed full bodies use the same nested MIME part numbering; missing and duplicate filenames remain distinct by part identity.
- On-demand IMAP requests use `BODY.PEEK[part]` for the selected MIME part rather than downloading the entire message.
- MIME part identifiers are validated before command construction.
- Cache and download limits plus provider timeout are configurable. Oversize downloads return HTTP 413.
- Cached bytes survive metadata refreshes; stale attachment parts are removed transactionally.
- Fluent sanitizes filenames, creates collision-free paths, writes user-only files/directories, and requires a second explicit Open action for content outside the safe allowlist.
- Tests cover nested MIME, base64 decoding, malformed part identifiers, missing/duplicate filenames, size rejection, response headers, cached content, and Unix permissions.

### Mobile and OpenAPI contract

- `/api/v1/openapi.json` is generated from the canonical Axum route inventory; `export_openapi` provides a build-tool export.
- Mobile requests are checked against the generated TypeScript route contract and dynamic path segments are encoded.
- Router/client tests cover HTTPS policy, bearer headers, archive/delete verbs, structured errors, and protected server routes.
- Compose and destructive actions keep their screens open when requests fail.
- Mobile onboarding and switching behavior are documented in `docs/mobile-onboarding.md`.

## External release validation

The implementation and automated acceptance work are complete. The following are release-environment validations rather than missing source implementation:

- Run onboarding on an actual iOS and Android device to confirm OS prompts, Keychain/Keystore persistence, and server reachability on the intended network.
- Run the provider contract against disposable real Gmail/Microsoft/IMAP test accounts before a production release; automated mocks cover deterministic mutation and reconciliation behavior without requiring credentials.
- Run the Windows MSI job on its Windows Woodpecker agent and verify install, upgrade, launch, login, and uninstall.

## Verification log

- `cargo metadata --locked --format-version 1` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- `cargo test --workspace --locked` — passed after implementation (255 Rust unit tests plus doc tests).
- Mobile `npm test` — passed (3 contract tests).
- Frontend `npm run build` — passed.
- Mobile `npm run build` — passed.
- Production `docker build --tag email-rs-ci .` — passed; local image ID `sha256:fee72c394cf12c2e175bb137861c4e258393c05f548b00290503a66f06723ec9`.
