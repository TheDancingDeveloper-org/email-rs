# FluentGUI Phase 7k Progress

Last updated: 2026-05-26

## Scope Completed

- Wired native quick-reply and full-compose recipient token inputs to the typed
  `email-client` contact suggestion API.
- Added native recipient suggestion request state with stale-response rejection
  and short-query clearing.
- Merged server-backed contacts ahead of visible message senders so compose
  still has local fallback suggestions while contact history is loading or
  unavailable.
- Synced recipient suggestions into To, Cc, Bcc, and quick-reply token inputs.
- Added focused tests for contact-first suggestion merging, sender fallback
  dedupe, stale suggestion responses, rejection, and clearing.
- Updated the feature tracker to close the server-backed compose recipient
  suggestion gap.

## Remaining Gaps

- Reading pane safe HTML/rich body rendering remains the largest mail-view
  parity gap.
- Attachment behavior remains a product gap because the current app only
  exposes an attachment indicator, not download/open controls.
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
- `cargo test -p email-desktop-fluent recipient_suggestions -- --nocapture`
- `cargo test -p email-desktop-fluent`
