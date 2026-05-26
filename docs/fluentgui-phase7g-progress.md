# FluentGUI Phase 7g Progress

Last updated: 2026-05-26

## Scope Completed

- Added native sidebar search suggestion state, request tracking, and stale
  response rejection scoped to the current query.
- Loaded message and calendar event suggestions through `email-client` after
  the existing 150 ms suggestion delay.
- Rendered native search suggestion rows below the sidebar search box with
  subject/title, sender or event type, and compact date metadata.
- Wired mouse selection so message suggestions open their target folder and
  preferred message, while calendar suggestions switch to Calendar and select
  the matching event week.
- Wired keyboard movement for Up/Down/Home/End/Escape and Enter selection of
  the active suggestion; Enter without an active suggestion still submits the
  full search.
- Added focused native tests for suggestion formatting, stale request handling,
  highlighted selection ids, keyboard state movement, and cleanup.
- Updated the feature tracker for native live sidebar suggestions and calendar
  suggestion/result parity.

## Remaining Gaps

- Search suggestion behavior still needs a manual UI walkthrough against seeded
  data to confirm focus propagation and visual placement in a running window.
- Command palette keyboard walkthrough remains pending, including a direct
  manual pass through Up/Down/Enter/Escape behavior.
- Native Calendar does not yet scroll to around 8am on mount.
- Timed event layout is functional but still needs richer overlap and duration
  fidelity compared with the web view.
- Larger parity remains open for Windows lifecycle/tray behavior,
  message-list/thread-row polish, richer reading body/attachment behavior, and
  real-provider OAuth/IMAP/SMTP verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
