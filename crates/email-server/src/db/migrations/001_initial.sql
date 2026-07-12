CREATE TABLE IF NOT EXISTS accounts (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    email       TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    auth_type   TEXT NOT NULL,
    oauth_token_json TEXT,
    host        TEXT,
    port        INTEGER,
    use_ssl     BOOLEAN NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS folders (
    id          TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    full_path   TEXT NOT NULL,
    special_use TEXT,
    unread_count INTEGER NOT NULL DEFAULT 0,
    total_count  INTEGER NOT NULL DEFAULT 0,
    synced_at   TEXT,
    uid_validity INTEGER NOT NULL DEFAULT 0,
    UNIQUE(account_id, full_path)
);

CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    logical_id      TEXT NOT NULL DEFAULT '',
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    folder_id       TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    uid             INTEGER NOT NULL,
    uid_validity    INTEGER NOT NULL DEFAULT 0,
    message_id      TEXT,
    thread_id       TEXT,
    subject         TEXT,
    from_name       TEXT,
    from_email      TEXT,
    to_json         TEXT,
    cc_json         TEXT,
    date            TEXT,
    is_read         BOOLEAN NOT NULL DEFAULT 0,
    is_flagged      BOOLEAN NOT NULL DEFAULT 0,
    is_draft        BOOLEAN NOT NULL DEFAULT 0,
    has_attachments BOOLEAN NOT NULL DEFAULT 0,
    preview         TEXT,
    synced_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(folder_id, uid_validity, uid)
);

CREATE INDEX IF NOT EXISTS idx_messages_folder ON messages(folder_id);
CREATE INDEX IF NOT EXISTS idx_messages_account ON messages(account_id);
CREATE INDEX IF NOT EXISTS idx_messages_date ON messages(date DESC);
CREATE INDEX IF NOT EXISTS idx_messages_rfc_message_id ON messages(message_id);
CREATE INDEX IF NOT EXISTS idx_messages_logical_id ON messages(logical_id);

CREATE TABLE IF NOT EXISTS message_bodies (
    message_id   TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    html_body    TEXT,
    text_body    TEXT,
    raw_headers  TEXT,
    fetched_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS attachments (
    id           TEXT PRIMARY KEY,
    message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    content_type TEXT NOT NULL,
    part_id      TEXT NOT NULL,
    content_id   TEXT,
    size_bytes   INTEGER NOT NULL DEFAULT 0,
    content      BLOB,
    UNIQUE(message_id, part_id)
);

CREATE TABLE IF NOT EXISTS calendar_events (
    id                TEXT PRIMARY KEY,
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    calendar_id       TEXT NOT NULL,
    provider_event_id TEXT,
    title             TEXT NOT NULL,
    description       TEXT,
    start_at          TEXT NOT NULL,
    end_at            TEXT NOT NULL,
    location          TEXT,
    is_all_day        BOOLEAN NOT NULL DEFAULT 0,
    recurrence_rule   TEXT,
    attendees_json    TEXT,
    meet_link         TEXT,
    synced_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_calendar_events_account ON calendar_events(account_id);
CREATE INDEX IF NOT EXISTS idx_calendar_events_start ON calendar_events(start_at);
