# Project Glossary

Canonical definitions for acronyms and terms used across the Fermi / Agent Bestiary codebase.

---

## Three-Letter Acronyms

| TLA | Full Name | Description |
|-----|-----------|-------------|
| **ADM** | Active Dream Memory | The core memory pipeline: episodes in, consolidation (dreaming), knowledge graph out. Not "Autonomous Declarative Memory" or "Active Data Model" or "Active Dreaming Memory" (close but not quite). |
| **AKP** | Agent Knowledge Protocol | (Future) Cross-agent knowledge sharing protocol. |
| **TEC** | Theory of Explanatory Coherence | Thagard 1989. The constraint-satisfaction algorithm behind the coherence engine. |
| **KG** | Knowledge Graph | Entities, facts, rules, communities extracted during consolidation. |
| **FPL** | Fermi Prediction Language | The DSL for writing forecasting models. |
| **MCP** | Model Context Protocol | Anthropic's protocol for tool/resource integration. |
| **SIWE** | Sign In With Ethereum | Web3 auth standard. Stub exists, not wired up. |
| **LSP** | Language Server Protocol | Powers Zed/editor integration for FPL. |

## Key Terms

| Term | Definition |
|------|------------|
| **Agent Card** | JSON manifest defining an agent's identity, capabilities, tools, model, and system prompt. Lives in `agents/curated/{name}/agent_card.json`. |
| **Bestiary** | The agent registry/catalogue. The collection of all available agents. |
| **Catalogue** | The public-facing agent listing page at `/catalogue`. |
| **Coherence** | Workspace-level evaluation using TEC. Measures how well agents' outputs align. |
| **Composition** | A workspace — a team of agents collaborating. User-facing name for "workspace." |
| **Consolidation** | The "dreaming" process: clustering episodes, extracting rules/entities/facts, building KG. Costs dreaming credits. |
| **Dream Narrator** | Agent that generates narrative summaries after consolidation cycles. |
| **Dream Synopsis** | The narrative output from the dream narrator, stored on ontology snapshots. |
| **Dreaming Budget** | Per-agent credit allocation for consolidation cycles. |
| **Episode** | A single agent execution record: input, output, tokens, timing, embedding. |
| **Gas Fee** | Per-action platform fee charged from wallet (separate from credit cost). |
| **Ontology** | The evolved knowledge structure for an agent: rules, entities, facts, communities. |
| **Reverse SEO** | The Similarity Lab's model: consumers control their preference profiles, advertisers pay to query similarity scores. |
| **Seeder** | Startup process that upserts filesystem agent cards into the database. |
| **Settling Engine** | The TEC implementation. Iteratively activates/deactivates propositions until coherence stabilizes. |
| **Similarity Lab** | The embedding marketplace at `/marketplace`. Consumer profiles, advertiser queries, cosine similarity. |
| **Snapshot** | A point-in-time capture of an agent's ontology state. |

## Platform Layers

| Layer | What It Does |
|-------|-------------|
| **Layer 1 (Credits)** | Platform economy. Gas fees, execution charges, marketplace transactions. Live. |
| **Layer 2 (Crypto)** | Future. Agent royalties, on-chain settlement, 2.5% tx fee. |

## Agent Categories

| Category | Examples |
|----------|---------|
| **Research** | macro_forecaster, market_research, sentiment_analyzer |
| **Creative** | social_media_studio, style_transfer, video_analyst |
| **Games** | daily_puzzle, xaman_ek |
| **Meta** | performance_coach, publish_coach, embedding_projector_guide |
| **OSINT** | deal_finder |
| **Coherence** | coherence_evaluator, dream_narrator, intention_coordinator |
| **Marketplace** | shopping_assistant, preference_modeler, embedding_broker |
