# FluentGUI Phase 6a Progress

Last updated: 2026-05-26

## Scope Completed

- Added native advanced-search state alongside basic search state so structured
  searches participate in guarded message collection loads and refreshes.
- Added a native advanced search modal with match all/any controls, dynamic
  condition rows, field/operator/value controls, boolean controls, add/remove,
  Clear, Cancel, and Search actions.
- Wired the sidebar Filter button to the native modal and indicates when an
  advanced search is active.
- Wired the command-palette Advanced Search entry to the native modal.
- Added an advanced search message source that calls the existing typed
  `email-client` advanced search method and reuses the current frontend query
  parameter shape.
- Added focused native tests for advanced-search route state, title behavior,
  and clearing advanced search when a basic search is submitted.

## Assumptions

- This slice preserves the current frontend URL/query shape. Backend gaps for
  structured operators, match all/any, and boolean/date-only queries remain
  product/backend parity items already logged in the tracker.
- Date values use the same `YYYY-MM-DD` string shape as the current browser date
  input rather than adding a separate native date picker for this modal.
- The existing untracked `.claude/` directory is unrelated and intentionally
  left unstaged.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
