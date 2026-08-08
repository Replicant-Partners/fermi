# 31 — Composability: audit and roadmap

**Status:** audit complete, roadmap proposed
**Companions:** `docs/COMPOSITION_AS_FIRST_CLASS.md` (the 2026 design),
`docs/AGENT_MODEL.md`, `SPEC_30_AGENT_TAXONOMY.md`
**Method:** every claim below is measured against the production database
and the current tree, not against the design intent.

---

## 1. The headline

The composition **substrate is extensively built and almost entirely
unadopted**. This is the exact inverse of the taxonomy problem, where rich
content existed and no surface read it. Here the machinery exists and the
content was never authored.

| | measured |
|---|---|
| Workspaces | **217** |
| …with a `mission` | **1** |
| …with a `coordination_strategist_id` | **1** |
| `composition_versions` rows (tune-team RSI) | **0** |
| Strategist agents tagged `coordination_strategy` | **1** of 6 designed |
| Forecasts that genuinely composed >1 agent | **48** |
| …via the mechanism the design document describes | **0** |

The last two lines are the important pair. Composition *does* work in
production — 48 forecasts, four agents each — but through a mechanism the
design doc does not mention.

---

## 2. What actually works, and why

**Driver binding.** A Fermi forecast declares named drivers. Each agent is
bound to specific drivers via `agents_used[].driver_refs`, returns a
multiplier for each, and the forecast model aggregates them. 48 forecasts,
33 of them with an identical 4-agent team.

```json
{"name": "football_analyst", "driver_refs":
  ["dynamic_performance", "squad_quality", "tactical_efficiency"]}
```

It works because it has all three things a composition needs, and it is the
only mechanism in the system that has all three:

1. **A typed join** — the driver name. Not a free-text label: the driver
   exists in the FPL model, so a binding either resolves or it doesn't.
2. **An aggregation operator** — the multiplier applied to a base rate.
   Composition is not "both agents ran"; it is a defined arithmetic.
3. **A scoring signal** — Brier on the resolved forecast, so the composition
   can be told whether it was any good.

**Agent-as-tool over MCP** also works and is the other genuinely typed path:
304 declared tools across 78 cards, of which **115 (37%) carry a real JSON
Schema `input_schema`**. A tool call either type-checks against that schema
or fails. `execute_agent` and `delegate_to_agent` are themselves published
tools, so agent→agent invocation is live today. What it lacks is (3): no
scoring signal attaches to a delegation.

---

## 3. Mechanism-by-mechanism audit

Scored against the three requirements. `join` = is there a typed thing that
connects two agents? `operator` = is there a defined way their outputs
combine? `score` = can the combination be told it was wrong?

| Mechanism | join | operator | score | State |
|---|:--:|:--:|:--:|---|
| **Driver binding** (`driver_refs`) | ✅ | ✅ | ✅ | **Works. 48 forecasts.** |
| **MCP tool call** (`input_schema`) | ✅ | ✅ | ❌ | **Works. 115 typed tools. Unmeasured.** |
| Counterfactual attribution | ✅ | ✅ | ✅ | Complete; waiting on mig-187 to deploy |
| Orchestra membership | ⚠️ | ❌ | ❌ | Works as a *roster*; injected into fermi's prompt |
| `dependencies.required/optional` | ⚠️ | ❌ | ❌ | On 95 cards; **one** consumer (`consolidation.rs`) |
| `workflow_template.stages` | ✅ | ✅ | ❌ | 9 cards, 36 stages, **never executed** |
| Strategist class | ⚠️ | ✅* | ⚠️ | 1 of 6 authored; *operator is the strategist's prompt |
| `composition_versions` (tune-team) | n/a | n/a | ⚠️ | Schema + 5 endpoints, **0 rows** |
| `teams.mission` / strategist | n/a | n/a | ❌ | Columns exist, **1 of 217 workspaces** |
| `accepts` / `produces` | ❌ | ❌ | ❌ | 267 values, **234 singletons** — labels |
| `output_contract` (typed) | ✅ | ⚠️ | ✅ | Only **3 cards**; carries a calibration signal |
| `dyad_state` | ⚠️ | ❌ | ⚠️ | 0 rows (89 profiles); in flight elsewhere |
| `workspace_dependencies` | ✅ | ⚠️ | ❌ | 48 rows, upstream→downstream DAG, undocumented |

### 3.1 The pattern in the failures

Every non-working mechanism is missing the **operator**, the **score**, or
both. None is missing only the join. That is diagnostic: the project has
repeatedly specified *what connects to what* and left unspecified *what
combining means* and *how you would know it worked*.

`accepts`/`produces` is the clearest case. 95 cards declare it, so it looks
like an interface layer. But 267 distinct values with 234 singletons means it
is free text, there is no operator that consumes a match, and nothing scores
the result. The Ecology lens now says this out loud — "labels, so
composability with them is asserted, not verified" — which is honest but is
not a fix.

### 3.2 The two most expensive gaps

**Counterfactual attribution is built and one deploy away from producing.**
`src/attribution/` implements exactly the right idea: re-run the forecast
model over subsets of the team to synthesise what each agent contributed,
rather than reading a team score as a per-agent score. Its own module doc
explains why the naive approach is unidentifiable — the membership matrix is
rank-deficient, so every member receives an identical score forever, at any
sample size.

`counterfactual_brier` is null on all 78 forecasts, but **not because
anything is broken**. Attribution reads per-agent driver claims from
`forecast_agent_claims` (mig-187) and returns `AttributionOutcome::NoClaims`
when it finds none. That table exists on disk and is registered in
`run_migrations`, but **does not yet exist in production** — it is in flight
from concurrent work, along with its write hook
(`workspace/agent_params_hook.rs`).

So the mechanism is complete and waiting on its input substrate to deploy.
Worth recording precisely, because "built and producing nothing" would have
sent someone debugging a working system. The one caution is that
`spawn_attribution` swallows `NoClaims` silently (`Ok(NoClaims) => {}`), so
there is no signal distinguishing "no claims recorded yet" from "claims
recording is broken" — which is exactly how this would go unnoticed a second
time.

**The strategist catalogue was never authored.** The design names six
strategists and states plainly that "none of these require new Rust code —
they're authored content." One exists (`moe_router_strategist`).
`cohere_and_coordinate` exists but is not even tagged `coordination_strategy`,
so the registry query the design specifies —
`SELECT … WHERE 'coordination_strategy' = ANY(tags)` — returns 1 row. The
composition creation flow has nothing to offer a user at step 3.

---

## 4. Roadmap

Ordered by ratio of value to remaining work. The first two are finishing
things that already exist.

### P0 — Land the claims ledger, then verify attribution produces (days)

Deploy mig-187 (`forecast_agent_claims`) and its write hook — already written,
registered, and in flight from concurrent work. Then confirm resolved
forecasts start carrying a `counterfactual_brier`.

Two things to add while doing it:

* **Make `NoClaims` observable.** `spawn_attribution` currently discards it
  (`Ok(NoClaims) => {}`), so "no claims yet" and "claim recording is broken"
  are indistinguishable. That silence is why an entire mechanism sat at zero
  without anyone noticing. A counter or a periodic log line is enough.
* **Backfill what can be backfilled.** The 48 already-composed forecasts
  record `agents_used[].driver_refs`, so some historical claims may be
  reconstructible. If they are not, say so — the attribution history starts
  from the deploy, and that is a fact worth stating rather than discovering
  later.

This is P0 because it is the only mechanism that can answer "which
combinations work", and that is the precondition for every later item.
Without it, Loop 4 has no input and composition quality is unfalsifiable.

**Done when:** resolved forecasts carry a counterfactual Brier, per-agent
contribution is queryable, and a zero-claims condition is visible rather than
silent.

### P1 — Tag and author the strategist catalogue (days, authored content)

Tag `cohere_and_coordinate` as `coordination_strategy` so the registry
returns more than one row. Then author the strategists the design already
specifies, cheapest first: `pipeline_strategist` (an operator that already
has a data model in `workflow_template.stages`), then `vote_strategist`,
then `debate_strategist`.

Each is a card plus a system prompt. No Rust. The reason to do it early is
that step 3 of composition creation is currently unanswerable, which is a
plausible cause of the 1-of-217 adoption.

**Done when:** ≥3 strategists tagged, and creating a composition offers a
real choice with `member_count_min/max` enforced.

### P2 — Execute `workflow_template` (1–2 weeks)

36 stages are declared across 9 cards, each with its own accepts/produces,
and nothing runs them. `pipeline_strategist` from P1 is the natural executor:
walk the stages, bind each to its agent, thread outputs to inputs.

This converts the richest existing composition declaration from documentation
into runtime, and it gives open slots meaning — a stage with no `agent` becomes
a typed vacancy the catalogue can be searched against. There is exactly one
open slot today (`observability_coordinator`), which will stay near zero until
slots do something.

**Done when:** a declared pipeline executes end-to-end and its stage outputs
are recorded.

### P3 — Type the interface, using MCP rather than a new vocabulary (2–3 weeks)

The instinct is to impose a controlled vocabulary on `accepts`/`produces`.
**Recommended against as the primary move.** A controlled vocabulary is a
naming exercise with no operator behind it — the same trap SPEC_30 found in
`order` and `genus`, one layer up.

Instead lean on the typed interface that already exists and already executes:
JSON Schema on MCP tools. 115 tools already carry one. The work is to make
*agent invocation itself* schema-typed — an agent's `accepts` becomes the
input schema of its published `invoke` tool — so composability becomes a
schema-compatibility check that can actually fail, rather than a string match.

Keep `accepts`/`produces` as human-facing labels for browsing. Do not promote
them to a contract; they have never been one.

**Done when:** the Ecology "can feed" panel can report *verified* schema
compatibility for schema-bearing agents, and drops its caveat for those.

### P4 — Extend the driver-binding pattern beyond forecasts (research)

Driver binding works because a quantitative model provides the join, the
operator and the score. The general question is whether that generalises: is
there an equivalent of a "driver" for non-forecast domains, or is
forecast-shaped work simply unusually composable?

Worth answering deliberately rather than assuming. If it does not generalise,
the honest conclusion is that the platform has one rigorous composition domain
and several ad-hoc ones — which is still a defensible product, stated plainly.

### P5 — Retire or revive the unadopted substrate (cleanup)

`composition_versions` has a full schema and five endpoints with zero rows.
`teams.mission` is populated once in 217. Either the tune-team RSI loop gets
driven by P0's attribution data, or this substrate should be marked
explicitly dormant so it stops reading as a shipped capability. Dead
machinery that looks alive is what made this audit necessary.

---

## 5. What not to do

**Do not add a new composition mechanism before P0.** There are already
thirteen, of which two work. The bottleneck is not expressiveness; it is
that eleven of them cannot be told whether they worked.

**Do not build a controlled vocabulary for `accepts`/`produces` as a
contract.** See P3. It would produce a tidier version of the same
unfalsifiable claim.

**Do not treat orchestra membership as composition.** It is a roster — it
answers "who is eligible", not "how do they combine". Conflating the two is
how `fermi_contract` came to be mistaken for membership (SPEC_29).

---

## 6. The one-line summary

Composition works exactly where something can score it. Everything else in
the composition surface is a declaration waiting for an operator — and the
mechanism that would score them is already written, correct, and one
migration away from running.

---

## 7. Method note

Every figure here came from querying production or the current tree, and two
of the audit's conclusions changed as a result:

* The design document's central mechanism (strategist-coordinated
  compositions) has **1 of 217** adoption, while the mechanism carrying all
  real composition (driver binding) **is not in the design document at all**.
  Auditing intent rather than data would have inverted the roadmap.
* Counterfactual attribution first read as "built and producing nothing",
  which would have sent someone debugging a working system. It is waiting on
  a table that exists on disk but not in production.

The recurring lesson across SPEC_28 through SPEC_31 is the same: this
codebase's failures are rarely missing machinery. They are machinery whose
silence is indistinguishable from success — an env fallback that quietly
billed the wrong account, a backfill that quietly re-granted revoked
memberships, a rank that quietly named every agent uniquely, and here an
attribution pass that quietly finds nothing to attribute.
