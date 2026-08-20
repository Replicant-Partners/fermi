# Hold'em: a compositional test of the grounding architecture

**Status:** design only. No code. Captured for review before anything is built.
**Date:** 2026-08-20
**Related:** `HUD_AGENT_LAYERS.md` (the layers), `GLASSES_SHELL_GENERATION.md` (the
shell generator), `SOURCE_RELIABILITY.md` (citations), `VERIFICATION_CORPUS.md`
(how a claim earns standing), `docs/papers/verification_for_agent_ecologies.md`
(the ladder and the eight rules)

---

## 0. What this is

A design for a Texas Hold'em decision-support composition, proposed as an
end-to-end exercise of the provenance architecture built for Wild. It is written
down before implementation because the interesting content is the *reasoning
about what can and cannot be grounded*, and that reasoning has twice now been
reconstructed from conversation rather than read from the repository.

Not a product plan. A test case chosen because it stresses the architecture in
places Wild cannot reach.

## 1. Why this use case, architecturally

Three properties Wild does not have.

### 1.1 It is the first domain where `sourced` is earned by computation

`card_contract::GROUNDING_STATUSES` has four values and no `derived`, so
`hud_field_scout` carries six fields labelled `inferred` whose `why` admits the
honest status is `platform_derived` and that the vocabulary lacks the word
(`HUD_AGENT_LAYERS.md §5`). For a taxonomy stamp that understatement is
defensible — it errs toward caution.

Pot odds break it. `call / (pot + call)` is arithmetic: exactly right or a bug,
with no interval and no judgement. Calling that an *inference* is visibly wrong,
and visibly wrong in a way a user would notice. The use case therefore forces
open item #4 to a decision instead of a vote.

### 1.2 It exposes correct arithmetic on fabricated inputs

In Wild the fabrication is at the leaf: an edibility verdict, where no available
tool could have grounded it. The floor rule catches it because the weak thing is
a *source*.

In Hold'em the weak thing can be a **premise the wearer supplied**. If the board
is misread — a vision model mistaking a suit, or ASR hearing "eight" for "ace" —
then every downstream number is legitimately, exactly computed and entirely
wrong. Worse, the band would report `high`, because the arithmetic block
genuinely *is* derived.

> The band would be honest about the arithmetic and silent about the reading.

This is paper §3.3's floor rule applied at the **input** boundary, which has
never been tested. Our floor handles a weak source; it has nothing to say about a
weak premise. The rule needed is:

> **A card cannot band above the provenance of its premises.** If the board was
> read by a model, every value computed from it inherits `model_inference` as a
> ceiling, however exact the computation was.

That is the ceiling half of §3.3 pointed the other way, and it generalises far
beyond poker — it is the correct treatment for any calculator sitting downstream
of a perception step.

### 1.3 It is the only candidate that closes the loop with no unresolved layer

Layer 1 (capture) does not exist and is blocked on Rokid SDK questions. Hold'em
needs **no camera**: *"ace-king suited, board queen-jack-deuce, pot 150, to call
50"* is voice → ASR → text. Craft Global simulates ASR. So an end-to-end render
is achievable now, which retires the standing item "nothing has rendered, nothing
has sent a real payload".

## 2. The framing constraint, and why it is not a compromise

A computational HUD at a live table is cheating in every card room, and in
several jurisdictions is a criminal matter rather than a house one — Nevada
NRS 465.075 covers use of a device to obtain an advantage, and other states have
analogues. Online operators ban real-time assistance in their terms.

**Not verified:** the NRS citation and the claim about other states are from
memory and must be checked before appearing in any user-facing text. Recorded
here as a flag, not a fact.

The resolution is the pattern already adopted for Wild: **a study tool, not a
live-play tool.** You replay a spot; the system shows the exact arithmetic and
labels which parts of "the right play" are computation and which are assumption.

This is a sharper proposition than the live version, for a reason internal to the
architecture. **Solver output is precisely the spec-shaped-but-ungrounded
artifact the paper is about.** A solver emits "bet 75% pot"; that number depends
entirely on the range assumptions fed to it, and those assumptions are invisible
in the output. Making the assumption layer explicit and machine-readable is a
real contribution to a category that currently ships confident numbers with no
provenance on their premises.

Two consequences in code: the shell requests **no camera** (and
`camera_is_not_requested_before_the_platform_can_carry_a_frame` would fail if it
did), and "study tool" belongs in the manifest description rather than in a
disclaimer nobody reads.

## 3. What the literature actually says

Verified by web search, 2026-08-20.

| Finding | Source |
|---|---|
| Libratus beat four top professionals over 120,000 hands of heads-up no-limit. Three prongs: a precomputed blueprint, subgame refinement during play, and a **self-improver that patched weaknesses opponents identified in the blueprint**. | Brown N, Sandholm T. *Superhuman AI for heads-up no-limit poker: Libratus beats top professionals.* Science 2018;359(6374):418-424 |
| Pluribus beat five elite professionals over 10,000 hands of six-player no-limit, having learned by self-play. | Brown N, Sandholm T. *Superhuman AI for multiplayer poker.* Science 2019;365(6456):885-890 |
| Rigorous opponent exploitation exists, and its input is **observed action frequencies** combined with a precomputed equilibrium — explicitly *not* domain-specific priors or hand-crafted features. | Ganzfried S, Sandholm T. *Game Theory-Based Opponent Modeling in Large Imperfect-Information Games.* AAMAS 2011 |

The third row is the one that shapes the design, and it corrects an earlier draft
of this argument. The claim "the machines that beat humans ignored opponents
entirely" is **too strong** — Libratus had a self-improver, and Ganzfried &
Sandholm is a whole line of work on exploitation.

The accurate statement is narrower and more useful:

> Peer-reviewed opponent exploitation runs on **countable actions**, not on read
> behaviour. Action frequencies are logged, countable and groundable — a database
> query. A microgesture is, at its best, a model inference.

That distinction is the spine of the composition.

## 4. The anchor citation: a second Hodgson

**Barrett LF, Adolphs R, Marsella S, Martinez AM, Pollak SD. "Emotional
Expressions Reconsidered: Challenges to Inferring Emotion From Human Facial
Movements." *Psychol Sci Public Interest.* 2019 Jul;20(1):1-68. PMID 31313636.**
Verified 2026-08-20 (PubMed, PMC6640856, SAGE, Glasgow eprints).

Verbatim from the abstract:

- how people communicate the six basic emotions *"varies substantially across
  cultures, situations, and even across people within a single situation"*
- *"similar configurations of facial movements variably express instances of more
  than one emotion category"*
- *"a given configuration of facial movements, such as a scowl, often
  communicates something other than an emotional state"*

The authors note the common view already *"influences legal judgments, policy
decisions, national security protocols"* and *"the development of commercial
applications"* — i.e. the products exist and are being relied upon, which is the
same observation Hodgson makes about foraging apps.

Structural parallel, and the reason this belongs in `SOURCE_RELIABILITY.md`:

| | Wild | Hold'em |
|---|---|---|
| The confident product category | mushroom identification apps | emotion / microexpression readers |
| The measured refutation | 49% best of three; death cap falsely identified | facial configuration → emotion does not hold |
| What the architecture refuses to emit | `edibility: not available` | `opponent state: not available` |
| Why the refusal is structural, not a disclaimer | no tool can supply it | no tool can supply it |

Two domains, two measured refutations, two refusals enforced in code. That makes
`SOURCE_RELIABILITY.md` a document about a *class* of defect rather than a note
about mushrooms.

### 4.1 Pentland: inspiration, not evidence

Pentland A. *Honest Signals: How They Shape Our World.* MIT Press, 2008. The
sociometric-badge work is the direct ancestor of what is proposed in §6.

**Deliberately not cited as evidential support.** My honest read is that the
popular-book effect sizes outrun the peer-reviewed record, and I have not
verified the replication status of the specific claims. Until that is checked, it
may inform *what to instrument* and must not appear in a `why` field as grounds
for believing a signal predicts anything.

Recording this distinction rather than resolving it, because the distinction is
the point: the badges are a good idea about *measurement*. Treating them as a
finished result would be the same error as trusting a mushroom app's confidence.

## 5. The Shannon reformulation

Shannon and Thorp did not intuit a roulette wheel; they instrumented it and
computed. The discipline is not "can a tell be detected" but "how many bits does
this channel carry, and with what error rate".

Making **bits** the unit derives the grounding contract for free:

| Quantity | Information | Provenance |
|---|---|---|
| pot odds | **0 bits** — arithmetic gains no information | `platform_derived`, exact |
| H(opponent range) | the uncertainty actually faced | `platform_derived` from a *stated* range |
| equity | 0 new bits; a function of board + range | `platform_derived`; exact on the river, interval earlier |
| break-even threshold | 0 bits | `platform_derived`, exact |
| action-frequency deviation | I(freq; showdown strength), **measured** | `tool_verified`, `n` declared |
| wearer's arousal signal | I(signal; own EV loss), measured **or not** | `pending_*` until measured |
| "he's bluffing" | claims bits, carries none | `unavailable_no_tool_source` |

And the whole architecture in one sentence:

> **A signal whose mutual information is unmeasured contributes zero bits, and
> must not move the recommendation.**

This is §3.3's floor rule in information-theoretic clothing, and it is worth
noting that the information-theoretic statement is *stronger*: it gives a
quantity to measure and a threshold to cross, where the provenance vocabulary
only gives a category.

## 6. The pivot: the glasses measure the wearer

Pentland's badges were worn by the person being measured. The glasses are the
same instrument: the IMU is the wearer's head motion, the microphone is the
wearer's prosody. The opponent is across a table, occluded, and outside the
sensor's honest reach.

So the sensing story is that **the glasses measure you** — which makes "player
bias identification" the strong form of the idea rather than the weak one,
because your own biases have ground truth:

- your action log is recorded
- the baseline is computable
- the deviation between them is **measured, not inferred**
- showdowns supply outcome labels
- `n` accumulates over sessions

Loss aversion, chasing sunk cost, recency weighting, over-folding to 3-bets,
decay after the third hour — each is a difference between what you did and what
the baseline says, over a countable sample. `tool_verified` from your own
database, with the interval that `n` earns.

Deep Blue for poker in which the subject of analysis is the player. Not the safe
version — the version where the ground truth exists.

## 7. The composition

```
                 holdem_coach
        the front; floors over its premises
                      │
    ┌─────────┬───────┴────────┬──────────────┐
    │         │                │              │
holdem_    opponent_       bias_auditor   tilt_monitor
solver     frequencies                    (wearer: IMU
                                           + prosody)
platform_  tool_verified   tool_verified   pending_* until
derived    from logged     from your own   MI is measured
exact      hands, n        action log
                      │                        │
                      └──── verification ──────┘
                            corpus
                       src/verification.rs
```

| Agent | Product | Grounding | Notes |
|---|---|---|---|
| `holdem_solver` | equity, pot odds, break-even, equilibrium baseline | `platform_derived` | pure, deterministic, no model. River = exact enumeration; flop = 1081 runouts, also exact; preflop vs a range = MC with declared `n` |
| `opponent_frequencies` | action-frequency model from logged hands | `tool_verified` | the Ganzfried/Sandholm line — countable, groundable |
| `bias_auditor` | the wearer's deviations from baseline | `tool_verified` | own log; ground truth exists |
| `tilt_monitor` | wearer arousal from IMU + prosody | `pending_*` → `platform_derived` only once MI is measured | starts at zero bits and may stay there |
| `holdem_coach` | the recommendation | **floor over premises** | see §8 |

**The corpus is reused, not rebuilt.** `src/verification.rs` is already
App-scoped and already has `MIN_N_FOR_HEADLINE = 30`, because it exists so a
claim earns standing over accumulated verified observations. A tell is a
hypothesis; a showdown is its expert determination. The machinery that earns a
species identification is structurally the machinery that earns a tell's bit
rate — which is the strongest argument that the Wild corpus design was right, and
the first test of whether it is actually domain-neutral.

## 8. The property worth building this for

Because the coach's recommendation floors over its premises, an exploitative
deviation justified by a tell with `n = 6` and unmeasured mutual information
**cannot band above that tell**. The consequence is a product behaviour no
solver-based tool currently has:

> **The system refuses to recommend an exploitative deviation until the bits
> exist.**

It says, in effect: *the baseline play is X; I have no measured basis for
deviating; here is what would have to be true, and how many hands it would take
to establish it.*

That is §5.6 — unverified is a work item, not a verdict — expressed as a feature
rather than a caveat. It converts the absence of evidence into a stated
experiment.

## 9. On glass

Same renderer, same `hud_contract`, no new display code:

```
┌─ ON GLASS ──────────────────────────────────┐
│ QJ2 · AKs · pot 150 · to call 50            │
│   pot odds 25.0%                exact       │
│   equity 38.4% [35-41]          1081 runouts│
│   H(his range) 4.2 bits                     │
│ ~ range 12% of hands            assumed     │
│ ! read: not available                       │
│ * your tilt: n=6, MI unmeasured             │
│ [medium]                                    │
└─────────────────────────────────────────────┘
```

The demo is not the tags. It is the **marker column compared across domains**:

| | mushroom card | poker card |
|---|---|---|
| mostly | `~` inferred | unmarked (exact) |
| the `!` line | edibility | opponent read |
| what it shows at a glance | almost nothing here is grounded | almost everything here is grounded, except the part you most want |

Unmarked is the trustworthy case, so the marker column becomes a visual signature
of how much of an answer is actually grounded — **two domains, identical code,
and the difference legible in under a second.** That is the showcase.

## 10. Open questions, to settle before building

1. **The tilt channel may measure zero bits, permanently.** If I(signal; EV loss)
   is ~0 over 500 hands, the correct outcome is that the feature never activates.
   This needs to be agreed *in advance* as a success of the design, because
   otherwise there will be pressure to ship it anyway — and §5.2 says a check
   that fires on correct behaviour gets deleted, of which "a feature that
   correctly refuses to activate" is a close cousin.
2. **`H(range)` is exact given an assumption, which makes it the most seductive
   line on the card.** A precise number resting on a guess. Needs the `~` marker
   and probably needs the assumption rendered adjacent to it, not merely tagged.
3. **Scope.** Four agents and a corpus is a bigger build than a pot-odds card.
   Recommended order in §11.
4. **Verify NRS 465.075** and the multi-state claim before any user-facing text.
5. **Verify Pentland replication status** before *Honest Signals* is cited as
   anything but inspiration.
6. **Does the input-floor rule belong in `hud_contract` or one layer down?** It is
   not display logic — it is a general rule about calculators downstream of
   perception, and `grounding_trust` may be its right home. Deciding this by
   implementing it in the convenient place is how the safety block ended up
   foraging-shaped.

## 11. Build order, if approved

| # | Step | Why first |
|---|---|---|
| 1 | `src/holdem.rs` — exact equity by enumeration, pot-odds arithmetic | pure and deterministic; the `sourced` floor everything else rests on; testable without a model |
| 2 | add `derived` to `GROUNDING_STATUSES` + a test that it covers every `Grounding` variant | resolves open item #4; the sibling test makes the split vocabulary impossible to reintroduce |
| 3 | `CardProfile` — per-agent safety block and needles, resolved by `agent_id` | retires the foraging-shaped `SAFETY_BLOCK`; poker is a better second case than weather because it *has* a safety-analogue block |
| 4 | the input-floor rule + tests | the new finding; §1.2 |
| 5 | `holdem_solver` agent card + `FIELD_CONTRACTS` | the generator refuses a shell for an agent with no contracts |
| 6 | `ShellSpec` → `glasses/holdem_solver/`, no camera | one end-to-end render on a working spine |
| 7 | `bias_auditor`, then `opponent_frequencies` | both are database work with real ground truth |
| 8 | `tilt_monitor` — measurement harness only, no recommendation path | it must earn its bits before it can influence anything |

Steps 1–4 are independent of the framing decision in §2 and of every open
question in §10.

## 12. What this forces to resolve

Recorded so the value is visible even if the app is never built:

- **open item #4** — the card contract cannot say `derived` (§1.1)
- **the foraging-shaped safety block** — `SAFETY_BLOCK` is the literal string
  `"edibility"` and `SAFETY_LEAKS` is a list of needles about poison (§11 step 3)
- **the input-floor gap** — nothing currently prevents a card banding `high` on
  exactly-computed values derived from a model-read premise (§1.2)
- **"nothing has rendered end to end"** — no camera needed, so no blocked layer
  (§1.3)
- **whether the verification corpus is genuinely domain-neutral** — its first use
  outside foraging (§7)
