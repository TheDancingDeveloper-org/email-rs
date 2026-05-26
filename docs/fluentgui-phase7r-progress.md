# FluentGUI Phase 7r Progress

Last updated: 2026-05-26

## Scope Completed

- Matched the web sync strip visibility model in the native sidebar.
- Hid the native sync strip while the SSE listener is idle.
- Kept the strip visible while syncing, when sync errors occur, and briefly
  after completion.
- Added a 3 second completion expiry path driven by the existing sync polling
  loop.
- Added focused tests for sync strip visibility and expiry behavior.
- Updated the feature tracker to close the sync strip timing row.

## Remaining Gaps

- Native Calendar still needs scroll-to-8am and richer timed-event overlap and
  duration fidelity.
- Command palette keyboard and mouse behavior still needs a running-app manual
  walkthrough.
- Attachment behavior remains a product gap; the native app currently indicates
  attachments but does not provide full attachment workflows.
- Startup blank/skeleton behavior while accounts load still needs native parity
  work.
- Windows lifecycle, tray behavior, OAuth callbacks, Gmail/Microsoft OAuth, and
  real IMAP/SMTP verification remain manual/platform verification items.

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent sync_completion -- --nocapture`
- `cargo test -p email-desktop-fluent sync_updates -- --nocapture`
- `cargo test -p email-desktop-fluent`
- `git diff --check`
