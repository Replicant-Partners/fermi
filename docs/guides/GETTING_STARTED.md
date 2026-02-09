# Getting Started with Agent Bestiary

## Prerequisites

- Rust 1.75+ (with cargo)
- PostgreSQL 16 with pgvector extension (or Neon hosted)
- Node.js 18+ (optional, for frontend dev)

## Quick Setup

### 1. Clone and configure

```bash
git clone https://github.com/Replicant-Partners/fermi.git
cd fermi
cp .env.example .env  # Edit with your values
```

### 2. Required environment variables

At minimum, set these in `.env`:

```bash
DATABASE_URL=postgresql://user:pass@host/dbname?sslmode=require
JWT_SECRET=your-secret-here
ANTHROPIC_API_KEY=sk-ant-...
```

See [docs/operations/ENV_VARS.md](../operations/ENV_VARS.md) for the full list.

### 3. Run migrations

Migrations run automatically on server startup. To run manually:

```bash
cargo run --bin api-server
# Migrations execute on boot, then server starts on port 3000
```

### 4. Start the server

```bash
cargo run --bin api-server
```

Visit `http://localhost:3000` to see the agent catalogue.

## Key URLs

| Path | Description |
|------|-------------|
| `/` | Agent catalogue (public) |
| `/dashboard` | User dashboard (requires login) |
| `/agents/new` | Create a new agent |
| `/workspace/:id` | Workspace view |
| `/projector` | Embedding projector (3D visualization) |
| `/admin` | Admin panel (admin role required) |
| `/profile` | User profile |
| `/settings` | Account settings, API keys |

## Authentication

Three auth methods are supported:

1. **Google OAuth** - Set `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET`
2. **GitHub OAuth** - Set `GITHUB_CLIENT_ID` + `GITHUB_CLIENT_SECRET`
3. **Ethereum (SIWE)** - Connect wallet via MetaMask

New users receive 100 free credits on signup.

## Creating an Agent

1. Go to `/agents/new`
2. Fill in name, description, system prompt
3. Choose an LLM provider and model
4. Set visibility (public/private/shared)
5. Optionally allocate dream budget for ADM consolidation

Or import an existing `agent_card.json` via the Import tab.

## Credit System

| Action | Cost |
|--------|------|
| Agent execution | 1 credit per 1000 tokens + 10% gas |
| Workspace chat | 1 credit |
| Hire agent to workspace | 5 credits |
| Add agent to workspace | 2 credits |
| Generate avatar | 3 credits |
| Import embeddings | 5 credits |
| Manual coherence eval | 2 credits |

Buy more credits via Stripe on the dashboard (if configured).

## Running Tests

```bash
# Integration tests (requires DATABASE_URL)
cargo test --test api_tests -- --test-threads=1

# Memory store seed tests
cargo test -p agent-bestiary-memory --test test_seed -- --test-threads=1

# All unit tests
cargo test
```

## Project Structure

```
fermi/
  src/
    api_server.rs      # Main API server (Axum)
    gas.rs             # Gas fee configuration
    main.rs            # CLI entry point
  agent-bestiary/
    memory/            # ADM memory store (PostgreSQL)
    ontology/          # Ontology management + workspace git
    projector/         # PCA/t-SNE embedding projections
    consolidate/       # Dreaming/consolidation worker
    coherence/         # TEC coherence evaluation
  fermi-auth/          # Auth, credits, teams, visibility
  templates/           # Standalone HTML templates
  migrations/          # SQL migrations (run on startup)
  agents/curated/      # Filesystem agent cards
  docs/                # Documentation
  tests/               # Integration tests
```

See [docs/architecture/OVERVIEW.md](../architecture/OVERVIEW.md) for detailed architecture.
