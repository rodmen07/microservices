CREATE TABLE IF NOT EXISTS accounts (
    id          TEXT    PRIMARY KEY NOT NULL,
    owner_id    TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    domain      TEXT,
    status      TEXT    NOT NULL DEFAULT 'active',
    created_at  TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at  TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE INDEX IF NOT EXISTS idx_accounts_owner_id ON accounts(owner_id);
