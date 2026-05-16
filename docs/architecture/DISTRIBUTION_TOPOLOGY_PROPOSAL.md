# ABW Distribution Topology — Design Proposal

**Status:** Draft proposal — not yet implemented
**Date:** 2026-05-16
**Author:** Ivan Labra (drafted with Kilo)

**Related (internal):**
- `docs/AGENT_MODEL.md` — agent card shape, model ladder, capability gates
- `docs/COMPOSITION_AS_FIRST_CLASS.md` — composition as a peer primitive
- `docs/architecture/FEEDBACK_LOOPS.md` — the five feedback loops execution feeds
- `docs/architecture/OBSERVABILITY_ARCHITECTURE_SPEC.md` — how execution becomes signal
- `docs/architecture/AKP_DESIGN.md` §4.1 — economic-topology stance (see §11 reconciliation)
- `docs/specs/01_APP_PRIMITIVE.md`, `docs/specs/03_BUILDING_NEW_APPS.md` — App primitive and external-developer recipe
- ADR-011 (in-prompt, xaman_ek) — creature cognition economy
- `src/agent_backend/multi_model_executor.rs`, `tool_executor.rs` — current provider dispatch

---

## 0. Background

This proposal sits at the intersection of several lines of work — some from the platform's own design history, some from a longer tradition of distributed-systems and generative-platforms thinking. Both are listed here so readers can ground the rest of the document in a shared lineage rather than re-derive the framing from first principles. The treatment is deliberately brief; the point is to name what's being built on, not to summarise it.

### 0.1 The platform's design line

ABW was conceived as a **distributed complex adaptive system** (CAS) in the Holland / Santa Fe sense — heterogeneous agents with local rules interacting through a shared substrate, producing emergent collective behaviour. The current implementation runs as a centralised monolith. That is a deliberate complexity-management choice, not the design point. The architectural bet is that the platform's distinctive contribution — a five-loop recursive self-improvement (RSI) machinery (§1.2) — is hard enough to engineer correctly *on a single substrate* that introducing distribution simultaneously would compound two open problems and yield neither. Once the RSI machinery is stable, the topology can relax.

This proposal is the first explicit articulation of "when does the topology relax, and toward what."

### 0.2 The external lineage this proposal builds on

The intellectual lineage of this work is broader than the standard distributed-systems canon. Five threads matter for ABW specifically:

**Generativity as a first-class property.** Zittrain (*The Future of the Internet — And How to Stop It*, 2008) names *generativity* — a system's capacity to produce unanticipated change through unfiltered contributions from broad audiences — as the property worth preserving in networked platforms, and argues it is structurally in tension with the security and control properties of "appliance-ized" systems. ABW's existing design (agent cards anyone can author, compositions as recipes, fork as first-class, dreaming as user-controlled evolution) is already a bet on generativity. The topology question is whether the *substrate* matches the platform's generative ethos.

**Network-as-computer.** Sun's mid-1990s work — Jini, JavaSpaces, RMI, mobile code — was the first commercial attempt to build distributed-CAS-shaped infrastructure as a substrate, including capability-typed nodes, mobile agents, leased resources, and tuple-space-style discovery. The lessons from why it didn't fully land at the time (network performance, identity, trust models) are directly relevant to the question of *when* and *how* ABW's topology should relax.

**Government Web 2.0 and MMOG distributed work.** The post-2003 US transformational C2 work attempted to apply Web 2.0 architectures — RSS aggregation, wiki-style collaboration, mash-ups, identity federation — to partial-trust, high-stakes environments. The structurally parallel lineage is MMOG architecture (Habitat — Morningstar & Farmer 1985; Ultima Online; EVE Online), which solved persistent state across thousands of partial-trust clients with continuous evolution and emergent governance years before enterprise distributed systems caught up. ABW is structurally closer to an MMOG than to a SaaS app: compositions are guilds, agent cards are character sheets, the bestiary is the game world, dreaming is the levelling system, and rabble swarms are large-scale persistent-state coordination problems.

**Generative networks and capability-building.** The generative-networks research tradition (Drucker's late work on knowledge economies; NPS work on networks-as-capability-producers; Sen and Nussbaum on human capabilities applied to technology platforms) frames networks not as transport but as media that *build the capability of participants to do new things*. This grounds the AKP §4.1 reconciliation more sharply than scale-free vs not-scale-free does: the question is whether the substrate produces capability or extracts rent.

**Ultra-Large-Scale systems and self-*.** Northrop et al., *Ultra-Large-Scale Systems: The Software Challenge of the Future* (CMU/SEI 2006), describes what large software-intensive systems become when they cross certain thresholds (decentralised control, normal failures, eroded people/system boundary, continuous evolution). IBM's autonomic-computing programme (2001–2006) defined the four self-* properties (self-configuring, self-healing, self-optimising, self-protecting) such systems need to exhibit. ULS is the *prediction* of what a successful ABW eventually looks like; the self-* properties are the architectural targets.

**Practitioner ground truth.** The author has shipped distributed open-source platforms at scale before, including the first 100%-open-source end-to-end certified SaaS for a US government deployment (mid-2000s, built on Drupal with RSS/XMPP bridges as the capability-typed routing layer). The topology decisions in this proposal are informed by what those deployments actually broke on — identity, trust, governance, evolution under load — rather than by theory alone.

The synthesis: **ABW is a generative system (Zittrain) built as a distributed CAS (Holland), currently running as a deliberately simplified centralised substrate to protect the engineering of an RSI primitive (§1.2), with an eventual topology in the lineage of network-as-computer (Sun) / MMOG distributed state (Habitat → EVE) / generative-network capability-building (Drucker, NPS) / ULS architectural targets (CMU/SEI). The simplification is in the lineage of Gabriel's "worse is better" — temporary loss of architectural purity to make the actual hard thing tractable.**

That paragraph is the document's spine. The rest follows from it.

### 0.3 Cited prior art

- Northrop et al., *Ultra-Large-Scale Systems: The Software Challenge of the Future*, CMU/SEI 2006
- IBM, *An Architectural Blueprint for Autonomic Computing*, 2001/2006
- Zittrain, *The Future of the Internet — And How to Stop It*, Yale 2008
- Gabriel, *Patterns of Software*, OUP 1996; *Worse Is Better* essay, 1991
- Holland, *Hidden Order: How Adaptation Builds Complexity*, 1995 (CAS framing)
- Drucker, *Post-Capitalist Society*, 1993; *Management Challenges for the 21st Century*, 1999 (knowledge-economy framing)
- Sen, *Development as Freedom*, 1999; Nussbaum, *Creating Capabilities*, 2011 (capability approach)
- Morningstar & Farmer, *The Lessons of Lucasfilm's Habitat*, 1991 (MMOG distributed-state lineage)
- Barabási & Albert, *Emergence of Scaling in Random Networks*, 1999 (scale-free networks)
- Ostrom, *Governing the Commons*, 1990; Hess & Ostrom, *Understanding Knowledge as a Commons*, 2007
- Frischmann, Madison, Strandburg, *Governing Knowledge Commons*, 2014 (GKC framework)
- Beckstrom, *A New Model for Network Valuation*, 2009 (network value math)
- libp2p / IPFS / Kademlia / Pastry — capability-aware overlay networks in production
- Bluesky AT Protocol — portable identity + federation in a contemporary deployment
- Holochain, *Agent-Centric Distributed Computing* — agent-centric alternative to blockchain global consensus
- Miller & Hardy, object-capability literature

---

## 1. Problem statement

### 1.1 What is being protected

ABW today is a single-host, server-centric system: one Postgres, one Rust binary on Railway, cloud LLM providers as the only execution targets, all workspace and platform state in one database. This is the simplest viable topology and it was a deliberate choice — not because distribution was uninteresting, but because the platform's distinctive engineering surface lives elsewhere, and adding distributed-systems complexity on top of it would compound two open problems.

The distinctive engineering surface is the platform's **recursive self-improvement primitive** — five gated, observable feedback loops that let agents, compositions, and the platform's routing improve themselves over time. This is treated in §1.2. The simplification-of-topology choice is in the service of getting the RSI primitive right.

### 1.2 The RSI primitive (the thing the topology is currently protecting)

ABW implements recursive self-improvement at three nested layers of the stack. The loops are documented in detail in `docs/architecture/FEEDBACK_LOOPS.md`; this section summarises them as a single primitive so the topology trade-off becomes legible.

**Layer 1 — Individual agent learning** (Loop 1)
Eval dimension scores written per execution drive consolidation jobs (Active Dreaming Memory). Consolidation produces semantic rules, knowledge-graph mutations, and persona-baseline shifts that change how the agent reasons on its next invocation. *The agent's ontology is a function of its own execution history.*

**Layer 2 — Human-gated correction** (Loop 2)
Anomaly events (drift, conflict, rupture, safety) queue for human review. Reviewer actions become synthetic episodes at human-authority weight, bumping persona_version and propagating the correction into the agent's ontology. *Humans are inside the loop, not external to it; their judgement is consumed as high-weight training signal.*

**Layer 3a — Composition coherence** (Loop 3, inner)
TEC coherence evaluation runs every N messages; produces a coordination brief that members consume on the next turn. *The team's discourse coherence is monitored and corrected in-flight.*

**Layer 3b — Composition evolution** (Loop 3 outer + Loop 4)
Strategist Dreaming proposes composition_versions: member roster, valence diversity, strategist substitution. Owner-gated acceptance lets the team's *shape* mutate based on accumulated session signal. *The team is not a static recipe; it learns its own optimal composition.*

**Layer 3c — Routing calibration** (Loop 5)
Domain-constrained MoE strategists (e.g., `fermi` for forecasting, `simops` for process optimisation) accumulate Brier scores or analogous signals on resolved outputs. Routing weights self-correct against ground truth. *The platform's "which expert handles this?" judgement is itself an improvable parameter.*

Read together: **agents improve themselves, compositions improve their teams, and the platform's routing improves its routing.** That is RSI at three nested layers — individual, collective, meta — made legible by explicit feedback signals (eval_signals, anomaly_events, coherence_evaluations, composition_versions, calibration scores) and gated by explicit budgets and thresholds (dreaming_budget_credits, HITL gates, persona drift thresholds, calibration n_resolved minimums).

This is not how other multi-agent platforms work. The platforms in the public space treat agents as static prompt-engineered configurations, compositions as fixed pipelines, and routing as either hardcoded or driven by black-box vector similarity. ABW's RSI primitive — explicit, gated, observable, and operating at three nested layers — is a research-grade architectural commitment, not a feature.

Two nested sub-primitives are in service of this:

- **Domain-constrained MoE** (`docs/COMPOSITION_AS_FIRST_CLASS.md`; `output_contract` on the orchestrator agent card) is the carrier for Loop 5. The {domain, produces, schema, calibration, synthesis} contract is what makes routing *improvable*: it gives Loop 5 a typed output to score, a calibration signal to score against, and a synthesis protocol to combine member outputs deterministically.

- **Rule-based cognition reconfiguration** (`apply_tier_resolution()` in `src/agent_backend/agent_card.rs`; capability_gates; model_ladder) is the substrate-flexibility primitive that lets RSI run at varying compute budgets. The same agent card serves a free-tier user, a standard-tier user, and a premium-tier user with different (provider, model) bindings, the same persona, and the same eval criteria. Consolidation runs at one tier; production execution at another; HITL replay at a third. *RSI machinery does not assume uniform compute.*

These three primitives — RSI as the five gated loops, domain-constrained MoE as Loop 5's carrier, cognition reconfiguration as the substrate-flexibility primitive — are why the topology has been deliberately kept simple. Getting them right is the platform's research contribution. Doing them under partial-replica conditions, where state replicas can disagree about an agent's persona version, where consolidation can run against partial episode histories, where coherence evaluations can fork between replicas, is *how you destroy the RSI signal*. The current centralised topology is scaffolding around the real work.

### 1.3 Why the topology question now

The simplification is not free. As ABW matures and verticals push against the substrate, costs accumulate that the centralised topology is not equipped to bear:

| Pressure | Surface friction | Implication for topology |
|---|---|---|
| **Closed-data domains** (clinical, defence, regulated bio surfaces in SimOps and adjacent verticals) | Workspace context cannot leave premises | Execution must be locatable to the user's premises, with cloud as optional |
| **Cost-sensitive operators** (researchers, hobbyists, edge appliances; Ambu bioreactor; future Rabble field deployments) | Per-token billing every time an 8B model could have answered | Compute should be re-locatable to where it's cheapest, not assumed central |
| **Sovereignty / resilience** | Single-vendor cloud LLM outage = composition offline; pricing change = unit economics break | The substrate should degrade gracefully across provider failure, not stop |
| **Edge devices** (Ambu bioreactor on factory floor, future IoT integrations) | Every cascade calculation round-trips Railway → Anthropic → Railway → device | Execution should sit close to the sensors, with cloud as supervisor |
| **User-level sovereignty + privacy** | Even non-regulated users would prefer their episodes not live exclusively in the operator's database | Per-user replicas, opt-in cloud presence |
| **Scale** (eventual) | Centralised Postgres + single Rust process is a known scaling ceiling | Capability-aware distribution lets hot capabilities scale independently |

These pressures aren't six separate features. They are six manifestations of the same underlying gap: **the platform has deliberately deferred the topology question, and the verticals being built on ABW are reaching the point where deferring further costs more than answering does**. The RSI primitive (§1.2) is now stable enough to *survive* topology relaxation if done carefully — the question is which steps to take, in which order, with which reversibility guarantees.

This proposal asks the topology question explicitly. The first concrete answer — "support Ollama as a provider" — is one specialisation of a more general design space. The interesting work is everywhere else.

### 1.4 What "supporting local models" actually is

A request like "let me run an agent against my Ollama instance" looks like a feature ask. It is actually a request for the platform to have *any* coherent answer to the question:

> Where does execution happen, who controls the data it reads, what gets replicated, and how does the platform reason about a substrate it doesn't fully own — while preserving RSI signal integrity?

Today's answer is "execution happens on Railway, against cloud LLM APIs, owned and read by ABW; replication is trivial because there's only one replica; RSI signal integrity is automatic because there's only one source of truth." That's a single point on the topology design space, and it is *only* coherent because the platform is centralised.

The Ollama question pushes us to define more points on that space. The technical lift for the immediate step is small (~30 lines of provider plumbing). The interesting work is everywhere else: cognition tiers, gas economics, observability, capability gates, identity, eventual replication and discovery, **and the preservation of RSI signal integrity through any topology evolution**. All of these are decisions about the topology, not about Ollama specifically.

---

## 2. Goals and non-goals

### Goals (this proposal)

1. **Preserve RSI signal integrity through any topology change** (§1.2). This is the hard constraint that disqualifies many naive distribution choices. Episodes must remain linearly orderable per agent; persona_version transitions must be globally consistent; coherence evaluations must reference a single canonical message log per workspace; calibration scores must accumulate without double-counting. Any topology relaxation that breaks these invariants is rejected, not modified.
2. **A coherent topology framing for ABW** — name the deployment modes (operator-hosted / BYO endpoint / runner-relay / state-peer), the addressing primitives (provider + model + capability gates), and the long-arc end-state (§10) so future design conversations have shared vocabulary
3. **A local-first agent class** — agents that run entirely without cloud inference, with the same coherence engine, the same five loops, the same card shape
4. **Honest economics** — local execution carries platform gas (the cost of running the orchestrator) but no per-token charge
5. **Honest observability** — episodes, eval signals, anomalies all tag the provider and node used, so reviewers can distinguish "drift on local" from "drift on cloud" from "drift on node-X"
6. **Capability honesty** — agents declare what they require; the platform refuses graceful degradation when a frontier-only agent is asked to run on an 8B local model
7. **Catalogue legibility** — users can find local-first agents at a glance; operators can configure composition-wide topology policies (local-only, prefer-local, etc.)
8. **Keep the long-arc reachable** — every decision in Phases 0–5 should leave the door open to the §10 end-state without committing the platform to it prematurely

### Non-goals (this proposal)

- Bundling Ollama itself into the ABW container — operators bring their own endpoint
- Embedding generation on local models — keep using cloud embedders (Voyage, OpenAI) for now; embedding consistency is a separate, harder problem
- Memory consolidation (Loop 1 dreaming) on local models — initial scope is execution only; consolidation continues to use frontier cloud models for quality reasons
- Cross-tenant local serving — out of scope; if you self-host, you self-host
- Fine-tuning workflows — local models are used as-is
- Committing to the §10 end-state architecture (scale-free overlay with super-peers, capability-routed, self-configuring) — that is the long-arc *aspiration*, not the scope of this proposal. §10 documents it so we can make Phases 0–5 choices that don't foreclose it.
- Replacing the AKP economic topology stance (`AKP_DESIGN.md` §4.1) — see §11 reconciliation

---

## 3. The five real design decisions

Adding a provider is not the work. The work is fitting it into the five subsystems the platform already runs against execution.

### Decision A — Trust posture: who runs the local model, and what role does their machine play?

Four deployment modes; each implies a different platform contract and a different role for the user's machine in the architecture.

| Mode | Where Ollama runs | User-machine role | Reachability problem | ABW guarantees |
|---|---|---|---|---|
| **A1. Operator-hosted** | On the ABW host (Railway, Docker, on-prem) | None — same as cloud | None | Full observability, billing, eval — same as cloud providers |
| **A2. BYO endpoint** | User-controlled, on a stable public URL (homelab w/ reverse proxy, Cloudflare Tunnel, etc.) | Public server | User solves it | Treated like a webhook destination; best-effort |
| **A3. Runner-relay (compute-peer)** | On the user's machine; the user runs a small ABW runner binary that opens a WebSocket *to* ABW and pulls work down the pipe | Compute peer / worker node | Solved by the runner dialing home | Same as A1 once the runner is connected; queue-buffered if it disconnects |
| **A4. Local replica (state-peer)** | On the user's machine, **alongside a local replica of the workspace itself** — episodes, KG, chat, coherence evals all live on the user's disk and sync via CRDT when reconnected | State peer / replica node | Solved by sync, not reachability | Eventual consistency; some platform features (wallet, marketplace) remain server-authoritative |

This proposal covers A1 and A2 in detail (Phases 0–4). A3 is sketched and proposed as a deferred Phase 5. **A4 is treated separately in §10.4 as the T4 waypoint on the long-arc topology path** — it is not a feature, it is a different platform shape, and it deserves an explicit business-fit conversation (§10.4.6) before any engineering commitment.

#### Why A1/A2 first

- They require no new platform primitives — just a provider registration in the existing executor
- They satisfy the immediate use case (closed-data ops who can stand up their own Ollama host)
- They make the trust posture explicit without committing the platform to peer-to-peer state semantics
- They earn the right to A3 / A4 by exposing real demand

#### A1 / A2 configuration

The configuration knob is a single env var per endpoint:

```bash
OLLAMA_BASE_URL=http://ollama:11434/v1                 # A1: operator-hosted, internal DNS
# or
OLLAMA_BASE_URL=https://ollama.user-domain.com/v1      # A2: BYO endpoint, user owns the URL
```

No API key — Ollama doesn't require one. Authentication, if needed for A2, is handled by network policy (VPN, mTLS, IP allowlist) — outside ABW's scope.

#### A3 sketch (deferred — but worth recording)

A3 (runner-relay) is the natural extension of A2 once "I have a laptop, not a homelab" is the dominant ask. It's the model GitHub Actions self-hosted runners and Tailscale exit-node-relays use: the runner is a small client that dials *out* to the orchestrator over WSS and pulls work from a queue. NAT is irrelevant because the runner is always the initiator.

```
┌────────┐   wss (long-lived)    ┌──────────┐  http  ┌────────────────┐
│  ABW   │ ◄──────────────────── │  Runner  │ ──────►│ localhost:11434│
│ (cloud)│   job push / response │ (laptop) │        │  Ollama        │
└────────┘                       └──────────┘        └────────────────┘
```

Implementation surface is bounded:
- One new binary, `abw-runner` (~500–1000 LOC Rust): WSS client, job receive, HTTP forward to local Ollama, response stream-back
- One new endpoint on ABW: `wss://api.../runners` with auth (per-user runner token)
- One new table: `local_runners(runner_id, user_id, last_seen_at, capabilities_json)` — lists pulled models, GPU info, etc.
- Routing change in the executor: when the resolved provider is `ollama_runner`, look up an online runner for the user, push the job, await the response on the socket. Time out gracefully if no runner is online.
- The runner does not run any agent logic. It is a dumb inference target. **Workspace state stays on ABW.**

A3 doesn't change ABW's data model. It changes *one* line in the executor dispatch table and adds a queue. The user's laptop is a worker, not a peer. This is achievable in ~2–3 weeks and proposed as **Phase 5** of this plan (deferred until A1/A2 demand justifies it).

#### A4 sketch (covered fully in §10.4)

A4 is the "user laptop as state-peer" architecture: not "how do I reach your laptop's Ollama?" but "how does my whole workspace live on my laptop and sync with the cloud?" That's a different question — not a deployment mode for Ollama, but a different shape for ABW itself. In the long-arc framing (§10.1), A4 is stage T4 on the path to T5. **See §10.4 for the schema audit, cost analysis, and decision criteria.**

### Decision B — Where does Ollama sit in the cognition tier ladder?

ADR-011 maps `free | standard | premium` → specific `(provider, model)` rungs in each agent's `model_ladder`. Local models don't fit cleanly because they break a load-bearing assumption of the ladder: **monotone capability**. Free is worse than standard is worse than premium. `llama3.1:8b` is not strictly worse than `claude-haiku-4-5` — it's different.

Three coherent options:

#### Option 1 — Ollama becomes the new `free` floor

```json
"model_ladder": [
  { "tier": "free",     "provider": "ollama",    "model": "llama3.1:8b" },
  { "tier": "standard", "provider": "anthropic", "model": "claude-haiku-4-5" },
  { "tier": "premium",  "provider": "anthropic", "model": "claude-sonnet-4-6" }
]
```

**Pro:** zero ongoing per-token cost for free-tier users; predictable availability vs `openrouter/free`.
**Con:** quality regression vs current `openrouter/free` (which often gets a Llama-70B variant). Free users would notice. Drift baselines would shift.
**Verdict:** rejected for default behaviour. Could be opt-in per agent.

#### Option 2 — Ollama is a parallel tier, not a rung

Add a new tier `local` alongside `free | standard | premium`. Agents that explicitly support local declare a `local` rung. Users (or creatures) with `cognition_tier = "local"` route there.

```json
"model_ladder": [
  { "tier": "local",    "provider": "ollama",    "model": "qwen2.5:7b" },
  { "tier": "free",     "provider": "openrouter","model": "openrouter/free" },
  { "tier": "standard", "provider": "anthropic", "model": "claude-haiku-4-5" }
]
```

**Pro:** preserves the monotone-capability invariant for `free|standard|premium`; gives users explicit control.
**Con:** requires migration (`creature_conditions.cognition_tier` enum), tier-resolution logic, UI for tier selection, and forcing every agent to choose whether it has a `local` rung.
**Verdict:** the right long-term answer once the user signal justifies the work.

#### Option 3 — Ollama is per-agent opt-in, not tier-level

Don't touch the ladder for cloud agents at all. Let community agents declare `"provider": "ollama"` directly as their primary provider when their author wants local-first behaviour. The `cognition_tier` resolution still runs, but if the resolved ladder rung points to Ollama, you get Ollama.

**Pro:** zero-change for existing agents; opt-in surface for the small set of agents that care; doesn't risk free-tier quality regression.
**Con:** doesn't help operators who want to enforce "this entire workspace runs local-only" — that has to be a separate workspace-level policy.
**Verdict:** **recommended starting point**. Ships in a week. Earns the right to Option 2 once we see who actually uses it.

### Decision C — Gas / billing model

Today every agent execution costs credits via `calculate_cost(model, tokens)`. Three coherent stances for local:

| Stance | What it says to users | What it does to the loops |
|---|---|---|
| **Free at point of use** | "Local is fully free" — no gas, no per-token | Loop 1 still works (dreaming budget unaffected), but agents using only local never accrue earnings → marketplace signal goes dead |
| **Gas-only** | "Local pays platform gas (1cr message, 5cr hire) but no per-token execution charge" | Loops 1 + 4 stay funded; per-execution royalty path becomes 0 for local agents |
| **Synthetic price** | "Charge a notional standard-tier rate even though the inference was free" | Maintains marketplace economics at the cost of confusing the user ("I ran it locally, why was I charged?") |

**Recommendation:** **Gas-only**.

- Preserves "agents earn to think" — Loop 1 funding (`dreaming_budget_credits`) is independent of execution cost, but the *intent* of execution gas is that running orchestration infrastructure has a non-zero cost. Local doesn't change that.
- Hiring and adding agents to a workspace still costs gas — fair, because the workspace machinery still costs ABW to provision.
- Per-token execution royalty becomes 0 for the local provider — this is the honest number.
- Implementation: 5-line patch in `src/agent_backend/registry.rs:279` and `calculate_cost()` to short-circuit `provider == "ollama"` → returns 0.
- Marketplace consequence: agents that *only* run local will have zero `total_cost_usd` and zero accrued royalties. The catalogue UI needs to surface "Runs locally — no per-execution charge" so this isn't read as "this agent has never run."

### Decision D — Observability and eval signals

The five feedback loops assume execution generates meaningful signal. Local models will break some assumptions if we don't annotate them:

| Loop | Risk if we don't annotate provider | Mitigation |
|---|---|---|
| **1 — Learning** | Eval scores from 8B local outputs drag down avg metrics; observability flags "needs attention" even when the agent works fine *for local* | Tag every `episode`, `eval_signal`, `anomaly_event` with the provider+model actually used at execution time |
| **2 — Correction** | HITL reviewer sees an anomaly without knowing it was on local — may recommend wrong fix (intervene persona vs upgrade model) | Surface `provider_used` in the HITL review UI |
| **3 — Coherence** | Workspace coherence score drops; cause is "we put local agents in a frontier-reasoning composition" but the platform reads it as discourse failure | `coherence_evaluations` get a `provider_mix` summary so the consultant agent can explain the cause |
| **4 — Evolution** | Composition Dreaming proposes changes based on metric trends that confound provider-quality with composition-quality | Strategist memory should include provider info when reasoning about a member's contribution |
| **5 — Calibration** | Brier scores mixing cloud-frontier and local outputs are not directly comparable — calibration weight learning gets confused | Per-provider calibration tracking; require sufficient `n_resolved` per (agent, provider) pair before using calibration as a routing signal |

**Implementation surface** (small but cuts across several places):

```sql
-- Migration: add provider tracking to the hot path
ALTER TABLE episodes        ADD COLUMN provider_used TEXT;
ALTER TABLE episodes        ADD COLUMN model_used TEXT;
ALTER TABLE eval_signals    ADD COLUMN provider_used TEXT;
ALTER TABLE anomaly_events  ADD COLUMN provider_used TEXT;
ALTER TABLE coherence_evaluations ADD COLUMN provider_mix JSONB;

CREATE INDEX idx_episodes_provider ON episodes(provider_used);
CREATE INDEX idx_eval_signals_provider ON eval_signals(provider_used);
```

Populated by the executor (already has the resolved provider+model in scope after tier resolution). The observatory UI gains a provider filter on every per-agent panel.

### Decision E — Capability gates: what can local actually do?

Some agents need frontier reasoning. `xaman_ek` in composition_design mode, `comparator` writing 4-paragraph narratives, `fermi` decomposing forecasts into multipliers, `cohere_and_coordinate` running TEC + valence analysis — these will produce bad outputs on `llama3.1:8b` that look like agent regressions.

The card already has `capability_gates` for "this capability requires premium tier." Extend the pattern:

```json
"capability_gates": {
  "deep_reasoning": "premium",
  "min_provider_class": "cloud_frontier"
}
```

Where `min_provider_class` takes one of:

- `local` — happy on local models; any provider works
- `cloud_standard` — needs a cloud model but doesn't need frontier (most domain experts)
- `cloud_frontier` — needs a frontier-tier model; refuses local + refuses cheap cloud (strategists, narrators, decomposers)

The platform refuses execution with a clear error: `"Agent <id> requires min_provider_class=cloud_frontier; current provider is ollama. Either upgrade the workspace cognition tier or pick a different agent for this role."`

This is the **trust signal that makes local viable** — without it, users will hire frontier-grade agents, route them to local, get garbage output, and conclude the platform is broken. With it, the platform is honest about what runs where.

---

## 4. The local-first agent class

Pulling these decisions together: ABW should explicitly recognise a **local-first agent class** as a tagged subset of the bestiary.

### Definition

A local-first agent is one whose `model_ladder` includes at least one rung with `provider: "ollama"` (or another local provider), and whose `capability_gates.min_provider_class` is `local` or unset.

### Catalogue surface

```
┌──────────────────────────────────────────┐
│ ⚙ simops_narrator_local      [LOCAL]     │
│   Pipeline interpreter — runs entirely   │
│   on local Ollama models. No per-token   │
│   charge.                                │
│   Tags: simops, narrator, local-first    │
│   Hire: 5cr · Execute: gas only          │
└──────────────────────────────────────────┘
```

A new `[LOCAL]` badge, a new tag `local-first`, and a new filter chip in the catalogue (`Research | Creative | ... | Local-First`).

### Workspace policy: "local only"

Add a workspace setting:

```json
"workspace_policy": {
  "local_only": true,
  "allowed_providers": ["ollama"]
}
```

When set, hiring an agent with `min_provider_class: cloud_frontier` is refused with a clear message. Lets operators in regulated domains lock down an entire composition.

### Composition recipes

The xaman_ek prompt gains new patterns:

- **Local SimOps Pipeline** — `simops_advisor_local` + `simops_cascade` (deterministic, no LLM) + `simops_narrator_local` — entire process modelling without cloud round-trips
- **Air-gapped research** — local agents + local embedders (when we ship those) — for sites that genuinely can't reach the internet

---

## 5. Embedding consistency — the elephant

ABW's memory layer is built around vector embeddings (Voyage `voyage-2`, 1024-dim). Episodes get embedded; KG entities get embedded; semantic search rides on cosine similarity in the same space.

A local model that generates text still needs an embedder for memory writeback. Three approaches:

1. **Keep cloud embedders even for local-first agents** — pragmatic. The user's text still goes to Voyage briefly for embedding, but never to a generation model. Smaller leak surface than full cloud generation, but not zero.
2. **Local embeddings (e.g., `nomic-embed-text` via Ollama)** — true local-first. Requires either:
   - Splitting the embedding space (cloud agents have one KG, local agents have another) — fractures the platform
   - A cross-space mapping (cosine alignment between Voyage and local embedders) — research-grade work
3. **Defer embeddings entirely for local agents** — local agents don't write to KG; their episodes are stored but not searchable until cloud embedding catches up at a non-sensitive moment.

**Recommendation:** **start with Approach 1** (cloud embedders, local generators). State it explicitly in the local-first agent definition: "this agent's generation is local; its memory writeback uses platform-default cloud embeddings." Tackle Approach 2 as a separate effort with its own design doc once the local-first agent class has real usage.

This is the honest tradeoff to surface to operators in regulated domains. They may decide it's acceptable (the embedding API call carries no system prompt or PII-laden response) or they may need to deploy a local embedder before adopting — that's their call.

---

## 6. Phasing — what to build, in what order

### Phase 0 — Provider plumbing (1 day)

**Scope:** Add Ollama as a routable provider. No economic change. No tier change. No observability change.

- Add `ollama` to `MultiModelExecutor::from_env()` (~10 lines)
- Add `ollama` to `resolve_openai_provider()` in `tool_executor.rs` (~3 lines)
- New env var `OLLAMA_BASE_URL` documented in `.env.example` and `DEPLOYMENT_GUIDE.md`
- One community agent card with `"provider": "ollama"` as proof-of-life — e.g. a fork of `simops_narrator` named `simops_narrator_local`
- README note: "Ollama support is experimental; no platform billing or observability integration yet"

**Exit criteria:** an operator with Ollama running locally can hire `simops_narrator_local` and execute it. Charges flow as if it were OpenRouter free-tier. No surprises in dashboards.

### Phase 1 — Honest economics (2 days)

**Scope:** Decision C (gas-only billing for local).

- `calculate_cost(model)` short-circuits to 0 when the resolved provider is `ollama`
- Catalogue surface: "Runs locally — no per-execution charge" badge for any agent whose ladder includes an Ollama rung
- Document the policy in `docs/WALLET_SYSTEM.md`

**Exit criteria:** local-first agents accrue no token costs; gas still flows; marketplace UI surfaces the "local" badge so zero-cost is read as "by design" not "broken."

### Phase 2 — Observability annotation (3 days)

**Scope:** Decision D — tag execution provenance everywhere it matters.

- Migration: `episodes`, `eval_signals`, `anomaly_events`, `coherence_evaluations` gain provider/model columns
- Executor populates them at execution time
- Observatory UI: provider filter on per-agent panels; provider chip on HITL anomaly cards
- Calibration tracking splits per-provider; require min `n_resolved` per (agent, provider) before exposing as a routing signal

**Exit criteria:** an operator can answer "show me drift events on local for the last week" and get a meaningful filtered view.

### Phase 3 — Capability gates and trust signals (2 days)

**Scope:** Decision E — agents declare what they need.

- Extend `AgentCapabilities` with `min_provider_class: local | cloud_standard | cloud_frontier`
- Tier-resolution logic refuses to resolve to a provider class below the agent's minimum
- Clear error messages ("requires cloud_frontier; current is ollama")
- Workspace policy `local_only` (Decision A epilogue): refuses to hire agents with `min_provider_class > local`
- Backfill: existing frontier-grade agents (`xaman_ek`, `comparator`, `fermi`, `cohere_and_coordinate`) declare `min_provider_class: cloud_frontier`. Domain experts mostly stay `cloud_standard`. Pure-deterministic agents (`simops_cascade`, `coherence_evaluator`) become `local`.

**Exit criteria:** the platform refuses to silently run a frontier agent on local. Users get a usable error message and a clear upgrade path.

### Phase 4 — Local-first agent class (2 days)

**Scope:** Make local-first a recognised first-class concept.

- Tag `local-first` on the agent_card spec; catalogue filter chip
- `[LOCAL]` badge in agent cards (catalogue and detail view)
- New xaman_ek composition patterns: Local SimOps Pipeline, Air-gapped Research
- Documentation pass: `docs/LOCAL_MODELS.md` covering operator setup, BYO endpoint config, capability-gate semantics, and the embedding caveat

**Exit criteria:** a user browsing the catalogue can filter to local-first agents, understand the tradeoff (no per-token cost; quality ceiling lower; embedding still cloud), and hire confidently.

### Phase 5 — Runner-relay (deferred — A3 from §3)

**Scope:** Let users run Ollama on their own laptop without a public endpoint, via a runner binary that dials home over WSS.

- Build `abw-runner` binary (~500–1000 LOC Rust): WSS client, job receive, HTTP forward to local Ollama, response stream-back
- New endpoint `wss://api.../runners` with per-user runner token auth
- New table `local_runners(runner_id, user_id, last_seen_at, capabilities_json)`
- New provider `ollama_runner` in executor dispatch
- Per-user runner registry in the dashboard ("My local runners: 1 online, model `qwen2.5:7b`")
- Job queue with timeout/retry semantics when no runner online

**Exit criteria:** a user with a laptop behind NAT can install `abw-runner`, hire a local-first agent, and execute it against their laptop's Ollama from the cloud ABW UI.

**Why deferred:** A3 is mechanically straightforward but operationally heavier than A1/A2 (queue, runner lifecycle, "runner just went offline mid-execution" handling). Wait for real demand from A1/A2 users before committing.

### Phase 6 — Tier as parallel (deferred — let usage signal demand)

**Scope:** Decision B Option 2 — add `local` as a parallel cognition tier.

Deferred until real usage of Phases 0–5 reveals whether operators want tier-level enforcement or per-agent opt-in is sufficient. If demand materialises, this is its own design doc.

---

## 7. Open questions

These need answers before Phase 0 ships, but they shouldn't block discussion:

**Q1 — Embedding consistency.** Do we ship Approach 1 (cloud embedders even for local-first agents) with an explicit caveat, or do we hold local-first back until we have a local embedder story? My take: ship Approach 1 with the caveat documented in `LOCAL_MODELS.md`. Treat local embeddings as Phase 5b.

**Q2 — Model selection per agent.** When an agent has `"provider": "ollama", "model": "qwen2.5:7b"`, what happens if the operator's Ollama doesn't have `qwen2.5:7b` pulled? Options: (a) error clearly, (b) fall back to whatever Ollama has available, (c) prompt the user to pull. Recommend (a) — fail loud, no silent degradation.

**Q3 — Composition mixing.** Should we allow a workspace to have *both* local-first and cloud agents? Or is local-only an all-or-nothing workspace policy? Recommend allow mixing by default; surface the mix in the workspace metadata; let `local_only` policy be the lockdown switch.

**Q4 — Dreaming on local.** Memory consolidation is currently cloud-only. Should local-first workspaces have a different consolidation path (smaller batches? local consolidator agent?)? Recommend keeping dreaming cloud-only initially — consolidation quality matters more than execution quality for Loop 1 to actually close.

**Q5 — Catalogue economics.** If local-first agents have zero per-execution cost, marketplace ranking by `total_cost_usd` becomes meaningless for them. New ranking dimension needed: `executions_per_day`? `workspace_count_active`? Recommend a separate ranking dimension `usage_velocity` that doesn't depend on cost.

**Q6 — Eval baselines.** Many evaluators were calibrated against cloud-model outputs. Do we need per-provider eval baselines? Recommend yes for any evaluator producing numerical scores (relevance, accuracy, completeness) — but defer the recalibration work until we have ≥100 local episodes to baseline against.

---

## 8. Strategic framing — substrate strategy and vertical pull

### 8.1 ABW as substrate, not as product

ABW is pre-release and seed-stage. It does not have a direct customer pipeline in the conventional SaaS sense, and trying to evaluate topology decisions against "is there a buyer for this feature?" is anachronistic — the platform has deliberately not been built around direct end-user sales.

The strategy is structurally an AWS-style substrate play: ABW becomes useful by being the platform that the author's other projects (and, increasingly, external developers' projects) build on. Each vertical pulls capability from ABW, exposes that capability to its own market, and the cumulative pull *is* the substrate's validation. There is no separate "ABW customer" to interview about distribution preferences; the verticals consuming ABW *are* the demand signal.

This changes how Phases 0–5 should be justified. The relevant question is not "would a clinical-research buyer pay for local-first?" but "which capability does each vertical need ABW to expose, and does the current topology block it?"

### 8.2 Current verticals pulling on the substrate

Four verticals are active or imminent. Each has different topology requirements:

| Vertical | What it is | What it pulls from ABW | Topology requirement |
|---|---|---|---|
| **Rabble** | Spatial multi-user AR app (creatures, swarms, flights, perches) | The full agent stack + spatial state (H3, GeoJSON) + multi-tenant social graph + AR beacon primitives | Eventual federation across regions; resilience to single-cloud outage; capability-typed routing for spatial agents (gbif, naturalist, navigator) |
| **kask / SimOps** | Process-design workbench for bio-manufacturing (Doc 2) | Domain-constrained MoE (`simops` strategist) + Loop 5 calibration against SOSA observations + per-stage value mining + capability gates for regulated content | Operator-hosted Ollama (closed-data domains in clinical and regulated bio); regional data-residency for EU manufacturing tenants |
| **fermi-console** | Forecasting console (Tetlock-style decomposition, FPL, calibration backtests) | Fermi as canonical domain-constrained MoE + Loop 5 routing calibration + workspace state for forecast portfolios + Polymarket integration | Long-running forecast scheduling; partition tolerance for calibration data ingest; eventual user-hosted forecasters |
| **silat** | Author's genealogy App (your own project) | Ontology + KG primitives for genealogical relationships + long-running episodes spanning years + privacy-respecting workspace state | Per-user data residency; long-horizon RSI signal (a genealogy workspace persists for decades); offline-resilient access |

Each of these is an existence proof that the substrate is real. Each surfaces topology requirements the current centralised implementation does not yet satisfy.

### 8.3 The first external-developer milestone — efrain

The strategic milestone that converts ABW from "Ivan's substrate for Ivan's projects" to "substrate" is **the first App built on ABW by someone other than the author**. That milestone is named: Mario's project, **efrain**.

efrain matters out of proportion to its own product scope. Mario is building it using the public surfaces ABW now exposes:

- `agents/templates/` — the agent-card scaffolding, with `companion_builder_coach` walking authoring sessions
- `xaman_ek`'s MCP endpoint — for design dialogue and discovery (composition_design session mode, agent_design session mode)
- `docs/specs/01_APP_PRIMITIVE.md` + `docs/specs/03_BUILDING_NEW_APPS.md` — the App primitive and the "30-minute App" recipe
- `POST /api/apps` — manifest registration
- Workspace API + workspace-template + auto-hire + initial_files — runtime provisioning
- `@agent_name` messaging + `POST /api/agents/:id/execute/stream` — invocation patterns
- The bestiary catalogue and Xaman Ek's omniscient registry — capability discovery

If Mario can build efrain end-to-end using these surfaces without the author's direct involvement, that demonstrates: the App primitive is real, the agent-authoring loop is real, the composition design surface is usable by an outsider, the documentation is sufficient, and Xaman Ek functions as the platform's discovery interface for someone who didn't design it.

Every friction Mario hits while building efrain is a topology or surface-design signal that comes from *outside the author's head*. That is structurally different from any signal the author can generate by introspection. It is also the first piece of evidence that the AWS-strategy bet is real — that ABW is generative-enough as a substrate to host work the author did not anticipate.

This is what the topology proposal should be evaluated against: **each phase should leave Mario's path easier, not harder**. If a topology decision makes the external-developer recipe more complex, that decision is wrong, regardless of how clean the architecture diagram looks.

### 8.4 What Phases 0–4 contribute to vertical pull

| Phase | Rabble pull | kask/SimOps pull | fermi-console pull | silat pull | efrain pull |
|---|---|---|---|---|---|
| 0 — Ollama provider plumbing | Future field-deployment readiness | EU clinical/regulated tenants can self-host inference | Backtest runners can use local for cost | Privacy-conscious user can opt out of cloud inference | Mario can develop locally without burning credits |
| 1 — Honest economics | Lower per-rabble cost | Lower bioreactor monitoring cost | Lower backtest cost | User-friendly pricing | Lower development cost |
| 2 — Observability annotation | Per-region drift visibility | Per-tenant compliance audit trail | Per-provider calibration tracking | Per-user privacy log | Per-execution debugging surface |
| 3 — Capability gates | Frontier agents (xaman_ek) refuse local | SimOps strategists refuse local | Fermi refuses local | Silat ontology agents declare requirements | Mario gets clear errors instead of garbage output |
| 4 — Local-first agent class | Future Rabble-Lite (offline-capable rabbles) | Bioreactor-local agents | Local backtest agents | Privacy-first genealogy agents | Mario can author local-first agents |

The pattern: **no single vertical needs all of Phases 0–4 urgently, but every vertical needs at least one phase to unblock something concrete**. This is exactly the substrate-investment pattern AWS used in its early years — invest in capabilities each of which has a believable single-vertical justification, but which together compound into something none of the verticals could justify on its own.

### 8.5 What this means for the §10 long arc

The long-arc topology (§10) is not justified by "will there be customers for it?" It is justified by:

- The current verticals (Rabble, kask/SimOps, fermi-console, silat) eventually demand it
- External developers (efrain and successors) eventually demand it
- The RSI primitive (§1.2) is robust enough by then to *survive* the distribution
- Operating ABW as substrate-infrastructure-for-others is a structurally different posture from operating it as a vendor SaaS, and the topology should reflect that posture

This is the AWS posture: build the substrate the verticals need; the substrate's value becomes apparent in the cumulative pull, not in any single vertical's market analysis.

---

## 9. Recommendation

Approve Phases 0–4 (about 10 days of work, sequenced over 2 weeks). Defer Phase 5 (runner-relay) and Phase 6 (local as parallel tier) until real Phase 0–4 usage signals demand.

**On A4 (CRDT state-peer) and the §10 end-state:** do not pursue them *as engineering work* in this proposal. **Do** carry the §10/§11/§12 framing into product, governance, and architecture decisions so that the path remains reachable. Specifically:

- Treat Phases 0–4 as a *one-way door* test: every architectural choice in those phases should leave A3 / A4 / federated / commons-governed end-states *reachable* without committing to them. (§10.5 lists the specific reachability checks.)
- Treat A4 as a deferred phase with **specific re-evaluation triggers** (§10.8), not as "rejected." The triggers are written so an operator, an investor, or a future maintainer can recognise when the situation has changed.
- Treat the commons / ULS framing (§§10–12) as a *governance and product* track in parallel with the engineering track. The technical work in Phases 0–4 doesn't depend on it, but the conversations it forces — who governs, who stewards, what super-peer responsibilities look like — should start now, not after the technical work commits us to a particular shape.

Next action if approved: open a tracking issue per phase; scaffold Phase 0 (provider plumbing + one example agent); put `OLLAMA_BASE_URL` in `.env.example` and `DEPLOYMENT_GUIDE.md`; ship a kask integration test that exercises `simops_narrator_local` end-to-end; **and** open a tracking thread for the governance conversation (§12) involving Ivan + the curated-agent maintainers.

---

## 10. The long-arc end-state — ABW as a small ULS system

This section sketches the *aspirational* end-state for ABW's topology and traces how each phase of this proposal moves toward it (or away from it). It is **not** the scope of Phases 0–5. It is the frame against which Phases 0–5 should be evaluated.

The framing comes from three threads of prior art that interlock more than they're usually read as doing:

- **The ULS report (CMU/SEI 2006)** — describes what large software-intensive systems become when they cross certain thresholds (decentralized control, normal failures, eroded people/system boundary, continuous evolution). The report's central claim is that such systems require **new architectural paradigms** rather than scaled-up versions of today's approaches. Section 1 of this proposal already invoked the seven ULS characteristics; this section asks what they imply for topology specifically.
- **IBM Autonomic Computing (2001–2006)** — the self-* properties: self-configuring, self-healing, self-optimizing, self-protecting. These are the *runtime* properties a ULS-shaped system needs to have. Autonomic computing was an over-promised brand a generation ago, but the four self-* targets are still the cleanest articulation of "what does a system that operates itself look like."
- **Scale-free network theory (Barabási-Albert 1999) + libp2p / IPFS / Kademlia** — the empirical observation that most real distributed systems exhibit power-law degree distributions (a few hub nodes carry most of the traffic), combined with the engineering question of how to *use* that fact rather than fight it. The modern P2P stacks (libp2p / IPFS / Bluesky's AT Protocol) embrace scale-free topology and add capability-aware routing on top.

The end-state in one paragraph: **ABW runs as a network of heterogeneous nodes, each declaring its capabilities (what models it can run, what data it holds, what governance role it accepts), with a discovery layer that routes requests to the right node and a governance layer that constrains how super-peer status accrues and what duties it carries. Workspace runtime state is replicable across nodes; platform-economic state remains under explicit governance (today: server-authoritative; long-term: under a commons-governance protocol — see §11). The system is self-configuring (a new node joining is a single command, capabilities discovered automatically), self-healing (a node failing doesn't bring its workspaces down, they reconcile against other replicas), and exhibits emergent scale-free topology *constrained by* governance rules that prevent winner-take-all collapse.**

That is the aspiration. It is decades of work if approached as a re-platforming. It is *years* of work if approached as a series of phases each of which delivers something useful on its own. This proposal is the first phase.

### 10.1 The five topologies along the path

To make the long arc concrete, here are the topology stages — each useful on its own, each a stepping stone — and where each phase of this proposal lives:

| Stage | What it is | What's distributable | What's centralized | Where this proposal lives |
|---|---|---|---|---|
| **T0. Server-monolith** | Today: one Postgres, one Rust binary, cloud LLMs | Nothing | Everything | Status quo |
| **T1. Server + heterogeneous compute** | T0 + local LLM endpoints (A1/A2) | Inference target | Orchestrator + state + economics + governance | **Phases 0–4** of this proposal |
| **T2. Server + remote-compute peers** | T1 + user-side runners (A3) | Inference target, located on user hardware via WSS dial-out | Orchestrator + state + economics + governance | **Phase 5** |
| **T3. Federated state-peers** | T2 + multiple coordinating ABW operators (multi-tenant or multi-operator, with cross-operator workspace sharing under federation protocols) | Inference + workspace runtime state across operators | Per-operator economics; federation governance protocol | **Future — not in this proposal but unblocked by Phases 0–5** |
| **T4. Per-user state-peers (A4)** | T3 + every user can host their own ABW replica with CRDT sync | Inference + workspace state + most runtime state, per user | Wallet / marketplace / cross-operator identity | **Future — see §10.4 (formerly §10 of the previous draft, A4 analysis)** |
| **T5. Scale-free commons** | T4 + capability-aware overlay + emergent super-peers + commons governance protocol | Effectively everything except the constitutional / governance protocol | The protocol itself, enforced socially + cryptographically | **The aspiration — §10.6** |

Each stage is independently useful. Each stage leaves the next stage reachable without committing. T1 (this proposal) is the foundation that lets every subsequent stage exist as an extension rather than a rebuild.

### 10.2 The four self-* properties as ABW design targets

The IBM autonomic-computing framing maps cleanly to ABW design questions, and gives Phases 0–4 a checklist:

| Self-* property | What it means | ABW today | Phase 0–4 contribution |
|---|---|---|---|
| **Self-configuring** | Nodes discover available capabilities and configure themselves; no central installer | Manual env vars per provider | Phase 0 keeps the env-var pattern; **doesn't move us forward**, but doesn't block (capability declaration on agent cards is the lever, not env vars) |
| **Self-healing** | Failures are normal; system reconciles without operator intervention | Single point of failure (Postgres); HITL queue handles agent-level anomalies | Phase 2 (observability annotation) and Phase 5 (runner-relay reconnect logic) add small partial healing surfaces |
| **Self-optimizing** | The system steers toward better outcomes given its goals; the five feedback loops are a partial implementation | Loops 1–5 partially implemented | The proposal's capability-gates work (Phase 3) and per-provider eval (Phase 2) make Loop 5 (calibration) genuinely cross-provider, which is a self-optimisation precondition |
| **Self-protecting** | The system defends its own integrity against attack and misuse; identity, trust, governance | HITL queue + curated-tier review + auth | Phase 3 capability gates are a small but real self-protection move: agents declare what they need, platform refuses bad routings |

ABW today has weak partial implementations of all four. The honest framing is that the platform is **on the path** but not yet self-* in any robust sense. This proposal contributes incrementally to two (self-optimizing, self-protecting) and is neutral on the other two.

### 10.3 Scale-free topology as inevitable (and how to constrain it)

A property of essentially every large distributed system observed empirically: **degree distributions follow power laws**. New nodes preferentially attach to nodes that are already well-connected (Barabási-Albert preferential attachment). This is true of the internet's AS graph, the web's hyperlink graph, GitHub's import-from-which-package graph, Bluesky's follow graph, npm's dependency graph, every observed P2P network in production.

For ABW this means: as the network grows, *some nodes will become super-peers whether we design for them or not*. The Railway-hosted "official" ABW node is already one. If we federate (T3), a few federation hubs will emerge. If we go peer-to-peer (T4–T5), a few well-resourced operators will become discovery / routing / caching hubs.

The choice is not "scale-free or not." It is **"do we acknowledge scale-free emergence and design governance for it, or do we pretend it won't happen and watch the platform get captured by whichever super-peer accumulates fastest?"**

The architectural answer is *capability-aware scale-free with explicit super-peer responsibilities*:

- Nodes declare capabilities (which models, which agents are mirrored, which workspaces are replicated, which discovery indexes are served)
- Routing is done against capabilities, not against node identity (capability-based addressing — Miller's object-capability literature, IPFS's CID-based addressing)
- Super-peers emerge naturally for high-traffic capabilities (a node that mirrors many curated agents will get more routing requests for them)
- Super-peer *status* is observable, and confers *duties* (uptime SLAs, governance participation, contribution back to commons) — not just privileges
- The constitutional layer (§11) constrains how super-peer privilege/duty ratios can shift

This is what libp2p, IPFS, and Bluesky's AT Protocol actually do in production. It is the closest existing template for what an ABW commons could become.

The connection to AKP's "NOT scale-free" stance is important and is treated in §11.

### 10.4 Waypoint analysis: A4 (state-peer architecture)

This section is the substantive analysis of stage T4 in the §10.1 stage table — the point at which the user's machine becomes a *state peer*, not just a compute target. The analysis is retained from earlier drafts because the conclusions are sound, but the framing has shifted: A4 is one waypoint on the path to T5, not "the alternative architecture," and the implementation strategy is treated as a capability decision (replication / sync) with multiple plausible mechanisms — not a commitment to any single one of them.

#### 10.4.0 Sync is a capability, not an implementation

Earlier drafts of this proposal framed A4 as "CRDT-synced state-peer architecture." That conflated a capability (state replication across distributed replicas) with one implementation of it (CRDT). The framing is wrong, and the correction matters because CRDTs have known scalability and correctness limits that make them a poor universal default for ABW's workload.

The capability is: **let workspace runtime state be authoritatively held on multiple nodes, with reconciliation when replicas reconnect, while preserving the RSI signal integrity invariants in §2 goal 1.**

The plausible implementations of that capability — each with different trade-offs — are:

| Mechanism | What it is | When it fits | When it doesn't |
|---|---|---|---|
| **CRDTs** (Yjs, Automerge, Loro) | Operation-log replication with deterministic merge; convergence guaranteed regardless of order | Append-mostly data (chat, comments, telemetry); data with no semantically meaningful conflicts | Op-log metadata cost is non-trivial at write scale; LWW on semantically meaningful fields (eval scores, persona_version) makes arbitrary choices that are *correct under the protocol but wrong under the domain*; garbage-collection of compacted ops is operationally painful |
| **Event sourcing with explicit reconciliation** | Append-only event log; state is a fold over events; conflicts resolved by domain-specific rebase rules | Data that already has a natural event log (which is most of ABW); cases where conflicts need *named* resolution rather than deterministic merge | Reconciliation rules must be authored per event-type; not a free lunch |
| **Server-arbitrated soft-merge** | Optimistic local writes; on reconnect, server picks the canonical state and replays divergent local ops on top, surfacing conflicts to the user | Cases where one party can be authoritative; cases where human review of conflicts is acceptable | Requires the server to be the writer of last resort; not viable for fully peer-to-peer |
| **Snapshot replication with offline-as-read-only** | One replica is canonical; others receive snapshots; offline = read-only | Cases where offline editing isn't required; cases where eventual consistency would lose data | Doesn't satisfy users who want to *work* offline — only to *read* offline |
| **Two-phase commit / consensus** (Raft, Paxos) | Strongly consistent across replicas at the cost of liveness during partition | Wallet / ledger / marketplace — anything where double-spend is fatal | Not viable for high-frequency runtime state — kills latency |

ABW's actual schema (worked through in §10.4.2 below) is not uniform across these mechanisms. Different table families fit different mechanisms. The right architectural move is to choose per-family, not to commit globally to one. CRDTs work for some tables. Event sourcing — which ABW *already partially does* — works for more. Consensus stays for the ledger.

An under-recognised fact: **ABW's existing data model is already largely sync-friendly in shape, just not in protocol.**

- `episodes` are append-only with monotonic clocks. Already a natural event log.
- `eval_signals` are append-only per-evaluator per-episode. Already a natural event log.
- `composition_versions` is a DAG with parent pointers. Already git-shaped.
- `workspace_messages` is chronologically ordered with sequence numbers. Already an event log.
- Active Dreaming Memory's ontology consolidation reads episodes (event log) and produces snapshots (`ontology_snapshots`). Already event-sourced with explicit reconciliation points.

The architectural pattern ABW *already uses* for Loop 1 (episode log → consolidation job → ontology snapshot) is the same pattern that solves multi-replica reconciliation for these tables. **What's needed is to make the event log replicable and the consolidation reconcilable, not to graft CRDTs on top of an already-event-sourced design.**

This is a substantively different recommendation from "add CRDT." It reuses existing shape. It preserves the RSI signal integrity invariants because consolidation already operates over event-log inputs. It is structurally cheaper than a CRDT rebuild. And it leaves CRDTs available for the table families (workspace_messages chat content, team_members membership ops) where they actually fit cleanly.

The rest of §10.4 walks through which ABW tables fit which mechanism, what the consequences are for the five RSI loops, and what the cost/risk picture looks like under a per-family strategy.

#### 10.4.1 What A4 actually is

A4 reframes the user's machine from a *compute target* (A3) to a *state peer* (T4). Concretely:

- The user's laptop runs a **local replica** of (some subset of) ABW: the agent executor, the local Ollama, **and a local copy of the workspace state** (workspace_messages, episodes, entities, facts, coherence_evaluations — the runtime state of one or more workspaces the user has access to)
- The cloud instance also holds the workspace state
- The two replicas synchronise via the per-family mechanisms identified in §10.4.0 (event sourcing with reconciliation as the primary pattern, CRDTs for chat-shaped content, consensus retained for ledger)
- Agents can execute on either replica — local executions write episodes locally; reconnection replicates and reconciles upward
- The user can work entirely offline. Their workspace doesn't stop existing when their Wi-Fi drops

This pattern — local-first plus selective sync, with mechanism chosen per data family — is what real production local-first systems do. Linear's offline mode, Couchbase Sync Gateway, Jazz (jazz.tools), Notion offline, the Ink & Switch local-first prototypes. None of these are pure-CRDT systems; all of them have authority boundaries where consensus or arbitration takes over from CRDT merging. **The protocol layer is mature; the architectural skill is choosing which mechanism per data family.**

#### 10.4.2 Schema audit: which mechanism per family

Walking the ER diagram domain-by-domain, with the right replication mechanism (per §10.4.0) for each:

| Domain | Best-fit mechanism | Why |
|---|---|---|
| **Episodes** (`episodes`, `episode_corrections`, `episode_tags`) | **Event sourcing** | Already append-only with monotonic clocks. Replication = ship the event log. Reconciliation = re-fold on receive. ABW already does this for consolidation. |
| **Memory & KG** (`entities`, `facts`, `semantic_rules`, `communities`) | **Event sourcing + content-addressed dedup** | Each new fact / entity carries a content hash; replicas dedup on hash. Conflicts on `description` / `confidence` fields are rare and resolvable via "highest confidence wins" or by re-extraction. RSI-safe because consolidation already treats this as event-sourced. |
| **Workspace chat** (`workspace_messages`) | **CRDT (or event log with monotonic clocks)** | Chat is the canonical CRDT case; Yjs/Automerge ship it out of the box. Equivalently solvable as an event log with sequence numbers, which ABW already has. Either works. |
| **Coherence evaluations** (`coherence_evaluations`) | **Event sourcing, server-authoritative writer** | Each evaluation is a snapshot at `(workspace_id, evaluated_at)`. Authoritative writer prevents fork: local replicas observe but only the home node writes. Critical for RSI signal integrity — divergent coherence scores would corrupt Loop 3. |
| **Workspace metadata** (`teams`, `team_members`) | **Server-arbitrated soft-merge** | "Member added" and "member kicked" are not commutative. CRDT remove-wins / add-wins protocols exist but produce surprising outcomes (kicked member rejoins by accident). Better to have an authoritative writer and surface conflicts. |
| **Composition versions** (`composition_versions`) | **Event sourcing (already git-shaped)** | DAG with `parent_id` pointers. Replication is just shipping the DAG. Acceptance / rejection events are themselves nodes. ABW is already structurally git here. |
| **Agents** (`agents`, `agent_versions`, `agent_avatars`) | **Server-authoritative for catalogue + LWW for user-mutable fields** | Two replicas cannot independently mint different agents under the same `agent_id`. Catalogue create / fork is a synchronous server operation. Once an agent row exists, fields like `display_alias`, `ontology_stats` are LWW-safe. |
| **Wallet / billing** (`wallets`, `credit_ledger`) | **Consensus / strong consistency, full stop** | Double-spend is fatal. Credits cannot be deducted offline and reconciled later. Every charge is a server round-trip. This is non-negotiable. |
| **Marketplace** (`marketplace_listings`, `marketplace_transactions`) | **Consensus, same as wallet** | Listings / purchases are atomic transactions; conflict resolution is meaningless ("two users bought the last copy at the same time"). |
| **Stripe / payment state** | **External authority** | Stripe is the source of truth; ABW never holds it locally. |
| **Observability ingest** (`anomaly_events`, `eval_runs`, `eval_signals`) | **Event sourcing** | Ingestion is append-only; ship the event log. RSI-critical: eval_signals are Loop 1 input, must replicate cleanly. |
| **Observability resolution** (`hitl_actions`) | **Server-arbitrated** | "Two reviewers independently resolved the same anomaly" needs reviewer locks. CRDT would produce arbitrary winner. |
| **Rabble + spatial** (`creatures`, `swarm_events`, `flight_telemetry`) | **Event sourcing for telemetry; server-arbitrated for state transitions** | Telemetry is append-only and high-volume — event log fits. State transitions (active → sleeping, anchor changes) need authority. |
| **Social** (`creature_friendships`, `creature_invites`, `notifications`) | **Two-party protocol + server arbitration** | Friendship is a two-party agreement; cannot be unilaterally synced. Each party emits an "accept" event; server arbitrates conflict (e.g., simultaneous block + invite). |
| **Forecasting / calibration** (`fermi_*`) | **Event sourcing** | Forecasts are append-only; calibration scores update on resolution events. RSI-critical for Loop 5: must replicate without double-counting. |

The pattern: **most ABW tables fit event sourcing (because they're already structurally event-sourced); CRDTs are appropriate for chat-shaped data; server arbitration handles two-party / membership / resolution operations; consensus stays for the wallet and marketplace.** No single mechanism wins globally.

The line is drawable, but more granularly than the previous draft suggested. A4 is feasible as a **per-family-mechanism hybrid** where local replicas hold workspace runtime state via event sourcing (with CRDT supplementing chat-shaped fields), the server retains arbitration authority for membership and resolution operations, and consensus is reserved for economic state. This is *substantially less* engineering than a global-CRDT design, because most of the event-sourcing pattern is already in ABW.

##### RSI signal integrity — the non-negotiable invariants

Goal 1 in §2 requires that any topology change preserve RSI signal integrity. Concretely, this means:

- **Per-agent episode log is linearly orderable.** Episodes from a single agent must have a total order under reconciliation. Two replicas writing concurrent episodes for the same agent must produce a deterministic merge (sequence number assigned by home node, or vector clock with stable tiebreaker).
- **Persona_version transitions are globally consistent.** A persona bump is the boundary for a new drift baseline; concurrent bumps from two replicas would corrupt the baseline. Persona_version is a server-arbitrated counter.
- **Coherence evaluations reference a single canonical message log per workspace.** Two replicas cannot compute coherence over divergent message orderings and call them comparable. Loop 3 requires a stable message order per workspace; the home node assigns it.
- **Calibration scores accumulate without double-counting.** Loop 5 routing-weight updates are idempotent against resolution events; resolution events have a stable identity (per-forecast UUID + outcome) that prevents replication-induced double-counting.

These invariants are achievable under the per-family-mechanism strategy above. They are **not** trivially achievable under naive CRDT — which is why the previous draft's "use CRDT" framing was too coarse.

#### 10.4.3 Consequences for the five feedback loops

Each loop assumes a specific shape; A4 forces each one to be reconsidered against the per-family-mechanism strategy (§10.4.2):

| Loop | Today | Under A4 (per-family mechanism) | Required change |
|---|---|---|---|
| **1 — Learning** | Server runs consolidation against the agent's episodes | Episodes replicate via event sourcing; consolidation can run on home node or remote, reading the event log either way | Consolidator becomes location-elective: the home node runs canonical consolidation, remote replicas can run *advisory* consolidation locally for fast feedback, but only home-node consolidation produces the authoritative `ontology_snapshot`. Preserves RSI integrity. |
| **2 — Correction (HITL)** | Anomalies queue on the server; reviewers act against the server | Anomaly events replicate (event sourcing); resolution stays server-arbitrated (reviewer locks) | Same as today — resolution remains authoritative. Local replicas surface anomalies for *awareness* but don't *resolve*. |
| **3 — Coherence** | Inner loop fires every N server-side messages | Workspace_messages replicate as an event log with stable ordering assigned by home node; coherence evaluations are server-authoritative writes (§10.4.2) | Local replicas can *display* coherence scores but cannot *write* them. Loop 3 inner computation runs on home node against the canonical message order. Convergence is automatic because there is one writer. |
| **4 — Composition evolution** | Dreaming runs server-side, proposes composition_versions | composition_versions DAG replicates; Dreaming runs on home node | Lower priority — composition Dreaming is a weekly cadence, can wait for sync. Proposals appear in all replicas after they're written on home node. |
| **5 — Calibration** | Brier scores accumulate against resolved forecasts | Resolution events replicate via event sourcing with stable per-forecast IDs; calibration updates remain server-arbitrated to prevent double-counting | Server-authoritative on score updates; resolution events replicate cleanly with idempotent identity. |

The pattern: **assign a single canonical writer per RSI-critical state, replicate the event logs that feed it, let advisory computation run anywhere.** This preserves all four RSI integrity invariants from §10.4.2 without requiring CAP-theorem-breaking consensus across replicas. Loops 1, 2, 4, 5 work cleanly. Loop 3 is the most stringent — but is handled by the simple rule that *the home node writes coherence scores; remote replicas display them*.

This is structurally easier than the previous draft suggested. The previous framing said Loops 1 and 3 created "real complexity" under CRDT, which was true. Under event sourcing with single-writer arbitration for the few RSI-critical write paths, the complexity drops substantially.

#### 10.4.4 Cost and risk under the per-family-mechanism strategy

The earlier draft estimated 6–12 months for a CRDT-first A4. Under the per-family-mechanism strategy (§10.4.0) the cost is materially lower, because ABW's existing event-sourced shape carries most of the work:

| Dimension | Estimate | Notes |
|---|---|---|
| **Engineering effort** | 3–6 months for a credible v1 | Event-sourcing replication for episode-shaped tables (~1 month — leverages existing consolidation pattern); CRDT layer for chat-shaped fields (~3 weeks — off-the-shelf Yjs/Loro); server-arbitrated paths for membership / resolution (~2 weeks); local-replica runtime + UI (~6–8 weeks); full eval cycle and partition-tolerance regression suite (~6–8 weeks) |
| **Schema migrations** | Moderate. Add `node_id` and `vector_clock` columns to event-log tables; add reconciliation metadata where needed. Most tables already have the right shape. | Backwards-compatible: existing server-only deployments ignore the new columns. |
| **Client-side runtime** | Real but bounded. Embedded SQLite for local replica; reuse the executor stack from the existing Rust binary; a thin replication / sync daemon. | The `abw-local` binary is feasible because the Rust executor stack is already library-shaped, not webapp-shaped. |
| **Eval baseline drift** | Manageable | Per-provider eval baselines (Phase 2 of this proposal) already segment cloud vs local. Per-replica drift is the same problem at finer granularity; same machinery applies. |
| **Operational complexity** | Real | Two-headed product: bugs become "did it happen on which replica? did the event log converge? was reconciliation correct?" Mitigated by single-writer-arbitration on RSI-critical paths (most race conditions disappear). |
| **Wallet / marketplace boundary** | Constant UX friction | "I can do this work offline, but I can't *hire* an agent because that costs credits, and credits require a server round-trip." Surfaced as a clear UX boundary, not a bug. Same line every local-first product with monetisation draws. |
| **Marketplace telemetry impact** | Smaller than feared | Agents executing locally still emit episodes; episodes replicate when reconnected; `total_executions` reconciles eventually. Royalty calculations operate over the replicated event log, not real-time telemetry. |

The headline cost change: **3–6 months under event-sourcing-first, vs. 6–12 months under CRDT-first**. The cost reduction comes from not fighting ABW's existing data model. CRDTs were never the right primary mechanism; they remain useful for a small set of table families.

#### 10.4.5 What A4 would unlock

A4 (T4 in the §10.1 stage table) would enable, in capability terms (not customer-fit terms):

1. **True air-gapped operation.** A vertical operating in a closed-data environment runs ABW locally with the cloud component disabled. The replication daemon is a no-op; the local replica is the only replica. SimOps for regulated bio is the obvious near-term consumer.
2. **Offline-first UX for any vertical.** Field researchers (Rabble field deployments), conference travel for kask users, intermittent connectivity for silat users curating ancestral records — all continue working when the network drops.
3. **Per-user data sovereignty.** Episodes for a silat user's family history live on their own disk by default. Cloud replication is opt-in per workspace.
4. **Edge deployments at zero per-edge infra cost.** The Ambu bioreactor on a factory floor runs ABW directly with cloud as supervisor, not as critical-path orchestrator.
5. **Substrate-grade reliability for the platform as a whole.** When the cloud node goes down, workspaces with local replicas degrade gracefully rather than disappearing. This is *self-healing* (§10.2) achieved through replication topology.

These are platform capabilities. Whether to ship them depends on vertical pull (§8) — which is treated in §10.4.6 below using the corrected framing.

#### 10.4.6 Vertical-pull assessment (not customer-fit)

As §8 establishes, ABW is substrate-strategy, not direct-customer-sales. The relevant question is not "which buyer wants A4?" but **"which vertical pulls hard enough on A4-grade capability that the platform is blocked without it?"**

A worked-through vertical-pull table:

| Vertical | Phases 0–5 sufficient? | A4 needed when? | Signal |
|---|---|---|---|
| **Rabble** | Yes for current scope (single-region cloud-hosted) | When Rabble field deployments need partial-connectivity resilience, or when cross-region federation begins | When rabble users start reporting "I lost a flight because Wi-Fi dropped" or when a partner deployment requires regional residency |
| **kask / SimOps** | Yes for current scope (operator-hosted Ollama covers regulated tenants) | When SimOps gains a tenant with absolute air-gap requirement (no cloud surface at all, ever) | First time a tenant signs an NDA that prohibits *any* outbound network connection from the workspace |
| **fermi-console** | Yes for current scope (cloud-hosted with provider redundancy) | When fermi-console gains backtests that need to run on user-managed hardware for cost or data-residency reasons | When ~10+ users have asked to run fermi locally |
| **silat** | Yes for current scope | Early-adopter signal — genealogy users often want sovereignty over family records | Already present in this segment as a population trait; would materialise as feedback the moment silat ships its first beta |
| **efrain (Mario)** | Yes — A1/A2 sufficient for development | When efrain users demand offline editing of their workspace state | Unknown — depends on what efrain becomes |
| **Future external developers** | Yes — A1/A2 sufficient for most app shapes | When an external developer wants to build a local-first or sovereignty-first App on ABW | Wait for the signal; don't pre-build |

The picture: **A4 is on the critical path for ABW's substrate strategy because at least three current verticals (Rabble, kask/SimOps, silat) have plausible near-term scenarios that surface A4-grade pull, and the engineering cost under the corrected mechanism strategy is half what the earlier framing suggested.** That changes the recommendation calculus.

That said, no current vertical pulls *urgently* on A4 today. The right move is to ship Phases 0–4 (which several verticals need now), keep §10.5 reachability discipline so A4 is not foreclosed, and re-evaluate A4 after Phases 0–4 land. The decision criteria (§10.4.8) name the specific signals that should trigger committing.

#### 10.4.7 Alternatives that capture some A4 benefits without A4

If the goal is *some* of what A4 offers — particularly air-gap capability and offline-first UX — there are lower-cost paths that capture much of the value:

| Alternative | Captures | Misses | Cost |
|---|---|---|---|
| **Self-hostable ABW (single-tenant Docker)** | Air-gap; data sovereignty; on-prem | Offline-first UX; per-user data partition | Already largely possible — just needs explicit documentation |
| **Self-hostable + A1/A2 Ollama** | Air-gap with no cloud LLM dependency either | Offline-first UX | Phase 0–4 of this proposal, plus a self-hosting guide |
| **A3 runner-relay only** | User's laptop GPU; per-user privacy of inference content | Air-gap (still needs cloud orchestrator); offline UX | Phase 5 of this proposal — 2–3 weeks |
| **Read-only offline mode** (cache workspace state for viewing only, no editing) | Offline reading; partial offline UX | Offline execution; offline editing | ~4–6 weeks; doesn't require CRDT |
| **Cloud-first with strong export** (snapshot workspace to disk, "ABW Pocket" personal viewer) | Data sovereignty in archival form; sovereignty narrative | Live offline work | ~2–3 weeks; mostly a UX exercise |
| **Full A4** | Everything A4 unlocks | Six to twelve months of work; halved velocity on everything else during build | Already discussed — 6–12 months |

The strongest mid-point is **self-hostable ABW + Phase 5 runner-relay**. That combination gives almost every benefit of A4 (air-gap, sovereignty, local inference) at roughly 1/10th the engineering cost, without committing the platform to CRDT semantics across half the schema.

The single thing the mid-point doesn't give: **offline editing of an active workspace**. That is the unique A4 benefit. The question is whether it's worth 6–12 months for that single benefit, given the customer evidence available today.

#### 10.4.8 Decision criteria — what would change the A4 recommendation

A4 should be revisited and probably approved if any of the following triggers fire. These are framed as *vertical-pull* signals (per §8), not customer-prospect signals.

1. **A current vertical surfaces a hard A4 requirement.** Specific examples:
   - kask/SimOps signs a regulated-bio tenant whose contract requires that no workspace data ever traverse a cloud provider, even for inference. A2 endpoint hosting is not sufficient because the *workspace runtime state* itself must stay on-premises.
   - Rabble field deployments (drone trainers, fieldwork apps) demonstrate that partial-connectivity resilience is a routine failure mode, not an edge case. Users start regularly losing flight state to network drops.
   - silat early-adopter feedback consistently surfaces "I want my genealogy workspace on my machine, not yours" as a top concern.
2. **External-developer signal.** Once efrain ships and 3+ other external developers have built Apps on ABW, if any of them encounters A4-grade blockers (offline editing, full data residency), that's a substrate-design signal that's stronger than internal speculation.
3. **A capability-replication runtime matures enough to absorb 50%+ of the cost.** The per-family-mechanism strategy in §10.4.0 reduces the cost, but a mature substrate library (Jazz, Loro production-readiness, a Rust-native local-first stack) reduces it further. If the cost falls below ~3 months engineering, the calculus changes.
4. **Strategic forcing function.** Regulatory rule changes (e.g., new EU AI Act provisions on data residency for agentic systems), a competitor positioning explicitly on offline-first, or a high-leverage partnership conditional on A4 capability.

Until one of these fires, A4 is the wrong shape of work for ABW's current trajectory. Phases 0–4 are sufficient for the current verticals.

#### 10.4.9 A4 waypoint summary

A4 (state-peer architecture, T4 in the §10.1 stage table) is **technically credible, scoped at 3–6 months of engineering under the per-family-mechanism strategy (§10.4.0), and structurally reachable from the Phase 0–4 baseline if reachability discipline (§10.5) is maintained**.

It is **not on ABW's immediate path** because:

1. No current vertical pulls *urgently* on A4 — Phases 0–4 plus optional Phase 5 (runner-relay) cover the active verticals' near-term needs
2. The opportunity cost during seed-stage substrate-building is non-trivial — 3–6 months is real platform time
3. The reachability discipline in §10.5 keeps A4 cheap to start *later* without paying for it *now*

A4 stays on the radar as the next waypoint after T2, and the next-but-one before T5 (the commons end-state, §10.6). When the §10.4.8 triggers fire — and they probably will for at least one current vertical within 12 months — A4 becomes the next major architectural commitment.

### 10.5 Reachability checklist for Phases 0–5

Phases 0–5 should not commit ABW to T1 in a way that makes T2/T3/T4/T5 harder than necessary. Concrete reachability checks for each phase:

| Phase | Specific reachability risk | Mitigation |
|---|---|---|
| **Phase 0** (provider plumbing) | Hardcoding `ollama` as a single global env var means later we can't have per-workspace or per-user routing | Make `OLLAMA_BASE_URL` the *default*, but allow workspace-level override in workspace settings. Cost: ~1 day extra. |
| **Phase 1** (gas-only billing) | Hardcoding "ollama provider == zero cost" means later providers (federation hops, runner relays) need separate cases | Introduce a `cost_class` enum: `metered \| flat_gas \| host_subsidised`. Ollama becomes `host_subsidised`. Federation hops become a new class without rewriting Phase 1. Cost: same as Phase 1. |
| **Phase 2** (observability annotation) | Provider tag is a `TEXT` column with free-form values | Constrain to a small enum *or* add `node_id` alongside provider — anticipates T3/T4 where execution location ≠ provider identity. Cost: marginal. |
| **Phase 3** (capability gates) | `min_provider_class: cloud_frontier` as enum is fine, but if it's hardcoded in the executor it can't extend to per-node trust scoring later | Make capability resolution a strategy pattern, not a hardcoded match. Cost: ~1 day extra. |
| **Phase 4** (local-first agent class) | "Local" as a single tag is fine; we just need the catalogue UI to be a *tag filter*, not a hardcoded button | Already a tag in the spec; no extra work. |
| **Phase 5** (runner-relay) | The runner protocol design choices here lock in a lot of T3/T4 surface | This phase deserves its own design doc when triggered. The protocol should be capability-aware from day one, not just an Ollama-specific job queue. |

The pattern: **all the reachability moves are cheap if made deliberately, expensive if discovered later.** This is the cost of carrying the §10 framing through Phases 0–5 even though §10 isn't the scope of those phases.

### 10.6 T5: the commons end-state

This is the aspiration that gives the rest of §10 its shape. It is the most speculative section of this proposal and is included as **target documentation**, not as a build plan.

T5 is ABW running as a **commons-governed scale-free network of capability-typed nodes**, with the following properties:

- **No single operator owns the network.** The "official" ABW (Railway-hosted) is one node among many, distinguished by reputation and history rather than by privileged protocol position. Other operators can run nodes that interoperate fully.
- **Capability-typed nodes.** Every node declares: which agents it mirrors (with what trust level), which workspaces it replicates, which models it can run, what compute it offers, what governance roles it accepts. Discovery and routing are capability-aware (libp2p/IPFS pattern).
- **Workspace runtime state is replicable across nodes.** Workspaces have a *home node* (the one that holds the authoritative replica) and *mirror nodes* (CRDT-synced replicas). Users pick which nodes to trust with their data.
- **Per-user identity is portable.** A user's identity is not a `users.user_id` in one operator's Postgres; it is a public-key identity that can authenticate against any node in the network (think DIDs, AT Protocol identities, or Nostr keys depending on which template wins).
- **Super-peers are observable and constrained.** Nodes that accumulate disproportionate traffic, replication weight, or routing centrality are *visible to the network* (the discovery layer exposes them) and bound by governance rules (§11) about what they can charge, exclude, or capture.
- **The economic layer is under explicit governance.** Wallets, credit issuance, marketplace transactions are not free-for-all-peer-to-peer; they sit under a commons-governance protocol (which may or may not use cryptographic enforcement — see §11) that *prevents* the scale-free topology from translating into scale-free wealth.
- **Self-* properties hold at the network level.** A node joining is self-configuring (it discovers what's needed and announces what it offers). A node failing is self-healing (workspaces with replicas on other nodes don't lose state). Resource allocation across nodes is self-optimising (capability-aware routing learns where to send what). Network-level threats are self-protecting (governance rules and capability-revocation give the network teeth against bad actors).

This is **not** original architecture. It is the synthesis of:

- **libp2p / IPFS** for the routing and discovery layer (capability-typed addressing, DHT-based peer discovery, well-developed Rust implementations)
- **Bluesky's AT Protocol** for portable identity, account-portability semantics, and federated-but-not-quite-P2P aggregation
- **Holochain's agent-centric model** for the per-user/per-agent-owns-their-chain pattern (each agent's ontology lives on the agent's home node, not in a global ledger)
- **Ostrom's eight design principles** (Hess & Ostrom 2007, on knowledge commons) for the governance layer
- **Frischmann/Madison/Strandburg GKC framework** (Governing Knowledge Commons, 2014) for the operational rules of digital commons
- **Beckstrom's Law** for the economic-network valuation that grounds the commons math

None of these alone solves ABW's problem. The interesting work is showing how they compose into a single design that respects each one's invariants. That work is the topic of a future architecture proposal — when ABW has the customer signal and engineering capacity to commit to T5.

For now, the value of documenting T5 is:

- It gives Phases 0–5 a target to point toward (the reachability checklist in §10.5 makes sense only with T5 in mind)
- It makes the AKP economic-topology reconciliation tractable (§11)
- It clarifies what ABW *is for* in the long arc — a commons substrate for multi-agent work, not a closed product — which has real implications for fundraising, partnerships, and culture
- It lets us say honestly to potential partners and contributors: "this is where the platform is going; here is the framing; here is what would have to be true for us to commit to building it"

---

## 11. Reconciliation with `AKP_DESIGN.md` §4.1 — commons economics vs runtime topology

The AKP design doc (`docs/architecture/AKP_DESIGN.md`) takes an explicit position in §4.1:

> **NOT scale-free** — scale-free topology means power law wealth concentration and monopolies. Monitor and prevent.

This proposal advocates **embracing scale-free runtime topology** (§10.3). On the surface that contradicts AKP. It doesn't, but the reconciliation has to be made explicit or future readers will see incoherence.

### 11.1 Two different topology questions

The AKP stance and this proposal are talking about *two different graphs* that happen to share the same vocabulary:

| Graph | Nodes | Edges | What "scale-free" means here | What it threatens |
|---|---|---|---|---|
| **Economic / knowledge-trading graph** (AKP §4.1) | Agents | Knowledge transactions, royalty flows, market bids | A few agents capture most knowledge-market revenue; the long tail starves | **Generativity collapse via economic extraction.** A captured economic graph sterilises the substrate: contributors stop contributing because they get no economic return; new agents don't appear because the existing ones extract all value. The platform stops producing unanticipated capability. |
| **Runtime / substrate graph** (this proposal §10) | Nodes hosting ABW capabilities | Routing requests, replication links, discovery announcements | A few nodes carry most traffic and serve most capability lookups | **Generativity collapse via routing capture.** A super-peer with no governance constraint can deny routing, throttle competitors, lock-in users, or simply fail and bring down its replication subtree. The platform stops being open to unanticipated participation. |

Both are *generativity failures* in Zittrain's sense — they reduce the system's capacity to produce unanticipated change through unfiltered contributions. The AKP stance and this proposal are addressing the same underlying threat from two structural angles.

The reconciliation: **the platform commits to preserving generativity at both layers, but the mechanism differs**. The economic graph is preserved through *active market governance* (Beckstrom-Law-positive market design, anti-capture rules in the marketplace, royalty distribution algorithms). The runtime graph is preserved through *capability decoupling and super-peer duties* — accepting that scale-free emergence is inevitable in routing graphs, but binding super-peer status to obligations rather than privileges.

### 11.2 The reconciled stance

Combining AKP §4.1 with this proposal §10:

1. **Both graphs are governed for generativity.** The success criterion in both cases is the same: does the system continue to produce unanticipated capability from unfiltered contributions? Both economic capture and runtime hub capture are failure modes of this criterion.
2. **Runtime topology will be scale-free**, because empirically every large distributed system is. The choice is to design for it (T5: capability-aware super-peers with explicit duties) rather than pretend otherwise. Generativity is preserved by the duty structure, not by attempting to forbid super-peer emergence.
3. **Economic topology must be actively governed against power-law capture**, per AKP. The marketplace, the credit ledger, the royalty system, and any commons-governance protocol (§12) are the levers for this. Generativity is preserved by the market structure, not by attempting to forbid agents from accumulating value.
4. **Super-peer status in the runtime graph confers duties, not economic privilege.** A node that mirrors many curated agents pays no special access fees, earns no special royalties; it serves the discovery / routing function for the network. This is the *structural decoupling* of the two graphs — generativity in each is preserved by preventing the other from feeding power-law accumulation back into it.
5. **Beckstrom's Law is the network-wide success metric.** The network is generative to the degree participants gain more than they pay to participate. Super-peers paying high uptime/replication costs but earning no special economic position only works if the *participation cost* of being a super-peer is offset by something — reputation, governance influence, intrinsic motivation (commons stewardship), or institutional support. §12 explores which of these are viable.

This is the working pattern Wikipedia uses: a power-law admin/editor hierarchy in the volunteer governance (runtime-graph super-peers) coexists with flat content rules that prevent admins from capturing content value (economic graph governed for generativity). The two graphs are decoupled. Each is preserved by the right mechanism for its layer.

### 11.3 What this means for AKP §4.1's wording

AKP §4.1 doesn't need to change in substance — its stance on the *economic* graph is correct and important. It should clarify that it's talking about the economic graph specifically, and frame the concern in generativity terms rather than only in scale-free terms. Suggested edit (not part of this proposal's scope, but worth noting):

> **Economic graph: generativity-preserving, not winner-take-all.** Power-law wealth concentration in the knowledge-trading economy means monopolies and starvation of the long tail — both of which sterilise the platform's generative capacity by removing the incentive for new agents and contributions. The market design, governor, and price-setting index must actively maintain Beckstrom-positive economics across the long tail. (Note: this constraint applies to the *economic* topology — agents earning from knowledge transactions. The *runtime* substrate that hosts ABW is permitted to be scale-free under explicit super-peer duty structures; see `DISTRIBUTION_TOPOLOGY_PROPOSAL.md` §10–§12.)

---

## 12. Governance roadmap — generativity at scale

### 12.0 The framing

Governance is the mechanism by which a system maintains generativity at scale. Without governance, scale-free emergence collapses both into economic capture (§11) and into routing capture (§10.3); the system stops producing unanticipated capability. With governance well-designed, scale-free runtime topology and active economic markets coexist with generativity preserved at both layers.

ABW today operates under *operator-as-sole-authority* governance — the author makes platform decisions, the curated tier is reviewed in one head, the economic rules are static. This is appropriate for seed-stage substrate development. It is not viable for T3+ federation, and it doesn't have to become viable until the platform itself demands it. This section sketches the path so the conversation can start in parallel with engineering, not after engineering has foreclosed options.

### 12.1 The two-commons structure of ABW

ABW already has two commons running in parallel, even though they're not named that way:

| Commons | Resource | Rivalrousness | Today's governance | Generativity threat |
|---|---|---|---|---|
| **Knowledge commons** | Agent ontologies, semantic rules, KG entities, episodes, eval signals, accumulated coherence patterns | Non-rival (info commons; congestion at access/governance) | Curated-tier review + community-tier visibility tiers + AKP economic protocol (proposed) | Free-riding, vandalism, low-quality contributions, capture of curated tier by gatekeepers |
| **Compute / credit commons** | Credits, gas, wallet balances, agent execution capacity | Rival (depletable; Ostrom natural-resource case) | Server-authoritative ledger; Stripe-mediated minting; static gas table | Double-spend, fraud, sybil attacks, hoarding |

The first is what Hess & Ostrom 2007 calls a *knowledge commons*; the second is what Ostrom 1990 calls a *common-pool resource* (CPR). They require structurally different governance machinery — knowledge commons govern *access and quality*, CPRs govern *consumption and replenishment*.

ABW's current implementation handles both by operator authority. That works under T0–T1 (the verticals consume what the author provides). It becomes untenable somewhere between T2 and T3 — when external developers (efrain and successors) start contributing to the knowledge commons, or when federated operators start sharing credit economies.

### 12.2 Ostrom and the GKC framework — applied selectively

Ostrom's eight design principles (1990, refined in Hess & Ostrom 2007 for knowledge commons) and the Frischmann/Madison/Strandburg GKC framework (2014) are the empirical baseline for stable commons. Rather than running through the eight as a mechanical checklist, the relevant question for ABW is: **which principles are already supported by platform machinery, and which ones surface gaps the topology path has to close?**

Three categories:

**Already structurally supported (light governance work):**
- Clear boundaries — auth identifies the commons participant; tier system separates curated / community / system
- Monitoring — HITL queue and observatory are the substrate for "accountable observers"; Loop 2 already runs
- Graduated sanctions — persona_version bumps and anomaly_events are proportionate responses; the machinery exists, the policy is implicit

**Partially supported (visible gaps at T2+):**
- Rules adapted to local conditions — static gas table and dreaming budgets are uniform today; T2+ needs per-workspace policy (already in proposal), T3+ needs per-operator policy
- Cheap conflict resolution — HITL queue and two_reviewer_requests pattern are a substrate; cross-operator dispute resolution is the T3+ gap

**Major gaps (the work that has to start before T3 is reachable):**
- *Collective-choice arrangements* — the largest gap. Today only the operator (Fermi/Ivan) can modify rules. A federated or commons-governed ABW requires actual community governance machinery. Treated in §12.4.
- *Recognition of rights to organise* — trivially satisfied today (no external authority), becomes substantive at T3+ when the constitutional protocol must be explicit and external-authority-proof
- *Nested enterprises (polycentricity)* — workspace ≈ operator ≈ federation layers exist informally; T3+ requires them to be formally polycentric, each governing its own scope

The pattern: **most of Ostrom's principles have substrate support in ABW already; the gaps are in collective-choice and polycentricity, both of which surface only at T3+**. This is good news for Phases 0–4 (no governance work blocks them) and predictive for what conversations have to start now to be ready for T3.

The GKC framework (Frischmann/Madison/Strandburg) is more operationally useful at the per-decision level — it structures commons analysis around background environment, attributes, governance, patterns, and outcomes. When the platform makes specific commons decisions (e.g., "how does the curated-tier review process work under polycentric governance?"), GKC is the analytic lens. Ostrom is the design baseline.

### 12.3 The constitutional protocol question

At T3+ (federated and beyond) ABW needs a *constitutional protocol*: the rules-about-rules that no individual operator or node can change unilaterally. Three templates:

| Template | Example | Pros | Cons |
|---|---|---|---|
| **Social/legal constitution** | Wikipedia (Foundation policy + community norms) | Cheap; human-readable; adapts fast | Hard to enforce cross-operator without legal infrastructure; depends on social consensus |
| **Cryptographic protocol** | Bluesky AT Protocol, Holochain, Bitcoin | Enforcement is automatic; cross-operator works by construction | Inflexible; protocol changes are slow and contentious; eats engineering effort |
| **Hybrid** | Most actual federations (ActivityPub-with-block-lists, AT Protocol-with-PDS-governance) | Balances flexibility and enforcement | Has all the failure modes of both; needs explicit conflict-of-laws design |

ABW would almost certainly want the **hybrid** path: a small protocol kernel (identity, capability declarations, message authentication) backed cryptographically, with the bulk of governance (curated-tier review, dispute resolution, sanctions) handled socially under documented norms.

This is a years-of-work conversation. Documenting it here makes it visible. Starting it now — in parallel with Phases 0–5 engineering — means we don't arrive at T3 unprepared.

### 12.4 Collective-choice: the open question

Ostrom's principle 3 (collective-choice arrangements) is the single biggest gap in ABW's current governance. Today: Ivan makes all platform decisions. That's appropriate for product-fit phase and probably for the next ~12 months. After that, if ABW is genuinely a commons, *somebody else* has to have decision power, or it's a benevolent dictatorship, not a commons.

Models that work in practice — drawn from where commons governance has been tested at digital scale:

- **Wikimedia Foundation** — a small staff manages infrastructure; an elected board sets policy; the editor community runs day-to-day governance. Works for the resource (encyclopedic content) but not perfectly for the people (governance disputes are chronic).
- **Mozilla Foundation** — similar shape; an organisation steward, a community of contributors, a foundation-owned protocol. Has eroded over decades but still functional for Firefox.
- **Apache Software Foundation** — explicit project-level governance, each project a meritocratic mini-commons, with cross-project standards set by foundation-level votes. Works extremely well for code commons.
- **MMOG governance (EVE Online, established MMORPGs)** — operator-run economy with elected player councils (CSM in EVE), persistent commons across persistent worlds, with the operator retaining authority over economic levers but the community having genuine policy voice. Most relevant precedent for ABW: the *structural* parallel to ABW is closer than to most open-source governance models, because MMOGs face the same problem ABW will face (persistent state across partial-trust users with emergent economic behaviour and continuous evolution).
- **Curated DAOs** (Optimism Citizens' House, Gitcoin grants, Protocol Guild) — token-weighted or reputation-weighted voting on a defined scope. Works for resource allocation; less proven for substantive policy.
- **Anarcho-syndicalist commons** (some open-source projects) — explicit rejection of formal structure, decisions by rough consensus. Works at small scale, fragments at large scale.

For ABW the closest existing template is probably an **Apache + Wikimedia + MMOG-council hybrid**: a Fermi-the-company stewards infrastructure and protocol; a curated-tier maintainer community has authority over the bestiary and ontology standards (Apache-shaped); an elected user council surfaces broader community policy (Wikimedia + MMOG-shaped); the constitutional protocol (§12.3) is the *minimum* coordination kernel binding the layers; everything else is social.

Concrete first step (not for this proposal, but as a starting marker): **a documented curated-tier maintainer process**. Today curated-tier review is "Ivan reads the PR." Making that an explicit process with multiple maintainers, documented standards, and a clear path from community-tier to curated-tier is Ostrom principle 3 in microcosm.

### 12.5 What this proposal commits to (and doesn't)

This proposal **does** commit to:

- Documenting the commons framing (§11–§12) so it's available for future conversations
- Reachability-checking Phases 0–5 against the §10 long arc so we don't foreclose T2–T5
- Starting the curated-maintainer-community conversation in parallel with Phase 0–4 engineering

This proposal **does not** commit to:

- Building a constitutional protocol
- Starting a foundation or DAO
- Tokenising or financialising the commons
- Federating with other operators before T3 is approved
- Any specific governance reform on a timeline

The commons framing is a *direction*. Phases 0–4 are the engineering. The governance conversation is the parallel track that has to start now if T3+ is ever going to be reachable.

---

## 13. Document map

For readers approaching this proposal by section:

- **§0** — Background: the platform's design line and the intellectual lineage this proposal builds on
- **§1** — Problem statement: what's being protected (the RSI primitive) and why the topology question is being asked now
- **§2** — Goals and non-goals; the RSI signal integrity invariants
- **§3** — The five engineering decisions (the immediate work)
- **§4** — The local-first agent class (the concrete output of Phases 0–4)
- **§5** — Embedding consistency (the honest caveat)
- **§6** — Phases 0–6, with effort estimates
- **§7** — Open questions
- **§8** — Strategic framing: AWS-style substrate strategy and vertical pull (Rabble, kask/SimOps, fermi-console, silat, efrain as the external-developer milestone)
- **§9** — Recommendation
- **§10** — Long-arc end-state: topology stages T0–T5, self-* targets, sync-as-capability with per-family mechanism, A4 waypoint analysis, T5 commons vision
- **§11** — Reconciliation with AKP §4.1: both economic capture and runtime hub capture are generativity failures
- **§12** — Governance roadmap: generativity at scale, the two-commons structure, Apache + Wikimedia + MMOG-council hybrid template

Reader tracks:

- **Engineering reviewers:** §§1–6 and §9 are the operational scope; §§10–12 are framing and only need a skim
- **Strategic reviewers (funders, partners, future maintainers):** §0, §1, §8, §10, §11, §12 are the substantive material; §§3–6 are implementation detail
- **Architecture reviewers (Gabriel / distributed-systems peers):** §0, §1.2, §10, §11 are where the philosophical and architectural commitments live
Strategic reviewers: §§1, §8, §10, §11, §12 are the substantive material. §§3–6 are implementation detail.
