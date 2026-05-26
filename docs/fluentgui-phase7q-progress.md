# FluentGUI Phase 7q Progress

Last updated: 2026-05-26

## Scope Completed

- Added the same focused modal shell pattern used by account setup and settings
  to the native advanced-search modal.
- Wired native advanced-search keyboard handling:
  - Escape closes the modal.
  - Enter/Return submits only when the draft has an active condition.
- Added focused coverage for the advanced-search shortcut mapping.
- Updated the feature tracker to close the advanced-search Enter/Escape polish
  row.

## Remaining Gaps

- Native Calendar still needs scroll-to-8am and richer timed-event overlap and
  duration fidelity.
- Command palette keyboard and mouse behavior still needs a running-app manual
  walkthrough.
- Sync strip hide-after-completion behavior still needs a manual parity check.
- Attachment behavior remains a product gap; the native app currently indicates
  attachments but does not provide full attachment workflows.
- Startup blank/skeleton behavior while accounts load still needs native parity
  work.
- Windows lifecycle, tray behavior, OAuth callbacks, Gmail/Microsoft OAuth, and
  real IMAP/SMTP verification remain manual/platform verification items.

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent advanced_search_key -- --nocapture`
- `cargo test -p email-desktop-fluent`
- `git diff --check`
