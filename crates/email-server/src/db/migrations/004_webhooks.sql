CREATE TABLE IF NOT EXISTS webhooks (
    id          TEXT PRIMARY KEY,
    url         TEXT NOT NULL,
    secret      TEXT,
    events      TEXT NOT NULL DEFAULT 'message.created',
    account_id  TEXT REFERENCES accounts(id) ON DELETE CASCADE,
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
