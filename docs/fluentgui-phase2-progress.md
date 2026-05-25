# FluentGUI Phase 2 Progress

Last updated: 2026-05-25

## Scope Completed

- Added `crates/email-desktop-fluent` as the native desktop skeleton crate.
- Wired the new crate to local FluentGUI path dependencies under
  `/home/sprooty/Working/Active/apps/FluentGUI`.
- Started `email-server` in-process on a background Tokio runtime.
- Added readiness waiting before opening the FluentGUI app.
- Added a server-thread error channel so bind/startup failures surface instead
  of silently connecting to an unrelated process.
- Preserved server-only behavior for `EMAIL_RS_HEADLESS=1` and Linux no-display
  environments.
- Loaded initial native state through `email-client`: accounts, folders, labels,
  selected inbox messages, and API base URL.
- Added a Phase 2 app state scaffold for theme, density, selected view, selected
  folder, selected label, selected message, selected event, compose state, modal
  states, and palette state.
- Added a FluentGUI root workspace with sidebar, mail/reading-pane placeholder,
  status bar, modal host, and command palette host.
- Added light/dark theme actions and density actions on the root app.

## Assumptions

- `email-desktop-fluent` remains a separate crate until cutover so the existing
  `email-server` desktop/headless behavior stays stable.
- Phase 2 native state loads synchronously before opening the window. Later
  phases should replace this with cancellable async resources and stale-response
  protection.
- Native HTTP calls use `http://localhost:{port}` after readiness. This matches
  the existing desktop browser host more closely than `127.0.0.1` for OAuth
  redirect behavior.
- `run_server` still has no graceful shutdown handle. The Phase 2 skeleton lets
  process exit stop the background server; Windows tray/quit behavior remains a
  later lifecycle milestone.
- Placeholder views intentionally render loaded data without implementing mail,
  compose, settings, or calendar interactions. Those remain Phase 4+ work.

## Verification

- `cargo fmt --all --check`
- `cargo check -p email-desktop-fluent`
- `cargo test --workspace`
- `npm run build` from `frontend/`
- `cargo run -p email-server --bin seed_fixtures -- fixtures/fluentgui-phase0`
- Headless native server smoke on `127.0.0.1:8594` using
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8595` using the same fixture
  DB. The process initialized the in-process server and GPUI/Blade window, then
  stayed alive until the 10 second smoke-test timeout stopped it.

The fixture IMAP sync still logs the expected DNS failure for
`imap.example.test`; fixture credentials remain offline-safe.
