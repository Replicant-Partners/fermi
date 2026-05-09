# ─── Stage 1: Build the api-server binary ──────────────────────────
#
# We only copy the crates that api-server actually depends on.
# fermi-console and fermi-lsp are EXCLUDED because they depend on gpui
# (a GPU-accelerated UI framework that requires macOS Metal / system
# graphics libraries) and will never compile on a Linux Docker build.
# They are native-desktop-only crates and are NOT needed by the server.
#
FROM rust:1.85 AS builder
WORKDIR /app

# Copy workspace manifest and lock file first (layer-cache friendly).
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Copy all server-side source trees.
# Ordering: dependencies before dependents for better layer caching.
COPY src ./src
COPY fermi-auth ./fermi-auth
COPY fermi-memory ./fermi-memory

# Agent Bestiary crates (server-side: memory, evaluators, observability,
# coherence-gate, and the 5 Track B evaluators — all pure Rust, no GPU deps).
COPY agent-bestiary/memory            ./agent-bestiary/memory
COPY agent-bestiary/evaluators        ./agent-bestiary/evaluators
COPY agent-bestiary/observability     ./agent-bestiary/observability
COPY agent-bestiary/coherence-gate    ./agent-bestiary/coherence-gate
COPY agent-bestiary/evaluator-wildguard    ./agent-bestiary/evaluator-wildguard
COPY agent-bestiary/evaluator-faithfulness ./agent-bestiary/evaluator-faithfulness
COPY agent-bestiary/evaluator-sotopia      ./agent-bestiary/evaluator-sotopia
COPY agent-bestiary/evaluator-lifelong     ./agent-bestiary/evaluator-lifelong
COPY agent-bestiary/evaluator-character    ./agent-bestiary/evaluator-character
COPY agent-bestiary/ontology          ./agent-bestiary/ontology
COPY agent-bestiary/consolidate       ./agent-bestiary/consolidate
COPY agent-bestiary/projector         ./agent-bestiary/projector
COPY agent-bestiary/coherence/crates  ./agent-bestiary/coherence/crates

# SimOps crate (server-side).
COPY crates/simops ./crates/simops

# NOTE: fermi-lsp and crates/fermi-console are intentionally NOT copied.
# They depend on gpui and cannot compile on Linux.
# The workspace Cargo.toml lists them as members but cargo build --bin
# api-server does NOT compile workspace members that aren't dependencies
# of the target binary.
#
# We provide stub Cargo.toml files so the workspace resolver doesn't error
# on missing members. The stubs declare no source files — cargo sees them
# as empty library crates and skips them.
RUN mkdir -p fermi-lsp/src crates/fermi-console/src && \
    echo '[package]\nname = "fermi-lsp"\nversion = "0.1.0"\nedition = "2021"' > fermi-lsp/Cargo.toml && \
    echo '' > fermi-lsp/src/lib.rs && \
    echo '[package]\nname = "fermi-console"\nversion = "0.1.0"\nedition = "2021"' > crates/fermi-console/Cargo.toml && \
    echo '' > crates/fermi-console/src/lib.rs

# Build only the api-server binary in release mode.
RUN cargo build --release --bin api-server && \
    ls -la /app/target/release/ | grep api-server

# ─── Stage 2: Minimal runtime image ───────────────────────────────
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies.
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder.
COPY --from=builder /app/target/release/api-server /app/api-server

# Copy templates and static assets.
COPY templates /app/templates
COPY static /app/static

# Copy Flutter web build (Rabble SPA).
COPY rabble-web /app/static/rabble

# Copy agents directory.
COPY agents /app/agents

# Copy ontologies directory.
COPY ontologies /app/ontologies

# Copy migrations directory (for startup migration runner).
COPY migrations /app/migrations

# Create avatars cache directory.
RUN mkdir -p /app/avatars_cache

# Set the PORT environment variable (Railway will override this).
ENV PORT=3000

# Expose the port.
EXPOSE 3000

# Run the binary.
CMD ["/app/api-server"]
