# Rabble.world: Investment Memo V2 — What We Built vs. What We Projected

*Replicant Partners — February 2026 (Updated)*

---

## Preface: Why This Update

The original Rabble investment memo (February 2026, V1) was written when the app had 10 screens, basic creature minting, and AR viewing. It projected a roadmap, economics, and market strategy.

This document revisits those projections against what has actually been built. It also incorporates a significant architectural insight that emerged during development: the Rabble/ABW infrastructure is domain-agnostic and applies directly to supply chain management, environmental monitoring, fleet tracking, and any domain requiring autonomous agents that learn from spatially-indexed sensor data.

---

## I. What We Projected vs. What We Built

### Original MVP Gaps (from V1 "What's Next")

| Feature                  | V1 Status               | Current Status | Notes                                                                                       |
| ------------------------ | ----------------------- | -------------- | ------------------------------------------------------------------------------------------- |
| Profile/account screen   | "1 day"                 | **Built**      | AuthService with profile fetch, displayName, role, email, avatar                            |
| Wallet balance UI        | Not mentioned as screen | **Built**      | Full wallet_screen.dart: balance card, 3 billing tiers, transaction history, cost reference |
| Push notifications       | "2 days"                | Not built      | Deferred — SSE broadcast in place for real-time                                             |
| Creature trading/gifting | "3 days"                | Not built      | Deferred                                                                                    |
| Flight analytics         | "2 days"                | Partial        | Flight map with paths, location counts, but no heatmap                                      |
| Creature evolution       | "1 week"                | Not built      | Deferred                                                                                    |
| Sound design             | "1 week"                | Not built      | Voice synthesis tool exists (Cartesia Sonic) but not integrated in Rabble                   |

### Features Built That V1 Didn't Anticipate

| Feature                   | What It Does                                                              | Business Impact                                                                     |
| ------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| **Creature CRUD**         | Rename, archive, restore, retire creatures                                | Lifecycle management — creatures are now manageable assets, not one-shot mints      |
| **Social contacts layer** | Asymmetric follow model, user search, nickname, invite to private rabbles | Network effects accelerator — the "find your friends" missing from V1               |
| **Device pairing**        | AirTag/SmartTag/GPS tracker paired with creatures                         | **The supply chain bridge** — physical things track as creature flights             |
| **Admin dashboard**       | 4-tab admin (Stats, Users, Creatures, Swarms) with moderation tools       | Platform governance — flag creatures, cancel swarms, grant credits, user management |
| **Help & onboarding**     | 8-section expandable guide, getting started flow, contextual empty states | Retention — new users actually understand what to do                                |
| **Status filtering**      | Active/Archived/All filter on collections                                 | UX quality — collections don't become unusable at scale                             |
| **Cost hints**            | Credit costs shown on mint/fly/create buttons                             | Conversion — users understand the economy before acting                             |
| **65-test widget suite**  | Full test coverage across all screens with MockClient                     | Engineering velocity — confident refactoring, regression detection                  |
| **SOSA observation API**  | W3C SSN vocabulary sensor ingestion with auto-analysis                    | **The IoT bridge** — any sensor feeds the learning pipeline                         |
| **Swarm telemetry**       | GPS/accelerometer batch ingestion with auto-analysis agent                | Spatial intelligence generation at scale                                            |

### Infrastructure Growth

| Metric                 | V1 State        | Current State          |
| ---------------------- | --------------- | ---------------------- |
| Flutter screens        | 10              | **15**                 |
| Backend handlers       | ~15 (estimated) | **30 handler modules** |
| Database migrations    | ~40             | **56**                 |
| Agents (filesystem)    | ~25             | **41**                 |
| Agents (catalogue API) | ~25             | **34**                 |
| Platform tools         | ~9              | **30**                 |
| Test files             | 1               | **13 files, 65 tests** |
| API client methods     | ~15             | **50+**                |

---

## II. Economics Update

### V1 Projections vs. Reality

The original memo projected these credit costs:

| Action         | V1 Projected | Actual | Delta |
| -------------- | ------------ | ------ | ----- |
| Mint creature  | 3 cr         | 3 cr   | Same  |
| Art generation | 5 cr         | 5 cr   | Same  |
| Record flight  | 3 cr         | 3 cr   | Same  |
| Create rabble  | 5+ cr        | 5 cr   | Same  |
| Chat message   | 1 cr         | 1 cr   | Same  |

The gas fee schedule was fully implemented as projected. What changed is the **breadth** of chargeable operations — the original memo didn't price these:

| New Chargeable Action        | Credits           | V1 Anticipated?          |
| ---------------------------- | ----------------- | ------------------------ |
| Swarm telemetry ingest       | 1 cr/batch        | No                       |
| Observation session create   | 2 cr              | No                       |
| Observation ingest           | 1 cr/batch        | No                       |
| Voice synthesis              | 2 cr              | No                       |
| Image generation (via tools) | 3 cr              | Partially (art gen only) |
| Image editing                | 3 cr              | No                       |
| Marketplace listing          | 3 cr              | Mentioned but not priced |
| Marketplace match            | 1 cr base + price | Mentioned but not priced |
| Eval run                     | 2 cr              | No                       |
| Consolidation cycle          | 3 cr              | Mentioned                |

**Net impact**: More revenue per active user than V1 projected. The typical monthly spend estimate of 60-150 credits was for mint+fly+rabble+chat only. Adding telemetry, observations, and marketplace interactions could push active power users to 200-300 credits/month, raising effective ARPU.

### Billing Tiers — Implemented as Projected

| Pack         | Credits | Price  | Per Credit |
| ------------ | ------- | ------ | ---------- |
| Starter      | 100     | $9.99  | $0.100     |
| Professional | 500     | $39.99 | $0.080     |
| Enterprise   | 1,000   | $69.99 | $0.070     |

Note: prices shifted upward from V1 projections ($5/$20/$35 → $9.99/$39.99/$69.99). Per-credit cost is ~2x V1 estimates. This affects ARPU calculations:

- **V1 projected ARPU**: $2-4/mo (phone AR), $6-8/mo (glasses AR)
- **Updated ARPU at new pricing**: $4-8/mo (phone AR), $12-20/mo (glasses AR) — assuming same credit consumption

This approximately doubles the revenue projections in all scenarios, or allows the same revenue at half the user count.

### Cost Structure — Validated

| Cost            | V1 Estimate (1K MAU) | Actual (pre-launch) |
| --------------- | -------------------- | ------------------- |
| Railway hosting | $50                  | $5/mo (current)     |
| Neon PostgreSQL | $20                  | $0/mo (free tier)   |
| Anthropic API   | $100                 | Pay-per-call        |
| Gemini API      | $50                  | Pay-per-call        |
| Stripe fees     | $30                  | 2.9% + 30¢          |
| **Total**       | **$250**             | **~$10/mo**         |

Pre-launch costs are negligible. The scaling estimates in V1 appear conservative — actual hosting costs will likely be lower than projected at 1K-10K MAU due to Rust's efficiency (single Railway instance handles substantial load).

---

## III. The Supply Chain Insight

This is the most significant development since V1. During the implementation of device pairing and the SOSA observation API, we realized that the Rabble/ABW architecture maps directly to adaptive supply chain management:

### The Direct Mapping

| ABW/Rabble Concept            | Supply Chain Equivalent                                                |
| ----------------------------- | ---------------------------------------------------------------------- |
| Creature                      | Shipment / Asset / Package                                             |
| Flight                        | Transit leg / Route segment                                            |
| Swarm                         | Cross-dock / Consolidation point / Delivery hub                        |
| Device pairing (AirTag/GPS)   | RFID tag / GPS tracker on container                                    |
| SOSA observation              | Temperature, humidity, shock, location sensor reading                  |
| H3 spatial grid               | Warehouse zones, geofences, delivery routes                            |
| Consolidation (dream cycle)   | Pattern extraction: "Tuesday shipments from Supplier X spoil 3x more"  |
| Knowledge graph               | Supplier network, route performance, risk factors                      |
| Coherence evaluation          | Is the logistics plan internally consistent?                           |
| Embedding marketplace         | "Which carriers match our reliability requirements?"                   |
| Credit economy                | Cost attribution per operation, per team, per shipment                 |
| Agent composition (workspace) | Logistics team: route optimizer + customs advisor + spoilage predictor |

### What This Means for Market Sizing

V1 sized the market purely as consumer AR (nature → education → sports → brands). The supply chain application opens an entirely different market:

| Market                  | Estimated Global Size        | ABW Entry Point                                       |
| ----------------------- | ---------------------------- | ----------------------------------------------------- |
| Supply chain visibility | $3.6B (2025), growing 15%/yr | SOSA API + device pairing + spatial analytics         |
| Cold chain monitoring   | $8.1B (2025), growing 19%/yr | Temperature observations + spoilage pattern learning  |
| Fleet management        | $25B (2025), growing 14%/yr  | GPS device pairing + route intelligence + H3 indexing |
| IoT platform services   | $80B+ (2025)                 | Domain-agnostic sensor ingestion + agent analytics    |

Even at 0.01% penetration, the B2B supply chain opportunity dwarfs the consumer AR opportunity in revenue per customer (enterprise contracts vs. $8/mo consumer ARPU).

### The Dual-Market Strategy

Rabble and supply chain are not competing priorities. They share 100% of the infrastructure:

| Layer                    | Shared? | Notes                                                    |
| ------------------------ | ------- | -------------------------------------------------------- |
| Agent execution pipeline | Yes     | Same ToolAwareExecutor, same multi-model dispatch        |
| Credit/wallet economy    | Yes     | Same Stripe checkout, same ledger                        |
| SOSA observation API     | Yes     | Designed for any W3C SSN sensor                          |
| H3 spatial indexing      | Yes     | Same resolution, same tools                              |
| Device pairing           | Yes     | Same creature_devices table, just different device_types |
| Consolidation/ADM        | Yes     | Same dream cycle extracts patterns from any domain       |
| Knowledge graph          | Yes     | Same entity-fact-rule structure                          |
| Coherence evaluation     | Yes     | Same TEC engine                                          |
| Embedding marketplace    | Yes     | Same cosine similarity, same privacy model               |

**The consumer app (Rabble) generates volume and validates the platform. The enterprise application (supply chain) generates revenue and validates the business model.**

---

## IV. Revised Scenario Projections

### V1 Scenarios (Consumer AR Only)

| Scenario    | 5-Year Revenue | Break-Even |
| ----------- | -------------- | ---------- |
| Pessimistic | $586K          | ~2029      |
| Base Case   | $10M           | Early 2028 |
| Optimistic  | $71M           | Late 2027  |

### V2 Scenarios (Consumer AR + Supply Chain)

#### Scenario A: Pessimistic — Consumer Niche + No Enterprise Traction

AR stays niche. Nature enthusiasts only. Enterprise supply chain doesn't materialize. But higher credit pricing (2x V1) improves unit economics.

| Year | Consumer MAU | Enterprise | MRR   | Annual Rev |
| ---- | ------------ | ---------- | ----- | ---------- |
| 2026 | 200          | 0          | $800  | $10K       |
| 2027 | 800          | 0          | $4.8K | $58K       |
| 2028 | 2,000        | 0          | $12K  | $144K      |
| 2029 | 4,000        | 1 pilot    | $24K  | $288K      |
| 2030 | 6,000        | 2 pilots   | $40K  | $480K      |

5-year cumulative: ~$980K (vs. V1's $586K). The 2x credit pricing lifts all scenarios.

#### Scenario B: Base Case — Consumer Growth + First Enterprise Contracts

AR glasses arrive 2028. Nature + education verticals. 2-3 supply chain pilot contracts by 2029 ($5K-15K/mo each). The enterprise revenue is modest but validates the B2B model.

| Year | Consumer MAU | Enterprise Contracts | Consumer MRR | Enterprise MRR | Total Annual |
| ---- | ------------ | -------------------- | ------------ | -------------- | ------------ |
| 2026 | 500          | 0                    | $2K          | $0             | $24K         |
| 2027 | 3,000        | 0                    | $18K         | $0             | $216K        |
| 2028 | 12,000       | 1                    | $72K         | $8K            | $960K        |
| 2029 | 35,000       | 3                    | $280K        | $30K           | $3.7M        |
| 2030 | 80,000       | 6                    | $640K        | $60K           | $8.4M        |

5-year cumulative: ~$13.3M (vs. V1's $10M). Enterprise adds ~30% above pure consumer revenue by 2030.

#### Scenario C: Optimistic — Multi-Vertical + Supply Chain Product-Market Fit

Viral AR moment. $300-500 glasses by 2028. Supply chain product achieves PMF — a cold chain logistics company or fleet operator adopts the platform. Enterprise contracts at $20K-50K/mo.

| Year | Consumer MAU | Enterprise Contracts | Consumer MRR | Enterprise MRR | Total Annual |
| ---- | ------------ | -------------------- | ------------ | -------------- | ------------ |
| 2026 | 1,500        | 0                    | $9K          | $0             | $108K        |
| 2027 | 15,000       | 1                    | $90K         | $15K           | $1.3M        |
| 2028 | 60,000       | 4                    | $480K        | $80K           | $6.7M        |
| 2029 | 180,000      | 10                   | $1.4M        | $300K          | $20.4M       |
| 2030 | 500,000      | 20                   | $4M          | $700K          | $56.4M       |

5-year cumulative: ~$84.9M (vs. V1's $71M). Enterprise revenue contributes 15-20% and growing faster than consumer.

---

## V. Competitive Position Update

### V1 Assessment — Largely Unchanged

| V1 Competitor     | V1 Assessment                    | Status (Feb 2026)                                   |
| ----------------- | -------------------------------- | --------------------------------------------------- |
| Pokemon Go        | Fictional creatures, no learning | Still dominant but aging; no knowledge accumulation |
| iNaturalist       | Real species, no AR              | Growing but no economic layer, no companion model   |
| Merlin            | Bird ID only                     | Audio-focused, no AR, no social gathering           |
| Peridot (Niantic) | Virtual pets                     | Launched and plateaued; no taxonomic depth          |

### New Competitive Dimension: Supply Chain

In the supply chain space, Rabble/ABW competes differently — not as an AR app but as an "intelligent sensor platform":

| Competitor | Model                      | ABW Difference                                                 |
| ---------- | -------------------------- | -------------------------------------------------------------- |
| FourKites  | SaaS visibility platform   | No agent learning, no knowledge graph, no coherence evaluation |
| Project44  | API-first supply chain     | Tracking only — no autonomous pattern extraction               |
| Tive       | Sensor + tracking hardware | Hardware-dependent; no LLM intelligence layer                  |
| Samsara    | IoT fleet management       | Massive but monolithic; no agent composition or marketplace    |

The ABW moat in supply chain is the same as in consumer: **agents that learn from experience**. A cold chain monitoring system that simply alerts on temperature thresholds is a rule engine. An ABW-powered system that consolidates thousands of shipment episodes, extracts "Supplier X's packaging fails above 30°C when humidity exceeds 80% on routes through the Gulf states in summer," and coherence-evaluates this against other knowledge — that's qualitatively different.

---

## VI. What V1 Got Right

1. **Credit economics**: Gas fee schedule implemented exactly as designed. Unit economics validated.
2. **GBIF integration**: 650K+ species, real taxonomy, remains a unique differentiator.
3. **Intelligence flywheel**: ADM consolidation, coherence evaluation, knowledge graph — all built and operational.
4. **80% gross margins**: Infrastructure costs minimal. API costs scale linearly as projected.
5. **Network effects are local**: Still true. Contacts system now explicitly supports this (find friends in your area).
6. **ABW/Rabble symbiosis**: Bidirectional value creation confirmed — Rabble generates episodes that feed agent learning.
7. **Cost structure**: Pre-launch at $10/mo as projected. Scaling estimates appear conservative.

## VII. What V1 Got Wrong (or Missed)

1. **Underestimated the creature lifecycle**: V1 treated creatures as mint-and-fly-forever. Reality: users need to rename, archive, retire. Creature CRUD wasn't in the original MVP list but proved essential for the app to feel like "yours."

2. **No social layer in V1 roadmap**: The original memo emphasized local network effects but had no mechanism for finding friends. The contacts system (search, follow, invite to private rabbles) was a gap that had to be filled.

3. **Device pairing was not anticipated**: The entire supply chain thesis emerged from a feature (creature-device pairing) that wasn't in V1. This is the most significant strategic development since the original memo.

4. **SOSA observation API not anticipated**: W3C SSN-standard sensor ingestion was built for swarm telemetry but creates a domain-agnostic IoT bridge. V1 didn't imagine sensor integration beyond GPS.

5. **Admin tools undervalued**: V1 had no admin dashboard in the roadmap. Moderation (flag creatures, cancel swarms, manage users) is essential for any social platform.

6. **Credit pricing was too low**: V1 priced at $5/$20/$35 for 100/500/1000 credits. Actual implementation is $9.99/$39.99/$69.99. The 2x increase improves all revenue projections.

7. **Testing infrastructure not mentioned**: 65-test widget suite with MockClient wasn't on the roadmap but is essential for development velocity. You can't build a reliable app without reliable tests.

8. **41 agents, not ~25**: The agent catalogue grew significantly during development. New categories (observation, swarm, marketplace, billing) emerged organically.

---

## VIII. Updated Current State (February 2026)

### What's Built and Working

**Flutter App (15 screens)**:

- Collection with status filtering (Active/Archived/All)
- Species browser with GBIF search
- Creature detail with rename, archive/restore/retire, devices, flight map, cost hints
- Mint wizard with Gemini art generation
- Rabble creation (6-step) with funding and visibility
- Rabble joining, real-time chat with creature attribution
- AR viewer with GPS + compass overlay
- Wallet: balance card, 3 billing tiers, Stripe checkout, dev topup, transaction history
- Contacts: user search, follow, nickname, invite to private rabbles
- Admin: Stats/Users/Creatures/Swarms with moderation tools
- Help & onboarding: 8-section guide, getting started flow
- Home shell: 5-tab navigation, credit balance badge, account menu

**Backend (30 handler modules, 56 migrations)**:

- Full credit economy (Stripe checkout, webhooks, idempotent wallet)
- OAuth (Google, GitHub) with JWT sessions and API keys
- 41 agents with multi-model execution (Anthropic, Mistral, Qwen, OpenRouter)
- 30 platform tools (spatial, image, voice, delegation, marketplace, coherence)
- Tool-aware executor with 5-iteration agentic loop
- SOSA observation API (W3C SSN vocabulary)
- Swarm telemetry with auto-analysis
- H3 spatial grid indexing
- Knowledge graph (entities, facts, rules, communities)
- ADM consolidation with dream narrator
- TEC coherence evaluation
- Embedding marketplace (privacy-preserving similarity)
- Eval framework with LLM-as-judge regression detection
- Device pairing (AirTag, SmartTag, Tile, GPS, BLE)
- Agent version history and forking

**Testing**:

- 65 widget tests across 13 files with comprehensive MockClient
- Covers all 15 screens: models, auth flow, CRUD flows, navigation

**Infrastructure**:

- Axum (Rust) on Railway: $5/mo
- Neon PostgreSQL with pgvector: free tier
- Docker with cargo-chef (2-3 min deploys for code changes)
- Custom domains: agent-bestiary.world, rabble.world

### What's Next

| Priority | Feature                                                       | Impact                |
| -------- | ------------------------------------------------------------- | --------------------- |
| HIGH     | Push notifications (rabble invites, device alerts)            | Retention             |
| HIGH     | Supply chain demo (rebrand creature → shipment for B2B pitch) | Enterprise validation |
| MEDIUM   | Creature trading/gifting                                      | Social engagement     |
| MEDIUM   | Flight analytics dashboard                                    | User value            |
| MEDIUM   | Background device location polling                            | Autonomous tracking   |
| LOW      | Creature evolution/leveling                                   | Engagement depth      |
| LOW      | AR glasses SDK integration                                    | Future-proofing       |

---

## IX. The Investment Thesis — Updated

The original thesis was: **Rabble is a consumer AR companion app powered by a learning economy. AR glasses are the catalyst. Nature is the beachhead.**

The updated thesis adds a second dimension: **The same infrastructure is an enterprise IoT intelligence platform. Consumer Rabble validates the platform and generates volume. Enterprise supply chain generates revenue and validates the business model.**

### What Makes This More Investable Than V1

1. **Dual market, single codebase**: Consumer AR and enterprise supply chain share 100% of infrastructure. Development investment serves both markets simultaneously.

2. **Enterprise revenue path**: V1 was pure consumer, dependent on volume. V2 has an enterprise path ($5K-50K/mo contracts) that de-risks the AR glasses dependency.

3. **More built than projected**: 15 screens vs. 10. 41 agents vs. ~25. 30 tools vs. ~9. 56 migrations vs. ~40. 65 tests vs. 1. The engineering output exceeded V1 projections.

4. **Higher pricing validated**: 2x credit pricing improves all revenue projections without evidence of demand sensitivity (pre-launch).

5. **SOSA standard compliance**: W3C SSN vocabulary means any standards-compliant sensor can feed the platform. This is not a proprietary protocol — it's an open standard that enterprises already use.

6. **The white paper exists**: "Agentic Infrastructure for Learning Adaptive Systems" articulates the supply chain thesis in detail, ready for enterprise conversations.

7. **The moat deepened**: 41 agents with accumulated knowledge, 30 platform tools, TEC coherence evaluation, embedding marketplace — the gap between "what's built" and "what a competitor would need to replicate" widened significantly.

### Use of Funds — Updated

| Allocation                  | %   | Purpose                                                                        |
| --------------------------- | --- | ------------------------------------------------------------------------------ |
| User acquisition (consumer) | 25% | Geo-targeted campaigns, nature community partnerships                          |
| Enterprise pilots           | 20% | Supply chain demos, cold chain pilot with 1-2 logistics companies              |
| Engineering                 | 30% | AR glasses SDK, background device polling, enterprise dashboard, notifications |
| Content & partnerships      | 15% | Species expansion, education pilots, tourism board partnerships                |
| Operations                  | 10% | Community management, App Store, enterprise sales                              |

The key shift from V1: 20% allocated to enterprise pilots (vs. 0% in V1). This reflects the dual-market strategy.

---

## X. Conclusion

V1 was right about the fundamentals: credit economics, intelligence flywheel, local network effects, AR glasses catalyst. What V1 missed was that the architecture we were building for AR creature companions is, with zero modification, an enterprise IoT intelligence platform.

The creature is an abstraction. The flight is an abstraction. The swarm is an abstraction. The device pairing is an abstraction.

Replace butterflies with shipping containers and parks with warehouses, and you have adaptive supply chain intelligence with 41 agents, 30 tools, knowledge graphs, coherence evaluation, and an embedding marketplace — all built, tested, and deployed.

**Two markets. One platform. Compounding intelligence in both.**

---

*Rabble.world / Agent Bestiary World — February 2026*
*rabble.world | agent-bestiary.world*
