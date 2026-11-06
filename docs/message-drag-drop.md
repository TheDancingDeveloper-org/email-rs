# Desktop message drag and drop

Drag a message row onto a folder to move it. Hold Ctrl while dropping on
Windows or Linux to copy instead, leaving the original in its source folder.
Dragging a selected row carries the selected messages; the drag preview shows
the count. Dropping onto a label applies that label without moving messages.

Folder targets must belong to the messages' account. Unknown messages, missing
folders, and a selection entirely within the target folder are rejected.
The existing delayed-action Undo cancels a queued move or copy before execution.
Copies that have already executed are not undone by that cancellation window.

The client uses `POST /api/v1/messages/{id}/copy` with
`{"folderId":"destination-id"}` for copies, and the existing `/move` endpoint
for moves. Both require `mail.modify` for integration tokens. A copy uses IMAP
`UID COPY`, never source deletion or expunge, and returns `status: "copied"`.
When the provider supplies COPYUID, `messageId` identifies the cached destination
occurrence. Otherwise the original is retained and synchronization discovers
the destination. Successful copies emit `message.copied` webhooks.
