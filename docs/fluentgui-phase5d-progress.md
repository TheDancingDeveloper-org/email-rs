# FluentGUI Phase 5d Progress

Last updated: 2026-05-26

## Scope Completed

- Expanded native compose state from quick-reply metadata into a full draft
  model with To/Cc/Bcc, subject, body seed, mode, quote, signature, visibility,
  sending status, and revision tracking.
- Added a full native compose pane that replaces the reading pane while open,
  with recipient token inputs, optional Cc/Bcc rows, subject input, multiline
  body editor, signature preview, quote preview, Send, and Discard.
- Sidebar Compose now opens a new native draft for the first account.
- Reading-pane Reply and Forward now open the full compose pane instead of
  seeding the quick-reply footer.
- Row context-menu Reply and Forward select/load the row message and open a full
  reply or forward draft.
- Command-palette New Message, Reply, and Forward now dispatch native compose
  actions, with palette entries refreshed when the palette opens.
- Compose send builds the same `/api/v1/messages` payload shape as the web
  flow, including normalized recipient arrays, signature appending, reply quote
  appending, and omitted `In-Reply-To` for forwards.
- Added a minimal `Textarea::set_value` and `Textarea::set_disabled` primitive
  API for native multiline editor seeding and cleanup.

## Assumptions

- Signature loading follows the web behavior of silently continuing without a
  signature if account settings cannot be fetched.
- Recipient suggestions still come from visible message senders for this native
  slice; server-backed contact suggestion fetching remains a later parity item.
- The existing untracked `.claude/` directory is unrelated and intentionally
  left unstaged.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check` in email-rs and FluentGUI
- `git diff --check` in email-rs and FluentGUI
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
