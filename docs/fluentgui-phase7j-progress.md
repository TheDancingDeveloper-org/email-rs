# FluentGUI Phase 7j Progress

Last updated: 2026-05-26

## Scope Completed

- Added native message-list thread grouping for regular folder views.
- Kept native search, advanced search, smart-folder, and label result views flat
  to match the React/WebView behavior.
- Added thread expansion state, automatic child visibility for selected thread
  messages, collapse controls, and thread-count badges on grouped rows.
- Preserved visible-row keyboard navigation so collapsed child messages do not
  become hidden keyboard targets.
- Added focused native tests for regular-folder grouping, latest-message
  selection inside a thread, expanded child rows, collapse markers, and flat
  non-regular views.
- Updated the feature tracker for message-list thread grouping and native row
  detail status.

## Remaining Gaps

- Settings rule creation/editing and Settings modal Escape/Enter handling are
  implemented from Phase 6f; account setup remains tracked under startup.
- Reading pane safe HTML/rich body rendering remains the largest mail-view
  parity gap.
- Attachment behavior remains a product gap because the current app only
  exposes an attachment indicator, not download/open controls.
- Native compose recipient suggestions still use visible message senders rather
  than server-backed contact suggestions.
- Native Calendar still needs scroll-to-8am and richer timed-event overlap and
  duration fidelity.
- Command palette keyboard and mouse behavior still needs a running-app manual
  walkthrough.
- Windows lifecycle, tray behavior, OAuth callbacks, Gmail/Microsoft OAuth, and
  real IMAP/SMTP verification remain manual/platform verification items.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check -p email-desktop-fluent`
- `cargo test -p email-desktop-fluent message_list_render_rows -- --nocapture`
- `cargo test -p email-desktop-fluent`
