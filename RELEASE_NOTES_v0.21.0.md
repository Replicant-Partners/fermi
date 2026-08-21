# v0.21.0 — a declaration nothing reads, found eight times and then made to fail the build

This release started as one bug report: a driver assignment did not survive a
save. It ended as eight fixes with a single shape, and a test that makes the
ninth fail to compile.

The shape is **a declared thing that nothing consults**. Present, well-typed,
required by the parser, persisted to the database — and compared against
nothing, rendered nowhere, or read by no code path a human ever sees. Every
defect below is an instance. None was found by a failing test; all were found by
reading.

---

## Why this kept happening

It is the shadow side of a deliberate choice. This platform is declaration-driven
rather than driven by hardcoded Rust tables, and that is right: an agent becomes
routable for a new domain by editing `metadata.domains` on its card, not by
shipping a console release. But the pattern has one failure mode:

> **Adding a declaration is cheap. Adding its reader is separate work. Nothing
> fails in between.**

A hardcoded table breaks loudly when it is incomplete. A declaration sits there
looking correct.

The second cause is structural. `cockpit.rs` and `main.rs` are **51,998 lines
with no test coverage at all** — rustc segfaults expanding GPUI's element chains
under `--test`, which is why the crate keeps its testable logic in a GPUI-free
lib target. That is exactly where the missing readers live: rendering,
provenance, attribution, the FPL emitter. The layer an operator looks at is the
layer no test can look at.

The two causes multiply. Declarations go unconsumed, and the place they would be
consumed is the place nothing can check.

---

## The systemic fix — `tests/declaration_contract.rs`

Every field of `DriverStmt` and `BaseRate` now names what reads it, or admits
nothing does and says what closing the gap would take.

The mechanism is **exhaustive destructuring with no `..` rest pattern**. Adding a
field to `DriverStmt` fails to compile in this file until somebody writes down
its consumer. The compiler enforces completeness; the list cannot drift.
`CROSS_CHECK_EXEMPTIONS` in `grounding_trust` is the model — the point is not
that everything is consumed, it is that nothing is *silently* unconsumed.

`EXPECTED_ORPHANS` is pinned rather than bounded, so movement in either
direction has to be deliberate. It opened at 2 and both were closed in this
release.

**What it is not.** It does not prove a consumer exists. A text search for
`.field` was tried first and produced false results in both directions —
`source`, `query` and `kind` collide across unrelated structs, and one filter
accidentally hid `src/assertions.rs` from itself and reported a consumed field as
orphaned. Shipping a detector whose measurements cannot be trusted would be this
same mistake one level up. So it forces a human statement per field and makes
that statement reviewable in a diff. A weaker claim, honestly made.

---

## The four Tetlock requirements

The release is organised around four things an operator should be able to
answer. Three of the four could not be answered before it.

| | before | after |
|---|---|---|
| Why the base rate is the base rate | provenance false and invisible | producer carried, rendered, checked against ERA5 |
| Which drivers have what impact | already good | unchanged |
| See the curves of each driver | 1 of 5 distribution types | all 5 plus discrete, from the real sampler |
| How research changed the inside view | join key discarded | attribution and `evidence_refs` link |

---

## Driver assignments did not survive a save

`generate_fpl_text` rebuilds the cached FPL from the AST and walked question →
drivers → evidence → model → simulate. It emitted no `agent` statements at all,
so an assignment lived in the AST, was deleted on emit, and the save reported
success. The only trace was a driver reading "No agents" on reload while the
schedule panel still listed it — schedules are persisted server-side, outside
the FPL.

`cached_fpl_is_richer_than_ast` looked like a guard and was not: it declines to
regenerate when the *text* already contains `agent `, which protects a forecast
that arrived with one and cannot protect an assignment just made.

`Schedule` had no way to spell two of its three variants. `Once` existed only as
the value of an ABSENT field and `Cron` could not be written at all, so
`schedule: once` — what every manual assignment produces — parsed as a field
name and failed on the missing colon. The parser now accepts `once` and
`cron "<expr>"`, and rejects an unknown cadence rather than silently returning
`Once` without consuming it. Measured: 118 `schedule:` uses in the corpus, all
`every N <unit>`; 82 stored programs still parse, 0 failures.

The emitter is no longer trusted to be complete. `regenerate_cached_fpl_if_safe`
reparses what it produced and refuses to overwrite `cached_fpl` if a statement
kind, an agent binding, or a driver link would be lost.

Three more fields the emitter could not write are now written: `applies_to`,
`evidence_refs`, and `discrete` drivers, which were `_ => {}` and vanished
entirely.

---

## The assignment gate observed and permitted

Both conditions were already detected and neither stopped anything.
`agent_is_routable` was consulted only by the chat dispatcher, so the picker
could attach an agent no executor can run. `bind_input` was consulted only by the
picker, which pushed a warning reading **"Running it anyway"** and then ran it
anyway. Two doors with different locks is one door with none — and permitting is
not free: assignment mutates the AST, fires the agent and bills credits.

`negotiate::admit_assignment` is now the single gate for both paths. Measured
before shipping:

| | |
|---|---|
| Agents declaring ports, none text-shaped | **47 of 111** |
| Of those, appearing in any live binding | **0** |
| Live bindings that still pass | **104 of 104** |
| Agents declaring no `accepts` (admitted) | 10 |

`watermark` takes `image`; `instagram_publisher` takes `caption`;
`weather_calibrator` takes `raw_predictive_distribution` — a comment in
`negotiate.rs` already recorded that the router had been fixed to avoid it, while
manual assignment could still do it.

A domain check was considered and deliberately left out: only 4 of 111 declare
`metadata.domains` explicitly (3 of them empty), so refusing on an explicit
mismatch is near-inert, and refusing on the tag fallback would reject deliberate
human choices on a heuristic written for search.

---

## One judgement, recorded three times, then bound to one driver of five

`extract_from_prose` scans the whole `raw_response`, and an agent states its
conclusion in `key_findings`, again in the prose body, and again in a JSON
restatement. Each match became a separate `Assertion`.

Measured over the production episode table:

| | |
|---|---|
| Multiplier rows | 64 |
| Episodes holding them | 31 |
| **Distinct triples** | **31** |
| Duplicate rows | 33 (52%) |

Not one episode held two different multipliers. The count is load-bearing:
anything deciding whether an agent bound to five drivers had said five things
would read three restatements as three judgements.

That matters because of the **broker pattern** — one complex agent responsible
for several drivers, resolving the relationships internally. It is a pattern the
platform must support, and it produces one number for many drivers, which cannot
honestly be split:

* applying it to every ref **compounds** it — 1.25 across five drivers is 3.05.
  That is what `agent_params_hook` does.
* applying it to `driver_refs.first()` picks arbitrarily and says nothing. That
  is what the console did, and it is why four of five drivers on a broker-driven
  forecast never moved while the first one did.

Neither is defensible, so neither is offered. `bind_judgement_to_driver` resolves
by `target_hint`, then by arity, and otherwise returns the candidates so a human
makes the choice that was always being made silently.

`Assertion.target_hint` — "the driver the agent seems to be talking about, if it
said" — had existed since the assertion layer, populated at 0 of 3 construction
sites and read nowhere. It is now the first thing consulted. No agent fills it
yet, so that arm is unreachable today, deliberately: it is the shape this becomes
correct in when an agent names its target.

---

## The trajectory recorded that the forecast moved, never what moved it

`forecast_spacetime` has carried `triggering_agent` and `evidence_delta` since
migration 140, and `forecast_timeline_handler` projects both. The console sent
`agent_id: None, evidence_added: None` with a reason string naming only Monte
Carlo statistics, so the server derived `revision_trigger = 'manual'`. Accepting
an agent's multiplier was recorded as a manual edit by nobody citing nothing.

`DriverStmt.evidence_refs` was the other half. Read by `semantic.rs` and the CLI
report, written by nothing — all five console construction sites passed `vec![]`.
The only association between a finding and its driver was the `{agent}_{driver}`
shape of an evidence id: **a naming convention standing in for a reference**.

Evidence now attaches to every driver the agent was hired for, and the asymmetry
with multipliers is deliberate: a multiplier is a *mutation* and compounds
silently; evidence is a *record*, and over-broad attachment is visible,
reversible, and judgeable by a human reading the driver card.

---

## The base rate disagreed with the measurement, and nobody compared them

`weather_climatology` counts ERA5 observations over a calendar window and applies
an OLS warming trend. The FPL separately declares `historical_frequency`. Nothing
had ever compared the two, though a disagreement means one is wrong about a
question of fact.

| question | declared | measured | gap | relative | fires |
|---|---|---|---|---|---|
| Chicago 78–79°F | 8.3% | 13.5% | 5.2pp | 38% | yes |
| Miami 92–93°F | 12.0% | 5.9% | 6.1pp | 103% | yes |
| Houston 74–75°F low | 12.0% | 10.0% | 2.0pp | 20% | no |
| London 32°C | 0.8% | 1.04% | 0.24pp | 23% | no |

Every driver is a multiplier on this number. Miami was carrying twice the
frequency the tool measured.

Both a gap floor (1.0pp) and a relative floor (25%) must be crossed; neither
alone works across base rates spanning 0.8% to 33%. Houston at 20% is a genuine
near-miss rather than a tuned exclusion — both its numbers share the same
bucket-width error, so they agree with each other while both being wrong, and
this check cannot see that and should not pretend to.

Reported, not applied. The measurement has a reference class too.

Separately: `apply_base_rate_only` accepts only `{"base_rate": {...}}` or a bare
`historical_frequency`, and `weather_oracle`'s card mandates neither. So the
specialist was routed to, the tool counted the observations, and the operator was
told "no parseable base rate in response" — which reads as the agent having
failed. The message now distinguishes that from "measured it, and the response
cannot be used", and names the card as the place to fix it.

---

## Who produced the base rate: written wrongly because it could not be seen

`BaseRate.generated_by` is required by the parser, emitted by the FPL writer and
persisted into `forecasts.metadata`. Its only two reads in the codebase were
those serialisation sites. Three consequences, one cause:

* `apply_base_rate_only` wrote `"fermi"` as a literal over whichever specialist
  routing had picked, so a base rate measured from 525 station-days of ERA5 was
  recorded as the generalist's;
* the `state.json` restore overwrote it with `Agent("fermi")` on every reload, so
  `examples/reference_bucket_indicator_kord.fpl`'s honest
  `generated_by: weather_oracle` did not survive one open/close cycle;
* nothing rendered it, which is why neither of the above was detectable.

`base_rate_update_in_flight: bool` became `base_rate_producer: Option<String>` —
the flag and the name are one fact, so they are one field and cannot disagree.
That also fixed a third symptom with the same root: the run row was looked up as
`"fermi_base_rate"` while `update_outside_rate` pushes `"{producer}_base_rate"`,
so any specialist's row spun forever, a finished run displayed as still working.

---

## Four of five distribution types rendered nothing

`render_driver_card` gated on `Continuous` **and** `Triangular`, and both
else-arms returned the element untouched. `normal`, `lognormal`, `uniform`,
`beta` and every `discrete` driver drew no curve at all, while the text beside
them read "no distribution" for a driver that had one.

Not cosmetic. `examples/reference_bucket_indicator_kord.fpl` carries
`predictive_error_f` as `normal(0.0, 2.796)` — the measured forecast error the
calibration rests on — and `model_cluster` as a two-point discrete whose
bimodality the file's own commentary calls "the dominant uncertainty". Neither
was visible anywhere.

The one curve that did render used `Density::from_quantiles`, which the density
module labels a sketch: a two-sided Gaussian through three percentiles cannot
show skew, a bound, or a second mode. `Density::from_samples` — a Gaussian KDE
over real draws — had been written, tested, and never called from any render
path.

`fermi::distributions::sample_literal` delegates to the same per-family samplers
the executor uses, and `sample_categorical` was lifted out of the executor so
both callers share one implementation. A second copy for display is a picture
that can disagree with the model.

`driver_curve` returns `None` for a parameterised driver rather than drawing it.
`expr_to_f64` reads a non-literal as 0.0, so the old summary rendered every World
Cup driver as "0.0 – 0.0 – 0.0" and looked authoritative doing it.

---

## Method

Three checks changed design after their first measurement, and each change
mattered:

* the no-text-input gate was calibrated against the live roster (47 of 111
  refused, 0 of 104 live bindings affected) before it was allowed to reject;
* the base-rate threshold was calibrated against four live forecasts so that two
  fire and two stay quiet;
* the assertion binder was rewritten entirely once the episode table showed 31
  distinct judgements behind 64 rows.

One measurement error is worth recording because it nearly shipped a wrong
conclusion: a grep filter of `-i assert` silently excluded `src/assertions.rs`
from a scan of its own fields, reporting a consumed field as orphaned. Prefer a
test to an ad-hoc query.

---

## Where the logic went

Every decision touched in this release was moved into the GPUI-free lib target,
which is why `fermi-console` lib tests went 294 → 327. `cockpit.rs` keeps the
GPUI call and nothing else: `plot::curve`, `drivers::bind_judgement_to_driver`,
`drivers::attach_evidence_to_drivers`, `wire::RevisionAttribution`,
`calibration::base_rate_agreement`, `negotiate::admit_assignment`.

The GPUI call sites themselves remain verified by review, not by CI. That is the
structural reason this work was hard and it is still true.

---

## Verification

```
fermi lib                    598
api-server                   197
fermi-console lib            327  (+33)
agent_block_roundtrip          7  (new)
declaration_contract           4  (new)
base_rate_provenance           4  (new)
reference_forecast_contract    5
fpl_corpus_contract      82 programs, 0 parse failures
```

17 files, +3,403 / −134.

**Not verified in this release:** `grounding_contract` has not run end to end.
The workspace build was broken four times during this work by concurrent edits in
`coherence-gate`, `memory`, `liveness_trust` and `handlers/workspace/coherence.rs`.
It should be run once that tree settles; it is the check that confirms the
weather cross-checks are still green.

## Known gaps, recorded rather than closed

* **`cockpit.rs` and `main.rs` have no tests.** 51,998 lines. Every defect in this
  release lived there or in the seam it owns.
* **The FPL emitter is lossy for parameterised distributions.** `expr_to_f64`
  flattens `triangular(socio_p5, …)`. The round-trip gate deliberately does not
  assert on distribution parameters, because it would fire on every save of a
  World Cup forecast — a gate that cries wolf gets ignored.
* **The bucket-width error.** Houston's base-rate reasoning says "this specific
  1°F band" for a label denoting the integer set {74, 75}, which is 2°F wide.
  A factor of two, in the term that is the forecast. Belongs in the base-rate
  prompt.
* **`ConditionalPosterior` reaches no forecast.** Session-scoped cache, no FPL
  consumer.
* **`set_json_params` has no caller on any server-side FPL execution path,** so a
  `learnable: true` driver silently uses its prior there.
