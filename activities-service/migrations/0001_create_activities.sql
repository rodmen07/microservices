CREATE TABLE IF NOT EXISTS activities (
    id            TEXT    PRIMARY KEY NOT NULL,
    account_id    TEXT,
    contact_id    TEXT,
    activity_type TEXT    NOT NULL,
    subject       TEXT    NOT NULL,
    notes         TEXT,
    due_at        TEXT,
    completed     INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
