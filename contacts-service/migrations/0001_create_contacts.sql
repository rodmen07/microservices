CREATE TABLE IF NOT EXISTS contacts (
    id               TEXT    PRIMARY KEY NOT NULL,
    account_id       TEXT,
    first_name       TEXT    NOT NULL,
    last_name        TEXT    NOT NULL,
    email            TEXT,
    phone            TEXT,
    lifecycle_stage  TEXT    NOT NULL DEFAULT 'lead',
    created_at       TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at       TEXT    NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE INDEX IF NOT EXISTS idx_contacts_account_id ON contacts(account_id);
CREATE INDEX IF NOT EXISTS idx_contacts_email ON contacts(email);
