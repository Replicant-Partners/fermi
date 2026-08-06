# v0.11.9 — The ops board: teams as corporations, ops as missions

v0.11.7 made team collaboration *legible* — who shared what, who moved which
number. This release makes it **directable**: the Teams panel now leads with a
board of work the team should pick up.

The operator's framing, which turned out to be the right one:

> *"i'm really thinking things like EVE Online corporation dashboard type of
> thing… a forecast, a portfolio and its cascades are the kinds of missions
> and activities that need to be coordinated."*

## Teams are standing, ops are bounded

| | Team | Op |
|---|---|---|
| Lifespan | **permanent** — a roster, a record, a shared surface | **bounded** — exists only while its condition holds |
| Analogue | corporation | fleet operation |

The objective lives on the **op**, not the team — which is what lets a team
outlive any particular push. (`teams.mission` is untouched; it means something
else.)

## Detected, never authored

**Nothing stores an op.** Each is a condition currently true of the team's
shared surface, recomputed per request. One decision, three consequences:

* **The definition of done is the detector going quiet.** No lifecycle, no
  close button, no assignee column — and structurally no way to accumulate
  stale tickets. An op exists exactly as long as the situation does.
* **Retroactively correct** — the board fills from forecasts that long predate
  it, with no backfill.
* **Cannot drift** — a stored op can disagree with the world; a derived one
  can't.

Ops are claimed by *acting*, not by being assigned. There is deliberately no
"mark complete" control, because it would be a lie.

Stated cost: an op can't yet carry "I'm on this", a thread, or a snooze. The
time to add a table is when someone asks for one of those.

## The four detectors

| Op | Raised when |
|---|---|
| ⚡ `cascade_review` | cascades queued and undecided, grouped by trigger forecast |
| ⚔ `contested` | two people moved the same forecast >2pp in **opposite** directions within 21 days |
| ⏱ `resolution_due` | active, target date within 14 days or already past |
| 👁 `unreviewed` | live >7 days on a shared surface, **never revised by anyone** |

`contested` is the flagship — and it only became possible because v0.11.7
added `actor_user_id`. Genuine disagreement between forecasters is the most
valuable thing a team can surface; it's where the assumptions live. Before
attribution it was invisible **even to the two people doing it.** The audit
layer paid for the coordination layer.

`unreviewed` is keyed on *zero revisions* rather than *no second reviewer* on
purpose: the nicer-reading version would be blind to everything published
before v0.11.7, since those revisions have no recorded actor. Keyed this way it
fires on legacy data — which is why the board isn't empty on first use.

## Urgency is comparable across kinds

Because the point of one board is ranking dissimilar work against each other:

```
cascade_review   80–100   damage: the numbers are known incoherent
resolution_due   70– 90   overdue — the calibration record is rotting
contested        50– 79   information: valuable, not an emergency
resolution_due   45– 64   upcoming, plannable
unreviewed       20– 44   maintenance, always last
```

**Damage outranks information** — no disagreement, however large, outranks an
unreviewed cascade. That rule is what makes the ranking explainable.

These bands are asserted in unit tests, not just documented. Writing those
tests caught a real inconsistency: `contested` had a ceiling of 84 against
`cascade_review`'s floor of 80, so the code contradicted its own stated rule.
The ceiling is now 79.

## Cascades were private to one person

Slice 1 had a prerequisite that turned out to be a genuine defect.

`pending_cascades.owner_id` gated the queue, and apply/dismiss gated on
ownership. So the **single most coordination-hungry object in the product was
visible to exactly one person**: a team could share a portfolio, jointly manage
the forecasts inside it, and still have one member able to see or clear the
cascades their resolutions queued. Everyone else saw an empty queue while
coherence rotted.

Now: visibility follows **forecast access**, decisions require **edit on the
trigger forecast**. `owner_id` is retained as attribution — who triggered it —
which is what it should always have been.

`undo` stays owner-gated. Reversing someone else's *applied* decision is a
different act from clearing a queue, and its blast radius is
already-propagated values; widening it deserves its own decision.

### One ACL, four consumers

The forecast-view predicate had been hand-copied into `list_forecasts_handler`
and was about to be copied into the cascade queue and the detectors. It's now
`fermi_auth::visibility::forecast_view_predicate` — one definition. Every copy
is a place for an ACL to rot: the team-share branch was missing from the list
for a full release because someone added it to the detail handler only.

## Also fixed: the Shared tab that never loaded

Reported alongside this work — Teams → Shared showed *"Could not load the
team's shared items"* while the pill read *"Shared (4)"*. Not a server bug: the
request was never sent.

Four code paths change the selected team (auto-select, Dashboard card,
post-create, left-pane click). Three set `selected_team_id` directly and called
only `fetch_team_detail`; the v0.11.7 surfaces were wired to the fourth. And
the panel couldn't self-heal, because the click handler early-returns when the
team is already selected — so anyone with **one** team was permanently stuck.

Fixed at the root: `select_team` is now the single funnel all four paths call,
and treats a re-select as a retry when data is missing.

The placeholder also conflated three conditions behind one sentence — request
failed, request never fired, genuinely empty. That cost a database-forensics
session to tell apart when the SQL had been fine all along. It now records the
real error per surface and distinguishes in-flight / failed / never-requested,
each with a Retry.

## Validation

* 8 new unit tests on the urgency algebra (the band ordering *is* the product
  judgement, so it's asserted, not commented), plus char-safe truncation over
  the accented WC dataset.
* All four detectors executed against production data read-only: three
  correctly return zero (no queued cascades, attribution only days old, no
  target dates set) and `unreviewed` raises 6 real ops on an 18-forecast
  surface.
* `cargo check --workspace` clean; 45 collaboration tests green.

## Not built yet

* **Treasury / Bridge panes.** `teams.workspace_budget` and `workspace_spent`
  exist and are *still* unrendered. A corp wallet with per-member agent spend
  is the natural next slice.
* **Functional roles** — Analyst / Reviewer / **Resolver** / Quartermaster
  instead of owner/admin/member/viewer. Resolution rights are a real
  governance question.
* **Cross-team home** — one board aggregating ops across every team, plus an
  "acting as" context so new work lands on a team instead of arriving private
  and being shared afterwards.
* No XP, badges or streaks. The score is Brier and it's already honest.

Design: `docs/specs/SPEC_27_TEAM_OPS.md`.
