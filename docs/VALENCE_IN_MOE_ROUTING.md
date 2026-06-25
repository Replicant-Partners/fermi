# Valence as a Diversity Guard in MoE Routing (Loop 5)

> **Companion to:** `docs/VALENCE_IN_ABW.md` · `docs/architecture/FEEDBACK_LOOPS.md` Loop 5 · `agents/curated/moe_router_strategist/agent_card.json`.

---

## What the strategist is doing in Loop 5

Loop 5 is the **calibration-and-routing** loop. The strategist on the hook is
`moe_router_strategist`, a Mixture-of-Experts coordinator. Its job, per its
card, is *"route each input to the most capable expert, then synthesise"*. It
does **not** try to make the experts agree — it tries to make each expert
right on the slice of the problem it owns.

The first-order signal is empirical, not affective: `get_agent_calibration`
returns a per-member `calibration_score` (Brier-derived for forecasters,
projection-accuracy-derived for SimOps), a `trend`, and a `domain_calibration`
breakdown. The strategist's Stage 0 ranks candidates primarily on that signal,
then on semantic/skill match. Calibration is what closes Loop 5
(`src/handlers/forecasts.rs:700` annotates routing episodes when forecasts
resolve).

So where does **valence** come in? Not as a routing score. Valence enters
Loop 5 as a **diversity guard** that prevents the calibration signal from
silently collapsing the team.

## The failure mode valence prevents

Pure calibration-greedy routing has a well-known pathology: the moment one
expert gets a few-tenths edge on `calibration_score`, the strategist routes
*everything in its semantic neighbourhood* to that expert. Two bad things
follow:

1. **Specialisation atrophy.** Other members stop accumulating outcome
   annotations on the queries they were meant to specialise in, so their
   calibration confidence stays low forever — a Matthew effect baked into the
   routing weights.
2. **Hidden-domain blind spots.** Queries that *look* like the high-calibration
   member's domain but actually require a different framing get answered
   confidently and wrongly, and the synthesis step has no dissenting voice to
   notice.

Valence diversity is the structural antidote. The MoE strategist does not need
the experts to **agree** (that's `cohere_and_coordinate`'s job in Loop 3) — but
it does need the workspace to retain enough affective range that synthesis is
not a rubber stamp.

## How the strategist actually uses valence

Stage 0 ranks candidates by calibration and semantic fit. Before committing the
routing plan, the strategist runs a lightweight **valence check** over the
selected experts, using the same fields and threshold as Composition Dreaming:

1. **Read the candidate set's valence distribution.** Each member's
   `metadata.valence` (`primary_affect`, `arousal`, `valence`,
   `personality_traits`) is available from `list_workspace_agents` and the
   `agents.valence` JSONB column (migration 114).
2. **Compute spreads** across the *selected* experts:
   - `arousal_spread = max − min`
   - `valence_spread = max − min`
3. **Apply the homophily floor.** If the top-ranked candidates collapse to
   `spread < 0.25` on either axis (the same threshold used in
   `composition_dream_handler`, `src/handlers/composition.rs:310-311`), the
   strategist does one of three things, in order of preference:
   - **Decompose**: split the query into sub-queries and route at least one
     sub-query to a member whose valence broadens the spread, even if its
     calibration is slightly lower. This is the MoE-native move — more
     experts, narrower slices.
   - **Add a critic seat**: route the *same* query to a second member with
     contrasting `primary_affect` (e.g. `vigilant` against `alignment`) and
     have synthesis explicitly weight the disagreement.
   - **Flag and proceed**: if no broadening member is available, log a
     `routing-record` episode tagged `homophily_unresolved`. That episode
     becomes evidence for Loop 4 (Composition Evolution) — the team itself
     needs to change, which is *not* the strategist's authority to fix.

The choice between these is conditioned on calibration confidence: if the
high-calibration member has `n_resolved >= 10` and `confidence >= 0.5`, option
(b) is preferred (keep the precision, add the foil); below that, option (a) is
preferred (don't over-trust a thin track record, broaden first).

## Why this is a balance, not a compromise

Note what the strategist is *not* doing. It is not down-weighting calibration
to make the team look diverse. The calibration ranking is preserved — the
high-calibration expert still gets the routing decision (or the largest
sub-query slice). Valence only governs **the shape of the surrounding context**:
who else gets a seat, with what role, on which sub-query. The result is an MoE
team that is calibration-greedy *within* each routing slot and
diversity-guarded *across* slots.

That is the distinction worth holding onto: Loop 5 optimises for accuracy;
valence keeps Loop 5 from achieving that accuracy by silently collapsing into a
monoculture. Calibration tells the strategist *who is right*; valence reminds
it that the team it routes to today is the same team whose calibration data it
will be reading tomorrow.

## One-line summary

In Loop 5, calibration scores choose the experts; valence spreads decide
whether that choice is allowed to stand, or whether the routing plan must
broaden — by sub-query decomposition, by adding a contrasting voice, or by
escalating the imbalance to Loop 4 as evidence that the team itself needs to
change.
