# Handoff — closing the loops and gates

> **Terminology note (2026-08-23):** loop numbering in this document predates the settled taxonomy. Routing is now Loop 4.B (was Loop 5); BayesOps parameter fitting is now Loop 5.B (was "Loop A"); SimOps projection accuracy is now a signal path of Loop 5.A (was 5b). See `docs/architecture/FEEDBACK_LOOPS.md`.

**Date:** 2026-08-22 · **Branch:** `main` · **Commits:** `014e0a58`, `ba398849`, `dc39df72`, `3e6c9e08`

> ## Session 2 addendum — the rung was measuring the wrong population
>
> §4.3 below sets out a two-step plan for Loop 5b and warns that the order is
> forced. **Do not follow it.** Both steps address the last link in a chain of
> five, and three of the earlier links were broken. Following it would have
> wired a trigger that fires zero times, into a reader that selects the empty
> set, for want of an anchor that nothing writes — and the suite would still
> have said `0 / 12,167` afterwards.
>
> The plan was derived from that 12,167, and **the 12,167 was wrong**. The
> contract's `opportunity_sql` counted
> `sosa_observations WHERE extra ? 'projection_id'` and its comment called each
> row "a real observation carrying a projection_id … exactly the event that
> triggers scoring". Every one of those rows is a *projection*: 61 runs sampled
> at ~200 trajectory points each, zero measurements. The rung whose entire
> purpose is to make `count(*) = 0` mean something was itself asserting a proxy
> — §5's defect, on the check that exists to catch §5's defect.
>
> What the row counts actually say, once the chain is separated:
>
> | link | writer | state |
> |---|---|---|
> | 1 · projection written | external `kask:dynamics` runner → `POST /api/observations` | 61 runs / 12,167 points |
> | 2 · commitment anchored | `commit_projection` | **had no callers at all** |
> | 3 · projection recognised | `extra->>'source' = 'simops_simulation'` | **matched 0 rows, ever** |
> | 4 · measurement resolves | `resolve_against_projection` | reached, nothing to resolve |
> | 5 · accuracy scored | `ProjectionScoringEvaluator` | never triggered |
>
> Links 2 and 3 are fixed. Link 5 is still open and is now correctly reported as
> `INERT` rather than `SILENT`, because **Loop 5b has never had an input**: the
> projections cover thirteen `chem:`/`bio:` properties and the 7,576 measurements
> on file cover fourteen different ones, with a single overlapping row. No amount
> of triggering produces a signal until those two streams overlap. That is a
> deployment fact, not a wiring defect, and it is the thing the phantom 12,167
> was hiding.
>
> Detail in §9. §4.3 is superseded and retained only so the reasoning is
> auditable.

An audit of the five verification rungs and five feedback loops against what
`verification_for_agent_ecologies.md` and `abw_logical_architecture.md` claim,
followed by fixes. This is the state at handover: what was found, what was
changed, what is still open, and the one thing that would settle the rest.

---

## 1. Why none of them were closing

Every defect had the same shape, and it is the shape the papers describe:
**code that is present, correct-looking, often carefully commented, and never
executed — with nothing downstream that would notice its absence.**

Two structural causes account for almost all of it:

1. **No forcing function.** Presence runs at boot and Binding runs per request
   because something waits on them. Liveness, Truth, and every loop gated it
   nothing — so their absence was observationally identical to their passing.
2. **Assertion of a proxy.** Where checks existed, several asserted something
   cheaper to satisfy than the property they claimed. See §5.

The remedy that worked was not better checks. It was giving each one a clock,
an endpoint, or a test that can go red.

---

## 2. What the first live run found

`scripts/liveness_contract_live.sh` had **never been executed** against a
database. First run (2026-08-22):

```
6 live · 2 inert · 0 excused · 1 silent · 0 unrunnable
```

| sink | writes | opps | status |
|---|---|---|---|
| consolidation_jobs (Loop 1 cadence) | 31 | 49 | OK |
| eval_signals.projection_accuracy (Loop 5b) | 0 | 12,167 | **SILENT** |
| forecast_agent_claims | 0 | 0 | INERT |
| semantic_rules.application_count | 27 | 2,092 | OK |
| episodes.assertions | 138 | 61 | OK |
| assertion_verifications | 0 | 0 | INERT |
| schema_migrations | 214 | 3,538 | OK |
| agent_timeline_entries | 1,405 | 3,538 | OK |
| semantic_rules | 248 | 2,326 | OK |
| anomaly_events | 0 | 1,405 | SILENT (conditional) |

Six live paths means the positive controls exist, so every other verdict is
readable rather than ambiguous.

**Re-run this after any deploy.** It is the cheapest signal in the system and
the numbers above are the baseline to compare against.

---

## 3. Fixed

| Area | Was | Now |
|---|---|---|
| **Liveness** | no schedule, no endpoint, not in CI; runner lived only in the test | runner lifted into the library (§3.4), hourly sweeper, `GET /api/admin/liveness`, offline tier in CI |
| **Loop 1** | cadence claimed, nothing scheduled | `spawn_consolidation_sweeper`, **opt-in** via `CONSOLIDATION_SWEEP_SECS`, agent-funded, capped 5/pass, refuses to run degraded |
| **Loop 2 gate** | refused 100% of AgentWide for arithmetic reasons | settles against the agent's real world model |
| **Loop 2 input** | `anomaly_events` empty; deadlock | grounding violations raise L1 anomalies |
| **Loop 3** | four-type taxonomy existed only in an LLM prompt | `coherence-core/src/incoherence.rs`, computed, persisted on both paths, consumed by the brief |
| **Loop 4** | feedback opt-in; accept ≡ no-op; absolute roster | feedback unconditional; accept reports what it applied; **delta** (migration 212) |
| **Grounding** | 4 of 9 contracts enforced on the human path | both execute boundaries enforce and stamp |
| **Scans** | asserted proxies (see §5) | tightened, and each verified by being broken |

### The two findings worth carrying forward

**Loop 2 was a closed deadlock.** Drift detection skips `persona_version <= 1`;
`bump_persona_version` has exactly one caller (`two_write.rs:201`), reachable
only via an `AgentWide` intervention, which the dead gate refused every time.
No anomalies → empty queue → no intervention → no bump → still v1 → drift
skipped on all 1,405 entries. **The loop required its own output as its input.**
`3e6c9e08` seeds it from a real grounding violation.

**Γ(C) is the wrong statistic for the Loop 2 gate.** Measured, not assumed: Γ is
identical (0.632) whether the correction is absorbed or rejected, because a
system that rejects a contradicting proposition stays perfectly coherent. The
discriminator is the correction's own post-settling activation.
➜ **`abw_logical_architecture.md` §3.2 still says `Γ(C) ≥ 0.5` and is now wrong.**

---

## 4. Open, in priority order

### 4.1 Loop 2 — confirm the seed, then widen
Watch `anomaly_events` after the next traffic. Expected chain:
grounding violation → anomaly → HITL queue → reviewer intervenes AgentWide →
gate passes → `bump_persona_version` → v2 → drift computable → detector
produces its own anomalies.
Then: raise from `execution_stream.rs` too (it stamps but does not raise —
left deliberately until the first producer is seen writing rows).
Also check **Pass 2 (dyad rupture)**, an independent anomaly source producing
nothing; `auto_form_dyads_handler` is manual-only, so there may be no dyads.

### 4.2 `forecast_agent_claims` — 61 judgements discarded
INERT at *zero* opportunities. The companion report shows 61 quantified
judgements, **all 61 produced outside any workspace and therefore discarded**,
14 also lost to markdown emphasis. Two-part fix already written in the
contract's `remediation`:
1. the `Suggested p50` regex cannot match `**1.15**` — `[\d.]+` will not match an asterisk;
2. the binding is workspace-only, so standalone evaluations lose the output entirely; that needs the assertion layer.

### 4.3 Loop 5b — two ordered steps, ordering is forced

> ⚠️ **Superseded — see §9.** The premise of this item (12,167 opportunities at
> the trigger site) was an artefact of an opportunity query that counted
> predictions. The ordering hazard described below is real and is now enforced
> in code rather than by this paragraph: the 30-day fallback is off unless a
> caller asks for it by name.
0 writes / **12,167 opportunities**.
1. **Stamp `projection_id` onto the dynamics_runner episode** when a projection is written. The evaluator reads `bundle.context.get("projection_id")` and nothing puts it there.
2. **Trigger scoring from the real-observation branch**, loading that episode via the link from (1). `EpisodeBundle::from_parts(episode, agent, …)` is the constructor.

⚠️ **Do not do (2) before (1).** `find_projection_match` falls back to a 30-day
heuristic when `projection_id` is `None`, so a triggered-but-unlinked evaluator
would write a hard-verified signal about the *wrong* projection. Loop 5b's whole
claim is that it is the one signal an agent cannot talk its way out of; a
mismatched one is worse than an absent one.

Note the stub at `simops_tools::execute_simops_write_observation` is on the
**synthetic** branch (the projection being written), not the real-observation
branch. It is the commitment site, not the trigger site.

### 4.4 Loop 4 — attribution roster
`member_delta` is written by the attribution deriver only. The
`propose_composition_change` tool deliberately names no members ("that is the
owner's decision"), so its proposals stay advisory — correct, and now reported
as such rather than as applied.

### 4.5 Docs to correct
- `abw_logical_architecture.md` §3.2 — the Γ threshold (see above).
- Line ~98's claim that every behavioural change is gated by human review *or* a coherence check: for Loop 2 the coherence check was, until today, either fatal or absent.

---

## 5. Guard rails that were not guarding

Found while verifying my own work. Both are now fixed **and verified by being
deliberately broken**:

- **`provenance_floor_coverage`** tested `contains(".with_provenance_oracle(")` — presence of a call, nothing about its argument. `.with_provenance_oracle(None)` satisfied it completely while producing exactly the ungraded rules it exists to prevent. Now requires `Some(`.
- **`grounding_execute_coverage`** proved `enforce` was *called*, not that its verdict was *used*. Now requires the `Report` to be consumed.

Two smaller traps, documented in-file: the tightened scan **matched its own
source** twice (via its needles, then via the failure message quoting them).
Needles are built with `concat!` and the scanner skips its own file.

**Rule for the next session: when you add or tighten a scan, break it and watch
it go red before you trust it.** One confirmed instance of a scan that never
could have failed is enough to distrust the rest.

---

## 6. Operational notes

| Variable | Default | Notes |
|---|---|---|
| `LIVENESS_SWEEP_SECS` | 3600 | on by default; read-only queries |
| `CONSOLIDATION_SWEEP_SECS` | **0 (off)** | **opt-in**: debits agent wallets, calls a paid model. Suggested 21600 |
| `SCHEMA_STRICT` | unset | set nowhere; the boot presence probe therefore can never abort. Enabling it is a separate decision |

- `GET /api/admin/liveness` — reports the last sweep; `status: never_run` until the first completes, because absence must not read as a pass.
- Migration **212** (`member_delta`) must be applied before Loop 4 accepts behave as documented.

---

## 7. Known unrelated issues in the tree

- `cargo check --workspace --tests` hits a **rustc SIGSEGV** in `gpui_macros` compiling `fermi-console`. Confirmed pre-existing by stashing all changes and reproducing.
- 46 Dependabot vulnerabilities reported on push (14 high).

---

## 8. A mistake worth repeating back

I twice reported "no database is reachable" and shaped decisions around it. It
was reachable the whole time; I had run `grep -oE "^[A-Z_]+="`, which strips the
values, and concluded absence from a measurement that could not answer the
question.

That is the defect class of the paper, committed while working on the paper's
own remedy. It cost a session of unnecessary "unverified" caveats. The general
form — *a plausible reading of an artifact, reported as a fact about the
system* — is what §5.8 means by "reading the code proves nothing", and it
applies to reading one's own tooling too.

**Everything in §3 is now verified against production. Everything in §4 is not.**

---

## 9. Session 2 — what was found and what changed

**Date:** 2026-08-22 (later) · database reachable throughout, every number below
measured against it.

### 9.1 The finding

Three independent breaks in Loop 5b, none visible from either side alone.

**The producer and every reader disagreed about what a projection looks like.**
The dynamics runner tags `extra.source_kind = "dynamics_projection"`. All three
readers — `eval_projection`, `simops_benchmark`, `observations` — matched
`extra.source = "simops_simulation"`, which is written only by an agent tool
that has produced **zero** observations. 12,167 projection rows on file, 0
carrying the tag every reader looked for. No reader was wrong on its own terms;
the defect exists only across files, and only against row counts.

**The commitment anchor had no callers.** `commit_projection` writes the row
that proves a prediction pre-dated its measurement — the thing that makes Loop
5b a verification rather than a transcription. The site that should have called
it was a `let _ = (…every argument…)` in `simops_tools`, annotated "hooks for an
observability path that may or may not be live", and both arms of the enclosing
`if` returned `None`, so the tool reported the same null commitment hash whether
a clock had started or not. It was also on the wrong path: projections arrive
over HTTP, not through the agent tool.

**Both lookup queries were unrunnable.** The primary bound a `Uuid` against
`produced_by_agent_id`, which is `TEXT`; the fallback bound a `TIMESTAMPTZ`
against `phenomenon_time`, which is `BIGINT`. Both are hard Postgres errors.
Neither had ever surfaced, because the `source` predicate above them matched no
rows, so the comparison was never reached with anything to filter.

### 9.2 Fixed

| Area | Was | Now |
|---|---|---|
| **Projection predicate** | three hand-rolled copies, all matching a tag with 0 rows | `src/projection_kind.rs` — one Rust predicate and one SQL macro from the same constants; `tests/projection_predicate_coverage.rs` fences the literals |
| **Commitment anchor** | dead code behind a module boundary | `src/projection_commit.rs` in the lib; called from **both** the HTTP ingest (where the projections actually arrive) and the agent tool |
| **Lookup queries** | two latent type errors, never executed | fixed, and `both_lookup_queries_execute_against_the_real_schema` runs them against the live schema |
| **The 30-day fallback** | on by default; would score a measurement against a projection it never answered | off unless requested by name. §4.3's ordering rule is now a control instead of a sentence in a document |
| **Loop 5b liveness** | one contract, `0 / 12,167`, remediation pointing at the wrong link | three contracts, one per link; `report_where_projection_calibration_stops` prints the chain |
| **Migration 212** | on disk, **never registered** in `run_migrations()`, so `member_delta` does not exist in production while `composition_evolution.rs` binds to it | registered; validated by applying and rolling back against production |
| **Γ threshold** | `abw_logical_architecture.md` §3.2 stated `Γ(C) ≥ 0.5` | corrected to what the gate tests, with the measurement that settles it |

### 9.3 Baseline after the change

```
6 live · 5 inert · 0 excused · 0 silent · 0 unrunnable
```

The suite passes, and that is a stronger statement than it looks: the one
`SILENT` it used to carry was the phantom. The five `INERT`s are honest — no
opportunity has occurred — and `INERT` is still not a pass.

| sink | writes | opps | status |
|---|---|---|---|
| consolidation_jobs (Loop 1 cadence) | 31 | 49 | OK |
| process_projection_commits (5b · anchor) | 0 | 0 | INERT |
| process_spacetime (5b · resolution) | 0 | 0 | INERT |
| eval_signals.projection_accuracy (5b · scoring) | 0 | 0 | INERT |
| forecast_agent_claims | 0 | 0 | INERT |
| semantic_rules.application_count | 27 | 2,098 | OK |
| episodes.assertions | 144 | 65 | OK |
| assertion_verifications | 0 | 0 | INERT |
| schema_migrations | 214 | 3,544 | OK |
| agent_timeline_entries | 1,411 | 3,544 | OK |
| semantic_rules | 248 | 2,326 | OK |
| anomaly_events | 0 | 1,411 | SILENT (conditional) |

The anchor rung counts only projections generated after the commit call site
existed (`COMMIT_HOOK_LIVE_FROM`). The 61 historical runs are deliberately not
counted as missed: an anchor written after the measurement proves nothing, so
backfilling them would manufacture precisely the evidence Loop 5b exists to make
unmanufacturable.

### 9.4 §5's rule, applied

Every check added or changed was broken and watched go red:

- `the_shape_that_fills_the_table_is_recognised_as_a_projection` — reverted the
  predicate to `source`-only; failed.
- `the_scoring_rung_counts_resolved_pairs_and_not_projections` — restored the
  old `opportunity_sql`; failed with the 12,167 query quoted back.
- `both_lookup_queries_execute_against_the_real_schema` — restored the `Uuid`
  bind; failed with `operator does not exist: text = uuid`, the original defect
  verbatim.
- `only_one_module_names_the_projection_tags` — needed no deliberate break: it
  went red on its first run against a real hit (prose in a contract quoting the
  tag).

### 9.5 Still open

1. **Loop 5b link 5** — nothing triggers the evaluator from the resolution hook.
   Both observations are in hand there. Worth doing, but it produces nothing
   until a measurement stream overlaps a projected property, so it is no longer
   the top of the list.
2. **Loop 5b's real blocker** — no measurement exists for anything projected.
   This is an operational question (which sensors, which twin), not a code one.
3. **`produced_by_agent_id` is NULL on all 19,743 observations.** The reader
   scopes projections to the producing agent, so `n_prior` reads 0 for every
   projection and the heuristic path selects nothing whatever the binding. A
   provenance gap in the writers.
4. **`anomaly_events`** — addressed in §10.
5. **§4.1, §4.2, §4.4** — unchanged. Note that §4.2's regex half is already
   done: the live census reports 51 of 65 lines parseable by the *old* pattern,
   which is the measurement, not the current behaviour; `assertions.rs` v2
   recovers all of them and 144 episodes carry assertions.
6. **Migration 212 applies at next deploy.** Validated in a rolled-back
   transaction; not applied to production by hand, because the boot path is the
   thing that should be shown to work.

### 9.6 A note on §8

§8 records mistaking an unreadable measurement for an absent database. The
symmetric error is in this session's finding: a *readable* number, correctly
computed, answering a question nobody had checked it was answering. 12,167 was
never wrong as a count. It was wrong as evidence, and it had already shaped a
plan by the time anyone looked at what it counted.

The habit that catches it is cheap and was not expensive here: before believing
an opportunity count, `GROUP BY` the population it draws from and read one row
of it.

---

## 10. The Loop 2 seed was rejected by the database

### 10.1 The finding

§4.1 says to watch `anomaly_events` after the next traffic, and gives the chain
to expect. The rows were never going to arrive.

`3e6c9e08` — "the loop required its own output as its input" — raises a
`grounding` anomaly when the grounding contract finds a violation. It builds the
event with:

```rust
kind:     "grounding".to_string(),
severity: "L1".to_string(),   // "a reviewable defect in one output"
```

The column says:

```sql
severity TEXT NOT NULL DEFAULT 'warning'
    CHECK (severity IN ('info', 'warning', 'critical'))
```

**`L1` is not in that set.** Verified against production — the exact row the
handler constructs fails on `anomaly_events_severity_check`, and the same row
with `warning` inserts. The write is `tokio::spawn`ed and its error is
`tracing::warn!`ed, so the request succeeded, the log line scrolled past, and
the table stayed at zero.

The seed planted to break Loop 2's deadlock could not germinate, and the
handover recorded the remedy as "wait and see".

`L1` was not careless. It is a coherent severity scheme, and it is a *second*
scheme for a column that already had one — `assertions.rs`'s "One ladder, not
two", with a CHECK constraint as the thing that disagreed.

### 10.2 What the zero actually means

With the write path repaired, the row count still reads 0 / 1,417, and that is
now a finding about the world rather than about the code:

- **262 of 1,417** timeline entries carry a flag.
- Every one of them is `social:observed` — bookkeeping, matched by no detector,
  by design.
- The four detector prefixes (`safety:`, `drift:`, `conflict:`, `rupture:`) have
  **never** appeared. `RollingConflict` is structurally impossible at the
  deterministic evaluator set (disjoint dimensions, documented in
  `live_observability`); the other three have simply never triggered.
- The scanner is healthy: 94 agents, latest scan minutes old, **zero** unscanned
  backlog.

So the detectors are working and nothing actionable has ever been flagged. The
open question moved upstream: **WildGuard has never returned a safety flag on
live traffic**, and that should be settled by feeding it something it must flag,
not by waiting.

### 10.3 What was built

`src/anomaly_vocabulary.rs` — one declared vocabulary for kinds, severities and
flag prefixes, with the bookkeeping exemptions carrying reasons.

`tests/anomaly_firing_probe.rs` — the probe the `anomaly_events` remediation
asked for. It does **not** assert that anomalies exist. It asserts that *if one
occurred it would be recorded*, which is the only half a test can own and the
half that was false:

| test | what it settles |
|---|---|
| `every_declared_kind_and_severity_is_accepted_by_the_table` | no writer can construct a row the database refuses — the incident |
| `the_invented_severity_is_still_rejected` | the fix was to use the platform's vocabulary, not to widen the CHECK |
| `the_table_accepts_nothing_the_vocabulary_omits` | the reverse drift — migration 200 widened the CHECK for `grounding` and no `AnomalyKind` variant was ever added |
| `every_flag_written_is_one_a_detector_reads_or_a_declared_no_op` | a producer emitting `harmful:` where the detector reads `safety:` has no symptom at all; this is the symptom |
| `the_flag_census_has_something_to_look_at` | positive control — 262 flagged entries, so the census is not a check over an empty set |

All five run from `scripts/liveness_contract_live.sh` as a third "firing tier",
after the offline and live tiers.

### 10.4 §5's rule, applied

- added `L1` to `SEVERITIES` → `every_declared_kind_and_severity_is_accepted_by_the_table`
  and `the_invented_severity_is_still_rejected` both failed, quoting the exact
  production constraint error five times over;
- removed `KIND_GROUNDING` from `KINDS` → `the_table_accepts_nothing_the_vocabulary_omits`
  failed with `the table accepts ["grounding"], which fermi::anomaly_vocabulary
  does not declare`;
- renamed the `social:` bookkeeping exemption →
  `every_flag_written_is_one_a_detector_reads_or_a_declared_no_op` failed with
  `social:observed 262 -> NO DETECTOR`.

### 10.5 What this says about `Conditional`

`Conditional` is the right expectation for a detector sink and it has a cost
that was not being paid: it makes the sink's zero **unfalsifiable**, which is
the same standing as a scan that cannot go red. The resolution is not to assert
the row count — that would assert anomalies must exist — but to assert the
*recordability* separately. Any future `Conditional` contract should ship with
its firing tier, or its zero means nothing.

### 10.6 Still open on Loop 2

The deadlock in §4.1 is intact until a grounding violation actually occurs on
live traffic. The seed can now write; nothing has yet given it something to
write. Watch `anomaly_events` — and this time the instruction is sound, because
the path underneath it has been shown to carry a row.
