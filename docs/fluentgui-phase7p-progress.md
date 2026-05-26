# FluentGUI Phase 7p Progress

Last updated: 2026-05-26

## Scope Completed

- Added native layout preferences for the sidebar and message-list widths.
- Persisted the widths in the native settings file using the parity keys
  `email_sidebar_w` and `email_msglist_w`.
- Initialized the FluentGUI dock and message-list split from saved preferences.
- Observed sidebar dock and message-list split resize notifications so user
  layout changes are saved back to disk.
- Kept the message-list width as a pixel setting while translating it to the
  FluentGUI split ratio used by `PaneGroup`.
- Added focused native tests for parsing, clamping, round-tripping, and split
  ratio translation.
- Updated the feature tracker to close the persisted layout-width row.

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

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent native_preferences -- --nocapture`
- `cargo test -p email-desktop-fluent native_layout -- --nocapture`
- `cargo test -p email-desktop-fluent`
- `git diff --check`
