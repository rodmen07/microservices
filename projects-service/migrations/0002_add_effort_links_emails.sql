-- Add estimated_hours to deliverables
ALTER TABLE deliverables ADD COLUMN IF NOT EXISTS estimated_hours REAL;

-- Project links (Upwork, Drive, GitHub, etc.)
CREATE TABLE IF NOT EXISTS project_links (
    id           TEXT PRIMARY KEY NOT NULL,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    link_type    TEXT NOT NULL,
    label        TEXT NOT NULL,
    url          TEXT NOT NULL,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at   TEXT NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
);

-- Gmail threads synced to a project
CREATE TABLE IF NOT EXISTS project_emails (
    id           TEXT PRIMARY KEY NOT NULL,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    thread_id    TEXT NOT NULL,
    subject      TEXT NOT NULL,
    from_email   TEXT NOT NULL,
    snippet      TEXT,
    body_html    TEXT,
    received_at  TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    updated_at   TEXT NOT NULL DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    UNIQUE (project_id, thread_id)
);
