# FluentGUI Phase 7f Progress

Last updated: 2026-05-26

## Scope Completed

- Preserved calendar events returned by the basic search endpoint in native
  search state instead of dropping them after message extraction.
- Rendered calendar event results above native mail rows for submitted sidebar
  searches, with title, time, and location metadata.
- Wired calendar search result rows to switch to Calendar, set the result week,
  select the matching event, reload the week, and load linked emails for the
  selected event.
- Cleared stale calendar search results when changing folders, labels, smart
  folders, or starting a new collection load.
- Added focused native tests for search-result event week selection, invalid
  event date handling, calendar result label formatting, and preservation of
  event results in loaded message collections.
- Updated the feature tracker for native submitted calendar search results and
  event-result navigation.

## Remaining Gaps

- Live sidebar search suggestions are still pending natively, including the
  150 ms suggestion fetch and Up/Down/Enter/Escape suggestion behavior.
- Command palette keyboard walkthrough remains pending, and palette-backed
  calendar event search remains separate from submitted sidebar search.
- Native Calendar does not yet scroll to around 8am on mount.
- Timed event layout is functional but still needs richer overlap and duration
  fidelity compared with the web view.
- Calendar search result behavior still needs a manual UI walkthrough against
  seeded data and real calendar data.
- Larger parity remains open for Windows lifecycle/tray behavior,
  message-list/thread-row polish, richer reading body/attachment behavior, and
  real-provider OAuth/IMAP/SMTP verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
