# Handoff — closing the loops and gates

**Date:** 2026-08-22 · **Branch:** `main` · **Commits:** `014e0a58`, `ba398849`, `dc39df72`, `3e6c9e08`

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
