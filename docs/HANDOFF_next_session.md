# Handoff — next session

**Date:** 2026-08-28
**References:** `docs/UX_HANDOFF_trust_surfaces.md` (the contract with the UI
team, 1,100 lines, current) · `docs/UX_RESPONSE_belt_outcomes.md` (this
session's reply) · `docs/AUDIT_loops_and_gates.md` (the original reference)

Everything in the previous handoff's Track A and Track B is either done or
superseded. This is a fresh triage.

---

## State

`8b3ed2c6` is on `origin/main`. **882 lib tests, 74 integration suites, zero
failures.**

Thirteen endpoints serve the loop / gate / trace surfaces. The UI team has
validated three of four earlier rounds against the live database themselves.

**Migrations 219, 220, 221 are registered and apply at boot. The deploy is
authorised and expected.** Everything below assumes it has happened; if it has
not, `gate_decisions` is still empty and every checkpoint rung correctly reports
`predates_retention`.

---

## Start here: confirm the ledger fills

This is the whole of v1's remaining risk, and it is an *observation*, not a
build. Nothing in the trust surfaces has ever been validated against data —
`gate_decisions` and `assertion_verifications` have both held **0 rows** for the
entire life of this work, so every surface was validated by construction. That is
why it took as long as it did, and it is why the first hour after deploy is worth
more than the next week of building.

After the first traffic, check in this order:

1. **`SELECT gate, decision, count(*) FROM gate_decisions GROUP BY 1,2`** — the
   recorder flushes every 15s from `api_server.rs:2571`. Expect `grounding` to be
   dominated by `undetermined`; migration 221 predicts ~3,065 of them, because an
   agent with no field contract has nothing to grade.
2. **`GET /api/episodes/:id/trace`** on a recent episode — the grounding rung
   should carry `decided`, not `decided_absent`. If it still reports
   `retained_but_absent` while rows exist, the `episode_id` binding is wrong and
   `tests/gate_decision_lineage.rs` is the check that should have caught it.
3. **`write_accounting`** for `Sink::GateDecisions` — attempts without failures.
   A failed flush **requeues** rather than dropping, so a persistent failure shows
   as a growing queue, not as silence.
4. **The drift finding.** Once both exist, look for a rung with
   `decided.decision == "approved"` beside `recomputed.violations > 0`. That is
   the platform's only finding about its own drift and nobody has ever seen one.

If attempts appear and failures equal attempts, the report says `rejected` with
the database's own error attached. That is the point of the whole recorder.

---

## Then: the two things the UI team is owed

1. **A route discriminator on `episodes`.** `checkpoint_route.recoverable` is `false`
   because the two execute commands declare **different checkpoints** — `agent.execute`
   four rungs, `agent.execute_stream` two — and nothing records which one an
   artifact travelled. The trace serves the wider route and says so. This is a
   column plus a bind at two write sites, and it turns an unverified safety claim
   into a verified one. **Small, and the UI team was asked whether they want it.**

2. **`templates/trace.html` is broken** and has been for longer than this
   session: it reads `o.outcome === "graded"`, and `Outcome` no longer exists.
   The UI team owns it and estimated "under a day". Do not fix it under them
   without asking — it is the file they are actively reworking.

---

## The Python question, raised and unanswered

The owner asked why there is Python in a Rust stack. Measured, not assumed:

* **The serving path has zero Python.** No `Dockerfile` reference, no `.rs` file
  shells out to it, no handler or migration runner touches it.
* **`scripts/break_*.py` (9 files)** are break-harnesses — they edit a Rust file,
  assert the break applied, run `cargo test`, require red *naming the right
  test*, revert, and `os.utime` to defeat the stale fingerprint cache. Dev-only,
  never in CI. Defensible; portable if wanted.
* **`.github/workflows/ci.yml:147` runs `python3 scripts/lint-schema-consistency.py`.**
  A **build gate written in Python.** This one is fairly on the execution path.
  It is a repo-walking scan, which is exactly what `SCANS` in
  `tests/falsification_registry.rs` already governs, so porting it to a `tests/`
  suite gets it a named falsifier for free. **Clean, self-contained, recommended.**
* **`tests/taxonomy_parity.rs` and `tests/port_binding_parity.rs` treat Python as
  the oracle.** `scripts/taxonomy.py --emit-expected` generates
  `agents/taxonomy_derived_expected.json`; the Rust test asserts
  `taxonomy::derive()` matches it. Failure messages read `python={} rust={}`.
  **Two implementations of one question with the Python authoritative**, and the
  fixture is refreshed by hand — so the gate silently weakens the moment someone
  regenerates the fixture instead of fixing the Rust.

That last one is the real finding. It is plausibly a **retirement gate from an
unfinished Python→Rust port**, in which case it is correct to have and deleting
it early is worse. **Needs the owner: is the port finished?** If yes, delete both
scripts and both fixtures and let the Rust stand alone.

---

## Method — the part worth carrying

Unchanged from the last handoff and reconfirmed three times this session:

* **A break must assert that it applied before its result is read.** Every edit
  over ~100 lines went through a script asserting `count(OLD) == 1`, because
  `edit_file` has silently truncated long replacements and corrupted a file
  mid-module in this repo.
* **Measure the population before believing a count.** It stopped the wrong thing
  being built again: a fifth `decided_absent` token was drafted for the
  "agent declares no field contract" case before anyone checked what
  `execution.rs:470` already did. It already recorded `Undetermined`. The variant
  being removed was compensating for an empty database.
* **A vacuity guard should be a measurement, not a guess.** The new checkpoint test
  asserted `checked >= 8` on the assumption that both execute routes matched. The
  real number is 6, the guard went red, and that is how the differing-routes defect
  was found. **The guess was the finding.**
* **A scan must be no broader than the property it asserts.**
  `provenance_floor_coverage` skipped `.git` by name and walked every other
  dot-directory, including a 5.2GB second copy of the repo in `.release-verify/`.
  It reported findings in files that are not this build's source.
* **Before writing a scan, check whether the compiler already owns the
  property.** Breaking `GateSpec::decides_before_the_artifact` is a *compile*
  error at every construction site, which is stronger than any scan over it.

And the one that keeps recurring in a different costume: **a true fact about an
artifact reported as a fact about the system.** This session's instance was
reading an empty `gate_decisions` as permanent architecture and nearly spending a
token on it.

---

## Do not

* **Do not repoint `agent_loops_handler`.** 610 lines of bespoke SQL giving a
  second answer to a question `loop_model` answers from the contracts. Let the
  model earn production time first.
* **Do not delete `hud_contract`** without asking. A thousand lines of display
  gate with no production caller is either dead code or an unwired safety
  control, and that is the owner's call.
* **Do not touch the parallel session's files** without checking mtime and
  `git show HEAD:<file>` first: `crates/fermi-console/src/cockpit.rs`,
  `templates/loops.html`, `templates/trace.html`, `tests/weather_composition.rs`,
  `src/handlers/specimen.rs`. It has broken the build under us roughly six times.
* **Do not retrofit or prune the legacy agent fleet yet.** It is a real and
  scheduled effort, and it **depends on** this infrastructure working, not the
  other way round. `declaration_ladder::disposition` already separates the two
  worklists — 110 `prune`, the rest `retrofit`.

---

## Known non-defects

* `sensitivity::tests::first_order_indices_do_not_over_explain_the_variance` is an
  **unseeded Monte Carlo flake**; passes 5/5 in isolation.
* `native_evaluators::loop_stalled_in_code` over-claims. Caveated in
  `evaluator_api::EVALUATOR_CAVEATS` rather than fixed, because narrowing it flips
  a live verdict platform-wide. Owner's call.
* `tests/weather_composition.rs` has 2 fmt diffs — not ours, unmodified in git.

---

## Still open, from the audit's §9

3. **Grounding is a metric, not a control, on the two general execute paths.**
   `enforce` mutates a local dropped two lines later. It *is* a control on the
   creature handlers. The paper's §4 claims the opposite of what the primary
   endpoints do. **This is now visible to users** — `checkpoints[].enforcement` is
   `metric` with `why_not_control` carrying our own words — which raises the
   question of whether it should be fixed rather than merely disclosed.
4. `hud_contract::enforce` has no production caller.
5. `delegate_to_agent` has no grounding gate at all.
6. `agent_loops_handler` duplication (above).
