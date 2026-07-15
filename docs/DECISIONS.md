# Decisions & Open Questions

Recommended defaults are chosen so build sessions can start immediately. Override any of
them before/at handoff. **Prepend this file to every build session** so all agents share the
same assumptions.

## Resolved defaults (change if you disagree)

| # | Question | Default |
|---|----------|---------|
| 1 | Email verification / password reset in v1? | **No** — auth = register / login / change-email only. Verification + reset deferred to v2 (needs an email provider). |
| 2 | Max CSV file size / download timeout? | **120 s** ingestion timeout, enforced as `max_execution_time` on the ClickHouse `url()` query (see #13). A **500 MB** ceiling is best-effort at submit time via the URL's `Content-Length`; ClickHouse streams the fetch so the worker never holds the file. |
| 3 | ClickHouse multi-tenancy model? | **One table per job**, prefixed `u{user_id}_j{job_id}`. |
| 4 | Dashboard generation — auto or configurable? | **Auto-inferred in v1**, user-configurable in v1.5. |
| 5 | Dedup — same URL submitted twice? | **New job every time** in v1 (no dedup). |
| 6 | RabbitMQ vs Celery? | **RabbitMQ + Rust worker** (`lapin`). One language, shared types, no Python runtime. |
| 7 | Frontend framework? | **Next.js App Router** — best shadcn/ui support. |
| 8 | Deployment target beyond local compose? | **Local docker compose only** for v1. Design config via env vars so a later move to K8s/PaaS is clean. |
| 9 | Auth token storage on the frontend? | **httpOnly refresh cookie + in-memory access token**, with refresh rotation. Best XSS posture. |
| 10 | Chart library? | **shadcn charts** (shadcn/ui's Recharts-based components) — stays in the shadcn ecosystem and theming. |
| 11 | Per-user job submission rate limit? | **No limit in v1.** Revisit once real usage patterns are known; add a per-user cap in v1.5. |
| 12 | Data retention for ingested tables? | **TTL 7 days** — ClickHouse tables/data auto-expire 7 days after ingestion. |
| 13 | How does the worker fetch + ingest the file? | **ClickHouse Cloud `url()` server-side**, not a worker-side download. The worker runs `DESCRIBE url(<src>)` for schema inference, then `CREATE TABLE … TTL 7d` and `INSERT … SELECT * FROM url(<src>)`. **No explicit format** is passed, so ClickHouse auto-detects the format (CSV/TSV/Parquet/JSON), compression (`.xz`/`.gz`/`.zst`), and header from the URL — accepting more than plain CSV. ClickHouse Cloud fetches, parses, infers types, and streams the load. **Why:** files can be large; downloading them into the worker (local compose, #8) would buffer/spill a big file locally, so we delegate fetch+parse+ingest to the Cloud. The worker keeps the state machine, per-stage OTel spans, retries, and error mapping around these SQL calls. Trade-offs: type/format inference and byte-level size limits are ClickHouse's rather than ours (see #2); auto-detection needs a recognizable file extension. |
