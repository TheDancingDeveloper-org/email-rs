# FluentGUI Phase 7l Progress

Last updated: 2026-05-26

## Scope Completed

- Audited the native reading-pane body path against the React iframe baseline.
- Confirmed native reading already routes HTML bodies through
  `RichDocument::from_html_fragment_lossy` and `DocumentViewer`, with text and
  preview fallbacks when rich body content is unavailable.
- Added focused native tests proving HTML is preferred over text, script/style
  content is omitted by the safe rich-document path, text fallback is preserved,
  and summary preview fallback still works before a full body is loaded.
- Updated the feature tracker to close the native safe HTML/text fallback row.

## Remaining Gaps

- The native rich body renderer is intentionally lossy compared with a browser
  iframe; complex email CSS/layout fidelity still needs a manual visual pass.
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
- `cargo test -p email-desktop-fluent selected_document -- --nocapture`
- `cargo test -p email-desktop-fluent`
