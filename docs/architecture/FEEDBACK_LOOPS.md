# Feedback Loops in the Agent Bestiary

**Date:** 2026-05-15, revised 2026-06-03, **verified and revised 2026-08-15**
**Status:** Reference — describes the five adaptive feedback loops, their verified implementation state, and BayesOps Loop A (now shipped through Phase 3).
**Verified against:** `main` @ `67066e4a`, migrations through `199`.

---

## How this document was verified

The 2026-06-03 revision described the design and asserted status. This revision
walks each assertion back to code and records what was actually found. Three
kinds of correction were needed, and the third is the one worth internalising:

1. **Stale locations.** Line numbers had drifted by hundreds of lines. This
   revision cites **file + symbol name** rather than `file:line`, because
   symbols survive refactors and line numbers do not. Where a line number
   appears it is indicative only.
2. **Stale status.** Several things marked "specified, not yet implemented"
   have shipped — most significantly all of BayesOps Phases 1–3.
3. **Declared ≠ dispatched.** The previous revision counted a tool named on an
   agent card as a working tool. It is not. A card can declare a tool name that
   has no dispatch arm in `ToolRegistry::execute`
   (`src/agent_backend/tools_legacy.rs`); the name is advertised to the model,
   the model calls it, and it returns `Unknown tool: X`. The codebase now names
   this defect class a **phantom tool**. Two loops were documented as closed
   through a phantom tool. They are not closed.

A loop is only called closed here when every hop has an executing call site.

**Status markers used below:**

| Marker | Meaning |
|---|---|
| ✅ Closed | Every hop verified with an executing call site |
| ◐ Partial | Closed on one path, broken or absent on another; the break is named |
| ⚡ Nascent | Mechanism verified, insufficient data for it to do anything yet |
| 🔧 Structural | Mechanism exists; no signal reaches it |
| ✖ Broken | Documented as working, verified as not working |

---

## Framing

A feedback loop is *negative* in the control-systems sense when the output signal is fed back to reduce the error between current behaviour and desired behaviour. Every loop described here does this: it measures a deviation from some target (coherence, accuracy, persona fidelity, team composition, routing quality), and the loop corrects toward it.

The word "negative" here does not mean harmful — it means stabilising and self-correcting. A thermostat is a negative feedback loop. So is evolution. So, if these loops work as designed, is a well-run Agent Bestiary composition.

What makes these loops *adaptive* rather than merely reactive is that the correction changes the internal state of the agent or composition permanently, not just its behaviour on the next turn. The agent that dreamed last night reasons differently today. That is adaptation.

### What these loops do and do not change

A useful framing from the SIA paper (arXiv:2603.27766): *"Harness shapes how the agent searches; weight updates change what the model knows."*

All five loops described here are **harness-level changes**. They modify:
- What semantic rules, entities, and facts the agent's prompt is enriched with before each execution (Loop 1)
- Which anomalies a human reviewer sees and corrects (Loop 2)
- What coordination brief the agents read on the next turn (Loop 3)
- Who is in the composition (Loop 4)
- Which member the routing strategist selects (Loop 5)

None of these loops update model weights. They change the context, the configuration, and the routing — not the underlying model's parameters. This is the correct design for API-hosted models where weight updates are unavailable. It is also the correct design even when fine-tunable local models are available: harness changes are reversible, auditable, and human-gateable; weight updates are none of those things by default.

The quality ceiling question — whether harness-level accumulation of semantic rules reaches the same improvement ceiling as gradient descent — is empirically open. The architecture does not preclude weight updates for local models; the quality-weighted episode history produced by these loops is a direct prerequisite for any future fine-tuning path.

Loop A (BayesOps) is the one exception to the harness framing, and it is not a
weight update either — see §4.

### Two classes of eval signal

The loops consume two structurally different kinds of eval signal, and the difference matters:

**LLM-judged signals** — scores produced by evaluators that use an LLM to assess output quality (LlmJudge, Faithfulness, Sotopia, etc.). These are fast and domain-general but inherit LLM non-determinism. They require the coherence gate in Loop 2 because a sufficiently adversarial or confused judge could produce a correction that damages the agent's world model.

**Hard-verified signals** — scores produced by deterministic comparison against ground truth that resolves independently of the agent's output. Brier score on resolved forecasts (Loop 5) and `projection_accuracy` on real SOSA observations vs. prior cascade projections (Loop 1, Spec 20) are both hard-verified. The scoring step has no LLM in it. The ground truth (market resolution, physical batch measurement) does not know or care what the agent predicted.

Hard-verified signals are epistemically stronger: they cannot be gamed by an agent that learns to produce plausible-sounding outputs, and they do not require a coherence gate before propagating into memory. When a real cultivation batch yields 3.8 kg against a predicted 4.2 kg, that delta is a fact. The semantic rule it produces ("this model overestimates yield at high temperature") is grounded in physical reality, not in an LLM's judgment of output quality.

A third class has since been added, and it is not an eval signal at all:

**Provenance signals** — `stamp_invocation` (`src/api_server.rs`) records *how
an agent was asked and why it was chosen* as slugged, forgery-resistant episode
tags (`route:{reason}`, `qsrc:*`, `ibind:*`). These carry no quality judgment.
They exist so that the loops can separate "the agent was sent the wrong
question" from "the agent is bad at the job" — previously the same row. See
`crates/fermi-console/src/negotiate.rs` for the contract that produces them.

---

## The five loops

### Loop 1 — Individual agent learning

**Target:** the agent should reason correctly about its domain, using what it has learned from past executions.

**Signal:** eval dimension scores written to `eval_signals` per evaluator per episode. Two classes of signal feed this loop:

*LLM-judged dimensions* — relevance, accuracy, completeness, persona_fidelity, and similar scores produced by the EvaluatorRegistry (LlmJudge, Faithfulness, Sotopia, etc.). Fast, domain-general, inherently noisy.

*Hard-verified dimensions* — scores computed by deterministic comparison against ground truth that resolves independently of the agent's output:
- `forecast_calibration` (Brier score on resolved `fermi_forecasts`) — Loop 5 feeds this back into Loop 1 for forecasting agents
- `projection_accuracy` (SOSA observation delta: `1 - |predicted - actual| / |actual|`) — introduced in Spec 20 for `simops_dynamics_runner` and `simops_cascade` agents; computed by `ProjectionScoringEvaluator` when a real batch measurement arrives against a prior synthetic projection

Hard-verified signals require no coherence gate before consolidation. They are facts about the physical world, not judgments about output quality.

**Correction path (verified):**
```
Agent executes → episode stored
    ├─ EVAL-RUN PATH ONLY ──────────────────────────────────────────┐
    │  EvaluatorRegistry scores it        (handlers/eval.rs)        │
    │  → eval_signals                     (memory/src/store.rs)     │
    │  → agent_timeline_entries           (EpisodeScorer::write_inline)
    │  → ObservabilityWorker (spawned post-eval-run, not a daemon): │
    │       PersonaDriftMonitor  (observability/src/drift.rs)       │
    │       AnomalyDetector      (observability/src/anomaly.rs)     │
    └───────────────────────────────────────────────────────────────┘
    │
    ├─ ALL EXECUTIONS ──────────────────────────────────────────────┐
    │  ConsolidationWorker (on-demand, handlers/consolidation.rs):  │
    │     failure episodes  → DBSCAN cluster → semantic rules       │
    │     success episodes  → LLM knowledge-rule extraction         │
    │     → dream_synopsis  (UPDATE on latest ontology_snapshot)    │
    │  → KG context injected into next execution (kg_context.rs)    │
    └───────────────────────────────────────────────────────────────┘

For hard-verified signals (projection_accuracy):
    Real SOSA observation ingested (agent_backend/simops_tools.rs)
    → ProjectionScoringEvaluator: find prior synthetic projection
      via CascadeProvenance.projection_id (crates/simops/src/cascade_v2.rs)
    → compute delta → write EvalSignal (dimension: "projection_accuracy")
    → same ConsolidationWorker path → semantic rules like:
       "kombucha_fermentation overestimates yield by ~15% when temp > 65°C"
    → injected into simops_dynamics_runner KG context on next execution
```

**What changes:** the agent's semantic memory — the rules, entities, and facts its system prompt is enriched with before each execution. The agent that has run 50 times on market analysis questions has accumulated domain-specific rules that make its 51st response qualitatively different from its first. For SimOps agents, hard-verified projection_accuracy scores produce physically grounded model-calibration rules with no LLM judgment in the scoring path.

**Timescale:** dreaming cycles for LLM-judged signals (hours to days). Hard-verified signals trigger consolidation as soon as a real observation arrives — potentially within the same session as the projection.

**Status: ◐ Partial.** Two corrections to the previous revision:

- **The eval leg does not fire on live traffic.** The previous revision said
  eval signals and timeline entries are written "inline, hot path". They are
  written inline *within the eval pipeline*. `EpisodeScorer::write_inline` has
  exactly one call site, inside `run_eval_cases`. Live executions
  (`handlers/execution.rs`, `execution_stream.rs`, `workspace/messages.rs`)
  store an episode and inject KG context, but produce no eval signal and no
  timeline entry — so PersonaDriftMonitor and AnomalyDetector never see them.
  The code states this plainly: *"timeline entries are written only by the eval
  pipeline — no live conversation has ever produced one"*
  (`agent-bestiary/observability/src/worker.rs`). **This is the largest single
  gap in the loop architecture:** Loop 2 is fed by anomalies, anomalies are
  detected from timeline entries, and live conversation generates none.
- **`create_snapshot` is not on the API path.** `agent-bestiary/ontology/src/snapshot.rs::create_snapshot` is called only from the standalone CLI (`agent-bestiary/consolidate/src/main.rs`). The API dreaming path writes `UPDATE ontology_snapshots SET dream_synopsis = … WHERE snapshot_id = (latest for agent)`, which is a no-op for any agent whose snapshots were never created by the CLI.

The episode → consolidation → KG leg is genuinely closed and running, on all
executions. That is the leg doing the actual learning.

**Instrumentation added since:** `/api/observatory/loops/dreaming/maturity`
(`src/handlers/dreaming_maturity.rs`) classifies the "91 dreaming cycles, zero
entities, zero facts, zero rules" failure mode — a loop that runs and learns
nothing. Check it before asserting this loop is working for any given agent.

**See also:** `docs/specs/20_SIMOPS_PROJECTION_SCORING.md`; `docs/specs/21_PERFORMANCE_SPEC_2026-06-05.md` for the HNSW/ANN retrieval and `spawn_blocking` DBSCAN rework (behaviour-preserving).

---

### Loop 2 — Behavioral correction via HITL

**Target:** the agent's behaviour should align with human judgment, especially on high-stakes or anomalous cases.

**Signal:** human reviewer decisions — Approve, Relabel, Intervene — applied to anomaly events surfaced by Loop 1.

**Correction path (verified):**
```
Anomaly detected (Drift, RollingConflict, Rupture, Safety) → anomaly_events
    → HITL review queue (/observatory/hitl; handlers/observatory.rs)
    → Reviewer acts: Intervene
    → InterventionEncoder: validate, stamp authority_weight=1.0
                           (coherence-gate/src/encoder.rs)
    → CoherenceGate: check Γ(C) against DEFAULT_GATE_THRESHOLD = 0.5
                     (coherence-gate/src/gate.rs)
        · AgentWide scope → blocking
        · Episode / Dyad scope → "settler mode": advisory, never blocks
    → AgentWide only: second-reviewer consensus required
        POST /api/observatory/hitl/consensus/:request_id
        different user enforced (handlers/observatory.rs)
    → TwoWriteMemory (coherence-gate/src/two_write.rs):
        Write 1: synthetic episode (SyntheticCorrection, authority=1.0)
        Write 2: episode_corrections (audit trail, coherence_check,
                 minimum_update_set)
        AgentWide: bump_persona_version()
    → Synthetic episode enters Loop 1 → consolidated
    → New persona_version creates new drift baseline
```

**What changes:** the agent's persona — its effective belief system as encoded across its episodic memory. An agent-wide intervention marks a version boundary: before and after the correction, the agent's behaviour is measurably different (drift monitor will detect this). The correction is preserved in the immutable audit trail.

**Safeguards (verified, with two corrections):**
- The gate blocks only at `AgentWide` scope. `Episode` and `Dyad` corrections
  run advisory — the previous revision's unqualified "blocks corrections that
  would create incoherence" overstates this. It is a deliberate design choice
  (`gate.rs` settler-mode comment), not an oversight.
- The second-reviewer requirement for agent-wide corrections is real and
  enforced by user identity, not merely documented.

**Correction — the synthetic episode is not consolidated at elevated weight.**
The previous revision said the correction "enters Loop 1 → consolidated at
HumanAuthority weight". `TwoWriteMemory` does stamp `authority_weight = 1.0`,
but `ConsolidationWorker` never reads `authority_weight` — the only occurrence
in `consolidation.rs` is a test fixture. A human correction is consolidated as
an ordinary success episode. It is also written with `embedding: None`
(acknowledged in `two_write.rs`), so it cannot participate in DBSCAN clustering
at all.

**Timescale:** human-initiated, but the effect propagates in the next dreaming cycle.

**Status: ◐ Partial.** The HITL mechanism — queue, encoder, gate, two-write,
consensus, audit trail — is fully closed and verified. What is not closed is
the *weighting*: a human correction currently carries no more influence over
consolidation than any other successful episode. And upstream, Loop 1's live
traffic produces no anomalies to review (see Loop 1 status), so the queue is
fed only by eval runs.

---

### Loop 3 — Workspace coherence

**Target:** a workspace's multi-agent conversation should produce coherent, evidence-grounded outputs without suppressing productive disagreement.

**Signal:** Γ(C) — the global coherence score from TEC settling — plus per-principle scores (P1 Symmetry, P2 Explanation, P3 Analogy, P4 DataPriority, P5 Contradiction, P6 Competition, P7 Acceptability; `coherence-core/src/principles.rs`) that distinguish productive incoherence (low P6 with high P4) from destructive incoherence (low P2, low P7).

**Correction path (inner — per session), verified:**
```
Workspace messages accumulate
    → Auto-coherence evaluation every N messages
      (COHERENCE_AUTO_EVAL_INTERVAL, default 10; workspace/messages.rs)
      → ConversationObserver → SettlingEngine::with_defaults().settle
      → Γ(C) + principle scores → coherence_evaluations row
      → posts a `coherence_update` system message
      → STOPS HERE. Does not invoke the strategist.

    → User triggers via Coherence shelf at Recommendations or Dream Notes tier
      (workspace/coherence.rs)
      → cohere_and_coordinate executed via registry.execute_agent(...)
      → NO ToolContext is constructed, therefore NO tools are available
      → Stage 0 (intention map) and Stage 3 (write brief) cannot execute
```

**Correction path (outer — across sessions):**
```
cohere_and_coordinate accumulates session episodes in its own memory
    → Composition Dreaming (POST /api/workspaces/:id/composition/dream)
      → posts an @cohere_and_coordinate [COMPOSITION DREAMING — TENSION AUDIT]
        message (handlers/composition.rs), charges 5 credits
      → Stage 4 / valence-homophily threshold (spread < 0.25) exists as
        PROMPT TEXT ONLY. No Rust computes arousal or valence spread.
    → propose_composition_change → PHANTOM TOOL (see Loop 4)
```

**What changes:**
- Inner loop: the direction of the next few turns — agents receive a coherence
  update system message. They do **not** receive a coordination brief; see
  below.
- Outer loop: nothing yet, via this path. See Loop 4 for the path that works.

**Timescale:** inner loop runs within the session (minutes). Outer loop requires accumulated session history and human approval (days to weeks).

**Status: ◐ Partial (inner) / ✖ Broken (outer, via this path).** Three
corrections:

1. **Auto-eval does not invoke the strategist.** It stores the evaluation and
   posts a system message. Strategist invocation happens only on the
   user-triggered shelf path. The previous revision's `OR` in the diagram was
   actually an `only`.
2. **The strategist runs without tools.** The shelf path calls
   `registry.execute_agent` directly rather than going through
   `ToolAwareExecutor` with a `ToolContext` (contrast
   `handlers/execution.rs`). The 4-stage prompt in
   `agents/curated/cohere_and_coordinate/agent_card.json` is fully written, and
   no Rust code anywhere references `_coordination`, `intention_map`, or
   `brief.md`. Stages 0 and 3 are inert.
3. **Even a written brief would not be read.** Auto-injected workspace context
   loads only files under `context/` (`workspace/messages.rs`,
   `list_files(slug, Some("context"))`). `_coordination/brief.md` is outside
   that prefix. An agent could reach it only by calling `read_workspace_file`
   itself.

What *is* verified and working: Γ(C) measurement, per-principle scoring, the
auto-eval cadence, and the `coherence_update` message. The measurement half of
Loop 3 is closed. The correction half is not.

Coherence signal semantics changed after the previous revision — see
`754edd39` (relevance gating, uptake-based Symmetry, principle checks that can
actually fire) and `5a9f925c` (dyads / companion loop). The P1–P7 description
above reflects the current semantics.

---

### Loop 4 — Composition evolution (tune-team RSI)

**Target:** the composition's team structure should improve over time to reduce chronic coordination failures and redundant membership.

**Signal — this is what changed most since 2026-06-03.** The previous revision
described the signal as "recurring patterns in cohere_and_coordinate's
consolidated memory". That path never produced a single proposal. The module
that replaced it says so directly:

> `composition_versions` has had an accept/reject flow since mig-113, and the
> dashboard has always had a card for it. It permanently read "no pending
> evolution proposals" because **nothing ever generated one**. The loop was
> structurally complete and empty: a mechanism with no signal feeding it.
> — `src/handlers/composition_evolution.rs`

The signal now exists, and it is quantitative: **exact Shapley attribution**
(`src/attribution/counterfactual.rs`, migrations 187–188) computes per resolved
forecast:

- `forecast_agent_credit` — each agent's marginal contribution φ
- `forecast_agent_interactions` — whether each *pair* is synergistic or redundant

The pairwise interaction index is the load-bearing part. Marginal credit alone
cannot answer "who should be on this team" — an agent can be individually
valuable yet wholly redundant with a cheaper one.

**Correction path (verified):**
```
Resolved forecast → exact Shapley decomposition (src/attribution/)
    → forecast_agent_credit (φ) + forecast_agent_interactions
    → GET  /api/workspaces/:id/composition/suggestions
       (composition_suggestions_handler)
       · candidates: mean_credit < 0 AND n_forecasts >= 5
         (MIN_FORECASTS_FOR_PROPOSAL — suppressed below this, deliberately)
       · every proposal carries the sample size it rests on
    → POST /api/workspaces/:id/composition/suggestions/materialise
    → composition_versions row (accepted_by IS NULL AND rejected_by IS NULL)
    → Owner: Accept → memory/src/store.rs apply path  ⚠ SEE DEFECT BELOW
             Reject + note → episode in strategist memory
               (Provenance::HumanCorrected, authority_weight 1.0,
                tags: composition_rejection / dreaming_material)
    → Rejection feeds back into Loop 1 for the strategist  ✅ verified
```

**Why proposals are generated but not applied** — quoting the module, because
the reasoning is the design: attribution measures contribution *through the
current model*, so a negative φ can mean a weak agent, a mis-specified driver
exponent, or a genuinely predictive driver that is currently mis-weighted.
Automatic pruning would let a modelling error silently strip the roster.

**Status: ◐ Partial — generation ✅, accept path ✖ Broken.**

Two defects, both verified:

1. **`propose_composition_change` is a phantom tool.** It is declared *with a
   full `input_schema`* in `agents/curated/cohere_and_coordinate/agent_card.json`,
   and the composition-dreaming prompt instructs the agent to call it
   (`handlers/composition.rs`). There is no dispatch arm in
   `ToolRegistry::execute` and it is not in `builtin_tools()`. Card tools
   carrying a schema are advertised verbatim to the model, so the model *will*
   call it and receive `Unknown tool: propose_composition_change`. This is why
   the old path produced zero proposals. The Shapley path above bypasses the
   tool entirely and uses HTTP routes — which is why it works.
2. **The accept path writes to a column that does not exist.**
   `agent-bestiary/memory/src/store.rs` runs
   `UPDATE teams SET member_weights = $1 WHERE id = $2` **twice** — once bound
   to the roster array, once to the weights. `teams` has neither
   `member_agent_ids` nor `member_weights`; only `composition_versions` does
   (mig-113). The authoritative column list is `src/schema_trust.rs`. As
   written, accepting any version that carries members or weights will error,
   and the previous revision's claim that accept updates
   `teams.member_agent_ids` was never true.

So Loop 4 today: generates evidence-backed proposals, surfaces them, records
rejections into Loop 1 — and cannot apply an acceptance.

**Timescale:** weeks to months. `MIN_FORECASTS_FOR_PROPOSAL = 5` is a floor,
not a target; the loop is young and a confident proposal derived from two
correlated forecasts would be worse than no proposal.

**Important distinction from Loop 3:** Loop 3's inner iteration changes conversation direction (fast, within-session). Loop 4 changes team composition (slow, across-sessions). They operate at different timescales and different levels of the system.

**See also:** `docs/architecture/COMBINATORIAL_CREDIT_ASSIGNMENT.md`.

---

### Loop 5 — Calibration and routing accuracy

**Target:** the platform's agents should become more calibrated over time; the MoE routing strategist should learn which members are genuinely accurate on which sub-domains.

**Two signal paths feed this loop:**

**5a — Forecast calibration (Brier score)**
- Signal: Brier score when `fermi_forecasts` resolve against actual outcomes. Computed by `BrierEvaluator` (`handlers/eval_brier.rs`, `BrierLookupSqlx`), written to `eval_signals.dimension = "forecast_calibration"`.
- Timescale: months. Requires sufficient resolved forecasts to establish calibration curves.
- Ground truth source: market resolution, event outcomes — independent of the agent's prediction.

**5b — SimOps projection accuracy**
- Signal: `projection_accuracy` score when real SOSA observations arrive against prior cascade projections. Computed by `ProjectionScoringEvaluator` (`handlers/eval_projection.rs`, `ProjectionLookupSqlx`), written to `eval_signals.dimension = "projection_accuracy"`.
- Timescale: days to weeks, depending on batch cycle time. Ground truth arrives with every completed cultivation run — far faster than forecast resolution.
- Ground truth source: physical batch measurement — the batch does not know what was predicted.
- **Key difference from 5a:** this signal is available for SimOps agents even when no `fermi_forecasts` exist. It feeds Loop 1 directly (semantic rules about model calibration) and Loop 5 routing (which dynamics model to select for which process conditions).

**Verified state of the signal paths:**
```
5a — Forecast calibration:
    Agent executes forecast question
    → BrierEvaluator reads fermi_forecasts filtered on agents_used   ✅
    → Computes 1 - brier_score → forecast_calibration dimension      ✅
    → Written to eval_signals                                        ✅
    → resolve_forecast_handler (handlers/forecasts.rs) spawns:
        find episodes tagged `moe_routing_decision` in last 7 days
        → UPDATE episodes SET context with
          outcome_quality (= 1 - brier.clamp(0,1)), outcome_source
          ("brier_forecast"), outcome_brier_score, outcome_annotated_at  ✅
    → GET /api/agents/:id/calibration serves it                      ✅
    → moe_router_strategist Stage 0 calls get_agent_calibration
      → ✖ Unknown tool: get_agent_calibration

5b — Projection accuracy:
    Cascade projection runs → synthetic SOSA observation written,
      projection_id stamped via CascadeProvenance
      (crates/simops/src/cascade_v2.rs, agent_backend/simops_tools.rs)  ✅
    → Real batch completes → operator enters SOSA observation
    → ProjectionScoringEvaluator: match projection → compute delta     ✅
      (registered in EvaluatorRegistry, handlers/eval.rs)
    → EvalSignal (projection_accuracy) → ConsolidationWorker           ✅
    → surfaced in the calibration response as projection_accuracy_mean ✅
    → same phantom-tool break at the router                            ✖
    Migration 130 deployed long ago (repo is at 199).
```

**The break, stated plainly.** The previous revision marked Loop 5 closed. Every
*producer-side* claim in it holds: the evaluators are wired, the annotation
fires on resolution, the endpoint is live and returns all five documented
fields. But the consumer cannot read it.
`agents/curated/moe_router_strategist/agent_card.json` declares
`get_agent_calibration`; `ToolRegistry::execute` has no arm for it. The only
implementation is the HTTP route
`src/api_server.rs → handlers::agents::get_agent_calibration_handler`. Stage 0
calls the tool, gets `Unknown tool`, and the card's own cold-start fallback
("calibration data not yet available") makes the broken wire look like sparse
data. `debate_strategist` and `vote_strategist` carry the same declaration.

**Also worth correcting:** the doc's headline field, `calibration_score`, is the
one the handler's own doc-comment warns against — *"Gate 'is this loop closed?'
on skill, not on `calibration_score`, which is inflated by outcome-skewed
question sets."* Use `brier_skill_score`, which the previous revision omitted
entirely.

**What is missing for full closure:**
- A `"get_agent_calibration"` dispatch arm delegating to the existing handler
  (both 5a and 5b) — this is the whole break, and it is small
- Widening the phantom-tool regression test
  (`weather_agent_cards_declare_no_phantom_tools`, `agent_backend/weather_tools.rs`)
  from its four hardcoded weather agents to all of `agents/curated`. A corpus
  scan found **27 curated cards declaring undispatchable tools.**

**Timescale:** 5a: months (forecast resolution cadence). 5b: days to weeks (batch cycle cadence).

**Status:**
- 5a: ◐ Partial — signal collection, outcome annotation, and endpoint all ✅; router read path ✖ phantom tool.
- 5b: ◐ Partial — full evaluator chain ✅ and deployed; same router read path ✖; awaiting a first real SOSA observation cycle for operational evidence.

**See also:** `docs/specs/20_SIMOPS_PROJECTION_SCORING.md` for 5b implementation detail.

---

## 2. The hierarchy

The five loops operate at different timescales and different system levels:

```
Timescale    Loop                          Level              Status
────────────────────────────────────────────────────────────────────────────────
Hours        1a. Individual learning        Single agent       ◐ KG leg closed;
                                                                 eval leg eval-runs only
Hours        1b. Projection accuracy        SimOps agents      ✅ Closed, awaiting data
Days         2.  HITL correction            Single agent       ◐ Mechanism closed;
                                                                 authority weight unread
Session      3a. Coherence (inner)          Composition chat   ◐ Measurement closed;
                                                                 brief path inert
Weeks        3b. Coherence (outer)          Composition team   ✖ Phantom tool
Months       4.  Composition evolution      Team structure     ◐ Shapley generation ✅;
                                                                 accept path broken
Days-weeks   5b. Projection calibration     SimOps routing     ◐ Producer ✅; router ✖
Months+      5a. Brier calibration          Platform-wide      ◐ Producer ✅; router ✖
────────────────────────────────────────────────────────────────────────────────
Offline      A.  BayesOps — parameter fit   FPL distributions  ✅ Phases 1–3 shipped
             (feeds Loop B / FPL executor)
```

They are nested, and two of the nestings are now real rather than aspirational:
Loop 2 feeds into Loop 1 (corrections become episodes — verified, though
unweighted). **Loop 5 now feeds Loop 4**: Shapley attribution over resolved
forecasts is what generates composition proposals — this is the connection the
previous revision listed as future work, and it is the single most important
thing that closed since. Loop 3's outer iteration was *supposed* to feed Loop 4
and never did; the Shapley path replaced it.

---

## 3. Loop 5 — Closure Status (revised 2026-08-15)

The four steps of the original plan, re-verified:

| Step | Status | Where |
|---|---|---|
| Bootstrap calibration data (backtest seed) | ✅ `BrierLookupSqlx` wired to `fermi_forecasts` | `src/handlers/eval_brier.rs` |
| `GET /api/agents/:id/calibration` endpoint | ✅ Live — `calibration_score`, `brier_skill_score`, `trend`, `domain_calibration`, `projection_accuracy_mean`, `model_accuracy` | route in `src/api_server.rs` → `handlers::agents::get_agent_calibration_handler` |
| `get_agent_calibration` tool on `moe_router_strategist` | ✖ **Declared, not dispatched** — phantom tool | card: `agents/curated/moe_router_strategist/agent_card.json`; missing arm: `src/agent_backend/tools_legacy.rs::ToolRegistry::execute` |
| Routing episode outcome annotation | ✅ Fires on forecast resolution | `src/handlers/forecasts.rs::resolve_forecast_handler` |

**Loop 5 is three-quarters closed with a one-function gap at the consumer end.**

### New since 2026-06-03: routing moved to a measured substrate

The previous revision modelled Loop 5 as *endpoint + episode annotation*. Three
changes since have made routing itself measurable:

**a) Route provenance on every episode** (`7b768a08`). `stamp_invocation`
(`src/api_server.rs`) writes caller-supplied invocation records as slugged
episode tags — `route:{reason}`, `route:fallback`, `qsrc:*`, `ibind:*`. Values
are slugged (≤64 chars, restricted charset) so a caller cannot forge
`status:success`. Contract in `crates/fermi-console/src/negotiate.rs`;
`bind_input` separately checks whether the agent even declares a free-text
input.

**b) Agents declare the domains they serve** (`67066e4a`). `AgentContract`
gained `domains` and `domains_explicit`, parsed from `metadata.domains` with a
`metadata.tags` fallback. `RouteReason::DeclaredSpecialist` is evaluated
*ahead of* the hardcoded table in `routing.rs`. An explicitly empty
`domains: []` is a meaningful opt-out.

This fixed a live failure worth recording: `routing::domain_specialist` is a
`match` over four domains that omitted `climate`, so every weather driver fell
through to `macro_forecaster` — London 32 °C returned 0.3 % against a 13.3 %
ensemble truth, and the divergence panel presented the gap as a trading signal.
A new domain now needs a card edit, not a release.

**c) Routing decisions joined to realised outcomes in SQL**
(`migrations/193_route_provenance_outcomes.sql`), five views:

| View | Answers |
|---|---|
| `route_outcomes` | Per-run join: route provenance → Brier + signed Shapley credit |
| `route_reason_performance` | Does a routing reason beat `default`, per domain? |
| `domain_agent_ranking` | Measured replacement for `domain_specialist()` |
| `router_override_scorecard` | Was overruling Fermi's suggestion right? |
| `declaration_quality_outcomes` | Do richer contracts produce better outcomes? |

Headline metric is `avg_shapley` — per-agent and signed, so unconfounded by
forecast difficulty in the way a raw Brier average is.

**Known weakness, carried deliberately:** `episodes` and
`forecast_agent_claims` share no correlation id, so `route_outcomes` joins
heuristically on `(agent_id, driver)` within −2 min/+10 min. It can **miss**
when an agent is invoked twice on the same driver in-window; it **cannot
mis-attribute** across agents or drivers. The fix — stamping `episode_id` onto
the claim row in the multiplier hook — is deferred.

### The cold-start progression

Holds as originally described, and the substrate for stage three now exists:
- **Month 0–2:** routing on `accepts`/`produces`/`skills`/`domains` declarations (semantic matching)
- **Month 2–4:** routing weighted by historical accuracy as forecasts resolve
- **Month 4+:** routing as a calibrated probabilistic classifier, with `domain_agent_ranking` replacing the hardcoded table

The value of the system increases monotonically with data. The architecture degrades gracefully to semantic matching at low data volume — that is the right design.

---

## 4. BayesOps — Loop A: Parameter Fitting (**shipped**)

**Status:** Phases 1–3 shipped 2026-06-16. Phase 4 not built. Phase 5 shipped
in a different shape than specified. The previous revision's "specified, not
yet implemented" and the "zero implementation" note in
`docs/specs/14_BAYESOPS_SPEC.md §12` are both stale; `docs/fermi/BAYESOPS_CONTRACT.md`
and `docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md` are current.

### What Loop A is and why it is not Loop 1–5

Loops 1–5 are all **harness-level changes**: they modify what context agents receive, how they are routed, and how their compositions are structured. They operate over agent episodes and produce semantic rules, coordination briefs, and routing weights.

Loop A is different in kind. It operates **upstream of Loop B** (the FPL Monte Carlo executor) and produces something the loops do not: **the distribution parameters themselves**.

```
Loop A (BayesOps — offline, per dataset):
  Historical observations
    → fit posterior distribution
    → FittedDistribution: Beta(9.4, 13.6) or Normal(4.8, 0.7)
    → written into FPL Driver as distribution parameters

Loop B (FPL executor — online, per forecast question):
  Driver yield: Beta(9.4, 13.6)   ← from Loop A, or from a human
    → Monte Carlo simulation (10,000 samples)
    → ExecutionResults: mean, p5, p95, Sobol indices
```

Loop B is entirely unchanged by BayesOps. The seam between Loop A and Loop B is the `Distribution` type in the FPL AST — `Beta`, `Normal`, `Lognormal`, `Triangular` — which already exists. Loop A produces those parameters from data rather than from human elicitation.

### Phase status (verified)

| Phase | Deliverable | Status | Evidence |
|---|---|---|---|
| **1** | `crates/posterior` marginal fitting | ✅ Shipped | `fit_marginal`, `FittedDistribution`, `to_fpl_params`, `FitMetadata`, `DataQuality::classify`, `DistFamily`; families in `beta.rs`/`normal.rs`/`lognormal.rs`/`triangular.rs`/`auto.rs`; `bootstrap_ci` |
| **2** | `crates/posterior-reg` HMC conditional fitting | ✅ Shipped, one model | `fit_conditional`, `ConditionalPosterior`, `RegressionConfig`, NUTS 4-chain via `spawn_blocking` (`sampler.rs`), R-hat/ESS (`diagnostics.rs`). **Gap:** only `LinearNormal` exists, so the spec's "selects StudentT when outliers injected" gate is unmet by construction |
| **3** | Four what-if query methods | ✅ Shipped | `whatif.rs`: `predict`, `input_sensitivity` (Saltelli pick-freeze), `compare_scenarios`, `prob_exceeds`, `optimise_for_target`. **Gap:** `HeteroscedasticNormal`, `NonlinearNormal` not built |
| **4** | SimOps `PredictorEngine::Conditional` behind a `bayesian` feature | ✖ Not found | No posterior dependency in `crates/simops/Cargo.toml`; no `PredictorEngine`, no `bayesian` feature |
| **5** | `data_driven()` in parser/AST/executor + posterior store + refit trigger | ◐ Superseded in shape | No `data_driven()` anywhere. Equivalent capability shipped as `learnable: true` + `feeds_from` (`src/ast.rs`, `src/parser.rs`), resolved in `src/executor.rs` (`fitted_distribution_for` → `LearnableSource::{Fitted, PriorFallback, Static}`, logged in `ExecutionResults.learnable_drivers`) |

**Tests:** `cargo test -p posterior -p posterior-reg` — 62 + 39 unit, 6
end-to-end (`recovers_known_linear_posterior`, `prob_exceeds_is_calibrated`,
`compare_scenarios_identifies_winner`, `optimise_for_target_finds_higher_x`,
and two more), 2 doc-tests. All pass.

**Surfaces:** all seven operations exposed over both MCP
(`src/bin/agent-mcp-server.rs`: `fermi_fit_marginal`, `fermi_fit_conditional`,
`fermi_predict`, `fermi_input_sensitivity`, `fermi_compare_scenarios`,
`fermi_prob_exceeds`, `fermi_optimise_for_target`) and HTTP
(`src/handlers/bayesops.rs`, ~900 lines, plus posterior cache list/evict,
workspace state, pending accept/reject, manual refit).

### What actually feeds Loop A today

**Not SOSA.** There is no wiring from `sosa_observations` into `fit_marginal`.
The live feed is **workspace resolutions** (Spec 23, R-1):

```
Workspace resolution committed
  → post-commit hook (handlers/workspace/resolution.rs)
  → refit_workspace (handlers/workspace/refit.rs)
      collect: feeds_from.source == "upstream_resolutions"
               → registered Extractor (crates/posterior/src/extractors.rs)
               + workspace_outputs
      fit:     fit_marginal
      gate:    Monte Carlo impact gate
      apply:   auto-accept → write_fitted_params (params.<driver>_fitted)
               otherwise   → stage a pending row for human review
  → persisted: bayesops_posterior_snapshots / bayesops_pending_fits
               (migrations/148_bayesops_refit_ledger.sql)
```

Manual trigger: `refit_workspace_handler`. Conditional posteriors are held in a
`DashMap` cache only — persistent conditional storage remains unbuilt.
`harness_snapshots.bayesops_params` is still written null in
`handlers/forecasts.rs`, though `forecast_benchmark.rs` accepts and hashes the
column.

### How Loop A relates to Loops 1 and 5

**Extends Loop 1 (agent learning):** Loops 1 and 5 accumulate
`projection_accuracy` eval signals when real batches resolve against cascade
projections (Spec 20). Those signals feed semantic rules into the agent's KG
context — harness-level changes that tell the agent *which model is unreliable
under which conditions*. Loop A adds the complementary capability: given that
an agent knows which model to use, BayesOps provides *calibrated distribution
parameters for what that model predicts*.

| | Mechanism | Output | Level |
|---|---|---|---|
| Loop 1 / Spec 20 | EvalSignal → consolidation → semantic rule | "Use bc_optimization at 30 °C, not kombucha_fermentation" | Harness |
| Loop A / BayesOps | Observation history → posterior fit → `Beta(α,β)` | "At 30 °C, yield follows `Normal(4.8, 0.6)` based on 40 real runs" | Distribution parameters |

Together: Loop 1 tells the agent *what to run*; Loop A tells the FPL model *how to parameterise it*.

**Extends Loop 5 (calibration and routing):** the `ConditionalPosterior`
produced by `posterior-reg` generates input sensitivity indices, scenario
comparisons, and probability-at-threshold queries
(`P(yield ≥ 5.5 kg | lighting = 135)`). These are scored by the same
Brier/projection_accuracy infrastructure Loop 5 already uses — the fitted
model's predictions resolve against real outcomes, feeding evidence about which
BayesOps model variant is most accurate for which conditions. With only
`LinearNormal` implemented there is currently one variant to choose between, so
this is capability-in-waiting rather than an operating loop.

### Remaining Loop A work

1. Additional regression models (`StudentT`, `HeteroscedasticNormal`,
   `NonlinearNormal`) — without them the improvement ladder in
   `improvement.rs` is a one-element walk and model selection cannot be
   validated
2. Phase 4: SimOps `PredictorEngine::Conditional`
3. Persistent storage for conditional posteriors (currently cache-only)
4. A SOSA-history feed into `fit_marginal`, which is what the original spec
   framed the loop around
5. Populate `harness_snapshots.bayesops_params`

See `docs/specs/14_BAYESOPS_SPEC.md §12` (sequencing — note the phase-status
lines there are stale), `docs/fermi/BAYESOPS_CONTRACT.md`, and
`docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md`.

---

## 5. Loop instrumentation

The previous revision had no notion of measuring the loops themselves. Three
instruments now exist, and they should be consulted before any claim that a
loop is working:

| Instrument | What it answers | Where |
|---|---|---|
| `GET /api/me/loop-health` | Live per-loop health aggregation, Loops 1–5 | `src/api_server.rs` → `handlers::agents::loop_health_handler` |
| `GET /api/observatory/loops/dreaming/maturity` | Is Loop 1 running-but-learning-nothing? (the "91 cycles, zero rules" mode) | `src/handlers/dreaming_maturity.rs` |
| `agent_evolution` ledger | Four un-averaged progression dimensions — `memory` (Loop 1), `judgment` (Loop 5), `conduct` (Loop 2), `craft` — with a `peak_level` ratchet so regression is measurable | `migrations/190_agent_evolution.sql`, `src/handlers/evolution.rs` |

The dimensions are deliberately not averaged into a single score, and
`agent_evolution` deliberately replaced an activity-based maturity metric that
was measuring nothing but usage.

Diagnostic scripts: `scripts/run_loop5_probe.sh`, `scripts/loop1_*.sql`,
`scripts/loop_deploy_check.sql`.

---

## 6. Open breaks — consolidated

Every verified break, ordered by cost-to-fix against value:

| # | Break | Loop | Fix size |
|---|---|---|---|
| 1 | `get_agent_calibration` has no dispatch arm; router Stage 0 gets `Unknown tool` | 5a, 5b | One match arm delegating to the existing handler |
| 2 | `propose_composition_change` has no dispatch arm | 3b, 4 | One match arm, or delete the declaration since the Shapley path supersedes it |
| 3 | Composition accept path writes `teams.member_weights`, a column that does not exist | 4 | Correct the target table in `memory/src/store.rs` |
| 4 | Phantom-tool regression test covers only 4 weather agents; 27 curated cards declare undispatchable tools | all | Widen `weather_agent_cards_declare_no_phantom_tools` to all of `agents/curated` |
| 5 | Live executions write no eval signal / timeline entry, so drift and anomaly detection never see real traffic | 1, 2 | Non-trivial — needs a scoring path off the hot path |
| 6 | `ConsolidationWorker` never reads `authority_weight`; human corrections consolidate as ordinary episodes | 2 | Weight the extractor; also give synthetic corrections an embedding |
| 7 | Coherence shelf executes the strategist without a `ToolContext`; Stages 0 and 3 are inert | 3a | Route the shelf through `ToolAwareExecutor` |
| 8 | `_coordination/brief.md` sits outside the `context/` prefix that workspace auto-injection reads | 3a | Either move the brief or widen the prefix |
| 9 | `create_snapshot` reachable only from the CLI; the API `dream_synopsis` update is a no-op without it | 1 | Call it on the API dreaming path |
| 10 | Valence-homophily threshold (spread < 0.25) exists only as prompt text | 3b | Compute it, or stop documenting it as a mechanism |
| 11 | `route_outcomes` joins heuristically on `(agent_id, driver)` within a time window | 5 | Stamp `episode_id` onto the claim row (deliberately deferred) |

Breaks 1–4 are the phantom-tool family. They share a root cause — declaration
was never checked against dispatch for filesystem cards — and
`invalid_tool_declarations` (`tools_legacy.rs`) already exists to catch them; it
simply only runs on the DB agent-update path.

---

## 7. What makes this architecture coherent

Each loop corrects at the appropriate timescale:
- Fast loops (1, 2, inner-3) handle execution-level errors — the agent said the wrong thing, the team went in the wrong direction.
- Slow loops (outer-3, 4) handle structural errors — the team is wrong for the problem, the composition needs to change.
- Calibration loop (5) handles systematic bias — the routing classifier has persistent blind spots that need data to reveal.
- Offline loop (A) handles parameter bias — the distribution assumptions the forecasts run on are not grounded in operational data.

Each loop uses a different corrective mechanism:
- Loops 1 and 2: episodic memory → dreaming → semantic rules
- Loop 3: TEC coherence → coordination brief → conversation steering
- Loop 4: Shapley attribution → composition proposals → human approval → team change
- Loop 5: calibration scores + route provenance → routing weights → member selection
- Loop A: observation history → posterior fit → FPL distribution parameters

Each online loop (1–5) is separated from the others by a human or coherence gate:
- Loop 2 requires a human reviewer (anomaly → HITL queue), and a second reviewer for agent-wide scope
- Loop 4 requires owner approval (composition proposal → accept/reject), and proposals are suppressed below 5 forecasts of evidence
- Loop 5's routing weights are readable by humans via the calibration endpoint

Loop A is separated from Loop B by the operator: fitted parameters pass a Monte
Carlo impact gate and either auto-accept or stage a pending row for review.
Parameter changes to forecast models are reviewable before they affect
production forecasts.

No online loop can modify agent behaviour without either a human gate or the coherence gate. Loop A cannot modify forecast behaviour without passing the impact gate. These properties compound: the system learns continuously at the harness level (Loops 1–5) while requiring human acceptance of parameter-level changes (Loop A). Fast adaptation where the cost of error is low; human review where the cost is high.

**A closing note on this revision.** The architecture is sound and most of it is
built. What the 2026-06-03 revision got wrong was not the design — it was
mistaking a declaration for an implementation, in a system where declarations
are cheap and look exactly like implementations from the outside. Two loops
were reported closed through a tool that returns `Unknown tool`. The remedy is
structural, not editorial: break #4 above makes the class of error impossible
to reintroduce silently, and §5's instruments make a loop that runs without
learning visible as such.
