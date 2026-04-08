CREATE TABLE IF NOT EXISTS connections (
    id             TEXT PRIMARY KEY NOT NULL,
    provider       TEXT NOT NULL,
    account_ref    TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'connected',
    last_synced_at TEXT,
    created_at     TEXT NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at     TEXT NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);
