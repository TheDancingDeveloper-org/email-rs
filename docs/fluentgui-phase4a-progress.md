# FluentGUI Phase 4a Progress

Last updated: 2026-05-25

## Scope Completed

- Started Phase 4 native accounts/sidebar/navigation work.
- Replaced cloned placeholder state in the FluentGUI desktop skeleton with one
  shared `NativeAppState` entity observed by the shell, sidebar, message list,
  reading pane, and status bar.
- Wired sidebar rows for smart folders, account folders, labels, and the
  Mail/Calendar view switch to mutate shared native state.
- Wired message-list selection and activation to load the selected full message
  through `email-client`.
- Kept Axum/backend API behavior stable; navigation uses existing
  `list_folder_messages`, `list_smart_messages`, `list_label_messages`, and
  `get_message` client APIs.
- Updated native unread title/status when navigation marks an opened message
  read.

## Assumptions

- Navigation requests are performed synchronously inside the native event
  handler for this first state-sharing slice. Phase 4 follow-up work should move
  these calls onto GPUI async tasks using the existing `AsyncResource`
  generation model.
- The first message in a newly selected folder/smart folder/label is loaded
  immediately, which matches the current skeleton behavior but also marks that
  message read through the existing backend `GET /messages/{id}` semantics.
- Smart folders use `selected_folder = "smart:{kind}"` in native state until a
  richer native route enum replaces the placeholder fields.
- Account setup, OAuth browser flow, SSE sync progress, persistent widths, and
  sidebar context menus remain later Phase 4 work.
- Message actions, bulk actions, row context menus, and compose remain Phase 5
  work.
- The existing untracked `.claude/` directory is unrelated and is intentionally
  left unstaged.

## Verification

- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8616` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/smart-folders/unread/messages`
  - `/api/v1/labels/fx-a1-label-important/messages`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8617` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, served account/folder/smart-folder endpoints, then stayed alive until
  the smoke script stopped it.
