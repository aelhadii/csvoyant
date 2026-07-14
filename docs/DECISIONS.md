# Decisions & Open Questions

Recommended defaults are chosen so build sessions can start immediately. Override any of
them before/at handoff. **Prepend this file to every build session** so all agents share the
same assumptions.

## Resolved defaults (change if you disagree)

| # | Question | Default |
|---|----------|---------|
| 1 | Email verification / password reset in v1? | **No** — auth = register / login / change-email only. Verification + reset deferred to v2 (needs an email provider). |
| 2 | Max CSV file size / download timeout? | **500 MB** ceiling, **120 s** connect + streaming read timeout. Reject larger/slower with a clear reason. |
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
