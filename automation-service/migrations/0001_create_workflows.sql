CREATE TABLE IF NOT EXISTS workflows (
    id            TEXT    PRIMARY KEY NOT NULL,
    name          TEXT    NOT NULL,
    trigger_event TEXT    NOT NULL,
    action_type   TEXT    NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
