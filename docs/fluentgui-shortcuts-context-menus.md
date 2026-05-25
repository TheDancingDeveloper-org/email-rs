# FluentGUI Current Shortcuts And Context Menus

Last updated: 2026-05-25

## Keyboard Shortcuts

| Area | Shortcut | Current behavior | Source |
|---|---|---|---|
| App shell | Ctrl+K / Cmd+K | Toggle command palette. | `frontend/src/App.tsx:88` |
| Message list | j / k | Move selected message down/up when focus is not in an input or textarea. | `frontend/src/components/MessageList/MessageList.tsx:46` |
| Message list | Ctrl+A / Cmd+A | Select all current messages. | `frontend/src/components/MessageList/MessageList.tsx:174` |
| Message list | Escape | Clear bulk selection, otherwise deselect the current message. | `frontend/src/components/MessageList/MessageList.tsx:181` |
| Message list | x | Toggle selection for the focused/current message. | `frontend/src/components/MessageList/MessageList.tsx:193` |
| Message list | e | Archive the focused/current message. | `frontend/src/components/MessageList/MessageList.tsx:199` |
| Message list | Delete / Backspace | Delete the focused/current message. | `frontend/src/components/MessageList/MessageList.tsx:208` |
| Message list | u | Toggle read/unread for the focused/current message. | `frontend/src/components/MessageList/MessageList.tsx:217` |
| Message row | Enter | Open/select a focused message row. | `frontend/src/components/MessageList/MessageRow.tsx:98` |
| Reading pane | e | Archive the open message. | `frontend/src/components/ReadingPane/ReadingPane.tsx:257` |
| Reading pane | # | Delete the open message. | `frontend/src/components/ReadingPane/ReadingPane.tsx:258` |
| Reading pane | u | Toggle read/unread for the open message. | `frontend/src/components/ReadingPane/ReadingPane.tsx:259` |
| Reading pane | Escape | Close/deselect the open message. | `frontend/src/components/ReadingPane/ReadingPane.tsx:260` |
| Sidebar search | ArrowDown / ArrowUp | Move active suggestion. | `frontend/src/components/Sidebar/Sidebar.tsx:383` |
| Sidebar search | Enter | Open active suggestion. Enter without an active suggestion keeps the full search query. | `frontend/src/components/Sidebar/Sidebar.tsx:389` |
| Sidebar search | Escape | Close suggestions. | `frontend/src/components/Sidebar/Sidebar.tsx:392` |
| Sidebar nav | Enter | Open focused smart folder, calendar item, label, special folder, or nested folder. | `frontend/src/components/Sidebar/Sidebar.tsx:549` |
| Command palette | ArrowDown / ArrowUp | Move active command. | `frontend/src/components/CommandPalette/CommandPalette.tsx:226` |
| Command palette | Enter | Run active command and close palette. | `frontend/src/components/CommandPalette/CommandPalette.tsx:233` |
| Command palette | Escape | Close palette. | `frontend/src/components/CommandPalette/CommandPalette.tsx:237` |
| Recipient input | ArrowDown / ArrowUp | Move active contact suggestion. | `frontend/src/components/Compose/RecipientInput.tsx:81` |
| Recipient input | Enter / Tab | Accept active contact suggestion. | `frontend/src/components/Compose/RecipientInput.tsx:87` |
| Recipient input | Escape | Close contact suggestions. | `frontend/src/components/Compose/RecipientInput.tsx:90` |
| Advanced search | Enter | Run search when at least one condition has a value. | `frontend/src/components/AdvancedSearch/AdvancedSearchModal.tsx:23` |
| Advanced search | Escape | Close modal. | `frontend/src/components/AdvancedSearch/AdvancedSearchModal.tsx:24` |

## Context Menus

| Area | Items | Behavior | Source |
|---|---|---|---|
| Shared menu | Any supplied items, separators, disabled items | Prevents browser menu, opens at pointer coordinates, clamps to viewport, closes on outside mouse down or Escape. | `frontend/src/components/ContextMenu/ContextMenu.tsx:98` |
| Message row | Open, Reply, Forward, Archive, Move to Trash, Mark Read, Mark Unread, Flag, Unflag | Attached to thread parent rows, expanded thread child rows, and flat list rows. Reply/Forward currently select the message rather than immediately opening compose. | `frontend/src/components/MessageList/MessageList.tsx:259` |
| Reading pane header | Reply, Forward, Archive, Delete, Mark Read, Mark Unread, Copy sender email | Copy sender email uses the browser clipboard API and is disabled when no sender email exists. | `frontend/src/components/ReadingPane/ReadingPane.tsx:433` |
| Smart folders | Open folder | Applies to All Inboxes, Unread, Flagged, Snoozed. | `frontend/src/components/Sidebar/Sidebar.tsx:550` |
| Special IMAP folders | Open folder, Mark all as read | Applies to inbox, sent, drafts, archive, trash, spam. | `frontend/src/components/Sidebar/Sidebar.tsx:640` |
| Regular/nested IMAP folders | Open folder, Mark all as read | Applies under the collapsible Folders group. | `frontend/src/components/Sidebar/Sidebar.tsx:684` |
