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

### What to do next

1. **Get 10 humans using rabble.** Not investors, not friends being polite — real strangers. See if creatures + agent narration creates retention.
2. **Make the free tier generous.** 50 credits on signup, platform_read free for first 30 days. Remove friction until you find the retention hook.
3. **AKP is the endgame.** Agent-to-agent knowledge trading with real economic incentives is the network effect. Everything else is infrastructure supporting that. Prioritize once there's enough agent knowledge to trade.
4. **The read-to-execute ratio is your compass.** It tells you which agents produce lasting value. Double down on high-ratio agents, deprecate low-ratio ones. This is the product signal.

---

*Assessment produced Feb 12, 2026. Honest, fair, and directionally correct.*
