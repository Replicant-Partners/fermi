# ─── Stage 1: Chef prepare (generate dependency recipe) ────────────
FROM rust:1.86 AS chef
RUN cargo install cargo-chef@0.1.68 --locked
WORKDIR /app

# Copy manifests and source structure for recipe generation
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY agent-bestiary ./agent-bestiary
COPY fermi-memory ./fermi-memory
COPY fermi-auth ./fermi-auth
COPY fermi-lsp ./fermi-lsp
COPY crates ./crates

RUN cargo chef prepare --recipe-path recipe.json

# ─── Stage 2: Chef cook (build dependencies only — cached) ────────
FROM rust:1.86 AS deps
RUN cargo install cargo-chef@0.1.68 --locked
WORKDIR /app

COPY --from=chef /app/recipe.json recipe.json
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Cook: builds all dependencies but NOT our source code
# This layer is cached until Cargo.toml/Cargo.lock change
RUN cargo chef cook --release --recipe-path recipe.json

# ─── Stage 3: Build our source code (fast — deps already compiled) ─
FROM deps AS builder

# Copy actual source code
COPY src ./src
COPY agent-bestiary ./agent-bestiary
COPY fermi-memory ./fermi-memory
COPY fermi-auth ./fermi-auth
COPY fermi-lsp ./fermi-lsp
COPY crates ./crates

# Build the api-server binary (dependencies already compiled in stage 2)
RUN cargo build --release --bin api-server && \
    ls -la /app/target/release/ | grep api-server

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/api-server /app/api-server

# Copy templates and static assets
COPY templates /app/templates
COPY static /app/static

# Copy Flutter web build (single source of truth for Rabble SPA)
COPY rabble-web /app/static/rabble

# Copy agents directory
COPY agents /app/agents

# Copy ontologies directory
COPY ontologies /app/ontologies

# Copy migrations directory (for startup migration runner)
COPY migrations /app/migrations

# Create avatars cache directory
RUN mkdir -p /app/avatars_cache

# Set the PORT environment variable (Railway will override this)
ENV PORT=3000

# Expose the port
EXPOSE 3000

# Run the binary
CMD ["/app/api-server"]
