# FluentGUI Phase 4f Progress

Last updated: 2026-05-25

## Scope Completed

- Added a native sidebar search field to `email-desktop-fluent`.
- Added `NativeMessageSource::Search` so search results use the same async
  collection-loading path, selected-message loading, and stale-response guards
  as folders, labels, and smart folders.
- Search submissions call the existing `/api/v1/search` API through
  `email-client`.
- Message-list title now shows `Results for "..."` for active native searches.
- Search remembers the previous folder/label/smart route and restores it when
  an active search is cleared.
- Normal folder, label, and smart-folder navigation clears active search state.
- Added a generic FluentGUI `Searchbox::set_value` helper so app state can keep
  search field text in sync after external navigation changes.
- Added focused native tests for search route state, previous-route tracking,
  title text, and route selection clearing search state.

## Assumptions

- This slice implements basic submitted search first. Debounced suggestions,
  calendar suggestion navigation, and advanced search remain later Phase 4
  work.
- Search refresh on SSE sync-complete uses the same active-source refresh path
  added in Phase 4d. This is acceptable even though the React frontend only
  explicitly refetches folder/smart/label hooks on sync complete.
- The `Searchbox::set_value` change is generic FluentGUI surface area, not
  email-specific UI.
- The existing untracked `.claude/` directory is unrelated and is intentionally
  left unstaged.

## Verification

- `cargo fmt --all --check` from the email-rs workspace.
- `cargo fmt --all --check` from the FluentGUI workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test -p email-desktop-fluent` from the email-rs workspace.
- `cargo test -p fluent-primitives searchbox` from the FluentGUI workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8630` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native dev app smoke on `127.0.0.1:8631` using a temporary copy
  of `fixtures/fluentgui-phase0/one-account.sqlite` and a temporary
  `EMAIL_RS_FLUENT_SETTINGS_PATH` preference file; API smoke passed against the
  running app for accounts, folders, messages, selected full message, and
  search. The fixture IMAP sync error is expected because fixture accounts use
  non-routable `example.test` hosts.
