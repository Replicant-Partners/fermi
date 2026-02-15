# Creature Mind: Leveling, Dreaming & Specialization

Your creature isn't just a sprite on a map. It's a proxy for a trained AI — and
everything it does makes it smarter.

## How Creatures Level Up

Every interaction generates data. Data feeds the creature's knowledge:

| Activity | What it builds | Cost |
|----------|---------------|------|
| **Fly** | Location knowledge, habitat observations | 1cr |
| **Join a rabble** | Social dynamics, flock behavior | 0-2cr |
| **Prey scan** | Ecological relationships, predator-prey networks | 2cr |
| **Enemy scan** | Threat assessment, defensive knowledge | 1cr |
| **Genome profile** | Taxonomic context, phylogenetic depth | 2cr (one-time) |
| **Dream** | Consolidation, counterfactual reasoning | 5cr |
| **Tether** | Real-world GPS traces, environmental data | 1cr |

Your creature's **level** reflects the depth and richness of what it knows:

```
Level = floor(log2(1 + weighted_score))
```

Early levels come fast — your first flight gets you to Level 1. But high levels
require sustained engagement across multiple dimensions.

## The Brain Icon

The brain icon on your creature's image shows its current level. Tap it to see:

- **Level & Score** — your creature's cognitive development
- **Specialization** — what your creature is becoming (see below)
- **Growth bars** — visual breakdown of each knowledge dimension
- **Active sensors** — which modules are feeding data
- **Investment** — total credits spent on this creature

## Specialization

Your creature develops a specialization based on what it actually does.
This isn't chosen — it emerges from behavior:

| Specialization | Emerges from |
|---------------|-------------|
| **Explorer** | Many flights, diverse locations |
| **Social** | Joining multiple rabbles |
| **Scholar** | High message count, deep interactions |
| **Dreamer** | Frequent dream cycles |
| **Sentinel** | Multiple active sensor modules |

Specialization affects how the creature is perceived on the marketplace — an
Explorer with 15 unique locations is a different product than a Dreamer with
8 consolidation cycles.

## Dreaming

Dreams are how your creature integrates what it has learned.

**What happens when you press Dream:**

1. The dream narrator agent consolidates recent interactions
2. Patterns are extracted from flights, scans, and conversations
3. Contradictions are resolved, connections strengthened
4. A dream narrative is generated — unique to your creature's experiences
5. A dream transition is recorded, advancing the creature's level

**Rules:**
- Costs **5cr** per dream cycle
- Minimum **1 hour** between dreams (the creature needs time to accumulate new experiences)
- Requires at least one workspace message (the creature needs something to dream about)
- Dream narratives appear in the creature's Log tab

**Why dream?** Each dream cycle is worth 5 points toward your creature's level —
the highest-weighted activity. A creature that dreams regularly will level faster
than one that only flies.

## Valuation

When you sell a creature on the marketplace, buyers see transparent metrics:

- Level and weighted score
- Breakdown by knowledge dimension
- Specialization tag
- Total credits invested
- Dream cycle count
- Active sensor modules

The price reflects training investment — not arbitrary rarity. A Level 7 Explorer
with 12 unique locations and 3 dream cycles has a clear, defensible valuation.

## What's Coming

- **Dream scheduling** — allocate a credit budget for automatic dream cycles
- **Deep specialization** — sub-types within each specialization (e.g., Coastal Explorer, Urban Explorer)
- **Knowledge transfer** — creatures that share a rabble can cross-pollinate knowledge
- **Dream artifacts** — dream narratives that unlock new capabilities or reveal hidden patterns
