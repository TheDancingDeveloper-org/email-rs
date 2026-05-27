# FluentGUI Phase 7t Progress

Last updated: 2026-05-27

## Scope Completed

- Audited stale tracker rows against the current native shell/layout code.
- Added focused coverage for Linux display/headless decision logic.
- Confirmed the native Linux startup path opens FluentGUI when a display is
  available and runs server-only for `EMAIL_RS_HEADLESS=1` or no-display Linux.
- Closed the main 3-pane layout tracker row based on the current
  `Workspace`/`DockPanel`/`PaneGroup` implementation and persisted layout-width
  coverage.
- Replaced blocking initial state load with async account/message loading and
  added native loading status/skeleton states while accounts load.
- Mounted the FluentGUI title bar in the native workspace and supplied an
  email-specific icon asset for the native shell.
- Added stored attachment metadata/download endpoints to the typed API/client
  path and surfaced native reading-pane Open/Download actions.
- Added Calendar time-grid scroll targeting for the 8am row on week mount/change.
- Reworked timed Calendar events to preserve duration, clip to day bounds, and
  assign overlap columns.

## Remaining Gaps

- Attachment workflows are implemented for attachments whose content is stored in
  the local attachment cache; live provider extraction remains backend-dependent.
- Command palette keyboard and mouse behavior still needs a running-app manual
  walkthrough.
- Windows lifecycle, tray behavior, OAuth callbacks, Gmail/Microsoft OAuth, and
  real IMAP/SMTP verification remain manual/platform verification items.

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-server attachment`
- `cargo test -p email-server messages`
- `cargo test -p email-client`
- `cargo test -p email-desktop-fluent`
- `git diff --check`
