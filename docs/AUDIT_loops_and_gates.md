# Audit — the loop and gate system, end to end

> **Picking this up cold? Read `docs/HANDOFF_next_session.md` first.** It is 176
> lines and tells you what to do; this is 1,184 and tells you why. The sections
> worth reading here on their own are §9 (priority list), §19 (how a correctly
> computed number came to answer the wrong question) and §21 (state of the tree).

> **Terminology note (2026-08-23):** loop numbering in this document predates the settled taxonomy. Routing is now Loop 4.B (was Loop 5); BayesOps parameter fitting is now Loop 5.B (was "Loop A"); SimOps projection accuracy is now a signal path of Loop 5.A (was 5b). See `docs/architecture/FEEDBACK_LOOPS.md`.

**Date:** 2026-08-22 · **Method:** static read + live database · **Status:** read-only. No code changed in this pass.

Scope set by the request: audit the five feedback loops and every gate end to end,
confirm the wiring, check terminology against
`docs/papers/verification_for_agent_ecologies.md`, and establish what the
observability platform can and cannot say about the system's states.

**Verification convention.** Every finding is marked:

* **[V]** — I verified it myself, against the code or the live database, in this pass.
* **[R]** — reported by a delegated enumeration pass and *not* independently confirmed.

The distinction is the paper's §5.8 applied to my own tooling. Nothing in the
priority list below is `[R]`.

---

## 1. The loops, measured

Live row counts, 2026-08-22. **[V]**

| Loop | chain, in order | state |
|---|---|---|
| **1** agent learning | episodes → `consolidation_jobs` 282 → `semantic_rules` 253 → `application_count` 37 | **turning** |
| **2** human-gated correction | `anomaly_events` **0** → `hitl_actions` **0** → `two_reviewer_requests` **0** → `episode_corrections` **0** | **never turned, at any stage** |
| **3** workspace coherence | `coherence_evaluations` 25 → `pairwise_coherence` **0** · `workspace_intentions` **0** | first stage only |
| **4** composition evolution | `composition_versions` **0** → `forecast_attributions` **0** | **never turned, at any stage** |
| **5a** Brier | `forecast_commitments` 1,354 → `forecast_spacetime` 2,179 → signals 236 | **turning** |
| **5b** projection | 61 projections → commits 0 → spacetime 0 → signals 0 | never had an input |

Two of five turn. Loop 2 and Loop 4 have produced zero rows at *every* stage of
their chains — not a slow loop, an unstarted one.

---

## 2. The finding that matters most

### 2.1 Loop 2's seed still cannot write **[V]**

Last session found that the grounding anomaly — the seed committed to break Loop
2's deadlock — wrote `severity = "L1"` against a CHECK of
`('info','warning','critical')`, so every insert was rejected. That was fixed.

**It is still rejected, for a second and independent reason.**

```
src/handlers/execution.rs:219   let episode_id = uuid::Uuid::new_v4();   // minted, no row
src/handlers/execution.rs:397   tokio::spawn(… create_anomaly_event …)   // references episode_id
src/handlers/execution.rs:411   episode.episode_id = episode_id;
src/handlers/execution.rs:494   store_episode_with_provenance(episode)   // the row finally exists
```

`anomaly_events.episode_id` is a real foreign key
(`migrations/105_longitudinal_observability.sql:139`). The detached task at
`:397` races the write at `:494`, and everything between them — including
embedding generation — is work the spawned task does not have to wait for.

Confirmed against production, in a rolled-back transaction:

```
ERROR:  insert or update on table "anomaly_events"
        violates foreign key constraint "anomaly_events_episode_id_fkey"
```

The only trace is `tracing::warn!` at `:399`.

Three things make this the emblematic finding of the audit:

1. **The same swallow hid both defects.** Fixing the vocabulary was necessary
   and did not make the seed work, and nothing would have told us.
2. **The correct answer already exists in the same file.**
   `forecast_agent_claims.episode_id` was deliberately left FK-free for exactly
   this race — `migrations/197_claim_episode_correlation.sql:39-46`: *"integrity
   is enforced by construction rather than by constraint"* — and the comment at
   `execution.rs:274-277` describes the race explicitly. Same handler, same
   minted id, two opposite conclusions.
3. It is a **fifth** instance of the class in the same 200 lines of code.

### 2.2 No gate decision is persisted anywhere **[V]**

This is the precondition for everything the request asks for, and it is absent.

| gate | what happens when it refuses | trace |
|---|---|---|
| coherence gate `Blocked` | `gate.rs:234`/`:241` → HTTP 422 at `observatory.rs:485` | **none** — no row, no log, no counter |
| every credit refusal | returns before the `credit_ledger` INSERT (`fermi-auth/src/credits.rs:273`, `:345`) | **none** |
| all three rate limiters | in-memory `DashMap` (`api_server.rs:99`, `:122-128`) | **none** |
| publish refusal | `publish_pipeline.rs:136` → 400 | **none** — though the *bypass* is audited to `admin_bypass_events` |
| attachment refusal | `attachments.rs:309` → 400 | **none** |
| royalty withheld | `gas.rs:278`, `:281`, `:284` return `Ok((fee, 0))` | **none** |
| lifecycle transition refusal | `agent_state.rs:20` | **none** |

**The platform has a record of every request it served and none of any it
refused.** That is why the Γ arithmetic bug — a gate rejecting 100% of
agent-wide interventions — survived: there was no record it had ever been asked.

---

## 3. Gates that are computed and discarded **[V]**

| gate | site | what actually happens |
|---|---|---|
| consensus coherence re-check | `observatory.rs:711` | calls `gate.check`, which is `check_against(_, &WorldModel::default())`; `is_sufficient()` is false, so it returns `Undetermined` **on every call**. Never branched on. Overwrites the first reviewer's real verdict in `episode_corrections.coherence_check`. |
| grounding on `/execute` | `execution.rs:330` | `enforce` mutates a local `doc` dropped at `:332`. The persisted `response_text` and the rendered body are both un-stripped. |
| grounding on `/execute/stream` | `execution_stream.rs:276` | same, and no anomaly is raised either |
| bind check | `execution.rs:431`, `execution_stream.rs:299` | `if verified.is_mismatch()` guards a `warn!` and nothing else; control flow is identical either way |
| **the entire HUD contract** | `hud_contract.rs:495` | a ~1,000-line display gate that nulls unsourced safety prose and overwrites model-claimed confidence. **Zero production call sites** — only `tests/hud_contract.rs` and `examples/hud_preview.rs`. |
| `min_tier` / `capability_gates` | `agent_card.rs:288-299` | typed, persisted (`agents.rs:1173`), exposed (`agents.rs:109`), **never compared against anything** |

On grounding specifically, the paper's claim is:

> Grounding runs before anything persists or renders. Not as a report afterward.
> The check that runs after the write is a metric; the check that runs before it
> is a control.

On the creature and wild handlers it is a control. **On the two general-purpose
HTTP execute endpoints — the ones a third party actually calls — it is a
metric**, and that is documented as intentional at `api_server.rs:6232-6234`.
On `delegate_to_agent` (`tools_legacy.rs:6398`) there is no grounding gate at
all. **[R, but the call-site list is [V]]**

---

## 4. Loop 3 and Loop 4 — first audit

### Loop 4's declared driver executes nothing **[V]**

`composition_dream_handler` (`src/handlers/composition.rs:304`) builds a
`@cohere_and_coordinate [COMPOSITION DREAMING]` prompt, then:

```rust
// Post as a workspace message — routes through normal execute flow,
// response arrives via SSE stream.
state.memory_store.store_workspace_message(&msg).await?;   // a bare INSERT
```

It does neither thing the comment claims. `@`-mention parsing and agent
execution live only in `post_workspace_message_handler`
(`workspace/messages.rs:256-333`), which this bypasses; nothing in the codebase
consumes `workspace_messages` rows of `message_type = 'agent_invocation'`. It
then charges 5 credits (`:396`) and returns `"status": "dreaming_initiated"`.

So `propose_composition_change` (`tools_legacy.rs:2807`) has no reachable
caller, and the entire accept/reject machinery below it is built and starved.
`composition_versions` = 0 follows directly.

### Loop 3's automatic half is measurement-only **[R]**

The auto-eval at `workspace/messages.rs:760-865` settles TEC and stores the
evaluation — that is the 25 rows — and stops. Every strategist invocation and
every coordination brief lives behind `depth == "recommendations"` at
`workspace/coherence.rs:237`, a 2–5 credit manual button. Γ is computed and
nothing acts on it.

### Two dead declarations **[V]**

* **Loop 3 Stage 0 (intentions):** six tools implemented and wired to dispatch
  (`tools_legacy.rs:3252-3522`, `:2722-2727`), exposed to every workspace agent,
  and **no prompt anywhere asks for them**. `workspace_intentions` = 0.
  *(Prompted 2026-08-16, and the rows that followed turned out to be the
  coordinator's guesses about its members rather than the members' plans — see
  §22. This bullet was right about the count and wrong about what fixing it
  would buy.)*
* **`pairwise_coherence`:** table (`migrations/049:19`), foreign keys
  (`migrations/169:43-44`), two indexes — and **no Rust reader or writer of any
  kind**. The single occurrence in `src/` is a comment.

---

## 5. Swallowed failures — the class that hides the others

**30 distinct sites** write to loop/verification sinks with a swallowed failure,
covering **15 of 22** such tables. **There is no failure counter anywhere in the
repository** — no dead-letter table, no retry queue, no metrics crate in
`Cargo.toml`. Every failure path terminates at `tracing::warn!`,
`tracing::error!`, `eprintln!`, or nothing. **[R]**

The nearest thing is `schema_migrations` (`migrations/207`), which has
`failures` and `consecutive_failures` columns — for migrations only, and its own
write failure is `eprintln!`ed. **[V]**

Worst individual sites **[R]**:

* `simops_benchmark.rs:182` — `process_spacetime` INSERT with two CHECK
  vocabularies supplied as bare string literals, `if r.is_ok() { written += 1 }`
  at `:220`, **no log of any kind**, and the count discarded by `let _ =` at
  `observations.rs:645`. The only loop sink with zero observability on failure.
* `workspace/messages.rs:565`+`:575` and `rabble_workspace.rs:426`+`:436` — a
  swallowed `episodes` write immediately followed by an unguarded timeline write
  against that episode's FK. One failure, two tables lost, no signal.
  `execution_stream.rs:353-367` does the same sequence correctly, so the guard is
  known and applied at one of three sites.
* `workspace/messages.rs:836` — `if let Ok(eval_id) = …` does not bind the error
  at all.

---

## 6. Terminology drift against the paper

The paper names five rungs. The code names three of them after their mechanism
instead, and the modules' self-declared ordinals contradict the paper on three
of five. **[R, spot-checked [V]]**

| rung | paper | module | drift |
|---|---|---|---|
| 1 Presence | Presence | `schema_trust` | named for substrate; vocabulary is `missing`/`drift` |
| 2 Liveness | Liveness | `liveness_trust` | term OK; calls itself "the **fifth** trust contract" |
| 3 Truth | Truth | `rollup_trust` | named for the denormalisation mechanism; no `Truth` anywhere |
| 4 Grounding | Grounding | `grounding_trust` | term OK; calls itself the "**Third** sibling" |
| 5 Binding | Binding | `port_trust` | module says "port", type says `Binding`; calls itself "**Fourth**" |

Further, all **[R]**:

* `Status` has **five** liveness verdicts where the paper defines three, and
  `sweep` then folds `NotDeployed` into the `inert` bucket, so the report cannot
  express the distinction its own enum draws.
* `card_contract.rs:53` declares a **four**-member grounding vocabulary against
  the runtime's **five**-member `Grounding` enum. `derived` has no card token, so
  the runtime can emit a verdict no card author can declare — the exact shape of
  the `gbif_verified`/`tool_verified` incident.
* **§3.4 is violated in two places.** `hud_contract.rs:337`/`:352` is a second
  strength ordinal and a second min-over-sources (self-documented, and live —
  it computes the card floor). `provenance_oracle.rs:80`/`:87-120` is a second
  floor+ceiling that does *not* call `extracted_floor` and reinstates the
  tier-collapse the canonical version documents as fixed. **[V]**
* One concept, `unsourced`/`unavailable`, is spelled four different ways across
  the enum, the card token, the provenance token and the display word.

---

## 7. The defect class, restated

Every finding sits on a **seam** — a boundary where each side is independently
correct — and takes one of six forms:

| form | example |
|---|---|
| **vocabulary drift** | `L1` vs `warning`; `source_kind` vs `source`; four spellings of *unsourced* |
| **unexecuted path** | `commit_projection`, migration 212, `hud_contract::enforce`, `pairwise_coherence` |
| **swallowed failure** | 30 sites, 15 sinks, zero counters |
| **proxy assertion** | `contains(".with_provenance_oracle(")`; the 12,167 |
| **fatal gate** | Γ rejecting 100% for arithmetic reasons |
| **discarded verdict** | `enforce` mutating a dropped local; the consensus re-check |

They share one property: **absence is the success signal.** Working and broken
produce the same observable — nothing.

And there is a causal ordering that tells us where to intervene:

> **Swallowed failure is what hides the other five.** The `L1` bug was
> vocabulary drift; it was *invisible* because the insert was spawned and its
> error logged. The FK race is the same bug in the same statement, and it is
> invisible for the same reason. Fixing instances one at a time has now twice
> produced a new instance of the same class in the same code.

The remedy is therefore not another instance-fix. It is to make the class
observable.

---

## 8. Proposed build, in dependency order

1. **Failure accounting.** One sink every non-fatal side write reports
   `(path, sink, outcome, error_class)` into. Then liveness watches *attempts*
   as well as successes, and `0 succeeded / 340 attempted` is a different and
   immediately actionable statement from `0`. This is first because it is what
   makes everything else visible.
2. **Gate decision ledger.** Every gate — broad reading, including credit, rate
   and admission — writes `(gate, subject, verdict, reason, measured)`.
   "Approved 0 of 47" becomes visible on day one.
3. **Seam vocabularies.** Generalise `projection_kind` / `anomaly_vocabulary`:
   producer tokens ⊆ consumer tokens ⊆ schema tokens, asserted two-way against
   the live database. Start with `process_spacetime`, whose CHECK vocabularies
   are bare literals today.
4. **A declared loop state model.** One const table — stages, transitions,
   gates, sinks — in the shape of `LIVENESS_CONTRACTS`, with the observatory
   surface *derived* from it rather than hand-written. `agent_loops_handler` is
   610 lines of bespoke per-loop SQL in a 3,666-line file, and is a place where
   the reported verdict can drift from the contracts.
5. **A native evaluator registry** — separate from the pluggable agent-output
   registry — whose evaluators score the loop and gate machinery itself, fed by
   (1)–(4).
6. **Terminology reconciliation.** Rename to the paper's rung names, collapse
   the duplicate trust arithmetic, and align the card and runtime vocabularies.
7. **A falsification registry.** Added after steps 1–6 landed, because building
   them produced the evidence for it: three of the checks written in this audit
   could not catch their own motivating case, and that was found by breaking
   them by hand. **Nothing in the build enforces that a check has ever gone
   red.** `native_evaluators::every_evaluator_can_produce_a_finding` is the one
   place it is structural, and only because evaluators are pure functions over
   a snapshot. Generalising that shape is what stops the discipline being a
   property of whoever happens to be working. Specced in
   `docs/HANDOFF_next_session.md` — Track A.

---

## 9. Priority list

| # | finding | consequence | verified |
|---|---|---|---|
| 1 | Loop 2's seed **would** lose an FK race on its first violation — see §19, it has never been attempted | Loop 2 cannot start | **[V]** |
| 2 | No gate decision is persisted anywhere | no gate's behaviour is knowable | **[V]** |
| 3 | Loop 4's driver executes nothing and charges 5 credits | Loop 4 cannot start | **[V]** |
| 4 | 30 swallowed writes, 15 sinks, no failure counter | the class stays invisible | [R] |
| 5 | Grounding is a metric, not a control, on both general execute paths | the paper's central claim is false where it matters most | **[V]** |
| 6 | `hud_contract::enforce` has no production caller | a whole safety gate is dead | **[V]** |
| 7 | Loop 3's correction half is behind a paid manual button | Loop 3 measures and does not correct | [R] |
| 8 | `delegate_to_agent` has no grounding gate | agent-to-agent output ungated | [R] |
| 9 | Two duplicate trust arithmetics (§3.4) | the copy nearest the writer wins | **[V]** |
| 10 | Rung naming, verdict counts and card vocabulary drift from the paper | the terms cannot be relied on | [R] |

---

## 10. Build log — step 1 of 6: failure accounting

**Landed.** `src/write_accounting.rs`, wired into `liveness_trust`, adopted at
15 sites, fenced by `tests/write_accounting_coverage.rs`.

### What it changes

Liveness asks whether a sink has rows. This asks whether anybody tried. Both
read `Silent` from outside and they have opposite remedies, so the pair is what
separates *a missing scheduler* from *a statement the database refuses*.

The report gained one field and no new verdict. **`Status` still has the
paper's three answers**, because attempts are a different question from rows and
folding them into one enum is how five verdicts came to occupy four buckets.
Instead each outcome carries a `diagnosis`:

| counters | diagnosis |
|---|---|
| 0 attempts | `never_attempted` |
| all attempts refused | `rejected` |
| some refused | `partially_rejected` |
| no counters at all | `uninstrumented` — **not** a clean bill |

And `LivenessReport::rejected` fails `is_healthy()` **outside every exemption**.
A sink may be excused for being empty — `KNOWN_SILENT`, `Conditional`, `Inert` —
and none of those readings survives a refused write. That is also what finally
makes a `Conditional` contract falsifiable: asserting on `anomaly_events`' row
count would assert that anomalies must exist, whereas asserting its writer is
not being refused asserts nothing about the world.

`GET /api/admin/liveness` reads the counters at request time rather than from
the hourly snapshot, and ranks `writes_refused` above `degraded`.

### Why in memory

A failure ledger that is itself a fallible database write has the property it
exists to detect: when the database refuses the anomaly it also refuses the
record of the refusal. Atomic counters cannot fail, cannot recurse and need no
migration. A restart clears them, which matters for a trend and not for *"340
attempts today, none succeeded"*.

### Coverage, and its honest limits

* `every_declared_sink_is_instrumented_somewhere` — exact. All 15 sinks have a
  call site.
* `uninstrumented_swallows_may_only_decrease` — a **burn-down ratchet**, at
  `episodes: 2, semantic_rules: 1`. Asserting zero today would be a false claim
  or a suppression list, and §5.2 says what happens to a check that fires on
  correct behaviour. The baseline may only shrink, and the test says so when it
  is stale.
* The ratchet sees raw SQL writes, not writes through a store method. Stated
  rather than left to be discovered.

### Two things the checks caught in their own author

Both are the audit's defect class, committed while building the remedy for it.

1. **The exactness test was a proxy assertion.** It searched whole files for
   `Sink::X` — and `liveness_trust` names every variant in its `accounted:`
   field. A declaration satisfied a check about call sites. Now a variant only
   counts within three lines of an `observe(` or `record(`, and the declaration
   site is excluded.
2. **The swallow detector could not see the shape behind every finding.** It
   matched `let _ =` and `.ok()` and not `if let Err(e) = … { tracing::warn!(…) }`,
   which is the commonest form in this codebase and the form of the rejected
   severity, the foreign-key race and the unbound coherence error alike. With
   the detector fixed it immediately found two more real sites. Held by
   `the_scan_sees_the_shape_that_caused_every_finding`.

### A third, about method rather than code

The first attempt to break the `kg_context` instrumentation was a `str.replace`
that matched nothing, because the file had been reformatted since. The tests
stayed green and **I read that as the break being survived** — when the break had
never applied. One `grep -c` returning `1` was misread as a leftover doc mention
when it was the instrumentation still in place.

That is §8 of the handoff exactly: a plausible reading of an artifact, reported
as a fact about the system. The correction is cheap and now standard here:
**a break must assert that it applied before its result is read.** The redone
version prints `mentions: 2 -> 0` and asserts the count decreased.

---

## 11. Build log — step 2 of 6: gate accounting

**Landed.** `src/gate_trust.rs`, seven gates instrumented, exposed on
`GET /api/admin/liveness`, fenced by `tests/gate_trust_coverage.rs`.

### The reading nobody checks

§5.1 of the paper says a check that has never failed has not been tested. The
same sentence about a gate is sharper, because a gate has **two** failure modes
and they are symmetric:

| counters | reading |
|---|---|
| asked = 0 | never exercised. Not a pass. |
| asked > 0, approved = 0 | **refuses everything** — the Γ bug's exact signature |
| asked > 0, refused = 0 | **admits everything** — indistinguishable from no gate at all |

The third is the dangerous one, and nothing anywhere was looking for it. A gate
that never fires looks like a well-behaved system rather than a broken control.
`hud_contract::enforce` is a thousand lines of display gate with no production
caller, and from every surface the platform has, that is identical to a display
that never needed correcting.

`refuses_everything` is **asserted** — it makes no claim about the world, only
that the gate ran and let nothing through. `admits_everything` is **reported and
never asserted**, for the same reason `anomaly_events` is `Conditional`:
asserting it would assert that violations must exist.

`Undetermined` is a third decision, not a rounding of the other two. A gate that
cannot form an opinion has neither approved nor refused, and folding it either
way is how "the check could not run" becomes a verdict.

### Instrumented

`coherence`, `grounding`, `input_binding`, `admission`, `credit`, `rate_limit`,
`attachment`. Each declares its clock (§4.1's three), its retention, its
decision site, and — required — **what it would mean if this gate refused
nothing**. A zero refusal count is only actionable if someone wrote that down.

Two asymmetries closed: the admin *bypass* of the publish gate was audited to
`admin_bypass_events` while the *refusal* left no trace; and a forced publish now
records `undetermined` rather than vanishing.

`/api/admin/liveness` ranks `gate_refusing_everything` above `writes_refused`
above `degraded`, and reads the counters live rather than from the hourly sweep.

### The check that could not catch its own case

I wrote `a_refusal_site_records_before_it_returns` — a scan looking backwards
from each `Decision::Refused` for a preceding `return`, to catch a record placed
after the early return that refuses. The bug is silent in the friendliest way:
the gate works, the caller is correctly refused, the counter reads zero for ever.

**The deliberate break passed it.** Moving the record below the return pushed the
`return` seven lines up, past a four-line window. Widening the window would have
traded a false negative for false positives on the many legitimate early returns
nearby.

What settled it: constructing that break required `#[allow(unreachable_code)]`
to compile. **rustc had the answer the whole time**, and not as a heuristic about
line distance — as reachability analysis. So the scan was deleted, because a
check that certifies without being able to fail is worse than no check, and both
crate roots now carry `#![deny(unreachable_code)]`. Re-running the break without
the `allow` produces `error: unreachable statement`.

`the_crate_roots_deny_unreachable_code` holds the attribute, and additionally
refuses any `allow(unreachable_code)` sited next to a decision or write record —
the one place the lint is load-bearing.

The general form is worth keeping: **before writing a scan, check whether the
compiler already owns the property.** A lint is exact where a text scan is a
guess, and this codebase now has one scan fewer and one guarantee more.

### §5.1, applied

* removed `Gate::Credit`'s two call sites (verified `2 -> 0` before reading the
  result) → `every_declared_gate_is_recorded_somewhere` failed, naming `credit`;
* moved a refusal record below its return → the scan passed, which is why the
  scan is gone; the lint fails it with a hard error.

### Composition

The recorded tier writes through [`write_accounting`], so a gate ledger that
cannot write is itself counted. The rungs compose in the right direction: the
thing that watches the gates is watched by the thing that watches the writes.
`RESTORED-OK` printed anyway from the `;`. The file happened to be correct.
Had it not been, a broken tree would have been reported as restored on the
strength of an echo.

Same family as §8 of the handoff and as the no-op `str.replace` in step 1: an
artifact read as a fact about the system. The rule that keeps working is to end
every restore with the compiler, not with a message.

### Composition

The recorded tier writes through [`write_accounting`], so a gate ledger that
cannot write is itself counted. The rungs compose in the right direction: the
thing that watches the gates is watched by the thing that watches the writes.

---

## 12. Build log — step 3 of 6: seam vocabularies

**Landed.** `src/seam_vocabulary.rs`, ten seams registered, checked three ways
against production by `tests/seam_vocabulary_contract.rs`, fenced by
`tests/seam_vocabulary_coverage.rs`.

### An index, not a copy

Where a declaration already exists it is **referenced**. `anomaly_events.kind`
points at `anomaly_vocabulary::KINDS`; `semantic_rules.provenance_floor` and
`assertion_verifications.verdict` both point at the same
`grounding_trust::PROVENANCE_VALUES` — which is the useful part, because it puts
on the record that two columns share one ladder, and that sharing is exactly
where a copy would have been made.

Restating them would be the §3.4 violation the registry exists to prevent. Each
entry carries `owned_by`: `None` means the set had no Rust owner at all and is
declared here (they were bare literals at the write site — the `L1` setup);
`Some(path)` means an upstream module is the authority and this registry only
indexes it.

### Three checks, and the third has no substitute

Against production, all ten seams agree:

| | |
|---|---|
| declared token the constraint rejects | none |
| constraint token Rust never writes | none |
| **value in the column nobody declares** | none, across 4 columns with data |

The third is the one the others cannot do: it sees values written before a
constraint existed, values admitted by a `NOT VALID` constraint, and any drift
on a column with no constraint at all — where the data is the only authority
there is. It refuses to pass over an empty set, for the same reason the liveness
suite needs positive controls.

### Adopted

`process_spacetime.delta_direction` and `.resolution_mode` — the pair the audit
flagged as "the `L1` setup with the alarm removed as well" — now use named
constants, as does `eval_signals.evaluator_tier`, which was a bare
`'dimensional'` inside three SQL strings in three files, none referencing the
others. Those two are now **bound parameters** rather than spliced literals, so
the token is the declared one by construction.

### Three times the fence caught itself

1. **Positional indexing is a new proxy.** The first adoption wrote
   `DELTA_DIRECTION[2] // exact`. Reorder the array and every call site changes
   meaning while the comment still claims otherwise. Replaced with named
   constants, the pattern `anomaly_vocabulary` already used.
2. **The scan fired on 64 correct sites.** Fencing the provenance ladder flagged
   every fixture, test and legitimate caller of the module that owns it. §5.2:
   a check that fires on correct behaviour gets deleted, and the deletion looks
   like cleanup. Fixed by reading exemptions from the registry's `owned_by`
   rather than a hand-written list — which had already drifted from the registry
   about `rate_card` on its first run.
3. **The exemption was broader than the thing it exempted.** Read-side filters
   were exempted *by file*, and the deliberate break — a bare `"anomaly_delta"`
   put back on the write path — sailed through, because the write path is in the
   same file as the read filter. An exemption broad enough to cover what it was
   not written for is this audit's subject matter, reproduced in the fence.
   Now line-scoped, and the exemption must still match a live line or the test
   demands its removal.

   Narrowing it immediately surfaced two genuine sites the file-level version
   had been hiding.

### Why reads and writes are exempted differently

A wrong token in a **write** is refused and, on these paths, swallowed — the
sink stays empty and every surface reads "unused". A wrong token in a **read
filter** returns no rows, which someone notices on the next refresh. Different
risk, so different treatment, stated in the exemption rather than assumed.

---

## 13. Build log — step 4 of 6: the declared loop model

**Landed.** `src/loop_model.rs` — six chains, 22 stages — walked live by
`tests/loop_model_contract.rs` and exposed on `GET /api/admin/liveness`.

### Chains, not stages

Every stage of Loop 2 is empty. Read stage by stage that is five findings; read
as a chain it is **one**, and only the first is actionable — the four below it
are empty *because* the first is. That is the whole argument for the model.

Each loop declares its stages in order, and each stage declares how it fires. A
stalled loop names the link and a reason drawn from whichever layer knows:

| reason | source |
|---|---|
| `no_trigger` | the stage declares `Trigger::None` — nothing calls it |
| `scheduler_off` | `Trigger::Scheduler` whose var is unset-and-opt-in, or explicitly `0` |
| `writes_refused` | `write_accounting` — attempted, refused every time |
| `gate_refuses_everything` | `gate_trust` — the gate ran and approved nothing |
| `awaiting_upstream` | the link above is empty too |
| `no_input` | everything above produced; this link has had no occasion |

The last two are the honest answers for a healthy idle loop, and keeping them
apart from the first four is the point: **`no_input` is a fact about the world,
`no_trigger` is a fact about the code.** The model owns no arithmetic — it
declares shape and delegates every interpretation to steps 1–3.

### What it says today

```
  loop1   episodes 3558 -> consolidated 212 -> rules 253 -> retrieved 38   turning
  loop2   anomaly 0 <- stalled: no_input
  loop3   intentions 0 <- stalled: NOTHING CALLS IT
  loop4   claims 0 <- stalled: no_input
  loop5a  committed 1354 -> resolved 2179 -> scored 236                    turning
  loop5b  projected 61 -> anchored 0 <- stalled: no_input

  2 of 6 loops turning end to end.
```

Two details the chain view surfaced that a per-stage dashboard hides:

* **Loop 2's `persona_bumped` reads 13 while every stage above it reads 0.** The
  drift baselines moved via some path that is not Loop 2. Visible only because
  the stages are ordered.
* **Loop 4 stalls at `claims`, not at `proposed`.** The dead
  `composition_dream_handler` is real and it is the *second* break; fixing it
  first would have produced nothing, because there are no claims to attribute.

### The test that was right and permanently red

The live tier first asserted all three code-fault reasons, and went red on
`loop3.intentions: no_trigger` — a finding **already declared** in the model and
**already pinned** by `every_untriggered_stage_explains_itself`, which fixes the
exact set and insists it may only shrink.

Asserting it twice bought nothing and cost the thing that matters: a suite
permanently red for a known state is a suite people stop reading, and §5.2 says
the deletion that follows will look like cleanup. **A static fault belongs to a
static test.** The live tier now asserts only `writes_refused` and
`gate_refuses_everything` — the two that are dynamic, that can begin at any
deploy, and that no static check can see. `no_trigger` remains the loudest thing
in the report and is held from the other direction: a dead link that starts
producing must lose its declaration.

### §5.1, applied

Changing `loop3.intentions` from `Trigger::None` to `Trigger::Request` failed
`every_untriggered_stage_explains_itself` with the set diff. A first attempt at
that break did not compile — which is a legitimate red, and was verified as
such rather than counted as a pass.

### On `agent_loops_handler`

Still 610 lines of bespoke per-loop SQL. It is now the *second* answer to a
question this module answers from the contracts, which is exactly the
duplication §3.4 warns about. Replacing its internals is step 6 work: the model
has to be trusted in production for a while before a user-facing surface is
repointed at it.

---

## 14. Build log — step 5 of 6: the native evaluator registry

**Landed.** `src/native_evaluators.rs` — six evaluators — on
`GET /api/admin/liveness` and as the sixth tier of
`scripts/liveness_contract_live.sh`.

### Separate from the pluggable registry, on purpose

`agent_bestiary_evaluators` scores an agent's **output** — harmful, in
character, faithful. These score the **machinery** — is Loop 4 turning, is a
gate refusing everything, is a writer being refused by the database. Same
modular shape, different registry, because one health verdict over both makes
neither legible.

And the ordering is the point: *none of the pluggable scores mean anything if
the loops they feed are not closing.* A perfect safety score on a response whose
episode is never consolidated, scored or attributed is a measurement with
nowhere to go.

### Pure functions over a snapshot

An evaluator takes an `Observation` and returns a `Verdict`. It reads no globals
and touches no database — `Observation::collect` does that once, so every
evaluator sees the same instant.

That is not tidiness. **It makes §5.1 structural.** Building a world in which an
evaluator must fire is a struct literal, so
`every_evaluator_can_produce_a_finding` can hold the whole registry at once: *an
evaluator that cannot produce a finding is decoration, and it will read healthy
for ever.* A companion test breaks each evaluator's condition **alone** and
asserts exactly that one fires — an evaluator carried by another's condition
makes both findings unactionable.

Three verdicts: `Healthy`, `Finding`, `Inconclusive`. The third is not a pass,
and it merges "no data" with "could not run" because for an evaluator both mean
no information; the reason travels in the message rather than in a fourth
variant. A registry of six `Inconclusive`s reports `inconclusive`, never
`healthy`.

### It found a real defect on its first production run

`UndocumentedSilence` reported `anomaly_events` as silent with no excuse, while
the liveness script reported `0 silent`. Both were reading the same contracts.

`sweep()` pushed **every** silent sink into `undocumented_silent`; the runner in
`tests/liveness_contract.rs` additionally excused `Conditional` ones. Two
implementations of one classification — §3.4 — and the copy that got believed
was whichever the reader reached first. Nothing had noticed because until now
nothing read the library's report; the script recomputed it.

Fixed by extracting `is_actionable_silence` and having both call it, with
`a_conditional_sink_is_never_an_undocumented_silence` holding it. The evaluator
earned its place before it was finished.

### Severity is an ownership rule

The live tier asserts **`Critical` only**, and that is a decision about
duplication rather than about strictness:

| severity | means | asserted by |
|---|---|---|
| `Critical` | a control is inverted right now | this tier; nothing else can see it |
| `Warning` | a known structural gap | the tier that owns it — `loop_model`'s static pin, the liveness script's `silent` assertion |
| `Notice` | reported, asserts nothing | nobody |

The first version asserted `Warning` too and went red on
`loop3.intentions: no_trigger` — already pinned by a static test that fixes the
exact set and insists it may only shrink. **That is the same mistake the loop
tier made one step earlier**, which is why it is now written down as a rule
instead of being quietly fixed a second time: *every finding is asserted by
exactly one tier.* Assert it in three and the suite goes red three times for one
state, and §5.2 says what happens next.

### What it says about production

```
[ok]   positive_control           6 live contract(s), 2 turning loop(s)
[?]    refused_writes             no instrumented write attempted since boot
[?]    gate_refusing_everything   no gate asked since boot
[WARN] loop_stalled_in_code       loop3.intentions: no_trigger
[ok]   undocumented_silence       6 contract(s) live, none silently broken
[?]    gate_admitting_everything  no gate asked since boot
```

Three `Inconclusive` is honest and expected: the counters are per-process and
the test harness is not the server. It becomes a finding if it persists on a
long-running instance, and `report_which_evaluators_had_nothing_to_look_at` says
so.

### §5.1, applied

Making `GateRefusingEverything` unable to fire failed both
`every_evaluator_can_produce_a_finding` (naming it) and
`each_evaluator_fires_on_its_own_condition_alone` (`left: []`).

### The script now has six tiers

offline → live → firing → chain → seam → native. Exit 0, seven test binaries,
zero failures.

---

## 15. Build log — step 6 of 6: terminology reconciliation

**Landed.** `src/ladder.rs`; `provenance_oracle` delegates its arithmetic;
`card_contract` declares its subset; the module ordinals are corrected.

### The one that was a correctness bug, not cosmetics

`provenance_oracle::FloorAccumulator` kept the weakest **strength** and
`resolve` rebuilt a verdict from it:

```rust
let base = if s >= 2 { PROV_TOOL } else { PROV_INFERRED };
```

That is the tier collapse `grounding_trust::floor` documents as a fixed bug,
reintroduced one layer out. Measured on the two cases where the two
implementations disagreed:

| sources | old answer | true floor |
|---|---|---|
| weakest `human_endorsed` | `model_inference` | `human_endorsed` |
| weakest `tool_no_match` | `unavailable_no_tool_source` | `tool_no_match` |

Both **misattribute the mechanism** — a value settled by a person reported as a
model's inference; "the tool answered and had nothing" reported as "no tool
exists" — and both looked right because the strength was right. These floors are
written to `semantic_rules.provenance_floor` and are what decides how much a
distilled rule is trusted when it is injected into the next prompt.

The accumulator now keeps verdicts and delegates to `extracted_floor`. The
unknown rule is unchanged and still tested both ways — it is asked of the *raw*
floor, because the ceiling can clamp a strength-2 floor to 1 and asking the
clamped value whether it is at the bottom would answer no when the unknowns are
in fact irrelevant.

Pinned by `with_nothing_ungradeable_the_answer_is_exactly_extracted_floor`,
which checks **every pair** of the ten provenance values rather than the handful
someone thought to write a case for. That is the §3.4 rule made enforceable
rather than restated.

### The card vocabulary is a declared subset now

Four authoring tokens against five runtime dispositions, and the gap was
invisible from both sides: `GROUNDING_STATUSES` looks complete, and `Grounding`
gives no hint that one variant is unreachable from a card.

Recorded rather than closed, because the gap is probably **correct**:
`platform_derived` asserts that the *platform* computed a value reproducibly,
which is not a claim an agent's author can make about the agent's own output.
So `PLATFORM_ASSIGNED_ONLY` carries it with that reason, and
`author_vocabulary_is_a_declared_subset_of_the_runtime` requires every
disposition to be either declarable or excused — not both, not neither — so a
sixth variant cannot appear without someone deciding which it is.

### The ladder is a map, not a rename

Three of five modules are named for their mechanism (`schema_trust` for
Presence, `rollup_trust` for Truth, `port_trust` for Binding), and each declared
its own position relative to whatever existed when it was written — a
chronology, which disagreed with the paper on three of five. `liveness_trust`
called itself "the fifth trust contract" while being the paper's **second**, and
that sentence had been copied into two other modules.

Renaming the files would touch every call site and change no behaviour. The
drift that costs something is that **no artifact stated which module answers
which question**, so a reader reconstructed it from ordinals that actively
misled them. `src/ladder.rs` is that artifact, with tests: the order, the
questions, the clocks, the "passes while" column, and
`liveness_is_the_second_rung_not_the_fifth` pinned on its own. The four module
headers now state their rung and keep their chronology as history.

### Reported, not changed

`hud_contract::band_rank` / `weakest` remain a second strength ordinal and a
second min-over-sources. They are **dead code** — `hud_contract::enforce` has no
production caller — so the §3.4 violation is inert, and deleting a thousand
lines of display gate is a decision for the owner, not a tidy-up. Finding 6 in
§9 stands.

### A merge, mid-step

The live script went from green to exit 101 between two runs. The parallel
session had refactored `loop_model::diagnose` from my `upstream_empty: bool` to
an `Upstream { Produced, Empty, Unknown }` enum — a better model, because it
separates "the stage above is empty" from "the stage above could not be read" —
and two of my tests still passed a `bool`.

Kept their model, fixed my call sites. Worth recording only because the failure
was caught by running the script rather than by assuming the tree was still the
one I had tested, which is the same discipline as everything else here.

---

## 16. Where the plan stands

| step | state |
|---|---|
| 1 · failure accounting | landed |
| 2 · gate decision ledger | landed |
| 3 · seam vocabularies | landed |
| 4 · declared loop model | landed |
| 5 · native evaluator registry | landed |
| 6 · terminology reconciliation | landed |

Six new library modules, eight new test files, six live tiers in
`scripts/liveness_contract_live.sh`, exit 0.

### Still open, in priority order

1. **The two held fixes.** The `anomaly_events` foreign-key race
   (`execution.rs:397` vs `:494`) and the unguarded timeline spawns in
   `workspace::messages` and `rabble_workspace`. Held deliberately so they could
   be *verified* rather than believed — the machinery to do that now exists, and
   after the fix `write_accounting` for those sinks must show attempts with no
   failures.
2. **Loop 3 stage 0 and Loop 4 stage 3** — the two `Trigger::None` links. Both
   are declared, pinned, and may only shrink.
3. **Grounding is a metric, not a control, on the two general execute paths**
   (§3). Unchanged by this work.
4. **`hud_contract::enforce` has no production caller** — wire it or delete it.
5. **`delegate_to_agent` has no grounding gate at all.**
6. **`agent_loops_handler`** — 610 lines of bespoke SQL, now a second answer to
   a question `loop_model` answers from the contracts. Repoint it once the model
   has earned production time.

### What changed about the method

Every check added in these six steps was deliberately broken and watched go red,
and three times the break revealed the check could not have caught its own
motivating case: the write-accounting scan was satisfied by a declaration rather
than a call site, the swallow detector could not see the commonest swallow
shape, and the refusal-ordering scan missed a seven-line gap that `rustc`
catches exactly. Two further times a *break itself* silently failed to apply and
the green was nearly believed.

The habits that came out of it, in the order they were learned:

* **A break must assert that it applied before its result is read.**
* **Before writing a scan, check whether the compiler already owns the
  property.**
* **Every finding is asserted by exactly one tier.** Assert it in three and the
  suite goes red three times for one state, and §5.2 says what happens next.
* **An exemption must be no broader than the thing it exempts.** A file-scoped
  exemption for a read filter covered the write path in the same file.

---

## 17. The two held fixes

Held since the audit so they could be **verified rather than believed**. The
machinery to do that now exists.

### 17.1 The `anomaly_events` foreign-key race

The grounding anomaly was raised in the `!is_clean()` branch, referencing an
`episode_id` minted at the top of the handler whose row is not written until
~200 lines later. `anomaly_events.episode_id` is a real foreign key, so the
detached task raced the episode write and lost whenever anything between them
took time — embedding generation, for instance.

**Fixed by moving the raise below the episode write**, not by dropping the
foreign key. Dropping it was the available precedent — migration 197 did exactly
that for `forecast_agent_claims.episode_id`, reasoning that "integrity is
enforced by construction rather than by constraint" — but construction is a
promise and a constraint is a guarantee, and there was a placement that keeps
both. `store_episode_with_provenance` `?`-returns on failure, so reaching the
new site means the episode landed.

**And the placement is now enforced by the compiler.** The post-store binding
was shadowing the minted one, so moving the raise back up would have compiled
and silently used the un-written id again. Renaming it to `stored_episode_id`
makes the correct ordering the only one that builds:

```
error[E0425]: cannot find value `stored_episode_id` in this scope
```

That is the step-2 lesson applied again: *before writing a scan, check whether
the compiler already owns the property.* A comment saying "keep this below the
write" is a request; a binding that does not exist above it is a control.

### 17.2 The unguarded timeline spawns

`workspace::messages` and `rabble_workspace` each swallowed an `episodes` write
and then spawned a timeline write whose foreign key points at that episode. One
failure, two loop sinks lost, no signal anywhere. `execution_stream` has always
guarded its equivalent spawn — the guard was known and applied at one of three
sites.

Both now bind the `observe` result and guard on it. The failure is counted as
well as prevented, so if the episode write starts failing the liveness report
says `rejected` with the error attached rather than showing an empty table.

### 17.3 How these will be confirmed

Neither fix can be proven by a unit test — the race needs concurrency and a
violation needs a violating agent. What can be checked, and now is:

* the ordering fault cannot be reintroduced without a compile error;
* the next grounding violation shows `anomaly_events` with **attempts and no
  failures** in `write_accounting`, instead of a silent zero;
* `loop_model` moves Loop 2's `stops_at` off `anomaly`, or explains why not.

That is the whole point of having held them.

---

## 18. Validation at close

**53 offline suites pass.** All five live tiers pass against production:
`liveness_contract`, `anomaly_firing_probe`, `loop_model_contract`,
`seam_vocabulary_contract`, `native_evaluator_contract`.

**The `api-server` binary does not currently build**, and not because of this
work: the parallel session is mid-refactor on `crates/posterior`, swapping
`ExtractorRegistry` for `FeedRegistry`, and two call sites in
`handlers::workspace::{refit,resolution}` have not caught up. Every file this
audit touched compiles clean.

One edit was made outside this work's scope: `refit.rs:205` gained the
`workspace_id` field that the same refactor added to `WorkspaceContext`. It was
mechanically forced — the struct requires it — and set to `Some(workspace_id)`
rather than `None` because the field's own doc warns that a feed taking its
workspace from config "could be pointed at another workspace's data". The two
remaining errors were **not** guessed at; they need the new registry's design.

---

## 19. A reviewer's question, and what measuring it turned up

A session working on the UX read the loop report and asked a good question:

> Loop 2's `anomaly` stage reported `no_input`. But the audit establishes that
> seed **cannot write** — FK violation on every attempt. Both readings are
> honest; they differ because the counters are in-memory, per-process, and a
> fresh test process has attempted nothing.

The observation is right and the explanation is wrong, and finding out which
took one query.

### The seed has never been attempted — in any process, ever

```
episodes                     3558
  grounding:violations          0
  grounding:enforced            5   (all weather_oracle)
```

`create_anomaly_event` on the grounding path is reached only inside
`if !grounding_report.is_clean()`. **No episode has ever carried a violation.**
So the FK race is not a thing that has been happening and going unrecorded — it
is a defect that was waiting for its first violation, and none has come.

`no_input` was therefore the *correct* reading, and not by luck: the write had
no occasion, which is exactly what it says.

### The audit overstated it, in the way §8 warns about

§9 said the seed "loses an FK race and **is** silently rejected". What was
actually verified is that the constraint **rejects such a row** — proven in a
rolled-back transaction with a synthetic id. That it was *being* rejected was
never established, and could not have been: there was no counter, and there were
no violations to count.

A true fact about the schema, reported as a fact about the system's behaviour.
Same shape as reading `grep -oE "^[A-Z_]+="` and concluding the database was
unreachable. Corrected in the priority table.

The fix was still right, and is still worth having: the defect was real and
latent, and it would have fired on the first violation — silently.

### Two defects the question exposed

**The grounding gate was counting non-engagement as approval.** `enforce`
returns an empty report when an agent has no contract, and the instrumentation
recorded that as `Approved`. The comment directly above it said *"`enforce`
returns an empty report for an agent with no declared contract, which looks
exactly like a clean pass from here"* — the trap was named, and then walked into.

At this system's actual scale that is not cosmetic: **5 of 3,558 episodes carry
a grounding tag at all.** The gate would have reported `3,558 asked, 0 refused`,
which reads as *a control that has never needed to fire* when the truth is *a
control that almost never engages*. Different findings, different remedies, and
no row count distinguishes them. It now records `Undetermined` — the gate was
reached and formed no opinion, which is what actually happened.

**`no_input` claimed more than cold counters can support.** This is the
reviewer's underlying point, and it is valid even though it did not explain this
case. `no_input` is a *positive* claim — the trigger had its chance and there
was nothing to do. On a freshly booted server the counters are zero, and in that
state `no_input` is indistinguishable from *this path has been failing since
before the restart*.

An instrumented stage with zero attempts now reports **`unobserved`**, which is
a different instruction to the reader: `no_input` says look at the world,
`unobserved` says wait for traffic or look at a longer-lived process. An
*uninstrumented* stage keeps `no_input`, because it has no attempt count to be
cold and downgrading it would make the reading useless.

The report now reads:

```
loop2   stalled at `anomaly`:    unobserved
loop3   stalled at `intentions`: no_trigger
loop4   stalled at `claims`:     unobserved
loop5b  stalled at `anchored`:   unobserved
```

Three of the four stalls were overclaiming, and the one that was not is the one
backed by a static declaration rather than by a counter.

### What this says about the in-memory decision

It stands, and it now has a stated cost. Counters that cannot fail are worth
more than counters that survive a restart, because a durable ledger that shares
the database it is reporting on is silent exactly when it matters. What was
missing was not durability — it was **saying so in the verdict**, which
`unobserved` now does.

Durable accounting remains the honest next step, and the shape is already
settled: the hourly liveness sweeper flushes a snapshot, and a failed flush is
counted in memory where it cannot be lost.

---

## 20. Loop 2 — the raise now fires from every path that enforces

### The finding

Nine files called `grounding_trust::enforce`. **One** raised an anomaly, and it
carried almost none of the traffic from agents that have contracts:

| agent | episodes | reached `/execute` | grounding-stamped |
|---|---|---|---|
| football_analyst | 208 | 0 | 0 |
| prey_locator | 93 | 0 | 0 |
| genome_profiler | 65 | 0 | 0 |
| enemy_sensor | 62 | 0 | 0 |
| weather_oracle | 54 | 27 | 5 |

The creature paths *run* the control — they strip the fabricated field before it
renders — and then said nothing. A violation on the path where violations are
most likely was caught, corrected, and forgotten. `anomaly_events` is Loop 2's
only input, so a control that corrects without reporting is a loop that cannot
start.

Loop 2 was never waiting on its machinery. Every stage of that has been
verified. It was waiting on a raise wired to ~1% of gated traffic.

### What changed

`src/grounding_anomaly.rs` — the one place a violation becomes a Loop 2 input.
Wired into **all seven** enforcing paths: `/execute`, `/execute/stream`, the
five creature module handlers, and the forage identifier.

`execution.rs`'s inline copy is gone; it now calls the shared function, so the
count of implementations went from one-plus-eight-absences to one.

Two preconditions are stated in the signature rather than in prose:

* **`persisted_episode_id` means persisted.** The foreign key is real and the
  race it caused is §17.1. `None` is always legal, and most sites pass it —
  an anomaly with no episode is worth far more than no anomaly.
* **A failure to raise is counted.** Including the easiest to miss: a slug that
  resolves to no agent row. `agent_id` is `NOT NULL` with a foreign key, so an
  unresolvable slug means no anomaly can be filed, and that must not look like a
  clean run.

`identify_specimen` now returns its `Report` instead of summarising it into a
JSON field and dropping it — the control had been firing into a response body
that nothing aggregates.

### The forcing function

`tests/grounding_raise_coverage.rs`: every file that calls `enforce` must also
call `grounding_anomaly`, or be exempted with a reason. Three exemptions remain
— the delegation hop (the child's own path already raised), the tool-loop cache
read (no store in scope, and the generating path raises), and the HUD contract
(no production caller at all, finding 6).

**The scan caught two things on its first run:** `execution.rs` was still using
its inline copy, and three of my six exemptions were stale — files that mention
`enforce` only in prose and never call it. The exemption test refused them.

§5.1: commenting out the five creature raises produced
`1 path(s) enforce the grounding contract and tell Loop 2 nothing`. A first
attempt at that break failed to compile, which is a red for the wrong reason and
was redone.

### How this gets confirmed

Not by a unit test — it needs an agent to fabricate a field. What is checkable:
the next violation on *any* enforcing path shows `anomaly_events` with attempts
in `write_accounting`, and `loop_model` moves Loop 2's `stops_at` off `anomaly`.

---

## 21. State of the tree at handover

**54 offline suites pass**, including all thirteen modules from this work.

Two things are red, and neither is a defect in this work:

1. **`seam_vocabulary_contract`** reports `gate_decisions.{gate,decision}`:
   *table does not exist*. Migration `214_gate_decisions.sql` is written and
   **registered**, and has not run because the server has not rebooted.
   Migration 212 is in the same state. This is the check doing its job on a
   pending deploy, and the message now distinguishes the three causes:
   no table (pending, or never registered — 212's failure), table without the
   constraint (ran and did nothing), and a `schema_migrations` row with failures
   (ran and could not apply).
2. **`panel_absence::every_agent_scoped_panel_is_probed_or_declared_unresolved`**
   — the parallel session's file, its own pin drifting from its own
   declarations. Untouched.

**Both clear on the next deploy** (1) and by its author (2).

### Track B is not started

The typed-enum seam reduction — `#[derive(sqlx::Type)]` at the database
boundary — has not been begun. It remains the highest-leverage work available,
because it is the only item on the list that reduces the defect *rate* rather
than the time to discovery: `L1`, `source_kind`, `delta_direction` and
`evaluator_tier` would each have been a compile error rather than a silent
runtime rejection.

The four registry-owned vocabularies in `seam_vocabulary` are the place to
start, and the shape is settled: derive the declared arrays *from* the enums so
the array cannot drift from the type, then bind the enum instead of the string
at each write site. `simops_benchmark` is the natural first conversion — it
writes two CHECK vocabularies and was the site the audit flagged as "the `L1`
setup with the alarm removed as well".

---

## 22. Loop 3 — the stage that ran, produced rows, and was not the thing

*2026-08-28. Two findings, recorded together because they were found together
and share one victim: the coordination strategist.*

### 22.1 The instrument was not silent, it was measuring the wrong thing

§4 recorded Stage 0 as a **dead declaration** — six dispatchable tools that no
prompt asked for, `workspace_intentions` = 0. That was accurate, and it was
fixed on 2026-08-16 by asking for them. The stage then began producing rows.

It still did not coordinate, and this is the part §4 could not have anticipated,
because the finding is invisible from a count.

The only caller was the strategist's Stage 0. It read twenty messages of
transcript and called `declare_intention` once per member, describing what it
*supposed* each was about to do. **No member was ever asked anything.**
`workspace_intentions` had `agent_id` — the agent a row is *about* — and no
column recording the agent that *wrote* it. So a member's own plan and the
coordinator's guess about it were byte-identical.

The duplication pass is built on the premise that two rows are two agents'
plans. When both rows come from one model summarising one transcript in one
turn, an `OVERLAP_WARNING` between them measures the coordinator's paraphrasing
— and a cosine threshold of 0.82 is tuned to fire on exactly that. So the check
fired **most reliably in the case where it meant least**, and
`suggest_differentiation` then told two agents to divide work neither had ever
claimed.

Every counter read healthy throughout. The map filled, the tools returned, the
signals came back CLEAR or OVERLAP_WARNING as appropriate. This is a **third
defect shape**, distinct from the two this audit already names:

| Shape | Signature | How it is found |
|---|---|---|
| Write path works, read path points elsewhere | sink fills, reader reports zero | compare the two queries |
| Hop reached on every cycle, never returns | call site exists, sink stays empty | count what the call *produced*, not that it ran |
| **Mechanism runs and is not the thing** | **every count healthy** | **ask what a row means, and who is entitled to treat it as evidence** |

The third is the hardest, because no row count can catch it and the instrument
is confidently reporting a number. §7's defect class — *a declaration mistaken
for an implementation* — extends: **a row is a declaration too.**

The rule this yields:

> A count tells you a write happened. It never tells you what was written, who
> wrote it, or whether the reader was entitled to treat it as evidence. For any
> signal derived from more than one row, **provenance is part of the signal**,
> not metadata about it.

### 22.2 The coordinator was the one agent excluded from Loop 1

Found while tracing 22.1. `handlers::workspace::coherence` was the only
agent-execution path in the repository that called neither
`enrich_with_kg_context` nor `agent_output_to_episode`. Every other path —
`execution`, `execution_stream`, `workspace::messages`, `rabble_workspace`, and
the `execute_agent` tool — does both.

A closed circle of zero: no episodes → nothing to consolidate → no rules →
nothing to retrieve. Meanwhile `cohere_and_coordinate`'s card opens Stage 4 with
*"Read consolidated memory: review your past dreaming episodes for this
workspace. What coherence patterns recur? Which principles are chronically
weak?"*

Nothing was behind that instruction. The agent appointed the platform's
longitudinal learner opened every session as its first, and "chronically" was a
word it had no way to mean.

**Why it survived every previous pass, including this audit's §1.** Loop 1's
`episodes` stage counts rows platform-wide and has never been empty — 3,558 at
the time of §13's report. Nothing asked *which* agents produce them, and an
agent that writes none is indistinguishable from one that has not run. The chain
view in §13 is a genuine advance over a per-stage dashboard and it is still a
platform-scoped instrument; per-subject scope (`loop_api::SubjectScope`) is what
makes this class visible, and it is the reason `loop3.plans` is declared
`PerAgent`.

### What changed

| Piece | Effect |
|---|---|
| `migrations/218_intention_provenance.sql` | `declared_by`, `source` ∈ {`self`, `solicited`, `inferred`, `unattributed`}. Existing rows backfill to `unattributed` — the author is genuinely unrecorded, and guessing `inferred` (almost certainly correct) is how a denormalised value starts drifting from the truth |
| `solicit_agent_plan` | Invokes the member with its peers' intentions in context and records the answer as that agent's own. The round trip that turns a belief into a report |
| `fermi::intentions` | Duplication between two `inferred` rows suppressed; resource and dependency conflicts unaffected, because a named target is a checkable claim about a file regardless of who wrote the row |
| `Grounding` | `GROUNDED` / `PARTIAL` / `UNGROUNDED` returned on every map read and every write. A CLEAR signal over an ungrounded map is not evidence of alignment |
| `loop_model` loop3 | `plans` (`source = 'solicited'`) split from `intentions` (all rows). One combined count is what let the stage read as healthy |
| `handlers::workspace::coherence` | Retrieval before the run, episode after, pre-minted id on the `ToolContext` so delegated work is not orphaned |

Supporting research for 22.1: **ReMALIS**, arXiv:2407.12532 §3.1 — agent *i*
holds a private intention `I_i = (γ_i, Σ_i, π_i, δ_i)`; what another party holds
is a belief `b_j(I_i | m_ji) = f_Λ(m_ji)` formed from a message *i* actually
sent. §4.4 Table 3 measures the gap: 31%/23%/17% aligned sub-tasks with no
communication against 91%/71%/62% with full intention sharing. Declaring on an
agent's behalf is the first row wearing the last row's vocabulary.

### §5.1, applied

Seven mutations in `scripts/break_coordination_loop_closure.py`, each requiring
the named test to go red. All seven caught.

Break 2 is the one worth recording. The first draft of
`every_agent_execution_path_persists_an_episode` scanned for the string
`agent_output_to_episode` — which `coherence.rs` **already contained**, in an
import it never called, with a comment beside the import saying so. The test
passed against the exact defect it was written to catch, and would have passed
again the moment someone deleted the call and left the import.

The guard now strips `use` lines and comments before scanning, and break 2
reproduces that precise state: call removed, import left behind. This is §5.1's
rule reaching source scans — *a break that comes back green is a failure of the
guard* — and it is the second time in this audit that a scan was nearly believed
because nothing had tried to falsify it.

### What is not fixed

* `solicit_agent_plan` has **not been observed turning on real traffic.** The
  confirming query is `SELECT count(*) FROM workspace_intentions WHERE source =
  'solicited'`, and `loop3.plans` reports it. Until it is non-zero, Stage 0's
  closure is a code claim, not a measurement — which is the distinction §16
  exists to keep.
* ~~`Trigger::Prompted` still applies to both `plans` and `intentions`.~~
  **Closed the same day — see §22.3.**
* Contradiction detection remains honestly absent. §4's table is unchanged on
  that row.

### 22.3 The fix was itself contingent on a model, and that was caught here

The bullet above is the one worth dwelling on, because it was written by the
same pass that had just finished documenting why the shape is fatal.

`solicit_agent_plan` shipped as a tool. The card's Stage 0 named it, the shelf's
prompt named it, and whether any member was ever asked came down to whether a
language model felt like making N tool calls. That is defect 0 of Loop 3
reproduced exactly: `record_coordination_observation` existed, was dispatched,
was asked for by name in two places, and produced **0 of 3,576 episodes**.

Writing the honest caveat and shipping anyway would have been placing the same
bet twice, with the losing ticket still on the desk. So:

| Piece | Effect |
|---|---|
| `src/plan_solicitation.rs` | One implementation, two callers — the tool (targeted, model's judgement) and the shelf (the floor). The same division `coordination_note` established |
| The floor runs **before** the strategist | The one deliberate difference. A brief is retrospective; a plan is not. A post-hoc plan floor produces identical counts, climbs `loop3.plans` identically, and grounds nothing in the run that paid for it |
| `FRESHNESS_SECS` = 600, `MAX_PER_RUN` = 8 | Each ask is an LLM call the user did not press a button for. Bounded, and the cap is *reported* when it bites rather than truncating silently |
| Concurrent | Eight sequential model calls in front of an HTTP handler is not a slow endpoint, it is a broken one |
| `loop3.plans`: `Prompted` → `Request` | The reading changes, which is the point. A zero under `Prompted` could be an untried feature or an ignored instruction, indistinguishable; under `Request` it can only be traffic or failure |
| `Sink::WorkspaceIntentions` | The stage was `accounted: None`. A floor whose write failures nobody counts cannot be told apart from a floor nobody triggered — §5's whole subject |

### The stale-excuse check, and what it found immediately

`loop3.plans` moving off `Prompted` made its `NO_DOOR` entry dead: the door
check skips stages that are not person-driven *before* it consults the list, so
a stale excuse is never read and never fails. It just sits there being a
documented reason that has stopped being true — §7's defect class, applied to a
guard rather than to a mechanism.

The list had no staleness check, so one was added. **It found `loop2.corrected`
on its first run** — an entry whose reason is accurate prose about a
`Trigger::Upstream` stage, unreachable since whenever the trigger changed, read
by nothing. Removed; `Upstream`'s own declaration already says it, in the one
place that cannot drift from the trigger.

This is the second time in two passes that a guard was found asserting nothing,
and both were found by trying to break them rather than by reading them. The
mutation script is now 12 breaks. Break 10 — delete the line that records the
cap biting — came back **green** on the first attempt, because the test scanned
for `floor.capped` and the logging line still contained it. Exactly the
`agent_output_to_episode`-in-an-import trap from §5.1, in the same suite,
against a test written by someone who had just fixed that one. The guard now
asserts the assignment.

> Three for three: every source scan in this work was vacuous on first writing,
> and none of the three was noticed by reading it. A source scan should be
> assumed non-functional until a break has been seen to turn it red.
