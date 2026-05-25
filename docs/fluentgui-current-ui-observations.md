# FluentGUI Current UI Observations

Last updated: 2026-05-25

These are structured observations of the current React/WebView desktop UI. They
are the Phase 0 baseline until native screenshots are added.

## Startup And Shell

- Linux desktop mode starts Axum in-process, waits for the server, then opens a
  WebKitGTK window at `1280x800` titled `email-rs` when a display is available.
- Linux headless mode runs Axum plus static files only when no display exists or
  `EMAIL_RS_HEADLESS` is set.
- Windows starts Axum on a background runtime, enforces a named mutex single
  instance, opens a Wry/WebView2 window at `1280x800` with `800x600` minimum
  size, and hides to tray on close.
- The browser tab title is `email-rs` or `(<unread>) email-rs` based on stored
  folder unread counts.

## First Run And Accounts

- While accounts load, the app renders no content.
- With no accounts, the first-run account setup screen is the whole app surface.
- Account setup offers Gmail OAuth, Microsoft OAuth, Microsoft app-password, and
  manual IMAP/SMTP forms.
- Microsoft app-password defaults are `outlook.office365.com` for IMAP and
  `smtp.office365.com` for SMTP. The backend uses the same IMAP default when
  no host is supplied.
- Add account from the sidebar opens the same provider choices in a modal.

## Main Workspace

- The normal mail view is a 3-pane layout: sidebar, message list, reading pane.
- Sidebar and message-list widths are draggable and persisted in localStorage
  under `email_sidebar_w` and `email_msglist_w`.
- Calendar view keeps the sidebar and replaces the message list plus reading
  pane with the calendar workspace.
- Theme and density are persisted through the Zustand store under
  `email-rs-app`, with only `theme` and `densityLevel` persisted.

## Sidebar

- The sidebar includes Compose, sync progress, mail/calendar search, advanced
  search, smart folders, calendar, labels, account folders, account list, add
  account, density controls, theme toggle, and settings.
- Search suggestions debounce at 150 ms and combine message suggestions with up
  to three calendar event suggestions.
- Selecting a message suggestion navigates to the message's folder and message.
  Selecting an event suggestion switches to calendar and selects the event.
- Labels are fetched only for the first account in the sidebar.
- Folder indentation strips a leading `[Gmail]/` before computing nested depth.

## Message List

- The list can load regular folders, smart folders, label folders, basic search,
  and advanced search result URLs.
- Basic search result fetching is debounced at 300 ms.
- Thread grouping applies only to regular folder view and uses `threadId` or
  message id, with latest message first.
- Bulk selection replaces the normal header with Archive, Mark Read, Mark
  Unread, Delete, and cancel controls. Delete shows a browser confirmation.
- Rows show avatar/checkbox, sender, timestamp, subject, preview, unread state,
  attachment indicator, flag indicator, snooze indicator when supplied, and
  thread count.
- The frontend expects `snoozedUntil`, but the current backend message row DTO
  does not serialize it.

## Reading Pane

- Empty state says `Select a message to read`.
- Opening a message calls `GET /api/v1/messages/{id}`, marks it read locally,
  and attempts provider mark-seen in the backend when provider data permits it.
- Toolbar actions are Reply, Forward, Read/Unread, Snooze, Archive, and Delete.
- Snooze choices are later today plus 3 hours, tomorrow 9am, next Monday 9am,
  and a custom `datetime-local` value.
- Labels on a message render as removable chips, with an add-label dropdown for
  labels not already on the message.
- HTML mail renders in a sandboxed iframe with a light/dark background style
  injection. Text-only mail renders in a `pre` fallback.
- Attachment UI is only an `attachment` indicator; no current download/open
  control exists.
- Quick reply is a textarea plus Send button and sends text only.

## Compose

- Compose replaces the reading pane rather than opening a detached window.
- Modes are new message, reply, and forward.
- Fields are To, optional Cc/Bcc, Subject, and body.
- Recipients use the shared contact suggestion input and are comma-separated in
  the compose state before POSTing arrays.
- Account signature is fetched from account settings and appended on send.
- Reply includes a quoted block on send. Forward pre-populates the body with a
  forwarded-message header and quoted text.

## Search

- Basic search sends `GET /api/v1/search?q=...` and uses message results for the
  message list while event results appear in suggestions.
- Advanced search modal uses the shared condition builder and stores a condition
  group in app state.
- Advanced search URL construction emits `match`, `q`, per-field values, and
  text operator params, but the current backend search handler parses only `q`
  and optional `account_id`.

## Settings

- Settings are modal with left navigation: General, each account, Rules per
  account, and Webhooks.
- General settings expose theme select and density stepper levels `0..8`.
- Account settings expose display name, read-only email/provider, sync days
  limit, signature, excluded folders, and a delete confirmation flow.
- Folder exclusion toggles mutate the folder object optimistically after a PATCH.
- Rules tab lists, toggles, edits, deletes, and creates account rules using the
  condition builder and action rows.
- Webhooks tab lists webhooks, toggles enabled state, deletes webhooks, and adds
  URL/secret/events entries.

## Command Palette

- Ctrl/Cmd+K opens a modal palette with grouped actions.
- The palette groups Actions, Navigate, Labels, Folders, and Message actions.
- Fuzzy matching scores exact, prefix, substring, keyword, and ordered-character
  matches, capped to 14 visible results.
- Message actions only appear when a message is selected.

## Calendar

- Calendar is a Monday-to-Sunday week view.
- Toolbar includes title, previous week, Today, next week, and week range.
- The view scrolls to around 8am on mount.
- All-day events render in a separate row when present.
- Timed events render in a 24-hour grid and are selectable.
- Event detail shows title, time, location, meet link, description, attendees,
  and linked emails.
- Linked emails navigate back to mail. Existing links can be removed.
