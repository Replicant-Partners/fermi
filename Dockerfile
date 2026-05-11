# ─── Stage 1: Build the api-server binary ──────────────────────────
#
# fermi-console and fermi-lsp are workspace members that depend on gpui,
# which requires macOS Metal / Cocoa / CoreFoundation system frameworks.
# Even though `cargo build --bin api-server` does NOT compile those crates,
# cargo still resolves their full dependency graph and some of those crates
# (cocoa, core-graphics, cbindgen, bindgen) have build.rs scripts that
# fail or download things that don't exist on Linux.
#
# Solution: remove fermi-console and fermi-lsp from the workspace members
# list in a server-side copy of Cargo.toml before running the build.
# The sed command strips those two entries from the members array.
# All other workspace members (simops, agent-bestiary/*) are pure Rust
# and compile cleanly on Linux.
#
FROM rust:1.85 AS builder
WORKDIR /app

# Copy everything (nixpacks-style — simpler and correct).
COPY . .

# Strip the desktop-only workspace members from Cargo.toml so cargo
# never touches their dependencies. Uses a simple in-place sed that
# removes the exact line added by the console commits.
# The line is: "    \"fermi-lsp\", \"crates/fermi-console\","
# After removal the workspace members list is still valid.
RUN sed -i '/"fermi-lsp", "crates\/fermi-console"/d' Cargo.toml && \
    sed -i '/"crates\/fermi-console"/d' Cargo.toml && \
    sed -i '/^    "fermi-lsp",$/d' Cargo.toml

# Also remove the desktop crate path dependencies from [dependencies]
# (fermi-console is not a [dependency], but fermi-lsp might add one later).
# This is a no-op if the lines aren't present.

# Build only the api-server binary in release mode.
RUN cargo build --release --bin api-server

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
# rabble-web is tracked in git (39 files); static/rabble/ is gitignored
# because it's a Flutter build artifact copied from rabble-web at deploy time.
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
