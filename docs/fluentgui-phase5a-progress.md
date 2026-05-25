# FluentGUI Phase 5a Progress

Last updated: 2026-05-25

## Scope Completed

- Replaced inert native reading-pane toolbar chips with real FluentGUI buttons
  wired to the existing message APIs through `email-client`.
- Native reading pane actions now support reply draft seeding, forward draft
  seeding, mark read/unread, archive, delete, and custom-date snooze.
- Archive, delete, read/unread, and snooze actions refresh the account tree,
  labels, active mailbox, selected message, and native unread title/status
  through the existing guarded refresh path.
- Added a native quick-reply footer with recipient token input, reply body
  input, send button, sending state, and error/sent feedback.
- Quick reply builds real `SendRequest` payloads, defaults replies to the
  selected sender, preserves `In-Reply-To`, and treats forwards as explicit
  recipient sends without `In-Reply-To`.
- Fixed message mutation APIs to refresh folder `unread_count` and
  `total_count` after get/open, patch read state, delete, archive, and bulk
  mutations so native refreshes do not restore stale sidebar counts.

## Assumptions

- This slice keeps compose scoped to the reading pane quick action surface.
  The full native compose screen with Cc/Bcc, signatures, discard handling, and
  recipient suggestions remains a later Phase 5 item.
- Native label add/remove controls are still pending; the backend APIs and web
  UI already exist, but the native reading pane does not yet render label chips.
- The custom snooze picker uses the existing generic FluentGUI
  `DateTimePicker`; preset snooze buttons can be layered on top later.
- The existing untracked `.claude/` directory is unrelated and intentionally
  left unstaged.

## Verification

- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
- `cargo test -p email-server api::messages::tests:: -- --nocapture`
