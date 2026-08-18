# v0.18.0 — checks that could not fail, and the defects they hid

Every check in this release was written because an existing one was green for a
reason that had nothing to do with the thing being correct.

`genome_profiler` is asked for genome size, karyotype, divergence date and IUCN
status. It holds two GBIF tools and both return taxonomy, so three of its four
output blocks have no possible source. It filled them anyway: 56 episodes, 13
cached profiles served to users, values like `"200-400 Mb"` for a scale insect
nobody has sequenced. Card conformance passed, the publish gate passed, JSON
parsing passed, cache validity passed, type checking passed, anomaly detection
passed. All of them examine **shape**. The failure was **content**.

That is the class this release is about, and it recurs at every layer: a value
that looks verified because it is present, well-formed, correctly typed, and
compared against nothing.

---

## The five trust contracts

| Contract | Question it asks |
| --- | --- |
| `schema_trust` | Is the column present? |
| `rollup_trust` | Is the column telling the truth? |
| `grounding_trust` | Could this value have come from anywhere? |
| `port_trust` | Is the caller sending what the agent said it takes? |
| `liveness_trust` | Does the write path ever run? |

`grounding_trust` classifies every declared field as `Sourced`, `Unsourced`,
`Inferred`, `Derived` or `Narrative`, and gates on the one rule that makes the
classification load-bearing: **every `Sourced` field must carry a
`cross_check_sql` or an explicit exemption**. A field that claims a tool could
supply it, and that nothing ever compares, is the `genome_profiler` defect with
extra paperwork.

`port_trust` moved server-side because the check already existed and was pointed
at the wrong process. `negotiate::bind_input` shipped in v0.16.0 and its only
callers were two sites in the desktop console — the API server never called it,
so every HTTP execute path was unchecked, including the creature modules that
charge credits. Worse, `stamp_invocation` read `input_binding` from a
**caller-supplied** JSON object and filed it on the episode as fact. The record
of whether the interface matched was the caller's claim about the match.

`liveness_trust` is the one nobody writes, because `count(*) = 0` is ambiguous:
unused and broken are indistinguishable, so it looks unactionable. The
disambiguator is the **opportunity count**. Zero claims beside fourteen
multiplier-bearing episodes is broken; zero beside zero is merely unused. Same
number, opposite meanings, and only the second is fine. It produced five
findings in one afternoon:

- a `CHECK` constraint declared by seventeen migrations and applied by none
- a provenance oracle wired into one of three construction sites
- `forecast_agent_claims` — coded, wired, exhaustively commented, zero rows
- `anomaly_events` never fired
- `semantic_rules.application_count`, declared in migration 010, never incremented

All five look correct in the source. Reading the code proves nothing, and
`forecast_agent_claims` has the most thorough comments in the repository.

---

## Feedback loops 1–5: built, and mostly not running

Every break was the same shape — a write path that worked, and a read path
pointing somewhere else.

**Loop 1** gated retrieval on `card.ontology_stats`, which nothing maintains:
DB-reconstructed cards hardcode `0`, 31 of 100 curated cards omit the block, and
the sole updater counted `SELECT COUNT(*) FROM kg_entities` — a table that has
never existed — with the error swallowed. The gate was shut for effectively every
agent, so no execution ever read back what consolidation had learned. It now
queries the knowledge tables and names a third state: rows present but
unembedded, therefore unreachable.

**Loop 1 (observation)** — `EpisodeScorer::write_inline` had one call site inside
the eval pipeline, so drift and anomaly detection never saw live traffic and the
HITL queue was fed only by fixtures. Live turns now write a timeline entry using
deterministic evaluators only — a `RegexSet` and a heuristic, zero LLM tokens.

**Loop 3** — `intention_coordinator` Stage 0 had never once run.

**Loop 5** — FPL statement names now resolve to agents, and driver-only forecasts
are no longer counted as lost signals.

The knowledge extractors now read what the agent actually answered.

---

## The weather prediction-market suite

Brought under the contract, and it immediately reported that it had never
recorded anything checkable: **12 successful runs, 0 retained responses, 0
claims, 0 attributions, every digest `summary = null`**. Three format mismatches
between what the card declared and what the extractor reads. None of them
raised. `status = success` throughout.

So the suite's failure mode is **silence**, and "it ran fine" is not evidence of
anything. `scripts/weather_first_run_verify.sh` pairs every sink with the
opportunity that should have driven it and reports a four-state verdict —
`OK` / `SILENT` / `INERT` / `FAIL` / `UNRUNNABLE`. `INERT` is counted separately
and is **not a pass**, so a suite that has proven nothing cannot look green.

Twelve field contracts, nine cross-checks, and an inertness probe that confirms
each predicate can actually fire.

### The checks were inert, not clean

All eight weather checks gated on `response_text IS JSON OBJECT`. That predicate
is false for `weather_oracle` — and not because the document is bad. The model
narrates before it answers when it has just made eight tool calls, so it emits a
prose preamble and a ` ```json ` fence around a correct, complete object. The
checks reported zero mismatches on nine documents they never looked at.

They now extract the outermost `{...}`, agreeing with the rest of the platform:
`parse_evidence_text` already scans to the outer braces and
`extract_summary_from_json_contract` already strips the fence. `football_analyst`
keeps the strict guard — its prose carries no document at all, 18 retained
responses and 0 structured, and relaxing it would turn an accurate `INERT` into a
confident nothing.

### Domain routing is declaration-driven

`domain_specialist` matched over four hardcoded domains and climate was not one
of them, so every weather driver fell through to `macro_forecaster`. Agents now
declare the domains they serve on the card (`RouteReason::DeclaredSpecialist`)
rather than being enumerated in Rust, and route provenance records *why* the
router chose an agent — a generalist fallback that lost is otherwise
indistinguishable from a chosen specialist that underperformed.

### Domain facts that are load-bearing

Recorded because each one is a silent 6× error waiting to happen:

- **Settlement stations are traps.** Polymarket London is **EGLC**, not Heathrow.
  NYC temperature is **KLGA**, not Central Park — but NYC *precipitation* is
  Central Park. Dallas is **KDAL**, Denver **KBKF**, Paris **LFPB**, Seoul
  **RKSI**, Taipei **RCSS**.
- **Bucket labels are integer sets, not thresholds.** `"32"` means
  `[31.5, 32.5)`.
- **Measured dispersion beats priors.** RMSE *is* the predictive sd. The
  `SSR ≈ 0.85` prior is a single-model, medium-range result; applying it to a
  pooled multi-model spread double-counts. At EGLC lead 1 the measurement and the
  prior point in **opposite directions** (0.78 vs pooled, 1.28 vs ECMWF-only).
- **Raw Brier is useless on a ladder.** An ~1/11 base rate means predicting ≈0
  everywhere scores well. Use the Brier Skill Score against climatology, which is
  why `brier_skill_score` is a required field.
- **Portfolio correlation is milder than assumed** — cross-station forecast-error
  correlation 0.05–0.25, N_eff 4.23/6, haircut 0.84. The real risk is
  *within-ladder*: buckets are mutually exclusive, so use multi-outcome Kelly,
  and an incomplete ladder manufactures a phantom arbitrage.

---

## 23 quantified judgements were dropped without a trace

`Spread::validate` correctly rejects a multiplier below the declared floor, above
the ceiling, or not ordered — repairing one would put a number into a forecast no
agent asserted. The rejection is returned to the caller. Then `episodes.rs`
logged it with `tracing::warn!` and nothing else, and
`agent_params_hook.rs` does `let (found, _rejected) = …` at three sites.

Not queryable, not retained, not countable. "How many judgements has this
platform thrown away?" had no answer.

It does now — `assertion:rejected` and `assertion_rejected:<n>` on the episode.
Of **507 `[MULTIPLIER]` lines, 23** fall outside the range their own card
declares and were discarded, every one on a run reporting
`execution_status = success` with an empty `assertions` array, which reads
exactly like an agent that quantified nothing:

```
weather_oracle       6 of 10  (60%)     equity_analyst       2 of 14
macro_forecaster     8 of 28            entity_investigator  2 of 4
nba_analyst          3 of 16            biotech_analyst      1 of 12
football_analyst     1 of 182
```

**Weather's 60% is not weather's bug.** It is asked about one bucket of an
eleven-way ladder, where the honest adjustment to a broad prior is routinely
0.01–0.05. A multiplier scales a *driver's* prior; `P(this single bucket)` is not
a driver, and the floor was correctly refusing a category error. The card now
says so, rather than merely tightening a number.

Assertions are captured in `agent_output_to_episode` — the single constructor
every execution passes through, so a new execution path cannot silently skip it.
`Some([])` when the agent quantified nothing, never `None`: `None` means "this
writer does not extract", and an agent that ran and stayed silent is a different
fact from one nobody looked at.

---

## The first cross-check that needs no external source of truth

`advanced_metrics.xgd` must equal `xg - xga`. Every other cross-check compares
agent output against a record held elsewhere; this one compares the document
against itself, which makes it the only one always affordable. The replay checks
the other football fields need cost an external call each and spend the agent's
own rate limit, so they stay deferred with their unlock conditions written down.

Two things that would have made it useless, both avoided: the cast lives inside a
`CASE` (the one construct SQL guarantees to short-circuit, since
`'not json'::jsonb` raises), and `jsonb_typeof(NULL)` being `NULL` means an
absent field drops the row rather than erroring — because **an unrunnable check
reports healthy forever**.

---

## Migrations and schema

- Creatures rebuilt, reclaiming **1,575 column slots**; the replay leak that
  consumed the table is fixed.
- A lint rule that could not run, and therefore reported nothing, now runs.
- A constraint declared seventeen times and never once applied is applied.
- Migration failures are written down rather than inferred.
- `workspace_intentions` and `workspace_intention_signals` declared as relations
  — mig-210 added eight of their columns to `SCHEMA_COLUMNS` without the tables,
  which is exactly what `every_column_belongs_to_a_declared_relation` exists to
  catch: a column entry whose relation is undeclared produces a check that can
  never pass and never says why.
- GBIF: a filter that returned the wrong organism, and a key never read.

## Observatory and HUD

- Nine bars that were not nine of the same kind of number.
- Base-rate skew is no longer reported as 99% calibration.
- `anomaly_events` read by the column it actually has.
- Provenance a wearer can see without reading a tag.

---

## Measured state at release

```
weather_oracle    23 episodes, 10 structured, 10 naming a settlement station
                  9 cross-checks LIVE (were inert)
                  8 ok · 1 silent · 2 inert · 0 fail

cargo test --lib  505 passed, 0 failed
```

The two `INERT` are honest: no run has yet executed inside a forecast workspace
(claims require one) or arrived via a console decomposition (route provenance).
The one `SILENT` is a real finding — six judgements that predate the rejection
tag and left no trace at all.

---

## Known gaps, documented rather than discovered later

1. **The framework can detect a defect but not a fix.** `episodes` carries no
   card or prompt version, so a cross-check cannot scope to "runs since the card
   was corrected". Six `stages.forecast` rows and one `challenge` row will fail
   permanently, and the card fixes in this release can only be asserted, never
   demonstrated. A permanently-red check gets ignored, which is how this whole
   class of bug survives. This is the next structural gap.
2. **`MULTIPLIER_MIN`/`MULTIPLIER_MAX` are global constants `[0.1, 3.0]`** whose
   comment claims they come "from the card contract". Twelve cards declare twelve
   different ranges and eight declare something the runtime never reads, in both
   directions — `biotech_analyst` is invited to `0.05` and rejected at it, while
   `sentiment_analyzer` is capped at `0.3` by its card and accepted at `0.15` by
   the runtime. Measured before claiming harm: **wrongly-rejected is 0,
   wrongly-accepted is 1.** The mismatch is latent, not realised.
3. `agent_params_hook.rs` still discards `_rejected` at three sites; only
   `episodes.rs` tags the drop.
4. **Phase 3 external replay is not built.** Weather is the one agent where it is
   affordable — a keyless free tool behind every block and ground truth published
   daily — so this is the highest-value check still missing.
5. Football's replay checks stay deferred: an external call each, against the
   agent's own rate limit.
6. `genome_profiler`'s three unsourced blocks have no tool to source them. They
   are labelled `Unsourced` rather than fixed, because no endpoint exists.
7. The weather composition has run its members only twice, and the claim path
   needs a workspace it has not yet had.
8. Orchestra promotion for the weather suite is still `curated_seed`, not
   `approved`. Volume and a BSS reliability curve gate it.
