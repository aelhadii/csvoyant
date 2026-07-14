# CSVoyant task runner. Install `just` (https://github.com/casey/just), then run `just <task>`.
# Falls back gracefully: most tasks are thin wrappers over cargo / docker compose.

set dotenv-load := true

# List available tasks.
default:
    @just --list

# ── Stack ────────────────────────────────────────────────────────────────────

# Build and start the full stack in the background.
up:
    docker compose up --build -d

# Stop the stack, keeping volumes.
down:
    docker compose down

# Stop the stack and delete volumes (Postgres/ClickHouse/RabbitMQ data).
down-hard:
    docker compose down -v

# Tail logs for all services (or `just logs api`).
logs service="":
    docker compose logs -f {{service}}

# Show service status.
ps:
    docker compose ps

# ── Rust ─────────────────────────────────────────────────────────────────────

# Type-check the whole workspace.
check:
    cargo check --workspace

# Run all tests.
test:
    cargo test --workspace

# Format all code.
fmt:
    cargo fmt --all

# Lint with clippy (warnings as errors).
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run the API locally (needs infra up + a local .env).
run-api:
    cargo run --bin api

# Run the worker locally.
run-worker:
    cargo run --bin worker

# ── Database ───────────────────────────────────────────────────────────────────

# Apply SQL migrations (requires sqlx-cli: `cargo install sqlx-cli`). Migrations land in Prompt B.
migrate:
    sqlx migrate run --source crates/api/migrations

# ── Frontend ───────────────────────────────────────────────────────────────────

# Install frontend deps.
fe-install:
    cd frontend && npm install

# Run the frontend dev server.
fe-dev:
    cd frontend && npm run dev
