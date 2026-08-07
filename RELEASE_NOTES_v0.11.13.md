# v0.11.13 — Assumptions: disagree with the input, not the question

The operator's model for what teams coordinate on is *"trajectories and
research and **assumptions**"*. The first two have had homes since Spec 26.
This release builds the third.

## The hole

There was no way to say the single most common thing one forecaster says to
another:

> *"your base rate for `elo_current` is wrong, here's why"*

It happened in Slack, or as a probability revision with a `reason` string,
or not at all. None of those attach to the thing being disputed, so the
objection was invisible to whoever opened the forecast next — which is
exactly when it matters.

The `reason` string was the closest existing thing and it's the wrong shape
twice. It makes you *change the number* to say something, so a reader who
thinks a base rate is wrong but hasn't got a better one has no move at all.
And it describes a transition, so there's no state for "we still disagree
about this".

## Anchored to the driver

A forecast-level comment thread would have been much easier and much less
useful. Disagreement here is almost never about the question; it's about
**one input**. Anchoring at `(forecast, driver)` means the objection renders
next to the number it disputes, survives a revision of some *other* driver,
and makes "which assumptions are contested" a query.

Three kinds — `challenge` (this is wrong), `question`, `note` — and
resolution is **status, not deletion**: `accepted` (the driver changed) and
`declined` (considered, rejected) are different outcomes, and the difference
is exactly the reasoning a team wants to re-read later. The database refuses
a resolution that doesn't record who and when; that gap is what Spec 26
existed to close and there's no reason to reopen it in a new table.

## The check that changed the design

The obvious anchor was `fermi_forecasts.drivers`, a JSONB column that looks
exactly like where driver state lives. Production said otherwise:

```
 typ  | count | max_len
array |    78 |       0     ← every row is an EMPTY array
```

**Nothing populates it.** A driver is a `driver <name> { ... }` declaration
inside the FPL program — a language construct, which is what the executor,
the LSP and BayesOps all read. Re-anchored to `fpl_source` and re-checked:
**66 of 78** forecasts declare drivers there, and **66 of 66** real programs
parse and yield a name set (342 names).

Had this shipped against the column, the feature would have attached to a
phantom — every annotation instantly orphaned, every badge zero, the
detector permanently silent — and all of it *working as coded*.

## Renaming a driver, and undoing that

A name is not a foreign key, so a driver can be renamed out from under an
annotation. The sweep parses the program after any edit and reconciles. Two
properties are load-bearing:

**It fails safe.** Unparseable source touches nothing. The composer
autosaves mid-keystroke, so a half-written program is routine — "we couldn't
establish the name set" has to be a different answer from "there are no
drivers", or one unclosed brace would orphan every annotation on the
forecast.

**It runs backwards.** Orphaning is an observation about the current
program, not a decision, so a Spec 31 revert that restores a deleted driver
restores its objections with it. Undo that silently dropped them would be
lossy in exactly the way this collaboration model says it isn't. A human's
`accepted`/`declined` is a judgement and stays put.

## Ops detector 5 — `contested_assumption`

The payoff, and what the other four were working towards. `contested`
*infers* disagreement from probabilities moving in opposite directions —
real, but it can only tell you two people disagree, never about what. This
is the same disagreement stated outright and anchored: the difference
between *"reconcile this forecast"* and *"settle whether the base rate for
`elo_current` is right"*.

Same **50–79** band as `contested`, deliberately: neither stated nor
inferred disagreement is more urgent in the abstract, and they should
interleave by age and size. It climbs 2/day, so **one unanswered objection
reaches the band ceiling on age alone in a fortnight** — the failure mode
here isn't disagreement, it's an objection nobody ever answered.

Only challenges count; a `note` implies no action and a `question` is
answered by talking. Orphaned annotations fall out for free, which is the
point of the sweep — a board item you can't act on because its subject is
gone is worse than no board item. `done_when` is *"each open challenge is
accepted or declined"*: both outcomes close it, so the board never reads as
pressure to agree.

## Console — the Assumptions tab

Every **declared** driver, not just the annotated ones. The uncontested ones
are the context that makes a contested one meaningful, and it's the only
place in the console that answers "what does this forecast assume?" as a
list. Contested rows are tinted and badged `contested ×n`.

```
⚖ Assumptions                              3 open challenges

  strength_factor
  conditions                    contested ×1        Challenge
  │ ! Bo    the 0.7 floor is too generous
  │   [Accept — I changed it]  [Decline — considered, keeping it]
  disruption                                        Challenge
  ─────────────────────────────────────────────────────────
  The forecast as a whole                           Challenge
```

The badge comes from the server, never derived client-side: the detector
counts the same thing, and two implementations of "is this contested" would
eventually have the badge and the board telling the team different stories.

Answering is two buttons side by side, neither styled as primary. One button
would have to pick an outcome silently, and making Decline the quiet
secondary would put a thumb on the scale.

## Permissions — creating is view-gated

The one place the moderate permission model bends toward the wiki. A `view`
grant exists so people can read *and react to* a forecast; "you may see this
but not say it's wrong" would defeat the point of publishing it. Annotating
changes no forecast state — it's the cheapest reversible act in the product.

Resolving needs `edit`. Deleting is **author-only**, because an editor
deleting an objection against their own work is the one way this could hide
disagreement; their route out is `declined`, which stays on the record.

## Also in this release

**One-click grant edit.** Clicking a `view` chip on a share row promotes it
to `edit`. `POST .../shares` was always an idempotent upsert, so this was a
UI gap, not a backend one — but the only route before was
revoke-then-re-add: two destructive steps to make one additive change, with
a window in between where the colleague had no access at all. Demotion is
deliberately not offered on the chip; taking access away shouldn't happen by
mis-click.

**History reconciler.** A periodic sweep re-commits recently-updated
forecasts, closing v0.11.12's stated gap (the resolution sweeper and
`workspace/refit` hold only a `PgPool`). `commit_files_as` no-ops on an
unchanged tree, so it's idempotent with the existing hooks — and it catches
*future* unhooked writers too, rather than only the two we knew about.
`FERMI_HISTORY_RECONCILE_SECS`, default 300, `0` disables.

## Validation

`scripts/spec26_sql_check.sh` gains **PART C**: migration 183 applied twice,
the orphan reconcile asserted reversible and asserted not to touch resolved
rows, the attribution CHECK asserted to refuse an unattributable `accepted`,
and detector 5 asserted to go quiet for all four reasons it should. Plus
unit tests for name extraction against a real production program, the
fail-safe cases, and the urgency band ordering.

## Migration

`183_driver_annotations.sql` — one new table, no changes to existing ones.
Idempotent, verified applying twice.
