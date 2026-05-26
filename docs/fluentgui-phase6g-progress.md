# FluentGUI Phase 6g Progress

Last updated: 2026-05-26

## Scope Completed

- Added a native account setup surface that opens automatically when no accounts
  exist and also works as the Add account modal for configured workspaces.
- Added sidebar and command-palette Add Account entry points.
- Wired Gmail and Microsoft 365 OAuth starts through the typed `email-client`
  authorize APIs and opens the returned URL in the system browser.
- Added a native refresh action for the OAuth return path so the app can reload
  accounts after browser authorization completes.
- Added native Microsoft app-password setup with Outlook/SMTP defaults.
- Added native manual IMAP/SMTP setup with host/port/password fields and the
  same-password toggle for SMTP.
- Wired native account creation through the typed account API and reloads the
  account tree on success.
- Preserved the current workspace selection when adding an account to an
  existing workspace, while first-run setup adopts the newly loaded mailbox.
- Added focused tests for manual and Microsoft account request construction,
  validation, first-run reload behavior, and existing-workspace preservation.

## Remaining Gaps

- OAuth is launched from native and can be refreshed manually after callback,
  but native does not yet poll or subscribe for callback completion.
- Gmail/Microsoft OAuth and manual IMAP/SMTP still need real-provider credential
  verification outside automated tests.
- The account-loading blank state still needs a dedicated native loading
  treatment; the new setup surface handles the no-account state after load.
- Settings and account setup focus order should get one manual keyboard
  walkthrough after the remaining modal work settles.
- Larger parity remains open for Windows lifecycle/tray behavior, Calendar,
  command-palette navigation/actions, message-list and reading-pane polish,
  advanced-search backend structured params, and snoozed row serialization.

## Verification

- `cargo check -p email-desktop-fluent`
