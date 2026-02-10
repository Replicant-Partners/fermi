# Embedding Marketplace

The Embedding Marketplace inverts the advertising model. Instead of advertisers surveilling consumers, consumers build rich shopping preference profiles and **sell similarity access** to advertisers. The consumer holds the power.

---

## Getting Started

### As a Consumer (Selling Your Preferences)

**Time: ~10 minutes to set up, then it works passively.**

#### Step 1: Create a Shopping Workspace

1. Go to the **Dashboard** and click **New Workspace**
2. Name it something like "My Shopping Lab"
3. **Hire** the `shopping_assistant` agent
   - This automatically hires its required sub-agents: `preference_modeler` and `deal_finder`
   - Cost: 5 credits for the hire + 2 credits each for the sub-agents

#### Step 2: Start Shopping

Chat naturally with the shopping assistant about things you want to buy:

```
You: I need an espresso machine under $500 — semi-automatic, something 
     compact for my small kitchen. I make 2-3 drinks a day.

Shopping Assistant: I'll research that for you. Let me get deal_finder 
     on the comparisons...
     
     [deal_finder returns structured comparison of Breville Bambino Plus, 
      Gaggia Classic Pro, DeLonghi Stilosa]

Shopping Assistant: Based on your requirements — compact, semi-auto, 
     daily use — here's my top recommendation...
```

Keep chatting. Ask about different products. Mention brands you like or dislike. The system learns from every interaction.

#### Step 3: Build Your Profile

After 5+ shopping conversations, ask the assistant to update your profile:

```
You: Update my shopping profile and tag it as "kitchen" and "electronics"

Shopping Assistant: I'll have the preference modeler analyze your 
     interactions and build your profile...
     
     [preference_modeler computes weighted centroid from episode embeddings]

     Done! Your profile has been updated:
     - 12 episodes analyzed
     - Categories: kitchen, electronics  
     - Price sensitivity: 0.65 (moderately budget-conscious)
     - Quality bias: 0.72 (leans premium)
     - Brand affinities: breville (0.85), gaggia (0.62)
```

#### Step 4: List on the Marketplace

When you're ready to monetize:

```
You: List my profile on the marketplace at 5 credits per query

Shopping Assistant: I'll hand this to the embedding broker to set up...

Embedding Broker: Here's what happens when you list:
     - Advertisers can run similarity queries against your profile
     - They see ONLY a similarity score (0.0-1.0), never your actual data
     - You earn 4.25 credits per query (5 minus 15% platform fee)
     - One-time listing fee: 3 credits
     - You can delist any time
     
     Shall I proceed?

You: Yes, list it

Embedding Broker: Listed! Your "kitchen" profile is now on the marketplace 
     at 5 credits/query. You'll earn credits whenever advertisers match 
     against your preferences.
```

### As an Advertiser (Querying Consumer Preferences)

#### Step 1: Go to the Marketplace Dashboard

Navigate to `/marketplace` from the top nav.

#### Step 2: Describe Your Product

In the query builder on the left, describe what you're selling:

```
Premium wireless noise-cancelling headphones designed for remote workers. 
Features: 30-hour battery, transparency mode, multipoint Bluetooth, 
memory foam ear cups. $349 retail.
```

#### Step 3: Set Filters

- Select category chips (e.g., "electronics", "office")
- Set minimum similarity threshold (0.30 is a good starting point)
- Set max results (10 is default)

#### Step 4: Run the Match

Click **Run Match**. The system:

1. Converts your product description into an embedding
2. Runs cosine similarity against all listed consumer profiles
3. Returns matches ranked by similarity

You'll see cards with:
- **Similarity score**: How well your product matches their preferences (0-100%)
- **Price sensitivity**: Are they budget-conscious or premium buyers?
- **Quality bias**: Do they prioritize quality or value?
- **Category tags**: What they shop for
- **Cost**: Credits charged per match

#### Step 5: Use the Insights

High similarity scores (70%+) mean strong alignment. Use these insights to:
- Understand your target audience's preference patterns
- Validate product positioning (are premium buyers matching?)
- Find market segments you didn't know existed

---

## How It Works

### The Core Idea: Reverse SEO

Traditional advertising: Advertiser profiles consumers -> targets ads -> consumer has no control.

Bestiary marketplace: Consumer builds preference profile -> lists it voluntarily -> advertiser pays to query similarity -> consumer earns credits, controls data, can delist any time.

Raw embeddings are **never** exposed. Advertisers only receive similarity scores (0.0 to 1.0).

## Architecture

Three layers work together:

### Layer A: Shopping Assistant (Workspace)

A compound agent workspace where you research purchases:

| Agent | Role | Tools |
|-------|------|-------|
| `shopping_assistant` | Orchestrator | delegate_to_agent, execute_agent, get_shopping_profile |
| `deal_finder` | Product research | Text-only (no tools) |
| `preference_modeler` | Embedding computation | update_shopping_profile, search_knowledge |
| `embedding_broker` | Marketplace management | create_listing, list_marketplace |

The shopping_assistant coordinates the team. As you interact, your episodes accumulate embeddings. After ~5 interactions, the preference_modeler computes a **composite embedding** — a weighted centroid of your episode vectors.

### Layer B: Marketplace API

Server-side cosine similarity matching via pgvector:

```
POST /api/marketplace/match
{
  "product_description": "Premium wireless headphones for remote workers",
  "category_filter": ["electronics"],
  "min_similarity": 0.3,
  "max_results": 10
}
```

The server:
1. Generates an embedding from the product description
2. Runs pgvector cosine similarity against all listed profiles
3. Charges the advertiser per match
4. Credits the consumer
5. Returns similarity scores (never raw embeddings)

### Layer C: Marketplace Dashboard

The `/marketplace` page provides a visual interface for advertisers:
- **Query Builder**: Product description, category filters, similarity threshold
- **Match Results**: Similarity bars, preference metadata, cost breakdown
- **History**: Past queries with scores and credit spend

## Shopping Profiles

Each user can have multiple named profiles (e.g., "electronics", "fitness", "kitchen"):

| Field | Description |
|-------|-------------|
| `composite_embedding` | Weighted centroid of episode embeddings (1024-dim) |
| `embedding_version` | Incremented on each recomputation |
| `episode_count` | Number of episodes used in computation |
| `category_tags` | Product categories covered |
| `price_sensitivity` | 0.0 (insensitive) to 1.0 (very budget-conscious) |
| `quality_bias` | 0.0 (value-focused) to 1.0 (premium-focused) |
| `brand_affinities` | Brand preference scores, e.g., `{"breville": 0.85}` |

### Composite Embedding Computation

The embedding is computed as a weighted centroid:

1. Fetch all episodes with embeddings for the agent
2. Weight by recency: `w = exp(-0.1 * age_days)` (recent interactions matter more)
3. Weight by success: successful episodes = 1.0, errors = 0.3
4. Combine: `composite = L2_normalize(sum(w_i * embedding_i))`

This captures nuanced taste that goes beyond explicit preferences.

## Privacy Model

| What's shared | What's NOT shared |
|---------------|-------------------|
| Similarity score (0.0-1.0) | Raw embedding vectors |
| Category tags | Specific brand names |
| Price sensitivity score | Purchase history |
| Quality bias score | Conversation content |

All queries are logged with a SHA-256 hash of the product vector for audit trail — but no raw data is stored on the advertiser side.

### Consumer Controls

- **List/delist** at any time
- **Set your price** per query (minimum 1 credit)
- **Cap queries** per buyer (privacy limiter)
- **Category tags** control discoverability

## Pricing & Economics

### For Consumers

- One-time listing fee: **3 credits** (configurable)
- Per-query payout: your price minus 15% platform fee
- Example: You set 10 credits/query -> you receive 8.5 credits per match

### For Advertisers

- Per-match cost: listing price + 1 credit platform base fee
- Example: Listing priced at 10 credits -> you pay 11 credits per match
- Batch queries: cost scales with number of matches returned

### Fee Flow

```
Advertiser pays: price_credits + match_base (1 cr)
Platform keeps:  match_base + floor(price * 0.15)
Consumer gets:   price - floor(price * 0.15)
```

### Pricing Strategy

| Profile Type | Episodes | Suggested Price |
|-------------|----------|-----------------|
| Niche (specific category) | 50+ | 5-10 credits |
| General (broad interests) | 20-50 | 2-5 credits |
| New (building up) | < 20 | 1-2 credits |

## Advertiser Workflow

1. Go to `/marketplace`
2. Describe your product in the query builder
3. Select category filters and similarity threshold
4. Click "Run Match"
5. Review results — similarity scores, preference metadata
6. Each match charges credits from your wallet

## Consumer Workflow

1. Create a workspace with `shopping_assistant`
2. Chat about products you're interested in
3. After 5+ interactions, ask the assistant to update your profile
4. Ask the `embedding_broker` to list your profile
5. Set your price and category tags
6. Earn credits when advertisers query your profile
7. Delist any time you want

## API Reference

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/marketplace/match` | Run similarity query (advertiser) |
| `GET` | `/api/marketplace/listings` | Browse active listings |
| `POST` | `/api/marketplace/listings` | Create a listing (consumer) |
| `GET` | `/api/marketplace/history` | Query history (advertiser) |
| `GET` | `/api/shopping/profile` | Get your profiles |
| `PUT` | `/api/shopping/profile/:id/listing` | Update listing |

## Workspace Tools

| Tool | Description |
|------|-------------|
| `get_shopping_profile` | Retrieve profile metadata |
| `update_shopping_profile` | Recompute embedding + update metadata |
| `list_marketplace` | Browse active listings |
| `create_listing` | List profile on marketplace |
