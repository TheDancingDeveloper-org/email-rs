# FluentGUI Phase 3f Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the sixth Phase 3 generic FluentGUI extension: selectable list
  layout.
- Added `fluent-layout::SelectableList` with lazy uniform-row rendering through
  GPUI `uniform_list`, single selection, keyboard roving, activation callbacks,
  context-menu callbacks, and loading/empty/error states.
- Added `SelectableListItem` for generic title/subtitle/detail/meta/badge/icon
  rows without email-specific fields in FluentGUI.
- Added FluentGUI unit coverage for disabled-row exclusion and keyboard target
  movement.
- Updated the FluentGUI widgets example and README component inventory.
- Updated the email-rs FluentGUI skeleton message list to consume
  `SelectableList`, including initial selected message state and in-list
  selection updates.

## Assumptions

- `SelectableList` uses uniform row heights so it can rely on GPUI's
  `uniform_list`. Message rows that need variable body previews or expanded
  thread content can layer those details outside this primitive or introduce a
  separate variable-height virtual list later.
- Phase 3f wires selection inside the native skeleton only. It does not yet
  fetch full message bodies, mark messages read, or navigate folders; those stay
  in Phase 4/5.
- The current selected message is initialized to the first loaded folder
  message to make the reading pane and list selection visibly connected during
  native smoke tests.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.

## Verification

- `cargo fmt --all --check` from the FluentGUI workspace.
- `cargo test -p fluent-layout` from the FluentGUI workspace.
- `cargo check -p widgets` from the FluentGUI workspace.
- `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8608` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8609` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the smoke script stopped it.
