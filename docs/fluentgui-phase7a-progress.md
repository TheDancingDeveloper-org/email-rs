# FluentGUI Phase 7a Progress

Last updated: 2026-05-26

## Scope Completed

- Wired command-palette smart-folder entries to native message collection
  navigation.
- Wired label and folder palette entries to native label/folder loading instead
  of leaving them disabled.
- Added account palette entries that navigate to the account inbox, or the first
  visible account folder when no inbox exists.
- Added selected-message palette actions for Archive, Delete, Mark Read/Unread,
  and Flag/Unflag.
- Kept Reply and Forward palette actions on the existing native compose paths.
- Added focused tests for account-folder target selection and selected-message
  palette state fallback.

## Remaining Gaps

- Command-palette keyboard behavior still needs a manual UI walkthrough in the
  native app, even though the generic palette handles Up/Down/Enter/Escape.
- Calendar-event search/navigation from the palette is not implemented natively.
- Additional message actions may still be needed if web parity expands beyond
  the current selected-message actions.
- Larger parity remains open for Windows lifecycle/tray behavior, Calendar,
  message-list and reading-pane polish, advanced-search backend structured
  params, snoozed row serialization, and real-provider verification.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent`
