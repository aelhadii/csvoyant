-- Auto-generated dashboard metadata for a ready job: summary stats, per-column aggregations,
-- and suggested charts. One dashboard per job (regeneration upserts).
CREATE TABLE dashboards (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id     UUID NOT NULL UNIQUE REFERENCES ingestion_jobs (id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    config     JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_dashboards_user ON dashboards (user_id);
