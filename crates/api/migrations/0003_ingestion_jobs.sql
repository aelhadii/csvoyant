-- Ingestion jobs: one row per submitted CSV URL, tracking the lifecycle
-- queued → downloading → inferring → ingesting → ready | failed.
CREATE TABLE ingestion_jobs (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    source_url       TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'downloading', 'inferring', 'ingesting', 'ready', 'failed')),
    error            TEXT,
    clickhouse_table TEXT,
    row_count        BIGINT,
    inferred_schema  JSONB,
    attempts         INTEGER NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at      TIMESTAMPTZ
);

CREATE INDEX idx_ingestion_jobs_user ON ingestion_jobs (user_id);
CREATE INDEX idx_ingestion_jobs_status ON ingestion_jobs (status);
