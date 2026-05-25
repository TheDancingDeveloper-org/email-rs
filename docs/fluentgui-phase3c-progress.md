# FluentGUI Phase 3c Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the third Phase 3 generic FluentGUI extension: token/chip input
  with anchored suggestions.
- Added `fluent-primitives::TokenInput` with freeform token creation,
  dismissible chips, filtered suggestions, keyboard selection, validation state,
  and token/query callbacks.
- Added reusable `TokenInputToken` and `TokenInputSuggestion` data types for
  application-owned values and display metadata.
- Added FluentGUI unit coverage for suggestion filtering, selected-token
  exclusion, and token validity state.
- Updated the FluentGUI widgets example and README component inventory.
- Updated the email-rs FluentGUI skeleton to consume `TokenInput` in the reading
  pane placeholder as a recipient field seeded from already-loaded message
  senders.

## Assumptions

- Phase 3c covers the generic primitive and an email-rs consumption seam. The
  full native compose surface, contact history fetching, and send flow remain
  Phase 5 work.
- Recipient suggestions are seeded from initial message senders for now because
  live `/api/v1/contacts/suggest` integration needs the later async resource
  and navigation refresh work.
- Freeform token entry is enabled so manual email entry is possible before
  contact suggestions are connected.
- The anchored suggestion surface is owned by `TokenInput`; a standalone generic
  suggestion list can be extracted later if sidebar search needs a lower-level
  primitive.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.

## Verification

- `cargo fmt --all --check` from the FluentGUI workspace.
- `cargo test -p fluent-primitives` from the FluentGUI workspace.
- `cargo check -p widgets` from the FluentGUI workspace.
- `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8602` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8603` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the 10 second smoke-test timeout stopped it.
