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
| Window title is `email-rs`; browser title shows unread count. | Native title/tray/status shows unread count. | Current audited; pending native. | Browser title remains for web mode. |
| Default size 1280x800, Windows min 800x600, icon set. | Same window size/min/icon. | Current audited; pending native. | Needs FluentGUI min-size/icon support. |
| Snap/WebKit-specific Linux environment workarounds. | Re-evaluate after GPUI. | Pending native. | Keep only if still needed for packages. |

## Startup And Account Setup

| Current | Native | Verified | Notes |
|---|---|---|---|
| Blank while accounts load. | Native loading state or brief skeleton. | Current audited; pending native. | Native should improve clarity without changing behavior. |
| First-run account setup screen. | Native setup screen. | Current audited; pending native. | Covered by no-account fixture. |
| Gmail OAuth start. | Fetch authorize URL and open system browser. | Manual required. | Requires credentials. |
| Microsoft 365 OAuth start. | Same. | Manual required. | Requires credentials. |
| Microsoft app-password fallback. | Native form with Outlook/SMTP defaults. | Current audited; pending native. | Defaults are `outlook.office365.com` and `smtp.office365.com`. |
| Manual IMAP/SMTP form. | Native form with same fields and same-password toggle. | Current audited; pending native. | Fixture accounts avoid live service calls. |
| Add account modal from sidebar. | Native modal. | Current audited; pending native. | Same success refresh behavior. |

## Main Layout And Navigation

| Current | Native | Verified | Notes |
|---|---|---|---|
| 3-pane mail layout: sidebar, message list, reading pane. | FluentGUI workspace with resizable sidebar and message-list split. | Current audited; pending native. | Requires generic split panels. |
| Calendar replaces message list plus reading pane while sidebar remains. | Same view switch. | Current audited; pending native. | Preserve selected event state. |
| Persist theme and density. | Persist native theme and density. | Current audited; pending native. | Current store persists only these keys. |
| Sidebar width and message-list width persist in localStorage. | Persist in native settings. | Current audited; pending native. | Current keys are `email_sidebar_w` and `email_msglist_w`. |
| Density controls from sidebar footer and settings. | Same. | Current audited; pending native. | Levels `0..8`. |
| Light/dark toggle. | Same through FluentGUI theme. | Current audited; pending native. | Verify custom views subscribe to theme. |

## Sidebar

| Current | Native | Verified | Notes |
|---|---|---|---|
| Compose button opens new compose for first account. | Same. | Current audited; pending native. | Disabled/no-op when no account. |
| Sync progress strip from SSE. | Same. | Current audited; pending native. | Uses `/api/v1/events`, completion hides after 3s. |
| Search mail and calendar with 150 ms suggestions. | Same. | Current audited; pending native. | Message plus event suggestions. |
| Search suggestion keyboard Up/Down/Enter/Escape. | Same. | Current audited; pending native. | Enter without active suggestion keeps full search. |
| Advanced search filter button state. | Same. | Native implemented; automated tests. | Opens native modal; active state follows advanced search state. |
| Smart folders All Inboxes, Unread, Flagged, Snoozed. | Same. | Current audited; pending native. | API IDs are `all`, `unread`, `flagged`, `snoozed`. |
| Calendar nav item. | Same. | Current audited; pending native. | Switches to calendar. |
| Labels section with color dots. | Same. | Current audited; pending native. | Labels currently fetched only for first account. |
| Per-account collapsible sections. | Same. | Current audited; pending native. | Preserve account unread badge. |
| Special folders ordered inbox, sent, drafts, archive, trash, spam. | Same. | Current audited; pending native. | Backend list orders by `special_use`. |
| Other folders under collapsible Folders group with indentation. | Same. | Current audited; pending native. | Preserve `[Gmail]/` depth behavior. |
| Folder context menu: open, mark all read. | Same. | Current audited; pending native. | Smart folders expose only Open folder. |
| Account list at bottom with avatars, email, Add account. | Same. | Current audited; pending native. | Multiple accounts covered by fixture. |
| Footer density buttons, theme toggle, settings button. | Same. | Native implemented; cargo check. | Settings opens the native modal. |

## Message List

| Current | Native | Verified | Notes |
|---|---|---|---|
| Fetch folder, label, smart-folder, search, and advanced-search result messages. | Same through `email-client`. | Native implemented; automated tests. | Advanced search source wired; backend gap logged. |
| Debounced search results. | Same. | Current audited; pending native. | Current debounce is 300 ms. |
| Header shows folder/search label, unread count, search count. | Same. | Current audited; pending native. | Preserve exact labels where possible. |
| Select-all checkbox with indeterminate state. | Same. | Current audited; pending native. | Needed for bulk actions. |
| Bulk bar with Archive, Mark Read, Mark Unread, Delete, cancel. | Same. | Current audited; pending native. | Delete confirmation remains. |
| Mark all read for regular folders. | Same. | Current audited; pending native. | Hidden for smart/label/search/advanced search. |
| Empty/loading states. | Same. | Current audited; pending native. | Preserve `No folder selected`, `Loading...`, `No messages`. |
| Thread grouping only in regular folder view. | Same. | Current audited; pending native. | Groups by `threadId` or message id; latest first. |
| Rows show avatar/checkbox, sender, timestamp, subject, preview, unread dot, attachment, flag, snooze, thread count. | Same. | Product gap; pending native. | Backend does not currently serialize `snoozedUntil` in row DTO. |
| Row context menu has open, reply, forward, archive, trash, read/unread, flag/unflag. | Same. | Current audited; pending native. | Reply/forward currently select the message. |
| Keyboard j/k, Ctrl/Cmd+A, Escape, x, e, Delete/Backspace, u. | Same. | Current audited; pending native. | Blocking parity item. |

## Reading Pane

| Current | Native | Verified | Notes |
|---|---|---|---|
| Empty state: select a message. | Same. | Current audited; pending native. | Text is `Select a message to read`. |
| Loading and not found states. | Same. | Current audited; pending native. | Text is `Loading...` and `Message not found`. |
| Opening a message marks it read locally and attempts provider mark-seen. | Same backend call behavior. | Current audited; pending native. | Fixture avoids live providers. |
| Toolbar: Reply, Forward, Read/Unread, Snooze, Archive, Delete. | Same. | Current audited; pending native. | Preserve tooltips/shortcuts. |
| Snooze presets: later today +3h, tomorrow 9am, next Monday 9am, custom datetime. | Same. | Current audited; pending native. | Requires native date/time picker. |
| Message header: subject, flagged tag, labels, sender avatar/name/email, date. | Same. | Current audited; pending native. | Label chips removable. |
| Label add dropdown lists labels not already on the message. | Same. | Current audited; pending native. | Uses label APIs. |
| Header context menu: reply, forward, archive, delete, read/unread, copy sender email. | Same. | Current audited; pending native. | Requires clipboard support. |
| Attachment indicator. | Same current level. | Current audited; product gap. | No current attachment download/open UI. |
| HTML body shown in sandboxed iframe when available; text fallback otherwise. | Native rich document viewer with safe rendering and text fallback. | Current audited; pending native. | Largest migration risk. |
| Quick reply textarea, send button, sending/error states. | Same. | Current audited; pending native. | Sends via `/api/v1/messages`. |
| Shortcuts e, #, u, Escape. | Same. | Current audited; pending native. | Ignore while focus is in text inputs. |

## Compose

| Current | Native | Verified | Notes |
|---|---|---|---|
| Compose replaces reading pane when open. | Same. | Current audited; pending native. | No detached window currently. |
| Modes: new, reply, forward. | Same. | Current audited; pending native. | Mode tag displayed. |
| Fields: To, optional Cc/Bcc, Subject, body. | Same. | Current audited; pending native. | Recipient token input required. |
| Recipient autocomplete from message/contact history. | Same. | Current audited; pending native. | Uses `/api/v1/contacts/suggest`. |
| Account signature appended on send and previewed in editor. | Same. | Current audited; pending native. | Preserves `--` separator. |
| Reply includes quoted block on send and quote preview. | Same. | Current audited; pending native. | Quote source is text body. |
| Forward pre-fills forwarded message body. | Same. | Current audited; pending native. | Header text must be preserved. |
| Send disabled until recipient and body exist; shows sending/error; close on success. | Same. | Current audited; pending native. | Network error handling required. |
| Discard closes compose. | Same. | Current audited; pending native. | Optional confirm not part of parity. |

## Search And Advanced Search

| Current | Native | Verified | Notes |
|---|---|---|---|
| Basic search from sidebar searches messages and calendar, with suggestions. | Same. | Current audited; pending native. | Event suggestion navigates to calendar. |
| Advanced search modal with condition builder. | Same. | Native implemented; automated tests. | Native modal uses the existing condition model. |
| Match all/any. | Same. | Product gap; native UI wired. | Frontend and native emit `match`; backend currently ignores it. |
| Text operators contains, not_contains, equals, starts_with. | Same. | Product gap; native UI wired. | Frontend and native emit operator params; backend currently ignores them. |
| Boolean/date controls. | Same. | Product gap; native UI wired. | Backend support still needs implementation or decision. |
| Clear, Cancel, Search, Enter/Escape behavior. | Same. | Native implemented; pending keyboard polish. | Search disabled when no active values. |

## Settings

| Current | Native | Verified | Notes |
|---|---|---|---|
| Settings modal tabs: General, each account, Rules per account, Webhooks. | Native modal/nav sections. | Native partial; documented gaps. | General/account/Webhooks tabs implemented. Rules currently one placeholder, not per-account tabs. |
| General theme select. | Same. | Native implemented; cargo check. | Native uses buttons for Light/Dark. |
| General density stepper with labels. | Same. | Native implemented; cargo check. | Levels `0..8`. |
| Account info display name, email, provider. | Same. | Native implemented; automated tests. | Email/provider read-only; name request is validated and trimmed. |
| Sync setting: download last N days or all. | Same. | Native implemented; automated tests. | PATCH account; `null` clear semantics fixed for all/no limit. |
| Signature textarea. | Same. | Native implemented; automated tests. | Blank signature clears the account signature. |
| Excluded folders checkbox list. | Same. | Native implemented; cargo check. | Patches folder exclusion and refreshes account tree. |
| Remove account danger zone with confirmation. | Same. | Native implemented; automated tests. | Deletes through account API, reloads native account state, and closes Settings. |
| Save status. | Same. | Native implemented; cargo check. | Account save and folder-update success/error feedback. |
| Rules list with enable toggle, name, counts, edit, delete, new. | Same. | Current audited; pending native. | Documented Phase 6b gap; per account. |
| Rule editor with name, condition builder, actions, folder selector, save/cancel/error. | Same. | Current audited; pending native. | Documented Phase 6b gap. |
| Webhooks list, enabled toggle, signed indicator, delete. | Same. | Native implemented; cargo check. | Loads through typed API and preserves event/signed display. |
| Add webhook: URL, secret, events select, add/error. | Same. | Native implemented; automated tests. | Uses same payload; event control supports `new_message` and `all`. |

## Command Palette

| Current | Native | Verified | Notes |
|---|---|---|---|
| Ctrl/Cmd+K opens/closes palette. | Same. | Current audited; pending native. | Bind through native actions. |
| Groups: Actions, Navigate, Labels, Folders, Message. | Same. | Current audited; pending native. | Requires enhanced generic palette. |
| Fuzzy matching with label/keyword scoring. | Same or better. | Current audited; pending native. | Preserve predictable ordering. |
| Actions: New Message, Advanced Search, Settings, theme, density. | Same. | Native implemented; cargo check. | New Message, Advanced Search, Settings, theme, and density wired. |
| Navigation: Mail, Calendar, smart folders, labels, folders. | Same. | Current audited; pending native. | Folder hints include account/full path. |
| Message actions: reply, forward, archive, delete, read/unread, flag/unflag. | Same. | Current audited; pending native. | Only for selected message. |
| Keyboard Up/Down, Enter, Escape. | Same. | Current audited; pending native. | Mouse hover/select too. |

## Calendar

| Current | Native | Verified | Notes |
|---|---|---|---|
| Week view Monday to Sunday. | Same. | Current audited; pending native. | Preserve `mondayOf` behavior. |
| Toolbar title, previous week, Today, next week, week range. | Same. | Current audited; pending native. | Preserve formatting. |
| Scrolls to around 8am on mount. | Same. | Current audited; pending native. | Needs native scroll support. |
| All-day event row when all-day events exist. | Same. | Current audited; pending native. | Fixtures include all-day event. |
| Timed grid with 24 hours and event chips. | Same. | Current audited; pending native. | Fixtures include timed event. |
| Event selection toggles detail panel. | Same. | Current audited; pending native. | Search-selected event must open detail. |
| Detail shows title, time, location, meet link, description, attendees, linked emails. | Same. | Current audited; pending native. | Join meeting opens external URL. |
| Linked emails can navigate to message and unlink. | Same. | Current audited; pending native. | Current UI has no add-link control. |
| Calendar events appear in search suggestions/results. | Same. | Current audited; pending native. | Covered by fixture titles. |

## Backend And Data Features

| Current | Native | Verified | Notes |
|---|---|---|---|
| Accounts list, create, update, delete, settings, trigger sync. | Same. | Current audited; pending native. | Covered by forms/settings. |
| Folders list, exclude/include, mark read, list messages. | Same. | Current audited; pending native. | Folder counters are stored. |
| Messages read body, read/unread, flag/unflag, delete, archive, bulk actions. | Same. | Current audited; pending native. | Fixture includes action targets. |
| Labels list, show message labels, add/remove labels, label messages. | Same. | Current audited; pending native. | Full label CRUD is not current UI parity. |
| Snooze/unsnooze. | Same for snooze. | Current audited; pending native. | Unsnooze UI not currently exposed. |
| Rules engine. | Same. | Current audited; pending native. | Fixture includes active and inactive rules. |
| Search and suggestions. | Same. | Product gap; pending native. | Structured advanced-search params not parsed. |
| Webhooks. | Same. | Current audited; pending native. | Fixture includes signed/enabled and disabled rows. |
| SSE sync events. | Same. | Current audited; pending native. | Preserve sidebar progress. |
| OAuth callbacks. | Same. | Manual required. | Requires real provider credentials. |
