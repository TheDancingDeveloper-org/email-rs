# FluentGUI Phase 3g Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the seventh Phase 3 generic FluentGUI extension: rich document
  viewing.
- Added `fluent-layout::DocumentViewer` plus `RichDocument`,
  `RichDocumentBlock`, and `RichTextRun`.
- Added lossy HTML/plain-text conversion helpers for safe rendering of headings,
  paragraphs, inline emphasis, links, code, images, and simple tables.
- Added FluentGUI unit coverage for paragraph splitting, safe HTML block
  extraction, script/style skipping, and attribute decoding.
- Updated the FluentGUI widgets example and README component inventory.
- Updated the email-rs FluentGUI skeleton reading pane to render the initially
  selected message body through `DocumentViewer`, preferring HTML body content
  and falling back to text or preview content.

## Assumptions

- The generic viewer does not execute HTML, preserve arbitrary CSS, run scripts,
  iframe content, or load remote images. It converts known safe tags into a
  FluentGUI document model and ignores the rest.
- The current HTML helper is intentionally lossy. It is sufficient for fixture
  validation and early native reading-pane work, but real-world email acceptance
  still needs visual review and likely policy tuning.
- Loading the initially selected full message uses the existing backend
  `GET /messages/{id}` behavior, which marks that message read. This matches
  opening a message in the current app but is still a Phase 4/5 behavior point
  to revisit when native navigation is fully stateful.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.

## Verification

- `cargo fmt --all --check` from the FluentGUI workspace.
- `cargo test -p fluent-layout` from the FluentGUI workspace.
- `cargo check -p widgets` from the FluentGUI workspace.
- `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8610` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/messages/fx-a1-msg-html-unread`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8611` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the smoke script stopped it.
