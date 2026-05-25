# FluentGUI Phase 3d Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the fourth Phase 3 generic FluentGUI extension: async
  resource/loading state.
- Added `fluent-core::AsyncResource` with `Idle`, `Loading`, `Ready`, and
  `Error` states.
- Added `ResourceRequest` generation tokens so async completions can be accepted
  only when they match the current request generation.
- Preserved cached ready values through refresh/loading and error states so
  views can keep rendering stale-but-useful data while a refresh is in flight.
- Added FluentGUI unit coverage for idle state, current-request resolution,
  stale-response rejection, refresh with cached data, retry generation, and
  reset behavior.
- Updated the FluentGUI README to document the generic helper.
- Updated the email-rs FluentGUI skeleton to represent initial account and
  message loads with `AsyncResource<Vec<_>, String>` while preserving the
  current synchronous startup behavior.

## Assumptions

- Phase 3d introduces the generic state machine and an email-rs consumption seam.
  It does not yet move startup or navigation fetches onto background tasks.
- Request spawning, cancellation policy, and SSE-triggered refreshes stay with
  application code because those decisions depend on account/folder/search
  semantics.
- The helper stores previous ready values by moving them into the loading/error
  state. Callers that need immutable shared data can store `Arc<T>` or cloneable
  collections as their resource value.
- The current native shell still loads accounts/messages before opening the
  window. Later Phase 4 navigation can replace that with cancellable async
  refreshes using the same resource fields.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.

## Verification

- `cargo fmt --all --check` from the FluentGUI workspace.
- `cargo test -p fluent-core` from the FluentGUI workspace.
- `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8604` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8605` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the 10 second smoke-test timeout stopped it.
