# FluentGUI Phase 5c Progress

Last updated: 2026-05-25

## Scope Completed

- Added native checked-message selection state for the message list.
- Added select-all checkbox behavior with checked, unchecked, and
  indeterminate states based on visible rows.
- Native message rows now include selection checkboxes without changing the
  focused/opened message behavior.
- Added a selected-count bulk bar surface in the message-list header with
  Archive, Mark read, Mark unread, Delete, and Clear actions.
- Bulk actions call the existing `/messages/bulk` API through `email-client`
  and refresh accounts, folders, labels, active mailbox, selected message, and
  unread title/status through the guarded refresh path.
- Added message-list keyboard parity for Ctrl/Cmd+A, Escape, `x`, `e`,
  Delete/Backspace, and `u`, with checked-message actions taking precedence
  over the focused message.
- Added native row right-click context menus with Open, Reply, Forward,
  Archive, Delete, Mark read/unread, and Flag/Unflag actions.

## Assumptions

- Row Reply and Forward currently load/open the row's message, matching the
  previous web row-menu behavior where the full compose flow is still a later
  Phase 5 item.
- Bulk delete does not yet add an extra native confirmation prompt.
- The existing untracked `.claude/` directory is unrelated and intentionally
  left unstaged.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
