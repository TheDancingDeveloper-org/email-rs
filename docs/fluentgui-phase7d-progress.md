# FluentGUI Phase 7d Progress

Last updated: 2026-05-26

## Scope Completed

- Added the native Calendar route so the sidebar Calendar item replaces the
  mail split with a calendar surface while keeping the sidebar mounted.
- Added native week loading through `email-client` calendar APIs, with stale
  request rejection and selection cleanup when the visible event set changes.
- Added a Calendar toolbar with previous week, Today, next week, and native
  week range text.
- Rendered Monday-to-Sunday headers, all-day event chips, and a 24-hour timed
  grid grouped by local day and start hour.
- Added native event selection with a detail panel for title, time, location,
  meet link, description, and attendee status.
- Added focused native tests for Monday week helpers, week range formatting,
  stale event-load rejection, and selected-event retention.
- Updated the feature tracker for native Calendar route, week view, toolbar,
  all-day row, timed grid, event selection, and partial detail-panel parity.

## Remaining Gaps

- Linked email list/navigation/unlink is not implemented in the native event
  detail panel yet.
- Calendar search suggestions/results still need native navigation to the
  Calendar view with the matching event selected.
- Native Calendar does not yet scroll to around 8am on mount.
- Timed event layout is functional but still needs richer overlap and duration
  fidelity compared with the web view.
- Calendar UI still needs a manual walkthrough against seeded and real calendar
  data.
- Larger parity remains open for Windows lifecycle/tray behavior, command
  palette keyboard walkthrough, message-list/thread-row polish, richer reading
  body/attachment behavior, and real-provider OAuth/IMAP/SMTP verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
