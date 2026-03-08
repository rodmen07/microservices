CREATE TABLE IF NOT EXISTS contacts (
    id               TEXT    PRIMARY KEY NOT NULL,
    account_id       TEXT,
    first_name       TEXT    NOT NULL,
    last_name        TEXT    NOT NULL,
    email            TEXT,
    phone            TEXT,
    lifecycle_stage  TEXT    NOT NULL DEFAULT 'lead',
    created_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_contacts_account_id ON contacts(account_id);
CREATE INDEX IF NOT EXISTS idx_contacts_email ON contacts(email);
