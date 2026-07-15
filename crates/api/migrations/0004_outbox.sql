-- Transactional outbox: messages to publish, written in the SAME transaction as the job row
-- so the enqueue can't be lost if the process dies before publishing. A background relay
-- publishes unpublished rows to RabbitMQ and stamps `published_at`.
CREATE TABLE outbox (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    queue        TEXT NOT NULL,
    payload      JSONB NOT NULL,
    attempts     INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ
);

-- Partial index so the relay's "unpublished, oldest first" scan stays cheap as the table grows.
CREATE INDEX idx_outbox_unpublished ON outbox (created_at) WHERE published_at IS NULL;
