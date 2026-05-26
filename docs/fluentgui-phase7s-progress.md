# FluentGUI Phase 7s Progress

Last updated: 2026-05-26

## Scope Completed

- Added native debounced basic search result submission.
- Scheduled result loads 300 ms after sidebar search text changes, matching the
  React message-list debounce.
- Reused the app's existing periodic native tick to start only due searches.
- Dropped stale pending queries and skipped already-submitted queries to avoid
  duplicate search loads.
- Added focused tests for due-time, stale-query, empty-query, and
  already-submitted behavior.
- Updated the feature tracker to close the debounced search-results row.

## Remaining Gaps

- Native Calendar still needs scroll-to-8am and richer timed-event overlap and
  duration fidelity.
- Attachment behavior remains a product gap; the native app currently indicates
  attachments but does not provide full attachment workflows.
- Startup blank/skeleton behavior while accounts load still needs native parity
  work.
- Command palette keyboard and mouse behavior still needs a running-app manual
  walkthrough.
- Windows lifecycle, tray behavior, OAuth callbacks, Gmail/Microsoft OAuth, and
  real IMAP/SMTP verification remain manual/platform verification items.

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent debounced_search -- --nocapture`
- `cargo test -p email-desktop-fluent`
- `git diff --check`
