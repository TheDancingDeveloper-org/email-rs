# FluentGUI Phase 3h Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the eighth Phase 3 generic FluentGUI extension: app/window
  lifecycle title hooks.
- Added `fluent-app` builder support for desktop app id, minimum window size,
  initial window status, and a generic platform close guard.
- Added `fluent-app::WindowTitle` as a global title/status state surface and
  wired `FluentApp` to keep the platform title synchronized after state
  changes.
- Added a `TitleBar` mode and `window_title_bar` helper that can follow the
  shared `WindowTitle` state for custom Fluent title bars.
- Updated the email-rs FluentGUI skeleton to set the GPUI app id, 1280x800
  startup size, 800x600 minimum size, unread-count title, and unread status.
- Surfaced unread status in the native status bar.

## Assumptions

- Native unread count currently comes from loaded folder metadata, matching the
  current browser title approach more closely than counting only the selected
  folder's loaded messages.
- The title/status is initialized at startup. Full dynamic updates after folder
  navigation, read/unread mutations, sync events, and SSE refreshes remain Phase
  4/5 work because the current native skeleton still uses cloned placeholder
  state.
- GPUI in this repository exposes app id, minimum size, title, and close hooks,
  but not a public cross-platform window icon API. Window icon parity remains a
  Phase 8 packaging/shell blocker.
- Windows single-instance and tray close-to-tray behavior still require
  platform work and manual Windows verification. The new close guard is the
  generic FluentGUI hook needed before implementing that behavior.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.
- The existing untracked `.claude/` directory in the email-rs repo is unrelated
  and is intentionally left unstaged.

## Verification

- FluentGUI framework verification before `2be7313`:
  - `cargo fmt --all --check` from the FluentGUI workspace.
  - `cargo test -p fluent-app` from the FluentGUI workspace.
  - `cargo check -p widgets` from the FluentGUI workspace.
  - `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8612` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8613` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the smoke script stopped it.
