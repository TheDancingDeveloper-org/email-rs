# FluentGUI Phase 7o Progress

Last updated: 2026-05-26

## Scope Completed

- Replaced native sidebar label pin icons with compact color dots matching the
  React sidebar label affordance.
- Added a small CSS-hex parser for label colors with support for `#RGB`,
  `#RRGGBB`, and unprefixed `RRGGBB` values.
- Kept malformed label colors safe by falling back to the current Fluent accent
  color.
- Added focused native coverage for accepted and rejected label color formats.
- Updated the feature tracker to close the sidebar label color-dot row.

## Remaining Gaps

- Persisted sidebar/message-list widths still need native settings support.
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
- `cargo test -p email-desktop-fluent label_color -- --nocapture`
- `cargo test -p email-desktop-fluent`
