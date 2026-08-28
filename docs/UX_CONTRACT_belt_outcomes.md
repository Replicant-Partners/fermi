# Contract — belt rung outcomes

**Between:** the trust surfaces UI and the team that owns `artifact_trace` and
`episode_trace_handler`.
**Status:** ✅ **FULFILLED.** Implemented as specified — `decided` /
`decided_absent` / `recomputed` as siblings, both omitted rather than nulled, the
four tokens, the three decisions, `decision_id`, and no `agrees` boolean. The
client landed against it in the same window. Kept as the record of what was
agreed; see the two follow-ups at the end.
**Blocks:** nothing. The belt is in v1.
**Supersedes:** `UX_REQUEST_the_object_model.md` §③a, §③b, §③c — those three
asks are folded into the single shape below so they land in one pass rather than
three.

---

## Why one shape rather than three changes

Migrations 220 and 221 shipped the column and promoted `grounding` to
`Retention::Recorded`. Nothing reads them: `artifact_trace::belt()`
(`src/artifact_trace.rs:150`) hardcodes `Outcome::NotRecorded` on every rung, and
`episode_trace_handler` never selects from `gate_decisions`. **220 and 221
currently have no observable effect.**

Fixing only that would immediately create the next two problems, so all three are
specified together:

1. **The join** — a rung must be able to carry what the gate actually decided.
2. **The silence must be typed** — "no row" now means four different things, and
   prose cannot be branched on. `credit` is permanently NULL *by design*; today
   it renders identically to a gate that simply has not been promoted, so it
   looks like an unpaid debt on every belt forever.
3. **`decided` and `recomputed` must not be merged** — 221's own text establishes
   that they disagree on 10 episodes, and that the disagreement is the finding.

---

## The shape

Replace the `Outcome` tagged enum on each rung with three sibling fields.
**Exactly one of `decided` / `decided_absent` is non-null; `recomputed` is
independent of both.**

```jsonc
{
  "rung": "grounding",
  "clock": "invocation",
  "enforcement": "metric",
  "why_not_control": "…",              // unchanged
  "refuses": "…",                      // unchanged
  "site": "…",                         // unchanged

  // ── What the gate decided about THIS artifact, from the ledger.
  //    null when there is no row; then `decided_absent` says why.
  "decided": {
    "decision":    "approved",         // approved | refused | undetermined
    "reason":      "…",                // gate_decisions.reason, verbatim
    "at":          "2026-08-28T10:07:51Z",
    "decision_id": "…"                 // so a reviewer can act from here
  },

  // ── Present if and only if `decided` is null. Never both, never neither.
  "decided_absent": {
    "token":   "retention_counted",    // closed set of four, below
    "because": "…"                     // the human sentence you already send
  },

  // ── Independent axis: the contract re-run over the retained response.
  //    null on every rung that cannot be recomputed.
  "recomputed": {
    "fields":     15,
    "violations": 1
  }
}
```

### `decided.decision` — three values, closed

| value | means |
|---|---|
| `approved` | the gate ran and let it through |
| `refused` | the gate ran and stopped it |
| `undetermined` | the gate ran and **could not decide** |

`undetermined` is first-class and **must never be folded into either
neighbour.** 221 already commits to this ("folding 'the check could not run' into
either verdict is how an absent check becomes indistinguishable from a passing
one") and expects ~3,065 of them; we render it as the third reading, as we do
everywhere else.

### `decided_absent.token` — four values, closed

| token | when | permanent? | how we render it |
|---|---|---|---|
| `fires_before_artifact` | the gate decides *whether to run at all* — `credit`, `rate_limit` | **yes, by design** | **not a gap.** Neutral, no finding, no debt |
| `retention_counted` | `gate_trust::GATES[g].retention == Counted` — today `input_binding`, `attachment`, `output_schema` | no — a decision could change it | a declared gap, with the gate named |
| `predates_retention` | gate is `Recorded`, and `episode.created_at` < the gate's earliest `decided_at` | no — expected for old artifacts | calm; explains itself |
| `retained_but_absent` | gate is `Recorded`, artifact is *not* older than the ledger, and there is still no row | no | **genuinely `unknown`** — the only one of the four that is a finding |

`predates_retention` is derivable without storing a deploy timestamp: compare
against `min(decided_at)` for that gate. If the gate has **no rows at all**, that
minimum is null and the honest answer is `retained_but_absent`.

**Unrecognised token → we render `unknown`, never healthy.** If you add a fifth,
we will show it as indeterminate until told otherwise; that is the ratchet, not
an objection.

### `recomputed`

Non-null only where the contract can be re-run over `response_text` — `grounding`
today. This is what the trace already computes; it is being *moved*, not added.

**Do not reconcile it with `decided` server-side.** If they disagree we render
*"approved when it ran; 2 violations under today's contract"* — a drift finding
about the platform rather than the agent, and the only one the system can
currently produce.

---

## Invariants — please assert these

Named so they can be written as tests rather than inferred:

1. **Every declared rung appears, always.** A belt that omits checkpoints it
   cannot report on looks shorter and safer than it is. (Already held by
   `every_rung_says_whether_it_can_actually_refuse`; extend it.)
2. **Exactly one of `decided` / `decided_absent` is non-null**, on every rung of
   every belt. Never both. Never neither.
3. `decided_absent.token == "fires_before_artifact"` **iff** the gate fires before
   the artifact exists. Assert against the gate registry, not a literal list, so
   it cannot drift.
4. `decided_absent.token == "retention_counted"` **iff**
   `GATES[g].retention == Counted`. Same reason.
5. `recomputed` is non-null **only** where the contract is re-runnable.
6. `decided.decision` is one of exactly three values.
7. `because` is non-empty whenever `decided_absent` is present — the sentence is
   the part a human reads, and it is the one thing we cannot generate.

---

## Two things we are not asking you to change

* **`credit` and `rate_limit` staying NULL.** Correct and final. We only need it
  *labelled*, so it stops reading as debt.
* **`input_binding` / `attachment` / `output_schema` staying `Counted`.** Your
  call, one at a time with a reason, exactly as you said. `retention_counted`
  renders that honestly and updates itself when you promote one.

---

## Compatibility

**We are the only consumer.** `/api/episodes/:id/trace` is read by
`templates/trace.html` and nothing else. A clean break is cheaper than a compat
shim — please replace the enum rather than adding fields beside it, and we will
land the client change in the same window.

Also please update the handler's own `contract` string
(`src/handlers/loops.rs:900`), which still tells clients *"`gate_decisions` has
no `episode_id` and nothing else can be joined."*

---

## What we ship the moment this lands

Already built, currently rendering the degraded version:

* The belt drawn with **real per-artifact outcomes** instead of a uniform grey.
* `?` versus faint `·` replaced by four labelled states, driven by `token`
  instead of by the client-side `since` heuristic we are using as a stopgap —
  **that heuristic is deleted the day this lands**, and it is the only guess in
  the surface.
* **Judge a decision from the artifact**: `decision_id` makes
  `POST /api/gates/:gate_id/decisions/:decision_id/review` reachable from the
  trace, which is currently only reachable from the gate list. This is the
  single biggest usability gain in the release and it costs you one field.
* The **drift finding** — recorded verdict beside today's recomputation.

**Client-side estimate once the payload changes: under a day.** The renderer is
already split along these lines.

---

## Closed — and two things that came back with it

Both were raised by the backend in the handoff, and both deserve an answer rather
than silence.

### 1. The route discriminator — **yes, prioritise it.** Small, and it is the last unverified claim on the surface

You asked. The answer is yes, and here is the reasoning so it can be weighed
rather than taken on trust.

`agent.execute` declares four rungs, `agent.execute_stream` declares two, and
`episodes` carries no discriminator — so the trace serves the wider belt for
every artifact. **Serving the wider belt was the right call** and we would have
chosen it too: dropping two real checkpoints for the majority is the worse error.

But it leaves the belt as *an unverified safety claim*, and this screen is the
only place a person will ever encounter it. We now render `belt_route.recoverable
: false` as a low-key **"belt unverified"** note, as asked — and that note is
currently on **every trace in the product**, which is precisely the condition
under which a warning stops being read. A warning that is always on is
indistinguishable from a decoration.

It is also the only remaining place where the surface shows something it cannot
substantiate. Everything else now resolves to a served declaration. One column
retires the last one.

**Not urgent for v1** — the note is honest and ships as is. Worth doing before
the belt is used to make any claim about coverage.

### 2. `undetermined` as the majority reading — handled, and worth naming

You flagged that `undetermined` is the majority reading of the grounding rung and
asked for a real third visual. It has one: its own hue, distinct from both
approved and refused, and distinct again from the four absence states.

The reason it works is the `substrate` split. Previously a rung had to say *"this
agent declares no field contract"*, which put an agent-level backlog inside a
per-artifact diagram — so the reader met field contracts before they met a
checkpoint. Moving that question to `substrate.disposition` means the rung can
now say the small true thing (*the gate ran and could not decide*) while the
agent-level fact is stated once, at the top, in its own register. **That was the
single most useful change in this handoff** and it is worth saying so: it removed
a category error we had been rendering faithfully for weeks.
