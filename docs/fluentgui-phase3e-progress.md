# FluentGUI Phase 3e Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the fifth Phase 3 generic FluentGUI extension: local date/time
  picking.
- Added `fluent-primitives::DateTimePicker` plus `DateTimeValue` and
  `DateTimeSegment` for local wall-clock values.
- Added generic parsing/formatting for `YYYY-MM-DDTHH:MM`, leap-year/day
  validation, segment keyboard movement, and segment increment/decrement
  behavior without email-specific code in FluentGUI.
- Updated the FluentGUI widgets example and README component inventory.
- Updated the email-rs FluentGUI skeleton reading pane to consume the picker
  for the custom snooze date/time placeholder.

## Assumptions

- The picker intentionally models a local wall-clock value, not an absolute
  instant. App code remains responsible for converting selected local values to
  backend ISO timestamps when the real snooze action is wired.
- The native reading pane still does not call the snooze API. Phase 3e only
  proves that the generic control can represent the current web UI's custom
  `datetime-local` interaction in the native shell.
- The skeleton picker opens with a local "tomorrow at 9:00" draft but remains
  blank until the user adjusts a segment, matching the current web custom
  snooze field's blank initial state.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.

## Verification

- `cargo fmt --all --check` from the FluentGUI workspace.
- `cargo test -p fluent-primitives` from the FluentGUI workspace.
- `cargo check -p widgets` from the FluentGUI workspace.
- `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8606` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8607` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the smoke script stopped it.
