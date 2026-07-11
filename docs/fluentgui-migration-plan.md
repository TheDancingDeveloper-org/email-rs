# FluentGUI Desktop Migration Plan

> Historical planning document. The native crate now exists and committed
> manifests use immutable Git dependencies rather than the local paths proposed
> below. Use [the feature tracker](fluentgui-feature-tracker.md) for UI parity and
> [the recovery audit](recovery-audit-2026-07-10.md) for current release blockers.

Last updated: 2026-05-25

Target framework: `/home/sprooty/Working/Active/apps/FluentGUI`

## Goal

Replace the desktop app's WebView/React GUI with a native FluentGUI desktop UI while preserving the current desktop feature surface. The migration is not complete until a user can move from the current desktop app to the FluentGUI app without asking where a mail, calendar, settings, search, command, account, or desktop-shell feature went.

The current Axum backend, SQLite database, sync logic, OAuth, SMTP/IMAP providers, rules engine, labels, smart folders, snooze, webhooks, and calendar storage should remain the behavioral source of truth.

## Non-goals

- Do not rewrite the mail/calendar backend as part of the GUI migration.
- Do not remove the existing browser/headless web UI until the native desktop UI has passed parity gates.
- Do not migrate the mobile app in this project. `mobile/` remains separate.
- Do not push email-specific screens into FluentGUI. FluentGUI can be extended, but extensions must be generic.
- Do not silently downgrade HTML mail rendering, keyboard shortcuts, tray behavior, sync progress, or settings. If a feature is hard, it becomes an explicit blocking item.

## Migration Principles

1. Keep the current desktop path available until native parity is proven.
2. Build the native UI against the existing API contract first; optimize to direct service calls later only if needed.
3. Put reusable shell/widgets into FluentGUI and app-specific mail behavior into `email-rs`.
4. Treat the feature matrix below as a release checklist, not a rough guide.
5. Every user-visible feature needs an owner, a native implementation, and a verification path.
6. Any README-claimed feature that the current desktop UI does not actually expose must be resolved before cutover by either adding it or correcting the product claim.

## Target Architecture

### Crates

Use this shape so the migration can happen incrementally:

- `crates/email-server`
  - Keep the Axum API, database, sync, providers, auth, rules, and webhooks.
  - Extract reusable startup functions into a library surface if needed, instead of keeping everything only in `main.rs`.
  - Keep static-file serving for browser/headless/Docker mode.

- `crates/email-types`
  - New small crate for shared Rust DTOs used by Axum handlers and the native GUI client.
  - Mirror current frontend TypeScript types: accounts, folders, messages, message body, labels, conditions, events, webhooks, rules, sync progress.
  - Use the same JSON casing as the existing `/api/v1` contract.

- `crates/email-client`
  - New async Rust client for the current HTTP API.
  - Handles `GET`, `POST`, `PATCH`, `PUT`, `DELETE`, JSON errors, request cancellation, and SSE sync events.
  - Keeps native UI code from manually formatting endpoint URLs everywhere.

- `crates/email-desktop-fluent`
  - New FluentGUI native app crate or a new feature-gated binary in `crates/email-server`.
  - Depends on FluentGUI via local path dependencies.
  - Owns app state, views, native command actions, and mail/calendar screen code.

- `frontend/`
  - Keep as the browser/headless UI until a separate decision removes it.
  - Continue building into `crates/email-server/static` for Docker and browser mode.

### Runtime Model

Initial migration path:

1. Start the existing Axum server in-process on the configured port.
2. Start the FluentGUI window instead of a WebView window.
3. The FluentGUI app talks to `http://127.0.0.1:{port}/api/v1` through `email-client`.
4. SSE sync progress continues to flow through `/api/v1/events`.
5. OAuth opens the provider URL in the system browser; callback still lands on the local Axum server.

This keeps backend behavior identical to the current desktop app. A later optimization can replace HTTP calls with direct service methods, but that should happen only after UI parity is complete.

### Desktop Modes

- Linux with display: launch Axum plus FluentGUI.
- Linux headless or `EMAIL_RS_HEADLESS=1`: launch Axum plus static web UI only.
- Windows: launch Axum plus FluentGUI, preserve single-instance and tray behavior.
- Docker/server mode: unchanged.

## FluentGUI Extension Policy

Because FluentGUI can be extended, each framework addition must satisfy these rules:

- Generic name and API; no email-specific types, endpoint names, or copy.
- Data enters through callbacks, entities, or generic structs.
- The extension is demonstrated in a FluentGUI example or test.
- Styling comes from `fluent-core` tokens.
- The extension remains useful for at least one non-email productivity/admin app.

Email-specific screens that should stay in `email-rs`:

- Mail folder tree semantics.
- Message row layout.
- Mail compose behavior.
- Rules fields/actions.
- Calendar event/email link behavior.
- Provider account setup copy and OAuth flow.

## Generic FluentGUI Extensions Needed

| Extension | Why email-rs needs it | Generic FluentGUI shape | Cutover gate |
|---|---|---|---|
| `SplitView` or `ResizablePanelGroup` | Current app has sidebar, message list, and reading pane with persisted widths. `DockPanel` covers outer docks but not arbitrary nested resizable panes. | Layout primitive for 2+ resizable panels, min/max sizes, persisted size callbacks, vertical/horizontal orientation. | Sidebar and message list resize like current app and restore widths after restart. |
| Enhanced `CommandPalette` | Current palette has grouped commands, hints, fuzzy scoring, navigation, labels, folders, message actions, and action handlers. Existing FluentGUI palette is entry-selection oriented. | Grouped action palette with shortcuts/hints, weighted fuzzy scoring, section headers, disabled entries, dynamic refresh, direct action callbacks. | Ctrl+K can execute every current palette action. |
| `AsyncResource`/loading helpers | Native views need request state, cancellation, stale-response protection, error states, and refresh after SSE. | Generic entity helper or pattern for async loading with `Idle/Loading/Ready/Error`, request generation, and refresh callbacks. | No stale folder/message/search result flashes during rapid navigation. |
| `VirtualList` or `SelectableList` | Mail folders can have long message lists; current React list is simple but native should not regress performance. GPUI has low-level list support but FluentGUI should expose a reusable primitive if needed. | Virtualized list with selection, keyboard navigation, row context menu, empty/loading states. | 1,000+ message folder remains responsive. |
| Token/chip input with suggestions | Compose recipients need comma-separated recipients and autocomplete. | Generic multi-value input with chips, async suggestions, keyboard selection, validation state. | To/Cc/Bcc preserve autocomplete and comma token behavior. |
| Anchored suggestion popover | Sidebar search and recipient input need fixed-position suggestions. | Generic anchored popover/listbox with arrow navigation, Enter/Tab selection, Escape close, click-away close. | Search suggestions and contact suggestions behave like current UI. |
| Date/time picker | Snooze custom datetime currently uses `datetime-local`; settings/rules can need dates. | Generic date/time field or popover with local time conversion. | Custom snooze can set an exact local date/time. |
| Rich document viewer | Current reading pane displays HTML email in an iframe with sandboxing, or text fallback. Native GPUI cannot simply iframe arbitrary HTML. | Generic sanitized document viewer for a safe subset of HTML/rich text: paragraphs, headings, inline formatting, links, quotes, tables, basic images policy, scroll container. Email-specific sanitization policy stays in `email-rs`. | HTML and plain-text message bodies are readable enough that this is not a downgrade. |
| App lifecycle hooks | Windows currently has single-instance, close-to-tray, tray Open/Quit, icon, and foreground existing instance. | `fluent-app` builder hooks for app id, single instance, tray menu, close behavior, title/icon update. Platform-specific internals can use existing crates where appropriate. | Windows desktop behavior matches current app. |
| Dynamic window title/status | Current browser title shows unread count. Native app should surface unread count in the title/tray tooltip/status. | Generic title/status update API in `fluent-app` or app-owned wrapper. | Unread count remains visible without opening folders. |

## Feature Parity Matrix

### Desktop Shell

| Current behavior | Native target | Notes |
|---|---|---|
| Linux opens a WebKitGTK window automatically when display exists. | Linux opens a FluentGUI window automatically when display exists. | Keep `EMAIL_RS_HEADLESS=1` server-only behavior. |
| Linux falls back to browser/server mode when headless. | Same. | Static frontend remains available. |
| Windows uses Wry/WebView2. | Windows uses FluentGUI/GPUI. | Remove WebView2 runtime dependency only after parity. |
| Windows enforces single instance and foregrounds existing window. | Same through generic FluentGUI app lifecycle extension. | Blocking parity item. |
| Windows close button hides to tray, tray has Open and Quit. | Same. | Blocking parity item. |
| Window title is `email-rs`; browser title shows unread count. | Native title/tray/status shows unread count. | Browser title still applies to web UI. |
| Default size 1280x800, Windows min 800x600, icon set. | Same window size/min/icon. | Use FluentApp `.window_size()` plus new min-size/icon support if needed. |
| Snap/WebKit-specific Linux environment workarounds. | Re-evaluate after GPUI. | Keep only if still needed for packaged Linux builds. |

### Startup and Account Setup

| Current behavior | Native target | Notes |
|---|---|---|
| Blank while accounts load. | Native loading state or brief skeleton. | Avoid confusing empty app. |
| First-run account setup screen. | Native setup screen. | Same provider choices. |
| Gmail OAuth start. | Button fetches authorize URL and opens system browser. | Callback handled by local server. |
| Microsoft 365 OAuth start. | Same. | Preserve OAuth error messaging. |
| Microsoft app-password fallback. | Native form with defaults `outlook.office365.com` and `smtp.office365.com`. | Same validation/submitting/error behavior. |
| Manual IMAP/SMTP form. | Native form with same fields and same-password toggle. | Same POST body. |
| Add account modal from sidebar. | Native modal. | Same success refresh. |

### Main Layout and Navigation

| Current behavior | Native target | Notes |
|---|---|---|
| 3-pane mail layout: sidebar, message list, reading pane. | FluentGUI workspace with resizable sidebar and message-list split. | Requires generic split/resizable panels. |
| Calendar replaces message list + reading pane while sidebar remains. | Same view switch. | Preserve selected event state. |
| Persist theme and density. | Persist native theme and density. | Can use config file or existing app state persistence. |
| Sidebar width and message-list width persist in localStorage. | Persist in native settings. | Store under email-rs app data path. |
| Density controls from sidebar footer and settings. | Same. | Existing levels 0..8 and labels should remain. |
| Light/dark toggle. | Same through FluentGUI theme. | Verify all custom app views subscribe to theme. |

### Sidebar

| Current behavior | Native target | Notes |
|---|---|---|
| Compose button opens new compose for first account. | Same. | Disabled/no-op when no account. |
| Sync progress strip from SSE with spinner/progress bar/completion timeout. | Same. | Use `email-client` SSE stream and FluentGUI progress primitives. |
| Search mail and calendar with 150 ms suggestions. | Same. | Suggestions include messages and calendar events. |
| Search suggestion keyboard: Up/Down/Enter/Escape. | Same. | Enter on active suggestion navigates; Enter without active searches all. |
| Advanced search filter button state. | Same. | Opens native advanced search modal. |
| Smart folders: All Inboxes, Unread, Flagged, Snoozed. | Same. | Same IDs and APIs. |
| Calendar nav item. | Same. | Switches to calendar view. |
| Labels section with color dots. | Same. | Current UI displays labels but does not expose full label CRUD. Do not invent CRUD unless separately scoped. |
| Per-account collapsible sections. | Same. | Preserve account unread badge. |
| Special folders ordered inbox, sent, drafts, archive, trash, spam. | Same. | Preserve icons. |
| Other folders under collapsible Folders group with indentation. | Same. | Preserve `[Gmail]/` depth behavior. |
| Folder context menu: open, mark all read. | Same. | Use FluentGUI `ContextMenu`. |
| Account list at bottom with avatars, email, Add account. | Same. | Preserve multiple accounts. |
| Footer density buttons, theme toggle, settings button. | Same. | Use FluentGUI buttons/icons. |

### Message List

| Current behavior | Native target | Notes |
|---|---|---|
| Fetch folder messages, label messages, smart-folder messages, search results, or advanced-search results. | Same through `email-client`. | Preserve selection reset on folder/search changes. |
| Debounced search results. | Same. | 300 ms debounce. |
| Header shows folder/search label, unread count, search count. | Same. | Same labels: `Advanced search results`, `Results for "..."` |
| Select-all checkbox with indeterminate state. | Same. | Needed for bulk actions. |
| Bulk bar with Archive, Mark Read, Mark Unread, Delete, cancel. | Same. | Delete confirmation remains. |
| Mark all read for regular folders. | Same. | Not shown for smart/label/search/advanced search. |
| Empty/loading states. | Same. | Preserve text or equivalent. |
| Thread grouping only in regular folder view. | Same. | Groups by `threadId` or message id; latest first; expand/collapse. |
| Rows show avatar/checkbox, sender, timestamp, subject, preview, unread dot, attachment, flag, snooze, thread count. | Same. | Visual can be Fluent, content must match. |
| Row context menu: open, reply, forward, archive, trash, mark read/unread, flag/unflag. | Same. | Current reply/forward from row only selects message; native should match or improve by opening compose, but do not remove options. |
| Keyboard: `j`/`k` move, Ctrl/Cmd+A select all, Escape clear/deselect, `x` select focused, `e` archive, Delete/Backspace trash, `u` toggle read. | Same. | Blocking parity item. |

### Reading Pane

| Current behavior | Native target | Notes |
|---|---|---|
| Empty state: select a message. | Same. | Native pane state. |
| Loading and not found states. | Same. | Preserve clear errors. |
| Opening a message marks it read locally and attempts provider mark-seen. | Same backend call behavior. | Preserve list patch. |
| Toolbar: Reply, Forward, Read/Unread, Snooze, Archive, Delete. | Same. | Same shortcuts/tooltips. |
| Snooze presets: later today +3h, tomorrow 9am, next Monday 9am, custom datetime. | Same. | Custom date/time picker required. |
| Message header: subject, flagged tag, labels, sender avatar/name/email, date. | Same. | Preserve label chips and remove buttons. |
| Label add dropdown lists labels not already on the message. | Same. | Add/remove via label APIs. |
| Header context menu: reply, forward, archive, delete, mark read/unread, copy sender email. | Same. | Needs clipboard support. |
| Attachment indicator. | Same current level. | Current UI only displays `attachment`; no download UI exists. |
| HTML body shown in sandboxed iframe when available; text fallback otherwise. | Native rich document viewer with safe rendering and text fallback. | Largest migration risk. Must not silently degrade. |
| Quick reply textarea, send button, sending/error states. | Same. | Send via `/api/v1/messages`. |
| Shortcuts: `e`, `#`, `u`, Escape. | Same. | Ensure no conflict with text inputs. |

### Compose

| Current behavior | Native target | Notes |
|---|---|---|
| Compose replaces reading pane when open. | Same. | Preserve context, not a detached window unless separately scoped. |
| Modes: new, reply, forward. | Same. | Mode tag and subject behavior preserved. |
| Fields: To, optional Cc/Bcc, Subject, body. | Same. | Recipient token/suggestion input needed. |
| Recipient autocomplete from message/contact history. | Same. | Current React code uses `/search/suggest`; native can use `/contacts/suggest` only if output matches user expectations. |
| Account signature appended on send and previewed in editor. | Same. | Preserve `--` separator behavior. |
| Reply includes quoted block on send and quote preview. | Same. | Preserve quoted sender text. |
| Forward pre-fills forwarded message body. | Same. | Preserve header text. |
| Send disabled until recipient and body exist; shows sending/error; close on success. | Same. | Preserve network error handling. |
| Discard closes compose. | Same. | Optional confirm can be added only if not disruptive. |

### Search and Advanced Search

| Current behavior | Native target | Notes |
|---|---|---|
| Basic search from sidebar searches messages and calendar, with suggestions. | Same. | Search result list remains message-focused, calendar suggestion navigates to event. |
| Advanced search modal with condition builder. | Same. | Fields: from, to, cc, subject, body, filename, has attachment, is read, is starred, date after, date before. |
| Match all/any. | Same. | Preserve URL param construction. |
| Text operators: contains, doesn't contain, equals, starts with. | Same. | Backend support must be verified for every operator. |
| Boolean/date controls. | Same. | Native controls should validate active values. |
| Clear, Cancel, Search, Enter/Escape behavior. | Same. | Same disabled Search when no active conditions. |

### Settings

| Current behavior | Native target | Notes |
|---|---|---|
| Settings modal with tabs: General, each account, Rules per account, Webhooks. | Native modal, likely `SettingsNav` sections. | Can change layout but not lose sections. |
| General theme select. | Same. | May use segmented/toggle control. |
| General density stepper with labels. | Same. | Levels 0..8. |
| Account info: display name, email, provider. | Same. | Email/provider read-only. |
| Sync setting: download last N days or all. | Same. | PATCH account. |
| Signature textarea. | Same. | Used by compose. |
| Excluded folders checkbox list. | Same. | Toggle `isExcluded`; reflect state. |
| Remove account danger zone with confirmation. | Same. | Cascade delete behavior remains backend. |
| Save status. | Same. | Success/error feedback. |
| Rules list per account with enable toggle, name, condition/action counts, edit, delete, new. | Same. | Preserve existing actions. |
| Rule editor: name, condition builder, actions, move-to-folder selector, save/cancel/error. | Same. | App-specific screen using generic controls. |
| Webhooks list, enabled toggle, signed indicator, delete. | Same. | Preserve `events` display. |
| Add webhook: URL, secret, events select, add/error. | Same. | Same API payload. |

### Command Palette

| Current behavior | Native target | Notes |
|---|---|---|
| Ctrl/Cmd+K opens/closes palette. | Same. | Bind with GPUI actions. |
| Groups: Actions, Navigate, Labels, Folders, Message. | Same. | Requires enhanced generic palette. |
| Fuzzy matching with label/keyword scoring. | Same or better. | Preserve predictable result ordering. |
| Actions: New Message, Advanced Search, Open Settings, toggle theme, density up/down. | Same. | Same disabled bounds for density. |
| Navigation: Mail, Calendar, smart folders, labels, folders. | Same. | Folder hints include account/email/full path where useful. |
| Message actions: reply, forward, archive, delete, mark read/unread, flag/unflag. | Same. | Current selected message only. |
| Keyboard: Up/Down, Enter, Escape. | Same. | Mouse hover/select too. |

### Calendar

| Current behavior | Native target | Notes |
|---|---|---|
| Week view Monday to Sunday. | Same. | Start week with current `mondayOf` behavior. |
| Toolbar: Calendar title, previous week, Today, next week, week range. | Same. | Preserve formatting. |
| Scrolls to around 8am on mount. | Same. | Native scroll support required. |
| All-day event row when all-day events exist. | Same. | Event chips selectable. |
| Timed grid with 24 hours and event chips by start/end. | Same. | Same color hashing by calendar id or Fluent equivalent. |
| Event selection toggles detail panel. | Same. | Selected event from search should open detail. |
| Detail shows title, time range, location, meet link, description, attendees/status, linked emails. | Same. | Join meeting opens external URL. |
| Linked emails can navigate to message and unlink. | Same. | Current UI does not expose add-link control; do not remove existing list/unlink. |
| Calendar events appear in search suggestions/results. | Same. | Sidebar suggestion click opens calendar and event. |

### Backend and Data Features

| Current behavior | Native target | Notes |
|---|---|---|
| Accounts: list, create, update, delete, settings, trigger sync. | Same. | Covered by native forms/settings. |
| Folders: list, exclude/include, mark read, list messages. | Same. | Covered by sidebar/settings/list. |
| Messages: read body, mark read/unread, flag/unflag, delete, archive, bulk actions. | Same. | Covered by list/pane/palette. |
| Labels: list, show message labels, add/remove labels, label messages. | Same. | Current full label CRUD API remains unused unless scoped. |
| Snooze/unsnooze. | Same for snooze. | Current UI exposes snooze, not unsnooze action except via smart folder visibility. Evaluate whether native should expose unsnooze if current UX makes it hard. |
| Rules engine. | Same. | Preserve rule editor. |
| Search and suggestions. | Same. | Preserve message and calendar search. |
| Webhooks. | Same. | Preserve settings UI. |
| SSE sync events. | Same. | Preserve sidebar progress and refresh behavior. |
| OAuth callbacks. | Same. | Native browser open flow must handle success. |

## Known Product Gaps To Resolve Before Cutover

These are not necessarily caused by FluentGUI, but they must be clarified so the migration does not hide them:

- README says "reply-all"; current desktop UI exposes Reply and Forward, not a Reply All command. Decide whether to add Reply All during migration or update docs before cutover.
- README says Google Calendar read + write; current desktop UI exposes week/detail viewing and linked email navigation, not event create/edit/delete screens. Decide whether write UI is required for desktop parity.
- Attachments are only indicated in the current reading pane. There is no visible attachment download/open UI. Do not claim more unless implemented.
- The backend has label create/update/delete APIs, but current desktop UI only displays labels and adds/removes labels on messages. Full label management is not current UI parity.
- Native HTML email rendering is a blocking risk. A plain-text-only fallback is not acceptable as the default cutover behavior unless explicitly accepted.

## Migration Phases

### Phase 0 - Baseline Audit and Fixtures

Deliverables:

- Create a feature tracker from this document with `Current`, `Native`, `Verified`, and `Notes` columns.
- Build a deterministic seed database with:
  - no accounts;
  - one Gmail-like account;
  - two accounts;
  - special folders and nested folders;
  - unread, flagged, snoozed, threaded, attachment, HTML, text-only, and label-tagged messages;
  - calendar events: all-day, timed, with location, meet link, attendees, linked emails;
  - rules and webhooks.
- Capture screenshots or structured observations of the current React desktop UI for every major state.
- List all current keyboard shortcuts and context menus.

Acceptance:

- The tracker has every row from the feature matrix.
- Seed data can run both current web UI and new native UI.
- Any README-vs-UI mismatch is logged as a product gap.

### Phase 1 - Backend/API Prep

Deliverables:

- Extract shared DTOs into `email-types`.
- Add `email-client` with typed methods for all UI-used endpoints.
- Add tests for API client URL construction, JSON casing, errors, and search condition params.
- Refactor `email-server` startup enough that a native app can start the server without duplicating `main.rs`.

Acceptance:

- Existing `cargo test -p email-server` still passes.
- Existing frontend build still works.
- Native client can list accounts, folders, messages, labels, events, rules, and webhooks against a running server.

### Phase 2 - Native App Skeleton

Deliverables:

- Add the FluentGUI desktop crate/binary with path dependencies to FluentGUI.
- Start Axum in-process, wait for readiness, then open a FluentGUI window.
- Add app state entity: theme, density, selected view, selected folder, selected label, selected message, selected event, messages, accounts, labels, folders, compose state, modal states, palette state.
- Add light/dark theme bridge and density tokens.
- Add root workspace with sidebar placeholder, mail placeholder, calendar placeholder, modal host, command palette host.

Acceptance:

- `cargo run` opens a native window and loads account state.
- Headless mode still runs the server only.
- Closing native app shuts down cleanly or hides to tray on Windows once lifecycle work lands.

### Phase 3 - FluentGUI Generic Extensions

Deliverables:

- Implement the generic extensions required for parity, starting with:
  1. resizable split panels;
  2. enhanced command palette;
  3. token/chip input and anchored suggestions;
  4. async resource/loading helper;
  5. date/time picker;
  6. rich document viewer;
  7. app lifecycle hooks for single-instance/tray/title updates.
- Add FluentGUI examples/tests for each extension.

Acceptance:

- FluentGUI workspace checks pass after extensions.
- Each extension is documented as generic and has no email-rs imports.
- email-rs can consume each extension without local forks beyond the target FluentGUI path.

### Phase 4 - Accounts, Sidebar, and Navigation

Deliverables:

- First-run account setup.
- Add account modal.
- OAuth open-in-browser flow and success refresh.
- Sidebar compose/search/advanced search/smart folders/calendar/labels/accounts/folders/footer.
- SSE sync progress strip.
- Persistent widths, theme, and density.

Acceptance:

- A new user can add Gmail/Microsoft/manual accounts from native UI.
- Existing accounts/folders/labels display correctly.
- Search suggestions navigate to messages/events.
- Sidebar context menus work.

### Phase 5 - Mail List, Reading Pane, Compose

Deliverables:

- Message list with all current data modes: folder, smart, label, basic search, advanced search.
- Thread grouping and expansion.
- Bulk selection/actions and mark-all-read.
- Row context menu and keyboard shortcuts.
- Reading pane with message actions, labels, snooze, body rendering, quick reply, context menu.
- Compose pane with recipient suggestions, Cc/Bcc, signature, quote/forward bodies, send/discard.

Acceptance:

- Every message-list, reading-pane, and compose row in the parity matrix is checked.
- HTML and text-only body fixtures render acceptably.
- Keyboard shortcut smoke test passes with focus in list, pane, and text inputs.

### Phase 6 - Settings, Rules, Webhooks, Advanced Search

Deliverables:

- Settings modal/nav with General, per-account, rules, and webhooks.
- Account update/delete, sync days, signature, excluded folders.
- Rule list/editor using native condition builder.
- Webhook list/add/toggle/delete.
- Advanced search modal with all fields/operators.

Acceptance:

- Settings can reproduce every current mutation.
- Rule create/edit/delete/toggle works against seed data.
- Webhook add/toggle/delete works against seed data.
- Advanced search URLs match existing frontend behavior.

### Phase 7 - Calendar

Deliverables:

- Native week grid with all-day/timed events.
- Week navigation and Today.
- Event detail panel.
- Join meeting external link.
- Linked emails list, navigate, and unlink.
- Search suggestion event navigation.

Acceptance:

- Calendar fixtures match current view behavior.
- Linked email navigation returns to mail view with correct folder/message selected.

### Phase 8 - Desktop Shell and Packaging

Deliverables:

- Windows single-instance behavior.
- Windows tray Open/Quit and close-to-tray.
- Window icon, min size, title/unread count updates.
- Linux display/headless behavior.
- Remove obsolete WebKit/Wry desktop dependencies only after native replacement is stable.
- Update native build docs and CI/release packaging.

Acceptance:

- Windows behavior matches existing desktop app.
- Linux native build no longer depends on WebKitGTK for desktop mode.
- Browser/headless/Docker behavior remains documented and working.

### Phase 9 - Cutover

Deliverables:

- Make FluentGUI desktop the default desktop feature.
- Keep old WebView desktop behind a temporary fallback feature for one release if needed.
- Update README and release notes with exact behavior changes.
- Close every parity tracker row or explicitly mark it as a product decision.

Acceptance:

- No blocking feature parity rows remain open.
- Existing tests plus native smoke tests pass on Linux and Windows.
- Manual walkthrough using seed DB and a real account completes without missing current functionality.

## Verification Plan

### Automated

- `cargo fmt --all --check`
- `cargo test -p email-server`
- `cargo test -p email-client`
- `cargo check -p email-desktop-fluent`
- FluentGUI checks after framework extensions:
  - `cargo fmt --all --check`
  - `cargo test --workspace`
  - `cargo check --workspace`
- Frontend remains valid while retained:
  - `cd frontend && npm run build`

### Native UI Smoke Tests

Use seed DB plus scripted/manual checks:

- First-run account setup renders.
- Sidebar folder selection changes list.
- Basic search and advanced search return expected messages.
- Message open marks read.
- Archive/delete/read/flag/snooze mutate backend.
- Bulk actions mutate backend.
- Compose sends or reports SMTP/auth error.
- Settings updates account name, sync days, signature, folder exclusion.
- Rule create/edit/toggle/delete works.
- Webhook add/toggle/delete works.
- Calendar event detail opens; linked email navigation works.
- Ctrl+K can run every command category.
- Theme and density persist.
- Window title/tray unread count updates.

### Manual Real-Service Checks

- Gmail OAuth and sync.
- Microsoft OAuth if credentials are available.
- Microsoft app-password account.
- Generic IMAP/SMTP account.
- HTML-heavy email, plain-text email, and email with attachments.
- Calendar event with meet link and attendees.
- Windows install/run/single-instance/tray.
- Linux display/headless startup.

## Cutover Checklist

Before removing or demoting the current WebView desktop path:

- [ ] All feature matrix rows are implemented and verified.
- [ ] FluentGUI generic extensions have examples/tests and no email-specific code.
- [ ] HTML mail rendering has been accepted against real and fixture messages.
- [ ] Windows single-instance/tray behavior is restored.
- [ ] OAuth browser flow works from native UI.
- [ ] Browser/headless web mode still works.
- [ ] README feature claims match actual desktop behavior.
- [ ] Release notes call out the GUI framework swap and any intentional UX changes.
