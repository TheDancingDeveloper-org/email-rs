# FluentGUI Phase 5b Progress

Last updated: 2026-05-25

## Scope Completed

- Added native selected-message label state and guarded async loading alongside
  selected message body loading.
- Native message collection loads now fetch labels for the selected message so
  the reading pane can render current label chips immediately.
- Native full-message loads now fetch message labels and reset stale compose and
  label state when the user changes messages.
- Added reading-pane label chips with remove actions and add buttons for labels
  on the selected message's account.
- Add/remove label actions call the existing message label APIs through
  `email-client`, then reload only the selected message's labels.

## Assumptions

- Labels are rendered as compact action chips for this slice. A richer dropdown
  or color-dot treatment can be added later without changing the state/API
  wiring.
- Label CRUD remains outside the reading pane; this slice only adds/removes
  existing labels on a message.
- The existing untracked `.claude/` directory is unrelated and intentionally
  left unstaged.

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
