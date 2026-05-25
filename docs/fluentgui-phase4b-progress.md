# FluentGUI Phase 4b Progress

Last updated: 2026-05-25

## Scope Completed

- Removed synchronous folder, label, smart-folder, and message-body navigation
  loads from native GPUI event handlers.
- Added `NativeMessageSource` routing for folder, label, and smart-folder
  navigation while keeping the current Axum API as the source of truth.
- Moved live navigation HTTP calls onto GPUI background tasks. Each task creates
  a short-lived Tokio runtime away from the UI callback because `email-client`
  is currently implemented on `reqwest`/Tokio.
- Added `AsyncResource` request guards for message collections and selected
  full-message loads so stale completions from rapid navigation cannot overwrite
  the current route or selected message.
- Added native state tests for route loading, stale collection responses,
  current collection application, and stale full-message responses.
- Refined the native skeleton toward a mail-app shell: the sidebar now uses a
  flat Outlook-style folder tree with account headers, indentation, selected
  accent bars, hover states, and right-aligned unread counts instead of boxed
  rows.
- Tightened the message-list header and reading-pane shell so the app no longer
  presents as a generic placeholder stack while deeper parity work continues.
- Kept the browser/headless frontend path unchanged.

## Assumptions

- Startup still performs the initial account/folder/message load synchronously
  before the GPUI window opens. This is acceptable for Phase 4b because the
  user-visible event-handler stalls are removed, but startup loading should
  become an async startup skeleton in a later Phase 4 slice.
- Navigation continues through the local Axum HTTP API rather than direct
  service calls, matching the migration plan's initial runtime model.
- Background navigation uses short-lived Tokio runtimes per request for now.
  A later optimization can introduce a shared native client runtime once the
  UI parity surface is larger.
- The first message in a newly selected collection is still loaded immediately,
  preserving Phase 4a behavior and the existing backend `GET /messages/{id}`
  read-marking semantics.
- SSE sync progress, account setup/OAuth, persistent widths, sidebar search,
  and sidebar context menus remain later Phase 4 work.
- Message actions, bulk actions, row context menus, and compose remain Phase 5
  work.
- The current layout polish is intentionally app-owned in `email-rs`; if the
  same folder-tree/list style becomes needed outside email, it can be extracted
  later as a generic FluentGUI navigation/list variant.
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
- Display-mode native dev app run on `127.0.0.1:8624` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`; API smoke passed against the
  running app for accounts, folders, messages, selected full message, and
  search. The fixture IMAP sync error is expected because fixture accounts use
  non-routable `example.test` hosts.
