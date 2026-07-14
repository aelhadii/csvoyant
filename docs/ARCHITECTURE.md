# Architecture

## System diagram

```
┌─────────────┐     ┌──────────────┐     ┌───────────────┐
│  Frontend   │────▶│   API (Rust  │────▶│   RabbitMQ    │
│ (Next.js +  │◀────│   Axum)      │     │  (job queue)  │
│  shadcn/ui) │     └──────┬───────┘     └───────┬───────┘
└─────────────┘            │                     │
                           ▼                     ▼
                    ┌─────────────┐      ┌───────────────┐
                    │ PostgreSQL  │      │ Worker (Rust) │
                    │ users/jobs/ │◀─────│ fetch+ingest  │
                    │ metadata    │      └───────┬───────┘
                    └─────────────┘              │
                                                 ▼
                                          ┌─────────────┐
                                          │ ClickHouse  │
                                          │ (analytics) │
                                          └─────────────┘

   All services ──▶ Axiom (logs / traces / metrics via OpenTelemetry)
```

## Component responsibilities

- **API (Axum)** — auth, request validation, RBAC guards, job creation, publishing to
  RabbitMQ, read endpoints for job status / dashboard config / table data. Owns Postgres.
- **Worker (Rust)** — consumes RabbitMQ, streams the CSV download, infers schema, creates
  the ClickHouse table, bulk-loads rows, drives the job state machine, generates dashboard
  metadata on success. Writes ClickHouse + updates Postgres job rows.
- **PostgreSQL** — application state only (users, jobs, dashboards). *Not* the ingested data.
- **ClickHouse** — the ingested CSV data; source for all dashboard aggregations.
- **RabbitMQ** — durable job queue with a dead-letter exchange for failed/retried jobs.

## Cargo workspace layout

```
/Cargo.toml            # workspace
/crates/
  api/                 # Axum HTTP server
  worker/              # RabbitMQ consumer + ingestion
  shared/              # shared types (job status, DTOs, config, OTel setup)
```

## Data models (PostgreSQL)

- `users` — `id, email, password_hash, role[user|admin], created_at, updated_at`
- `refresh_tokens` — `id, user_id, token_hash, expires_at, revoked`
- `ingestion_jobs` — `id, user_id, source_url, status, error, clickhouse_table, row_count, inferred_schema jsonb, created_at, finished_at`
- `dashboards` — `id, job_id, user_id, config jsonb, created_at`

## Job status state machine

```
queued ─▶ downloading ─▶ inferring ─▶ ingesting ─▶ ready
   │           │             │            │
   └───────────┴─────────────┴────────────┴────────▶ failed (error set)
```

- Status transitions are persisted to `ingestion_jobs.status`.
- v1: clients poll `GET /jobs/{id}`. v1.5: push via SSE/WebSocket.
- On failure, `error` holds a clear, user-facing reason; the AMQP message is routed to the
  dead-letter queue and retried with exponential backoff up to N attempts.

## ClickHouse tenancy

**One table per job**, named with the user id + job id as a prefix (e.g.
`u{user_id}_j{job_id}`). Simpler isolation than a shared table with `tenant_id`, and each
CSV gets a schema that exactly matches its inferred columns. (See DECISIONS.)

## Schema inference

1. Read the header row for column names (sanitize to valid ClickHouse identifiers).
2. Sample the first N data rows.
3. Per column, attempt to parse as `Int64 → Float64 → Bool → Date → DateTime → String`,
   widening to `String` on any failure. Nullable if empty values are present.
4. Emit the inferred schema as JSON, stored on the job and used to `CREATE TABLE`.

## API surface (representative)

- `POST /auth/register`, `POST /auth/login`, `POST /auth/refresh`, `PATCH /auth/email`
- `POST /jobs` — submit a CSV URL, returns `{ job_id }`
- `GET /jobs` — list caller's jobs (Admin: all)
- `GET /jobs/{id}` — job status + error
- `GET /jobs/{id}/dashboard` — dashboard config
- `GET /jobs/{id}/data?page=&page_size=&...` — paginated / queryable table data
- `GET /admin/jobs`, `GET /admin/users` — Admin only

## Observability

- Instrument both Rust binaries with `tracing` + `tracing-opentelemetry`, exporting spans,
  logs, and metrics to **Axiom** via OTLP.
- Every ingestion stage is its own span; job id + user id are span attributes so a full
  ingestion is traceable end-to-end.
- Frontend can forward client errors/events to Axiom as well (optional in v1).
