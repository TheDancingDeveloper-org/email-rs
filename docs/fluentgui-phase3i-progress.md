# FluentGUI Phase 3i Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the remaining Phase 3 generic FluentGUI extension: anchored
  suggestion popovers.
- Added `fluent-primitives::SuggestionPopover`, `SuggestionItem`,
  `SuggestionListState`, and `SuggestionKeyAction`.
- Added generic keyboard roving coverage for Down/Up/Home/End, Enter/Return/Tab
  selection, Escape close, and disabled-row skipping.
- Refactored `fluent-primitives::TokenInput` to render its filtered suggestion
  list through the reusable `SuggestionPopover`.
- Kept the extension generic: it has no email-rs imports or mail-specific
  assumptions.
- email-rs consumes the extension through the existing native reading-pane
  recipient `TokenInput`; no additional email-rs source changes were needed for
  this milestone.

## Assumptions

- Sidebar search will use the same generic popover/state model when Phase 4
  implements native folder/search navigation.
- Recipient autocomplete already exercises the popover path through
  `TokenInput`, but full async contact lookup and server-backed suggestions
  remain Phase 5 compose work.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.
- The existing untracked `.claude/` directory in the email-rs repo is unrelated
  and is intentionally left unstaged.

## Verification

- FluentGUI framework verification before `a78ce0a`:
  - `cargo fmt --all --check` from the FluentGUI workspace.
  - `cargo test -p fluent-primitives` from the FluentGUI workspace.
  - `cargo check -p widgets` from the FluentGUI workspace.
  - `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8614` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8615` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the smoke script stopped it.
