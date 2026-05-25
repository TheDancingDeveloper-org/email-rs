# FluentGUI Phase 0 Progress

Last updated: 2026-05-25

## Scope Completed

- Created a parity tracker from the migration plan feature matrix:
  `docs/fluentgui-feature-tracker.md`.
- Recorded structured current-UI observations instead of screenshots:
  `docs/fluentgui-current-ui-observations.md`.
- Recorded current keyboard shortcuts and context menus:
  `docs/fluentgui-shortcuts-context-menus.md`.
- Added a deterministic fixture generator:
  `crates/email-server/src/bin/seed_fixtures.rs`.
- Added fixture usage notes:
  `fixtures/fluentgui-phase0/README.md`.

## Assumptions

- Phase 0 is the current executable milestone. Phase 1 starts after the tracker,
  fixtures, and current UI audit are in place and verified.
- Structured observations satisfy the Phase 0 requirement for screenshots or
  observations. Screenshots can be added later if Playwright/browser automation
  becomes part of native smoke testing.
- Fixture accounts intentionally use offline-safe credentials and non-routable
  hosts. This keeps local verification deterministic and avoids credentials or
  external mail services.
- SQLite fixture files remain untracked because `.gitignore` excludes `.sqlite`
  and `.db` files. The committed generator is the reproducible source.

## Product And Parity Gaps Logged

- README says Reply All, but current desktop UI exposes Reply and Forward only.
- README says Google Calendar read plus write, but current desktop UI exposes
  week/detail viewing, search navigation, and linked-email navigation only.
- Attachments are indicated only; no current desktop download/open UI was found.
- Label CRUD APIs exist, but current desktop UI only lists labels and adds or
  removes labels from messages.
- Advanced search frontend builds structured query parameters, but the backend
  currently parses only `q` and optional `account_id`.
- `messages.snoozed_until` is stored and used by smart-folder filters, but the
  message list API row does not currently serialize `snoozedUntil`.

## Next Milestone

Phase 1 should extract shared DTOs into `email-types`, add `email-client`, test
URL/JSON/error behavior, and refactor server startup enough for the native app
to start Axum in-process.
