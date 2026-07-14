# CSVoyant — CSV → ClickHouse → Dashboard Platform

Submit a direct URL to a CSV file (e.g. `https://example.com/file.csv`). A background worker
fetches it, infers a schema, ingests the data into ClickHouse, and the platform renders an
auto-generated, interactive dashboard.

Multi-tenant, JWT-authenticated (User / Admin), fully observable via Axiom, and runnable
end-to-end with `docker compose up`.

> **Input model:** the URL is a plain HTTP(S) link that resolves directly to a `.csv` file
> (like `https://example.com/file.csv`). The worker does a straight streaming GET on it — no
> HTML scraping, no auth negotiation, no cloud-storage SDKs in v1.

## Stack

| Layer | Choice |
|-------|--------|
| API | Rust + Axum (`tokio`, `sqlx`) |
| Worker | Rust binary (same cargo workspace), `lapin` AMQP client |
| Queue | RabbitMQ (durable queues, dead-letter, retry-with-backoff) |
| App DB | PostgreSQL (users, jobs, dashboards) |
| Analytics DB | ClickHouse (ingested CSV data) |
| Frontend | Next.js (App Router) + TypeScript + shadcn/ui + Tailwind + Tremor/Recharts |
| Observability | OpenTelemetry → Axiom (traces, logs, metrics) |
| Infra | docker compose |

**Why RabbitMQ + a Rust worker over Celery:** keeping the worker in Rust means one
language across the workspace, shared types with the API, and no Python runtime. Choose
Celery only if you specifically want Python's data-libs ecosystem in the worker — not
needed here.

## Docs

- [`docs/PRD.md`](docs/PRD.md) — product requirements, actors, flows, success metrics
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — system design, data models, state machine
- [`docs/PROMPTS.md`](docs/PROMPTS.md) — copy-paste kickoff prompts for build sessions
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — resolved defaults + remaining open questions

## Build order

Run the sessions in [`docs/PROMPTS.md`](docs/PROMPTS.md) in sequence:

1. **A** — Repo scaffold & infra (compose, workspace, OTel wiring)
2. **B** — Auth system (JWT, roles)
3. **C** — Ingestion pipeline (API endpoint + worker)
4. **D** — Dashboard generation + read APIs
5. **E** — Frontend

Prepend [`docs/DECISIONS.md`](docs/DECISIONS.md) to each session so every agent shares the
same assumptions.
