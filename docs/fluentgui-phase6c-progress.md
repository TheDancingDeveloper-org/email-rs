# FluentGUI Phase 6c Progress

Last updated: 2026-05-26

## Scope Completed

- Added the native Settings account danger zone with a two-step account deletion
  confirmation.
- Wired account deletion through the existing typed `email-client` account API.
- Reloaded the native account tree and initial mailbox state after deletion,
  closing Settings on success.
- Preserved native-only state across the reload, including theme, density,
  selected high-level view, selected calendar event, palette state, sync status,
  and account setup modal state.
- Reset compose state when the deleted account was the active compose account.
- Added a focused native test for account-delete reload behavior.

## Remaining Gaps

- Rules management remains pending. Native Settings still needs per-account rule
  tabs, rule list controls, enable toggles, edit/delete/new, and the rule editor.
- Webhook management remains pending. Native Settings still needs webhook list,
  enable/delete, signed status, add-webhook controls, event selection, and error
  handling.
- The Settings nav still uses one Rules placeholder instead of per-account
  `Rules (...)` tabs.
- Add-account/setup remains tracked in the startup/account setup section rather
  than the settings modal.
- Keyboard polish for Settings modal Escape/Enter handling remains pending.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
