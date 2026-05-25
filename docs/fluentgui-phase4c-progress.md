# FluentGUI Phase 4c Progress

Last updated: 2026-05-25

## Scope Completed

- Added a native sync-event listener for `email-desktop-fluent` that connects to
  the existing Axum `/api/v1/events` SSE endpoint through `email-client`.
- Added `NativeSyncStatus` state and a compact sidebar sync strip that shows
  connection state, current folder progress, completion, and stream errors.
- Kept the listener off the GPUI event path by running the SSE stream on a
  background Tokio runtime and draining updates into app state on a lightweight
  GPUI timer.
- Added reconnect behavior after stream closure or error using a simple
  five-second retry loop.
- Used the generic FluentGUI `ProgressBar` primitive inside the email-owned
  sidebar strip.
- Added focused native tests for sync status transitions and SSE parsing across
  split chunks and CRLF event boundaries.

## Assumptions

- The Outlook-style folder/list layout remains app-owned in
  `email-desktop-fluent`. It should move into FluentGUI only if a generic
  `NavigationTree`, compact list-row variant, or split-pane primitive becomes
  useful outside email-specific screens.
- The sync listener is currently owned by the native email app rather than
  FluentGUI. A framework-level SSE or event-source helper can be considered
  later only if another app needs the same behavior.
- The native app continues to use the existing `/api/v1/events` SSE contract so
  backend behavior and the browser/headless UI remain unchanged.
- Reconnect behavior is intentionally basic for this milestone: stream errors
  are surfaced in the strip, then the listener retries after five seconds.
- Sync-complete events update the native status strip only in this slice.
  Automatic refresh of folders and the current message collection remains a
  follow-up Phase 4 parity item.
- The existing untracked `.claude/` directory is unrelated and is intentionally
  left unstaged.

## Verification

- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8626` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native dev app smoke on `127.0.0.1:8624` using a temporary copy
  of `fixtures/fluentgui-phase0/one-account.sqlite`; API smoke passed against
  the running app for accounts, folders, messages, selected full message, and
  search. The fixture IMAP sync error is expected because fixture accounts use
  non-routable `example.test` hosts.
