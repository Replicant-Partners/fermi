# Learning Loops and Contractual Gates: Requirements for Self-Improving A2A

**Status:** Draft requirements doc
**Scope:** ABW gate architecture, episode contracts, and the loop-gate interaction that makes agent-to-agent self-improvement demonstrable rather than merely instrumented.

---

## 1. Problem Statement

ABW's five-rung verification gate (Presence / Liveness / Truth / Grounding / Binding) is a filter applied to agent output *after* generation. As gate granularity has increased, the system has surfaced more failure classes rather than converging toward fewer — this is diagnostic, not regressive: finer checks reveal failures that coarser checks previously absorbed as noise. But two distinct problems get conflated when this happens, and the retrofit below only works if they're kept separate:

1. **Checkability vs. trustworthiness.** A rung being checkable (decidable predicate) does not make it trustworthy (hard to satisfy without the invariant actually holding). Precision in a check specifies a target as legibly for the model to satisfy minimally as it does for a human to verify. Structure narrows the search space for genuine compliance and for gaming simultaneously — it does not bias toward one over the other unless the check is constructed to.
2. **Seam ownership.** Each rung can pass locally while the artifact crossing rung boundaries is silently substituted, regenerated, or resummarized. No single rung's jurisdiction covers the joint between rungs. This produces "unclosed seam" failures that look like gate failures but are actually undefined module boundaries in the Parnas sense — the interface contract for what crosses a boundary was never specified.

Neither problem is solved by tightening individual rung strictness. Both require structural changes to *when* the gate applies (before vs. after generation) and *what* persists across rung and episode boundaries (identity, not just data).

---

## 2. Design Principle: Constrain Generation, Don't Just Filter Output

A gate applied post-hoc is a checkpoint with no influence over the process it inspects. There is no gradient from gate failure back into generation unless one is deliberately wired in. This is the mechanism behind the observed pattern where more disciplined specification produces more, not fewer, discovered escape paths: the upstream generative process remains unconstrained regardless of downstream filter precision, so filter improvements only increase detection resolution, not first-pass conformance.

Four levers close this gap, in order of directness:

1. **Retrieval before generation.** Where a rung requires grounding to source data (e.g. Binding rung against GBIF), the retrieval must be a precondition tool call that generation is conditioned on — not a claim generated first and checked against source second. This removes fabrication as an available path rather than detecting it after the fact.
2. **Schema/tool-call enforcement over prose instruction.** Gate compliance encoded in a system prompt is advisory. Gate compliance encoded as typed tool-call signatures is structural — an agent cannot emit a Truth-rung-passing artifact without the tool call that required grounding data as an argument.
3. **Structured failure feedback, not blind retry.** A failed rung must feed the next attempt a structured diff — which predicate failed, expected vs. actual — not just a fresh generation prompt. Without this, repeated attempts are independent draws against a fixed filter with no memory, which is indistinguishable from a loop that isn't closing at all.
4. **SFT selection on gate-passing trajectories.** The slowest but only lever that shifts agent disposition rather than per-episode behavior: episodes with full closure and intact provenance are selected into training data; gate-failing episodes are excluded or down-weighted. This is downstream of and dependent on 1–3 — it requires clean, well-labeled episodes to train on, which levers 1–3 are what produce.

Levers 1–3 without 4 produce a system that behaves correctly under supervision but whose underlying policy hasn't shifted — any gap in the constraint architecture reopens the escape path. Lever 4 without 1–3 has nothing but noisy, sparse post-hoc labels to train against.

---

## 3. The Episode Contract

To make gate behavior — and specifically *degradation prevention across learning cycles* — inspectable rather than asserted, every agent emits a uniform, content-addressed episode record. This is defined once at the loop/dispatch level and imposed on all agents; it is not derived from any individual agent's existing behavior, to avoid re-instantiating the seam problem as five different per-agent contracts.

```
{
  episode_id,
  input_artifact:  { ref, hash },                  // content-addressed; what the agent received
  output_artifact: { ref, hash },                  // content-addressed; what it produced
  evidence: [ { source, retrieved_at, hash } ],     // independent of the agent's own output
  rung_result: { rung, pass_fail, predicate_id },
  failure_detail: { predicate_id, expected, actual } | null,
  prior_episode_id: episode_id | null,              // links correction attempts to what they correct
}
```

Two fields do the load-bearing work:

- **`hash` on input/output artifacts** functions as the identity token discussed for seam integrity. If the hash of what rung N emitted differs from what rung N+1 receives, that is a mechanically detectable seam leak — no manual inspection required.
- **`prior_episode_id`** makes correction chains queryable: did episode N+1's `output_artifact` hash differ from episode N's specifically along the dimension named in N's `failure_detail`? This is the operational test distinguishing genuine correction from independent regeneration that happens to also fail differently.

### Retrofit sequence

1. Write the schema once, outside any individual agent, as a shared type at the Strategist dispatch layer.
2. Retrofit the Genome Profiler path first — it is the one case with known provenance and a resolved fabrication bug, giving a validated instance of the schema against ground truth before propagating further.
3. Apply the identical schema to every other agent. Where an agent's existing rungs don't map cleanly onto the schema, treat this as diagnostic: it typically indicates a rung is doing two jobs at once (checking output correctness *and* checking seam integrity) that need to be split into separate predicates.

---

## 4. What Constitutes Independent Evidence

Rung trustworthiness (Section 1.1) reduces to three properties, applied per-rung during the retrofit rather than assumed globally:

- **Independence of evidence source.** A rung is gameable if the model both generates the claim and generates or controls the justification for it. It is trustworthy if evidence originates from a source the model does not author — a direct GBIF lookup rather than a model-stated citation of GBIF, a second agent instance without access to the first's reasoning trace, a deterministic recomputation rather than an asserted result.
- **Cost asymmetry.** The honest path must be cheaper than the fabricated path. If constructing a plausible provenance chain is cheaper than performing the actual retrieval, the check selects for fabrication under any optimization pressure regardless of model intent.
- **Falsifiability against a held-out case.** For each rung, a synthetic input should be constructible in advance where the rung is known to fail, with confirmation that it does. A rung with no known synthetic failure case is a rung whose actual scope is undetermined, independent of how granular its check is.

---

## 5. MVP Scope: Adaptive Self-Correction, Not Full Closure

Full five-rung closure across every seam is the target for the mature system, not the bar for MVP. The MVP claim is narrower and more honest than a general trustworthiness claim: **the platform demonstrates a closed-loop cycle of detect → attribute → correct → re-verify, without human intervention, on a real failure class.**

This is a demo of loop behavior under perturbation, not an ablation study against an outcome metric — the earlier framing in terms of Brier score improvement was rejected as the primary MVP claim because it invites confounds (episode volume, prompt drift, selection effects) that require a controlled ablation to rule out, which is not MVP-scoped work. "Adaptive self-correcting" is directly checkable without that apparatus.

Three conditions for the demo to be honest rather than staged:

1. **Pre-registered failure injection.** The failure class is selected before the run, not chosen after the fact because the gate is known to catch it well.
2. **Correction attributable to the loop, not the operator.** No manual intervention between detected failure and corrected re-attempt — this is where the structured-feedback mechanism (Section 2, lever 3) is the thing being demonstrated, not background plumbing.
3. **One clean, reproducible cycle is sufficient.** MVP does not require demonstrating that self-correction holds generally across the platform — it requires one legible inject → detect → correct → re-verify cycle, backed by the episode contract's `prior_episode_id` chain as evidence that correction, not independent regeneration, occurred.

The Genome Profiler fabrication case is the strongest available candidate: known provenance, already-resolved, re-injectable against the retrofitted contract to produce a first validated cycle before propagating the demo pattern to other agents.

---

## 6. Open Questions Carried Into Implementation

- Where episode records attach to the existing event-sourced CQRS log — as a native event type, or as a derived projection.
- Which agent's rungs, on retrofit, reveal the split-predicate pattern (Section 3, step 3) first, since that's the leading indicator of which rung was silently doing seam-integrity work.
- Whether cross-agent uniformity of the schema (Section 3) is sufficient on its own to support the eventual "platform property" claim, or whether that requires the demo in Section 5 run across more than one agent before it generalizes as evidence.
