# FluentGUI Phase 7n Progress

Last updated: 2026-05-26

## Scope Completed

- Added native reading-pane snooze preset buttons for later today, tomorrow,
  and next week.
- Kept the existing native `DateTimePicker` path as the custom snooze action.
- Moved snooze controls into a dedicated wrapping row under the primary reading
  actions so the header does not depend on horizontal overflow.
- Added focused tests for the web-compatible preset calculations: +3 hours,
  tomorrow at 9am, next Monday at 9am, and skipping the current Monday.
- Updated the feature tracker to close the native snooze preset row.

## Remaining Gaps

- Attachment behavior remains a product gap because the current app only
  exposes an attachment indicator, not download/open controls.
- Sidebar label color-dot fidelity and persisted sidebar/message-list widths
  still need native polish.
- Native Calendar still needs scroll-to-8am and richer timed-event overlap and
  duration fidelity.
- Command palette keyboard and mouse behavior still needs a running-app manual
  walkthrough.
- Sync strip hide-after-completion behavior still needs a manual parity check.
- Windows lifecycle, tray behavior, OAuth callbacks, Gmail/Microsoft OAuth, and
  real IMAP/SMTP verification remain manual/platform verification items.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent snooze -- --nocapture`
- `cargo test -p email-desktop-fluent`
