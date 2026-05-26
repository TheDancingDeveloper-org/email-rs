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

- Add-account/setup moved forward in Phase 6g: first-run setup,
  Gmail/Microsoft OAuth launch, Microsoft app-password, manual IMAP/SMTP, and
  the sidebar Add account entry are now native, with real-provider verification
  still manual.
- Settings focus management still needs manual UI walkthrough once the broader
  account setup surfaces exist.
- Advanced-search backend structured message params were implemented in Phase
  7b; remaining search work is native/manual UI verification and
  calendar-event palette navigation.
- Larger native parity remains open for Calendar, command-palette navigation and
  message actions, richer message-list and reading-pane polish, Windows
  lifecycle/tray behavior, and real-provider verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
