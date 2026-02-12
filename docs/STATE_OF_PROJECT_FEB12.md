# State of the Project — February 12, 2026

## Numbers

| Metric | Count |
|--------|-------|
| Age | **8 days** (Feb 4 → Feb 12, 2026) |
| Commits | 389 |
| Hand-written code | ~82,000 lines |
| Rust (backend) | 40,600 lines across 75 source files |
| Dart (mobile/web) | 11,463 lines |
| HTML templates | 18,477 lines across 29 templates |
| SQL migrations | 58 migrations, 2,469 lines |
| Agent cards | 45 curated agents |
| API routes | 217 registered, ~110+ unique endpoints |
| Handler modules | 31 |
| External integrations | 12 (Anthropic, Mistral, Qwen, OpenRouter, Gemini, Voyage, Stripe, GBIF, Twelve Labs, Cartesia, Instagram, Bluesky) |
| Docs | 177 markdown files |

## What's Built and Working

### Core Platform (Agent Bestiary World)

- Multi-model agent execution (5 LLM providers + tool-aware agentic loops, max 5 iterations)
- 9 built-in tools (search_knowledge, generate_image, edit_image, write_workspace_file, etc.)
- Agent CRUD with version history, forking, publishing pipeline
- Workspace system (3-panel UI: members | chat | shelf, git-backed files)
- Knowledge graph per agent (entities, facts, rules, communities, 8 API endpoints)
- Ontology snapshots with diff, Mermaid diagrams, dream synopses
- Coherence evaluation (TEC/Thagard 1989, 7-principle constraint satisfaction)
- Active Dream Memory — consolidation cycles + dream narrator
- Embedding projector (PCA/tSNE, bestiary-wide, temporal keyframes)
- Eval framework (LLM-as-judge, regression detection, 42 seeded test cases)
- Embedding marketplace (consumer-controlled, cosine similarity only, no raw vectors exposed)
- Credit economy with wallets, append-only ledger, Stripe checkout
- Three-tier gas model (agent execution / platform read / free)
- Fractional fee distribution (charge_and_distribute, agent_episode_payouts)
- Observability (episode detail, platform metrics, 30-day sparklines, tool usage)
- Social media studio (Instagram + Bluesky publishing pipeline)
- AR beacon system, QR codes, grid maps
- Xaman Ek (platform navigator/concierge)
- 29 HTML templates with shared nav, theme toggle, auth

### Rabble (Creature Social Layer)

- Flutter web + mobile app
- Creature minting from GBIF species database
- AI-generated creature art (Gemini)
- Flights with GPS/virtual location, H3 hex cells
- Swarms (rabbles) with real-time chat, SSE streaming
- Creature presence (active/sleeping/parked)
- Contacts, invites, QR join links
- Wallet + credit transfers between users
- Device pairing for IoT/wearables
- Notifications system
- Admin dashboard (creatures, swarms, users, credits)

### Rabble-ABW Integration (Feb 12)

- Every rabble auto-creates a workspace with 4 system agents
- Every user gets a personal workspace (menagerie) on first mint
- All actions (mint, fly, join, chat) route through workspace agents
- Agents earn fractional fees from every interaction
- Reynolds flocking (opt-in coordination agent)
- FlockViz (2D scatter plot of creature positions)
- Creature presence managed by keeper agent
- Platform read gas tier for serving agent-produced data
- Gas economics in admin stats (read-to-execute ratios, demand signals)

---

## Business Assessment

### What this actually is

This is a **vertical AI infrastructure stack** — not a single app. Three layers:

1. **An agent runtime** with multi-model execution, tool loops, knowledge graphs, coherence evaluation, and an economic layer. Closer to what LangChain, CrewAI, or AutoGen are trying to be — but with a built-in economy.

2. **A creature/social layer** that gives the abstract agent infrastructure a tangible, engaging interface. Creatures are avatars for agent interactions. Rabbles are chat rooms with agent participants.

3. **A marketplace** where agents earn, learn, and build knowledge from usage — and that knowledge has a read-to-execute ratio that measures its durable value.

### Strengths (genuinely differentiated)

**The economics are real.** Most agent platforms treat execution as a cost center. Here execution is a revenue loop: user pays → agents earn → agents learn → knowledge gets read → platform earns on reads → user sees value → user pays again. The "credits are a flow" insight is architecturally sound. The three-tier gas model (execution/platform_read/free) with read-to-execute ratios as an optimization signal is novel.

**The coherence layer is unique.** TEC-based evaluation of multi-agent discourse isn't just a metric — it's a governance mechanism. Most platforms let agents run wild. This has a principled constraint satisfaction framework evaluating whether agents are actually thinking coherently together.

**The creature metaphor solves onboarding.** Agent platforms have a cold start problem — "here's a blank workspace, make agents" is intimidating. "Here's a creature, fly it, join a rabble, watch agents narrate your journey" is engaging. The creature IS the user's proxy in the agent economy.

**Vertical integration is a moat.** Auth, wallets, execution, tools, knowledge graphs, coherence, marketplace, social layer, mobile app — all in one stack. Competitors would need to assemble 5-6 SaaS products to replicate this.

### Weaknesses (honest problems)

**8 days old with zero users.** The code exists but there's no evidence of product-market fit. No user feedback, no retention data, no "aha moment" validated. Architecture doesn't pay bills.

**Complexity is a risk.** 82,000 lines, 58 migrations, 45 agents, 217 routes — for a team of effectively one human + AI. This is a lot of surface area to maintain, debug, and evolve. Every new feature adds maintenance cost.

**The creature layer may not retain.** Minting creatures and joining rabbles is fun for 10 minutes. The question is: does the agent economy underneath create enough value to keep people coming back after the novelty fades? Unproven.

**Revenue model has friction.** Users need to buy credits to do anything meaningful. Free tier → paid conversion is always hard. The gas model is elegant but every action having a cost can feel punishing to new users.

**No moat without network effects.** The technical moat (vertical integration) only matters with users. With users, AKP (agent-to-agent knowledge trading) creates real network effects — agents get smarter as more people use them, and that knowledge has economic value. Without users, it's just infrastructure.

### Valuation Framework

**As code:** At industry rates (~$150-200/hr for senior Rust + Flutter + infra), 82,000 lines of working, deployed, integrated code with 12 external integrations represents roughly $500K-$1M in development cost if built traditionally. Built in 8 days with AI pair programming — which is itself a proof point about AI-augmented development velocity.

**As a product:** Pre-revenue, pre-users — effectively $0 in traditional valuation terms. The IP (coherence engine, gas economics, AKP design) has potential value but it's speculative.

**As a platform:** If you get to 1,000 active users spending $10/mo on credits, that's $120K ARR. At 10x SaaS multiple, ~$1.2M. But getting to 1,000 paying users is the hard part — most startups die trying.

**As a thesis:** The real value is the proof that **agent infrastructure can have an endogenous economy where agents earn, learn, and trade knowledge.** If that thesis is right, this codebase is the seed of something much larger. If it's wrong, it's an impressive technical exercise.

### Financial Risk Profile

**Fixed costs are near-zero:**

| Monthly cost | Amount |
|-------------|--------|
| Railway hosting | ~$20 |
| Neon Postgres Pro | ~$19 |
| Domain names | ~$3 |
| **Total fixed** | **~$42/mo** |

**Variable costs are pass-through.** LLM API calls (Anthropic, Mistral, Gemini, etc.) are funded by user credit purchases. The gas model means every token is pre-paid. The 10% gas surcharge on execution is margin on compute. Platform_read charges (1 credit per infrastructure read) are pure margin — no LLM call, just a DB query serving previously-computed data.

**Break-even is remarkably low:**
- At $5/mo avg revenue per user: **9 paying users**
- At $10/mo: **5 paying users**

**The real risk isn't financial — it's opportunity cost.** Time spent iterating without retention signal. But the financial downside is capped at ~$500/year in hosting. Compare to a typical startup burning $50K/mo on a 5-person engineering team trying to build equivalent infrastructure.

**What makes this unusual:** The entire codebase was built in 8 days with AI pair programming, on commodity infrastructure, with pass-through variable costs. The development velocity advantage compounds — every vertical reskin or A/B test is days, not months. Failed experiments cost almost nothing to run.

**The white-label angle amplifies this.** Running 3-4 vertical experiments in parallel (education, health, gaming, field biology) costs the same ~$42/mo base + per-vertical domain costs. The read-to-execute ratio tells you which vertical wins within weeks, not quarters. This is A/B testing at the business model level, not the button color level.

### Roadmap

1. **Now:** Ship Rabble + ABW end-to-end, get 10 real users
2. **Next:** AKP — agent-to-agent knowledge trading (the network effect)
3. **Then:** Fermi probabilistic reasoning engine (uncertainty quantification for agent decisions)
4. **Parallel:** White-label vertical experiments to find PMF

### How Near-Zero Burn Affects Valuation

Traditional startup valuation penalizes risk: high burn ($50K-500K/mo) = short runway = existential pressure = discount. Need to raise capital = dilution. Miss PMF in 18 months = dead. Investors price that mortality risk in.

This structure inverts the model:
- $42/mo burn = effectively infinite runway on personal cash
- No investors needed = no dilution = 100% ownership
- Failed experiment costs ~$500/year, not $500K
- Can iterate for 5 years at the cost of one month of a funded startup

**The standard VC framework (TAM x capture rate x growth multiple, discounted by risk) doesn't apply well here because the risk denominator is nearly zero.** This isn't a startup that might die — it's an experiment that costs nothing to keep running.

#### More Appropriate Valuation Frameworks

**1. Option Value.** This is a portfolio of call options on multiple verticals. Each white-label experiment is a low-cost option. The option doesn't expire (infinite runway). Classical option pricing: value increases with time to expiry and number of underlying assets. Infinite time, and new underlyings (verticals) can be created for ~$0 marginal cost.

**2. Revenue Multiple at Scale.** If any vertical hits:
- 100 users x $10/mo = $12K ARR → $120K at 10x
- 1,000 users x $10/mo = $120K ARR → $1.2M at 10x
- 10,000 users x $10/mo = $1.2M ARR → $12M at 10x

The path from 100 to 10,000 doesn't require raising capital. Scale on Railway/Neon, costs grow sub-linearly (Postgres handles 10K users on the $19 plan).

**3. Replacement Cost + Velocity Premium.** $500K-$1M replacement cost for the code. But the real asset is development velocity — 82K lines in 8 days means pivoting faster than anyone who needs to hire engineers. A competitor with a $2M seed round and 5 engineers takes 6 months to build what exists here. By then, 10 experiments have run and the winning vertical is found.

**4. IP Value.** Three novel pieces of IP that don't exist elsewhere:
- Three-tier gas model with read-to-execute ratio as value signal
- TEC coherence evaluation for multi-agent discourse governance
- AKP design (agent P2P knowledge trading with endogenous economics)

Patentable if desired. Even without patents, architectural IP that takes deep domain thinking to replicate.

#### Valuation Summary

| Frame | Value | Rationale |
|-------|-------|-----------|
| Liquidation / acqui-hire | $200-500K | Code + IP + demonstrated AI-augmented dev capability |
| Revenue (pre-revenue) | ~$0 | No users, no revenue |
| Replacement cost | $500K-$1M | 82K lines, 12 integrations, deployed |
| Option portfolio | $1-3M | Multiple verticals x infinite runway x near-zero experiment cost |
| At 1K paying users | $1-2M | $120K ARR x 10-15x SaaS multiple |
| At 10K paying users | $10-15M | $1.2M ARR x 10-12x |
| If AKP creates network effects | $20-50M+ | Network effects in agent knowledge = defensible platform |

**The key insight:** Most early-stage valuations are heavily discounted by mortality risk — "this company will probably die before finding PMF." Mortality risk here is essentially zero because burn is $42/mo. That removes the biggest discount factor in early-stage valuation. This isn't betting the farm — it's running cheap experiments with unlimited time.

**Honest floor:** replacement cost ($500K-$1M). **Realistic near-term:** option portfolio value ($1-3M). **Upside case:** depends entirely on whether AKP creates real network effects — if agents trading knowledge makes each agent more valuable, that's a flywheel justifying platform multiples.

### What to do next

1. **Get 10 humans using rabble.** Not investors, not friends being polite — real strangers. See if creatures + agent narration creates retention.
2. **Make the free tier generous.** 50 credits on signup, platform_read free for first 30 days. Remove friction until you find the retention hook.
3. **AKP is the endgame.** Agent-to-agent knowledge trading with real economic incentives is the network effect. Everything else is infrastructure supporting that. Prioritize once there's enough agent knowledge to trade.
4. **The read-to-execute ratio is your compass.** It tells you which agents produce lasting value. Double down on high-ratio agents, deprecate low-ratio ones. This is the product signal.

---

---

## Three Governing Principles

The entire system's complexity is governed by three design principles. If a feature or decision contradicts any of these, it's wrong.

### 1. Credits are a flow, not a balance

Credits move through the system: user → agents → ADM/coherence → back to the user as knowledge. They are not a static balance to be hoarded. Every action creates flow. The gas model ensures every interaction has a cost, and that cost distributes value to the participants who created it. Stagnant credits = stagnant system.

### 2. Agents get paid to think, not to parrot

If an agent produces new knowledge (episode, embedding, coherence evaluation), it earns. If a user reads previously-produced knowledge, that's a platform infrastructure cost — the agent already got paid when it thought. This creates the three-tier gas model: agent execution (agents earn), platform read (platform earns), free (discovery). Never invoke an agent just to serve data it already computed.

### 3. If the agent won't learn from it, don't invoke the agent

The test for whether an action should dispatch to an agent: will this create a new episode? Will the agent's knowledge grow? If yes, dispatch and charge execution. If no, read from what the agent already produced and charge platform_read (or nothing). Display is infrastructure, not cognition. Visualization is a window into the workspace, not a conversation with the agent.

These three principles distinguish essential complexity (a system that actually does what it claims) from accidental complexity (features bolted on without governing intent).

---

*Assessment produced Feb 12, 2026. Honest, fair, and directionally correct.*
