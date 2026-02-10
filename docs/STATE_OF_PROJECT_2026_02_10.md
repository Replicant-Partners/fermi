# State of Project — 2026-02-10

## Platform Summary

Agent Bestiary is live at agent-bestiary.world. 254 commits, 16 workspace crates, ~55K lines of Rust, ~15K lines of HTML templates, 27 curated agents, 122 API routes, 27 SQL migrations, 17 built-in tools, 7 MCP tools, 231 tests.

---

## What's Built

### Core Infrastructure
- **Auth**: Google/GitHub OAuth2, self-issued HS256 JWTs (`abw_session` cookie), API keys with Argon2 hashing (`ferm_` prefix), scopes (read/write/execute/admin)
- **Credit system**: Wallets + append-only ledger, SELECT FOR UPDATE, 100 free credits on signup
- **Gas fees**: Configurable per-action (message 1cr, hire 5cr, add 2cr, execute varies, consolidate 3cr, file write 1cr, avatar 5cr, embedding import 5cr, eval run 2cr)
- **Database**: PostgreSQL on Neon (US East), 27 migrations, pgvector for embeddings, PgBouncer `statement_cache_capacity(0)` + `test_before_acquire(true)` + `DISCARD ALL`
- **Stripe**: Credit purchase flow, checkout sessions, webhooks, receipts
- **Railway**: Auto-deploy from main, custom domain agent-bestiary.world

### ADM Pipeline (Autonomous Declarative Memory)
- Execute -> Episodic memory -> Consolidation -> Semantic rules -> Ontology evolution
- Dream budget: per-agent credits for consolidation cycles
- Dream synopses: LLM-generated narratives after consolidation
- Ontology snapshots with spacetime index, diff API, version history

### Agent System (27 Curated Agents)
- **Research**: fermi (coordinator), market_research, monte_carlo_sim, macro_forecaster, sentiment_analyzer, video_analyst
- **Creative**: micro_patron_template, style_transfer, watermark, delivery
- **Social Media**: instagram_publisher, bluesky_publisher, social_media_studio (compound)
- **Games**: daily_puzzle
- **Meta**: performance_coach, companion_builder_coach, publish_coach, dream_narrator, embedding_projector_guide
- **OSINT**: entity_investigator
- **Coherence**: coherence_evaluator, coherence_consultant, cohere_and_coordinate, intention_coordinator
- **Billing**: stripe_billing, stripe_connect_advisor
- **System**: xaman_ek (platform navigator), ar_avatar_renderer

Agent features:
- Agent CRUD (POST/PUT/DELETE) with version history (snapshot-before-update, restore)
- Multi-provider model catalogue (Anthropic/Mistral/OpenRouter/Qwen) — DB fields stored, multi-model executor dispatches by provider
- Agent creation wizard: 5-step with import toggle, provider tabs, ontology seeds
- Agent forking with royalty tracking
- Custom embeddings import with dimension validation
- Lifecycle management (draft/published/deprecated/archived)
- Sample queries per agent (seeded into eval test cases)
- Curated + community tiers, filesystem cards upserted to DB on startup

### Execution System
- **MultiModelExecutor**: dispatches by provider (Anthropic native, others OpenAI-compatible)
- **ToolAwareExecutor**: agentic loop with max 5 iterations, dual-protocol (Anthropic tool_use/tool_result + OpenAI tool_calls/role:tool)
- **17 built-in tools**: search_knowledge, query_ontology, execute_agent, list_agents, read_workspace_file, list_workspace_agents, generate_image, edit_image, write_workspace_file, reduct_list_projects, reduct_get_project, reduct_get_transcript, reduct_create_reel, reduct_add_block, evaluate_coherence, coherence_snapshot, get_workspace_messages
- **Image generation**: Gemini `gemini-2.5-flash-image` (text-to-image + image-to-image editing)
- Token accumulation across turns, tool invocation tracking in episode JSONB

### Coherence System
- TEC engine (Thagard 1989) — deterministic core + LLM interpretive layer
- ConversationObserver: heuristic keyword classification
- SettlingEngine: iterative relaxation to stable coherence scores
- Tiered pricing: depth=index (free), recommendations (2cr), dream_notes (5cr)
- Auto-eval every Nth message (COHERENCE_AUTO_EVAL_INTERVAL env var)
- 3 coherence tools in built-in registry (evaluate_coherence, coherence_snapshot, get_workspace_messages)

### Embedding Projector
- PCA dimensionality reduction via linfa 0.7 + ndarray 0.15
- Three.js 3D visualization with OrbitControls, raycaster tooltips, source-type coloring
- Temporal scrubber with play/pause for drift animation
- DashMap cache with 5min TTL
- Per-agent + bestiary-wide projections
- "Ask Guide" panel calls embedding_projector_guide agent

### Workspaces
- Teams with budget, members (Owner/Admin/Member/Viewer roles)
- 3-panel UI: members sidebar | chat center | shelf right
- @ agent invocation with workspace context injection
- Workspace git repos: auto-commit on events, file browser, binary support, diff viewer
- Coherence shelf: TEC evaluation display, auto-eval trigger
- Slug collision prevention (timestamp suffix)

### Eval Framework
- eval_test_cases + eval_runs tables
- Background runner via tokio::spawn, reuses ToolAwareExecutor pipeline
- LLM-as-judge: Haiku scores relevance/accuracy/completeness (1-5)
- Regression detection: pass rate >10% drop, judge >0.5 drop, latency >50% increase
- Auto-seeded from agent sample_queries (42 cases across 14 agents)
- UI: "Eval Suite" button in owner panel, test case CRUD, run history with polling

### Observability
- Episode detail: clickable rows expand with iteration timeline, timing waterfall, tool calls
- Execution Activity on agent_detail: 30-day sparkline, tool usage bars
- Platform Activity on dashboard: stat cards, daily chart, tool frequency

### Web UI (26 templates, 15 static assets)
- **Public**: landing, aspiration, catalogue (world view), agent_detail, ontology, projector
- **Auth'd**: dashboard, workspace, agent_create, profile, settings, admin
- Dual themes: Hasui (dark, default) + OP-1 (light, Teenage Engineering inspired)
- Shared CSS variables + common.css + 12 JS widgets (nav, auth, theme, toast, modal, agent-card, fork-modal, tabs, tag-renderer, micro-chart, xaman-ek, api)
- Catalogue: world view (no pagination), compound/deck/system visual indicators, category filtering, complexity sorting

### MCP Server (7 tools)
- Stdio transport for Zed editor integration
- list_agents, get_agent, execute_agent, save_agent, search_agents, get_catalogue, ask_xaman_ek
- Documented in docs/shared/MCP_SETUP.md + docs/fermi/guides/mcp-guide.md

### Regression Test Seed
- 177 deterministic records: 3 agents, 75 episodes, 18 rules, 30 entities, 36 facts, 9 communities, 6 consolidation jobs
- 10 integration tests covering full ADM pipeline

---

## Codebase Metrics

| Metric | Count |
|--------|-------|
| Workspace crates | 16 |
| Rust LOC (core src/) | 22,872 |
| Rust LOC (memory crate) | 6,359 |
| Rust LOC (coherence crates) | 5,213 |
| Rust LOC (auth crate) | 2,568 |
| Rust LOC (ontology crate) | 1,939 |
| Rust LOC (projector crate) | 678 |
| **Total Rust LOC** | **~39,600** |
| HTML templates LOC | 14,772 |
| Static JS LOC | 1,392 |
| Static CSS LOC | 920 |
| SQL migrations | 27 |
| Curated agents | 27 |
| API routes | 122 |
| Built-in tools | 17 |
| MCP tools | 7 |
| Tests | 231 |
| Git commits | 254 |
| api_server.rs | 8,375 lines |

---

## What's Missing (Production Gaps)

### Tier 0: Core Execution Reliability
1. **Embedding generation on execution** — episodes stored with `embedding: None`, must call EmbeddingGenerator in the execute pipeline
2. **Multi-model execution beyond Anthropic** — DB fields exist for Mistral/OpenRouter/Qwen, MultiModelExecutor scaffolded, but non-Anthropic paths untested in production
3. **Consolidation trigger endpoint** — `POST /api/agents/:id/consolidate` to run dreaming cycles on demand
4. **Entity/fact extraction in consolidation** — consolidation extracts rules but not KG entities/facts yet

### Tier 1: Money In
5. **Stripe Connect for creator payouts** — stripe_connect_advisor agent exists as guide, actual Connect integration not wired
6. **Credit top-up UX** — "Buy More" buttons wherever balance is shown, low-balance warnings
7. **SIWE wallet connection** — stub exists in fermi-auth/src/siwe.rs, not wired to UI

### Tier 2: Discovery & Engagement
8. **Full-text search** — catalogue has client-side category filtering but no server-side text search
9. **Notifications** — migration 021 exists, no handler/UI
10. **Agent detail actions** — execution history tab, budget top-up from detail page

### Tier 3: Platform Safety
11. **Rate limiting** — no per-user/per-endpoint throttling
12. **Content moderation** — no agent output filtering
13. **Admin tools** — admin.html template exists, minimal backend

### Tier 4: Polish
14. **Mobile nav** — hamburger menu, consistent breakpoints
15. **Error pages** — 404.html/500.html exist, not always served correctly
16. **Analytics** — no PostHog/Plausible integration

---

## Architecture Health

| Component | Health | Notes |
|-----------|--------|-------|
| Core engine (FPL, agents) | 9/10 | Solid, well-tested |
| Auth system | 8/10 | OAuth + API keys working, SIWE stubbed |
| Credit/gas system | 8/10 | Working, ledger append-only, gas configurable |
| Database layer | 7/10 | PgBouncer issues fixed, needs connection pool tuning |
| ADM pipeline | 7/10 | Consolidation works, entity extraction missing |
| Execution pipeline | 8/10 | Tool-aware loop working, multi-model scaffolded |
| Coherence system | 8/10 | Full TEC engine, tools in registry, auto-eval |
| Workspace system | 7/10 | Functional, slug fix applied, needs stress testing |
| Web UI | 7/10 | 26 templates, dual themes, world view catalogue |
| Eval framework | 7/10 | Judge + regression detection, needs more test cases |
| MCP server | 8/10 | 7 tools, Zed integration documented |
| Observability | 6/10 | Episode detail + metrics, needs structured logging |
| Test coverage | 5/10 | 231 tests, heavy on integration, light on unit |

---

## Sprint History

| Sprint | Focus | Key Deliverables |
|--------|-------|------------------|
| A-C | Foundation | Stripe, profile, settings |
| D-F | Agent detail | Search, notifications stubs, admin |
| G | Visual identity | Hasui/OP-1 themes, Gruvbox palette |
| H | Agent categories | 6 categories, tag-based filtering |
| I | Landing page | Public hero, route restructure (/ = landing, /catalogue = index) |
| J | Coherence | TEC engine, observer, settling, auto-eval, shelf |
| K | Platform depth | Static assets, agent wizard, version history, multi-model, projector |
| L | Agent creation | 5-step wizard, multi-provider catalogue, ontology seeds |
| M | Tool execution | ToolAwareExecutor, 9 built-in tools, dual-protocol |
| N | Observability | Episode detail, execution activity, platform metrics |
| O | Eval framework | Test cases, background runner, LLM judge, regression detection |
| P | Coherence tools | evaluate_coherence, coherence_snapshot, get_workspace_messages |
| Q | Compound agents | social_media_studio, artist deck (style_transfer + watermark + delivery) |
| R | Social publishing | instagram_publisher, bluesky_publisher, cross-posting |
| S | Image tools | generate_image, edit_image (Gemini), write_workspace_file binary support |
| T | Platform ops | API key UI fixes, catalogue world view, PgBouncer fix, workspace slug fix |
| U | Public face | Aspiration page, hero ownership messaging, Xaman Ek + MCP expansion |
