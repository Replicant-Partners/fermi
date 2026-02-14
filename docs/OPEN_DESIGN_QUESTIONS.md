# Open Design Questions & Conceptual Clarifications

Captured 2026-02-08 after MVP Sprint 1-5 deployment. These need resolution before next iteration.

---

## 1. Hire vs Execute — Agent Lifecycle

**Current state:** "Hire Agent" button was replaced by "Execute Agent" — but these are different concepts.

**Correct model:**
- **Hire** = bring an agent into a team/workspace. This is an organizational action, not a computation.
  - For agents you DON'T own: hiring means adding them to your workspace (like recruiting)
  - For agents you DO own: hiring to a workspace means assigning them to a shared context
- **Execute** = run a query against an agent. Can only happen AFTER the agent is in your workspace (or is your own personal agent).
- **Flow:** Browse bestiary -> Hire agent to workspace -> Execute within workspace context

**Action needed:** Restore "Hire" as a distinct concept. Execute should require the agent to be "hired" (associated with a workspace/team the caller belongs to, or owned by the caller directly).

---

## 2. Gas Fees Apply to ALL A2A Transactions

**Current state:** Gas fees only apply to execution (10% surcharge on token cost).

**Correct model:** Gas should apply to ALL agent-to-agent transactions regardless of ownership:
- Agent execution (already implemented)
- Hiring an agent to a workspace
- Agent-to-agent communication (future AKP)
- Education/consolidation cycles
- Any computational action that touches the platform

**Rationale:** Gas fees fund the platform. Even your own agents consume infrastructure. This creates a uniform economic model where every transaction has a cost, encouraging efficiency.

**Action needed:** Define gas fee schedule for each transaction type. Current: 10% on execution. Need rates for: hire, transfer, consolidation, A2A messages.

---

## 3. Agentified Agent Creation (MCP Coach)

**Current state:** Agent creation is a dumb HTML form that posts JSON to an API.

**Correct model:** Agent creation should be an agentified process with an MCP-type coaching server that:
- Steers users toward high-quality agent design
- References practices from `/agents/templates/` (design checklist, prompt engineering guide, ontology patterns)
- Coaches through the 11-point design checklist interactively
- Validates agent cards against schema in real-time
- Suggests improvements to system prompts
- Recommends appropriate executor types, models, and temperature settings
- Warns about common pitfalls (overly broad scope, missing error handling, ontology over-engineering)
- Can be "deep and efficient" — thorough when needed, streamlined when the user knows what they want

**Architecture options:**
1. MCP server that the creation page talks to via WebSocket
2. Conversational UI in the creation page that calls an agent-creation-coach agent
3. Zed extension integration (the original DSL vision)
4. All of the above — web UI for casual users, Zed for power users

**Action needed:** Design the agent-creation-coach agent. It needs access to the template library and design practices as context.

---

## 4. Model Support — Not Just Claude

**Current state:** Everything is hardcoded to Claude models (Haiku, Sonnet, Opus). The creation form and executor only reference Anthropic.

**Correct model:** Support multiple model providers:
- **Anthropic** (Claude): Haiku, Sonnet, Opus — current default
- **Mistral**: Open-weight models, self-hostable
- **Qwen**: Open-weight models, self-hostable
- **OpenRouter**: Meta-router for many models (Llama, Gemma, etc.)
- **Bring Your Own**: User-provided API endpoints

**Self-hosting note:** Replicant Partners will likely host and run own Mistral and Qwen instances for:
- Testing custom embeddings
- Running proprietary workloads without external API dependency
- Cost optimization for high-volume operations

**Action needed:**
- Abstract LLM executor to support multiple backends (not just Anthropic API)
- Update agent_card schema: model field should reference provider + model_id
- Update creation form with model provider dropdown + model selector
- Define embedding provider options (currently hardcoded to Voyage/Anthropic)

---

## 5. Executor Types & Agent Taxonomy

**Current state:** Executor types are: `llm`, `mcp`, `manual`, `skill`. But deterministic agents don't fit cleanly.

**The Monte Carlo question:** Fermi currently has a Monte Carlo simulation model. As a standalone agent, it would be "deterministic" — no LLM involved. Where does it sit in the taxonomy?

**Proposed taxonomy:**

| Executor | Description | Example |
|----------|-------------|---------|
| `llm` | LLM-powered, single model call | sentiment_analyzer |
| `mcp` | LLM + external tools via MCP | market_research |
| `deterministic` | Algorithmic, no LLM | monte_carlo_sim, coherence_evaluator (TEC core) |
| `hybrid` | Deterministic core + LLM interpretation layer | coherence_evaluator (TEC + LLM narration) |
| `skill` | Orchestrates multiple sub-agents | risk_assessment_pipeline |
| `manual` | Human-in-the-loop | expert_review |
| `custom` | User-provided execution endpoint | bring-your-own |

**Key insight:** The coherence evaluator already demonstrates the `hybrid` pattern — deterministic TEC computation with an LLM layer for interpretation. This is likely a common pattern.

**Action needed:** Formalize the taxonomy. Update AgentCard executor enum. Ensure the execution pipeline can route to the right backend based on executor type.

---

## 6. Workspace Draft Implementation

**Current state:** Workspaces are thin wrappers around teams with budget fields. The concept needs more substance.

**Open questions:**
- What does a workspace "look like" to a user? Is it a persistent environment where agents run?
- Do agents in a workspace share context/memory? (Shared ontology? Shared episodes?)
- Can a workspace have its own ontology that emerges from the collective activity of its agents?
- How do workspace-level gas fees differ from individual agent gas fees?
- What's the relationship between workspace budget and individual agent education budgets?

---

## 7. Dual-Layer Economic Model — Credits + Crypto Transaction Fees

**Resolved 2026-02-08.** Two distinct economic layers:

### Layer 1: Credits (The Product)

Credits are the platform's unit of commerce. Users **buy credits with real money**. Every platform action has a credit cost (the gas fee schedule). This is the primary revenue model.

- Credits are a product — purchased via Stripe, crypto, or other payment rails
- Every action costs credits: messages (1), hires (5), adds (2), execution (~1/1k tokens + 10%), consolidation (3), agent creation (education_budget)
- Platform can allocate free credits for: beta access, promotions, free tier, referral bonuses
- Credit pricing is set by the platform and can be adjusted
- Credits are non-transferable between users (prevents arbitrage)

### Layer 2: Crypto Token Transfers (Agent Economy)

When agents generate value (execution, hire, etc.), agent **owners earn real crypto tokens**. The platform takes a **% transaction fee** on every transfer — like a blockchain gas fee or marketplace commission.

```
User executes agent:
  Layer 1: User spends 5 credits (platform gas)
  Layer 2: User pays 0.002 ETH (agent owner's fee)
           Platform takes 2.5% tx fee on the ETH transfer
```

- Agent owners set their own prices (in tokens)
- Platform takes a configurable % on every owner payout
- Requires wallet connection (SIWE — stub exists in fermi-auth)
- Settlement can be on-chain or custodial (design TBD)
- This enables agent builders to monetize their work directly

### Two Revenue Streams

1. **Credit sales** — selling the product (platform operations revenue)
2. **Transaction fees** — % cut of agent economy (marketplace revenue)

### Prerequisites for Layer 2
- SIWE wallet connection (fermi-auth stub exists)
- Agent pricing model (owner sets token fee per action type)
- Payment settlement (on-chain vs custodial)
- Transaction fee % configuration
- Wallet balance display in dashboard

### Implementation Order
- Layer 1 is **live** (credits, gas fees, wallets)
- Layer 2 is **future** — build after SIWE, wallet UX, and agent pricing are designed

---

## 8. Flight & Telemetry Model (Resolved 2026-02-14)

### Creature Lifecycle

Every creature follows: **Mint → Set Down → Fly/Join → Fly → ...** 

- **Set Down (2cr)**: First placement. Gives the creature a location in the world. Configures the perch (walk-in pricing, invite pool, walk-in budget). One-time action.
- **Fly (1cr)**: Every subsequent move. Always requires a flight plan (destination + optional route description). Creature flies the route, arrives perched at destination. Generates simulated telemetry along the path.
- **Join**: Enter an existing rabble/perch. Creature moves to that location.
- **Perched**: Status, not an action. A creature is "perched" whenever it's at a location and not in flight.

### Two Telemetry Sources

All flights produce structurally identical telemetry — timestamped (lat, lng, altitude, metadata) points. The difference is the source:

| Mode | Source | Telemetry | Fly action? |
|------|--------|-----------|-------------|
| **Simulated** | Flight plan (user picks destination) | Generated/interpolated along route | Yes — pick destination, creature flies there |
| **Live (tethered)** | Sensor signal (GPS, radio, tag) | Recorded from device | No — creature tracks automatically, position = sensor position |

**Key rule**: A creature can only exist in one location at a time. Tethered creatures cannot fly simulated routes because they'd be in two places — the sensor location and the flight path. Tethered creatures have no Fly button, only a track log.

### Tethering (Design — not yet built)

A creature can be tethered to a signal source. The tether replaces user-initiated Fly with automatic position tracking.

**Tether sources (in priority order):**
1. **Phone GPS** — browser Geolocation API or native, simplest first implementation
2. **Meshtastic LoRa radio** — BLE serial protocol, GNSS position packets
3. **GPS tracker / smart tag** — BLE beacons, proprietary APIs (Tile, AirTag-like)
4. **Fixed sensor** — weather station, environmental monitor. Stationary but streams condition data. The "flight" is through conditions, not space.

**Tether data model:**
- `creature_tethers` table: creature_id, tether_type (phone_gps, meshtastic, gps_tracker, fixed_sensor), device_id, config JSON, active boolean, created_at
- When tethered: background process (or client push) writes telemetry points to `creature_flights` at interval
- Creature presence auto-set to "tracking" (new presence state alongside active/sleeping/parked)

**Analytics potential**: Same telemetry format, radically different signal shapes:
- Phone: human movement — commutes, walks, pauses, daily/weekly rhythms, seasonal patterns
- Drone: flight corridors, grid surveys, altitude changes, sharp turns
- Meshtastic: off-grid mesh network coverage mapping, relay paths
- Simulated: smooth interpolated arcs between waypoints
- At multiple time scales, the same data reveals different patterns (day = neighborhood, month = city, year = seasons)

The creature is a **data avatar** — it gives identity and narrative to a telemetry stream.

### Agent-in-Flight (Design — not yet built)

Agents can be invited into a flight or rabble workspace, gaining access to:
1. **Telemetry context** — the creature's current and historical track
2. **Scoped embedding similarity** — owner's relevant embeddings (not raw vectors, only similarity scores)
3. **Workspace conversation** — the chat context of the flight/rabble

**Economics:**
- Per-agent invite fee (hire into flight workspace)
- Per-agent invocation fee (varies by agent complexity/model)
- Existing gas pipeline handles both

**Use case example**: Phone-tethered creature + shopping assistant agent + user's shopping preference embeddings → agent discovers deals along your live walking path, nudges contextually.

**Implementation path:**
1. Flight gets a workspace (like rabbles do on first join) — or reuse existing rabble workspace
2. "Invite Agent" action on creature detail when placed
3. Agent picker sheet with cost display and capability summary
4. Agent invocation passes flight telemetry as context to the agent's system prompt
5. Start simple: logic puzzle agent on a flight (no location/embedding needed, just proves plumbing)
6. Layer in: location-aware agent → then embedding-scoped agent

### Two-Pool Perch Economics (Implemented 2026-02-14)

Each perch has two separate spending pools:
- **Invite pool**: Pre-funded credits for contacts/invitees to join free. Decremented per contact join.
- **Walk-in budget**: Spending cap for free walk-ins where host pays 1cr per stranger join. Hard reject for strangers when exhausted; soft pass for contacts (they still get in).

Walk-in pricing model:
- `walk_in_price = NULL`: Private — invite only
- `walk_in_price = 0`: Free — host pays per walk-in from walk_in_budget  
- `walk_in_price > 0`: Paid — joiner pays, host gets 90% revenue

---

## 9. Creature-as-Persona Identity Model (2026-02-14)

Creatures are proxy personas / avatars. All communication interfaces go through the creature, not around it. The human identity behind the creature is private by default.

**Chat display rules:**
- **Owner is your contact**: Creature name + owner display name (because you already know who they are)
- **Owner is a stranger / hidden profile**: Creature name only — the creature IS the identity
- **Your own creature**: Creature name (+ "You" indicator)

**Implications:**
- Rabble chat messages show creature avatar + creature name as primary identity
- Owner name is secondary/hidden based on contact relationship
- Tapping a creature in chat → creature detail, NOT owner profile
- API should respect this: `GET /api/rabble/:id/messages` should only include owner display name if caller is a contact of the owner
- Creature detail page for non-contacts: shows creature info, species, flight history — but NOT owner profile link
- Contact relationship is the gate that reveals the human behind the persona

**Why this matters:** Users manage multiple creatures as multiple personas. A researcher might have one creature for professional rabbles and another for casual ones. Breaking the persona by showing the human identity defeats the purpose.

**Implementation touches:**
- `rabble_chat.rs`: conditionally include owner display_name based on contact check
- `rabble_chat.dart`: message bubbles show creature name, owner name only if contact
- Creature detail: remove "Add Owner to Contacts" for strangers → replace with "Add to Contacts" (through the creature, not bypassing it)
- Explore screen: creature cards show creature identity, not owner

---

## Priority Order

1. **Phone GPS tethering** — First live telemetry source, proves the data avatar concept
2. **Agent-in-flight** — Invite agents into flight/rabble workspaces with telemetry context
3. **Hire vs Execute** — Conceptual fix, affects UX flow
4. **Gas on all transactions** — Economic model foundation
5. **Model support** — Technical prerequisite for diverse agents
6. **Executor taxonomy** — Needed before agent creation coaching makes sense
7. **Agentified creation** — HIGH value but depends on 5 & 6
8. **Workspace substance** — Ongoing design conversation

---

## References

- Agent templates: `/home/ilabra/fermi/agents/templates/`
- Design checklist: `/home/ilabra/fermi/agents/templates/DESIGN_CHECKLIST.md`
- Prompt engineering guide: `/home/ilabra/fermi/agents/templates/PROMPT_ENGINEERING_GUIDE.md`
- Coherence evaluator design (hybrid pattern): `/home/ilabra/fermi/agent-bestiary/coherence/DESIGN_V2.md`
- AKP whitepaper: `/home/ilabra/fermi/docs/papers/coherence_improvement_loop.md`
