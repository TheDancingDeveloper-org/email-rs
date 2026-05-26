# FluentGUI Feature Tracker

Last updated: 2026-05-26

Status values:

- `Current audited`: React/WebView behavior has a code or structured-observation baseline.
- `Product gap`: Current UI or README claim needs a product decision before cutover.
- `Pending native`: Native FluentGUI implementation has not started.
- `Manual required`: Requires OS, credential, or real-service verification.

## Desktop Shell

| Current | Native | Verified | Notes |
|---|---|---|---|
| Linux opens a WebKitGTK window automatically when display exists. | Linux opens a FluentGUI window automatically when display exists. | Current audited; pending native. | Keep display detection and readiness wait. |
| Linux falls back to browser/server mode when headless. | Same. | Current audited; pending native. | Preserve `EMAIL_RS_HEADLESS=1`. |
| Windows uses Wry/WebView2. | Windows uses FluentGUI/GPUI. | Manual required. | Remove WebView2 only after parity. |
| Windows enforces single instance and foregrounds existing window. | Same through FluentGUI lifecycle extension. | Manual required. | Blocking cutover item. |
| Windows close button hides to tray, tray has Open and Quit. | Same. | Manual required. | Blocking cutover item. |
| Window title is `email-rs`; browser title shows unread count. | Native title/status shows unread count. | Native implemented; automated tests. | Tray unread count remains tied to platform tray work. |
| Default size 1280x800, Windows min 800x600, icon set. | Same window size/min/icon. | Current audited; pending native. | Needs FluentGUI min-size/icon support. |
| Snap/WebKit-specific Linux environment workarounds. | Re-evaluate after GPUI. | Pending native. | Keep only if still needed for packages. |

## Startup And Account Setup

| Current | Native | Verified | Notes |
|---|---|---|---|
| Blank while accounts load. | Native loading state or brief skeleton. | Current audited; pending native. | Native should improve clarity without changing behavior. |
| First-run account setup screen. | Native setup modal shown automatically when no accounts exist. | Native implemented; automated tests. | Request builders and first-run reload/close behavior covered. |
| Gmail OAuth start. | Fetch authorize URL and open system browser. | Manual required; native plumbing implemented. | Requires credentials; native exposes account refresh after browser callback. |
| Microsoft 365 OAuth start. | Same. | Manual required; native plumbing implemented. | Requires credentials; native exposes account refresh after browser callback. |
| Microsoft app-password fallback. | Native form with Outlook/SMTP defaults. | Native implemented; automated tests. | Defaults are `outlook.office365.com` and `smtp.office365.com`. |
| Manual IMAP/SMTP form. | Native form with same fields and same-password toggle. | Native implemented; automated tests. | Fixture accounts avoid live service calls. |
| Add account modal from sidebar. | Native modal. | Native implemented; cargo check. | Opens from sidebar and command palette; successful create reloads accounts. |

## Main Layout And Navigation

| Current | Native | Verified | Notes |
|---|---|---|---|
| 3-pane mail layout: sidebar, message list, reading pane. | FluentGUI workspace with resizable sidebar and message-list split. | Current audited; pending native. | Requires generic split panels. |
| Calendar replaces message list plus reading pane while sidebar remains. | Same view switch. | Native implemented; cargo check. | Calendar route owns the main content while sidebar remains mounted. |
| Persist theme and density. | Persist native theme and density. | Native implemented; automated tests. | Native settings file persists these keys. |
| Sidebar width and message-list width persist in localStorage. | Persist in native settings. | Current audited; pending native. | Current keys are `email_sidebar_w` and `email_msglist_w`. |
| Density controls from sidebar footer and settings. | Same. | Native implemented; cargo check. | Levels `0..8`; palette density actions are also wired. |
| Light/dark toggle. | Same through FluentGUI theme. | Native implemented; cargo check. | Theme toggle persists through native preferences. |

## Sidebar

| Current | Native | Verified | Notes |
|---|---|---|---|
| Compose button opens new compose for first account. | Same. | Native implemented; cargo check. | Uses the first account and surfaces an error when no account is available. |
| Sync progress strip from SSE. | Same. | Native partial; automated tests. | Native SSE parser/status strip implemented; hide-after-completion timing still needs manual parity check. |
| Search mail and calendar with 150 ms suggestions. | Same. | Native implemented; automated tests. | Native suggestions fetch messages and events after the existing delay; submitted search also preserves event results. |
| Search suggestion keyboard Up/Down/Enter/Escape. | Same. | Native implemented; automated tests. | Active suggestion selection is tracked separately from Enter without an active suggestion. |
| Advanced search filter button state. | Same. | Native implemented; automated tests. | Opens native modal; active state follows advanced search state. |
| Smart folders All Inboxes, Unread, Flagged, Snoozed. | Same. | Native implemented; cargo check. | API IDs are `all`, `unread`, `flagged`, `snoozed`. |
| Calendar nav item. | Same. | Native implemented; cargo check. | Switches to calendar and loads the active week. |
| Labels section with color dots. | Same. | Native implemented; automated tests. | Native label rows render parsed label colors with accent fallback. |
| Per-account collapsible sections. | Same. | Native implemented; cargo check. | Account rows preserve unread badges and folder groups. |
| Special folders ordered inbox, sent, drafts, archive, trash, spam. | Same. | Native implemented; cargo check. | Backend list orders by `special_use`; native renders ordered folder rows. |
| Other folders under collapsible Folders group with indentation. | Same. | Native implemented; cargo check. | Native preserves folder path indentation. |
| Folder context menu: open, mark all read. | Same. | Native implemented; cargo check. | Regular folders expose mark-read; smart folders stay navigation-only. |
| Account list at bottom with avatars, email, Add account. | Same. | Native implemented; cargo check. | Native Add account opens the setup modal; avatar/email polish still tracks with sidebar parity. |
| Footer density buttons, theme toggle, settings button. | Same. | Native implemented; cargo check. | Settings opens the native modal. |

## Message List

| Current | Native | Verified | Notes |
|---|---|---|---|
| Fetch folder, label, smart-folder, search, and advanced-search result messages. | Same through `email-client`. | Native implemented; automated tests. | Advanced search source wired; backend gap logged. |
| Debounced search results. | Same. | Current audited; pending native. | Current debounce is 300 ms. |
| Header shows folder/search label, unread count, search count. | Same. | Native implemented; cargo check. | Native header uses current mailbox title and visible unread/result counts. |
| Select-all checkbox with indeterminate state. | Same. | Native implemented; automated tests. | Selection helper covers checked/unchecked/indeterminate states. |
| Bulk bar with Archive, Mark Read, Mark Unread, Delete, cancel. | Same. | Native implemented; cargo check. | Bulk actions reuse typed message mutation refresh. |
| Mark all read for regular folders. | Same. | Native implemented; cargo check. | Hidden for smart/label/search/advanced search. |
| Empty/loading states. | Same. | Native implemented; cargo check. | Native status covers no folder, loading, errors, empty search, and empty views. |
| Thread grouping only in regular folder view. | Same. | Native implemented; automated tests. | Native groups by `threadId` or message id, renders latest first, and keeps search/smart/label views flat. |
| Rows show avatar/checkbox, sender, timestamp, subject, preview, unread dot, attachment, flag, snooze, thread count. | Same. | Native implemented; cargo check. | Native rows render initials avatar, checkbox, sender/date, subject/preview, unread accent, attachment/flag/snooze badges, and thread count. |
| Row context menu has open, reply, forward, archive, trash, read/unread, flag/unflag. | Same. | Native implemented; cargo check. | Reply/forward select and start the draft path. |
| Keyboard j/k, Ctrl/Cmd+A, Escape, x, e, Delete/Backspace, u. | Same. | Native implemented; automated tests. | Message-list shortcut helper covered. |

## Reading Pane

| Current | Native | Verified | Notes |
|---|---|---|---|
| Empty state: select a message. | Same. | Native implemented; cargo check. | Native empty text is `Select a message to read it.` |
| Loading and not found states. | Same. | Native implemented; cargo check. | Native shows body loading and load-error status messages. |
| Opening a message marks it read locally and attempts provider mark-seen. | Same backend call behavior. | Native implemented; automated tests. | Full-message load updates summary read state and unread counts. |
| Toolbar: Reply, Forward, Read/Unread, Snooze, Archive, Delete. | Same. | Native implemented; cargo check. | Native toolbar uses typed message mutation APIs. |
| Snooze presets: later today +3h, tomorrow 9am, next Monday 9am, custom datetime. | Same. | Native implemented; automated tests. | Native has preset buttons plus a custom `DateTimePicker`. |
| Message header: subject, flagged tag, labels, sender avatar/name/email, date. | Same. | Native implemented; cargo check. | Native header renders subject/sender/date, badges, and label row; avatar visual parity remains polish. |
| Label add dropdown lists labels not already on the message. | Same. | Native implemented; cargo check. | Uses typed label APIs and hides labels already on the message. |
| Header context menu: reply, forward, archive, delete, read/unread, copy sender email. | Same. | Native implemented; cargo check. | Right-click message header opens native menu; sender-email resolution is covered. |
| Attachment indicator. | Same current level. | Current audited; product gap. | No current attachment download/open UI. |
| HTML body shown in sandboxed iframe when available; text fallback otherwise. | Native rich document viewer with safe rendering and text fallback. | Native implemented; automated tests. | Native uses a lossy safe `RichDocument` renderer; manual visual fidelity pass remains. |
| Quick reply textarea, send button, sending/error states. | Same. | Native implemented; automated tests. | Sends via `/api/v1/messages`; request defaults and errors covered. |
| Shortcuts e, #, u, Escape. | Same. | Native implemented; automated tests. | Ignored while quick-reply inputs or snooze picker are focused. |

## Compose

| Current | Native | Verified | Notes |
|---|---|---|---|
| Compose replaces reading pane when open. | Same. | Native implemented; cargo check. | No detached window currently. |
| Modes: new, reply, forward. | Same. | Native implemented; automated tests. | Mode tag displayed; reply/forward payload behavior covered. |
| Fields: To, optional Cc/Bcc, Subject, body. | Same. | Native implemented; cargo check. | Recipient token inputs and multiline body editor are wired. |
| Recipient autocomplete from message/contact history. | Same. | Native implemented; automated tests. | Native suggestions merge server-backed contact suggestions with visible message senders and reject stale responses. |
| Account signature appended on send and previewed in editor. | Same. | Native implemented; automated tests. | Preserves `--` separator and waits for signature load. |
| Reply includes quoted block on send and quote preview. | Same. | Native implemented; automated tests. | Quote source is text body. |
| Forward pre-fills forwarded message body. | Same. | Native implemented; automated tests. | Forward omits `In-Reply-To` and preserves forwarded body seed. |
| Send disabled until recipient and body exist; shows sending/error; close on success. | Same. | Native implemented; automated tests. | Network send path updates native sending/error/notice state. |
| Discard closes compose. | Same. | Native implemented; cargo check. | Optional confirm not part of parity. |

## Search And Advanced Search

| Current | Native | Verified | Notes |
|---|---|---|---|
| Basic search from sidebar searches messages and calendar, with suggestions. | Same. | Native implemented; automated tests. | Message suggestions open the folder/message; event suggestions/results navigate to Calendar. |
| Advanced search modal with condition builder. | Same. | Native implemented; automated tests. | Native modal uses the existing condition model. |
| Match all/any. | Same. | Backend implemented; native UI wired. | Search backend parses `match=all/any` for structured message filters. |
| Text operators contains, not_contains, equals, starts_with. | Same. | Backend implemented; native UI wired. | Search backend parses field operator params for message filters. |
| Boolean/date controls. | Same. | Backend implemented; native UI wired. | Search backend parses boolean/date params for message filters. |
| Clear, Cancel, Search, Enter/Escape behavior. | Same. | Native implemented; pending keyboard polish. | Search disabled when no active values. |

## Settings

| Current | Native | Verified | Notes |
|---|---|---|---|
| Settings modal tabs: General, each account, Rules per account, Webhooks. | Native modal/nav sections. | Native implemented; cargo check. | General/account/per-account Rules/Webhooks tabs implemented. |
| General theme select. | Same. | Native implemented; cargo check. | Native uses buttons for Light/Dark. |
| General density stepper with labels. | Same. | Native implemented; cargo check. | Levels `0..8`. |
| Account info display name, email, provider. | Same. | Native implemented; automated tests. | Email/provider read-only; name request is validated and trimmed. |
| Sync setting: download last N days or all. | Same. | Native implemented; automated tests. | PATCH account; `null` clear semantics fixed for all/no limit. |
| Signature textarea. | Same. | Native implemented; automated tests. | Blank signature clears the account signature. |
| Excluded folders checkbox list. | Same. | Native implemented; cargo check. | Patches folder exclusion and refreshes account tree. |
| Remove account danger zone with confirmation. | Same. | Native implemented; automated tests. | Deletes through account API, reloads native account state, and closes Settings. |
| Save status. | Same. | Native implemented; cargo check. | Account save and folder-update success/error feedback. |
| Rules list with enable toggle, name, counts, edit, delete, new. | Same. | Native implemented; automated tests. | Per-account list, enable toggle, counts, edit, delete, and new implemented. |
| Rule editor with name, condition builder, actions, folder selector, save/cancel/error. | Same. | Native implemented; automated tests. | Create/update payloads, validation, save/cancel, and error status implemented in Phase 6f. |
| Webhooks list, enabled toggle, signed indicator, delete. | Same. | Native implemented; cargo check. | Loads through typed API and preserves event/signed display. |
| Add webhook: URL, secret, events select, add/error. | Same. | Native implemented; automated tests. | Uses same payload; event control supports `new_message` and `all`. |

## Command Palette

| Current | Native | Verified | Notes |
|---|---|---|---|
| Ctrl/Cmd+K opens/closes palette. | Same. | Native implemented; automated tests. | Native app binds both Ctrl+K and Cmd+K to the palette action. |
| Groups: Actions, Navigate, Labels, Folders, Message. | Same. | Native implemented; FluentGUI tests. | Shared palette renders group headers; native entries populate action, navigate, label, folder, account, and message groups. |
| Fuzzy matching with label/keyword scoring. | Same or better. | Native implemented; FluentGUI tests. | Shared palette scores label, subtitle, group, and keywords with stable ordering. |
| Actions: New Message, Add Account, Advanced Search, Settings, theme, density. | Same. | Native implemented; cargo check. | New Message, Add Account, Advanced Search, Settings, theme, and density wired. |
| Navigation: Mail, Calendar, smart folders, labels, folders. | Same. | Native implemented; automated tests. | Smart folders, labels, folders, and account inbox entries call native navigation. |
| Message actions: reply, forward, archive, delete, read/unread, flag/unflag. | Same. | Native implemented; automated tests. | Only for selected message; actions reuse native message mutation refresh. |
| Keyboard Up/Down, Enter, Escape. | Same. | Native implemented; manual required. | Shared palette handles keys and mouse selection; running-app walkthrough still pending. |

## Calendar

| Current | Native | Verified | Notes |
|---|---|---|---|
| Week view Monday to Sunday. | Same. | Native implemented; automated tests. | Native week helpers preserve Monday start. |
| Toolbar title, previous week, Today, next week, week range. | Same. | Native implemented; automated tests. | Native week range formatting covered. |
| Scrolls to around 8am on mount. | Same. | Current audited; pending native. | Needs native scroll support. |
| All-day event row when all-day events exist. | Same. | Native implemented; cargo check. | Native row renders per-day all-day chips. |
| Timed grid with 24 hours and event chips. | Same. | Native implemented; cargo check. | Native grid groups chips by local day and start hour. |
| Event selection toggles detail panel. | Same. | Native implemented; automated tests. | Calendar search-result navigation selects the matching event week. |
| Detail shows title, time, location, meet link, description, attendees, linked emails. | Same. | Native implemented; cargo check. | Linked emails load for the selected event and stay scoped to current selection. |
| Linked emails can navigate to message and unlink. | Same. | Native implemented; automated tests. | Current UI has no add-link control; native supports open-message and unlink flows. |
| Calendar events appear in search suggestions/results. | Same. | Native implemented; automated tests. | Native suggestions and submitted search result rows both include calendar events. |

## Backend And Data Features

| Current | Native | Verified | Notes |
|---|---|---|---|
| Accounts list, create, update, delete, settings, trigger sync. | Same. | Current audited; pending native. | Covered by forms/settings. |
| Folders list, exclude/include, mark read, list messages. | Same. | Current audited; pending native. | Folder counters are stored. |
| Messages read body, read/unread, flag/unflag, delete, archive, bulk actions. | Same. | Current audited; pending native. | Fixture includes action targets. |
| Labels list, show message labels, add/remove labels, label messages. | Same. | Current audited; pending native. | Full label CRUD is not current UI parity. |
| Snooze/unsnooze. | Same for snooze. | Current audited; pending native. | Unsnooze UI not currently exposed. |
| Rules engine. | Same. | Current audited; pending native. | Fixture includes active and inactive rules. |
| Search and suggestions. | Same. | Backend implemented; pending remaining native polish. | Structured message params are parsed; native submitted search preserves calendar events. |
| Webhooks. | Same. | Current audited; pending native. | Fixture includes signed/enabled and disabled rows. |
| SSE sync events. | Same. | Current audited; pending native. | Preserve sidebar progress. |
| OAuth callbacks. | Same. | Manual required. | Requires real provider credentials. |
