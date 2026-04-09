CREATE TABLE IF NOT EXISTS spend_records (
    id              TEXT PRIMARY KEY NOT NULL,
    platform        TEXT NOT NULL,
    date            TEXT NOT NULL,
    amount_usd      DOUBLE PRECISION NOT NULL,
    granularity     TEXT NOT NULL DEFAULT 'daily',
    service_label   TEXT,
    source          TEXT NOT NULL DEFAULT 'manual',
    notes           TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_spend_platform ON spend_records(platform);
CREATE INDEX IF NOT EXISTS idx_spend_date ON spend_records(date);
CREATE UNIQUE INDEX IF NOT EXISTS idx_spend_dedup ON spend_records(platform, date, service_label)
    WHERE service_label IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_spend_dedup_no_label ON spend_records(platform, date)
    WHERE service_label IS NULL;
