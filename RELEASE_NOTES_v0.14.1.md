# v0.14.1 — Stop losing work, stop spending unasked, stop lying about variance

Everything here came from one operator running real forecasts on v0.14.0
and reporting what happened. Seven defects, three of which destroyed work
or money.

## Ctrl+Enter destroyed the forecast

`orchestrate_question` opened with an unconditional
`self.program = Program::empty()`. The only upstream check was a debounce
on `orchestration_running`. So the shortcut for decomposing a **new**
question silently discarded every driver, hand-edited estimate, evidence
item, agent assignment and base rate on an existing one — including
research already paid for.

Now arm/confirm, same idiom as the Delete chip, naming what is at stake:

```
Re-decomposing discards 5 drivers, 7 evidence items and 6 agent
assignments — including any estimates you edited by hand. Press
Ctrl+Enter again within 5s to confirm, or edit drivers directly instead.
```

## Decomposition spent ~$6 before you could look at it

`process_macro_forecaster_result` called `fire_agent` inside the
assignment loop. Assigning an agent to a driver and billing for it were
the same action, so there was no point at which the operator could see
which specialist had landed where. With routing still improving, that
reliably bought the wrong research.

Agents are now **staged**, not fired. They are bound to drivers in the
AST — so they appear on the driver cards for review, re-assignment or
deletion — and nothing executes until you say so.

```
🔬 Staged 5 agents on 5 drivers: football_analyst (4), entity_investigator (1).
   Nothing has run yet and nothing has been billed — review the assignments
   on each driver card, re-assign anything that looks wrong, then press
   Ctrl+Enter to run them.
```

Ctrl+Enter is now three-state: run staged research if any, otherwise
decompose (arming a confirm if that would overwrite). The Research chip
shows which state it's in — `▶ Run 5 staged` in gold, `⚠ Overwrite —
confirm` in red — so the review step is visible rather than being a
shortcut you had to know about.

## Sobol indices were not Sobol indices

On a five-driver model with one binary driver, the analyser reported for
that driver:

```
first_order_index = 1.000     total_order_index = 0.892
```

Total-order must be **≥** first-order — the total effect contains the
direct effect. And the five first-order indices summed to **1.13** when
they are additive shares of V(Y) and cannot exceed 1. The console
faithfully rendered *"regulatory_risk dominates (100% of variance)"* off
an index that was never computed, plus influence percentages totalling
110%.

Three causes, all the same shape: **a ratio whose numerator and
denominator came from different estimators.**

1. First-order divided V(E[Y|X_i]), measured with
   `Executor::with_fixed_drivers`, by a `baseline_variance` from a
   separate `Executor::execute` run. When one driver dominates the
   variance — and any binary driver does, being a discrete jump rather
   than a nudge — the ratio exceeded 1 on noise alone, and `.min(1.0)`
   published a saturated `1.000` as if it were certainty. The denominator
   is now rebuilt from the same runs via the law of total variance,
   V(Y) = V(E[Y|X]) + E[V(Y|X)], so the ratio is in [0,1] by
   construction.
2. The between-group variance was uncorrected. Each conditional mean is
   itself estimated from n draws and carries noise of variance s²/n, so
   the observed spread overstates the true spread by that much. Corrected,
   and switched to the unbiased (m−1) denominator.
3. The Saltelli total-order estimator divided *its* numerator by that
   same foreign baseline. Now uses V(A) from its own sample matrix.

`S_Ti ≥ S_i` is now enforced in the engine rather than re-derived
defensively by each consumer.

Measured on the reported model: the binary driver moved from a saturated
`1.000` to `0.618` against a total-order of `0.969`, and first-order now
sums to `0.75` with the remaining `0.25` correctly attributed to
interactions.

The two placeholder tests in that module had asserted, in comments,
exactly the properties that were broken. They are now five real tests.

## Saving one schedule hid all the others

`render_schedules_tab` gated its entire "things you could schedule" list
on `schedules.is_empty()`. Saving your first weekly cadence dropped you
into the persisted-schedules branch, which renders only persisted rows —
so every agent×driver pair you hadn't reached yet vanished mid-task, with
no way to schedule or dismiss them.

The gate was redundant too: the draft enumeration already excludes
persisted pairs. Unscheduled pairs now render below the active list with
a count in the header.

## "Analyze URL" always summoned market_research

`ingest_url_evidence` hardcoded the agent. Pasting a link about a
manager's succession onto a football driver invoked a market analyst while
`football_analyst` sat assigned to that same driver, unused — and the two
then disagreed about the same evidence. Now prefers whoever is already on
the driver, then domain routing, then the generalist.

## Simulating a shared forecast rewrote it

`run_simulation` called `mark_dirty()` unconditionally, so opening a
forecast someone shared with you and pressing Ctrl+R queued an autosave
that overwrote the shared artefact's index and appended revisions to its
history. Simulating is a read; it stays one now when the write would be
refused anyway.

## Agent runs failed after burning tokens

Research agents legitimately take three to four minutes — the console
logged successes at 3m39s — against a client-wide 120s HTTP ceiling meant
for CRUD calls. Those runs bill upstream and *then* fail on our side, so
you pay for research that gets thrown away. Agent execution now has its
own 420s budget, and the timeout message reports the budget that actually
applied instead of hardcoding "120s".

This does **not** fix the other timeout in the logs — the server's own
upstream call to `api.anthropic.com` giving up. That is server-side
configuration.

## Smaller things

- Agent descriptions wrapped one letter per line in the picker: two more
  instances of the flex-shrink defect fixed in the driver editor last
  release (a shrinkable description sharing a row with the agent id, with
  `min_w(0)`).
- The recommended-agent card falls back to the server roster for its
  description, so it isn't blank on installs without a local card
  directory.
- Library `println!`s in the sensitivity module — three lines per driver
  per simulation to stdout — are now `tracing`.

## Known, not fixed

- **Fermi chat cannot execute.** There is no mutation path from chat to
  driver estimates, weights, or simulation. Needs a design decision about
  the action surface before code.
- **A duplicate schedule row** keyed on the bound agent name
  (`football_analyst_squad_quality_trajectory`, `every 0h`,
  `next 3000-01-01`) violates the invariant documented at
  `cockpit.rs:5793`. The writing path hasn't been found.
- **One shared forecast showed index 99.9% against a simulation mean of
  0.3497**, with the revision recorded as "manual". Unresolved; needs that
  forecast's revision rows.
- **Clicking a driver doesn't open the Edit tab.** This is deliberate —
  `focus_driver` avoids yanking the right panel away from what you were
  reading — but it reads as a bug because there's no feedback.

## Testing

`fermi` lib: 245 passing. `fermi-console` lib: 196 passing.

Every behavioural change in this release was verified by compilation and
unit tests, **not** by driving the binary. The staged-research flow, the
arm/confirm guard and the schedules panel want a smoke pass on a
throwaway forecast before you trust them with real spend.
