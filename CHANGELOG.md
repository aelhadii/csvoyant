# Changelog

All notable changes to CSVoyant are documented here. One entry per merged prompt/PR.

## [Unreleased]

### Prompt C — Ingestion pipeline (#3)

**Added**
- **API**: `POST /jobs` (validate URL scheme/reachability/content-type + best-effort
  `Content-Length` size cap, persist a `queued` job, publish to RabbitMQ, return `{ job_id }`),
  `GET /jobs` (own jobs; Admins see all), `GET /jobs/{id}` (status; cross-tenant → 404).
- **Worker**: consumes RabbitMQ and ingests via ClickHouse Cloud's `url()` table function
  (DECISIONS #13) — `DESCRIBE` for schema, `CREATE TABLE … TTL 7d`, `INSERT … SELECT * FROM
  url(…)`, then `count()`. Drives the state machine `queued → downloading → inferring →
  ingesting → ready|failed`, one OTel span per stage.
- **Resilience**: retryable failures (ClickHouse unreachable) retry with exponential backoff up
  to `MAX_ATTEMPTS`; permanent failures (bad URL, unparseable CSV, type mismatch) and exhausted
  retries are dead-lettered to the DLQ with a clear user-facing `error`.
- **Format auto-detection**: `url()` is called with no explicit format, so ClickHouse detects
  CSV/TSV/Parquet/JSON and compression (`.xz`/`.gz`/`.zst`) from the URL (DECISIONS #13).
- **Uniform response envelope**: every endpoint now returns `{ "data": …, "error": … }`
  (`data` on success, `error` on failure). Dropped the redundant `token_type` from token
  responses (kept `expires_in`).
- **Transactional outbox**: `POST /jobs` writes the job row *and* an `outbox` row in one
  transaction; a background relay (`FOR UPDATE SKIP LOCKED`, nudged on submit) publishes to
  RabbitMQ — at-least-once delivery even if the API crashes mid-publish. The worker is
  idempotent (skips already-`ready` jobs; `DROP+CREATE` so a redelivery re-ingests cleanly).
- Migrations `0003_ingestion_jobs`, `0004_outbox`; `JobMessage` + AMQP topology in `shared`.
- Tests: 6 worker unit tests (describe parsing incl. malformed → permanent, DDL/insert SQL
  builders, SQL-injection escaping) + 3 ClickHouse error-mapping tests.


### Prompt B — Auth system (#2)

**Added**
- JWT auth against Postgres (sqlx): `POST /auth/register`, `POST /auth/login`,
  `POST /auth/refresh`, `PATCH /auth/email`, plus `GET /auth/me` and an Admin-only
  `GET /admin/ping` that exercises the RBAC guard.
- Access tokens (HS256, 15 min) + opaque rotating refresh tokens (7 days, SHA-256-hashed at
  rest, revoke-on-use). Refresh accepted via JSON body or the httpOnly `refresh_token` cookie.
- `argon2` password hashing; case-insensitive unique emails.
- `AuthUser` / `AdminUser` Axum extractors for authentication + role-based authorization.
- Request validation (`validator`) and consistent structured errors
  (`{ "error": { "code", "message" } }`).
- Migrations `0001_users` + `0002_refresh_tokens`, applied automatically on API startup.
- Tests: 8 unit (password/jwt/tokens) + 7 integration (`#[sqlx::test]`, isolated DB) covering
  register/login/refresh-rotation/change-email/validation/conflict and an RBAC test proving a
  User cannot reach an Admin route.

**Hardening (from code review)**
- Refresh rotation is a single atomic `UPDATE … WHERE revoked=false RETURNING user_id`, so a
  token can't be double-spent under concurrent redemption.
- Login verifies against a dummy hash when the email is unknown, equalizing timing to prevent
  account enumeration.
- `change_email` returns 401 (not 500) if the token's user no longer exists.

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
