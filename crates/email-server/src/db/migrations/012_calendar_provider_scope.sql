DROP INDEX IF EXISTS idx_calendar_events_provider;

CREATE UNIQUE INDEX IF NOT EXISTS idx_calendar_events_provider
ON calendar_events(account_id, calendar_id, provider_event_id)
WHERE provider_event_id IS NOT NULL;
