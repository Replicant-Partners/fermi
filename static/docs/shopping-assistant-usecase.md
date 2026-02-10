# Use Case: Shopping Assistant & Reverse-SEO Marketplace

## The Problem

Online advertising is broken. Advertisers spend billions profiling consumers through invasive tracking — cookies, fingerprinting, behavioral surveillance. Consumers have no say in how their data is used, no share of the revenue it generates, and no way to opt out without losing the internet.

What if we flipped the entire model?

## The Vision

**Consumers own their preference data. Advertisers pay to access it. The consumer sets the price.**

Instead of advertisers building shadow profiles of you from tracking pixels, you build your own preference profile through natural shopping conversations. You decide what categories to share. You set the price. You earn credits when advertisers query your profile. You delist whenever you want.

The advertiser never sees your actual preferences, purchase history, or conversation content. They only see a **similarity score** — a single number between 0 and 1 that tells them how well their product aligns with your taste. That's it.

## How the Shopping Assistant Works

The Shopping Assistant is a **compound agent** — an orchestrator that coordinates three specialist sub-agents in a workspace:

### The Team

| Agent | What It Does | How It's Called |
|-------|-------------|-----------------|
| **shopping_assistant** | Takes your shopping requests, coordinates the team, maintains conversation context | You talk to it directly |
| **deal_finder** | Researches products, compares specs and prices, returns structured comparisons | Called via `execute_agent` (text-only) |
| **preference_modeler** | Extracts preference signals from your conversations, computes your composite embedding | Called via `delegate_to_agent` (has tools) |
| **embedding_broker** | Manages your marketplace listing — pricing, privacy, delisting | Called via `delegate_to_agent` (has tools) |

### A Typical Session

**1. You ask about a product:**

> "I need running shoes for marathon training. Budget around $150, I like Nike and Hoka but I'm open to others. Prefer lightweight with good cushioning."

**2. The assistant delegates to deal_finder:**

The deal_finder returns a structured comparison:

```
## Marathon Running Shoes Comparison

### 1. Nike Vomero 18 - $150
Value: 4.5/5 | Best for: Daily training + race day versatility
- Pros: ZoomX foam, lightweight (9.2oz), excellent energy return
- Cons: Durability concerns after 400mi, narrow toe box

### 2. Hoka Clifton 9 - $140
Value: 4/5 | Best for: Maximum cushion, easy runs
- Pros: Signature cushion, meta-rocker geometry, wide base
- Cons: Less responsive for speed work, may feel "squishy"

### 3. New Balance Fresh Foam X 1080v13 - $160
Value: 4/5 | Best for: All-around comfort, wider feet
- Pros: Plush cushion, knit upper, good for longer runs
- Cons: Slightly above budget, heavier (10.8oz)

### Verdict
Best Overall: Nike Vomero 18 — hits the sweet spot
Best Budget: Hoka Clifton 9 at $140
Best Premium: NB 1080v13 if you have wider feet
```

**3. You have a conversation about it:**

You discuss trade-offs, mention that you had a bad experience with Asics, say you care more about injury prevention than speed. Every message becomes an episode with an embedding that captures the semantic meaning of your preferences.

**4. After several sessions, the preference_modeler builds your profile:**

It analyzes your conversation history and extracts:

- **Brand affinities**: Nike (0.82), Hoka (0.75), New Balance (0.60), Asics (0.15)
- **Price sensitivity**: 0.55 (moderate — willing to spend for quality but has limits)
- **Quality bias**: 0.78 (leans premium — values durability and injury prevention)
- **Categories**: running, fitness, footwear

The composite embedding is a 1024-dimensional vector that captures the **nuance** of your taste — things that are hard to express in simple attributes. Maybe you consistently prefer brands with strong sustainability practices, or you gravitate toward products with minimalist design. The embedding captures these latent patterns.

**5. You list on the marketplace:**

The embedding_broker walks you through the process:
- Explains that only similarity scores are shared (never raw data)
- Recommends pricing based on your profile richness
- Creates the listing with your chosen price and category tags
- You start earning credits when advertisers match against your profile

## How the Marketplace Works

### The Match

When an advertiser describes their product ("lightweight trail running shoe for ultramarathon runners, $180"), the system:

1. **Generates an embedding** from the product description (same 1024-dim space)
2. **Computes cosine similarity** against every listed consumer profile using pgvector
3. **Returns a score**: 0.0 = no alignment, 1.0 = perfect alignment

A score of 0.85 means this product strongly resonates with your shopping patterns. The advertiser learns: "this consumer is very likely to be interested in our product." They don't know why. They don't know your name, your history, or your preferences. Just a number.

### The Economics

```
Consumer sets price:        5 credits per query
Advertiser pays:            5 + 1 (base fee) = 6 credits
Platform keeps:             1 + floor(5 * 0.15) = 1 credit
Consumer receives:          5 - floor(5 * 0.15) = 5 credits
```

At scale, a consumer with a rich profile in a popular category could earn meaningful credits just from their shopping data. Those credits can be spent on other Bestiary services.

### Privacy Guarantees

| Layer | What Happens |
|-------|-------------|
| **Computation** | Cosine similarity runs server-side. The advertiser's product embedding and the consumer's profile embedding never leave the server. |
| **Response** | The API returns only similarity scores (0.0-1.0), category tags, price_sensitivity, and quality_bias. No raw vectors. |
| **Audit** | Each query is logged with a SHA-256 hash of the product embedding. The hash is meaningless without the original vector. |
| **Control** | Consumer can delist instantly. Can cap queries per buyer. Can change price. All through the embedding_broker agent. |

## Why This Matters

### For Consumers

- **You own your data**: Your preference profile is yours. Built from your conversations, stored in your workspace.
- **You set the terms**: Price, categories, query caps — all your choice.
- **You earn from it**: Instead of advertisers profiting from your data, you get paid.
- **Privacy by design**: Raw embeddings never leave the server. Advertisers only see scores.

### For Advertisers

- **Better signal**: Instead of noisy behavioral tracking, you get a direct measure of preference alignment.
- **Consent-based**: Every consumer opted in. No tracking, no cookies, no legal risk.
- **Honest scores**: The similarity is computed from genuine shopping conversations, not manufactured engagement.
- **Cost-effective**: Pay only for matches above your similarity threshold.

### For the Platform

- **Sustainable economics**: 15% platform fee + base fee per match funds operations.
- **Aligned incentives**: Better consumer profiles = better matches = more advertiser queries = more consumer revenue.
- **No surveillance infrastructure**: No tracking pixels, no cookie syncing, no data brokers.

## Building Your Own Shopping Agent

The Shopping Assistant pattern is reusable. You can fork it and customize:

1. **Fork** `shopping_assistant` from the catalogue
2. **Customize** the system prompt for your domain (e.g., "Wine Assistant", "Tech Gear Advisor")
3. **Adjust** the deal_finder's comparison framework for your category
4. **Deploy** to a workspace and start building preference data

The embedding marketplace works with any agent that computes composite embeddings from episode history. The shopping_assistant is just the first implementation.

## Technical Details

For implementation specifics, see the [Embedding Marketplace](/docs/embedding-marketplace) technical documentation covering:
- Database schema and pgvector queries
- Composite embedding computation (weighted centroid algorithm)
- API endpoints and workspace tools
- Gas fees and credit flow
