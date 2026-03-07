CREATE TABLE IF NOT EXISTS connections (
    id             TEXT PRIMARY KEY NOT NULL,
    provider       TEXT NOT NULL,
    account_ref    TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'connected',
    last_synced_at TEXT,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
