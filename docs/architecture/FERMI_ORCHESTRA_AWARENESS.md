# Fermi orchestra awareness

**Status:** Complete — `DomainRoster` injection shipped; hardcoded orchestra
removed from fermi's static prompt; `get_agent_calibration` declared on
fermi's card; `routing_decision` output field required.
**Date:** 2026-09-08
**Related:** `docs/architecture/META_AGENT_FLEET_AWARENESS.md` — xaman_ek's
             fleet-awareness pattern (the sibling problem),
             `docs/specs/SPEC_29_ORCHESTRA_MEMBERSHIP_AS_GOVERNED_STATE.md` —
             governance model,
             `RELEASE_NOTES_v0.11.2.md` — original orchestra infrastructure,
             `src/handlers/orchestras.rs` — injection implementation,
             `src/calibration.rs` — Loop 5 calibration measurement

---

## The problem

`fermi` is a domain-constrained MoE orchestrator for probabilistic forecasting.
It decomposes forecast questions, assigns each driver a specialist research
agent, and synthesises their findings into a calibrated probability estimate.

Doing that job requires fermi to know its orchestra: which agents exist, what
domain each covers, and how well each is calibrated.

The original implementation answered this by placing the entire orchestra
directly in fermi's system prompt — eight agent descriptions, 3,592 characters.
This produced the same three defects that xaman_ek's roster produced, adapted
to fermi's narrower scope:

1. **New members required a prompt edit.** The admission process
   (`fermi_contract` → `orchestra_members` grant) was correct governance, but
   it did not take effect until someone also edited the prose. Two sources of
   truth, guaranteed to diverge.
2. **Calibration was invisible.** The prompt described each agent's domain
   expertise as static text. There was no way for fermi to know whether an
   agent had a strong Brier score, was newly admitted, or had never had a
   forecast resolve. Routing was uninformed.
3. **Competition within a domain was unresolvable.** When two specialists
   covered the same domain, the prompt listed both but gave fermi no signal to
   choose between them. The selection was arbitrary, and nothing surfaced it to
   the analyst.

The design below eliminates all three.

---

## What already existed

| piece | description |
|---|---|
| `fermi_contract` field | Capability declaration on agent cards — can emit finding-labels + multipliers in fermi's aggregation format. Separate from membership. |
| `orchestra_members` table (mig-180, SPEC_29) | Governance grant table. Membership = a row here, not just `fermi_contract`. |
| `orchestra_fermi_members` view | The live roster. 12 agents at time of writing. |
| `inject_orchestra_context` in `orchestras.rs` | Appended a dynamic roster block to fermi's system prompt at execute time. Used `FullRoster` strategy: flat list of name + 140-char description. |
| `get_agent_calibration` platform tool | Already implemented in `src/agent_backend/tools/domains/observability.rs`, backed by `eval_signals` where `dimension = 'forecast_calibration'`. Loop 5.A. |
| 12 agents with `fermi_contract` | `macro_forecaster`, `market_research`, `sentiment_analyzer`, `entity_investigator`, `equity_analyst`, `biotech_analyst`, `nba_analyst`, `football_analyst`, `macro_data_agent`, `football_institution_agent`, `fixture_context_agent`, `weather_oracle`. All tagged `fermi-orchestra`. |

---

## Design decisions

### 1. Domain derivation — derive, don't add a field

Fermi's routing is domain-based. The obvious implementation is a new `domain`
field on the agent card. The chosen implementation derives the domain from
`metadata.tags` at injection time.

**Why derivation:** a `domain` field would be a third representation of
information already encoded in tags. It would need to stay in sync with tags
while serving a different consumer. Derivation keeps the card as the single
source; the injection layer is the policy.

**Rule:** the first tag that matches a known domain keyword becomes the
canonical domain. Tags that classify the agent's platform role rather than its
subject matter are skipped before matching.

Tags to skip before matching:
`fermi-orchestra`, `compound`, `forecasting`, and any tag matching `fleet:*`,
`factor-*`, or `world-*`.

Domain keyword mapping:

| keyword(s) | canonical domain |
|---|---|
| `macro-economics`, `macro`, `economics`, `gdp`, `inflation`, `policy`, `world-bank`, `country-data` | `macro` |
| `equity`, `finance`, `stocks`, `valuation`, `fmp` | `equity` |
| `biotech`, `pharma`, `clinical-trials`, `life-sciences`, `bioportal` | `biotech` |
| `sentiment`, `social-media`, `public-opinion` | `sentiment` |
| `market`, `competitive-analysis`, `tam-sizing`, `industry-trends` | `market` |
| `osint`, `investigation`, `entity-resolution`, `due-diligence` | `entity` |
| `nba`, `basketball` | `sports/basketball` |
| `football-institutions`, `football`, `soccer`, `fixture-context`, `host-advantage` | `sports/football` |
| `weather`, `prediction-markets` | `weather` |

Tags are matched in the order they appear on the card, skipping the meta tags
listed above. The first match wins.

### 2. Uncalibrated agents appear, but marked

Excluding uncalibrated agents would make the roster useless for new joiners
and would break the admission-to-activation path. The chosen policy is
inclusion with a visible status marker:

| signal count | marker in roster |
|---|---|
| 0 | `· uncalibrated` |
| 1–2 | `○ new (n=N)` |
| ≥ 3 | `✓ Brier X.XX n=N` |

`eval_signals` where `dimension = 'forecast_calibration'` is the source.
Signals accumulate as forecasts resolve. An agent admitted today begins as
`· uncalibrated` and progresses to `✓` without any prompt or card edit.

### 3. Competition within a domain — select best, surface all

When multiple admitted specialists share a domain, fermi must choose one to
call. The selection rule:

1. Call `get_agent_calibration` on each candidate.
2. Pick the agent with the highest Brier score among those with ≥ 3
   observations.
3. If no candidate has ≥ 3 observations, note the absence and default to the
   first listed.

All candidates — the winner and the bench — are recorded in the
`routing_decision` output field. The analyst sees who was considered and why
the winner was chosen. Opaque routing is not acceptable.

### 4. Separation of concerns — static methodology, dynamic roster

The previous prompt conflated two things that change at different rates: what
fermi *is* and who its orchestra *currently* contains. The separation:

**Static prompt (authored, version-controlled):**
- Fermi's identity and role
- Tetlock calibration principles
- Domain taxonomy — expressed as domain *names*, not agent IDs
- Output contract requirements, including `routing_decision`
- No agent names. No orchestra list.

**Dynamic injection (computed at execute time, `orchestras.rs`):**
- `DomainRoster` strategy: admitted specialists grouped by derived domain
- Calibration status per agent
- Clearly labeled `[static fallback]` if the DB query fails or returns empty
  (hardcoded 8-agent emergency list so fermi always has names to call)

The static prompt is stable across roster changes. The dynamic block is
regenerated on every execution from the live `orchestra_fermi_members` view.

**Static fallback:** the emergency list is deliberately named `[static
fallback]` in the injected block. Fermi can see it and communicate to the
analyst that its roster information may not reflect the current state.

### 5. `routing_decision` as a required output field

Routing transparency is an output contract requirement, not a soft preference.
Every driver assignment must include a structured `routing_decision`:

```
driver:     <driver name>
domain:     <canonical domain>
candidates: <agent_id> (Brier X.XX, n=N) | (uncalibrated) | (new, n=N)
            ...
selected:   <agent_id>
reason:     <why this agent was chosen>
```

This makes fermi's selections auditable without inspecting trace logs.

---

## What changed

| file | change |
|---|---|
| `src/handlers/orchestras.rs` | Added `DomainRoster` strategy variant; `primary_domain()` function that runs the keyword-matching rule; `FERMI_STATIC_FALLBACK` const; `build_domain_roster_block()` function; wired fermi's injection path to `DomainRoster` instead of `FullRoster`. |
| `agents/curated/fermi/agent_card.json` | Removed hardcoded `## Your Orchestra` section (8 agent descriptions, 3,592 characters); replaced with 5-line reference to dynamic injection. Rewrote `## Driver Taxonomy` to use domain names, not agent IDs. Added `## Routing Decision` required output section. Added `get_agent_calibration` to `mcp_tools`. |

---

## What this enables

**Automatic roster inclusion.** New orchestra members appear in fermi's
injected roster without any prompt edit. The admission path is the complete
path: declare `fermi_contract` → request → approve → `orchestra_members` grant.
The next execution picks them up.

**Calibration-driven selection.** As forecasts resolve, Brier scores accumulate
in `eval_signals`. Fermi's routing improves automatically — no code change,
no config change.

**ABW agents can join the pool.** The `fermi_contract` capability declaration
is the signal. Membership is the decision. Any agent that can emit
finding-labels and multipliers in fermi's aggregation format can be admitted.

**Transparent selection.** `routing_decision` makes the bench visible. The
analyst sees which agents were candidates, their calibration status, and why
the winning agent was selected.

**Dual-purpose specialists.** The same agents serve fermi as orchestra members
and serve ABW apps and other consumers directly. Fermi is not their only
caller. The agent card does not change based on who is calling — the
`fermi_contract` capability is passive metadata, not a behavioural mode.

---

## The broader pattern

Fermi and `simops_companion` (the SimOps domain-constrained MoE strategist)
are instances of the same pattern:

- **Static prompt** = methodology only (routing logic, output format, quality
  principles). No agent names.
- **Dynamic injection** = structured specialist roster, grouped by domain,
  with calibration status per agent.
- **Tool access** = `get_agent_calibration` for per-agent Brier detail on
  demand at routing time.
- **Output** = typed `routing_decision` showing all candidates, not just the
  winner.

This is the **domain-constrained MoE orchestrator pattern**. It is distinct
from the fleet-awareness pattern implemented for `xaman_ek`:

| | xaman_ek | fermi / simops_companion |
|---|---|---|
| Scope | Whole fleet (100+ agents) | Governed subset (12–30 agents) |
| Roster format | O(structure): categories + counts, no agent names | O(members): names + domains + calibration status |
| Agent names in prompt | No — tools answer per-agent questions | Yes — orchestrator calls `execute_agent` by name |
| Competition signal | Not yet (see roadmap) | Yes — `routing_decision` output field |
| Dynamic selection | No — static guidance + tools | Yes — Brier-based at routing time |
| Injection mechanism | `ExecutionContext::enrich_system_prompt` (registry digest) | `inject_orchestra_context` / `DomainRoster` strategy |

The two patterns serve different architectures. A navigator that answers
questions about 102 agents cannot enumerate them; an orchestrator that routes
to 12 admitted specialists must call them by name. The constraint is not
preference — `execute_agent` requires an ID.

---

## Roadmap

### Near-term (this sprint)
- [x] `DomainRoster` injection for fermi
- [x] `get_agent_calibration` declared on fermi's card
- [x] `routing_decision` required output field
- [x] Hardcoded orchestra removed from fermi's static prompt

### Medium-term
- [ ] **xaman_ek competition signal**: extend the `who_answers` tool to return
      per-agent comparison facts (cost, model tier, Brier where available) when
      a cohort has more than one member. Tracked in `XAMAN_COMPETITION_SIGNAL.md`
      (to be written).
- [ ] **`compare_agents([id_a, id_b])` platform tool**: takes two agent IDs,
      returns a side-by-side comparison (cost, Brier, accepts, produces, tier).
      Used by `xaman_ek` for composition recommendations; potentially also by
      fermi for multi-candidate domain slots.
- [ ] **`simops_companion` DomainRoster**: apply the same injection pattern to
      `simops_companion`. Its specialist pool (cascade, predictor, optimizer,
      narrator) is small and stable today but will grow.
- [ ] **Brier-based admission gate**: optional minimum Brier threshold for
      fermi's orchestra (e.g., ≥ 0.60 after n ≥ 10 observations). Currently
      uncalibrated agents are admitted and marked; the gate would require
      demonstrated performance for ongoing membership. Requires a policy decision
      before implementation.
- [ ] **`fermi` output contract**: typed contract for fermi's full decomposition
      output (drivers, routing_decisions, base_rate, confidence intervals).
      Currently only `routing_decision` is specified. Blocked on fermi emitting
      structured documents consistently.

### Long-term
- [ ] **Counterfactual Brier**: `fermi_forecasts.counterfactual_brier` column
      reserved in mig-172. Compute Team Brier − Counterfactual Brier to isolate
      the strategist's synthesis contribution versus individual specialist quality.
      This distinguishes a good orchestra from a good conductor.
- [ ] **Automatic roster refresh**: emit a roster-stale signal when
      `orchestra_members` changes so running sessions can re-inject without
      waiting for the next execution.
- [ ] **Cross-domain membership**: agents that span multiple domains (e.g.,
      `macro_data_agent` plausibly covers both `macro` and `entity`) should
      appear in both domain groups in the injected roster. Currently
      `primary_domain()` assigns one domain per agent. Cross-domain membership
      requires either a secondary-domain derivation pass or an explicit
      `fermi_domains` field — the derivation approach is preferred to avoid a
      third source of truth.
