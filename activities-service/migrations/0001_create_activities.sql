CREATE TABLE IF NOT EXISTS activities (
    id            TEXT    PRIMARY KEY NOT NULL,
    owner_id      TEXT    NOT NULL,
    account_id    TEXT,
    contact_id    TEXT,
    activity_type TEXT    NOT NULL,
    subject       TEXT    NOT NULL,
    notes         TEXT,
    due_at        TEXT,
    completed     BOOLEAN NOT NULL DEFAULT false,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_activities_owner_id ON activities(owner_id);
