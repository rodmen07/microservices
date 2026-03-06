CREATE TABLE IF NOT EXISTS opportunities (
    id         TEXT    PRIMARY KEY NOT NULL,
    account_id TEXT    NOT NULL,
    name       TEXT    NOT NULL,
    stage      TEXT    NOT NULL DEFAULT 'qualification',
    amount     REAL    NOT NULL DEFAULT 0.0,
    close_date TEXT,
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
