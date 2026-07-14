# PRD — CSV → ClickHouse → Dashboard Platform

## 1. One-liner
A multi-tenant web platform where an authenticated user submits a URL to a CSV file; a
background worker fetches, validates, and ingests that data into ClickHouse; and the
results are rendered as an auto-generated, interactive dashboard.

## 2. Goals
- Turn an arbitrary remote CSV into queryable analytics + a dashboard with zero manual schema work.
- Handle large files asynchronously — never block an HTTP request on ingestion.
- Be observable end-to-end (traces, logs, metrics) via Axiom.
- Ship as a single `docker compose up` local stack.

## 3. Non-goals (v1)
- Real-time streaming ingestion (batch-per-URL only).
- In-app data editing/transformation (ingest as-is; light type coercion only).
- Arbitrary SQL for end users (Admin-only if at all).

## 4. Actors & Permissions

| Actor | Capabilities |
|-------|--------------|
| **User** | Register, login, change email, submit CSV URLs, view **their own** jobs + dashboards |
| **Admin** | Everything a User can, plus: view all users' jobs, manage users, view system health, (optional) run raw queries |

## 5. Core User Flows

1. **Auth** — Register → login (JWT access + refresh) → change email. (Email verification is out of v1 scope — see DECISIONS.)
2. **Submit** — User pastes a CSV URL → API validates → enqueues an ingestion job → returns `job_id`.
3. **Ingest (async)** — Worker consumes job → streams download → infers schema → creates/appends a ClickHouse table → bulk-loads rows → updates job status.
4. **Dashboard** — On success, backend generates dashboard metadata (column types → chart suggestions) → FE renders summary cards, charts, and a paginated data table.
5. **Monitor** — User polls (v1) or subscribes (v1.5) to job status: `queued → downloading → inferring → ingesting → ready | failed (with reason)`.

## 6. Ingestion Requirements
- **Streaming download** — do not buffer the whole file in memory.
- **Schema inference** — detect column types (int, float, bool, date/datetime, string) from the header + a sampled set of N rows.
- **CSV robustness** — quoted fields, embedded commas/newlines, configurable delimiter, BOM, UTF-8 baseline encoding.
- **Per-tenant isolation** — one ClickHouse table per job, prefixed by user id (see DECISIONS).
- **Error surfaces** — bad URL, non-CSV content-type, oversized file, malformed rows, network timeout — each mapped to a clear, user-facing reason.
- **Guardrails** — enforce a max file-size ceiling and download timeout (see DECISIONS).

## 7. Success Metrics
- p95 time-to-dashboard for a 100k-row CSV.
- Ingestion success rate; % of failed jobs that fail with a *clear* reason.
- Zero cross-tenant data leakage (verified by tests).

## 8. Feature Roadmap

### Must-have (v1)
- JWT auth: access + refresh tokens; register, login, change email; `argon2` password hashing.
- Role-based guards (User vs Admin).
- URL submission + async ingestion pipeline.
- Schema inference + ClickHouse table creation.
- Auto-generated dashboard: summary cards (row/column count), per-column stats, 2–3 suggested charts, paginated data table.
- Job status tracking + failure reasons.
- Full Axiom observability + docker compose.

### Should-have (v1.5)
- Real-time job status via SSE/WebSocket.
- Dashboard customization (pick columns, chart types, save layout).
- Rate limiting + max file-size guardrails (hardened).
- Dead-letter queue + automatic retry with backoff.
- Admin panel: all jobs, per-user usage, system health.

### Nice-to-have (v2)
- Scheduled re-ingestion (refresh a URL on a cron).
- More formats (TSV, Parquet, JSON).
- Shareable/public read-only dashboards.
- Email verification + password reset.
- Export dashboard as PDF/PNG.
