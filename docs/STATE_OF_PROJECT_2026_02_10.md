# State of the Project
**February 10, 2026**

---

## Summary

Agent Bestiary World is a live platform at **agent-bestiary.world** running on Railway with a Neon PostgreSQL database. In three days of intensive building (Feb 7-10), it went from a broken consolidation crate + no auth to a fully deployed multi-system platform with 32 agents, workspaces, an embedding marketplace, eval framework, and observability layer.

**154 commits** since Feb 7. ~49,000 lines of Rust. 32 migrations. 28 HTML templates. 22 static assets.

---

## What's Live and Working

### Core Platform
| System | Status | Notes |
|--------|--------|-------|
| Agent Registry | Live | 32 agents seeded from filesystem on startup |
| Agent Execution | Live | Multi-model: Anthropic, Mistral, Qwen, OpenRouter |
| Tool-Aware Execution | Live | 9 built-in tools, agentic loop (max 5 iterations) |
| ADM Pipeline | Live | Episodes, embeddings (Voyage-2), entity/fact extraction |
| Consolidation | Live | Dreaming cycles with LLM, budget-controlled |
| Knowledge Graph | Live | Entities, facts, rules, communities + 8 query API endpoints |
| Ontology Evolution | Live | Snapshots, diffs, dream synopses |

### Auth & Economics
| System | Status | Notes |
|--------|--------|-------|
| Auth | Live | Google + GitHub OAuth2, self-issued JWTs, API keys (Argon2) |
| Credit System | Live | Wallets + append-only ledger, SELECT FOR UPDATE |
| Gas Fees | Live | Configurable per-action, workspace + personal wallets |
| Stripe Integration | Live | Credit purchase checkout flow |
| Agent Lifecycle | Live | Fork, publish, version history, visibility controls |

### Workspaces (Compositions)
| System | Status | Notes |
|--------|--------|-------|
| Workspace CRUD | Live | Teams with roles (owner/admin/member) |
| Agent Hire/Add | Live | With dependency auto-hire, duplicate detection |
| Workspace Chat | Live | Message timeline with agent execution |
| Git-Backed Files | Live | Auto-commit on events, file read/write |
| Coherence Engine | Live | TEC (Thagard 1989), tiered pricing, auto-eval |
| Dream Narrator | Live | Auto-generates narrative after consolidation |

### Similarity Lab (Embedding Marketplace)
| System | Status | Notes |
|--------|--------|-------|
| Shopping Profiles | Live | Composite embeddings, weighted centroids |
| Marketplace Listings | Live | Consumer-controlled reverse-SEO |
| Similarity Matching | Live | pgvector cosine similarity, privacy-preserving |
| Marketplace Dashboard | Live | Query builder, results, history |
| 4 Marketplace Agents | Live | shopping_assistant, preference_modeler, deal_finder, embedding_broker |

### Observability & Eval
| System | Status | Notes |
|--------|--------|-------|
| Episode Detail API | Live | Iteration timeline, tool calls, timing |
| Platform Metrics | Live | Daily charts, tool frequency, stat cards |
| Agent Metrics | Live | 30-day sparkline, execution stats |
| Eval Framework | Live | Test cases (auto-seeded from sample_queries), LLM-as-judge |
| Regression Detection | Live | Pass rate, judge score, latency comparisons |

### Visualization
| System | Status | Notes |
|--------|--------|-------|
| Embedding Projector | Live | PCA dimensionality reduction, Three.js 3D point cloud |
| Temporal Projector | Live | Timeline scrubber, animated drift visualization |
| Projector Guide Agent | Live | Interprets clusters, narrates drift |

### Admin & Ops
| System | Status | Notes |
|--------|--------|-------|
| Admin Dashboard | Live | Agent management, ledger audit, waitlist |
| Waitlist System | Live | Email collection, bulk invite, status tracking |
| Documentation System | Live | Markdown docs served from manifest.json |

---

## The 32 Agents

### By Category

**Research**: macro_forecaster, market_research, sentiment_analyzer, entity_investigator, monte_carlo_sim
**Creative**: social_media_studio, bluesky_publisher, instagram_publisher, video_analyst, style_transfer, watermark, delivery
**Games**: daily_puzzle, xaman_ek
**Meta**: performance_coach, publish_coach, companion_builder_coach, embedding_projector_guide
**OSINT**: deal_finder
**Coherence**: coherence_evaluator, coherence_consultant, intention_coordinator, cohere_and_coordinate, dream_narrator
**Marketplace**: shopping_assistant, preference_modeler, embedding_broker
**Compound/Infra**: ar_avatar_renderer, ar_card_producer, stripe_billing, stripe_connect_advisor, micro_patron_template

### Compound Agents (Multi-Agent Pipelines)
- **shopping_assistant**: Orchestrates preference_modeler + deal_finder + embedding_broker
- **ar_card_producer**: 5-stage pipeline (intake, marker gen, video gen, AR scene, delivery)
- **cohere_and_coordinate**: Coordinates coherence_evaluator + intention_coordinator

---

## Architecture

```
Railway (us-west2)                    Neon (us-east)
+--------------------------+          +------------------+
| api-server (Axum)        |          | PostgreSQL       |
|   - 200+ routes          |--------->|   - pgvector     |
|   - Multi-model executor |  sqlx    |   - 32 migrations|
|   - Tool-aware executor  |          +------------------+
|   - Agent seeder         |
|   - Consolidation worker |
+--------------------------+
     |
     | serves
     v
+------------------+
| Static Assets    |
|   - 28 templates |
|   - nav.js       |
|   - theme.js     |
|   - auth.js      |
|   - 10 widgets   |
+------------------+
```

### Key Design Decisions
- **Standalone templates**: Each HTML file is self-contained (no base template inheritance)
- **DB-first agents**: Filesystem cards seeded to DB on startup; DB is source of truth at runtime
- **Charge-after-validate**: Gas/credit charges happen after confirming work exists (as of today's hardening)
- **Privacy-preserving marketplace**: Raw embeddings never exposed; only cosine similarity scores
- **Dual-protocol execution**: Anthropic (tool_use/tool_result) + OpenAI (tool_calls/role:tool)

---

## Reverse SEO / Similarity Lab — The Big Idea

The Similarity Lab (`/marketplace`) implements **reverse SEO** — a consumer-controlled alternative to surveillance advertising.

### How It Works
1. **Consumer builds a shopping profile** by chatting with the shopping_assistant about products they want
2. After 5+ interactions, the **preference_modeler** computes a composite embedding (weighted centroid of episode vectors, recency + success weighted, L2 normalized)
3. Consumer **lists the profile** on the marketplace at their chosen price
4. **Advertisers query** with product descriptions — the system returns only similarity scores (0.0-1.0), never raw embeddings
5. Consumer earns credits; can delist any time

### Why "Neighbors" Matters
The similarity matching isn't limited to shopping. The same infrastructure supports any "who is similar to me?" analysis:
- **Market research**: Which consumer profiles match this product concept?
- **Audience discovery**: Find market segments you didn't know existed
- **Competitive analysis**: How similar are two product positioning embeddings?
- **Content recommendation**: Which users' taste profiles align with this content?
- **Talent matching**: Skill embeddings vs role requirement embeddings

The embedding projector (Three.js 3D visualization) lets you **see** these neighborhoods — clusters of similar agents, drift over time, outliers. Combined with the projector guide agent that narrates what the clusters mean, it becomes an analytical tool, not just a marketplace.

### Full Documentation
See: `static/docs/embedding-marketplace.md` (served at `/docs/embedding-marketplace`)

---

## Known Issues

### Awaiting Deploy (fixes committed, Railway needs to pick them up)
- Docs 404 — route syntax fix (`{slug}` to `:slug`)
- Eval tx_type constraint — migration 032 (PgBouncer-safe)
- Workspace creation modal — replaces ugly `prompt()` dialog
- Agent hire/add duplicate detection — 409 Conflict instead of silent no-op
- Consolidation charge ordering — validates before charging gas

### Open Issues
- **11 agents not seeded in production DB** — needs redeploy to trigger seeder
- **No `/api/version` endpoint** — can't verify which commit is deployed
- **Dependency auto-hire swallows errors** — `let _ = charge_workspace_gas(...)` hides failures
- **No refund mechanism** — failed mid-execution charges are not refundable
- **`base.html` is dead code** — references nonexistent files, not served anywhere

### Technical Debt
- 39 compiler warnings (mostly unused variables, dead code)
- No CI/CD pipeline (manual Railway deploys)
- Limited test coverage (seed dataset exists but no integration test harness in CI)
- Theme CSS is partially inline (some templates) and partially in variables.css

---

## What Needs Testing

The platform has a lot of functionality that hasn't been exercised with real data yet:

1. **Execution pipeline end-to-end**: Execute an agent, verify episode + embedding stored, trigger consolidation, check KG populated
2. **Workspace collaboration**: Create workspace, hire agents, chat, verify coherence evaluation fires
3. **Similarity Lab flow**: Build shopping profile, list on marketplace, run advertiser query, verify credits flow
4. **Eval framework**: Run eval suite on an agent, verify judge scores, check regression detection
5. **Compound agents**: Execute ar_card_producer or shopping_assistant, verify tool delegation works
6. **Image generation**: Test generate_image and edit_image tools (Gemini integration)

---

## Metrics

| Metric | Value |
|--------|-------|
| Rust LOC | ~49,000 |
| Handler LOC | ~9,000 |
| API Routes | 200+ |
| DB Migrations | 32 |
| HTML Templates | 28 |
| Static Assets | 22 |
| Agent Cards | 32 |
| Commits (Feb 7-10) | 154 |
| Crates in Workspace | 7 (fermi, fermi-auth, fermi-memory, fermi-lsp, agent-bestiary, projector, consolidate) |

---

## Next Priorities

1. **Verify deploy** — confirm all committed fixes are live
2. **Test execution pipeline** — run a real agent, check embeddings + KG
3. **Test workspace flow** — hire agents, chat, verify coherence
4. **Test Similarity Lab** — end-to-end consumer/advertiser flow
5. **Add `/api/version`** — so we always know what's deployed
6. **Scheduled actions design** — clean hooks for fee-incurring background tasks (consolidation, eval, etc.)
