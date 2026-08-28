# Handoff — next session

**Date:** 2026-08-26 · **Reference:** `docs/AUDIT_loops_and_gates.md`,
`docs/UX_HANDOFF_trust_surfaces.md`

Two things closed this session: **Loop 3's terminal half**, and **the gate
surface's missing write side**. Both were structural rather than bugs.

---

## Verify first

```
# Defeat the stale fingerprint FIRST -- see the cargo warning below.
find src tests -name '*.rs' -newermt '-6 hours' -exec touch {} +

cargo test -p fermi --lib      # 846 tests, 845 passed, 0 failed
cargo test -p fermi --tests    # 68 suites, 1336 passed, 0 failed

python3 scripts/break_coordination_note.py      #  5 breaks, all seen
python3 scripts/break_declaration_ladder.py     # 12 breaks, all seen
python3 scripts/break_declaration_resolver.py   #  5 breaks, all seen
python3 scripts/break_response_floor.py         #  2 breaks, all seen
python3 scripts/break_graded_fields.py          #  7 breaks, all seen
python3 scripts/break_verification_queue.py     #  4 breaks, all seen
```

Live tiers (`set -a; . ./.env; set +a`, then `-- --ignored --test-threads=1`):
`verification_queue_contract` (3), `response_floor_recovery` (2),
`gate_review_contract` (5, needs a probe DB with migration 216),
`panel_absence_contract`, `liveness_contract`, `loop_api_contract`.

`cargo fmt --check -p fermi` reports one diff in
`tests/contract_sketch_corpus.rs`, which belongs to a **parallel session** — leave
it. Format your own files with `rustfmt --edition 2021 <file>`.

The gate-review breaks need a throwaway database, because **two of the six can
only be caught by one**:

```
docker run -d --name p -e POSTGRES_PASSWORD=probe -e POSTGRES_DB=probe \
    -p 55499:5432 postgres:16
PGPASSWORD=probe psql -h 127.0.0.1 -p 55499 -U postgres -d probe \
    -c 'CREATE EXTENSION IF NOT EXISTS pgcrypto' \
    -f migrations/214_gate_decisions.sql \
    -f migrations/216_gate_decision_reviews.sql
PROBE_URL=postgres://postgres:probe@127.0.0.1:55499/probe \
    python3 scripts/break_gate_review.py     # 6 breaks, all seen
docker rm -f p
```

**One red in the tree, and it is the documented pending-deploy one.**
`seam_vocabulary_contract::every_vocabulary_matches_its_check_constraint` reports
three seams on `gate_decision_reviews` as *"table does not exist … the migration
that declares it has not run"*. Migration 216 is written **and registered** in
`run_migrations`. It clears on the next deploy. The message distinguishes the
three causes; do not assume "pending" without reading it.

`panel_absence` and the eight other live tiers are green against production.

---

## What was done, and the argument for each

### Loop 3's terminal half — the brief now reaches member memory

**The defect was that the mechanism asked a language model to perform a side
effect.** `coordinator_observation` stood at 0 of 3,576 episodes. Nothing was
broken: `record_coordination_observation` existed, was dispatched, was exposed to
the strategist, and both the card's Stage 3 and the handler's prompt asked for it
by name. The strategist never called it, and there is no version of a prompt that
makes a side effect a guarantee.

The content of a coordination finding is a judgement and is the model's. The
*delivery* is bookkeeping and is ours. So:

* `src/coordination_note.rs` — `deliver(...) -> Delivery`, one implementation of
  the episode write, counted through `write_accounting::Sink::Episodes`.
  `AlreadyTargeted` is **not** a problem: the floor exists to be unnecessary.
* `tools_legacy::execute_record_coordination_observation` repointed at it, keeping
  its own strategist-authorisation check (that question only arises for a
  model-invoked call).
* `handlers/workspace/coherence.rs` delivers the brief to every member after a
  `depth=recommendations` run, skipping the strategist and any member the model
  already wrote to **during this run** (`Some(run_started_at)`).

It fills on the next `depth=recommendations` coherence run. Nothing to build.

### Gate decision review — the write side of the gate surface

`gate_api::GATE_DOORS` was `&[]` and the comment said the emptiness was the
finding. Half of that was right and stays right: **there is still no override.**
Nothing re-runs a gate, reverses a decision, or admits what one refused.

The other half was a hole, and the argument is arithmetic rather than ergonomic.
Every reading on the gate board comes from approve/refuse **counts**.
`refuses_everything` catches the Γ bug's signature exactly — asked, and approved
nothing. It cannot catch a gate that approves 90% of what it sees and refuses the
other 10% *wrongly*: that reads `discriminating`, which the surface renders as
healthy, and every counter agrees with it. **Correctness is not a property of a
count.** A reviewer is not a convenience on top of the measurement; it is the only
instrument that can see that failure.

* `migrations/216_gate_decision_reviews.sql` — append-only, migration 205's
  pattern. Load-bearing CHECK: `overturned` requires a rationale. `upheld`
  requires nothing, and **that asymmetry is deliberate** — making the cheap
  confirmation as costly as the finding leaves the denominator unknown.
* `seam_vocabulary::GateReviewVerdict` — `upheld` / `overturned` / `unclear`,
  declared with `closed_vocabulary!` so the array comes off the type. Three new
  `VOCABULARIES` entries.
* `src/gate_review.rs` — `standing`, `reading`, `tally_from_counts`,
  `classify_write_error`, `is_client_error`. All five registered in the
  falsification registry.
* `POST /api/gates/:gate_id/decisions/:decision_id/review`, doors on `coherence`
  and `admission` only.
* `tests/gate_review_contract.rs` — five live probes.

Three decisions in there worth not undoing:

1. **The rationale rule has one implementation and it is Postgres's.** A Rust
   pre-check would read as defensive good practice and be a §3.4 violation; the
   end state is a Rust guard narrower than the constraint, an insert that fails
   anyway, and a 500 where the reviewer was told their finding had been filed.
   `classify_write_error` *translates* the constraint's refusal instead.
2. **`unclear` is a first-class verdict.** Forcing upheld-or-overturned when
   `gate_decisions.reason` says too little manufactures agreement. *The ledger
   does not record enough to review its own decisions* is a finding about the
   ledger, and `Standing::Inconclusive` reports it.
3. **Attribution is `real_user_id()`**, so under impersonation the review is
   recorded against the admin. It is an audit record and impersonation must not
   launder a judgement.

---

## Three things found while building, all of the audit's own defect class

Worth reading even if none of the above is touched again.

1. **Two source scans in `coherence.rs` were satisfied by their own assertion
   strings.** `source()` read the whole file, so
   `src.contains("t.coordination_strategist_id")` was matched by the line
   asserting it. `coherence_shelf_reads_the_registered_strategist` had therefore
   **never been capable of failing**, and the new coordination-floor check was
   written with the same flaw and stayed green when the entire delivery block was
   deleted. Fixed by `handler_source()`, which cuts at `#[cfg(test)]` and asserts
   the cut left something behind. All four breaks now land.
2. **`seam_vocabulary` had four hand-maintained lists of its own types**, one per
   test, and adding a fifth vocabulary would have silently skipped three of them.
   That is this module's own subject matter relocated from the Rust/Postgres seam
   to a Rust/Rust one. Collapsed into `for_each_owned_vocabulary!`.
3. **`every_registry_owned_vocabulary_is_generated_from_a_type` asserted a count
   equality** as a proxy for "every type is registered", and the proxy stops being
   one the first time a single type governs two columns. `ActorKind` now does, and
   the assertion failed on that correct state — a check firing on the behaviour it
   wants is §5.2's road to deletion. Stated directly instead.

And the one the break harness earned its keep on: **`classify_write_error`'s
constraint names could not be verified from inside Rust.** The unit test feeds the
function the same literal the implementation contains — a closed loop, and the
same shape as the write-accounting scan that was satisfied by a declaration rather
than a call site. Migration 216 declares its CHECKs inline, so *Postgres* chooses
the names; they were four literals guessing at a convention. Verified against a
real server, and break 3 shows the live tier is the only thing that catches a
wrong one.

---

## Where the loops are

```
loop1   episodes 3576 → consolidated 213 → rules 258 → retrieved 39   turning
loop2   anomaly 0 ← stalled: unobserved
loop3   intentions 0 ← Stage 0 still unprompted; Stage 3 now closes structurally
loop4   claims 0 ← awaiting a run on a saved forecast
loop5a  committed 1354 → resolved 2180 → scored 239   turning, signal Uniform
loop5b  projected 61 → anchored 0 ← closes on the next projection after the hook
```

---

## The declaration ladder — why every surface says `unknown`

**Built after the above.** `src/declaration_ladder.rs`, `GET /api/declarations`,
`scripts/break_declaration_ladder.py` (12 breaks, all land).

Full argument in `docs/ROADMAP_artifact_trace_alignment.md` §0. The measurement,
over the 206 agents that have produced an episode:

| | agents | ports | output type | checkable schema | field contract |
|---|---|---|---|---|---|
| real | **96** | 93 | 8 | **2** | **7** |
| `test_agent_*` | **110** | 0 | 0 | 0 | 0 |

So `unknown` across every surface built so far is overwhelmingly **the subject
declaring no structure to check against** — not a stalled loop, not a cold
counter, and not a contract the platform failed to write. 3,571 of 3,576 episodes
carry no grounding stamp because 89 of 96 real agents have no field contract.

`panel_absence::Resolver` had five ways to explain an absence and none of them was
that, so it collapsed into `Unresolved { why }` — which reads as *the platform has
not written a contract for this*:

> `Unresolved` is a work item for **us**. `Undeclared` is a work item for the
> **agent's author**.

That made 89 agents' missing declarations look like 89 contracts the platform
owed. It owes none of them. The failure mode is not a wrong number, it is a wrong
**backlog**, and a backlog nobody can act on is one nobody does.

Four decisions worth not undoing:

1. **`attribute`'s ordering is the whole function.** Cold counter first (on a
   fresh boot it explains everything spuriously), then undeclared, then
   nothing-traversed, then `Unresolved`. Break 2 in the harness is this.
2. **`disposition` checks cruft before legibility.** A fixture that declared every
   rung must still be `Prune`, or the coverage numerator fills with rows about to
   be deleted. The fleet has no such row today, which is why it is asserted rather
   than observed.
3. **The two worklists never add up.** `retrofit` is 96, `prune_count` is 110.
   Pruning is a delete behind an existing safety gate; retrofitting is authoring
   work with a domain expert. One number makes the retrofit look twice its size.
4. **Coverage is reported, never ratcheted.** New agents arrive undeclared by
   definition, so a ratchet would fire on correct behaviour. The one safe ratchet
   is `FIELD_CONTRACTS`' agent count — a hand-maintained const where a shrink is
   always a regression. Pinned at 9, both ways.

`fermi_contract` is deliberately **not** a rung, and it is the case that tested
the design: 15 of 96 agents carry one, so it would have more than doubled a rung's
coverage, and it holds forecast-domain config no trust surface can read. Every rung
has to say what it `unlocks`, with a length floor, so the ladder cannot become a
checklist that inflates with things no consumer can use.

**The retrofit/prune effort is a dependent track.** It needs this to be actionable
and not the other way round; `/api/declarations` is what gives it a worklist. Two
things kept out of scope on purpose: nothing here deletes (pruning stays behind
`/api/admin/agents/cleanup-test-cruft`), and nothing here *ranks* the retrofit —
which of 89 agents to bring under a field contract first is a product judgement
about which outputs anyone relies on, and a coverage number is the wrong
instrument.

**The `panel_absence` integration is done, and the plan for it was wrong.** I
proposed a `Resolver::Undeclared` variant applied across the unresolved panels.
That is a category error: a `Resolver` answers *which contract explains THIS
PANEL platform-wide*, while `Undeclared` is a fact about a **subject**. Reading
the five unresolved panels, **four genuinely are the platform's work** — nothing
watches dyad formation, `eval_runs` has no liveness contract — and relabelling
them as the agents' fault is the original mistake in reverse.

What shipped is `Resolver::Declaration { rung }`, used by exactly one panel, and
that panel asked for it: `ecology.seams`'s own `why` read *"a census in a comment
is not a contract. Resolve by promoting the census to a rung."* Unresolved ratchet
5 → 4. Three states, and the middle one is the point: `no_census` (`unknown` — a
failed measurement is **not** zero coverage), `undeclared` (`unknown`, authors'
work, with a remediation), `declared` (`idle` — the input exists and what it adds
up to is the finding). Live, `ecology.seams` went from `unknown/unresolved/none`
to `idle/declared/declaration_ladder`.

It also caught a stale number — the panel cited `513 labels, 14 both-sides, 499
orphans`; re-measured it is **289 / 236 / 13** — and a §3.4 duplication: adding
`Observation::declarations` made the compiler enumerate five hand-built
`Observation` literals, two of which (`rounds.rs`, `specimen.rs`) were
byte-for-byte `Observation::collect`. They now call it. Without the new field
they would have kept drifting, and every declaration-resolved panel on those
endpoints would have reported `no_census` while the endpoint looked fine.

`scripts/break_declaration_resolver.py` — 5 breaks, all land.

## Parallel session: typed output contracts — check before touching

A parallel session has landed `src/contract_sketch.rs`,
`docs/DESIGN_typed_output_contracts.md`, `tests/contract_sketch_corpus.rs`,
`tests/equity_analyst_contract.rs`, and changes to `src/agent_backend/envelope.rs`
and `src/workflows/agent_contract.rs`. **Do not `cargo fmt -p fermi` blindly** —
`tests/contract_sketch_corpus.rs` is currently unformatted and it is theirs. Use
`rustfmt` on your own files.

It converges with the ladder rather than colliding: their generator is the remedy
for `output_schema`, the worst-covered rung (2 of 96), and their diagnosis is why
— *the contract was never disputed, only unaffordable.* The ladder's
`output_schema` entry now names `contract_sketch` as its owner.

Their `TYPED_TIER_EXEMPT` (86 → 85) and the `output_schema` rung are **not
duplicates**: theirs ratchets curated agents at publish, the rung measures
producing agents at trace. Recorded in `declaration_ladder`'s module docs so a
third list has to argue against it.

Also moved: `is_test_cruft` now lives in the library (`declaration_ladder`) and
`handlers::mod` re-exports it, so the five binary call sites are unchanged and
there is still exactly one definition. It became load-bearing — it is the pivot
deciding which worklist an agent lands on.

---

## Document recovery — the bare parse that graded 0 of 94

**Found while scoping the grounding writer, and it was blocking it entirely.**
`src/grounding_trust.rs::response_floor`, `tests/response_floor_recovery.rs`,
`scripts/break_response_floor.py` (2 breaks, both land).

The writer was going to populate `assertions[].basis` from `response_floor`.
Grouping the population first — as the method requires — showed **it would have
been a no-op.**

`response_floor` used a bare `serde_json::from_str` and returned
`unavailable_no_tool_source` the moment it failed, explaining itself with *"Prose.
An extraction from prose is ungrounded by construction."* That is a true statement
about a `from_str` parse presented as a fact about the agent's output — §19 again.

Agents wrap their document in prose, and `envelope::extract_json` has always known
that (balanced-brace scan for the largest object), which is why
`handlers::execution` grades responses this function called ungradeable. **Two
implementations of "get the document out of the response", with the weaker one
behind the trust calculation.**

Measured across the seven contracted agents that have run and retained a response:

| | |
|---|---|
| retained responses | **94** |
| bare JSON | **0** |
| embedded in prose | **64** |
| no document at all | 30 |

The old parse graded **0 of 94**. And **28 of 28** semantic rules carrying a
provenance floor read `unavailable_no_tool_source`, because `provenance_oracle`
computes a rule's extraction floor by re-running this function.

Floors after the fix: **44 at `model_inference` (strength 1)**, 20 at
`tool_no_match` (strength 0), 30 at `unavailable` (strength 0).

**Read the strength column, not the token** — a correction I had to make to my own
probe, which first reported "64 graded above unavailable". `tool_no_match` sorts
above `unavailable_no_tool_source` and both are strength **0**. The real change is
44 responses moving from strength 0 to 1, plus 20 getting a more accurate strength­-0
diagnosis.

Two further facts worth carrying:

* **0 of 94 reach strength 2.** No contracted agent's response is reproducible, so
  the verification queue will route essentially everything to `pending_*` — which
  is exactly Loop 2's missing content.
* **All 94 existing assertions are 75 `Multiplier` + 19 `Probability`, zero
  `Quantity`.** `route()` sends a non-verifiable kind to `InheritFromBasis`, so
  none of them yields a queue item *by design* — you cannot verify a multiplier.
  The queue's first real content comes from contracted **fields**, not from
  prose-extracted numbers.

**Stored rule floors are not backfilled.** The 28 carry the old verdict; backfill
is off the table here and `provenance_oracle` says so. Watch whether rules written
*after* this change still land at `unavailable`.

## The queue's content comes from contracted fields, not prose numbers

**Built:** `grounding_trust::GradedField` + `graded_fields`,
`assertions::from_graded_field` + `from_graded_fields`,
`scripts/break_graded_fields.py` (7 breaks, all land).

The writer was going to mint queue items from `episodes.assertions[]`. It cannot:
**all 94 live assertions are 75 `Multiplier` + 19 `Probability`, zero
`Quantity`**, and `Assertion::route` correctly sends a non-verifiable kind to
`InheritFromBasis` — *you cannot verify a multiplier.* So the prose extractor
cannot produce a queue item however well it works.

The content has to come from contracted **fields**, which purport to be
retrievals and are therefore checkable. Two halves, split along the §3.4 line:

* `grounding_trust::graded_fields(agent, doc, report)` → `Vec<GradedField>` — the
  path, the block, **the value the model claimed verbatim**, the block's grade,
  and `settleable_by` derived from `Grounding::Sourced { tool }`. All of it was
  already computed by `enforce` and discarded: `Report` carries `path`, `removed`
  and `kind`, and `stamp_grounding` reduced the lot to `grounding:violations` +
  `grounding:count-N`. This is the artifact trace's `fields[]`.
* `assertions::from_graded_field` → an `Assertion` the queue can key on.

Three things in there worth not undoing:

1. **`ExtractionPath::TypedField` is constructed for the first time.** It has
   existed since `assertions.rs` was written, is documented as *"the only path
   that can reach the top of the ladder"*, and nothing had ever built one — every
   assertion in production is `Prose`, capped at `model_inference`. That cap is
   what gives the retrofit a gradient; this is the path that rewards it.
2. **The basis is the whole point.** A `Quantity` with an empty basis floors at
   `pending_human_check` however well sourced its block was — correct behaviour,
   because a measurement with no stated source is work to be done. So minting
   from a tool-verified field *without* carrying the grade enqueues a person to
   re-check what a tool already answered. `carrying_the_blocks_grade_is_the
   _difference_between_verified_and_pending` measures the counterfactual rather
   than asserting it.
3. **A declared coverage gap, returned as a value.** `Assertion::value` is a
   `Spread`, so only numeric fields can be enqueued. `taxonomy.order =
   "Coleoptera"` cannot — and that is the canonical `Antaxius beieri` case, the
   claim most worth verifying. Those come back as `NotEnqueued { path, why }` and
   the caller counts them, because an empty queue that is empty because nothing
   could be enqueued reads identically to one that is empty because nothing is
   wrong. Widening `value` changes a stored JSONB shape 94 rows and 124 empty
   arrays already use, so it is a follow-up and not a quiet edit.

## The writer `assertion_verifications` never had — wired

**Built:** `src/verification_queue.rs`, `write_accounting::Sink
::AssertionVerifications`, the enqueue wired into `handlers::execution.rs`,
`tests/verification_queue_contract.rs` (3 live probes, green against production),
`scripts/break_verification_queue.py` (4 offline breaks, all land).

The table has held **0 rows since migration 205**. The audit's conclusion was
exactly right — *it needs a writer, not a schema* — and this is it.

Four decisions worth not undoing:

1. **`actor_kind = platform`, `actor = "grounding_contract"`, `verdict =
   pending_*`.** Not `tool`/`human`: at enqueue time **nobody has acted**, and
   recording the intended actor as though it had would make *queued for a person*
   and *checked by a person* the same row, so the queue could never be filtered to
   what still needs doing. The routing lives in the verdict; the actor records who
   actually decided, which is the platform.
2. **`ENQUEUE_SQL` omits `source_citation`.** Migration 205's CHECK requires one
   only for `human_sourced`, and a pending row has nothing to cite. Writing an
   empty string to satisfy a constraint that does not apply is how that citation
   requirement — the thing stopping a one-click *verified* button being a
   laundering UI — becomes decoration. A live test asserts the CHECK is still
   armed for `human_sourced`.
3. **The pre-enforcement document is kept.** `enforce` mutates: it nulls
   ungrounded fields. So the claimed values exist only in a copy taken before it,
   and reading them off the enforced document would find the nulls the platform
   just wrote and record the agent as having claimed nothing.
4. **`Enqueued` carries every count, including the ones that are not failures.**
   The question this table has never been able to answer is *why is it empty*, and
   "nothing was checkable", "everything was already reproducible" and "the writes
   were refused" are three different answers with three different remedies.
   `is_problem()` is false for the first two — a caller that warned on
   `already_settled` would fill the log on exactly the runs that went best.

The enqueue sits **below** the episode write, and the ordering is enforced by the
binding rather than by a comment: `assertion_verifications.episode_id` is a real
foreign key and `stored_episode_id` does not exist before that line, so moving it
up is a compile error. Same shape as `spawn_raise`, which is directly above it and
handles the other half — `anomaly_events` for the **exception**, this for the
**routine**. Keeping them apart is deliberate: a row per marked field in
`anomaly_events` would flood the HITL queue and destroy the semantics that keep
Loop 2 informative.

**Two bugs in my own live probe, worth recording because both were the probe
blaming the thing it was written to check.** The seed omitted `timestamp_ref` and
`context` (NOT NULL, no default), so every insert failed and all three tests
reported the database refusing the *verification*. And the constraint lookup
matched on definition text — `LIKE '%verdict%' LIMIT 1` returns the **citation**
check, because it reads `verdict <> 'human_sourced'` — so it reported the column
as having no vocabulary while `assertion_verifications_verdict_check` was sitting
there. Pin the constraint **name**, which is what `seam_vocabulary` declares.

## Step 2 — the artifact trace, and the stream's parity

**Built:** `src/artifact_trace.rs`, `GET /api/episodes/:episode_id/trace`,
`tests/artifact_trace_contract.rs` (2 live), the enqueue mirrored into
`execution_stream.rs`, and a new scan holding the pair together.

### The trace

The instance-level counterpart to `surface.rs`. It **holds no verdict of its
own** — the belt comes from `command_registry`, the clocks and refusal text from
`gate_trust::GATES`, the grades from `graded_fields`, the floor from
`grounding_trust::floor`, the routing from `Assertion::route`, and **the reason an
empty trace is empty from `declaration_ladder::attribute`**. The one thing it owns
is `reading`, which is a composition, and it is registered as one.

It re-runs the contract over `episodes.response_text`, retained since migration
199. That retention is the whole reason a historical episode can be traced — *a
digest is not a record*, and this is the payoff.

**Measured over the 256 most recent episodes with a retained response:**

```
nothing_checked  192      owner: agent_author  172
checked_clean     54      owner: no_one         54
violations        10      owner: platform       30
```

**And there are 10 real, correctable anomalies** — `prey_locator` (9),
`enemy_sensor` (1) — each with a named agent, a named field and the claimed value
retained. **None was recorded when it happened**: `episodes.tags` carries
`grounding:violations` on 0 rows, because the contract was not wired to those paths
when those episodes ran. This is the *sourced anomaly you can correct* that the
whole line of work was asked for.

### The stream, and one scan that should have existed earlier

`execution_stream.rs` now enqueues too. Doing it collapsed a **double
`enforce`**: the grounding block scoped its report and the schema check below
re-extracted and re-enforced, with a comment admitting the compromise. One pass
now serves all three readers.

The file's own comment said *"Keep them edited in pairs."* Comments do not fail
builds, and that instruction had already been ignored twice on this exact pair —
grounding wired to `execution.rs` and not the stream, claims retained on
`execution.rs` since mig-187 and never on the stream, which was the whole of the
remaining loss after migration 213 because the console prefers the stream. So
`grounding_execute_coverage` now has
`both_execute_paths_queue_contracted_claims_for_verification`, with its own
falsifier, and the break confirms it names the offending path.

### One exemption added, deliberately narrow

`grounding_raise_coverage` correctly flagged the trace handler: it calls `enforce`
and does not raise. Raising there would be **wrong** — it is a GET, so
`anomaly_events` would become a function of UI traffic, one row per page load,
attributed to an episode that ran weeks ago, with Loop 2's count determined by how
often someone opens a screen. `NO_RAISE` carries that reason. Whether a
*historical* violation should be backfilled into `anomaly_events` once is a real
and separate question; it needs a de-duplication key the table does not have.

## The hashes, and the `parent_episode_id` non-gap

**Built:** `src/artifact_hash.rs`, wired into the trace;
`tests/episode_lineage_coverage.rs`.

### Hashes are computed on read, not stored — deliberately

The obvious implementation was three columns plus a migration. `episodes.query`
and `response_text` are both retained, so a digest of them is a **pure function of
data the platform already holds** — and a computed digest cannot drift from its
subject, which a stored one can (`agents.total_executions` is why `rollup_trust`
exists). A migration would be storage in advance of a use, and the use is further
away than it looks: **the seam check does not work**, because a delegated child
receives a prompt built around its task rather than its parent's output verbatim.
The place equality would hold is the envelope payload, and nothing hashes that.

When a cross-episode hash *query* is genuinely wanted, columns are the right
answer and this module is what fills them.

### The field I named wrong, caught by a live cross-check

I shipped `grounding_changed_the_document`, then wrote a live assertion comparing
it against the contract's own violation count. It disagreed on **21 episodes** —
`weather_oracle` and `enemy_sensor` responses where the bytes changed and the
violation count was **zero**.

`enforce` does two things: it nulls a refused field (a finding) **and it stamps
`<block>_provenance` siblings onto the document** (bookkeeping, on every contracted
response). `Report.provenance` says so in its own doc — *"pairs written onto the
document"* — and a digest comparison cannot tell them apart.

So the field is now `enforcement_changed_the_bytes`, with the incident in its doc
comment, and the live suite asserts only the direction that is true (violations ⇒
bytes changed). **Asserting the reverse would fire on entirely correct behaviour.**
The fix was to the field, not the test.

### `parent_episode_id`: there was no missing writer

The roadmap said *"the column exists and every call site passes `None`"*. That was
wrong. `tools_legacy.rs:6188` writes it, both execute paths populate the context,
and the chain is thin (4 of 3,576) because **delegation is rare** and because four
of the ten `ToolContext` sites legitimately have no episode to point at.

What *was* missing is enforcement of a discipline the code already followed by
hand: of those four, **three carried a reason and one did not**
(`workspace::coherence`). `tests/episode_lineage_coverage.rs` now requires one, and
the break confirms it names the exact pre-fix state.

**Its own first run made it better.** Matching the field anywhere caught five
`Episode { parent_episode_id: None }` sites — which mean *this row has no parent*,
true of 3,572 of 3,576 rows and the ordinary case. Demanding a paragraph for those
would be a check that fires on correct code. So the scan is narrowed to
`ToolContext`, and it **fails closed**: an unrecognisable enclosure is skipped
rather than demanded of. *A scan must be no broader than the property it asserts —
the exemption rule, pointed the other way.*

### One test that is weaker than it looks, and says so

`the_document_hash_ignores_key_order` **cannot fail because of a mistake in this
code.** Order-independence is owned by `serde_json::Map` being a `BTreeMap`: both
fixtures parse to the same map before `of_document` sees them, so no edit here can
make the digest order-sensitive — an attempted sabotage came back green proving
exactly that. It is a **dependency guard** against someone enabling
`preserve_order`, and the doc comment now says so rather than implying the logic is
verified. The falsifiable half is `a_changed_value_changes_the_digest`.

## `Claim` — the Antaxius gap, closed without a migration

**Built:** `assertions::Claim`, `AssertionKind::Fact`,
`Assertion::shape_is_consistent`.

`Assertion::value` was a `Spread`, so a non-numeric claim could not be recorded at
all — which excluded `taxonomy.order = "Coleoptera"`, the `Antaxius beieri` case,
the claim most worth verifying and the one the queue existed for.
`grounding_trust` even has a `ViolationKind::ContradictsCanonical` for it and the
queue had nowhere to put the result.

### `untagged`, and the cost that had to be paid explicitly

`episodes.assertions` holds 94 rows written as bare `{"p5":…,"p50":…,"p95":…}`
objects. An externally tagged enum changes those bytes and every stored assertion
stops deserialising, so `Claim` is `#[serde(untagged)]` and **the stored
representation is unchanged — no migration**.

`untagged` **falls through silently**: `{"p5":1,"p50":2}` with no `p95` does not
fail to parse, it becomes a `Claim::Literal` carrying an object, and serde says
nothing. That is the same defect class as everything else here — a true fact about
a parse becoming a false fact about the world. `AssertionKind` is the independent
witness (set by which pattern matched, not by the value), so
`shape_is_consistent` compares the two and names *which way* they disagree,
because a numeric kind holding a literal is a broken spread while a `Fact` holding
a spread is a mislabelled extraction. Different remedies.

Two tests carry the guarantee: `a_malformed_spread_is_caught_rather_than_becoming
_a_literal` and `the_shape_already_in_the_database_still_reads`, the latter
asserting the serialised bytes are byte-identical to what is stored so no row is
rewritten into a shape the previous release cannot read.

### One test correctly died

`a_non_numeric_claim_is_refused_with_its_reason_rather_than_dropped` pinned the
gap. Closing the gap made it fail, which is the right failure. Replaced with
`a_claim_the_queue_still_cannot_carry_is_refused_with_its_reason`, covering what
`NotEnqueued` still holds — and noting that a non-finite float arrives as `null`
because `serde_json` cannot represent one, so the two refusal reasons collapse
into one there.

Five breaks, all land, including the one that matters: disable
`shape_is_consistent`'s literal arm and the malformed-spread test goes red.

## ⚠ A migration is unregistered, and it is not mine

`migrations/218_intention_provenance.sql` (Loop 3 Stage 0 — provenance on
`workspace_intentions`) appeared untracked and is **not in `run_migrations`**, so
`test_all_migrations_registered` is red. That test exists for exactly this: it is
what caught migration 212, which was written, committed, and never registered,
while `composition_evolution.rs` bound to a column production did not have.

Left for its author. It is the only failing test in the tree.

## `gate_decisions.episode_id`, and why the column alone was useless

**Built:** migrations 219/220/221, `gate_trust::decided_for_episode`,
`grounding` promoted to `Retention::Recorded`, a grounding review door,
`tests/gate_decision_lineage.rs`.

### The measurement that changed the work

The ask was "one column". Measured first: **every per-episode gate was `Counted`,
and both `Recorded` gates are not per-episode.** `coherence` fires on an AgentWide
correction, `admission` at publish. So the column would have been NULL on every
row that would ever exist — while making the trace's `not_recorded` look solved.
The blocker was retention.

That is the fourth time this session that grouping the population first stopped
the wrong thing being built.

### Migration 221 contains no DDL

It is the argument. Promoting `grounding` needed no schema change — 214 already
registered the whole of `GATE_IDS` — so the change is one line in a Rust const,
and *a decision made in a constant is one nobody can find*. The file records: why
the answer was no before (the per-field detail had no home; it now lives in
`assertion_verifications`), what a ledger row adds over re-running the contract
(what the gate decided **at the time** — and 10 violations exist that were never
recorded), the measured volume (**~30 episodes/day**; 214's rate-limit argument
does not transfer, because a tick fires per *request* including the floods it
rejects while this fires per completed *execute*), and how to reverse it.

### No foreign key, and the reason is the batch

`assertion_verifications.episode_id` **is** a real FK; this deliberately is not.
`spawn_gate_recorder` drains its queue with a single
`INSERT ... SELECT FROM UNNEST(...)`, so **one bad reference rejects the whole
batch** — an unrelated failed episode write would take every gate decision in the
flush with it. Decisions are also enqueued before their episode row exists.
`tests/gate_decision_lineage.rs` checks the reference instead, which is
`assertion_verifications.assertion_id`'s precedent for a different reason.

Verified against a real Postgres with the exact batched-UNNEST statement the
recorder issues.

### The seam drift my own comment predicted

`seam_vocabulary_contract` went red: `GATE_IDS` gained `output_schema`, migration
214's CHECK was widened, **216's was not**. My registry entry for that column had
already written down what it would cost — *"widening `GATES` and 214's constraint
while leaving 216's alone makes the new gate's decisions recordable and its
reviews unwritable"* — and that is exactly what happened. Migration 219 fixes it.
Worth noting the contract found it **with no traffic, no promotion and no
reviewer**, by comparing two declarations nothing else compares.

### A scan that drifted for the second time, now derived

`gate_trust_coverage` listed the reporting entry points — `decided(`,
`decided_ok(`, `decided_about(` — and its comment recorded that `decided_about`
had been omitted until its first caller appeared. Adding `decided_for_episode`
did it again, and reported `grounding` as *recording nothing* on the very change
that promoted it. **A list of entry points fails in the most misleading direction:
a gate that reports more looks like a gate that reports nothing.** Now derived
from the shape `decided*(`, with a falsifier covering an entry point that does not
exist yet.

## ⚠ Cargo's fingerprint cache is unreliable in this tree

**Read this before believing any test result.** The parallel session writes files
with inconsistent mtimes — some dependents are stamped *earlier* than the files
they depend on — and cargo's fingerprinting is mtime-based. The observed
symptoms, both of which cost real time this session:

* **a stale test binary.** `cargo test -p fermi --lib` reported `808 tests, 3
  failed` for three consecutive runs. The three failures were source-reading tests
  claiming `src/api_server.rs` and `src/schema_trust.rs` *did not exist*, and
  `verification_queue`'s tests were absent from the binary entirely. `touch
  src/lib.rs` → **846 tests, 0 failed.** Nothing had been wrong.
* **phantom unresolved imports.** `fermi::declaration_ladder`,
  `fermi::gate_review`, `gate_api::LedgerClaim` and six others reported as not
  found, with every file present and every `pub mod` in place.

So: before trusting a red — and especially before trusting a green —
`find src tests -name '*.rs' -newermt '-6 hours' -exec touch {} +`. Every break
harness in `scripts/` now calls `os.utime` on the file it edited for this reason,
so a green break result cannot be a cached one.

One finding for whoever does it: **`assertions` is not in `TRUST_MODULES`** and
should be. It holds `entitled_provenance`, `route` and `pending_verdict` — three
trust decisions the coverage scan does not currently demand be registered. Adding
it means registering the module's whole public surface, which is its own task. The
two new functions are registered voluntarily so they are not left uncovered.

## Reds that belong to the parallel session — check the clock before believing any of this

The typed-output-contracts session is **actively working** and broke the build
under this one four times. Every item below was verified as theirs: file modified
minutes before, zero references to any of this work. **Do not assume the tree is
the one you last tested**, and prefer `cargo test -p fermi --lib` when you only
need to know whether *your* change is sound — the integration suites build the
`api-server` bin, so their half-wired handler will fail your run for reasons
unrelated to it.

**Open at the time of writing:**

* `gate_trust_coverage::every_declared_gate_is_recorded_somewhere` — they added a
  `Gate::OutputSchema` variant and have not wired its `decided(...)` call site
  yet. Their own check catching their own in-flight work, which is the machinery
  behaving.

**Resolved while this session ran, recorded because the shapes recur:**

* `agent_backend::envelope::gate_tests` was **order-dependent** — the new
  `gate_tests` module reads `gate_trust`'s **process-global** counters, so its
  tests interfered with each other and with `envelope::tests`. In isolation each
  passed; run together one failed, and *which* one changed with ordering
  (`a_valid_document_approves_the_gate`, `left: 2, right: 1`). If it returns, the
  fix is a delta against a snapshot taken inside the test rather than an absolute
  count.
* `migrations/217_gate_decisions_output_schema.sql` briefly carried a non-atomic
  DROP+ADD and tripped the constraint ratchet at 15 against a grandfathered 14.
  Worth knowing it is a real defect and not a lint preference: through PgBouncer
  the DROP commits and the ADD does not, so the net effect is to **delete** the
  constraint, and `run_migrations` logs the failure and continues. Now clean.
* Two straight compile breaks (`handlers/contracts.rs` missing a
  `use serde_json::json;` and calling a `builtin_tool_catalogue` that did not yet
  exist; `envelope.rs` passing `&str` to their new `decided_about`'s
  `Option<&str>`). Both self-resolved.

Also from them, and welcome: `video_analyst` became the **10th** agent with a
field contract. `declaration_ladder`'s two-way ratchet went red on the next full
run and the floor is raised to 10 — which is the ratchet earning its keep on work
that was not its author's.

## A pre-existing flake, not mine

`sensitivity::tests::first_order_indices_do_not_over_explain_the_variance` failed
once in a full run (`first-order indices sum to 1.0542681668051066`) and passes
5/5 in isolation. It is an unseeded Monte Carlo test in `src/sensitivity.rs`, a
file this session did not touch. Owner's call whether to seed it; flagged so the
next red is not mistaken for a regression.

## Next, in order

0. **The grounding writer — highest-leverage item, and now actually unblocked.**
   `docs/ROADMAP_artifact_trace_alignment.md` §5 step 1. `grounding_trust::Report`
   already carries `path`, `removed` (the fabricated claim) and `kind` (the
   diagnosis) on every execute path, and `stamp_grounding` reduces the whole thing
   to `grounding:violations` + `grounding:count-N` tags — so the evidence is
   computed and dropped. Write it into `episodes.assertions[]` and
   `assertion_verifications` instead. **No migration.** That single writer delivers
   the UX team's endpoint ② entirely, makes ③ derivable, fills `assertions[].basis`
   (the contract's `Grounding` variant *is* the basis), and gives Loop 2 its
   content — which is the original ask: *a sourced anomaly I can correct.*

1. **Loop 3 Stage 0, structurally — same argument as Stage 3.** All 3,576 episodes are `auto_pass` and `coordinator_observation` was 0
   for the same reason: `workspace/coherence.rs` asks the strategist for stages 2
   and 3 and never Stage 0, so six implemented intention tools have no caller.
   The prompt fix is small and will not hold, for the reason above.

   The structural fix: the platform already knows *"agent X is about to do Y in
   workspace W"* at dispatch — `workspace/messages.rs`, the @-mention path.
   Declare the intention there through the existing `declare_intention`
   implementation. Conflict-check for **observability only; do not enforce** —
   that is a separate decision with safety weight and should be made on its own.

2. **Loop 2 contract coverage — and it now has a worklist.** The only honest lever
   for a real anomaly: `FIELD_CONTRACTS` holds 98 contracts across 9 agents, of
   which **7 have ever produced an episode**, against **96 real agents that have**.
   You cannot code your way to an anomaly. `GET /api/declarations` emits the
   candidates with the cheapest missing rung per agent; picking which to do first
   is a product judgement, not a coverage number.

3. **`gate_decisions` holds 0 rows** despite 214 being deployed and
   `spawn_gate_recorder` being wired, so `since: "ledger"` is an unbacked claim
   and every gate will read `nothing_to_review` regardless of the new door. Worth
   an hour: check the recorder is draining and `ledger_status().dropped`.
   Until this is fixed the review feature has nothing to point at.

4. **`native_evaluators::loop_stalled_in_code`** still over-claims (*"the rest are
   idle rather than broken"* about loops classified `unknown`). Caveated in
   `evaluator_api::EVALUATOR_CAVEATS`, not fixed, because narrowing it flips a
   live verdict platform-wide. Owner's call.

5. **The paired outcome measurement** — the one item left needing new machinery.
   Blocked on a control arm (suppress rule injection for a turn) and on Loop 4's
   claims, so there is agent-level outcome variance to move.

---

## Do not

* **Do not put a Rust guard in front of the rationale CHECK.** See above. The same
  applies to any new CHECK: translate, do not re-implement.
* **Do not require a rationale for `upheld`** for symmetry. It costs the
  denominator, and "3 overturned" without "of 400 reviewed" is not actionable.
* **Do not add a review door to a `Retention::Counted` gate.** Its decisions never
  leave the process; the queue would be permanently empty for a reason no message
  could explain. `gate_api::a_review_door_only_exists_where_the_decisions_do`
  enforces both directions.
* **Do not write a source scan over a file that contains the scan** without
  cutting the test module out. Two in `coherence.rs` had never been able to fail.
* **Do not repoint `agent_loops_handler`** further; it is already unrouted, and a
  test asserts it.
* **Do not delete `hud_contract`** on the strength of finding 6 without asking.
* **Do not touch `panel_absence.rs`, `crates/posterior`, or
  `handlers/workspace/{refit,resolution}.rs`** without checking for a parallel
  session.

---

## Still open, from §9

3. Grounding is a metric, not a control, on the two general execute paths.
   `enforce` mutates a local dropped two lines later. It *is* a control on the
   creature handlers. The paper's §4 claims the opposite of what the primary
   endpoints do.
4. `hud_contract::enforce` has no production caller.
5. `delegate_to_agent` has no grounding gate at all.
