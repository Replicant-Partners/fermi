# Native Rust Evaluator Family — Design Kickoff (Track B)

> Companion to [`OBSERVABILITY_IMPL.md`](./OBSERVABILITY_IMPL.md).
> Track B is the family of native-Rust evaluator implementations that
> plug into the registry trait introduced in Track A Phase 1.
>
> This document is **a discussion starter, not a finished spec.** It
> proposes per-evaluator shapes, captures open questions, and surfaces
> the design decisions that need to be made before implementation
> starts. Each evaluator can be designed and built in parallel after
> Track A Phase 1 lands.

## Why native Rust

The architecture doc names `Sotopia`, `LifelongBench`, `CharacterEval`,
`WildGuard`, and `Faithfulness` as the dimensional + pre-filter
evaluators. The published implementations of these benchmarks are
mostly Python and depend on heterogeneous toolchains (HuggingFace
datasets, pytest harnesses, `lm-eval`, etc.). For ABW we want:

1. **Deterministic deployment.** No Python sidecars, no per-evaluator
   environment setup. Every evaluator is a Rust crate that builds with
   the rest of the workspace.
2. **Loose coupling.** Each evaluator is independently versionable and
   replaceable. Track A's `EvalModel` trait is the only contract.
3. **Parallel work.** Five evaluators, five engineers (or five
   sessions). They share nothing but the trait.
4. **Transparent scoring.** Rule-based components are auditable;
   LLM-prompted components log their prompts and parsed outputs.

Native Rust does not mean "no LLM calls." Most evaluators will combine
deterministic pre-checks with LLM-based scoring. The point is that the
*orchestration*, *I/O*, *aggregation*, and *interfacing* are all in
the same Rust workspace.

## The shared contract (Track A Phase 1)

Every evaluator implements:

```rust
#[async_trait]
pub trait EvalModel: Send + Sync {
    /// Stable identifier — appears on `eval_signals.evaluator_name`.
    fn name(&self) -> &'static str;

    /// Pre-filter (cheap, runs first, can short-circuit) or
    /// dimensional (parallel, contributes to the aggregated signal).
    fn tier(&self) -> EvalTier;

    /// Dimensions this evaluator scores. The doc's mock shows e.g.
    /// Sotopia covers goal_completion, social_capital, rapport.
    fn dimensions(&self) -> &[Dimension];

    /// The actual scoring call. Inputs come from the `EpisodeBundle`
    /// (Phase 0), outputs feed the aggregator.
    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError>;
}
```

`EvalResult` carries `dimension_scores: HashMap<String, f64>` (each in
`[0.0, 1.0]`), optional flags, model identifier, latency, cost. Phase
1 specifies the exact struct.

## Per-evaluator first cut

Each section below proposes a starting design. Numbers and thresholds
are placeholders — they get tuned after the first runs against real
agent traffic.

### Sotopia — Social goals & capital (dimensional)

**Source benchmark:** Zhou et al. 2024, *Sotopia: Interactive Evaluation for Social Intelligence in Language Agents*.

**Dimensions covered:**
- `goal_completion` — did the agent achieve the social goal stated in
  the bundle's `goal_spec`?
- `social_capital` — did the interaction increase or decrease the
  agent's relational standing?
- `rapport` — affective tone and reciprocity within the exchange.

**Inputs from `EpisodeBundle`:**
- `transcript` (multi-turn required for meaningful rapport scoring)
- `goal_spec` (explicit social goal — Sotopia's defining feature)
- `agent_card` (system prompt informs goal interpretation)

**Proposed approach (hybrid):**

1. *Deterministic pre-checks.* Reject early when `goal_spec` is `None`
   — Sotopia is undefined without a goal. Reject when transcript has
   `< 2` turns. These are cheap signals to the registry that this
   evaluator does not apply.
2. *Structured LLM scoring.* The benchmark's published rubric becomes
   the prompt. Output is JSON with one score per dimension on a 1–10
   scale, normalized to `[0,1]`. Each dimension also gets a one-sentence
   rationale stored alongside the score.
3. *Calibration pass.* Phase 1 ships an unweighted scorer. Once we
   have ground-truth labels (HITL approvals/corrections), we fit a
   per-dimension correction polynomial. *(Open question: how many
   labeled episodes before this is meaningful? Probably 100+.)*

**Open questions:**
- Should `goal_completion` be partial-credit (continuous) or
  pass/fail? The original Sotopia paper uses continuous; pass/fail is
  easier to reason about for HITL.
- Where does the goal come from for ad-hoc agent executions that
  weren't initiated with an explicit social goal? Defer? Synthesize
  from the agent card's `produces` field?

### LifelongBench — Drift over sessions (dimensional)

**Source inspiration:** several "lifelong learning" benchmarks (LIBERO, LAMP, etc.); the architecture-doc tag is generic.

**Dimensions covered:**
- `persona_consistency` — does the agent's behaviour match earlier
  sessions with the same dyad / on the same topic?
- `retention` — does the agent recall facts established in prior
  episodes?

**Inputs from `EpisodeBundle`:**
- `agent_id`, `dyad_id`, `persona_version` (lookup keys)
- `transcript` (current behaviour to compare)
- Plus a *backward-pointing query* into `agent-bestiary-memory`
  episodic store — this evaluator is unique in needing read access
  beyond the bundle.

**Proposed approach (mostly deterministic):**

1. *Embedding-based persona drift.* Compute the embedding of the
   current bundle's response. Pull the mean embedding of the last `N`
   episodes at the same `persona_version`. Score `1.0 - cosine_distance`
   normalized to `[0,1]`. This reuses the existing `EmbeddingGenerator`
   trait — no new infrastructure.
2. *Retention probe.* Build a simple fact-retrieval check: extract
   declarative claims from the previous session(s), test whether the
   current transcript references them when a follow-up question is in
   scope. Phase-1 stub returns `None` for retention; Phase 2 wires the
   probe.
3. *Drift signal split.* Distinguish *desired* drift (the agent
   learning from feedback) from *undesired* drift (random walk).
   Desired drift correlates with persona_version bumps; undesired
   doesn't. Surface both, let the trend analyser decide.

**Open questions:**
- What's the minimum history required before LifelongBench produces a
  meaningful score? Probably 5+ prior episodes per dyad. Below that
  → return `None` and let the registry skip aggregation.
- Should this evaluator also flag *too little* drift (frozen-persona
  symptom)? The architecture doc's Plane C explicitly considers high
  variance as anomaly, so symmetric handling is consistent.

### CharacterEval — Persona fidelity (dimensional)

**Source inspiration:** persona-faithfulness benchmarks (RoleLLM, CharacterEval, Persona-Chat critiques).

**Dimensions covered:**
- `persona_fidelity` — does the agent stay in character per its
  system prompt and `agent_card`?
- `value_alignment` — do its responses align with the values implied
  by the agent's curated identity?

**Inputs from `EpisodeBundle`:**
- `agent_card.system_prompt` (the canonical persona definition)
- `agent_card.agent_type` (identity tag)
- `transcript` (actual behaviour)

**Proposed approach (mostly LLM):**

1. *System-prompt extraction.* Parse the system prompt to extract
   commitments: tone, allowed topics, claimed expertise, identity
   markers. This is regex-able for the curated agents (they use
   structured prompts) and LLM-extracted for community agents.
2. *Per-commitment compliance scoring.* For each commitment, ask the
   judge LLM whether the response honours it. Aggregate per dimension.
3. *Value alignment.* Compare value-laden language (trust markers,
   hedging, assertiveness) between the system prompt and the response.
   Cosine on a small fine-tuned embedding works; LLM Likert backup.

**Open questions:**
- How does this interact with intentional persona evolution? An agent
  that has *learned* via interventions is supposed to drift. This
  evaluator should respect `persona_version` bumps — it scores
  fidelity to the *current* persona baseline, not to v1.
- Per-tier rigour. Curated agents have well-formed prompts;
  community agents may have minimal prompts. Should this evaluator
  return `None` below some prompt-quality threshold?

### WildGuard — Safety pre-filter

**Source inspiration:** AllenAI WildGuard / OpenAI Moderation / Llama-Guard.

**Tier:** **Pre-filter** (runs first, can short-circuit the registry).

**Dimensions covered:**
- `safety` — binary unsafe/safe + harm category when unsafe.

**Inputs from `EpisodeBundle`:**
- `query`, `transcript` — both prompt and response are checked.
- `agent_card.agent_type` for context (some agents are deliberately
  forensic / safety-domain-adjacent and need looser thresholds).

**Proposed approach (hybrid):**

1. *Deterministic word/pattern filter.* Cheap, catches obvious
   policy-violation patterns. ~99% specificity, ~50% sensitivity —
   misses paraphrases.
2. *Small classifier model.* A finetuned classifier (or hosted
   moderation API) for the remaining cases. Output: harm category
   (e.g., violence, self-harm, illegal-instructions) + confidence.
3. *Output schema.* `safety` dimension with score `1.0 - p(unsafe)`;
   when unsafe, populate flags with `harm:<category>`.

**Open questions:**
- Local model vs. external API? Local removes a dependency but adds
  weights and ~200ms latency on cold start.
- Pre-filter short-circuit semantics: does an unsafe verdict skip the
  rest of the registry, or do dimensional evaluators still run for
  observability? *Suggest: dimensional still runs, aggregated signal
  is flagged, episode goes straight to HITL.*

### Faithfulness check — Pre-filter

**Source inspiration:** RAG faithfulness checks (FaithEval, RAGAs faithfulness, Selfcheck-GPT).

**Tier:** **Pre-filter**.

**Dimensions covered:**
- `grounding` — does the response cite, follow, or contradict the
  evidence the agent had access to?

**Inputs from `EpisodeBundle`:**
- `transcript` (the response being checked)
- `context` (the structured execution context — tool outputs, retrieved docs, etc.)
- `agent_card.agent_type` to scope what counts as "grounded" (a
  forecaster's grounding is different from an artist's)

**Proposed approach (mostly deterministic):**

1. *Claim extraction.* From the response, extract atomic factual
   claims. Use a small LLM call or rule-based sentence splitter.
2. *Source matching.* For each claim, check whether it can be
   matched against the bundle's `context` (tool outputs, RAG hits).
   Match → `supported`. Mismatch → `contradicted`. No match →
   `unsupported`.
3. *Score:* `supported / (supported + contradicted + unsupported)`.

**Open questions:**
- What about responses that synthesize across sources rather than
  quote them? Faithfulness benchmarks typically use entailment
  models; native Rust implementation likely calls an LLM.
- Does this apply uniformly? Some agents (e.g., creative ones) are
  not expected to be faithful to a context. The agent card needs an
  opt-in/opt-out flag — likely on `agent_card.capability_gates`.

## Cross-cutting design questions

These cut across all five evaluators and need answers before too much
implementation lands:

### Q-CC1 — LLM provider strategy
Each evaluator that uses an LLM needs to pick a model. Options:
- **(a)** Each evaluator hard-codes its own model.
- **(b)** Evaluator declares capability requirements; registry picks
  the model from the deployment config.
- **(c)** Use the existing `MultiModelExecutor` and `cognition_tier`
  machinery so eval-model choice obeys the same economy as agents.

(c) is most consistent with the rest of the platform.

### Q-CC2 — Cost budget per evaluation
Running five evaluators on every episode is expensive. We need:
- A per-agent budget per eval (mirrors `dreaming_budget_credits`)
- A skip rule: an evaluator that hasn't moved its score by `> ε` over
  the last `N` episodes is run on every `M`-th episode instead of
  every one
- Pre-filters always run; dimensional run subject to budget

### Q-CC3 — Calibration data
All five evaluators output `[0,1]` floats but each has its own scale
behaviour. After a few hundred labeled episodes from HITL we should
fit per-dimension calibrators. The registry should expose hooks for
this without each evaluator implementing it.

### Q-CC4 — Evaluator versioning
When we change a Sotopia prompt, that's a behaviour change. Eval
signals need to record the evaluator's *version* alongside its name so
the trend analyser can split before/after. Add `evaluator_version` to
the `EvalResult` struct in Phase 1.

### Q-CC5 — Bundles without enough context
Several evaluators need data that the bundle may not carry (Sotopia
needs `goal_spec`, LifelongBench needs prior episodes,
Faithfulness needs `context.tool_outputs`). The registry needs a
clean "this evaluator does not apply to this bundle" return path —
suggest `Result<Option<EvalResult>, EvalError>` or an explicit
`Inapplicable` variant on `EvalError`.

## Crate layout proposal

```
agent-bestiary/evaluators/
├── Cargo.toml
└── src/
    ├── lib.rs                  # trait, types, registry
    ├── registry.rs             # EvaluatorRegistry::run
    ├── aggregator.rs           # AggregatedSignal + conflict detection
    ├── tier.rs                 # EvalTier
    ├── error.rs                # EvalError
    └── prelude.rs              # re-exports

agent-bestiary/evaluator-sotopia/
├── Cargo.toml
└── src/lib.rs                  # impl EvalModel for SotopiaEvaluator

agent-bestiary/evaluator-lifelong/
└── ...

agent-bestiary/evaluator-character/
└── ...

agent-bestiary/evaluator-wildguard/
└── ...

agent-bestiary/evaluator-faithfulness/
└── ...
```

Each evaluator is a separate crate. The main `evaluators` crate has
no dependency on any individual evaluator — they're all wired in by
the application at startup. This is the loose-coupling guarantee.

## Phase 1 minimum bar (Track A blocker)

Before any Track B crate can be built:

1. `agent-bestiary/evaluators` crate exists with the trait, error,
   tier, and registry types.
2. Two reference implementations live somewhere (probably as
   sub-crates or in `agent-bestiary/evaluator-reference/`): an
   `LLMJudgeEvaluator` (refactor of `score_with_judge`) and a
   `BrierEvaluator` (thin read-only wrapper over the existing forecast
   resolver — D8). These prove the trait shape and serve as
   templates.
3. A unit test that constructs a fake `EpisodeBundle` and runs the
   registry against the two reference implementations.

That's it for Phase 1. The five Track B evaluators ship one at a
time after that, in any order.

## Open work — discussion items for the next session

Before implementing any Track B evaluator, please confirm or
correct:

- The five evaluator names and dimensions in the architecture-doc
  mock are the right ones to start with. Or do you want to subset
  (e.g., ship Sotopia + WildGuard first, defer the rest)?
- Cross-cutting Q-CC1 — provider strategy. Lean (c)?
- Cross-cutting Q-CC5 — `Inapplicable` return path semantics.
- Crate layout — one crate per evaluator vs. all evaluators inside
  one `evaluators` crate behind cargo features. The first is more
  loosely coupled; the second is fewer Cargo files.

After answering those, Track B can run in parallel sessions —
each evaluator is independently designed and built.
