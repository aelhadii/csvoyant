# Changelog

All notable changes to CSVoyant are documented here. One entry per merged prompt/PR.

## [Unreleased]

### Fixed — Docker glibc mismatch

- Pinned the Rust builder image to `rust:1.97-bookworm`. The default `rust:1.97` tag is
  Debian trixie (glibc 2.38+), while the runtime is `debian:bookworm-slim` (glibc 2.36), so
  the compiled binaries died at startup with `version 'GLIBC_2.38' not found`. Matching the
  builder to the runtime resolves it. Verified: full stack builds and all services report
  healthy (`api`/`worker` boot cleanly).

### Changed — use managed ClickHouse Cloud

- Removed the `clickhouse` service from docker-compose; the platform now targets a managed
  **ClickHouse Cloud** cluster (external). Connect over HTTPS on port 8443 via
  `CLICKHOUSE_URL`/`CLICKHOUSE_USER`/`CLICKHOUSE_PASSWORD`/`CLICKHOUSE_DATABASE` in `.env`.
- Enabled the `rustls-tls` feature on the `clickhouse` crate so the Rust client speaks HTTPS
  (CA roots via webpki). No application code changes — the client already reads these env vars.
- Dropped `clickhouse` from the `api`/`worker` `depends_on` and removed the `chdata` volume.

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
