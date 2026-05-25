# FluentGUI Phase 4d Progress

Last updated: 2026-05-25

## Scope Completed

- Added native refresh after SSE `sync_complete` events.
- Refresh now reloads accounts, folders, labels, and the currently selected
  folder, label, or smart-folder message collection through the existing Axum
  API and `email-client`.
- Kept the refresh off the GPUI event path by using the existing background
  task pattern and `AsyncResource` request guards.
- Preserved the selected message during sync refresh when that message still
  exists in the refreshed collection.
- Added stale-refresh protection so a sync refresh cannot overwrite a newer
  navigation or newer refresh request.
- Refactored account-tree and message-collection loading helpers so startup,
  navigation, and sync refresh share the same loading behavior.
- Added focused native tests for current-source detection, selected-message
  preservation, and stale sync-refresh rejection.

## Assumptions

- Sync refresh remains app-owned in `email-desktop-fluent` because it is tied to
  mail account, folder, label, and message semantics. No FluentGUI extension is
  needed for this slice.
- Refreshing the active message collection on sync complete matches the React
  frontend's user-visible behavior closely enough for Phase 4. Background
  refresh of inactive collections can wait until virtualized/search parity work
  makes that useful.
- If the selected message disappears after sync, the native app selects the
  first refreshed message in the active collection, matching current navigation
  behavior.
- Account setup/OAuth, persistent widths/theme/density, sidebar search, and
  sidebar context menus remain later Phase 4 work.
- The existing untracked `.claude/` directory is unrelated and is intentionally
  left unstaged.

## Verification

- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8627` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native dev app smoke on `127.0.0.1:8625` using a temporary copy
  of `fixtures/fluentgui-phase0/one-account.sqlite`; API smoke passed against
  the running app for accounts, folders, messages, selected full message, and
  search. The fixture IMAP sync error is expected because fixture accounts use
  non-routable `example.test` hosts.
