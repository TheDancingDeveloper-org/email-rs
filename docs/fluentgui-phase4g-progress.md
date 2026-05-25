# FluentGUI Phase 4g Progress

Last updated: 2026-05-25

## Scope Completed

- Reworked the native message list into an app-specific mail row layout instead
  of using generic rectangular list rows.
- Rows now show sender initials, sender, date, subject, preview, unread accent,
  and compact state badges for flagged, attachment, and snoozed messages.
- Preserved keyboard roving selection for arrows, Home/End, Page Up/Page Down,
  Enter/Space, and added `j`/`k` movement for the native list.
- Added a selected-folder header action for `Mark all read` on regular folders.
- Added sidebar folder right-click context menu actions for `Open` and
  `Mark all read`.
- `Mark all read` calls the existing folder API, then refreshes accounts,
  folders, labels, and the active message collection through the same guarded
  refresh path used by sync-complete updates.
- Fixed the folder mark-read API to zero the folder unread counter in the same
  transaction as the message updates, so native and web refreshes do not restore
  stale unread badges.
- Sidebar folder rows now render as list-tree rows with generic FluentGUI SVG
  icons, indentation, unread counts, and selected accent bars.

## Assumptions

- The Outlook-style folder/list layout is email-specific and belongs in
  `email-desktop-fluent`, not FluentGUI.
- This slice keeps the existing pane sizes and split behavior. Persisted split
  widths remain a later generic FluentGUI extension task.
- Sidebar folder context menus are implemented for real folders first because
  the backend already exposes folder mark-read. Message row context menus and
  bulk actions remain Phase 5 work.
- The existing untracked `.claude/` directory is unrelated and is intentionally
  left unstaged.

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-server api::folders::tests::mark_folder_read`
- `cargo test -p email-desktop-fluent`
- `cargo test --workspace`
- `npm run build` from `frontend/`
- Headless native server smoke on `127.0.0.1:8633` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/search?q=FluentGUI`
  - `POST /api/v1/folders/fx-a1-inbox/mark-read`, followed by folders reload
    verifying Inbox `unreadCount` changed to `0`
- Display-mode native app smoke on `127.0.0.1:8634` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite` and temporary native settings
  path; API smoke passed against the running app for accounts, messages, and
  folder mark-read unread-count refresh. The fixture IMAP sync error is expected
  because fixture accounts use non-routable `example.test` hosts.
