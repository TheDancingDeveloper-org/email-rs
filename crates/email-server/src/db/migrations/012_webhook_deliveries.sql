CREATE TABLE IF NOT EXISTS webhook_events (
    id          TEXT PRIMARY KEY,
    event_type  TEXT NOT NULL,
    account_id  TEXT NOT NULL,
    data_json   TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_events_created
    ON webhook_events(created_at, id);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    event_id        TEXT NOT NULL REFERENCES webhook_events(id) ON DELETE CASCADE,
    webhook_id      TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'pending',
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_attempt_at TEXT,
    response_status INTEGER,
    last_error      TEXT,
    delivered_at    TEXT,
    PRIMARY KEY (event_id, webhook_id)
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_due
    ON webhook_deliveries(status, next_attempt_at);
