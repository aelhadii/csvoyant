# Kickoff Prompts for Build Sessions

Run these in order (except Prompt F, it can be run the first one or in Parallel). **Prepend the contents of `docs/DECISIONS.md`** to each session so every
agent builds against the same resolved assumptions. Each prompt is self-contained.

---

## Prompt A — Repo scaffold & infra

```
Set up a monorepo for a CSV→ClickHouse→dashboard platform.
Stack: Rust (Axum API + a separate worker binary in one cargo workspace with a shared
crate), PostgreSQL, ClickHouse, RabbitMQ, and a Next.js + TypeScript + shadcn/ui frontend.

Deliver:
- A cargo workspace with `api`, `worker`, and `shared` crates.
- docker-compose.yml running: postgres, clickhouse, rabbitmq, api, worker, frontend.
- .env.example with every config value (DB URLs, AMQP URL, Axiom token/dataset, JWT secret).
- Health-check endpoints on the API; readiness checks in compose.
- A justfile/Makefile for common tasks (up, down, migrate, test, logs).
- Wire OpenTelemetry (tracing + tracing-opentelemetry) in both Rust binaries, exporting
  traces/logs/metrics to Axiom via OTLP, initialized from the shared crate.

Do NOT implement business logic yet — deliver a runnable skeleton where every service
starts, connects, and reports healthy.
```

---

## Prompt B — Auth system

```
Implement JWT auth in the Rust Axum API against PostgreSQL (sqlx).
Features: register, login, change email. Access + refresh tokens with refresh rotation.
Password hashing with argon2. Two roles: User and Admin, enforced via an Axum
extractor/middleware guard.

Deliver:
- Migrations for `users` and `refresh_tokens`.
- Endpoints: POST /auth/register, POST /auth/login, POST /auth/refresh, PATCH /auth/email.
- Request validation and structured JSON error responses.
- A role guard that Admin-only routes can require.
- Integration tests covering register/login/refresh/change-email + an RBAC test proving a
  User cannot reach an Admin route.
Follow REST conventions.
```

---

## Prompt C — Ingestion pipeline (API endpoint + worker)

```
Build the async ingestion pipeline.

API side:
- POST /jobs accepts a CSV URL, validates it (scheme, reachability, content-type), creates
  an `ingestion_jobs` row (status=queued), publishes the job to RabbitMQ (lapin), returns
  { job_id }.
- GET /jobs and GET /jobs/{id} for status (User sees own; Admin sees all).

Worker side (Rust, lapin consumer):
- Stream the download (do NOT buffer the whole file); enforce the max-size and timeout from
  DECISIONS.
- Infer column types from the header + a sampled set of rows
  (Int64→Float64→Bool→Date→DateTime→String, Nullable when empties present).
- Create a per-job ClickHouse table (u{user_id}_j{job_id}) with a 7-day TTL on the
  ingestion timestamp (data auto-expires 7 days after load), and bulk-load rows.
- Drive the state machine: queued→downloading→inferring→ingesting→ready | failed.
- Set a clear, user-facing `error` on failure.
- Dead-letter exchange + retry with exponential backoff (N attempts).
- Instrument every stage with an OpenTelemetry span (job id + user id as attributes) to Axiom.

Include tests with a sample CSV (happy path + a malformed-row failure path).
```

---

## Prompt D — Dashboard generation + read APIs

```
Add dashboard generation and the read APIs.

When a job reaches `ready`, generate dashboard metadata from the inferred schema and store
it in the `dashboards` table:
- Summary stats: row count, column count.
- Per-column aggregations queried from ClickHouse (min/max/avg/distinct-count as fits type).
- Suggested charts by column type: categorical→bar, numeric→histogram, datetime→time series.

Expose:
- GET /jobs/{id}/dashboard — the dashboard config.
- GET /jobs/{id}/data?page=&page_size=&sort=&filter= — paginated/queryable table data from
  ClickHouse.

Enforce tenancy in every handler: Users can only access their own jobs; Admins access all.
Add tests proving cross-tenant access is denied.
```

---

## Prompt E — Frontend

```
Build the Next.js (App Router) + TypeScript + shadcn/ui + Tailwind frontend.

Pages/flows:
- Auth: register, login, change-email. JWT with refresh rotation (in-memory access token +
  httpOnly refresh cookie). Protected routes redirect when unauthenticated.
- Submit: a page to paste a CSV URL and start a job.
- Jobs: a list with live-updating status (poll GET /jobs/{id}; upgrade to SSE later).
- Dashboard: render summary cards, charts (shadcn charts — the Recharts-based shadcn/ui
  chart components), and a paginated data table from the dashboard config + data endpoints.
- Admin: a section listing all users' jobs (visible only to Admin).

Handle loading / error / empty states everywhere. Match shadcn/ui design conventions.
```

---

---

## Prompt F — Misc

```
- Use this repo: https://github.com/aelhadii/csvoyant for this project and create gh issues along the way for each prompt(step) and assign each agent to a gh issue, update it as well along the way, create PR for each issue, using git flow for the naming convention, use simple commits, create release notes for each PR, etc.
- Init CLAUDE.md file as your context and update it along the way.
- Create unit tests for each crate along the way and make sure every test solidify the acutal business domain we have, also use DDD here to have a ubqitious language across everything.
- Create OpenAPI documentation for the user-facing API.
- Run full-review agents after each step to verify everything is working well and use Skills as you want for code reviewing, code smell, architecture patterns, etc.
- for each Prompt (step) from above, you can lock it in in its PR

```

---

## After the build

- Run `docker compose up`, submit a real CSV URL, and confirm the full path:
  submit → job transitions → dashboard renders.
- Confirm traces for one ingestion appear in Axiom end-to-end.
- Confirm a User cannot see another User's jobs or dashboards.

