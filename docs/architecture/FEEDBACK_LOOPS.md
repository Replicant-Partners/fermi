# Feedback Loops in the Agent Bestiary

**Date:** 2026-05-15, revised 2026-06-03  
**Status:** Reference — describes the five adaptive feedback loops as implemented, plus the BayesOps Loop A extension (specified, not yet built).

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

### Two classes of eval signal

The loops consume two structurally different kinds of eval signal, and the difference matters:

**LLM-judged signals** — scores produced by evaluators that use an LLM to assess output quality (LlmJudge, Faithfulness, Sotopia, etc.). These are fast and domain-general but inherit LLM non-determinism. They require the coherence gate in Loop 2 because a sufficiently adversarial or confused judge could produce a correction that damages the agent's world model.

**Hard-verified signals** — scores produced by deterministic comparison against ground truth that resolves independently of the agent's output. Brier score on resolved forecasts (Loop 5) and `projection_accuracy` on real SOSA observations vs. prior cascade projections (Loop 1, Spec 20) are both hard-verified. The scoring step has no LLM in it. The ground truth (market resolution, physical batch measurement) does not know or care what the agent predicted.

Hard-verified signals are epistemically stronger: they cannot be gamed by an agent that learns to produce plausible-sounding outputs, and they do not require a coherence gate before propagating into memory. When a real cultivation batch yields 3.8 kg against a predicted 4.2 kg, that delta is a fact. The semantic rule it produces ("this model overestimates yield at high temperature") is grounded in physical reality, not in an LLM's judgment of output quality.

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

**Correction path:**
```
Agent executes → episode stored → EvaluatorRegistry scores it
    → eval_signals, agent_timeline_entries written (inline, hot path)
    → ObservabilityWorker (background): PersonaDriftMonitor, AnomalyDetector
    → ConsolidationWorker (on-demand): DBSCAN cluster → semantic rules
                                        → ontology snapshot → dream_synopsis
    → KG context injected into next execution (kg_context.rs, every call)

For hard-verified signals (projection_accuracy):
    Real SOSA observation ingested
    → ProjectionScoringEvaluator: find prior synthetic projection
    → compute delta → write EvalSignal (dimension: "projection_accuracy")
    → same ConsolidationWorker path → semantic rules like:
       "kombucha_fermentation overestimates yield by ~15% when temp > 65°C"
    → injected into simops_dynamics_runner KG context on next execution
```

**What changes:** the agent's semantic memory — the rules, entities, and facts its system prompt is enriched with before each execution. The agent that has run 50 times on market analysis questions has accumulated domain-specific rules that make its 51st response qualitatively different from its first. For SimOps agents, hard-verified projection_accuracy scores produce physically grounded model-calibration rules with no LLM judgment in the scoring path.

**Timescale:** dreaming cycles for LLM-judged signals (hours to days). Hard-verified signals trigger consolidation as soon as a real observation arrives — potentially within the same session as the projection.

**Status:** fully closed and running for LLM-judged signals. `ProjectionScoringEvaluator` (hard-verified path) specified in Spec 20, ready to implement.

**See also:** `docs/specs/20_SIMOPS_PROJECTION_SCORING.md` for full data flow and implementation checklist.

---

### Loop 2 — Behavioral correction via HITL

**Target:** the agent's behaviour should align with human judgment, especially on high-stakes or anomalous cases.

**Signal:** human reviewer decisions — Approve, Relabel, Intervene — applied to anomaly events surfaced by Loop 1.

**Correction path:**
```
Anomaly detected (drift, conflict, rupture, safety) → anomaly_events
    → HITL review queue (/observatory/hitl)
    → Reviewer acts: Intervene
    → InterventionEncoder: validate, stamp authority_weight=1.0
    → CoherenceGate: check Γ(C) — does this correction cohere with
                     the agent's existing belief system?
    → TwoWriteMemory:
        Write 1: synthetic episode (SyntheticCorrection, authority=1.0)
        Write 2: episode_corrections (audit trail, coherence_check, minimum_update_set)
        AgentWide: bump_persona_version()
    → Synthetic episode enters Loop 1 → consolidated at HumanAuthority weight
    → New persona_version creates new drift baseline
```

**What changes:** the agent's persona — its effective belief system as encoded across its episodic memory. An agent-wide intervention marks a version boundary: before and after the correction, the agent's behaviour is measurably different (drift monitor will detect this). The correction is preserved in the immutable audit trail.

**Safeguards:** the coherence gate blocks corrections that would create incoherence in the agent's world model (Γ(C) < 0.5). Agent-wide corrections require a second independent reviewer.

**Timescale:** human-initiated, but the effect propagates in the next dreaming cycle.

**Status:** fully closed and running.

---

### Loop 3 — Workspace coherence

**Target:** a workspace's multi-agent conversation should produce coherent, evidence-grounded outputs without suppressing productive disagreement.

**Signal:** Γ(C) — the global coherence score from TEC settling — plus per-principle scores that distinguish productive incoherence (low P6 with high P4) from destructive incoherence (low P2, low P7).

**Correction path (inner — per session):**
```
Workspace messages accumulate
    → Auto-coherence evaluation every N messages (COHERENCE_AUTO_EVAL_INTERVAL)
      OR user triggers via Coherence shelf
    → TEC settling engine → Γ(C) + principle scores
    → cohere_and_coordinate invoked (Recommendations or Dream Notes tier):
        Stage 0: reads/updates intention map, feeds IntentionAligns/Conflicts
        Stages 1–3: diagnose → coordination brief → written to workspace
    → Agents read the coordination brief in their next turn context
```

**Correction path (outer — across sessions):**
```
cohere_and_coordinate accumulates session episodes in its own memory
    → Composition Dreaming (manual trigger): Stage 4 — Tension Audit
    → Valence homophily check: spread < 0.25 on arousal or valence axis?
    → Chronic destructive incoherence pattern detected?
    → propose_composition_change → composition_versions (pending)
    → Owner accepts/rejects (rejection stored as correction episode → Loop 1)
    → Accepted: teams.member_agent_ids updated, new team is live
```

**What changes:**
- Inner loop: the direction of the next few turns — agents receive a coordination brief that redirects them.
- Outer loop: the composition itself — team membership, member weights.

**Timescale:** inner loop runs within the session (minutes). Outer loop requires accumulated session history and human approval (days to weeks).

**Status:** inner loop fully closed. Outer loop structurally closed; operationally nascent — needs session history to accumulate before Stage 4 produces meaningful proposals.

---

### Loop 4 — Composition evolution (tune-team RSI)

**Target:** the composition's team structure should improve over time to reduce chronic coordination failures and valence homophily.

**Signal:** recurring patterns in cohere_and_coordinate's consolidated memory — which TEC principles are chronically weak, whether the team's valence distribution has collapsed, whether destructive incoherence is persistent.

**Correction path:**
```
cohere_and_coordinate sessions → episodes in strategist's memory
    → ConsolidationWorker: clusters session-observation episodes
                            → team-effectiveness rules in ontology
    → Composition Dreaming (POST /api/workspaces/:id/composition/dream):
        Read consolidated memory
        Compute valence spread: arousal max-min, valence max-min
        Classify chronic incoherence type
        If structural issue: call propose_composition_change
    → composition_versions row (status=pending)
    → Owner: Accept → teams updated | Reject + note → episode in strategist memory
    → Rejection feeds back into Loop 1 for the strategist
```

**What changes:** the team structure — who is in the composition, and with what weight.

**Timescale:** weeks to months. The strategist needs enough session history to distinguish persistent patterns from noise before making meaningful proposals.

**Status:** structurally closed. Data accumulation required before it becomes meaningful.

**Important distinction from Loop 3:** Loop 3's inner iteration changes conversation direction (fast, within-session). Loop 4 changes team composition (slow, across-sessions). They operate at different timescales and different levels of the system.

---

### Loop 5 — Calibration and routing accuracy

**Target:** the platform's agents should become more calibrated over time; the MoE routing strategist should learn which members are genuinely accurate on which sub-domains.

**Two signal paths feed this loop:**

**5a — Forecast calibration (Brier score)**
- Signal: Brier score when `fermi_forecasts` resolve against actual outcomes. Computed by `BrierEvaluator`, written to `eval_signals.dimension = "forecast_calibration"`.
- Timescale: months. Requires sufficient resolved forecasts to establish calibration curves.
- Ground truth source: market resolution, event outcomes — independent of the agent's prediction.

**5b — SimOps projection accuracy**
- Signal: `projection_accuracy` score when real SOSA observations arrive against prior cascade projections. Computed by `ProjectionScoringEvaluator` (Spec 20), written to `eval_signals.dimension = "projection_accuracy"`.
- Timescale: days to weeks, depending on batch cycle time. Ground truth arrives with every completed cultivation run — far faster than forecast resolution.
- Ground truth source: physical batch measurement — the batch does not know what was predicted.
- **Key difference from 5a:** this signal is available for SimOps agents even when no `fermi_forecasts` exist. It feeds Loop 1 directly (semantic rules about model calibration) and Loop 5 routing (which dynamics model to select for which process conditions).

**Current state of the signal paths:**
```
5a — Forecast calibration:
    Agent executes forecast question
    → BrierEvaluator reads fermi_forecasts filtered on agents_used
    → Computes 1 - brier_score → forecast_calibration dimension
    → Written to eval_signals
    → Appears in agent_timeline_entries.dim_scores
    → Observable in observatory trend charts
    
    moe_router_strategist routes query to member
    → Records routing decision as episode
    → BREAK: outcome annotation on routing episodes not yet wired

5b — Projection accuracy:
    Cascade projection runs → synthetic SOSA observation written
    → Real batch completes → operator enters SOSA observation
    → ProjectionScoringEvaluator: match projection → compute delta
    → EvalSignal (projection_accuracy) → ConsolidationWorker
    → Semantic rules in simops_dynamics_runner KG context
    → BREAK: routing weight update from projection_accuracy not yet wired
    Status: ProjectionScoringEvaluator specified (Spec 20), not yet implemented
```

**What is missing for full closure:**
- Routing episode outcome annotation (both 5a and 5b)
- `get_agent_calibration` tool on `moe_router_strategist` card (see §3)
- `ProjectionScoringEvaluator` implementation (Spec 20 checklist)

**Timescale:** 5a: months (forecast resolution cadence). 5b: days to weeks (batch cycle cadence).

**Status:**
- 5a: signal collected (`BrierEvaluator` + `BrierLookupSqlx` wired), routing episode annotation wired (`forecasts.rs:700`). **Loop closed.**
- 5b: `ProjectionScoringEvaluator` implemented and tested, `ProjectionLookupSqlx` implemented, evaluator registered in `EvaluatorRegistry`, `projection_accuracy` included in `get_agent_calibration` response, `projection_id` added to `CascadeProvenance`. **Loop closed pending migration 130 deployment and first real SOSA observation cycle.**

**See also:** `docs/specs/20_SIMOPS_PROJECTION_SCORING.md` for 5b implementation detail.

---

## 2. The hierarchy

The five loops operate at different timescales and different system levels:

```
Timescale    Loop                          Level              Status
────────────────────────────────────────────────────────────────────────────
Hours        1a. Individual learning        Single agent       ✅ Closed (LLM-judged)
Hours        1b. Projection accuracy        SimOps agents      🔧 Specified (Spec 20)
Days         2.  HITL correction            Single agent       ✅ Closed
Session      3a. Coherence (inner)          Composition chat   ✅ Closed
Weeks        3b. Coherence (outer)          Composition team   ⚡ Nascent
Months       4.  Composition evolution      Team structure     🔧 Structural
Days-weeks   5b. Projection calibration     SimOps routing     ✅ Implemented
Months+      5a. Brier calibration          Platform-wide      ✅ Closed
```

They are nested: Loop 2 feeds into Loop 1 (corrections become episodes). Loop 3's outer iteration feeds into Loop 4 (session patterns drive composition proposals). Loop 5, when closed, will feed into Loop 3 (calibration scores will weight the MoE routing classifier, which will influence which members are recommended in composition proposals).

---

## 3. Loop 5 — Closure Status (as of 2026-06-03)

Loop 5 is now closed. The four steps originally planned are all implemented:

| Step | Status | Where |
|---|---|---|
| Bootstrap calibration data (backtest seed) | ✅ `BrierLookupSqlx` wired to `fermi_forecasts` | `src/handlers/eval_brier.rs` |
| `GET /api/agents/:id/calibration` endpoint | ✅ Live — returns `calibration_score`, `trend`, `domain_calibration`, `projection_accuracy_mean`, `model_accuracy` | `src/handlers/agents.rs:1719` |
| `get_agent_calibration` tool on `moe_router_strategist` | ✅ On agent card, Stage 0 uses it | `agents/curated/moe_router_strategist/agent_card.json` |
| Routing episode outcome annotation | ✅ Fires on forecast resolution | `src/handlers/forecasts.rs:700` |

The cold-start progression holds as originally described:
- **Month 0–2:** routing based on `accepts`/`produces`/`skills` declarations (semantic matching)
- **Month 2–4:** routing weighted by historical accuracy as forecasts resolve and backtest data seeds
- **Month 4+:** routing is a calibrated probabilistic classifier

The value of the system increases monotonically with data. The architecture degrades gracefully to semantic matching at low data volume — that is the right design.

---

## 4. BayesOps — Loop A: Parameter Fitting (Planned)

**Status:** Specified (`docs/specs/14_BAYESOPS_SPEC.md`). Not yet implemented. Begins after Spec 20's `ProjectionScoringEvaluator` has accumulated a real observation cycle.

### What Loop A is and why it is not Loop 1–5

Loops 1–5 are all **harness-level changes**: they modify what context agents receive, how they are routed, and how their compositions are structured. They operate over agent episodes and produce semantic rules, coordination briefs, and routing weights.

Loop A is different in kind. It operates **upstream of Loop B** (the FPL Monte Carlo executor) and produces something the loops do not: **the distribution parameters themselves**.

```
Loop A (BayesOps — offline, per dataset):
  Historical SOSA observations
    → fit posterior distribution
    → FittedDistribution: Beta(9.4, 13.6) or Normal(4.8, 0.7)
    → written into FPL Driver as distribution parameters

Loop B (FPL executor — online, per forecast question):
  Driver yield: Beta(9.4, 13.6)   ← from Loop A, or from a human
    → Monte Carlo simulation (10,000 samples)
    → ExecutionResults: mean, p5, p95, Sobol indices
```

Loop B is entirely unchanged by BayesOps. The seam between Loop A and Loop B is the `Distribution` type in the FPL AST — `Beta`, `Normal`, `Lognormal`, `Triangular` — which already exists. Loop A produces those parameters from data rather than from human elicitation.

### How Loop A extends Loops 1 and 5

**Extends Loop 1 (agent learning):**

Loops 1 and 5 already accumulate `projection_accuracy` eval signals when real batches resolve against cascade projections (Spec 20). Those signals feed semantic rules into the agent's KG context — harness-level changes that tell the agent *which model is unreliable under which conditions*.

Loop A adds the complementary capability: given that an agent knows which model to use (Loop 1's semantic rules), BayesOps provides *calibrated distribution parameters for what that model predicts*. The two operate at different levels:

| | Mechanism | Output | Level |
|---|---|---|---|
| Loop 1 / Spec 20 | EvalSignal → consolidation → semantic rule | "Use bc_optimization at 30°C, not kombucha_fermentation" | Harness |
| Loop A / BayesOps | SOSA history → posterior fit → `Beta(α,β)` | "At 30°C, yield follows `Normal(4.8, 0.6)` based on 40 real runs" | Distribution parameters |

Together: Loop 1 tells the agent *what to run*; Loop A tells the FPL model *how to parameterise it*.

**Extends Loop 5 (calibration and routing):**

Loop 5 routes queries to agents based on measured calibration. BayesOps extends this in Phase 3 (what-if queries): the `ConditionalPosterior` produced by `posterior-reg` generates input sensitivity indices (which input drives outcome variance most?), scenario comparisons (A vs B), and probability-at-threshold queries (`P(yield ≥ 5.5 kg | lighting=135)`). These outputs are naturally scored by the same Brier/projection_accuracy infrastructure that Loop 5 already uses — the fitted model's predictions resolve against real batches, feeding Loop 5's routing classifier with evidence about which BayesOps model variant is most accurate for which process conditions.

### The planned loop structure with BayesOps

```
Timescale    Loop                          Level                    Status
─────────────────────────────────────────────────────────────────────────────────
Hours        1a. Individual learning        Single agent             ✅ Closed
Hours        1b. Projection accuracy        SimOps agents            ✅ Implemented
Days         2.  HITL correction            Single agent             ✅ Closed
Session      3a. Coherence (inner)          Composition chat         ✅ Closed
Weeks        3b. Coherence (outer)          Composition team         ⚡ Nascent
Months       4.  Composition evolution      Team structure           🔧 Structural
Days-weeks   5b. Projection calibration     SimOps routing           ✅ Implemented
Months+      5a. Brier calibration          Platform-wide            ✅ Closed
─────────────────────────────────────────────────────────────────────────────────
Offline      A.  BayesOps — parameter fit   FPL distribution params  🗓 Planned
             (feeds Loop B / FPL executor)
```

Loop A sits outside the online loop hierarchy because it is an offline computation triggered by data accumulation rather than an execution event. It is the layer that makes the inputs to Loop B data-driven rather than analyst-elicited.

### Phase gate

Loop A Phase 1 (`crates/posterior`, simple marginal fitting) begins after:
1. Migration 130 is deployed
2. `ProjectionScoringEvaluator` (Spec 20) has accumulated at least one real observation cycle
3. The real SOSA history is large enough to validate that `fit_marginal()` produces posteriors whose CI width meaningfully differs across agents and process conditions

See `docs/specs/14_BAYESOPS_SPEC.md §12` for the full sequencing, touch-count per phase, and validation gates.

---

## 5. What makes this architecture coherent

Each loop corrects at the appropriate timescale:
- Fast loops (1, 2, inner-3) handle execution-level errors — the agent said the wrong thing, the team went in the wrong direction.
- Slow loops (outer-3, 4) handle structural errors — the team is wrong for the problem, the composition needs to change.
- Calibration loop (5) handles systematic bias — the routing classifier has persistent blind spots that need data to reveal.
- Offline loop (A, planned) handles parameter bias — the distribution assumptions the forecasts run on are not grounded in operational data.

Each loop uses a different corrective mechanism:
- Loops 1 and 2: episodic memory → dreaming → semantic rules
- Loop 3: TEC coherence → coordination brief → conversation steering
- Loop 4: session patterns → composition proposals → human approval → team change
- Loop 5: calibration scores → routing weights → member selection
- Loop A: observation history → posterior fit → FPL distribution parameters

Each online loop (1–5) is separated from the others by a human or coherence gate:
- Loop 2 requires a human reviewer (anomaly → HITL queue)
- Loop 4 requires owner approval (composition proposal → accept/reject)
- Loop 5's routing weights are readable by humans via the calibration endpoint

Loop A (BayesOps) is separated from Loop B (FPL executor) by the operator: the fitted distribution parameters are written into FPL models explicitly, not injected automatically until Phase 5 ships `data_driven()`. This is intentional — parameter changes to forecast models should be reviewable before they affect production forecasts.

No online loop can modify agent behaviour without either a human gate or the coherence gate. Loop A cannot modify forecast behaviour without the operator accepting the fitted parameters. These properties compound: the system learns continuously at the harness level (Loops 1–5) while requiring human acceptance of parameter-level changes (Loop A). Fast adaptation where the cost of error is low; human review where the cost is high.
