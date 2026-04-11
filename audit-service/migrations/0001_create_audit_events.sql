CREATE TABLE IF NOT EXISTS audit_events (
    id           TEXT PRIMARY KEY NOT NULL,
    entity_type  TEXT NOT NULL,
    entity_id    TEXT NOT NULL,
    action       TEXT NOT NULL,
    actor_id     TEXT NOT NULL,
    entity_label TEXT,
    payload      TEXT,
    created_at   TEXT NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

CREATE INDEX IF NOT EXISTS audit_entity_type_idx ON audit_events(entity_type);
CREATE INDEX IF NOT EXISTS audit_actor_idx        ON audit_events(actor_id);
CREATE INDEX IF NOT EXISTS audit_created_idx      ON audit_events(created_at DESC);
