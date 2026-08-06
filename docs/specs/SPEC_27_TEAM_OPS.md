# Spec 27 — The ops board: teams as standing organisations, ops as bounded missions

**Status:** slice 1 implemented (v0.11.9)
**Builds on:** Spec 26 (share provenance, portfolio inheritance, actor attribution)

## 1. The problem with Spec 26

Spec 26 answered *"who did what?"* — provenance, attribution, activity feeds.
Useful, and entirely **retrospective**. It made the Teams panel an audit log
with good manners.

What a team actually needs is **prospective**: *"what do we do next, and who
has it?"* A team here was a permission group. An EVE Online corporation — the
reference the operator reached for — is a permission group **plus an
operations board, a treasury, and a standing record.** We had built the
membership layer and called it collaboration.

## 2. The model

| | Team | Op |
|---|---|---|
| Lifespan | **permanent** — outlives any particular push | **bounded** — exists only while its condition holds |
| Carries | roster, treasury, record, shared surface | objective, scope, participants, clearing condition |
| Analogue | corporation | fleet operation |

The objective lives on the **op**, not the team. That is what lets the team
outlive the work. `teams.mission` is deliberately untouched by this spec — it
is the composition/strategist field and means something else.

A person may belong to many teams; the substrate already allowed this
(`team_members` has no single-team constraint) and nothing here changes it.

## 3. Detected, never authored

**Nothing stores an op.** Each is a condition *currently true* of the team's
shared surface, recomputed per request. This is the load-bearing decision of
the whole design:

* **The definition of done is the detector going quiet.** No lifecycle, no
  close button, no assignee column, and structurally no way to accumulate
  stale tickets. An op exists exactly as long as the situation does.
* **Retroactively correct.** The board is populated on day one from
  forecasts that long predate it, with no backfill.
* **Cannot drift.** A stored op can disagree with the world. A derived one
  cannot.

Ops are **claimed by acting**, not by being assigned. The UI carries no
"mark complete" or "assign to" control, because either would be a lie.

The cost, stated plainly: an op cannot carry state a human wants to add —
"I'm on this", a discussion thread, a deliberate snooze. That is a real
limitation. The right moment to add a table is when someone asks for one of
those, not in advance.

This is the same derived-not-dual-written discipline as Spec 26 §4.2, for the
same reasons.

## 4. The detectors

`GET /api/teams/:id/ops`, scoped to `collab::team_surface` — so an op can
never point at work the team cannot see.

| kind | Fires when | Reads |
|---|---|---|
| `cascade_review` | cascades queued and undecided, grouped by trigger forecast | `pending_cascades` |
| `contested` | ≥2 humans moved the same forecast >2pp in *opposite* directions within 21 days | `fermi_forecast_updates.actor_user_id` |
| `resolution_due` | active, `target_date` within 14 days or past | `fermi_forecasts` |
| `unreviewed` | active >7 days on a shared surface, **zero** revisions ever | `fermi_forecast_updates` |

Two notes on detector design that matter more than they look:

**`contested` is the flagship, and it only became possible because of Spec
26.** Genuine disagreement between forecasters is the most valuable thing a
team can surface — it is where the assumptions actually live — and before
`actor_user_id` existed it was invisible even to the two people doing it.
The audit layer paid for the coordination layer.

**`unreviewed` is deliberately keyed on "zero revisions", not "no second
actor".** The latter reads better but would be blind to everything published
before v0.11.7, since those revisions have no recorded actor. Keyed this way
it fires correctly on legacy data — which is why the board is not empty on
first use.

Each detector is failure-isolated: a broken one degrades the board rather
than 500ing it. A partial ops board is useful; no ops board is not.

## 5. Urgency

`urgency` is 0–100 and comparable **across** kinds, because the point of one
board is ranking dissimilar work against each other. Bucketed server-side
into `critical/high/normal/low` so clients don't hardcode thresholds we may
retune.

The bands encode a product judgement, and it is asserted in tests rather than
left to a comment:

```
cascade_review   80–100    damage: numbers are known incoherent
resolution_due   70– 90    (overdue) damage: the calibration record is rotting
contested        50– 79    information: valuable, not an emergency
resolution_due   45– 64    (upcoming) plannable
unreviewed       20– 44    maintenance, always last
```

**Damage outranks information.** No disagreement, however large, may outrank
an unreviewed cascade. This rule is what makes the ranking explainable to the
people using it.

**Overdue resolutions are allowed to overlap the cascade band** — intentional,
not an oversight. A forecast weeks past its target date is losing the team its
calibration record; that is damage of the same class, so it competes on merit.

`contested` may never reach `critical`, or the board cries wolf on normal
analysis.

## 6. Cascade access, realigned

Slice 1 had a prerequisite that turned out to be a genuine defect.

`pending_cascades.owner_id` gated the queue (`WHERE pc.owner_id = $1`), and
apply/dismiss gated on ownership. So the single most coordination-hungry
object in the product was **private to one person**: a team could share a
portfolio, jointly manage the forecasts inside it, and still have exactly one
member able to see — or clear — the cascades their resolutions queued.
Everyone else saw an empty queue while coherence rotted.

Fixed by the Spec 26 principle: visibility follows **forecast access**, and
decisions require **edit on the trigger forecast**. `owner_id` is retained as
attribution — who triggered it — which is what it should always have been.

An ops board that shows a teammate work they cannot action is a nag list, so
this had to land in the same slice.

`undo` stays owner-gated: reversing someone else's *applied* decision is a
different act from clearing a queue, and its blast radius is
already-propagated values. Widening it deserves its own decision, not a side
effect of the ops board.

### The fourth copy

The forecast-view predicate had been hand-copied into
`list_forecasts_handler` and was about to be copied into the cascade queue
and the detectors. It is now `fermi_auth::visibility::forecast_view_predicate`
— one definition, four consumers. Every copy is a place for an ACL to rot:
the team-share branch was missing from the list for a full release because
someone added it to the detail handler only.

## 7. Console

The Teams panel leads with **Ops** as the default tab: Ops / Roster / Shared
/ Activity. Rows are dense and ranked by urgency, with the objective as the
most prominent element and `done_when` stated underneath — so the board
teaches its own mechanics rather than implying a button that doesn't exist.

Empty is good news on this board, so the empty state says so and enumerates
what *would* raise an op. That doubles as documentation of the detectors.

## 8. Not built, deliberately

* **No XP, badges, or streaks.** The score is Brier and it is already honest.
  Bolting invented currencies next to a real calibration metric cheapens the
  real one.
* **No hand-authored ops** (§3).
* **Treasury / Bridge panes.** `teams.workspace_budget` and
  `workspace_spent` exist and are still unrendered — a corp wallet with
  per-member agent spend is the natural next slice.
* **Functional roles.** Analyst / Reviewer / **Resolver** / Quartermaster
  instead of owner/admin/member/viewer. Resolution rights in particular are a
  real governance question.
* **Cross-team home** — one board aggregating ops across every team, and an
  "acting as" context that scopes creation so new work lands on a team
  instead of arriving private and being shared afterwards.
