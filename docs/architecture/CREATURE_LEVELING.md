# Creature Leveling: Knowledge-as-Value

## Core Insight

A creature is a proxy for the AI the user invested in training it. Its value
comes from the depth and richness of its knowledge graph and embedding space —
not from arbitrary XP or badges. When you sell a creature, you're selling the
trained AI behind it.

## How Creatures Level Up

Every interaction adds to the creature's knowledge:

### Knowledge Graph Growth
- **Flights** → location observations, habitat data, species encounters
- **Prey scans** → predator-prey relationships, ecological network edges
- **Enemy scans** → threat assessments, defensive knowledge
- **Rabble participation** → social dynamics, flock behavior data
- **Dream cycles** → counterfactual reasoning, consolidated insights
- **Genome profiling** → taxonomic context, phylogenetic relationships

Each of these creates entities and facts in the creature's workspace KG.

### Embedding Space Depth
- Every workspace message gets embedded (via the workspace's embedding model)
- More interactions = denser embedding space = better similarity search
- Diverse interactions = broader coverage = more useful agent

### Coherence Score
- TEC evaluations measure how well the creature's knowledge holds together
- Higher coherence = better-integrated knowledge = more valuable
- Dream cycles (ADM consolidation) improve coherence by resolving contradictions

## Leveling Metrics

A creature's "level" is derived from measurable quantities:

| Metric | Source | What it measures |
|--------|--------|-----------------|
| `kg_entities` | Knowledge Graph | Breadth of knowledge |
| `kg_facts` | Knowledge Graph | Depth of relationships |
| `kg_communities` | Knowledge Graph | Structural richness |
| `embedding_count` | Workspace messages | Interaction volume |
| `embedding_diversity` | Embedding projector | Coverage breadth |
| `coherence_score` | TEC evaluations | Knowledge integration |
| `dream_cycles` | ADM consolidations | Reflective depth |
| `unique_locations` | creature_flights | Geographical range |
| `agent_interactions` | creature_versions | Training investment |
| `total_credits_spent` | credit_ledger | Economic investment |

### Level Formula (draft)
```
level = floor(log2(1 + weighted_score))

weighted_score = 
    kg_entities * 1.0 +
    kg_facts * 2.0 +
    embedding_count * 0.5 +
    coherence_score * 10.0 +
    dream_cycles * 5.0 +
    unique_locations * 0.3 +
    agent_interactions * 1.0
```

Logarithmic scaling means early levels come fast (first flight, first scan),
but high levels require deep, sustained engagement.

## Valuation for Marketplace

When listing a creature for sale, the valuation is transparent:

```json
{
  "level": 7,
  "metrics": {
    "kg_entities": 42,
    "kg_facts": 156,
    "coherence_score": 0.73,
    "dream_cycles": 3,
    "embedding_count": 234,
    "unique_locations": 8,
    "total_credits_invested": 127
  },
  "suggested_price_range": [50, 150],
  "provenance": {
    "created_at": "2026-02-10",
    "total_flights": 12,
    "rabbles_joined": 4,
    "agent_modules_used": ["prey_locator", "enemy_sensor", "genome_profiler"]
  }
}
```

The buyer knows exactly what they're getting: a trained AI with specific
knowledge depth. The price reflects the training investment, not arbitrary
rarity.

## Dream State / Counterfactual Button

The "Dream" action triggers ADM consolidation for the creature:

1. Gather recent workspace messages + KG facts
2. Run coherence evaluation (TEC)
3. Consolidate: resolve contradictions, strengthen connections
4. Dream narrator generates narrative summary
5. Creature gains coherence score improvement + dream_cycle count

This is the creature's way of "sleeping on it" — integrating what it learned.
Each dream cycle costs credits (the creature is doing computational work)
and measurably improves the creature's knowledge quality.

### UX Flow
- Creature detail → "Dream" button (in actions row)
- Shows dreaming animation (particle/constellation effect)
- Returns dream narrative + updated coherence score
- Visible in creature's Log tab as a dream event

### Prerequisites
- Creature must have a workspace with messages (something to dream about)
- Minimum interval between dreams (prevent spam — 1hr?)
- Cost: ~5cr per dream cycle

## Connection to Existing Architecture

This design uses everything already built:

- **Workspace KG** → `src/handlers/kg.rs` (8 endpoints)
- **Embedding space** → `src/agent_backend/embedding_projector.rs`
- **Coherence/TEC** → `coherence-engine` crate
- **ADM consolidation** → `src/handlers/coherence.rs`
- **Dream narrator** → `agents/curated/dream_narrator/agent_card.json`
- **Credit ledger** → tracks total investment per creature
- **creature_versions** → logs every state transition (training events)

No new infrastructure needed — just wiring existing systems together with
creature-scoped queries and a level computation.

## Implementation Status (Feb 15)

**Shipped:**
- `GET /api/creatures/:id/level` — weighted score computation (7 metrics)
- `POST /api/creatures/:id/dream` — dream_narrator dispatch, 5cr, 1hr cooldown
- Dream transitions recorded in `creature_versions` (transition_type = 'dream')
- CognitivePill on creature hero image (brain icon, tap for full metrics sheet)
- Emergent specialization labels (Explorer/Social/Scholar/Dreamer/Sentinel)
- Cognitive growth bars visualization
- Dream chip in creature actions (world + tethered states)
- Gameplay docs: `docs/gameplay/CREATURE_MIND.md`

**Implemented level weights:**
```
messages * 0.5 + versions * 1.0 + locations * 0.3 +
dreams * 5.0 + flights * 0.2 + rabbles * 2.0 + modules * 1.0
```

## Emergent Specialization (design)

Specialization is derived, not assigned. The creature's behavior profile
determines its tag:

| Tag | Signal | Threshold |
|-----|--------|-----------|
| Explorer | `unique_locations * 3.0 + flights * 0.5` | Highest signal |
| Social | `rabbles_joined * 5.0` | Highest signal |
| Scholar | `message_count * 0.8` | Highest signal |
| Dreamer | `dream_cycles * 8.0` | Highest signal |
| Sentinel | `active_modules.len() * 4.0` | Highest signal |
| Nascent | (none above 1.0) | Default |

### Future: Deep Specialization

When KG queries are creature-scoped (not yet — KG is agent-scoped today):

- **Coastal Explorer** — creature has location entities clustered near coastlines
- **Predator Specialist** — KG has dense predator-prey fact network
- **Social Hub** — high rabble count + many unique co-member creatures
- **Lucid Dreamer** — dream narratives with high coherence delta (measurable improvement)

This requires:
1. Creature-scoped KG views (filter workspace KG by creature_id in metadata)
2. Coherence delta tracking (before/after dream comparison)
3. Location entity classification (requires reverse geocoding or habitat lookup)

## Dream Scheduling (design — not yet implemented)

### User Flow

1. Creature detail → Brain icon → Cognitive sheet → "Dream Scheduling" card
2. Opens dream budget configuration:
   - **Budget**: 10-100cr allocated for automatic dreams
   - **Frequency**: every 4hr / 8hr / 12hr / 24hr
   - **Until**: budget depleted / level target reached / manual stop
3. Backend creates a `dream_schedule` record
4. Background worker checks schedules, fires dreams for eligible creatures

### Data Model

```sql
CREATE TABLE creature_dream_schedules (
    schedule_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id UUID NOT NULL REFERENCES creatures(creature_id),
    owner_id TEXT NOT NULL,
    budget_credits INTEGER NOT NULL,
    credits_used INTEGER NOT NULL DEFAULT 0,
    interval_hours INTEGER NOT NULL DEFAULT 24,
    target_level INTEGER,          -- stop when reached (NULL = run until budget)
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'depleted', 'completed')),
    last_dream_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Background Worker

```rust
// In api_server.rs startup, after existing background tasks:
tokio::spawn(dream_scheduler_loop(state.clone()));

async fn dream_scheduler_loop(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await; // check every 5min
        // SELECT * FROM creature_dream_schedules
        // WHERE status = 'active'
        //   AND credits_used < budget_credits
        //   AND (last_dream_at IS NULL OR last_dream_at < NOW() - interval_hours * INTERVAL '1 hour')
        // For each: fire creature_dream_handler logic, update credits_used + last_dream_at
    }
}
```

### Economics

- Same 5cr per dream cycle (uses `gas.creature_dream`)
- Budget is pre-allocated from wallet (like dream_topup for agents)
- If wallet balance drops below next dream cost, schedule pauses
- Owner notified via existing notification system when budget depleted or level target hit

## Creature Knowledge Transfer (future)

When two creatures share a rabble, their workspaces overlap — they receive
the same messages. But knowledge transfer goes further:

- Creatures in the same rabble for > 1hr could share KG edges
- A high-level creature in a rabble "teaches" lower-level members
- Cross-pollination: Explorer shares location knowledge with a Scholar
- Transfer costs credits (the source creature's owner gets paid)

This creates a secondary economy: creature rental for knowledge transfer.
A Level 10 Explorer is worth renting because it accelerates other creatures'
leveling in the Explorer dimension.
