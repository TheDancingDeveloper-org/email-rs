-- Unique index to allow upsert by provider_event_id during Google Calendar sync
CREATE UNIQUE INDEX IF NOT EXISTS idx_calendar_events_provider
ON calendar_events(account_id, provider_event_id)
WHERE provider_event_id IS NOT NULL;

-- Email <-> calendar event links
CREATE TABLE IF NOT EXISTS message_calendar_links (
    id         TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    event_id   TEXT NOT NULL REFERENCES calendar_events(id) ON DELETE CASCADE,
    linked_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(message_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_mcl_message ON message_calendar_links(message_id);
CREATE INDEX IF NOT EXISTS idx_mcl_event ON message_calendar_links(event_id);
