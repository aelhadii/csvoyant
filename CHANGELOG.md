# Changelog

All notable changes to CSVoyant are documented here. One entry per merged prompt/PR.

## [Unreleased]

### Prompt A — Repo scaffold & infra (#1)

**Added**
- Cargo workspace with three crates:
  - `shared` — configuration (`Config`/`TelemetryConfig` from env), the domain vocabulary
    (`Role`, `JobStatus` state machine, `ColumnType`/`InferredSchema`, `dataset_table_name`),
    and OpenTelemetry/Axiom telemetry init (traces + logs over OTLP-HTTP, stdout fallback).
  - `api` — Axum server with `/health` (liveness) and `/ready` (probes Postgres, ClickHouse,
    RabbitMQ), fail-fast connection setup, graceful shutdown.
  - `worker` — RabbitMQ consumer skeleton that declares the durable topology (jobs queue +
    dead-letter exchange/queue) and serves a `/health` + `/ready` endpoint for compose.
- `docker-compose.yml` running postgres, clickhouse, rabbitmq, api, worker, frontend with
  healthchecks and dependency-ordered startup (`condition: service_healthy`).
- Multi-stage `Dockerfile` (parameterized by `BIN`) for the Rust binaries; `frontend/Dockerfile`
  for the Next.js standalone build.
- Minimal Next.js (App Router + TypeScript) frontend skeleton with a `/health` route.
- `.env.example` documenting every config value; `justfile` for common tasks.
- Unit tests in `shared` asserting the job-status state machine and role hierarchy (DDD).

**Notes**
- Runnable skeleton only — no business logic. Auth (B), ingestion (C), dashboards (D), and the
  full frontend (E) follow.
- Verified: `cargo check`/`test`/`clippy -D warnings` pass, `cargo fmt --check` clean,
  `docker compose config` valid. Live `docker compose up` run pending (needs Docker daemon
  access in the build environment).
