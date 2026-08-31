# The adversarial checker

**Status:** design, not built. Captured from a working discussion so the
conclusions are not lost.

Sibling of `docs/DESIGN_endorsement_scoring.md`. Both are blocked on the same
thing, described at the end.

## The proposition

Put every claim through an additional agent — a fact checker — to put a floor
under verification. It cannot be authoritative, so it raises the floor without
settling anything.

The instinct is right. The mechanism in that sentence is wrong, and the
difference decides what gets built.

## A checker cannot raise a floor by adding an opinion

`grounding_trust::strength` puts an uncited human opinion and a model's opinion
at the same tier, and says so:

> A judgement. Legitimate, and not a retrieval. An uncited human opinion sits
> here too: a person saying so is the same kind of claim as a model saying so.
>
> ```rust
> PROV_INFERRED | PROV_HUMAN_ENDORSED => 1,
> ```

So a checker that says *"I looked and this seems right"* moves a
`pending_human_check` claim from 0 to 1. That looks like a floor rising. It is
not. Read what tier 0 means:

> `pending_*` is weaker than `model_inference` **on purpose** — a judgement the
> agent was ASKED to make is legitimate output, while a **retrieval claim with no
> retrieval behind it is not yet anything.**

A retrieval claim does not become a legitimate judgement because a second model
agrees with it. It becomes an unbacked retrieval with an unbacked opinion
attached, and the number on the screen goes up. That is the laundering the
citation CHECK in migration 205 exists to prevent, arriving through another door.

There is a second reason, and it is worse: the checker and the claimant are
likely the same model. Production resolves Anthropic from one environment
variable. A model agreeing with itself is not a check, and the row would not say
so — `provider_used` and `model_used` are recorded on episodes, so when they
match, that belongs on the row as a caveat rather than in a footnote.

## What it can do: falsify

**The authority is asymmetric. A checker can lower with authority and cannot
raise with authority.** It cannot establish that a claim is true; it can
establish that a document is broken, because those findings are reproducible
inside the system:

| finding | reproducible? | status today |
|---|---|---|
| a block grades `tool_verified` and most of its values are absent | yes | **shipped** — `Field::produced`, computed from the document |
| the tool the contract names *does* answer a field the agent left null | yes | the probe reveals it; nothing records it |
| the agent stated one number in the field and another in the prose | yes | not built |
| the agent cited a source and the source does not contain the number | yes | not built |
| two agents in one workspace answered the same field differently | yes | not built |
| the claim is false | **no** | not possible |

Only the last needs a model, and it is the one a model cannot settle. Every
other row is a comparison over retained bytes — which means **most of what a
judge was wanted for is better done deterministically**, and using a model would
turn a reproducible finding into an opinion.

That is the real content of "raises the floor": the floor rises because things
are removed from under it, never because anything is lifted up.

## The shape to build: the `[citation needed]` bot

Wikipedia did not scale on voting. It scaled on a cheap, adversarial,
non-authoritative mark that says *this specific sentence is unsourced*, placed at
volume and resolved by people.

That mark is `pending_human_check`, which already exists. So:

> **The checker places the tags. It never resolves one.**

Which gives the operating rule:

> **A checker may move a claim in the queue. It may never close one.**

Ordering is where the value is anyway. The bottleneck is not a shortage of
verdicts — `assertion_verifications` held zero rows for its entire life until one
run two days ago. The bottleneck is that nobody has looked. Being wrong about an
ordering costs latency; being wrong about a verdict costs the training signal,
and a gate is what stops a bad artifact from becoming training data.

By the rule that a state exists if it implies a different human action:
`checker_thinks_this_is_wrong` + nobody has looked → look here first, so it earns
a state. `checker_agrees` → no action, so it does not.

## Scoring the checker is what makes "not authoritative" safe

Whatever orders the queue should be the one component whose ordering has been
measured. Score it on the same ledger as the endorsers: if it nominates
`rejected` and a human later cites a source proving the claim right, it takes the
hit.

Nearly free — the log is append-only and current state is the latest row per
`assertion_id`, so a nomination contradicted by a later settlement already *is* a
scored miss. `calibration::brier_skill` and `evidence_class` exist and already
handle the thin-data guards.

## Do not show the checker before people rule

If its opinion is visible while a human is deciding, you get herding, and the
crowd signal degenerates into "agrees with the checker". The most valuable row in
the table is one where the checker and the people **disagree**, and showing it is
how you stop producing those.

`gate_review` is the precedent for the surface: it exists because no arrangement
of counts distinguishes a correct refusal from an incorrect one. Checker-versus-
human is the identical problem one layer up.

## Open questions

1. **A fourth `actor_kind`?** The CHECK is `('tool','human','platform')`.
   `platform` currently means *we derived this deterministically*; folding a
   model's opinion into it makes `platform_derived` unauditable. Argues for a
   fourth.
2. **Its own verdict string?** Today "the agent inferred it" and "a checker
   inferred it about the agent" would both be `model_inference`. Different
   epistemic acts, different remedies, one word.
3. **May it write `rejected`?** Tier 0, and it routes differently. My instinct is
   no — it may *nominate* for rejection, which is a queue move rather than a
   verdict, and that is consistent with the rule above.

## The blocker, shared with endorsement scoring

Both designs need one thing, and it turns out the platform already has it and
throws it away.

`Grounding` has four variants, and they answer exactly the question that matters:

| variant | count | what it means | who can settle it |
|---|---|---|---|
| `Sourced { tool, .. }` | 43 | a named tool returns this | the tool, or a citation |
| `Unsourced` | 31 | no tool exists, **so it must be `null`** | nobody — and null is compliance |
| `Inferred { from }` | 27 | a judgement the agent is commissioned to make | nobody; endorsement is terminal |
| `Derived { from, how }` | 7 | platform code computes it | reproducible by construction |

`assessment` is `Inferred`, and its `why` says it outright: *"no database holds
them — which is why they cannot be verified directly."* The contract already
knows that claim can never resolve.

And then:

```rust
settleable_by: match c.grounding {
    Grounding::Sourced { tool, .. } => Some(tool),
    _ => None,
},
```

Four variants collapse to `Option<&str>`. `Unsourced`, `Inferred` and `Derived`
all become `None`, which every surface renders as *needs a person*. That is the
third time the same pattern has been found on this path: the platform computes a
distinction and discards it at the boundary, after `route:` reasons and after
`not_checkable`.

**It also means one shipped state is wrong.** `Field::produced` marks any null
contracted field `not produced`, in the colour of a fault. For an `Unsourced`
field the contract *requires* null — `squad_value` is `Unsourced`, and its two
absent totals are the agent obeying its contract, with a note explaining that
Transfermarkt is not integrated. The trace currently frames compliance as
failure, next to `advanced_metrics.xg`, which is `Sourced` and null and *is* a
finding. Opposite situations, same badge.

So the correction is small and precise: **serve the grounding kind.** Then

- `Sourced` + absent → the agent had a tool and returned nothing. A finding, and
  the probe can test it.
- `Unsourced` + absent → as declared. Not a fault, and the gap is a request for
  the integration that would close it.
- `Unsourced` + present → a violation, which `enforce` already catches.
- `Inferred` → nothing settles it, so endorsement is the terminal act rather than
  a weak substitute for a citation.
- `Derived` → reproducible; if it disagrees with the transform, that is a
  platform bug and not the agent's.

This also answers the endorsement confusion cleanly. Endorsing `assessment` was
**correct** — it is `Inferred`, and an endorsement is the strongest verdict
available. Endorsing `squad_value` recorded agreement with a retrieval that was
never made, which is a different act wearing the same button.

## Related

- `docs/DESIGN_endorsement_scoring.md` — the other half; same blocker.
- `docs/papers/verification_for_agent_ecologies.md` §3 — the ladder, and why a
  check answering an easier question passes while a harder one fails.
- `src/field_probe.rs` — the deterministic checker that exists: runs the tool a
  contract names, and decides nothing.
