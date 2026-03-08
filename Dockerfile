# ─── Stage 1: Build the api-server binary ──────────────────────────
FROM rust:1.85 AS builder
WORKDIR /app

# Copy everything needed for the build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY agent-bestiary ./agent-bestiary
COPY fermi-memory ./fermi-memory
COPY fermi-auth ./fermi-auth
COPY fermi-lsp ./fermi-lsp
COPY crates ./crates

# Build only the api-server binary in release mode
RUN cargo build --release --bin api-server && \
    ls -la /app/target/release/ | grep api-server

# ─── Stage 2: Minimal runtime image ───────────────────────────────
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
