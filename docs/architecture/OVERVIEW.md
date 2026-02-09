# Agent Bestiary -- Architecture Overview

Quick orientation for new developers. For memory-bank details see `MEMORY.md`.

## 1. System Overview

```
Browser / API client
        |
   agent-bestiary.world  (HTTPS, Railway custom domain)
        |
   Axum 0.7 web server   (src/api_server.rs, port 8080)
        |
   PostgreSQL 16          (Neon, US-East, pgvector enabled)
```

Railway auto-deploys from `main`. The single binary `api-server` boots Axum,
runs SQL migrations, seeds filesystem agent cards into the DB, and serves both
the JSON API and server-rendered HTML templates.

## 2. Workspace Crate Map

| Crate | Path | Purpose |
|---|---|---|
| **fermi** (lib + bins) | `.` | API server, FPL parser/AST, agent executor, MCP server |
| **agent-bestiary-memory** | `agent-bestiary/memory` | ADM store: episodes, rules, entities, facts, communities, embeddings, consolidation worker |
| **agent-bestiary-ontology** | `agent-bestiary/ontology` | Ontology snapshots, spacetime history, git-backed workspace manager |
| **agent-bestiary-projector** | `agent-bestiary/projector` | PCA/t-SNE dimensionality reduction, projection cache (DashMap, 5 min TTL) |
| **agent-bestiary-consolidate** | `agent-bestiary/consolidate` | Standalone consolidation binary (dreaming cycles) |
| **coherence-core** | `agent-bestiary/coherence/crates/coherence-core` | Shared types for multi-agent conversations |
| **coherence-engine** | `agent-bestiary/coherence/crates/coherence-engine` | Settling engine -- drives agent-to-agent deliberation |
| **coherence-observer** | `agent-bestiary/coherence/crates/coherence-observer` | Conversation observer, tracks coherence metrics |
| **coherence-api** | `agent-bestiary/coherence/crates/coherence-api` | HTTP routes for coherence subsystem |
| **coherence-protocols** | `agent-bestiary/coherence/crates/coherence-protocols` | Protocol definitions for multi-agent settling |
| **fermi-auth** | `fermi-auth` | OAuth2, JWT, API keys, SIWE, credits, teams, visibility |
| **fermi-memory** | `fermi-memory` | Legacy memory library (port 3001, being superseded by ADM) |
| **fermi-lsp** | `fermi-lsp` | Language server for FPL files |

## 3. Data Flow

### Agent loading (startup)

```
agents/curated/*.json  --seed-->  DB `agents` table
                                      |
AgentRegistry (in-memory index) <-----+
```

`AgentRegistry` scans `agents/curated/`, reads each `agent_card.json`, and
upserts rows into `agents` (idempotent by `agent_name`). At runtime
`list_agents()` and `get_agent()` query DB-first with filesystem fallback.

### Execution

```
POST /api/agents/:id/execute  { query }
  |
  v
LLMExecutor (Gemini / Anthropic / OpenRouter)
  |
  v
AgentOutput { response, confidence, token_usage }
  |
  +---> Episode stored in `episodes` table (with embedding if generator configured)
  +---> Gas fee charged to caller's wallet
  +---> Dreaming budget credit incremented on agent
```

### ADM Consolidation (dreaming)

```
ConsolidationWorker.run(agent_id)
  |
  +---> Cluster recent episodes (DBSCAN)
  +---> LLM extracts semantic rules from clusters
  +---> Upsert rules, entities, facts, communities
  +---> Create OntologySnapshot with dream_synopsis
  +---> Deduct 1 dreaming credit from agent budget
```

## 4. Auth Architecture

Handled entirely by the `fermi-auth` crate. No external auth provider.

| Layer | Mechanism |
|---|---|
| **Identity** | Google OAuth2 + GitHub OAuth2 (OIDC code flow) |
| **Session** | Self-issued HS256 JWT in `abw_session` HttpOnly cookie |
| **API access** | Bearer tokens with `ferm_` prefix, Argon2-hashed in DB |
| **Crypto** | SIWE (Sign In With Ethereum) -- stub wired, not yet live |
| **Authorization** | Visibility model: private / shared / public + team sharing |
| **Teams** | CRUD on teams, role-based membership (owner/admin/member) |

Auth middleware (`auth_middleware` / `optional_auth_middleware`) extracts
`AuthPrincipal` from cookie or API key and injects it into request extensions.

Key routes: `/auth/google`, `/auth/github`, `/auth/callback`, `/auth/logout`,
`/api/auth/me`, `/api/auth/api-keys`.

## 5. Credit System

Two-layer economic model defined in `src/gas.rs`:

**Layer 1 -- Platform credits (live)**

Users purchase credits via Stripe Checkout. Three tiers:

| Tier | Credits | Price | Discount |
|---|---|---|---|
| Starter | 100 | $5 | -- |
| Builder | 500 | $20 | 20% |
| Pro | 1000 | $35 | 30% |

Gas fee schedule (configurable via env vars):

| Action | Cost (credits) |
|---|---|
| Message send | 1 |
| Agent hire | 5 |
| Agent add | 2 |
| Execution | max(1, tokens/1000) + 10% gas surcharge |
| Consolidation cycle | 3 |
| File write | 1 |
| Avatar generate | 3 |
| Embedding import | 5 |

Wallets live in `wallets` table; every charge writes a row to
`credit_transactions` (full ledger). Low-balance warning at 10 credits.

**Layer 2 -- Crypto royalties (future)**

A 2.5% platform fee on agent-to-owner token transfers. Requires SIWE wallet
connection and a settlement layer (not yet implemented).

## 6. Key Abstractions

```rust
struct AppState {
    db: PgPool,                             // Neon Postgres connection pool
    memory_store: Arc<MemoryStore>,         // ADM read/write (episodes, rules, entities, ...)
    registry: Arc<AgentRegistry>,           // In-memory agent card index
    projection_engine: Arc<ProjectionEngine>, // PCA/t-SNE over embeddings
    projection_cache: Arc<ProjectionCache>,   // DashMap cache (5 min TTL)
    embedder: Arc<dyn EmbeddingGenerator>,    // Anthropic / Mistral / OpenAI / Qwen / Mock
    workspace_git: Arc<WorkspaceGitManager>,  // Git-backed ontology snapshots
    gemini_api_key: String,
    jwt_secret: String,
    oauth: OAuthConfig,
    gas_fees: GasFees,
    stripe: StripeConfig,
    rate_limits: RateLimitConfig,
}
```

`AppState` is the single source of truth passed to every Axum handler via
`State<AppState>`. It bridges World A (filesystem agent cards) and World B
(database ADM pipeline).

`MemoryStore` wraps `PgPool` and exposes typed queries for all ADM tables.
`EmbeddingGenerator` is trait-object'd so the provider (Anthropic, Mistral,
OpenAI, Qwen, or Mock) is chosen at startup.

## 7. Template System

Templates live in `templates/` and are **standalone HTML files** -- they do NOT
use a shared `base.html` layout. The server reads them with
`std::fs::read_to_string()`.

Key templates: `index.html`, `agent_detail.html`, `ontology.html`,
`projector.html`.

### Themes

Two themes, toggled via button or `Ctrl+T`, persisted in `localStorage`:

- **Hasui** (default) -- dark, Gruvbox-inspired palette
- **OP-1** -- light, Teenage Engineering aesthetic; uses colored dots instead of
  avatar images

Theme CSS is inlined in each template (not shared across files).
