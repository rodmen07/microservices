CREATE TABLE IF NOT EXISTS opportunities (
    id         TEXT    PRIMARY KEY NOT NULL,
    owner_id   TEXT    NOT NULL,
    account_id TEXT    NOT NULL,
    name       TEXT    NOT NULL,
    stage      TEXT    NOT NULL DEFAULT 'qualification',
    amount     REAL    NOT NULL DEFAULT 0.0,
    close_date TEXT,
    created_at TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE INDEX IF NOT EXISTS idx_opportunities_owner_id ON opportunities(owner_id);
