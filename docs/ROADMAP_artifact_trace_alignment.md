# The artifact trace — alignment and route

**Answering:** `docs/UX_REQUEST_artifact_trace.md`
**Date:** 2026-08-26. Every number below was re-measured against production
today; where it differs from the request's 2026-08-24 figures, both are given.

---

## Short version

The frame is right, and it is the architecture's own frame rather than a UI
preference. Accept it.

**And `unknown` is not the trace's problem to solve.** It is overwhelmingly the
*subject* declaring no structure to check against — not a stalled loop, not a cold
counter, and not a contract the platform failed to write. That has now been
measured, given a type, and made into a worklist (§0). It also means the retrofit
effort is a **dependent** track: it needs the substrate to tell it what to do, and
the substrate now can.

---

## 0. `unknown` is undeclared, not broken — and that is now a first-class answer

**Built this session:** `src/declaration_ladder.rs`, `GET /api/declarations`.

### What was measured

Of the 206 agents that have produced an episode:

| | agents | ports | output type | checkable schema | field contract |
|---|---|---|---|---|---|
| **real** | **96** | 93 | 8 | **2** | **7** |
| **`test_agent_*`** | **110** | 0 | 0 | 0 | 0 |

So the dominant cause of `unknown` across every surface built so far is neither a
loop that has not closed nor a check nobody wrote. **It is 110 fixtures that
declare nothing and 89 real agents that have not declared the one rung the trace
needs.** 3,571 of 3,576 episodes carry no grounding stamp for exactly that reason.

A detail worth having: **no real producing agent is fully `Opaque`** — all 96 have
at least one card rung, and all 110 opaque rows are cruft. Legibility separates
the two populations perfectly today. That is a coincidence of the current fleet
rather than a property, which is precisely why `disposition` checks cruft *before*
legibility and why a test asserts that ordering against a row the fleet does not
contain.

### Why it needed a type and not a dashboard

`panel_absence::Resolver` already had five ways to explain an absence —
`Liveness`, `LoopStage`, `Gate`, `GateLedger`, `Unresolved { why }`. None of them
is *the subject declared nothing*, so that case collapsed into `Unresolved`, which
reads as **the platform has not written a contract for this**:

> `Unresolved` is a work item for **us**. `Undeclared` is a work item for the
> **agent's author**.

Collapsing them made 89 agents' missing declarations look like 89 contracts the
platform owed. It owes none of them. Getting this wrong does not produce a wrong
number, it produces a **wrong backlog** — and a backlog nobody can act on is one
nobody does.

`declaration_ladder::Silence` now has four causes with four owners:

| silence | owner | remedy |
|---|---|---|
| `ColdCounter` | nobody | resolves on the next request |
| `NothingTraversed` | nobody | a throughput or product question |
| `Undeclared { rung }` | **the agent's author** | declare that rung |
| `Unresolved` | **the platform** | write the contract |

The ordering inside `attribute` is the whole content of the function, and it is
the reverse of the order the causes were found in: cold counter first (on a fresh
boot it explains everything spuriously), then undeclared (an agent that declared
nothing cannot produce a checkable artifact, so asking about throughput is
premature), then nothing-traversed, and `Unresolved` only when none of the three
applies.

### The two worklists are separated, and that is the point

`Disposition` is `Retrofit` | `Prune` | `Legible`. Pruning a `test_agent_<uuid>`
row is a delete behind an existing safety gate; retrofitting `weather_oracle` is
authoring work with someone who knows the domain. **Served as one number the
retrofit looks twice its real size** — 96 agents, not 206 — and its real size is
what decides whether it is worth doing.

The same reasoning fixes the coverage denominator: the ports rung is **93 of 96**,
not 93 of 206. Including cruft would report an almost-complete rung as a third
done and make the whole ladder look hopeless.

### The ladder, and what each rung buys

Ordered by **what it costs an author**, not by importance, because that is the
order a retrofit will actually proceed in.

| rung | declares | unlocks | coverage |
|---|---|---|---|
| `ports` | accepts / produces labels | `port_trust::bind_input` at every execute boundary; the seam census | 93/96 |
| `output_type` | `output_contract.produces_schema` — a type *name* | `envelope::declared_type`, so a delegated consumer knows what it was handed | 8/96 |
| `output_schema` | `output_contract.schema` — a checkable object | `schema_validate` at the hop, which is what makes a seam's `verified` mean *a schema resolved on both sides*; and the wrapped output type | 2/96 |
| `field_contract` | which tool could have supplied each field | **the grounding rung, `assertions[].basis`, the per-field grade, the weakest-link floor, and the verification queue** | 7/96 |

`produces_schema` and `schema` are two rungs, not two spellings — one names a
type, the other is validatable. `envelope.rs` reads both, for different purposes.

**`fermi_contract` is deliberately not on the ladder**, and it is the case that
tested the design. 15 of 96 real agents carry one, which would have more than
doubled a rung's coverage — and it holds domain configuration for forecast agents
(`finding_labels`, `multiplier_range`, `seed_facts`) that no trust surface can
read. Every rung must say what it `unlocks`, asserted with a length floor,
precisely so the ladder cannot become a checklist that inflates with things no
consumer can use.

### No target, and deliberately no ratchet

Coverage is reported and never compared to a figure. The house rule is that a
threshold must be a measurement or a two-way ratchet and never a target — and here
even a ratchet would be wrong, because **new agents arrive undeclared by
definition**, so a ratchet on the fleet count would fire on entirely correct
behaviour and §5.2 says what happens next.

One thing *is* safely ratcheted, two ways: the count of agents in
`FIELD_CONTRACTS`. It is a hand-maintained const, nothing arrives in it by
accident, and removing a contract takes an agent's whole output back to
unverifiable. Pinned at 9.

### The retrofit is a dependent track, not this one

Stated explicitly because it is the sequencing that matters: **rewriting or
pruning legacy agents depends on this infrastructure working, and not the other
way round.** `GET /api/declarations` is what makes that effort actionable — it
emits, per agent, the cheapest missing rung and who owns it. Before it existed the
effort had no worklist, so "retrofit the agents" was a sentence rather than a
plan.

Two things it deliberately does **not** do, and both should stay out of scope
until the infrastructure has run for a while:

* it does not delete anything — pruning stays behind
  `/api/admin/agents/cleanup-test-cruft`'s existing gate (zero executions, past a
  grace period, never curated or system tier);
* it does not rank the retrofit. Which of 89 agents to bring under a field
  contract first is a product judgement about which outputs anyone relies on, and
  a coverage number is the wrong instrument for it.

### One integration deferred

`panel_absence::Resolver` should gain an `Undeclared` variant so panel emptiness
is attributed the same way. Not done here: adding a variant means a pass over
every declared panel, and that file has broken the build under parallel sessions
three times. It is a clean follow-up once the file is quiet — the answer already
exists and `panel_absence` would only be pointing at it.

**One correction changes the plan, and it makes the plan cheaper.** ① is blocked
on something other than `gate_decisions.episode_id`. The data the trace wants —
which field, what the model actually claimed, why it was wrong — **is already
computed on every execute path and then reduced to a tag count.**
`grounding_trust::Report` carries `path`, `removed` and `kind` per violation, and
`stamp_grounding` turns the whole thing into `grounding:violations` and
`grounding:count-3`. The fabricated value, which the request specifically asks us
not to strip, is discarded before anything durable sees it.

The right home for it is the pair the request already identified: `episodes
.assertions[]` and `assertion_verifications`, *"a writer, not a schema."* Which
means **① and ③ are one piece of work**, it needs no new tables and no new
columns, and it also fills gap 2 (`basis` empty) and unblocks ④ — because the
grounding contract's `Grounding` variant **is** the basis.

So the sequence inverts. ③'s writer comes first, ① assembles on top of it, and
② splits into a free half worth shipping immediately and an expensive half the
free half tells you whether to build.

---

## 1. The frame is already the architecture's

> A loop is a path an artifact takes. A gate is a checkpoint on that path.

That is not a re-reading of the model, it is the model. The mapping is exact and
nothing needs renaming:

| trace concept | already exists as | state |
|---|---|---|
| the route's ordered checkpoints | `command_registry::Command.gates` — per route, in order, each `control` or `metric` with a reason | **[E]** |
| a checkpoint firing | `gate_trust::decided(Gate, Decision, reason)` | **[E]** |
| when a checkpoint fires | `gate_trust::Clock` — `Admission` / `Invocation` / `Standing` | **[E]** |
| the per-field mark | `grounding_trust::PROVENANCE_VALUES`, ten rungs incl. `pending_tool` / `pending_human` | **[E]** |
| weakest link | `grounding_trust::floor` / `extracted_floor` | **[E]** |
| what settles a field | `Grounding::Sourced { tool }` — the tool is named in the contract | **[E]** |
| the census, demoted to a lens | `loop_model` + `loop_api` | **[E]** shipped |

The census was not the wrong thing to build; it was the wrong thing to build
*first*, and only because `loop_model` was the layer that existed. The population
view answers *"is this loop turning"*, which is the operator's question. The
trace answers *"what happened to this thing"*, which is everyone else's.

**They are the same structure read from opposite ends, and one implementation has
to serve both** (§3.4). Concretely: the trace must not recompute a rung outcome
that `gate_trust` already holds, and `loop_api` must not learn to answer
per-episode questions. The join between them is the artifact, and that is the new
object.

---

## 2. The correction: where the trace data actually is

The request asks for four things under ①. Taking them in order of what they
actually buy:

### 2.1 `episode_id` on `gate_decisions` — yes, but it is not the blocker

One column, worth doing, and it buys the **coarse** rungs: credit, attachment,
input binding, admission. Those are per-request verdicts with a single reason
string, which is exactly what `gate_decisions` is shaped for.

It does **not** buy the grounding rung, and grounding is the only rung that
produces the `expected` / `actual` diagnosis the request says *"makes it a
diagnosis rather than a red dot"*. `gate_decisions.reason` is one free-text
column per decision. A grounding verdict is *n* per-field findings. Pushing them
in as prose would make the trace's most valuable field unparseable, and pushing
one row per field in would do to `gate_decisions` exactly what the request
correctly refuses to do to `anomaly_events`.

### 2.2 Grounding `Counted` → `Recorded` — no, and for a better reason than cost

This is the change to skip, because the record it would produce is the wrong
record. Here is what the report contains:

```rust
pub struct Report {
    pub violations: Vec<Violation>,           // path, removed, kind
    pub provenance: Vec<(String, &'static str)>,  // (block, ladder rung)
}
pub struct Violation {
    pub path: String,        // → fields[].name
    pub removed: Value,      // → fields[].value. THE FABRICATED CLAIM.
    pub kind: ViolationKind, // → UngroundedField | NarrativeLeak | ContradictsCanonical
}
```

`Violation.removed` is documented in `grounding_trust` as *"retained rather than
discarded so the caller can quarantine it for later comparison against a real
source — the difference between 'tag for reprocessing' and 'delete'."* The
retention was designed. It just never reaches storage: `stamp_grounding` emits
`grounding:violations` plus `grounding:count-N` plus one `prov:<block>-<rung>`
tag per block, and the paths and the values are dropped with the local.

**So the trace's headline field already exists as a Rust value on every execute
path and has never been written down.** `Retention::Recorded` would give it a
home with one reason string. The per-claim tables would give it its actual shape.

### 2.3 The per-claim tables are already right, and this is the keystone

The request's own table says `assertion_verifications` *"needs a writer, not a
schema"*, and that its `actor_kind` maps 1:1 onto `pending_tool` / `pending_human`.
Both true. What the request could not see is that **the writer's input is the
grounding report**, and the mapping is total:

| report | assertion / verification |
|---|---|
| `Violation.path` | the field |
| `Violation.removed` | `assertions[].value` and `.raw` — kept, marked, never stripped |
| `Violation.kind` | the `expected` / `actual` diagnosis |
| the contract's `Grounding` variant | `assertions[].basis` — **gap 2 closes here** |
| `Grounding::Sourced { tool }` present | `actor_kind = tool`, `settleable_by = tool` |
| `Grounding::Unsourced` | `actor_kind = human`, `settleable_by = null` |
| `Report.provenance` | `fields[].grade`, already ladder values |
| `floor(...)` over the block's rungs | `fields[].floor_of` |

Gap 2 deserves emphasis because the request calls `basis: []` a separate problem.
It is not. `basis` is empty because nothing has ever had an opinion about what a
claim rests on at write time — and the grounding contract is precisely that
opinion, declared per field, for the nine agents that have one. Wire the writer
and `basis` stops being empty for exactly the population that can compute it.

**Net:** ① `fields[]` + ③ entirely + ④ derivable + gap 2, from one writer, over
two existing tables, with no migration.

### 2.4 The hash and `parent_episode_id` — real, and separable

Both genuinely new and both cheap, but neither is on the critical path for a
legible trace. `parent_episode_id` has 4 non-null rows of 3,576; the correction
chain is a second screen. Keep them in the plan, after ①.

---

## 3. The number that should change how ① is scoped

This is the uncomfortable part and it is better said now than discovered in
review.

| | |
|---|---|
| episodes | **3,576** |
| carry a grounding stamp at all | **5** |
| carry a grounding **violation** | **0** |
| `assertions` column non-null | 166 (was 152) |
| …of which an **empty array** | **124** |
| …actually carrying claims | **42** (94 assertions: 75 `multiplier`, 19 `probability`) |
| assertions with non-empty `basis` | **0** |
| `gate_decisions` rows | **0** |
| `assertion_verifications` rows | **0** |
| `episode_corrections` rows | **0** |
| `parent_episode_id` non-null | 4 |

**The grounding contract has engaged five times in 3,576 episodes and found
nothing wrong.** So the trace, built today and served over the existing corpus,
returns a journey with no checkpoints for 3,571 of 3,576 episodes. That is not an
argument against building it. It is an argument about what must ship *with* it,
and about what the actual lever is:

* **What must ship with it:** an episode with no rungs is **not a clean journey**.
  It is an unchecked one, and rendering it as a green route end-to-end is the exact
  over-read this whole architecture refuses — the same rule as
  `gate_trust::never_asked` and `liveness_trust::Inert`. `reading: "unknown"`,
  and the `detail` is no longer something the trace has to invent:
  `declaration_ladder::attribute` returns
  `Undeclared { rung: "field_contract" }` with `whose_work` → `AgentAuthor`, and
  `LADDER`'s `without_it` carries the sentence — *nothing can say whether the
  agent fabricated a value*. Section 6 makes this a required field rather than a
  convention, and §0 means it is a **sourced** answer rather than the trace's own
  guess about why it is empty.
* **What the actual lever is:** contract coverage. `grounding_trust
  ::FIELD_CONTRACTS` holds 98 contracts across **9 agents**, of which **7 have
  ever produced an episode** — against **206 agents that have.** Two of the nine
  contracts have never been exercised at all, which is worth stating separately
  because "9 agents are covered" is a true fact about the const table and a false
  one about the system. You cannot code your way to a populated trace, and this is
  the same lever as Loop 2's — which is why the two roadmaps converge (§5).

Note also the 124 empty `assertions` arrays. `[]` and `NULL` currently read the
same to every consumer, and they are different states: *the extractor ran and
found no claims* versus *nothing has ever looked*. Worth fixing while the writer
is being built, because it is the same defect class one field over.

---

## 4. The five open questions, answered from the code

**Q1 — is the hash over raw output or the post-grounding document?**
Hash both, and the request's instinct is right for a stronger reason than it
gives. `grounding_trust::enforce` nulls ungrounded fields *in a local copy* and
the persisted `response_text` is the un-stripped original — deliberately, because
*"retention is a precondition for every later form of verification and a digest is
not a record."* So two documents genuinely exist, the difference between them is
literally the grounding verdict, and a single hash would have to pick one and
silently discard the evidence of the other. `input.hash` / `output.hash` /
`output.hash_post_grounding`.

**Q2 — where does `strategist.mode` live?**
Nowhere yet, and `teams.workflow_meta` is the wrong guess. `teams
.coordination_strategist_id` already names the strategist *agent*, and the mode is
a property of that agent, not of the workspace — `pipeline_strategist`,
`vote_strategist`, `debate_strategist` and `cohere_and_coordinate` are four
different agents. Derive `mode` from the resolved strategist and do not store it;
a stored copy is a second answer that can disagree with the agent actually
invoked, which is the bug `coherence_shelf_does_not_hardcode_a_strategist` exists
to prevent.

**Q3 — what settles `verified` on a seam?**
Schema resolution, as the request suspects, and the 3-of-10 figure is the reason
to defer the whole table rather than to weaken the definition. See §5, step 5.

**Q4 — does promoting grounding to a control break existing callers?**
Yes, and the request's own §"one request on shape" is the answer to its own
question — which is worth spelling out because the two sections were not written
as connected.

The current state is not a considered demotion. `command_registry` declares it
`metric` with the reason attached: *"`enforce` mutates a local doc that is
dropped; the persisted response_text and the rendered body are both un-stripped …
the endpoint a third party calls reports fabrication rather than preventing it."*
So somebody documented it, and nobody chose it.

Promoting it *as it stands* means fields go `null` in a third party's response
body with no version negotiation — a real break, and it also destroys the
`removed` value, which is the one piece of evidence that could ever answer which
model fabricates what.

**The wrapped shape removes the dilemma.** If `produces` declares
`{ value, provenance, verified }`, grounding **marks** instead of nulling. A
naive consumer breaks loudly on the wrapper rather than believing a bare value —
which is the entire safety property the promotion was for — the evidence
survives, and `port_trust` checks the declared type at the seam, so it is an
existing rung rather than a new mechanism. Callers opt in by declaring the
wrapped type on the card, so there is no flag day.

That reframes the ask: **not "promote grounding to a control", but "make the
wrapped output type declarable, and grounding is a control for every agent that
declares it."** Same safety outcome, no break, and it is a card change rather than
a gate change.

**Q5 — `Standing` or `sweep`?**
`Standing`, and it is already settled in the code: `Clock::Standing`'s own doc
reads *"Boot and sweep."* Sweep is one of the two occasions `Standing` covers, so
it is a narrower word for a wider clock and would lose the boot case. Use
`Standing` in both surfaces; "sweep" is fine as UI prose for the scheduled half.

---

## 5. The route

Five steps. Steps 1 and 2 are the request's ① and ③ merged; step 5 is where the
expensive, low-yield work has been deliberately pushed.

### Step 0.5 — document recovery — **done, and it was blocking step 1 entirely**

Step 1 was going to populate `assertions[].basis` from
`grounding_trust::response_floor`. Before writing it I grouped the population the
way the method requires, and **the writer would have been a no-op.**

`response_floor` used a bare `serde_json::from_str` and returned
`unavailable_no_tool_source` the moment it failed, with the comment *"Prose. An
extraction from prose is ungrounded by construction: there are no typed fields to
have been sourced."* That is a true statement about a `from_str` parse presented
as a fact about the agent's output — §19's defect, again.

Agents wrap their document in prose, and `agent_backend::envelope::extract_json`
has always known that: it does a balanced-brace scan for the largest object, which
is why `handlers::execution` grades responses this function called ungradeable.
**Two implementations of "get the document out of the response", disagreeing, with
the weaker one behind the trust calculation.** §3.4.

Measured over production, across the seven contracted agents that have run and
retained a response (`tests/response_floor_recovery.rs`):

| | |
|---|---|
| retained responses | **94** |
| bare JSON | **0** |
| document embedded in prose | **64** |
| no document at all | 30 |

So the old parse graded **0 of 94** and dismissed every one of them. And **all 28
semantic rules carrying a provenance floor read `unavailable_no_tool_source`** —
28 of 28 — because `provenance_oracle` computes a rule's extraction floor by
re-running this function over the episodes it was consolidated from.

After the fix, the floors as graded:

| floor | strength | n |
|---|---|---|
| `model_inference` | 1 | **44** |
| `tool_no_match` | 0 | 20 |
| `unavailable_no_tool_source` | 0 | 30 |

**Read the strength column, not the token** — and this correction was needed in my
own first draft of the probe, which reported "64 graded above unavailable".
`tool_no_match` sorts above `unavailable_no_tool_source` and both carry strength
**0**: a different word for the same amount of reliance. The real change is that
**44 responses moved from strength 0 to strength 1**, and 20 got a more accurate
strength-0 diagnosis (*the sourced block is absent* rather than *there is no
document*).

The other number worth having: **0 of 94 reach strength 2.** No contracted agent's
response is currently reproducible, which means the verification queue in step 1
will route essentially everything to `pending_*` — and that is Loop 2's content.

**Stored rule floors are not backfilled.** The 28 carry the old parse's verdict
and nothing here rewrites them; backfill is off the table in this codebase and
`provenance_oracle` says so. The number to watch is whether rules written *after*
this change still land at `unavailable`.

`scripts/break_response_floor.py` — 2 breaks, both land. The first restores the
exact shipped code.

### Step 1 — the grounding writer (the keystone), now unblocked

Write `grounding_trust::Report` into `episodes.assertions[]` and
`assertion_verifications`, at the one place the report is produced. No migration.

* fills `assertions[].basis` — which is only worth doing *because of step 0.5*.
  Before it, `response_floor` returned `unavailable` for all 94 responses and a
  `Multiplier` with basis `["unavailable"]` floors exactly where an empty basis
  already does, so the write would have changed nothing. Now 44 grade
  `model_inference`, and a multiplier reasoned from present blocks becomes
  `model_inference` instead of `unavailable_no_tool_source` — which is the correct
  answer, and is what "makes `floorOf` real for output as well as for memory"
  actually cashes out to;
* one caution found on the way: **all 94 existing assertions are 75 `Multiplier`
  and 19 `Probability`, and zero `Quantity`.** `route()` sends a non-verifiable
  kind to `InheritFromBasis`, so none of them produces a queue item by design —
  *you cannot verify a multiplier.* The queue's first real content will come from
  contracted **fields**, not from prose-extracted numbers, so the writer must
  cover both and the field half is the one that fills the queue;
* keeps `Violation.removed` as the claim, marked, never stripped;
* routes each item by whether `Grounding::Sourced { tool }` exists —
  `actor_kind = tool` and `settleable_by = <tool>`, or `actor_kind = human` and
  `settleable_by = null`;
* one implementation, called from wherever `enforce` is called, so the nine
  bespoke call sites cannot drift into nine writers.

**Delivers ③ complete, ④ derivable, gap 2 closed, and Loop 2 its content.** This
is also the step that answers the original ask from this whole line of work —
*"I need to be able to see a sourced anomaly I can correct"* — because a
`pending_human` row with the claimed value and a named field is exactly that, and
`assertion_verifications` already has the CHECK requiring a citation before a
person may call something `human_sourced`.

### Step 2 — `GET /api/episodes/:episode_id/trace`

Assembles from `episodes` + `assertions` + `assertion_verifications` +
`gate_decisions`. Needs the one column, `gate_decisions.episode_id`, for the
coarse rungs. Recomputes nothing: `command_registry` supplies the rung order and
each rung's control-or-metric status, `gate_trust` supplies the verdicts,
`grounding_trust::floor` supplies `floor_of`, and **`declaration_ladder` supplies
the reason an empty trace is empty** — which is the field that stops it being
misread and is now a lookup rather than a judgement.

Ships with the caveat from §3 as a required field, not a doc note.

### Step 2.5 — the declaration resolver in `panel_absence` — **done, and the plan was wrong**

The proposal in §0 was a `Resolver::Undeclared` variant applied across the
unresolved panels. **That was a category error and the work is smaller and
narrower than proposed.**

A `Resolver` answers *"which contract can explain THIS PANEL's emptiness,
platform-wide"*. `declaration_ladder::Silence::Undeclared` answers *"why is there
nothing for THIS SUBJECT"*. Different questions at different scopes — and reading
the five unresolved panels, **four of them genuinely are the platform's work**:
nothing watches dyad formation, `eval_runs` has no liveness contract, nothing
watches roster composition. Relabelling those as the agents' fault would be the
exact original mistake in reverse, moving our backlog onto authors who cannot act
on it.

What shipped instead is `Resolver::Declaration { rung }` — narrower, and it says
something a panel can own: *the platform has a contract and it measures a
declaration.* Exactly one panel uses it, and that panel asked for it: `ecology
.seams`'s own `why` read *"a census in a comment is not a contract. Resolve by
promoting the census to a rung."* The ladder is that rung, so leaving it
`Unresolved` would have been leaving a resolved thing marked unresolved. The
shrink-only ratchet goes 5 → 4.

Its three states are the point:

| state | reading | means |
|---|---|---|
| `no_census` | `unknown` | the measurement failed. **Not** zero coverage — that is the most alarming reading available and a failed query has no standing to make it |
| `undeclared` | `unknown` | nobody declares the rung. The panel cannot fill because its input does not exist. **The authors' work**, with a remediation pointing at `/api/declarations` |
| `declared` | `idle` | the rung is declared and the panel is still empty — a real finding about convergence rather than a shrug |

That last row is what changed `ecology.seams` from *"no contract watches which
ports can form a seam"* to *"93 of 96 real agents declare ports and 13 labels
appear on both sides"* — the vocabulary exists and is fragmented. Verified live.

It also found a stale number: the panel's comment cited `513 labels, 14 on both
sides, 499 orphans` from a one-off script. Re-measured: **289 / 236 / 13**. A
census in a comment drifts, which is what its own `why` said.

**And it surfaced a §3.4 duplication.** Adding `Observation::declarations` made
the compiler enumerate five hand-built `Observation` literals; two of them
(`rounds.rs`, `specimen.rs`) were byte-for-byte `Observation::collect`. They now
call it. Without the new field they would have kept drifting silently — every
declaration-resolved panel on those endpoints would have reported `no_census`
while the endpoint looked fine.

The per-subject `Undeclared` answer stays where the subject is known —
`declaration_ladder::attribute` — and is what step 2 consumes.

### A convergence worth knowing about

A parallel session has landed **`src/contract_sketch.rs` and
`docs/DESIGN_typed_output_contracts.md`** — a generator that makes a typed
`output_contract` cheap to author, with the first migration (`equity_analyst`)
done.

That is the remedy for this ladder's **worst-covered rung**: `output_schema` sits
at 2 of 96, and their diagnosis explains why — *"the contract was never disputed,
only unaffordable"*: six authored decisions expand to thirty-five artefacts, and
an author who writes that six times copies the nearest neighbour. The ladder's
`output_schema` entry now names `contract_sketch` as its owner, so the retrofit
worklist points at the tool.

Their `TYPED_TIER_EXEMPT` (86 → 85) and this ladder's `output_schema` rung are
**not duplicates and it is worth keeping them apart deliberately**: theirs
ratchets *curated agents at publish*, this measures *producing agents at trace*.
Different populations, different moments — theirs is the supply of typed
contracts, this is the coverage observed in the fleet. The gap between them is
the interesting number: an exempt agent that never runs costs nothing; an exempt
agent that runs constantly is where the missing checks actually bite. Recorded in
`declaration_ladder`'s module docs so a third list has to argue against it.

### Step 3 — Loop 3 Stage 0

Independent of the above, already scoped, small: declare the intention at
dispatch (`workspace/messages.rs`, the @-mention path) through the existing
`declare_intention`, rather than asking a model to. Conflict-check for
observability only. Worth keeping in the queue here because it is the last
structural gap in the loop set and it does not compete for the same files.

### Step 4 — hashes, `parent_episode_id`, the wrapped output type

The seam check and the correction chain, plus Q4's real answer. Separable, and
each is small. The wrapped type is the one with design in it: it wants a card
change, a `port_trust` check, and a decision about what a mixed composition does.

### Step 5 — declared seams, and only after the free half

`②` should split, because the two halves have very different costs and the cheap
one decides the expensive one:

* **Free now:** `seams: { labels, seam_forming, orphans }` from `agents.accepts` /
  `agents.produces`, which already exist as `text[]` columns. Re-measured today:
  **289 distinct `produces` labels, 236 distinct `accepts` labels, 13 forming a
  seam.** No table, no migration, and it is the number that says whether declared
  seams are worth building.
* **Deferred:** the declared-seam table. With 13 seam-forming labels the first thing it
  buys is measuring convergence, which the free half already buys. Build it when
  the seam count is rising, or when a strategist needs to *declare* an edge that
  the labels do not imply — which is the real motivation and is a different
  feature from drawing the graph.

`members`, `ports`, `calibration`, `cost_per_run`, `runs`, `strategist.agent_id`
and `budget` all exist and can be served in ② immediately, with `declared_seams: []`
and `seams` populated. That is a useful screen without the table.

---

## 6. What this means for the abstraction

`src/surface.rs` is the **population-level** trust abstraction: three parts that
are answers and are never shared (declared model, measurement, interpretation),
two that are shared (`Door`, `Caveat`), and one router scan. Three instances:
`loop_api`, `gate_api`, `evaluator_api`.

The trace is the **instance-level** counterpart, and it should be built as a
declared abstraction rather than a bespoke handler, for the same reason: there
will be more than one. An episode's trace is the first; a forecast's trace and a
composition run's trace are the obvious next two, and they will want the same
five parts.

Proposed `src/artifact_trace.rs`:

| part | owner |
|---|---|
| the rung sequence for a route | `command_registry::Command.gates` — **not** re-declared |
| a rung's verdict | `gate_trust` / `grounding_trust` — **not** recomputed |
| per-field grade and floor | `grounding_trust::floor` |
| **reading** | `panel_absence::Reading`, three words, same as everywhere |
| **caveat** | reused from `surface::Caveat` |
| **door** | reused from `surface::Door` — and there is a real one: verify a claim |

Two rules it must carry, both learned expensively elsewhere in this codebase:

1. **No rungs is `unknown`, never `idle`.** §3's number is why. This is the
   single most likely way the trace ships misleading, because the misleading
   version looks better.
2. **The trace holds no opinion of its own.** Every verdict it renders belongs to
   a module that already owns it and already has a falsification registered.
   The moment the trace computes a grade, there are two answers to one question
   and the surface is the one people will read.

### A note on the two judgement ledgers

The request says, correctly, *"not a new review queue —
`assertion_verifications` is it."* This session added
`gate_decision_reviews`, and the distinction is real: one records whether a
**claim** is true, the other whether a **control's refusal** was right. Different
subjects, different actors, and neither can answer the other's question.

But the *shape* is now instantiated twice — append-only, `actor` +
`actor_kind ∈ {tool,human,platform}`, a verdict from a closed vocabulary, and a
CHECK requiring a citation for the expensive verdict. **A third instance should be
a declared pattern rather than a third table**, the way `surface.rs` became a
pattern after the second trust domain. Flagging it now so the third one is a
decision instead of a habit.

---

## 7. What is being said "no" to, and why

* **`Retention::Recorded` for grounding** — the record it produces is the wrong
  shape (§2.2). The per-claim tables are the right home. `gate_decisions` keeps
  the coarse rungs.
* **the declared-seam table in the first pass** — 13 of 289 (§5, step 5). The free seam census
  gives the same information and tells us whether to build it.
* **`strategist.mode` as stored state** — derive it from the resolved strategist
  (§4, Q2).
* **Promoting grounding to a control as-is** — do the wrapped output type
  instead. Same safety property, no break, keeps the evidence (§4, Q4).

## 7.1 Queued — the chained probe

**Deferred deliberately, 2026-08-31, after the single-call probe shipped.**

The probe runs *one* call. The reference case cannot be closed with one:

```text
advanced_metrics.xg  →  contract names  fixtures/statistics.expected_goals
                        fixtures/statistics requires  fixture=<id>
                        the record carries no fixture id
                          (the agent called standings, teams/statistics×2,
                           players×2, injuries, players/topscorers — never fixtures)
```

So pressing run on the xG row returns `MISSING PARAMETER: fixture`, honestly and
uselessly. Completing the proof needs two calls: `fixtures?league&season&team` →
take an id → `fixtures/statistics?fixture=<id>`. That is a **chain**, and a chain
is a different object from a call: it has intermediate state, a step that can
fail on its own, and a question about which step the reader is endorsing.

Not built yet, and the reason is readability rather than difficulty. A chain adds
a second axis to a row that does not yet have a stable first one. Layout first;
then the chain has somewhere to live.

What exists already and does not need redoing:

* `field_probe::parse_hint` gives the endpoint per field, from one parser.
* `field_probe::search` locates a name anywhere in the full body, as key or as
  value, with its path — which is what a chain's last step would report.
* `endpoint_matches` already refuses to let one step's answer be cited for
  another step's question. A chain makes that check load-bearing rather than
  advisory.

## 8. What we need from the UI side

1. **Confirm the trace's `unknown` state is renderable.** An episode with no
   rungs is 3,571 of 3,576 today, so it is the *default* screen, not an edge case.
   It now arrives with a cause, an owner and a named next declaration rather than
   as a blank, so there is something real to draw — but if that state cannot be
   made to look like an answer rather than a loading failure, we should talk
   before step 2 rather than after.
2. **`GET /api/declarations` is available now and is probably a screen.** It is
   the honest version of "why is everything unknown", it carries the ladder with
   each rung's `unlocks` / `without_it` prose, and it emits the retrofit worklist
   with the cheapest next declaration per agent. Two numbers that must never be
   added together: `retrofit` (96 real agents) and `prune_count` (110 fixtures).
3. **Confirm `pending_human` is workable as the primary queue.** Step 1 makes
   Loop 2's queue real, and it will be small and slow — seven agents that have
   actually run, and zero violations found so far. A queue with four items in it
   needs different UI from one with four hundred, and this one starts at zero for
   reasons that are not a bug.
4. **The wrapped type is an A2A contract decision, not only a rendering one.**
   If the UI wants `{ value, provenance, verified }`, that shape becomes what
   `produces` declares and what `port_trust` enforces at the seam. Worth agreeing
   the exact wrapper before it is in a card, because cards are published artifacts
   and the shape is then hard to move.
