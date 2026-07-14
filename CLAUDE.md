# CLAUDE.md — CSVoyant

Project context for AI build sessions. Keep this updated as the build progresses.

## What this is

A multi-tenant web platform: an authenticated user submits a direct URL to a CSV file; a
background worker streams, validates, infers a schema for, and ingests that data into
ClickHouse; the result is rendered as an auto-generated interactive dashboard.

See `docs/` for the source of truth:
- `docs/PRD.md` — product requirements, actors, flows, roadmap.
- `docs/ARCHITECTURE.md` — system diagram, data models, state machine, API surface.
- `docs/DECISIONS.md` — 12 resolved defaults (prepend to every build session).
- `docs/PROMPTS.md` — the ordered build prompts (A–F).

## Stack

| Layer | Choice |
|-------|--------|
| API | Rust + Axum (`tokio`, `sqlx`) |
| Worker | Rust binary (same cargo workspace), `lapin` AMQP client |
| Queue | RabbitMQ (durable, dead-letter, retry-with-backoff) |
| App DB | PostgreSQL (users, jobs, dashboards) |
| Analytics DB | ClickHouse (ingested CSV data, one table per job) |
| Observability | OpenTelemetry → Axiom (traces/logs/metrics) |
| Frontend | Next.js (App Router) + TypeScript + shadcn/ui + Tailwind |

## Workspace layout

```
/Cargo.toml            # cargo workspace
/crates/
  api/                 # Axum HTTP server (owns Postgres, publishes to RabbitMQ)
  worker/              # RabbitMQ consumer + ingestion pipeline
  shared/              # shared types (job status, DTOs, config, OTel setup)
/frontend/             # Next.js app
/docs/                 # planning docs (source of truth)
docker-compose.yml     # full local stack
.env.example           # every config value
justfile               # common tasks (up, down, migrate, test, logs)
```

## Ubiquitous language (DDD)

Shared vocabulary across code, tests, docs, and commits:

- **User / Admin** — the two roles. Admin is a superset of User.
- **Ingestion Job** (`ingestion_jobs`) — one submission of a CSV URL. Has a lifecycle:
  `queued → downloading → inferring → ingesting → ready | failed`.
- **Source URL** — the direct HTTP(S) link to a `.csv` resolving to CSV bytes.
- **Inferred Schema** — the per-column types deduced from header + sampled rows.
- **Dataset Table** — the per-job ClickHouse table `u{user_id}_j{job_id}` holding ingested rows.
- **Dashboard** — auto-generated metadata (summary stats, per-column aggregations, suggested
  charts) derived from a ready job.

## Conventions

- **Git flow**: `feature/<slug>` branches off `main`, one per prompt/issue; PR into `main`
  with release notes; squash-friendly simple commits.
- **Every crate carries unit tests** that assert real domain behavior using the ubiquitous
  language above.
- **OpenAPI** documents the user-facing API.
- Config via env vars only (see `.env.example`) so a later move off docker-compose is clean.

## Build status

| Prompt | Scope | Status |
|--------|-------|--------|
| A | Repo scaffold & infra | in progress |
| B | Auth system | not started |
| C | Ingestion pipeline | not started |
| D | Dashboard + read APIs | not started |
| E | Frontend | not started |
| F | Cross-cutting (issues/PRs, CLAUDE.md, tests, OpenAPI, reviews) | ongoing |
