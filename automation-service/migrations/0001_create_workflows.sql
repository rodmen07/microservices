CREATE TABLE IF NOT EXISTS workflows (
    id            TEXT    PRIMARY KEY NOT NULL,
    name          TEXT    NOT NULL,
    trigger_event TEXT    NOT NULL,
    action_type   TEXT    NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at    TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);
