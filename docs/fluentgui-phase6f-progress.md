# FluentGUI Phase 6f Progress

Last updated: 2026-05-26

## Scope Completed

- Added native rule creation from each per-account Rules Settings tab.
- Added native rule editing with rule name, match all/any, condition rows,
  action rows, and cancel/save states.
- Reused the native advanced-search condition model for rule fields,
  operators, boolean controls, and date value entry.
- Added native rule actions for mark read, mark unread, flag, unflag, archive,
  delete, and move to folder.
- Added a native folder selector for move-to-folder actions using the selected
  account's synced folders.
- Wired rule save through the typed `email-client` create/update rules APIs and
  reloads the per-account rule list on success.
- Added rule-save validation for blank names, unsupported actions, empty action
  lists, and move actions without a folder.
- Added Settings modal keyboard handling for Escape and Enter-oriented flows:
  Escape cancels the rule editor or closes Settings, Enter saves the rule editor
  or adds a webhook, and Ctrl/Cmd+Enter saves account settings.
- Added focused native tests for rule save request construction and validation.

## Remaining Gaps

- Add-account/setup remains tracked outside Settings: first-run setup,
  Gmail/Microsoft OAuth, Microsoft app-password, manual IMAP/SMTP, and the
  sidebar Add account modal are still pending native work.
- Settings focus management still needs manual UI walkthrough once the broader
  account setup surfaces exist.
- Advanced-search backend behavior remains a product/backend gap: native and web
  clients emit match/operator/boolean/date values, but backend parsing still
  ignores some structured params.
- Larger native parity remains open for Calendar, command-palette navigation and
  message actions, richer message-list and reading-pane polish, Windows
  lifecycle/tray behavior, and real-provider verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
