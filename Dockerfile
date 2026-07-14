# Multi-stage build for the Rust binaries. Build a specific binary with `--build-arg BIN=api`
# or `BIN=worker`. Uses the full `rust` image so C toolchain deps (for rustls/ring, etc.) are
# present without extra apt installs.

# Pin the builder to bookworm so its glibc matches the debian:bookworm-slim runtime below.
# (The default `rust:1.97` tag is trixie-based → glibc 2.38+, which bookworm-slim lacks.)
FROM rust:1.97-bookworm AS builder
WORKDIR /app

# Copy the whole workspace and build the requested binary in release mode.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

ARG BIN=api
RUN cargo build --release --bin ${BIN} && \
    cp target/release/${BIN} /app/service

FROM debian:bookworm-slim AS runtime
# ca-certificates for outbound HTTPS (CSV downloads, Axiom); curl for compose healthchecks.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/service /usr/local/bin/service
ENTRYPOINT ["/usr/local/bin/service"]
