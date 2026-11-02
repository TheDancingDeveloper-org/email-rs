CREATE INDEX IF NOT EXISTS idx_messages_folder_date_id
    ON messages(folder_id, date DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_messages_folder_read_date_id
    ON messages(folder_id, is_read, date DESC, id DESC);
