# Agent Bestiary — Economic Model Scenarios

## Cost Basis (February 2026)

### Infrastructure
| Service | Plan | Monthly Cost |
|---------|------|-------------|
| Railway (Rust service) | Pro | $20/mo (includes $20 resource credit) |
| Neon PostgreSQL | Launch | $5/mo minimum (pay-as-you-go) |
| Domain (Name.com) | - | ~$1/mo (~$12/yr) |
| **Total fixed infra** | | **~$26/mo** |

### Variable Costs (per API call)
| Provider | Model | Input (per 1M tok) | Output (per 1M tok) |
|----------|-------|--------------------|--------------------|
| Anthropic | Haiku 4.5 | $1.00 | $5.00 |
| Anthropic | Sonnet 4.5 | $3.00 | $15.00 |
| Anthropic | Opus 4.5 | $5.00 | $25.00 |
| Voyage AI | voyage-3.5 (embed) | $0.06 | - |

### Assumptions for Modeling
- **Average execution**: 2,000 tokens total (800 input + 1,200 output)
- **Default model**: Haiku 4.5 (cheapest, good for most agents)
- **Average execution cost to us**: ~$0.007 per call (800 * $1/1M + 1200 * $5/1M)
- **Embedding per episode**: ~500 tokens → $0.00003 (negligible)
- **DB cost**: ~$0.0001 per query (Neon compute-hours, trivial at scale)

---

## Credit Pricing Assumption

**1 credit = $0.01 USD** (the "penny credit" model)

This means:
- Chat message (1 cr) = $0.01
- Hire agent (5 cr) = $0.05
- Add agent (2 cr) = $0.02
- Execution (~5 cr avg) = $0.05
- Agent creation (education budget, e.g. 50 cr) = $0.50

Users would buy credits in bundles: 100 credits = $1, 1000 credits = $10, etc.

---

## Scenario 1: Early Beta (Month 1-3)

| Metric | Value |
|--------|-------|
| Users | 50 |
| Agents per user | 2 (avg) |
| Workspaces | 20 |
| Messages/user/month | 100 |
| Executions/user/month | 20 |
| Hires/month (total) | 40 |

### Revenue (Layer 1: Credits)
| Action | Volume | Credits | Revenue |
|--------|--------|---------|---------|
| Chat messages | 50 * 100 = 5,000 | 5,000 | $50 |
| Executions | 50 * 20 = 1,000 | ~5,000 (avg 5 cr/exec) | $50 |
| Gas on executions (10%) | 1,000 | ~500 | $5 |
| Agent hires | 40 | 200 | $2 |
| Agent adds | 60 | 120 | $1.20 |
| Agent creation | 100 agents * 50 cr | 5,000 | $50 |
| **Total credit revenue** | | **15,820** | **$158.20** |

### Costs
| Item | Cost |
|------|------|
| Railway | $20 |
| Neon | $5 |
| Domain | $1 |
| Anthropic API (1,000 calls * $0.007) | $7 |
| Voyage embeddings (1,000 * $0.00003) | $0.03 |
| **Total cost** | **$33.03** |

### Margin
- **Gross revenue**: $158.20
- **Total cost**: $33.03
- **Gross margin**: **$125.17 (79%)**

Note: Beta users get free credits, so real revenue = $0. But this shows the *economic value* being generated. When credits are purchased, this is the margin structure.

---

## Scenario 2: Growth (Month 6-12)

| Metric | Value |
|--------|-------|
| Users | 500 |
| Agents per user | 5 (avg) |
| Workspaces | 150 |
| Messages/user/month | 200 |
| Executions/user/month | 50 |
| Hires/month (total) | 300 |

### Revenue (Layer 1: Credits)
| Action | Volume | Credits | Revenue |
|--------|--------|---------|---------|
| Chat messages | 500 * 200 = 100,000 | 100,000 | $1,000 |
| Executions | 500 * 50 = 25,000 | ~125,000 | $1,250 |
| Gas on executions (10%) | 25,000 | ~12,500 | $125 |
| Agent hires | 300 | 1,500 | $15 |
| Agent adds | 500 | 1,000 | $10 |
| Agent creation | 2,500 * 50 cr | 125,000 | $1,250 |
| Consolidation cycles | 500 * 3 cr | 1,500 | $15 |
| **Total credit revenue** | | **366,500** | **$3,665** |

### Costs
| Item | Cost |
|------|------|
| Railway (may need scaling) | $50 |
| Neon (increased compute) | $25 |
| Domain | $1 |
| Anthropic API (25,000 * $0.007) | $175 |
| Voyage embeddings | $0.75 |
| **Total cost** | **$251.75** |

### Margin
- **Gross revenue**: $3,665
- **Total cost**: $251.75
- **Gross margin**: **$3,413.25 (93%)**

---

## Scenario 3: Scale (Month 12-24)

| Metric | Value |
|--------|-------|
| Users | 5,000 |
| Agents per user | 8 (avg) |
| Workspaces | 1,500 |
| Messages/user/month | 300 |
| Executions/user/month | 80 |
| Hires/month (total) | 3,000 |

### Revenue (Layer 1: Credits)
| Action | Volume | Credits | Revenue |
|--------|--------|---------|---------|
| Chat messages | 5,000 * 300 = 1,500,000 | 1,500,000 | $15,000 |
| Executions | 5,000 * 80 = 400,000 | ~2,000,000 | $20,000 |
| Gas on executions (10%) | 400,000 | ~200,000 | $2,000 |
| Agent hires | 3,000 | 15,000 | $150 |
| Agent adds | 5,000 | 10,000 | $100 |
| Agent creation | 40,000 * 50 cr | 2,000,000 | $20,000 |
| Consolidation cycles | 5,000 * 3 cr | 15,000 | $150 |
| **Total credit revenue** | | **5,740,000** | **$57,400** |

### Revenue (Layer 2: Crypto tx fees, if active)
Assume 30% of executions involve paid agents (owner sets price):
- 120,000 paid executions * avg $0.10 agent fee = $12,000 in agent economy
- Platform 2.5% tx fee: **$300/mo**

### Costs
| Item | Cost |
|------|------|
| Railway (dedicated, scaled) | $200 |
| Neon (Scale plan) | $100 |
| Domain | $1 |
| Anthropic API (400,000 * $0.007) | $2,800 |
| Voyage embeddings | $12 |
| Self-hosted Mistral/Qwen (GPU) | $500 |
| **Total cost** | **$3,613** |

### Margin
- **Gross revenue**: $57,400 + $300 = $57,700
- **Total cost**: $3,613
- **Gross margin**: **$54,087 (94%)**

---

## Sensitivity Analysis

### What if average execution is more expensive?

Using Sonnet 4.5 instead of Haiku 4.5:
- Average execution cost: ~$0.022 per call (3x Haiku)
- At 25,000 calls/mo (Scenario 2): $550 vs $175 = +$375/mo
- Still 83% margin at Scenario 2

Using Opus 4.5 for premium agents:
- Average execution cost: ~$0.037 per call
- At 25,000 calls/mo: $925 vs $175 = +$750/mo
- Still 76% margin at Scenario 2

### What if credit price changes?

| Credit price | Scenario 2 revenue | Scenario 2 margin |
|-------------|--------------------|--------------------|
| $0.005 (half penny) | $1,833 | 86% |
| $0.01 (penny) | $3,665 | 93% |
| $0.02 (two cents) | $7,330 | 97% |
| $0.05 (nickel) | $18,325 | 99% |

Even at half-penny credits, the model is profitable at 500 users.

### Break-even analysis

| Scenario | Fixed + Variable cost | Break-even credits needed | Break-even users (at avg usage) |
|----------|----------------------|--------------------------|-------------------------------|
| Penny credits | $33/mo (beta) | 3,300 credits/mo | ~10 active users |
| Penny credits | $252/mo (growth) | 25,200 credits/mo | ~35 active users |
| Penny credits | $3,613/mo (scale) | 361,300 credits/mo | ~63 active users |

---

## Key Insights

1. **Chat messages are the #1 revenue driver** — high volume, pure margin (no API cost). At scale, 1.5M messages/mo = $15K revenue at zero marginal cost.

2. **Execution is the #2 driver but has real cost** — Anthropic API is the main variable cost. Still high margin (Haiku: ~85%, Sonnet: ~70%, Opus: ~60%).

3. **Agent creation (education budget) is a one-time burst** — meaningful early on, declines as a % over time as agents are reused more than created.

4. **Hire/Add fees are negligible revenue** — they're governance mechanisms, not revenue drivers. Could lower without impact.

5. **Self-hosted models dramatically improve margin** — Mistral/Qwen at ~$500/mo GPU covers unlimited inference vs per-token Anthropic pricing. Worth it at ~50K+ calls/mo.

6. **Layer 2 crypto fees are small initially** — 2.5% of a small agent economy. Becomes meaningful only when agent economy is large (>$10K/mo in agent transactions).

7. **The business scales beautifully** — fixed infra costs barely grow while credit revenue grows linearly with users. 94% gross margin at 5K users.

8. **Credit pricing is forgiving** — even at $0.005/credit, model works. Room to be generous with free tiers and still profit.

---

## Recommended Credit Pricing

| Tier | Price per credit | Credits | Total |
|------|-----------------|---------|-------|
| Starter | $0.01 | 100 | $1.00 |
| Builder | $0.008 | 1,000 | $8.00 |
| Pro | $0.006 | 10,000 | $60.00 |
| Enterprise | Custom | Custom | Custom |

Free tier: 100 credits/month (covers ~20 executions or 100 messages — enough to evaluate)
Beta grant: 500 credits (current Ivan allocation)

---

## Agent Owner Pricing (Layer 2: Agent Economy)

Agent owners can set their own prices. The platform facilitates the payment and takes a 2.5% transaction fee. This is the **marketplace** layer — distinct from platform credits (Layer 1).

### Pricing Models Available to Agent Owners

| Model | Description | Example |
|-------|-------------|---------|
| **Per-call** | Flat fee per execution | $0.10 per query |
| **Subscription** | Monthly access fee | $10/mo per portfolio |
| **Tiered** | Volume-based pricing | $5/mo (100 calls), $20/mo (unlimited) |
| **Markup** | % on top of coordinated agent costs | 10% on sub-agent costs |
| **Hybrid** | Subscription + per-call + markup | $10/mo + 10% on sub-agents |

### Fermi: The Coordinator Agent — Worked Example

Fermi is a forecasting research coordinator. It doesn't do research itself — it orchestrates specialized agents (market_research, sentiment_analyzer, monte_carlo_sim, etc.) and synthesizes their outputs into portfolio-level forecasts.

**Fermi's pricing (set by owner):**
- $10/month per portfolio (subscription)
- 10% markup on the sum of all sub-agent and executor costs it coordinates

**What happens when a user runs Fermi on a portfolio:**

```
User: "Analyze my tech portfolio for Q3 risk"

Fermi orchestrates:
  1. market_research agent    → 3,000 tokens (Sonnet)  → $0.051 API cost
  2. sentiment_analyzer agent → 1,500 tokens (Haiku)   → $0.008 API cost
  3. monte_carlo_sim agent    → 0 tokens (deterministic) → $0.00 API cost
  4. Fermi synthesis          → 2,000 tokens (Sonnet)  → $0.034 API cost
                                                  Total: $0.093 API cost
```

**Cost flow for this single execution:**

| What | Amount | Paid by | Received by |
|------|--------|---------|-------------|
| **Layer 1: Platform credits** | | | |
| Execution gas (4 agent calls) | ~12 credits ($0.12) | User (workspace) | Platform |
| Gas surcharge (10%) | ~1 credit ($0.01) | User (workspace) | Platform |
| **Layer 2: Agent economy** | | | |
| market_research owner fee | $0.05 per call | User | market_research owner |
| sentiment_analyzer owner fee | $0.02 per call | User | sentiment_analyzer owner |
| monte_carlo_sim owner fee | $0.01 per call | User | monte_carlo_sim owner |
| Sub-agent total | $0.08 | | |
| Fermi 10% markup on sub-agents | $0.008 | User | Fermi owner |
| Fermi subscription (amortized) | $0.33/day | User | Fermi owner |
| **Platform 2.5% tx fee on all Layer 2** | $0.010 | Deducted from payouts | Platform |

**User's total cost for one Fermi call:**
- Platform credits: $0.13 (13 credits)
- Agent fees: $0.008 (Fermi markup) + $0.08 (sub-agents) + $0.33 (subscription/day) = ~$0.42
- **Total: ~$0.55 per portfolio analysis**

**Who earns what:**

| Party | Per call | Monthly (daily portfolio analysis) |
|-------|----------|------------------------------------|
| Fermi owner | $0.008 markup + $10/mo sub | $10.24/mo |
| market_research owner | $0.05/call | $1.50/mo |
| sentiment_analyzer owner | $0.02/call | $0.60/mo |
| monte_carlo_sim owner | $0.01/call | $0.30/mo |
| **Platform (Layer 1 credits)** | $0.13/call | **$3.90/mo** |
| **Platform (Layer 2 tx fee)** | $0.010/call | **$0.31/mo** |

---

### Scaling the Agent Economy: 100 Fermi Subscribers

| Metric | Value |
|--------|-------|
| Fermi subscribers | 100 |
| Portfolios per subscriber | 3 (avg) |
| Analyses per portfolio/month | 20 (roughly daily on trading days) |
| Total Fermi executions/month | 6,000 |

**Monthly flows:**

| Party | Monthly Revenue |
|-------|----------------|
| Fermi owner | 100 subs * $10/mo + 6,000 * $0.008 markup = **$1,048** |
| market_research owner | 6,000 * $0.05 = **$300** |
| sentiment_analyzer owner | 6,000 * $0.02 = **$120** |
| monte_carlo_sim owner | 6,000 * $0.01 = **$60** |
| **Agent economy total** | **$1,528/mo** |
| Platform Layer 1 (credits) | 6,000 * 13 cr = 78,000 credits = **$780** |
| Platform Layer 2 (2.5% tx fee) | 2.5% * $1,528 = **$38.20** |
| **Platform total** | **$818.20/mo from Fermi alone** |
| Platform API costs | 6,000 * $0.093 = **$558** |
| **Platform net from Fermi** | **$260.20/mo** |

### Scaling further: 10 "Fermi-class" coordinator agents on the platform

If there are 10 coordinator agents like Fermi, each with ~100 subscribers:

| Flow | Monthly |
|------|---------|
| Agent economy (all owners) | $15,280 |
| Platform credits (Layer 1) | $7,800 |
| Platform tx fees (Layer 2) | $382 |
| Platform API costs | $5,580 |
| **Platform net revenue** | **$2,602/mo** |
| **Agent owner net revenue** | **$14,898/mo** |

---

### The Full Picture: Platform + Agent Economy Combined

At **Scenario 3 scale** (5,000 users) with a mature agent marketplace:

| Revenue stream | Monthly |
|----------------|---------|
| Platform credits (all users, all actions) | $57,400 |
| Platform tx fees (2.5% of agent economy) | $382 - $3,820 |
| Platform API costs | -$3,613 to -$8,400 |
| Platform infra | -$800 |
| **Platform net** | **$45,000 - $49,000/mo** |
| Agent owner earnings (total ecosystem) | $15,000 - $150,000/mo |

The agent economy **doesn't compete** with platform revenue — it **amplifies** it. Every dollar flowing to agent owners also generates credit spend and tx fees for the platform. The more successful agent owners are, the more the platform earns.

---

### Agent Pricing Guidelines (for owners)

| Agent type | Suggested pricing | Rationale |
|------------|------------------|-----------|
| Simple utility (sentiment, summarizer) | $0.01 - $0.05 per call | Low value-add, high volume |
| Specialized research (market, legal) | $0.05 - $0.25 per call | Domain expertise premium |
| Coordinator (Fermi-class) | $5-$50/mo subscription + 5-15% markup | High value orchestration |
| Deterministic (Monte Carlo, scoring) | $0.005 - $0.02 per call | No LLM cost, pure compute |
| Premium/enterprise | $100-$500/mo + per-call | White-glove, custom models |

Owners are free to set any price. The market will discover equilibrium. Platform takes 2.5% regardless.
