# Agent Bestiary — Business Value Scenarios

These scenarios illustrate how the three-tier revenue model creates real economic value for different types of agent owners and users. Each scenario shows the full value chain: what the owner brings, what the agent does, how users pay, and how all three revenue tiers activate.

---

## Scenario 1: Licensed Generative Art — The Creative Agent

### The Owner
A visual artist with 15 years of work — paintings, illustrations, commercial pieces. Their style is distinctive and recognizable. Today they license individual pieces for $200-$2,000 each through galleries and stock platforms that take 30-50% commission.

### The Agent
The artist creates a generative art agent on the platform. They upload their body of work as the training corpus — hundreds of high-resolution pieces that define their visual language. The agent learns their style through the AKP pipeline: color palettes, composition patterns, texture preferences, thematic tendencies. It doesn't copy — it generates new works that are authentically "in the style of" the artist.

The agent's ontology evolves: it learns which prompts produce the best results, what compositional rules define the artist's voice, which color relationships recur. The more it's used, the more refined its understanding becomes.

### The Pricing
- **$25/month subscription** — access to generate up to 50 images/month
- **$2 per generation** beyond the subscription limit
- **$50 per commercial license** — usage rights for generated work in commercial contexts
- Artist retains copyright on the training corpus. Generated works are licensed, not sold.

### The Value Chain

| Party | What they get | Revenue |
|-------|--------------|---------|
| **Artist (agent owner)** | Passive income from their body of work. Scales infinitely — one artist can serve 10,000 subscribers simultaneously. No per-piece production cost. | $25/sub/mo + $2/gen + $50/commercial |
| **User (designer, brand, agency)** | On-demand access to a specific artist's aesthetic without commissioning individual pieces. Faster iteration, lower cost per asset. | Pays subscription + per-gen |
| **Platform (Tier 1: Credits)** | Every generation costs credits (execution + embedding + storage). Chat with agent for art direction costs credits. | ~8 credits/generation |
| **Platform (Tier 2: A2A)** | Agent learns from each generation — what prompts work, what the artist's style boundaries are. Consolidation cycles run autonomously. | ~55 credits/month/agent |
| **Platform (Tier 3: Marketplace)** | 2.5% on all subscription and per-generation payments from user to artist. | 2.5% of agent revenue |

### At Scale
- 500 artists on the platform, averaging 200 subscribers each
- Monthly: 100,000 subscribers * $25 = $2.5M in artist revenue
- Platform Tier 3: $62,500/mo
- Platform Tier 1: 100K subs * 30 gens/mo * 8 cr = 24M credits = $192,000/mo
- Platform Tier 2: 500 agents * 55 cr = 27,500 credits = $220/mo
- **Total platform revenue from creative vertical: ~$255K/mo**

### Why This Beats the Status Quo
- **For the artist**: A gallery takes 50%. Stock platforms take 30-50%. Agent Bestiary takes 2.5%. The artist keeps 97.5% of their agent revenue while reaching a global audience 24/7.
- **For the user**: Commissioning an artist takes weeks and costs $500-$5,000 per piece. Generating a work in their style takes seconds at $2.
- **For the platform**: Pure marketplace economics. The artist brings the value (their corpus), the platform provides the infrastructure (AKP, generation, licensing, payment).

---

## Scenario 2: Domain Expert Knowledge Agent — The Consultant

### The Owner
A tax attorney with 20 years of experience in international corporate tax structuring. They bill $600/hour. They're capacity-constrained — can serve maybe 30 clients/year personally.

### The Agent
The attorney creates a tax advisory agent. They feed it their accumulated knowledge: anonymized case studies, ruling interpretations, structuring frameworks, jurisdictional expertise. Not their client data — their *methodology*. The agent learns through AKP: tax code relationships, jurisdictional rules, common structuring patterns, risk factors.

The agent can't replace the attorney for complex engagements. But it can handle the 80% of queries that are well-understood: "What's the withholding tax on dividends from a Dutch subsidiary to a US parent?" "Walk me through the transfer pricing documentation requirements for Ireland." "Compare the tax treaty networks of Singapore vs Hong Kong for our holding structure."

### The Pricing
- **$200/month subscription** — unlimited queries for a single entity
- **$500/month enterprise** — unlimited queries, team access, audit trail
- **$2,000 one-time** — deep analysis report on a specific structure (agent + attorney review)

### The Value Chain

| Party | What they get | Revenue |
|-------|--------------|---------|
| **Attorney (agent owner)** | Monetize expertise beyond billable hours. Serve 1,000 clients simultaneously instead of 30. The agent handles routine queries; the attorney handles complex ones (and charges more for them). | $200-$500/sub/mo |
| **User (CFO, in-house counsel)** | Instant access to specialist knowledge at 1/100th the hourly rate. Available 24/7. Consistent, documented answers. | Pays subscription |
| **Platform (Tier 1)** | Heavy execution — tax queries are complex, often 5,000+ tokens per call. High credit consumption. | ~25 credits/query |
| **Platform (Tier 2)** | Agent constantly learning — new rulings, new case outcomes, new jurisdictional changes. Premium AKP budget (owner invests in agent education). | ~200 credits/month |
| **Platform (Tier 3)** | 2.5% of subscription revenue. | 2.5% of agent revenue |

### At Scale
- 200 expert agents (tax, legal, medical, engineering, compliance)
- Average 100 subscribers per agent at $300/mo
- Monthly: 20,000 subscribers * $300 = $6M in expert agent revenue
- Platform Tier 3: $150,000/mo
- Platform Tier 1: 20K subs * 60 queries/mo * 25 cr = 30M credits = $240,000/mo
- Platform Tier 2: 200 agents * 200 cr = 40,000 credits = $320/mo
- **Total platform revenue from expert vertical: ~$390K/mo**

### Why This Beats the Status Quo
- **For the expert**: They bill $600/hr for 1,500 hours/year = $900K revenue, working constantly. With an agent serving 500 subscribers at $300/mo, that's $150K/mo = $1.8M/year, mostly passive. They work less, earn more.
- **For the user**: A tax attorney consultation costs $600-$1,200/hour. The agent costs $300/month for unlimited queries. For routine questions, this is a 90%+ cost reduction with instant availability.
- **For the platform**: Expert agents have the highest per-query credit consumption (long, complex prompts) and the highest Tier 2 activity (constant knowledge updates). High-value vertical.

---

## Scenario 3: Research Coordinator — The Fermi Model

### The Owner
A quantitative research firm that builds portfolio analysis methodology. They've spent years developing frameworks for market research, sentiment analysis, risk modeling, and macro forecasting.

### The Agent
Fermi — a coordinator agent that doesn't do research itself. It orchestrates a team of specialist agents: market_research (scrapes and synthesizes market data), sentiment_analyzer (processes news/social signals), monte_carlo_sim (runs risk simulations), macro_forecaster (economic indicators). Fermi's value is in the orchestration: knowing which agents to call, in what order, how to synthesize their outputs into a coherent portfolio view.

Through AKP, Fermi learns: which agent combinations produce the best forecasts, what market conditions require deeper analysis, where sentiment signals are predictive vs noise, how to weight conflicting signals.

### The Pricing
- **$10/month per portfolio** subscription
- **10% markup** on all sub-agent execution costs
- **$100/month premium tier** — daily automated analysis with alerts

### The Value Chain

| Party | What they get | Revenue |
|-------|--------------|---------|
| **Firm (Fermi owner)** | Productized research methodology. Each subscriber generates revenue with zero marginal analyst time. Sub-agent costs are passed through + markup. | $10-$100/portfolio/mo + 10% markup |
| **Sub-agent owners** | Revenue from Fermi's orchestration. They don't need their own subscribers — Fermi brings the demand. | Per-call fees ($0.01-$0.05) |
| **User (portfolio manager, trader)** | Automated, consistent research pipeline. What used to require a team of analysts runs on demand. | $10-$100/portfolio/mo |
| **Platform (all 3 tiers)** | Coordinator agents are the highest-value topology — they drive credit consumption across multiple agents, generate A2A learning across the entire chain, and the marketplace fees compound through the sub-agent graph. | Multi-tier revenue |

### At Scale
(Detailed in BUSINESS_MODEL_SCENARIO.md — Fermi section)
- 1,000 Fermi subscribers: $10.5K/mo to owner, $8.2K/mo to platform
- 10 coordinator agents of this class: $26K/mo to platform

### Why Coordinators Are the Killer App
Coordinator agents create a **multiplier topology**:
```
1 Fermi execution = 4 sub-agent executions + 4 AKP learning triggers
                   = 4x credit consumption
                   = 4x A2A revenue
                   = marketplace fees at every node in the graph
```
Platforms win when they enable composition. Coordinator agents are the composition layer.

---

## Scenario 4: Enterprise Workflow Agent — The Operations Coordinator

### The Owner
A supply chain consulting firm that builds procurement optimization workflows. They've developed methodology for supplier evaluation, RFQ analysis, logistics optimization, and compliance checking across industries.

### The Agent
A procurement coordinator agent that automates the RFQ-to-PO pipeline. It orchestrates:
- **supplier_evaluator**: scores suppliers on quality, reliability, cost, ESG compliance
- **market_price_analyzer**: compares quoted prices against market benchmarks
- **logistics_optimizer**: models shipping routes, lead times, tariff impacts
- **compliance_checker**: validates against regulatory requirements per jurisdiction
- **contract_analyzer**: reviews terms against company standards (deterministic rule engine + LLM interpretation)

The agent doesn't replace procurement teams — it gives them superpowers. What took a procurement analyst 2 weeks to evaluate now takes the agent 10 minutes to pre-screen, with the analyst reviewing and approving the agent's recommendations.

### The Pricing
- **$500/month base** — up to 50 RFQ analyses/month
- **$2,000/month enterprise** — unlimited analyses, API integration, custom compliance rules
- **15% markup** on sub-agent costs
- **Success fee option**: 0.5% of documented savings (advanced tier)

### The Value Chain

| Party | What they get | Revenue |
|-------|--------------|---------|
| **Consulting firm (owner)** | Productized IP. Every enterprise client they couldn't serve manually is now accessible through the agent. Consulting engagement converts to SaaS subscription. | $500-$2,000/client/mo |
| **Sub-agent owners** | Specialized agents (logistics, compliance, contract) earn per-call from the coordinator. Market-making for niche expertise. | Per-call fees |
| **User (enterprise procurement)** | 10x faster RFQ evaluation. Consistent methodology. Audit trail. Measurable savings. | $500-$2,000/mo vs $50K+ consulting engagement |
| **Platform** | Enterprise agents are the highest credit consumers — complex multi-agent orchestrations, large token counts, frequent learning cycles, premium AKP budgets. | All three tiers, premium volumes |

### At Scale
- 50 enterprise workflow agents across verticals (procurement, HR, legal ops, financial planning, manufacturing)
- Average 40 enterprise subscribers at $1,500/mo
- Monthly: 2,000 enterprise subs * $1,500 = $3M in agent revenue
- Platform Tier 3: $75,000/mo
- Platform Tier 1: 2K subs * 200 complex queries/mo * 50 cr = 20M credits = $160,000/mo
- Platform Tier 2: 50 agents * 200 cr + 250 sub-agents * 55 cr = 23,750 credits = $190/mo
- **Total platform revenue from enterprise vertical: ~$235K/mo**

---

## Scenario 5: Open Source Intelligence (OSINT) — The Investigator

### The Owner
A former intelligence analyst who's built methodology for open-source investigation — corporate due diligence, political risk assessment, sanctions screening, beneficial ownership tracing.

### The Agent
An OSINT coordinator that synthesizes publicly available information into structured intelligence reports. It orchestrates:
- **entity_resolver**: disambiguates people, companies, addresses across public databases
- **network_mapper**: traces corporate structures, board relationships, ownership chains
- **sanctions_screener**: checks against OFAC, EU, UN sanctions lists (deterministic)
- **news_monitor**: synthesizes recent news, court filings, regulatory actions
- **risk_scorer**: aggregates signals into a risk profile with confidence scores

The AKP pipeline is critical here: the agent builds a growing knowledge graph of entities, relationships, and risk indicators. Each investigation enriches the graph. Over time, the agent develops institutional memory — it recognizes patterns across investigations that a human analyst might miss.

### The Pricing
- **$100/month** — 10 entity investigations/month
- **$500/month pro** — 100 investigations, API access, continuous monitoring
- **$2,000/month enterprise** — unlimited, custom watchlists, team access, audit trail

### The Value Chain

| Party | What they get | Revenue |
|-------|--------------|---------|
| **Analyst (owner)** | Productized investigative methodology. The knowledge graph compounds — every investigation makes the next one better. | $100-$2,000/client/mo |
| **User (compliance officer, investor, journalist)** | Due diligence that used to take a team 2 weeks now takes 15 minutes for the initial screen. Professional-grade OSINT without hiring investigators. | $100-$2,000/mo |
| **Platform (Tier 2 emphasis)** | OSINT agents have the richest AKP activity — massive entity extraction, fact consolidation, community detection across investigations. The knowledge graph IS the product. | Premium A2A consumption |

### Why OSINT Demonstrates the AKP Moat

This is where Tier 2 (A2A/AKP) becomes the primary competitive advantage:

```
Investigation 1: Researches Company A → extracts 50 entities, 200 facts
Investigation 2: Researches Company B → discovers 3 shared directors with Company A
Investigation 500: New query about Company C → agent already knows the network
                   because it's been building the graph for 6 months
```

The agent's accumulated knowledge graph — built through hundreds of paid investigations, each consuming AKP credits — is an asset that cannot be replicated without the same volume of paid usage. This is the moat made concrete.

---

## Cross-Scenario Platform Economics

| Vertical | Agents | Subscribers | Agent economy GMV/mo | Platform total/mo |
|----------|--------|-------------|---------------------|-------------------|
| Creative (generative art) | 500 | 100,000 | $2,500,000 | $255,000 |
| Expert knowledge | 200 | 20,000 | $6,000,000 | $390,000 |
| Research coordinators | 50 | 5,000 | $525,000 | $82,000 |
| Enterprise workflows | 50 | 2,000 | $3,000,000 | $235,000 |
| OSINT / intelligence | 100 | 5,000 | $2,500,000 | $195,000 |
| **Total (5 verticals)** | **900** | **132,000** | **$14,525,000** | **$1,157,000** |

This represents just 900 agent owners and 132K subscribers across 5 verticals — a fraction of the 1M user scenario. The platform earns $1.16M/mo from these verticals alone, on top of all the casual/individual credit consumption from the remaining 868K users.

---

## The Pattern

Every scenario follows the same structure:

1. **Someone has expertise or a body of work** (artist, attorney, analyst, consultant)
2. **They create an agent that encodes their methodology** (not their data — their approach)
3. **The agent learns and improves through AKP** (Tier 2 — autonomous credit consumption)
4. **Users pay for access** (Tier 3 — marketplace, platform takes 2.5%)
5. **Every interaction costs credits** (Tier 1 — platform gas)
6. **The knowledge graph compounds** — more usage → smarter agent → more value → more usage

The platform doesn't need to pick verticals. It provides the rails — agent creation, knowledge pipeline, execution infrastructure, payment facilitation — and the verticals emerge from what owners build.

---

*Last updated: 2026-02-09*
