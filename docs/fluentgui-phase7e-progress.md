# FluentGUI Phase 7e Progress

Last updated: 2026-05-26

## Scope Completed

- Added native selected-event linked-email state with stale request rejection
  scoped to the currently selected calendar event.
- Loaded linked emails when a calendar event is selected and cleared stale link
  state when the event selection or visible week changes.
- Rendered linked emails in the native event detail panel with subject, sender,
  date metadata, loading state, empty state, and error state.
- Wired linked-email rows to open the linked message by loading the target
  folder with the linked message preferred.
- Wired native unlink actions through the calendar link API, then reloaded the
  event link list after deletion.
- Added focused native tests for linked-email stale request handling and link
  display label fallbacks.
- Updated the feature tracker for native linked-email detail, navigation, and
  unlink parity.

## Remaining Gaps

- Calendar search suggestions/results still need native navigation to the
  Calendar view with the matching event selected.
- Native Calendar does not yet scroll to around 8am on mount.
- Timed event layout is functional but still needs richer overlap and duration
  fidelity compared with the web view.
- Calendar linked-email behavior still needs a manual UI walkthrough against
  seeded data and real calendar data.
- Larger parity remains open for Windows lifecycle/tray behavior, command
  palette keyboard walkthrough, message-list/thread-row polish, richer reading
  body/attachment behavior, and real-provider OAuth/IMAP/SMTP verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
