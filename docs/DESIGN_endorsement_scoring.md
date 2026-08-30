# Scoring the endorsers

**Status:** design, not built. Captured so it is not lost while the trace's
readability work proceeds.

An endorsement that costs nothing is worth nothing. Migration 205 already says
so about citations — `human_sourced` requires one at the database level because
"a one-click *verified* button with no citation is a laundering UI" — but
`human_endorsed` is deliberately available uncited, at the strength of a model
inference, and nothing yet makes it cost anything.

This is what would.

## The claim it rests on

`grounding_trust::strength` puts an uncited human opinion and a model's opinion
at the same tier, and says why:

> A judgement. Legitimate, and not a retrieval. An uncited human opinion sits
> here too: a person saying so is the same kind of claim as a model saying so.

That has a consequence the platform has not yet absorbed. If a fact-checking
agent is ever added, it produces tier-1 opinions at scale for pennies — which is
exactly what an uncited human endorsement is worth. **A judge does not
complement the uncited endorsement; it commoditises it.**

What survives that is the *citation*: the act that reaches outside the system
and can be followed by a third party. So the design goal is not "collect more
opinions". It is: make opinions honest enough to be worth aggregating, and make
citations the thing the platform pays for.

Two mechanisms, two problems:

| problem | mechanism |
|---|---|
| endorsements are free, so they carry no information | score them |
| citations are scarce, and nothing resolves without them | pay for them |

Scoring does not create citations. Do not expect it to.

## Resolution

A **resolution** is the first row for an assertion whose verdict scores
`strength() == 2` — `tool_verified`, `human_sourced`, or `platform_derived`.
Every tier-1 row written for that assertion *before* that timestamp is a bet,
and resolution is what scores it.

Three constraints on that, each of which is a real failure if dropped.

### Derive the score; never store it

A citation is tier 2 because someone else can follow it to the same source — not
because it is true. It can be wrong, and the log is append-only precisely so a
later `rejected` supersedes it without destroying the earlier verdict.

So a resolution is itself revisable, and every score computed from one has to
move when it does. Storing a score freezes an answer to a question that is still
open. This is the same rule migration 205 states for current state:

> current state is the latest row per `assertion_id`, **derived rather than
> stored**, so a rejected-then-reverified assertion reads as exactly that
> instead of as "verified".

### A resolver does not score their own bet

Endorse a claim, then cite a source proving it right, and you have scored
yourself. Cheap to close: a resolution written by actor *X* scores every prior
bet except *X*'s own.

Not "forbid X from citing" — that would suppress the scarce act to protect the
cheap one, which is backwards. The citation still counts and still resolves the
claim for everyone else. X simply earns nothing from it.

### Count people, not rows

`assertion_verifications` has no unique constraint on `(assertion_id, actor)`,
and should not have one — a person changing their mind is information, and the
table is append-only by design.

But it means the naive crowd count is wrong, and the reference episode already
proves it. On `386a6248`, `assessment` carries two `human_endorsed` rows from
**the same actor**, 73 seconds apart:

```
ea399cc8  human_endorsed  human  2e644008-f5c…  23:33:56
ea399cc8  human_endorsed  human  2e644008-f5c…  23:32:43
ea399cc8  pending_human…  platf  grounding_co…  23:21:05
```

Counted as rows, one person clicking twice is a consensus of two. The fold is
the same one the trace already does for current state, one level deeper: **the
latest row per `(assertion_id, actor)`**.

## The blocker: claims that can never resolve

This is the part that decides whether the mechanic works at all.

The two claims queued by the reference run are different in kind, and the queue
treats them identically:

| claim | what it is | resolvable |
|---|---|---|
| `squad_value` | a retrieval that failed — "Market value data requires Transfermarkt integration" | yes, by a tool or a citation |
| `assessment` | *"Arsenal's squad performed at a higher collective level"* | **never** |

No citation settles a judgement. Every endorsement of `assessment` is therefore
an unscorable bet in perpetuity, and if judgements queue alongside retrievals the
scoreboard never moves — which is the bootstrap failure the verification queue
itself only just escaped after holding zero rows for its entire life.

The cause is a gap in the field contract vocabulary. `settleable_by` names
*which tool*, and `None` currently means "needs a person" — which conflates two
different states:

- **a person can settle this**, by citing a source that exists
- **nobody can settle this**, because it is a judgement and there is no source

Those imply different actions, so by the platform's own rule they are different
states. Today they render identically as `needs a person`, which is why a
judgement and an unintegrated retrieval sit side by side in the same queue
asking for the same thing.

**This is a precondition, not a nice-to-have.** Scoring built on top of a queue
that cannot distinguish them will have a scoreboard that reads `none` forever
while looking like it is working.

It is also the same shape as `not_produced` on the existing backlog: a
declaration vocabulary that has one word for two situations.

## Score the crowd before the individual

A single endorse/reject is binary. Brier on a binary is harsh — implied `p=1`,
so one miss scores 1.0 — and harshness on thin data is how a scoreboard becomes
something people avoid rather than something they read.

But *"7 of 9 endorsed"* is a probability, for free, with no UI change and no new
column. That is a natural Brier subject, it is the number worth putting on a
claim, and individual scores fall out of the same log whenever they are wanted.

```
squad_value   ▰▱  9 endorsements · 78% positive
              crowd Brier 0.14 over 40 resolved · usable
```

Start there.

## Reuse, do not rebuild

`src/calibration.rs` already has both halves:

- `brier_skill(brier_mean, n_yes, n_resolved)` → base rate, baseline, skill.
  Skill rather than raw score matters here for the same reason it matters for
  agents: a crowd that endorses everything on a corpus that is 90% true scores
  well and knows nothing.
- `evidence_class(n_resolved, baseline, skill)` → `none` · `undiscriminating` ·
  `no_skill` · `provisional` · `thin` · `usable`.

That second one is the guard this needs most. With **zero** resolved claims
today, an endorser scoreboard must read `none` — not `0.0`, not "perfect", not
an empty state that looks like a clean record. Absent must look different from
good, and `evidence_class` is where that is already decided.

## What it would take

Roughly, and in dependency order:

1. **Declare unsettleable fields.** Extend the field contract so a judgement can
   say it is one. Without this, nothing below produces a moving number.
2. **Fold `(assertion_id, actor)`** in the read that produces crowd counts. No
   migration; the trace already folds `assertion_id` and this is one key deeper.
3. **Derive resolutions** — first strength-2 row per assertion — and the bets
   that preceded them, excluding the resolver's own.
4. **Crowd Brier per claim**, through `brier_skill` + `evidence_class`.
5. **Endorser scoreboard**, same machinery, grouped by actor instead of claim.

No new table. The append-only log already records everything this needs; what is
missing is a vocabulary for the claims that can never leave it.

## Related

- `docs/papers/verification_for_agent_ecologies.md` §3 — the ladder, and why a
  check answering an easier question passes while a harder one fails.
- `migrations/205_assertion_layer.sql` — the citation CHECK, and the
  derive-don't-store rule this reuses.
- `tests/trace_verification_fold.rs` — the fold at the `assertion_id` level,
  already shipped, and the ratchet that keeps it.
