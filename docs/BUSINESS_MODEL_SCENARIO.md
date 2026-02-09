# Agent Bestiary — Business Model Scenario

## Executive Summary

Agent Bestiary is a dual-revenue platform for AI agent orchestration:

1. **Credits (Layer 1)**: Users buy credits to operate on the platform. Every action costs credits. This is the product.
2. **Agent Economy (Layer 2)**: Agent owners set prices for their agents. Users pay owners directly. The platform takes a 2.5% transaction fee on all payouts.

This document models the business from beta through 1M users.

---

## Foundations

### Credit Model (Layer 1 — The Product)

| Parameter | Value |
|-----------|-------|
| Base credit price | $0.01 USD |
| Volume discount (1K+) | $0.008 |
| Volume discount (10K+) | $0.006 |
| Free tier | 100 credits/month |

| Action | Credit Cost |
|--------|------------|
| Chat message | 1 |
| Execute agent (per 1K tokens) | 1 + 10% gas |
| Hire agent to workspace | 5 |
| Add own agent to workspace | 2 |
| Create agent (education budget) | 20-100 |
| Consolidation cycle | 3 |

### Agent Economy (Layer 2 — The Marketplace)

Agent owners set their own prices using any combination of:

| Pricing model | Example |
|---------------|---------|
| Per-call | $0.05 per execution |
| Subscription | $10/month |
| Markup (coordinator agents) | 10% on sub-agent costs |
| Tiered | $5/mo (100 calls), $20/mo (unlimited) |
| Hybrid | Subscription + per-call + markup |

**Platform take rate: 2.5% on all Layer 2 payouts.**

### Cost Basis

| Cost | Per unit | Notes |
|------|----------|-------|
| LLM API (Haiku 4.5) | $0.007/call | 800 in + 1,200 out tokens avg |
| LLM API (Sonnet 4.5) | $0.022/call | 3x Haiku |
| LLM API (Opus 4.5) | $0.037/call | 5x Haiku |
| Embedding (Voyage 3.5) | $0.00003/call | Negligible |
| Self-hosted inference | ~$0.001/call | At scale with dedicated GPU |
| DB (Neon) | $0.0001/query | Negligible |
| Chat message | $0.00 | No API call, pure margin |

### Model Mix Assumption (at scale)

| Model | % of executions | Avg cost/call |
|-------|----------------|---------------|
| Haiku 4.5 | 50% | $0.007 |
| Sonnet 4.5 | 30% | $0.022 |
| Self-hosted (Mistral/Qwen) | 15% | $0.001 |
| Opus 4.5 | 3% | $0.037 |
| Deterministic | 2% | $0.000 |
| **Blended average** | | **$0.011** |

---

## The Fermi Agent — Reference Case

Fermi is a forecasting research coordinator. It orchestrates specialist agents and synthesizes portfolio-level analysis. It demonstrates the coordinator pricing model.

### Fermi Pricing (set by owner)
- **$10/month per portfolio** (subscription)
- **10% markup** on sum of sub-agent + executor costs

### Single Execution Breakdown

```
User: "Analyze tech portfolio Q3 risk"

Fermi orchestrates:
  1. market_research     → 3,000 tok (Sonnet)      → $0.051 API cost
  2. sentiment_analyzer  → 1,500 tok (Haiku)        → $0.008 API cost
  3. monte_carlo_sim     → deterministic             → $0.000 API cost
  4. Fermi synthesis     → 2,000 tok (Sonnet)        → $0.034 API cost
                                           API total: $0.093
```

### Who Pays What — Single Call

| Flow | Amount | From → To |
|------|--------|-----------|
| Platform credits (4 calls + gas) | $0.13 | User → Platform |
| market_research owner fee | $0.05 | User → Owner A |
| sentiment_analyzer owner fee | $0.02 | User → Owner B |
| monte_carlo_sim owner fee | $0.01 | User → Owner C |
| Fermi 10% markup | $0.008 | User → Fermi Owner |
| Fermi subscription (amortized daily) | $0.33 | User → Fermi Owner |
| Platform 2.5% tx fee | $0.010 | Deducted from payouts |
| **User total** | **$0.55** | |

### Fermi at Scale

| Scale | Subscribers | Executions/mo | Fermi owner earns | Platform earns (L1+L2) | Platform API cost |
|-------|-------------|---------------|--------------------|-----------------------|-------------------|
| Early | 10 | 600 | $105/mo | $82/mo | $56/mo |
| Growth | 100 | 6,000 | $1,048/mo | $818/mo | $558/mo |
| Scale | 1,000 | 60,000 | $10,480/mo | $8,182/mo | $5,580/mo |
| Mature | 10,000 | 600,000 | $104,800/mo | $81,820/mo | $55,800/mo |

---

## Growth Trajectory: Beta to 1 Million Users

### User Growth Assumptions

| Phase | Timeline | Users | MoM Growth | Characteristics |
|-------|----------|-------|------------|-----------------|
| Beta | M1-M3 | 50 → 200 | 60% | Free credits, dogfooding |
| Seed | M4-M6 | 200 → 1,000 | 70% | First paying users, word of mouth |
| Growth | M7-M12 | 1K → 10K | 45% | Product-market fit, agent marketplace emerges |
| Scale | M13-M18 | 10K → 50K | 30% | Coordinator agents, enterprise interest |
| Expansion | M19-M24 | 50K → 200K | 25% | Multi-vertical, API partnerships |
| Mass | M25-M36 | 200K → 1M | 15% | Network effects, self-sustaining ecosystem |

### Per-User Behavior Assumptions (mature platform)

| Segment | % of users | Msgs/mo | Execs/mo | Agents owned | Agent $ spend/mo |
|---------|-----------|---------|----------|-------------|-----------------|
| Free tier | 40% | 50 | 10 | 1 | $0 |
| Casual | 25% | 150 | 30 | 3 | $5 |
| Active | 20% | 400 | 100 | 8 | $25 |
| Power | 10% | 800 | 250 | 15 | $75 |
| Enterprise | 5% | 1,500 | 500 | 30 | $200 |

### Blended Per-User Averages

| Metric | Value |
|--------|-------|
| Messages/user/month | 310 |
| Executions/user/month | 95 |
| Agents owned (avg) | 7 |
| Agent economy spend/user/month | $26 |
| Platform credits/user/month | ~850 credits |
| Platform credit revenue/user/month | $6.80 (at blended $0.008/credit) |

---

## Monthly P&L by Phase

### Phase 1: Beta (Month 3 — 200 users)

| Line | Monthly |
|------|---------|
| **Revenue** | |
| Credit revenue (mostly free grants) | $0 (real) / $1,360 (economic value) |
| Agent economy tx fees | $0 |
| **Costs** | |
| Infrastructure (Railway + Neon) | -$26 |
| LLM API (19K executions * $0.007) | -$133 |
| Team (founders, no salary) | $0 |
| **Net** | **-$159/mo** |
| **Burn** | **$159/mo** |

### Phase 2: Seed (Month 6 — 1,000 users)

| Line | Monthly |
|------|---------|
| **Revenue** | |
| Credit revenue | $4,080 |
| Agent economy tx fees (2.5% of $5K) | $125 |
| **Total revenue** | **$4,205** |
| **Costs** | |
| Infrastructure | -$75 |
| LLM API (95K execs * $0.009 blended) | -$855 |
| Embeddings | -$3 |
| **Total COGS** | **-$933** |
| **Gross profit** | **$3,272 (78%)** |
| Team (2 engineers, part-time) | -$5,000 |
| **Net** | **-$1,728/mo** |

### Phase 3: Growth (Month 12 — 10,000 users)

| Line | Monthly |
|------|---------|
| **Revenue** | |
| Credit revenue | $54,400 |
| Agent economy tx fees (2.5% of $130K) | $3,250 |
| **Total revenue** | **$57,650** |
| **Costs** | |
| Infrastructure (scaled) | -$500 |
| LLM API (950K execs * $0.011 blended) | -$10,450 |
| Self-hosted GPU (Mistral/Qwen) | -$2,000 |
| Embeddings | -$30 |
| **Total COGS** | **-$12,980** |
| **Gross profit** | **$44,670 (77%)** |
| Team (5 engineers + 1 ops) | -$30,000 |
| Marketing/growth | -$5,000 |
| **Net (pre-tax)** | **$9,670/mo** |
| **Annual run rate** | **$116K ARR profit** |

### Phase 4: Scale (Month 18 — 50,000 users)

| Line | Monthly |
|------|---------|
| **Revenue** | |
| Credit revenue | $272,000 |
| Agent economy tx fees (2.5% of $650K) | $16,250 |
| Enterprise contracts (10 @ $2K/mo) | $20,000 |
| **Total revenue** | **$308,250** |
| **Costs** | |
| Infrastructure (multi-region) | -$3,000 |
| LLM API (4.75M execs * $0.011) | -$52,250 |
| Self-hosted GPU cluster | -$8,000 |
| Embeddings | -$150 |
| **Total COGS** | **-$63,400** |
| **Gross profit** | **$244,850 (79%)** |
| Team (12 eng + 3 biz + 2 ops) | -$120,000 |
| Marketing | -$25,000 |
| Legal/compliance | -$10,000 |
| Office/misc | -$5,000 |
| **Net (pre-tax)** | **$84,850/mo** |
| **Annual run rate** | **$1.02M ARR profit** |

### Phase 5: Expansion (Month 24 — 200,000 users)

| Line | Monthly |
|------|---------|
| **Revenue** | |
| Credit revenue | $1,088,000 |
| Agent economy tx fees (2.5% of $2.6M) | $65,000 |
| Enterprise contracts (50 @ $3K/mo avg) | $150,000 |
| **Total revenue** | **$1,303,000** |
| **Costs** | |
| Infrastructure (dedicated, multi-region) | -$15,000 |
| LLM API (19M execs * $0.009 — better rates) | -$171,000 |
| Self-hosted GPU fleet | -$25,000 |
| Embeddings | -$600 |
| **Total COGS** | **-$211,600** |
| **Gross profit** | **$1,091,400 (84%)** |
| Team (30 people) | -$350,000 |
| Marketing | -$75,000 |
| Legal/compliance/security | -$30,000 |
| Office/ops | -$25,000 |
| **Net (pre-tax)** | **$611,400/mo** |
| **Annual run rate** | **$7.3M ARR profit** |

### Phase 6: Mass (Month 36 — 1,000,000 users)

| Line | Monthly |
|------|---------|
| **Revenue** | |
| Credit revenue | $5,440,000 |
| Agent economy tx fees (2.5% of $13M) | $325,000 |
| Enterprise contracts (200 @ $5K/mo avg) | $1,000,000 |
| API licensing / white-label | $200,000 |
| **Total revenue** | **$6,965,000** |
| **Costs** | |
| Infrastructure (global, redundant) | -$80,000 |
| LLM API (95M execs * $0.007 — volume deals) | -$665,000 |
| Self-hosted GPU fleet | -$120,000 |
| Embeddings | -$3,000 |
| **Total COGS** | **-$868,000** |
| **Gross profit** | **$6,097,000 (88%)** |
| Team (80 people) | -$1,200,000 |
| Marketing | -$250,000 |
| Legal/compliance/security | -$100,000 |
| Office/ops | -$100,000 |
| Customer success | -$150,000 |
| R&D (model training, custom infra) | -$200,000 |
| **Total OpEx** | **-$2,000,000** |
| **Net (pre-tax)** | **$4,097,000/mo** |
| **Annual run rate** | **$49.2M ARR profit** |
| **Annual revenue** | **$83.6M** |

---

## Revenue Composition at 1M Users

| Stream | Monthly | % of Total |
|--------|---------|-----------|
| Credit sales (Layer 1) | $5,440,000 | 78.1% |
| Agent economy tx fees (Layer 2) | $325,000 | 4.7% |
| Enterprise contracts | $1,000,000 | 14.4% |
| API licensing | $200,000 | 2.9% |
| **Total** | **$6,965,000** | 100% |

### Credit Revenue Breakdown at 1M Users

| Action | Volume/mo | Credits | Revenue |
|--------|-----------|---------|---------|
| Chat messages | 310M | 310,000,000 | $2,480,000 |
| Executions | 95M | 475,000,000 | $1,900,000* |
| Gas surcharge (10%) | 95M | 47,500,000 | $190,000* |
| Agent creation | 500K new agents * 50 cr | 25,000,000 | $200,000 |
| Hire/Add | 2M actions | 7,000,000 | $56,000 |
| Consolidation | 1M cycles | 3,000,000 | $24,000 |
| Other platform actions | - | ~75,000,000 | $590,000 |
| **Total** | | **~942M credits** | **$5,440,000** |

*At blended $0.008/credit for volume buyers.

---

## Agent Economy at 1M Users

| Metric | Value |
|--------|-------|
| Total agents on platform | 7,000,000 |
| Agents with owner pricing (paid) | 350,000 (5%) |
| Paid agent executions/month | 28,500,000 (30% of total) |
| Average agent owner fee | $0.12/call + subscriptions |
| Agent economy GMV | $13,000,000/mo |
| Platform tx fee (2.5%) | $325,000/mo |
| Number of agent owners earning >$100/mo | ~15,000 |
| Number of agent owners earning >$1K/mo | ~2,000 |
| Number of agent owners earning >$10K/mo | ~200 |
| Top coordinator agents (>$100K/mo) | ~20 |

### Agent Owner Earnings Distribution

| Percentile | Monthly earnings |
|-----------|-----------------|
| Top 0.01% (20 agents) | $100K - $500K |
| Top 0.1% (200 agents) | $10K - $100K |
| Top 1% (2,000 agents) | $1K - $10K |
| Top 10% (15,000 agents) | $100 - $1K |
| Median paid agent | $15 - $50 |

This follows a power law — coordinator agents that orchestrate many sub-agents and serve enterprise customers capture the most value, similar to app store economics.

---

## Growth Chart Summary

| Month | Users | Revenue/mo | COGS/mo | Gross margin | Net/mo | ARR |
|-------|-------|-----------|---------|-------------|--------|-----|
| 3 | 200 | $0* | $159 | -100% | -$159 | - |
| 6 | 1K | $4.2K | $933 | 78% | -$1.7K | - |
| 12 | 10K | $57.7K | $13K | 77% | $9.7K | $116K |
| 18 | 50K | $308K | $63K | 79% | $85K | $1.0M |
| 24 | 200K | $1.3M | $212K | 84% | $611K | $7.3M |
| 36 | 1M | $7.0M | $868K | 88% | $4.1M | $49.2M |

*Beta period — free credits, no real revenue.

---

## Key Business Model Properties

### 1. High-Margin, Improving with Scale
Gross margin goes from 78% to 88% because:
- Self-hosted models replace per-token API costs
- Volume deals with LLM providers reduce rates
- Chat messages (zero marginal cost) grow as % of revenue
- Fixed infra amortizes across more users

### 2. Two-Sided Network Effects
- More agents → more users (better selection)
- More users → more agent owners (bigger market)
- More coordinator agents → more sub-agent usage (compounding value)
- Agent quality improves via ADM consolidation (learning flywheel)

### 3. Credits Are the Product, Not the Cost
Users don't pay for compute — they pay for **agent access and orchestration**. The credit abstracts away infrastructure complexity. This means:
- Price is anchored to value delivered, not cost incurred
- Chat messages at $0.01 feel cheap to users but are pure margin
- We can lower costs (better models, self-hosting) without lowering prices

### 4. Agent Economy is a Multiplier
Every $1 in agent economy GMV generates:
- $0.025 in tx fees (Layer 2)
- ~$0.06 in platform credits (Layer 1, from the underlying executions)
- Total platform take: ~8.5% of agent economy GMV
- This is better than most marketplaces (Stripe: 2.9%, App Store: 30%, but our volume is higher)

### 5. Enterprise is the Upside
Enterprise contracts ($5K-$50K/mo) are:
- Predictable recurring revenue
- Higher margin (dedicated support amortizes)
- Driven by coordinator agents solving real business problems (Fermi = portfolio analysis, legal research coordinator, supply chain optimizer, etc.)

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| LLM API price increases | COGS rises | Self-hosted fleet, multi-provider, model mix |
| LLM API prices drop dramatically | Users question credit pricing | Lower credit prices, increase free tier, compete on orchestration value not compute cost |
| Low agent economy adoption | Layer 2 revenue stays small | Layer 2 is upside, not the core business. Layer 1 credits sustain independently |
| Security/abuse | Platform reputation | Rate limiting, spending caps, audit logging (credit ledger provides full trail) |
| Competition (other agent platforms) | User acquisition harder | Network effects from agent ecosystem, ADM learning flywheel is defensible IP |
| Crypto regulation | Layer 2 complications | Keep Layer 2 optional. Layer 1 works with fiat-only. SIWE is additive, not required |

---

## Fundraising Milestones (if applicable)

| Round | Timing | Users | ARR | Raise |
|-------|--------|-------|-----|-------|
| Pre-seed | M3-M6 | 200-1K | Pre-revenue | $500K-$1M |
| Seed | M9-M12 | 5K-10K | $100K+ | $2M-$5M |
| Series A | M18-M24 | 50K-200K | $1M-$7M | $10M-$25M |
| Series B | M30-M36 | 500K-1M | $25M-$50M | $50M-$100M |

---

*Last updated: 2026-02-09*
*Model assumes penny-credit base price with volume discounts.*
*All figures are monthly unless noted. ARR = annualized net profit run rate.*
