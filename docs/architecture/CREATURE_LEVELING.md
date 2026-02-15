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
