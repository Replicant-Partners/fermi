# Handoff — next session

**Date:** 2026-08-24 · **Reference:** `docs/AUDIT_loops_and_gates.md`

**The tree is fully green for the first time: 58 offline suites, 8 live tiers,
zero reds.** Track A and Track B are done, Loop 3 is wired, migration 215 has
deployed and is confirmed, and there is a new rung.

---

## Done this session

### Migration 215 — deployed and confirmed

`process_spacetime.committed_before_measured` compared `committed_at <
resolved_at`, and `resolved_at` is `NOW()` at scoring time. The resolver can
only score a commit it has already read, so **the column carrying Loop 5.B's
whole claim was `true` by construction for every row.** The table has carried
`measured_at` since migration 141 and the predicate never used it.

215 replaces it, guarded so the generated-column rewrite fires only while the
old expression is present. Verified by applying it to the production schema in a
rolled-back transaction *before* registering it, then confirmed by deploy:
`tests/projection_anchor_contract.rs` was **red before and is green after**,
reading the live expression from `pg_attrdef` rather than from the file.

`loop_model`'s Loop 5.B `resolved` stage now counts
`WHERE committed_before_measured`, so the chain cannot be closed by
transcription. The backfill remains off the table and is now blocked twice.

`seam_vocabulary_contract`'s `gate_decisions` red also cleared — 212/213/214
applied in the same deploy.

### The `Ok(false)` that meant three things

`apply_agent_multipliers` returned `Result<bool, String>` and `execution.rs`
discarded the false case with `// no multiplier found, nothing to do`. It
covered three states: no binding, a workspace whose FPL names no driver for this
agent, and evidence carrying no number. `forecast_agent_claims` has held zero
rows since mig-187, so the first *bound* run that still writes nothing is the
observation Loop 4 has been waiting for — and it would have arrived silent.

Now `ClaimOutcome` with four variants and a `label()`. The decision moved to
`src/claim_outcome.rs` in the **library**, for the same reason
`commit_projection` did: it lived in the api-server binary, which an integration
test cannot reach, so it could not be registered — and the rule the registry now
enforces is that a decision without a falsification does not get added.

Three registry pairs. **The second and third exist only because the first two
breaks came back green:** `recorded()` cannot tell `Unbound` from
`NoDriverForAgent`, and the first pair returned before the assertion branch was
ever reached.

### `src/outcome_trust.rs` — turning is not closed

The new question, and no other rung asks it:

| asks | module |
|---|---|
| does the declared object exist? | `schema_trust` |
| does the writer ever run? | `liveness_trust` |
| does the chain produce, stage by stage? | `loop_model` |
| **does what it produces carry the signal the claim needs?** | `outcome_trust` |

Not a rung of `ladder` — the paper defines five and a test pins that. This sits
over them, like loops and gates.

**Two findings on Loop 5.A, both measured against production:**

1. **`Uniform { events: 47 }`.** The `scored` stage is declared *"per-agent
   calibration is recorded"*. `record_forecast_calibration_signals` takes the
   *forecast's* Brier and writes it once per name in `agents_used`. 47
   forecasts, several agents each, **exactly one distinct score on every one**.
   An agent that carried the forecast and one that contributed nothing are
   indistinguishable. Four agents share identical minima (0.805) and means
   (0.987) because they are the same numbers.
2. **`Conflated { producers: 2 }`.** `brier_forecast_resolver v1` (188 signals,
   one per resolved forecast) and `brier v1` (51, one per aggregate over N
   forecasts) both write `dimension = 'forecast_calibration'`. Different
   denominators, one column, nothing comparing them. Any reader that averages it
   weights a single-forecast score equally with a mean over forty-eight.

The loop is turning. **Nothing downstream can be reading agent skill from that
metric, whatever it believes** — and the MoE router at Stage 0 and composition
evolution both do.

Both are in `KNOWN_GAPS` with what would clear them, reported rather than
asserted, and `every_declared_gap_is_still_open` insists they are still gaps —
the `KNOWN_SILENT` instrument, which had one entry removed by its own stated
condition on the first run that met it. Verified both ways: dropping a
declaration turns the finding red, and making a gap appear closed turns the
ratchet red.

---

## What it cost to get those two numbers right

Three wrong readings, each caught by the discipline rather than by luck:

* The spread query grouped on `rationale` and returned
  `Discriminates { events: 50, varied: 1 }`. The single varied event was two
  producers' rows sharing a string.
* Adding the producer to the `GROUP BY` did not fix it: `brier v1` writes
  `Brier 0.000 over 1 forecasts` for every agent-aggregate that scores that way,
  so 18 rows from three unrelated agents still collapsed into one bucket. Its
  rationale identifies neither the agent nor the forecast, so **its rows cannot
  be grouped into events by any column the table has** — which is part of the
  `Conflated` finding, not a gap in the check.
* The classifier said *"one varying event settles it"*. Against 1-in-50 that
  clears an instrument on noise. Now `Sparse`, with the threshold taken from the
  `min_events` the contract already declares, so nothing was fitted to the data
  after seeing it.

The scan also caught this session's own work twice: the falsification registry's
projection fixture spelled `dynamics_projection` in a literal, and
`only_one_module_names_the_projection_tags` named all five lines.

---

## Where the loops are

```
loop1   episodes 3558 → consolidated 212 → rules 253 → retrieved 38   turning (7 of 761 agents)
loop2   anomaly 0 ← stalled: unobserved
loop3   intentions 0 ← stalled: awaiting_agent
loop4   claims 0 ← stalled: unobserved
loop5a  committed 1354 → resolved 2180 → scored 236   turning, and the score is uniform
loop5b  projected 61 → anchored 0 ← stalled: unobserved
```

**Nothing on any loop is blocked by code in the tree.** One deploy has landed;
what remains is volume, adoption, and two product decisions:

| loop | needs |
|---|---|
| 1, 5a | agents through a dream cycle / with claims — volume |
| 5b | one projection written *after* the hook. 7,576 real readings already waiting |
| 3 | someone presses the 2–5 credit button. 4 of 267 workspaces ever have |
| 2 | field-contract coverage (≈4 agents of 206 producing episodes) and a reviewer |
| 4 | runs bound to a workspace or forecast. All 65 judgements had neither |

---

## `src/surface.rs` — the pattern, extracted

Every trust domain has five parts. Three are **answers** and are never shared;
two are the same idea and the same mistakes everywhere, and now live in one
place.

| part | loops | gates | evaluators | shared |
|---|---|---|---|---|
| declared model | `loop_model::LOOPS` | `gate_trust::GATES` | `native_evaluators::registry` | no |
| measurement | rows per stage | approve/refuse counters | verdicts over a snapshot | no |
| interpretation | `panel_absence::Reading` | `GateAccount::*` | `Verdict` | no |
| **door** | who acts, and where | | | **yes** |
| **caveat** | what a green tick does not mean | | | **yes** |

`Door` is keyed by `subject` in each domain's own vocabulary — `loop2.reviewed`,
`gate.coherence` — because the three do not share a key and forcing one would
produce a lowest common denominator that fits none. What *is* shared is the rule
set (`door_problems`, `caveat_problems`) and **one** router scan
(`doors_missing_from`) instead of three that can drift.

`Caveat` is a required field, not a doc comment: every check here is narrower
than the claim it serves, and a surface that renders a tick against the claim is
the over-reading the whole audit is about, committed in the one artifact a
non-author reads. `caveat_problems` rejects a `does_not_show` that merely
restates `checked`.

## `GET /api/gates` — the second instance

Which is what shows `surface` is a pattern and not a rename. `gate_api` maps the
counter pairs onto the same three readings:

| token | reading | why |
|---|---|---|
| `refuses_everything` | `fault` | asked, approved nothing. The Γ bug's signature |
| `never_asked` | `unknown` | **not a pass.** An unwired control and one with nothing to refuse are the same observation |
| `admits_everything` | `unknown` | reported, never asserted — asserting would assert that violations must exist |
| `discriminating` | `idle` | has both approved and refused |

`since: boot | ledger` is the field that stops `refused: 0` being read as "never
in its life" — three of five gates have process-local counters.

**`GATE_DOORS` is empty, and that is a finding.** Nothing anywhere lets a person
act on a gate: no review of what it refused, no override, no record that a
refusal was wrong. Defensible — a gate a person can wave through is not much of
a gate — but nobody had decided it, and until the list existed there was nowhere
to notice.

## `docs/UX_HANDOFF_trust_surfaces.md`

The handoff. Leads with the one idea (`unknown` is a third thing, not a side),
then the payload shapes, then — explicitly — what is guaranteed by a failing
build versus what is not. The "not guaranteed" list says outright that the
semantics behind some of these endpoints still need work and that most queues are
empty; the point of the surface is that it says *why* each one is.

## The loop surface — `GET /api/loops`

One shape for *"where does this loop stand, and what can a person do about
it"*. `src/loop_api.rs` assembles; nothing re-answers anything.

| question | owner |
|---|---|
| does the chain produce, stage by stage? | `loop_model` |
| is an empty thing idle, faulty or unknowable? | `panel_absence` |
| does what it produces carry the signal the claim needs? | `outcome_trust` |
| **what can a person do at this stage?** | `loop_api::STAGE_ACTIONS` — new |

```
GET /api/loops           six loops, first empty link, reading, doors
GET /api/loops/actions   the doors alone, no database walk — build buttons at startup
GET /api/loops/:loop_id  one loop, same shape
```

Not under `/api/admin`: the only honest account of the loops was behind an admin
scope, which is how it stayed invisible.

**Three contracts the payload makes to a client**, stated in the payload itself:
never render a bare zero (`measured: false` means show nothing, not `0`); every
empty panel carries `reading` — `idle` / `fault` / `unknown`, and `unknown` is
not a pass; and exactly one stage per loop is `is_first_empty`, because
everything below it is empty *because of* it and highlighting all four turns one
finding into four.

Live, right now:

```
6 loop(s): 2 turning · 0 stalled by a code fault · 0 stalled and idle
           · 4 stopped with no reading available · 0 unreadable

loop2  stalled  Unknown   stops at `anomaly` (unobserved)
     < anomaly           0  request
       reviewed          0  manual     POST /api/observatory/hitl/:event_id/action
       consensus         0  manual     POST /api/observatory/hitl/consensus/:request_id
       corrected         0  upstream
       persona_bumped   13  upstream
```

### `STAGE_ACTIONS` — the part that is genuinely new

Nothing declared where a human's door was. Half these loops are human-gated by
design — Loop 2's `reviewed` *is* a person acting — and a stalled manual stage
with no visible door is indistinguishable from a platform defect.

Every entry carries `why_manual`, required: **a stage that cannot argue for being
manual should be automated**, and a reviewer deciding whether a queue is worth
working needs the argument, not just the button.

Checked four ways: every action names a real stage in `loop_model`; no action
sits on a stage the platform drives itself (`Upstream`, `Scheduler`, `None`);
every `Manual`/`Prompted` stage has a door or is in `NO_DOOR` with the reason its
door is elsewhere; and **every advertised path exists in the router**.

That last one caught an invented path on its first run: `loop4.accepted` said
`/api/compositions/:composition_id/accept`, which does not exist. A button that
404s after a reviewer believes they recorded a correction is worse than no
button, and `hitl_actions` holds zero rows so no traffic would have told anyone.

The route-ordering claim (`/actions` before `/:loop_id`, or `actions` matches as
a loop id) was verified by swapping them and watching it go red.

### Two things the build caught in itself

* **My action rule was wrong.** It asserted doors only on `Manual`/`Prompted`,
  and `loop3.settled` failed — correctly. Its trigger is `Request`, and a request
  *is* a person pressing something. What must never carry a door is what runs
  without anyone.
* **The header was dishonest.** Three buckets printed *"2 turning, 0 stalled, 4
  unmeasured"* against production — wrong twice. Nothing was unmeasured; every
  probe ran. And "0 stalled" invites the conclusion that nothing is wrong when
  four loops have stopped. The words were borrowed from `loop_model`, where
  `unmeasured` means *a probe did not run*, and used here for *the reason is
  unknowable* — the two states this codebase most insists on separating. Now
  five buckets, asserted to partition the set.

### `agent_loops_handler` is still there

`/api/observatory/agents/:agent_id/loops` survives this commit because it is
per-agent where the new surface is platform-wide. It is still the audit's §9 item
6 — 610 lines of bespoke SQL, a second answer. Held together by
`the_two_loop_surfaces_do_not_disagree_about_which_loops_exist` until it is
repointed; that pin is deliberately the weakest useful one (it cannot compare
their numbers without an `AppState`) and it catches the shape of the original
defect, which was hardcoded rows under a live status column.

**Next on this surface:** a per-agent view built from `loop_api` rather than from
SQL, which is what lets the old handler go.

---

## Loop 4 — wired end to end, and its seam is now closed

There was nothing to build. The chain is complete: all three console paths chain
`.for_forecast(...)` and pass the driver; `InvocationProvenance` carries both
halves; `execution_stream.rs` retains claims (added last session, never
exercised); `classify_claim` resolves a prefix from a stated driver with no
workspace. **It is awaiting a run on a saved forecast.** An unsaved draft
correctly yields no claim.

What *was* missing is a guard on the seam it travels through. The two keys had
**four independent spellings across two crates** — serde field names in
`negotiate.rs`, string literals typed separately in `execution.rs` and
`execution_stream.rs` — and nothing compared them. A rename on either side
yields zero claims silently, which is **exactly the observation the platform
already has**: `forecast_agent_claims` has been empty since mig-187, so there is
no alarm to fall silent and no state to compare against.

Now one declaration and one reader: `fermi::claim_outcome::{KEY_FORECAST_ID,
KEY_DRIVER, binding_from_invocation}`, used by both handlers, pinned by
`crates/fermi-console/tests/invocation_envelope.rs` — which serialises a real
`InvocationProvenance` and reads it with the server's own parser. Verified from
both sides: renaming the console's field and renaming the server's constant each
turn it red.

The envelope read was the one piece of the two handlers' deliberate mirroring
worth de-duplicating: two independent reads of two JSON keys can diverge, the
divergence is invisible, and there is nothing for a shared function to paper
over. The surrounding episode/credit/royalty logic stays mirrored.

## Loop 1 — reach, and a two-way ratchet

The second `OutcomeContract`. `loop1.retrieved` promises the rules come back to
the agent that made them; **7 of 84 agents that own a rule have ever had one
retrieved.** A rule nobody retrieves is a dream cycle nobody woke from: the
agent paid for the consolidation, the row sits in `semantic_rules`, and the next
prompt is built without it.

The floor is **8%, the measured value** — not a target. Falling below it is a
regression; rising above it fails too, demanding the floor be raised. That is
`uninstrumented_swallows_may_only_decrease` pointed the other way, and it is
what lets this assert something without anyone inventing a number. Verified all
three ways: stale floor, fallen reach, and zero reach.

`Open` (nothing receives) is the only arm asserted outright. `reach_pct(0, 0)`
returns 0, registered as a falsification: every other emptiness in this codebase
has had a version that read as success, and `0/0 = 100%` would make a loop that
has produced nothing report perfect reach on the rung built to catch that.

### What is still not measured, precisely

Reach is the weaker claim and the honest one. **Whether retrieval changed the
agent's output is unmeasured, and cannot be taken from stored data:** it needs a
control arm, and forming one means suppressing rule injection for a turn, which
nothing does. `loop1.retrieved`'s `does_not_show` says so. Loop 1's own
`extraction_utility` signal has fired twice.

That is the remaining build, and it is now the *only* thing on this list that
needs new machinery rather than a run.

---

## Start here

### 1. Loop 4 is the keystone, and it is one gate

`apply_agent_multipliers` is wired, binds `forecast_id`, and mig-213's schema is
live. The gate is `execution.rs:353`: a run needs a workspace **or** a forecast.
Every quantified judgement on file was produced with neither.

It is the keystone because **both of `outcome_trust`'s exit conditions run
through it.** `attribution::counterfactual` already computes what each agent's
claim was worth — exact Shapley credit from one real forecast, no extra runs —
and has never executed because `forecast_agent_claims` is empty. Writing
per-agent calibration from that instead of copying the forecast's is what clears
`loop5a.scored/uniform`.

So: one bound run unblocks Loop 4's first link, which unblocks the attributor,
which clears the largest declared gap. Everything else on this list is downstream
of it.

### 2. `native_evaluators::loop_stalled_in_code` over-claims

With no loop stalled in code it returns `Healthy` with *"the rest are idle rather
than broken"* — a claim about four loops currently reading `unobserved` or
`awaiting_agent`, states `panel_absence` classifies `Unknown` precisely because
no claim is available. The function already has the right pattern: `probe_failed`
downgrades to `Inconclusive` rather than being absorbed.

Pre-existing; `awaiting_agent` widened it. Left alone because it flips a live
evaluator platform-wide and `each_evaluator_fires_for_its_own_reason` pins the
shape. Owner's call.

### 3. The measurement that still does not exist

`outcome_trust` asks whether a metric *can* discriminate. It does not ask whether
the loop **changed behaviour** — nothing anywhere compares an agent's output with
retrieved rules against its output without them, and Loop 1's own self-measure
has fired twice. That is still the build, and it now has a place to live.

---

## Do not

* **Do not backfill `process_projection_commits`.** Blocked twice now.
* **Do not fix a lower link before the first empty one.** Loop 4 stalls at the
  request shape, not at `composition_dream_handler`.
* **Do not add a check without a falsification.** Build failure in both
  directions: an unregistered public decision in a trust module, and a new
  corpus-walking suite with no named proof.
* **Do not remove a `KNOWN_GAPS` entry** without the ratchet agreeing the gap has
  closed. It will tell you.

---

## Method — carried forward

* **A break must assert the state it names, not the substitution.** Three times
  now: a `models` string shortened only on its first line; a probe that left
  `resolved_at` at its default so both expressions agreed and it **passed against
  production with the tautology live**; and two registry pairs that returned
  before the branch they claimed to cover.
* **Read the live definition; do not reason from the migration file.**
* **One pair per incident, not per branch** — and say so, so the registry is not
  read as exhaustive. Two of `classify_discrimination`'s arms are owned by that
  module's unit tests, verified by deletion before being left there.
* **A threshold chosen after seeing the data is fitted to it.** `Sparse` reuses
  `min_events` for exactly this reason.
* **Turning is not closed.** A chain that produces rows every stage is entirely
  compatible with producing a number that cannot distinguish the things it is
  named after, and two of six loops are in that state.
* **A threshold must be a measurement or a ratchet, never a target.** The reach
  floor is what was measured, and the check fails in *both* directions so the
  number cannot go stale. A target set after taking a reading is fitted to it.
* **The seam nobody guards is the one whose failure looks like the status quo.**
  Four spellings of two JSON keys survived because a break in them produces an
  empty table, and the table was already empty.
