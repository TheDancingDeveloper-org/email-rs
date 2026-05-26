# FluentGUI Phase 7c Progress

Last updated: 2026-05-26

## Scope Completed

- Added native reading-pane keyboard shortcuts for archive (`e`), delete (`#`,
  plus Delete/Backspace), read/unread toggle (`u`), and Escape clear-selection.
- Guarded reading-pane shortcuts while the quick-reply recipient input, quick
  reply body, or snooze picker has focus.
- Added the native reading header context menu with reply, forward, archive,
  delete, read/unread, and copy sender email actions.
- Wired copy sender email through GPUI clipboard support with visible
  success/error feedback in the existing reading-pane status area.
- Added focused native tests for reading shortcut mapping and selected sender
  email resolution.
- Updated the feature tracker for reading-pane shortcuts, header context menu,
  and already-native message-list shortcut/context-menu coverage.

## Remaining Gaps

- Calendar native implementation remains open.
- Windows lifecycle and tray behavior still need platform work and manual
  Windows verification.
- Reading-pane/body polish remains open for richer HTML fidelity and attachment
  download/open interactions.
- Message-list row parity still needs a dedicated pass for thread grouping and
  remaining row detail polish.
- Command-palette keyboard walkthrough and calendar-event search/navigation
  remain open.
- Real-provider OAuth and manual IMAP/SMTP verification remain manual.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
