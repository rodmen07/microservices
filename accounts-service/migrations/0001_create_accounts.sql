CREATE TABLE IF NOT EXISTS accounts (
    id          TEXT    PRIMARY KEY NOT NULL,
    name        TEXT    NOT NULL,
    domain      TEXT,
    status      TEXT    NOT NULL DEFAULT 'active',
    created_at  TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at  TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);
