# FluentGUI Phase 7i Progress

Last updated: 2026-05-26

## Scope Completed

- Added the missing native `Cmd+K` key binding alongside the existing `Ctrl+K`
  palette binding.
- Moved native key binding declarations behind a small spec helper so palette
  shortcut parity can be covered by focused native tests.
- Added a native test that verifies both Ctrl+K and Cmd+K are bound to the
  palette action.
- Audited the shared FluentGUI command palette implementation and tests for
  grouped entries, direct actions, disabled entries, fuzzy matching, subtitle,
  group, and keyword matching.
- Updated the feature tracker for command palette shortcut, grouping, fuzzy
  matching, and keyboard/mouse implementation status.

## Remaining Gaps

- Command palette Up/Down/Enter/Escape and mouse selection still need a
  running-app manual walkthrough.
- Native Calendar still needs scroll-to-8am and richer timed-event overlap and
  duration fidelity.
- Sync strip hide-after-completion behavior still needs a manual parity check.
- Server-backed compose recipient contact suggestions remain pending.
- Larger parity remains open for Windows lifecycle/tray behavior,
  message-list/thread-row polish, richer reading body/attachment behavior, and
  real-provider OAuth/IMAP/SMTP verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
- `cargo test -p fluent-layout command_palette` in the FluentGUI workspace
