# FluentGUI Phase 3a Progress

Last updated: 2026-05-25

## Scope Completed

- Implemented the first Phase 3 generic FluentGUI extension: constrained
  resizable split panels.
- Extended `fluent-layout::PaneGroup` with reusable pixel constraints for the
  first and second panes, optional max sizes, ratio guardrails, and a resize
  callback for application-owned persistence.
- Exported `PaneConstraints` and the `ResizablePanelGroup` alias from
  `fluent-layout`.
- Added FluentGUI unit coverage for default ratio guardrails, pane minimums,
  pane maximums, and impossible-constraint fallback behavior.
- Updated the FluentGUI gallery to demonstrate the generic split-panel API
  without email-specific imports.
- Updated the email-rs FluentGUI skeleton to consume the generic split panel for
  the message-list and reading-pane split.

## Assumptions

- Phase 3 is being split into working milestones. Phase 3a covers only
  `SplitView`/`ResizablePanelGroup`; enhanced command palette, async loading,
  token inputs, date/time picker, rich document viewer, and lifecycle hooks
  remain later Phase 3 milestones.
- Split ratios are stored in the FluentGUI entity for now. The generic resize
  callback provides the persistence seam, but writing sidebar and message-list
  widths to native settings belongs with the Phase 4 persistent layout work.
- `PaneGroup` applies constraints using the current window extent. This is
  adequate for the Phase 3a shell and gallery milestone, but a later nested
  layout pass can tighten this if FluentGUI exposes local pane bounds.
- The existing dirty `examples/screenshots/src/main.rs` formatting change in
  the FluentGUI repo is unrelated to this milestone and is intentionally left
  unstaged.

## Verification

- `cargo fmt --all --check` from the FluentGUI workspace.
- `cargo test --workspace` from the FluentGUI workspace.
- `cargo fmt --all --check` from the email-rs workspace.
- `cargo check -p email-desktop-fluent` from the email-rs workspace.
- `cargo test --workspace` from the email-rs workspace.
- `npm run build` from `frontend/`.
- Headless native server smoke on `127.0.0.1:8596` using a temporary copy of
  `fixtures/fluentgui-phase0/one-account.sqlite`:
  - `/api/v1/accounts`
  - `/api/v1/accounts/fx-a1/folders`
  - `/api/v1/folders/fx-a1-inbox/messages`
  - `/api/v1/search?q=FluentGUI`
- Display-mode native startup smoke on `127.0.0.1:8597` using the same fixture
  DB pattern. The process initialized the in-process server and GPUI/Blade
  window, then stayed alive until the 10 second smoke-test timeout stopped it.
