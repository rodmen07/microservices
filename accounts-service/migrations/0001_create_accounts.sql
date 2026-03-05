CREATE TABLE IF NOT EXISTS accounts (
    id          TEXT    PRIMARY KEY NOT NULL,
    name        TEXT    NOT NULL,
    domain      TEXT,
    status      TEXT    NOT NULL DEFAULT 'active',
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
