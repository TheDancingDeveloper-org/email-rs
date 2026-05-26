# FluentGUI Phase 6b Progress

Last updated: 2026-05-26

## Scope Completed

- Added a native Settings modal and wired it from the sidebar footer, command
  palette, and native `OpenSettings` action.
- Added a General settings tab with native theme switching and density controls.
- Added per-account tabs that load account settings, edit display name, sync-day
  limit, and signature, and save through the existing account API.
- Added account folder include/exclude checkboxes that patch the folder API and
  refresh the native account tree.
- Added settings save and folder-update status feedback in the native modal.
- Fixed the shared account update payload so JSON `null` can clear
  `syncDaysLimit` and `signature`, while omitted fields still mean unchanged.
- Added focused tests for native settings request building and backend clearing
  of optional account settings.

## Documented Gaps

- Account deletion and its confirmation flow remain pending for a later settings
  slice.
- Rules management is still a placeholder in the native Settings modal. The
  current web UI has per-account rules tabs, rule list toggles, edit/delete, and
  a full rule editor; none of that is implemented natively yet.
- Webhook management is still a placeholder in the native Settings modal. The
  current web UI has list, enable/delete, signed status, and add-webhook controls
  that remain pending.
- The native Settings nav currently uses a single Rules placeholder rather than
  per-account `Rules (...)` tabs. That should be corrected when rules are
  implemented.
- Add-account/setup remains outside this settings slice and is still tracked in
  the startup/account setup section.
- Keyboard polish for modal Escape/Enter handling is still pending beyond the
  existing click-driven flows.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
- `cargo test -p email-server`
