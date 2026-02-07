# Build stage
FROM rust:1.85 as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml ./
COPY rust-toolchain.toml ./

# Copy source code
COPY src ./src
COPY agent-bestiary ./agent-bestiary
COPY fermi-memory ./fermi-memory
COPY fermi-auth ./fermi-auth

# Downgrade incompatible dependencies for Rust 1.85
RUN cargo update time --precise 0.3.36 && \
    cargo update home --precise 0.5.9

# Build the api-server binary
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

# Copy templates directory
COPY templates /app/templates

# Copy agents directory
COPY agents /app/agents

# Copy ontologies directory
COPY ontologies /app/ontologies

# Create avatars cache directory
RUN mkdir -p /app/avatars_cache

# Set the PORT environment variable (Railway will override this)
ENV PORT=3000

# Expose the port
EXPOSE 3000

# Run the binary
CMD ["/app/api-server"]
