# FluentGUI Phase 6d Progress

Last updated: 2026-05-26

## Scope Completed

- Replaced the native Settings Webhooks placeholder with a functional Webhooks
  tab.
- Added webhook loading from the typed `email-client` API when the Webhooks tab
  is selected.
- Added native webhook enable/disable toggles, delete actions, busy states, and
  success/error feedback.
- Added native Add webhook controls for URL, optional signing secret, and event
  set.
- Added focused tests for native webhook create request validation and
  normalization.

## Remaining Gaps

- Rules management remains pending. Native Settings still needs per-account rule
  tabs, rule list controls, enable toggles, edit/delete/new, and the rule editor.
- The Settings nav still uses one Rules placeholder instead of per-account
  `Rules (...)` tabs.
- Add-account/setup remains tracked in the startup/account setup section rather
  than the settings modal.
- Keyboard polish for Settings modal Escape/Enter handling remains pending.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
