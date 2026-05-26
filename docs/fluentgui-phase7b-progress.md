# FluentGUI Phase 7b Progress

Last updated: 2026-05-26

## Scope Completed

- Changed the search endpoint so structured advanced-search requests no longer
  require a fallback `q` query.
- Added backend parsing for structured message search params:
  `match=all|any`, text fields, text operators, boolean fields, date bounds,
  filename filters, and account scoping.
- Kept basic `q` search on the existing FTS/LIKE paths when no structured
  message params are present.
- Avoided returning calendar search results for structured message searches;
  basic `q` search still returns matching events.
- Added backend tests for no-`q` structured search, match-all text operators,
  match-any boolean/date search, filename plus account filtering, and
  `snoozedUntil` row serialization.

## Remaining Gaps

- Advanced-search behavior still needs a native/manual UI walkthrough against
  the running app.
- Calendar-event search/navigation from the command palette is still pending.
- Larger parity remains open for Windows lifecycle/tray behavior, Calendar,
  message-list and reading-pane polish, and real-provider verification.

## Verification

- Targeted `cargo test -p email-server structured -- --nocapture`
- Targeted `cargo test -p email-server list_messages_serializes_snoozed_until -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-server -p email-client -p email-desktop-fluent`
- `cargo test -p email-server`
- `cargo test -p email-client`
- `cargo test -p email-desktop-fluent`
