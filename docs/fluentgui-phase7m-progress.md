# FluentGUI Phase 7m Progress

Last updated: 2026-05-26

## Scope Completed

- Audited stale feature-tracker rows against current native code and focused
  tests after the Phase 7j-7l work.
- Updated native window title/unread count status based on existing
  `native_window_title` coverage.
- Updated theme and density persistence/status rows based on native preferences
  parsing, round-trip tests, sidebar controls, settings controls, and palette
  actions.
- Updated sidebar folder/label/status rows where native rendering and actions
  are already wired.
- Updated message-list header, select-all, bulk bar, mark-all-read, and
  empty/loading status rows where native UI or helper tests already exist.
- Updated reading-pane empty/loading/read-state/header/label rows where native
  rendering, typed APIs, and tests already cover the behavior.

## Remaining Gaps

- Linux startup/headless behavior, Windows lifecycle/tray behavior, window icon,
  and platform cutover remain open.
- Sidebar label color-dot fidelity and persisted sidebar/message-list widths
  still need native polish.
- Native Calendar still needs scroll-to-8am and richer timed-event overlap and
  duration fidelity.
- Command palette keyboard and mouse behavior still needs a running-app manual
  walkthrough.
- Sync strip hide-after-completion behavior still needs a manual parity check.
- OAuth callbacks, Gmail/Microsoft OAuth, and real IMAP/SMTP verification remain
  manual/provider verification items.

## Verification

- `git diff --check`
