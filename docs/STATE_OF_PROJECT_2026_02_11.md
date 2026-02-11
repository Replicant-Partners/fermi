# State of the Project
**February 11, 2026**

---

## Platform Overview

Agent Bestiary World is a live platform at **agent-bestiary.world** running on Railway with a Neon PostgreSQL database. The platform combines multi-model agent execution, episodic memory (ADM), knowledge graph evolution, workspace collaboration, an embedding marketplace, and AR spatial tooling. **35 agents**, **27 tools**, **40 migrations**, **28 templates**. Full OAuth2 authentication, credit system, coherence evaluation, and embedding projector. The AR Spatial Suite shipped Feb 10, enabling agents to place, map, and animate AR assets in physical space using H3 hexagonal grids.

---

## Agent Roster (35 Agents)

### Research & Intelligence (5)
- **macro_forecaster**: Fermi estimates, Bayesian reasoning, quantitative forecasting
- **market_research**: Industry analysis, competitive intelligence, trend identification
- **sentiment_analyzer**: Social media sentiment, brand perception, review analysis
- **entity_investigator**: OSINT, entity resolution, relationship mapping
- **monte_carlo_sim**: Monte Carlo simulation, probability distributions, risk modeling

### Creative & Media (8)
- **social_media_studio**: Multi-platform content generation, campaign planning
- **bluesky_publisher**: Bluesky post creation, thread generation, community engagement
- **instagram_publisher**: Instagram content, Stories, Reels, carousel posts
- **video_analyst**: Video content analysis, transcript generation, clip extraction
- **style_transfer**: Image style transfer, artistic transformation
- **watermark**: Watermark generation, brand protection
- **delivery**: Content delivery orchestration, multi-channel distribution
- **ar_card_producer**: 5-stage AR card production pipeline (intake → marker → video → AR → delivery)

### Games & Entertainment (2)
- **daily_puzzle**: Daily puzzle generation, difficulty calibration
- **xaman_ek**: Mayan calendar agent, ritual timing, astronomical alignment

### Meta & Coaching (4)
- **performance_coach**: Agent performance tuning, prompt optimization
- **publish_coach**: Agent publication guidance, documentation support
- **companion_builder_coach**: Companion agent design, relationship modeling
- **embedding_projector_guide**: PCA/t-SNE interpretation, cluster narration, drift detection

### Coherence & Coordination (5)
- **coherence_evaluator**: Thagard's TEC algorithm, constraint satisfaction scoring
- **coherence_consultant**: Conversational coherence improvement recommendations
- **intention_coordinator**: Multi-agent intention alignment, conflict resolution
- **cohere_and_coordinate**: Compound agent orchestrating coherence_evaluator + intention_coordinator
- **dream_narrator**: Post-consolidation narrative synthesis, ontology storytelling

### Marketplace & Commerce (4)
- **shopping_assistant**: Compound agent orchestrating preference_modeler + deal_finder + embedding_broker
- **preference_modeler**: Shopping profile creation, composite embedding generation
- **deal_finder**: Product discovery, price comparison, deal aggregation
- **embedding_broker**: Marketplace listing management, advertiser query handling

### AR Spatial Suite (3)
- **ar_beacon**: AR asset placement with H3 grid addressing, TTL, decay styles
- **ar_cartographer**: Spatial grid mapping, quadrant naming, zone grouping
- **ar_choreographer**: AR asset animation, macro/micro motion, choreography compilation

### Infrastructure & Integration (4)
- **ar_avatar_renderer**: Avatar generation, profile visualization
- **stripe_billing**: Stripe checkout, credit purchase, webhook handling
- **stripe_connect_advisor**: Stripe Connect onboarding guidance
- **micro_patron_template**: Creator patronage model template

---

## Tool Registry (27 Tools)

### Memory & Knowledge (2)
- **search_knowledge**: Similarity search over agent's episodic memory (pgvector cosine)
- **query_ontology**: Get rules/entities/facts from knowledge graph

### Agent Orchestration (4)
- **execute_agent**: Invoke another agent (single-turn, no recursion)
- **list_agents**: Discover available agents in registry
- **list_workspace_agents**: List agents in current workspace
- **delegate_to_agent**: Delegate task to workspace agent with full tool access

### Workspace Files (2)
- **read_workspace_file**: Read from workspace git repo
- **write_workspace_file**: Write to workspace git repo (auto-commit on events)

### Image Generation (2)
- **generate_image**: Text-to-image via Gemini Imagen
- **edit_image**: Image-to-image editing via Gemini

### Video & Media (5)
- **reduct_list_projects**: List Reduct.video projects
- **reduct_get_project**: Get project details with recordings and reels
- **reduct_get_transcript**: Get recording transcript (JSON with timestamps)
- **reduct_create_reel**: Create a new reel in a project
- **reduct_add_block**: Add a clip or title block to a reel

### Coherence Evaluation (3)
- **evaluate_coherence**: Run TEC evaluation on workspace messages
- **coherence_snapshot**: Get latest coherence evaluation
- **get_workspace_messages**: Read recent workspace conversation

### Embedding Marketplace (4)
- **get_shopping_profile**: Retrieve user's shopping preference profile
- **update_shopping_profile**: Recompute composite shopping embedding
- **list_marketplace**: Browse active marketplace listings
- **create_listing**: List a shopping profile on the marketplace

### AR Spatial Suite (5)
- **h3_resolve**: H3 hexagonal grid operations (gps_to_h3, h3_to_gps, neighbors, distance, grid_disk)
- **geocode**: Address to GPS coordinates via OpenStreetMap Nominatim (free, no API key)
- **create_beacon**: Create an AR beacon at an H3 cell with asset, orientation, TTL
- **query_beacons**: Find AR beacons near a location (spatial proximity search)
- **save_grid_map**: Persist a named spatial grid with quadrants and zones

---

## Database Schema (40 Migrations)

### Core Agent System
- **agents**: Agent registry (name, model, system_prompt, tools, visibility, dreaming_budget)
- **episodes**: Execution records with prompt/response/timing
- **episode_embeddings**: 1024-dim Voyage-2 vectors (pgvector)

### Knowledge Graph
- **semantic_rules**: Extracted rules with embeddings
- **entities**: Knowledge graph entities
- **facts**: Triple store (subject, predicate, object)
- **communities**: Semantic clusters (Louvain algorithm)
- **ontology_snapshots**: Point-in-time KG snapshots with dream_synopsis

### Consolidation
- **consolidation_jobs**: Background processing records
- **consolidation_locks**: Distributed lock table

### Auth & Economics
- **users**: OAuth2 user records (Google, GitHub)
- **api_keys**: Argon2-hashed API keys with prefix lookup
- **wallets**: Credit balances (personal + workspace)
- **ledger**: Append-only transaction log

### Workspaces
- **workspaces**: Multi-user collaboration spaces
- **workspace_members**: Role-based access (owner/admin/member)
- **workspace_agents**: Agent hiring records
- **workspace_messages**: Chat timeline
- **workspace_files**: Git-backed file references

### Coherence Engine
- **coherence_evaluations**: TEC constraint satisfaction scores
- **workspace_coherence_snapshots**: Point-in-time coherence state

### Marketplace
- **shopping_profiles**: Composite embeddings from shopping interactions
- **marketplace_listings**: Consumer-controlled embedding listings (price, status)
- **similarity_queries**: Advertiser query history

### Evaluation
- **eval_test_cases**: Test cases (auto-seeded from agent sample_queries)
- **eval_executions**: Execution records
- **eval_judgments**: LLM-as-judge scores
- **eval_results**: Aggregated pass/fail/latency

### AR Spatial Suite (NEW)
- **ar_beacons**: Placement records (H3 cell, asset_path, azimuth, elevation, TTL, decay_style, interaction triggers)
- **ar_choreographies**: Motion sequences (macro steps, micro keyframes, triggers, loop config)
- **ar_grid_maps**: Named spatial grids (center, resolution, radius, quadrants, zones)

### Admin & Ops
- **waitlist**: Signup queue with invitation tracking
- **platform_metrics**: Daily aggregated stats

---

## AR Spatial Suite (End-to-End Ready)

The AR Spatial Suite enables agents to place, map, and animate augmented reality assets at real-world locations. Built on **H3** (Uber's open-source hexagonal grid system), which provides free, offline, sub-meter precision spatial addressing.

### What's Live

**Three Agents:**
- **ar_beacon**: Place AR assets with GPS/H3 addressing, orientation (azimuth/elevation/billboard), TTL (1h-30d), decay styles (fade/dissolve/instant/loop_decay), interaction triggers (gaze/tap/proximity/dwell)
- **ar_cartographer**: Map physical spaces into named hexagonal grids with quadrants (directional/functional/landmark-based) and zones (grouped quadrants)
- **ar_choreographer**: Animate beacons with macro motion (cell-to-cell paths) and micro motion (XYZ keyframes within cell), 10 built-in actions (bounce/orbit/hover/pulse/wander/spiral/figure_eight/breathe/swarm/wave)

**Five Tools:**
- **h3_resolve**: 5 operations (gps_to_h3, h3_to_gps, neighbors, distance, grid_disk) at resolutions 9-15 (174m down to 0.5m)
- **geocode**: Street address to GPS via OpenStreetMap Nominatim
- **create_beacon**: DB write + asset path storage
- **query_beacons**: Spatial proximity search via H3 grid disk
- **save_grid_map**: Persist named grids

**Public API (unauthenticated):**
- `GET /api/beacons/nearby?lat=X&lng=Y&radius=3&resolution=12` - Discover active beacons
- `GET /api/beacons/:beacon_id` - Get single beacon record
- `GET /api/beacons/:beacon_id/asset` - Serve asset file (image/model/video)
- `GET /api/grid-maps/:map_id` - Get grid map with quadrants/zones

**Database Tables:**
- Migration 041: ar_beacons (H3 cell indexed), ar_choreographies (beacon FK), ar_grid_maps (workspace FK)

**Documentation:**
- Complete guide at `/docs/ar-spatial-suite` (7,000+ words)
- Covers: H3 architecture, agent workflows, tool reference, API endpoints, two how-to guides (digital graffiti, venue mapping)

### Flagship Use Case: Digital Graffiti

Place AR art at real-world locations with built-in expiry dates. Example workflow:

1. `@ar_beacon Place a neon "HELLO WORLD" sign floating at eye level at 51.5074, -0.1278, facing south, for 24 hours. Billboard mode, pulse on gaze.`
2. Agent generates image via Gemini, resolves GPS to H3 cell `8c2a1072b59ffff`, stores beacon with 24h TTL
3. Public API serves beacon to AR clients (WebXR apps, mobile, display glasses)
4. After 24h, beacon expires (fade decay over final 20% of lifetime)

### What's Missing

**AR Client Renderer:** The platform is a spatial content management system. Agents produce structured placement records, but there's no reference AR client yet. A future WebXR/mobile/glasses app would:
- Poll `/api/beacons/nearby` based on device GPS
- Fetch assets from `/api/beacons/:id/asset`
- Render at H3 cell center coordinates with orientation
- Play choreography sequences (interpolate keyframes)
- Handle interaction triggers (gaze tracking, proximity, tap)
- Respect TTL and decay styles

---

## API Surface

### Public (Unauthenticated)
- Agent execution: `POST /api/agents/:id/execute`
- Agent discovery: `GET /api/agents`, `GET /api/agents/:id`
- Ontology: `GET /api/agents/:id/ontology`, `/history`, `/snapshots/:id`, `/diff`
- AR beacons: `GET /api/beacons/nearby`, `GET /api/beacons/:id`, `GET /api/beacons/:id/asset`
- AR grid maps: `GET /api/grid-maps/:map_id`
- Documentation: `GET /docs/:slug`
- Metrics: `GET /api/metrics/platform`, `GET /api/metrics/agents/:id`
- Embedding projector: `GET /api/agents/:id/projections`, `GET /api/projections/bestiary`

### Authenticated (API Key or Session)
- Auth: `GET /api/auth/me`, `POST /api/auth/logout`, `/api/auth/api-keys` (CRUD)
- Credits: `POST /api/credits/purchase`, `GET /api/credits/balance`
- Agent lifecycle: `POST /api/agents/:id/fork`, `PUT /api/agents/:id`, `POST /api/agents/:id/publish`
- Workspaces: CRUD on `/api/workspaces`, `/api/workspaces/:id/members`, `/api/workspaces/:id/agents`
- Workspace chat: `POST /api/workspaces/:id/messages`, `GET /api/workspaces/:id/messages`
- Marketplace: `GET /api/marketplace`, `POST /api/marketplace/listings`, `POST /api/marketplace/query`
- Evaluation: `POST /api/agents/:id/eval/run`, `GET /api/agents/:id/eval/results`
- Consolidation: `POST /api/agents/:id/consolidate` (trigger dreaming cycle)

### Admin Only
- `GET /admin/dashboard`
- `GET /admin/ledger`
- `POST /admin/waitlist/invite`

---

## Recent Commits (Last 10)

| Commit | Date | Description |
|--------|------|-------------|
| **ab1da7a** | Feb 11 | Fix duplicate agentId declaration in ontology template |
| **a364a13** | Feb 10 | **AR Spatial Suite: H3 tools, beacon infrastructure, public API, docs** |
| **8f77185** | Feb 10 | Add AR spatial suite: ar_beacon, ar_cartographer, ar_choreographer |
| **43bc327** | Feb 10 | Prompt scaffolding + user secrets (4 phases) |
| **9c3652f** | Feb 10 | Cap tool results at 12k chars to prevent context overflow |
| **bb2a3f5** | Feb 9 | Fix duplicate messages in workspace chat |
| **b577c8d** | Feb 9 | Fix UUID showing as username in workflow diagrams |
| **d1201ba** | Feb 9 | Trigger rebuild for agent valence deployment |
| **a869944** | Feb 9 | Add agent valence model + workflow templates (Agent Molecules) |
| **791bc24** | Feb 9 | Add workflow visualization: auto-generated mermaid sequence diagrams |

**Key shipments (Feb 7-11):**
- AR Spatial Suite (3 agents, 5 tools, 3 tables, public API, full docs) - **Feb 10**
- Prompt scaffolding system (4-phase onboarding) - **Feb 10**
- Tool result truncation (12k char limit) - **Feb 10**
- Agent valence model (Agent Molecules collaboration framework) - **Feb 9**
- Workflow visualization (auto-generated Mermaid diagrams) - **Feb 9**
- Embedding projector (PCA/t-SNE, Three.js 3D visualization) - **Feb 8**
- ADM pipeline (episodes, embeddings, consolidation, KG) - **Feb 7-8**
- Auth system (Google/GitHub OAuth2, API keys, JWT sessions) - **Feb 7**

---

## What's Next

### Immediate Priorities

1. **AR Client Renderer (Proof-of-Concept)**
   - WebXR-based AR viewer that consumes `/api/beacons/nearby`
   - Render beacons at H3 cell coordinates with orientation
   - Basic choreography playback (interpolate keyframes)
   - Test digital graffiti use case end-to-end

2. **Agent Creation DSL via Zed (Design Session Needed)**
   - Goal: Zed as primary agent creation IDE with prompt-assisted DSL
   - Seed from `agents/templates/` (agent_card.json, PROMPT_ENGINEERING_GUIDE.md, DESIGN_CHECKLIST.md)
   - Key questions: Zed extension? LSP? DSL validation? Prompt assistance wiring? CLI scaffolding?

3. **Embedding Generation on Execution**
   - Episodes currently stored with `embedding: None`
   - Wire up `EmbeddingGenerator` in execution pipeline
   - Verify similarity search and projector work with real execution data

4. **Consolidation Trigger API**
   - `POST /api/agents/:id/consolidate` to run dreaming cycles on demand
   - Budget validation, lock acquisition, snapshot generation
   - Test end-to-end: execute → embeddings → consolidate → KG populated

### Known Gaps

**Infrastructure:**
- No `/api/version` endpoint (can't verify which commit is deployed)
- No CI/CD pipeline (manual Railway deploys via Redeploy button)
- 39 compiler warnings (unused variables, dead code)
- `base.html` is dead code (references nonexistent files, never served)

**Economic Model:**
- No refund mechanism for failed mid-execution charges
- Dependency auto-hire swallows errors (`let _ = charge_workspace_gas(...)`)

**Agent Execution:**
- Entity/fact extraction not wired up (consolidation extracts rules but not KG entities/facts)
- No streaming responses (full response buffered before return)

**AR Spatial Suite:**
- No reference AR client (data model complete, renderer missing)
- Choreography playback logic not implemented (agents compile, clients must interpret)

**Auth & Security:**
- SIWE (Sign In With Ethereum) stub exists but not wired up
- API rate limiting not implemented
- No team-based visibility model (private/shared/public + team sharing planned)

### Future Features

- **Scheduled Actions Framework**: Clean hooks for fee-incurring background tasks (consolidation, eval, etc.)
- **Workspace Teams**: Visibility model with private/shared/public + team sharing
- **Streaming Execution**: SSE or WebSocket for real-time agent output
- **Agent Discovery**: Search, filtering, tags, featured agents
- **Agent Analytics**: Time-series metrics, success rates, latency trends
- **Marketplace Enhancements**: Bidding, auction models, dynamic pricing
- **AR Templates**: Gallery, festival, office, retail, park, conference grid presets

---

## Platform Metrics

| Metric | Value |
|--------|-------|
| Rust LOC | ~49,000 |
| Handler LOC | ~9,000 |
| API Routes | 200+ |
| Agents | 35 |
| Tools | 27 |
| DB Migrations | 40 |
| HTML Templates | 28 |
| Static Assets | 22 |
| Commits (Feb 7-11) | 160+ |
| Crates in Workspace | 7 (fermi, fermi-auth, fermi-memory, fermi-lsp, agent-bestiary, projector, consolidate) |

---

## Architecture Patterns

### Multi-Model Execution
- **Anthropic protocol**: `tool_use` / `tool_result` blocks
- **OpenAI protocol**: `tool_calls` / `role: tool` messages
- Unified `ToolAwareExecutor` with dual-protocol handling
- Max 5 tool-use iterations per execution

### Memory (ADM Pipeline)
- Episodes stored with prompt, response, timing
- Embeddings generated via Voyage-2 (1024-dim)
- Similarity search via pgvector cosine distance
- Consolidation: LLM extracts rules, entities, facts → ontology snapshots

### Workspaces (Compositions)
- Git-backed file storage (auto-commit on events)
- Multi-agent orchestration with dependency auto-hire
- Coherence evaluation (TEC algorithm, tiered pricing)
- Dream narrator generates narrative after consolidation

### Credit System
- Wallets (personal + workspace)
- Append-only ledger with SELECT FOR UPDATE
- Gas fees configurable per action
- Stripe integration for credit purchase

### Embedding Marketplace (Reverse SEO)
- Shopping profiles: composite embeddings from chat interactions
- Weighted centroid (recency + success weighted, L2 normalized)
- Privacy-preserving: only similarity scores exposed, never raw embeddings
- Consumer-controlled listing (price, status, delist any time)

### AR Spatial Suite
- H3 hexagonal grid addressing (free, offline, sub-meter precision)
- Three-layer model: beacons (placement) → choreographies (motion) → grid maps (spatial organization)
- Public API for AR clients (no auth required)
- Database-first with H3 cell indexing for fast spatial queries

---

**End of State of Project - February 11, 2026**
