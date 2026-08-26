# UX request — the artifact trace

**For:** the team that owns loops, gates, episodes and the composition model.
**From:** the trust surfaces UI, built against `UX_HANDOFF_trust_surfaces.md`.
**Status:** request. Four endpoints, of which one needs no new tables and one
needs a table that does not exist. Every claim about the current schema below
was checked against the live database on 2026-08-24 and is marked.

**Convention**, matching the handoff:

* **[E]** — exists today, verified in this pass.
* **[W]** — the table exists and has never been written to.
* **[B]** — must be built.

---

## The one idea

The handoff gave us the machinery, and we built it faithfully: six loops, five
tally buckets, stage chains, gate tokens, readings. It is correct and it is hard
to read, and the reason is not styling.

**Its primary object is the population.** A row that says
`3576 · experience accumulates · REQUEST` is a census. It is only legible to
someone who already holds the machine in their head, which is the team that
built it.

`docs/guides/compositions_v16.html` makes the opposite choice, and it is the
right one. **Its primary object is the artifact travelling** — one token crossing
one belt, passing checkpoints where rungs fire, getting stamped or refused or
routed to a person. That is legible without prior knowledge, because one thing
taking one journey is concrete.

The two views are the same structure from opposite ends:

> **A loop is a path an artifact takes. A gate is a checkpoint on that path.**

We already had the path — `stages` is literally a chain — and rendered it as a
table of counts. The fix is not a nicer table. It is to make **the episode** the
primary object and let the loops be the routes it can take, with the census
demoted to a secondary lens: *how many took each path*.

There is a second reason to want this, and for the platform it is the more
important one. `compositions_v16` is a **simulation**, because nothing in the
platform can currently be asked *"what happened to this artifact, and where could
it still have gone wrong?"* The episode contract in
`docs/architecture/learning-loops-and-contractual-gates.md` is precisely the data
model that turns that simulation into a live view. **Endpoint ① is that
contract, exposed.**

---

## What already exists — please do not rebuild these

Rather more than we expected, and one of them is the whole review queue.

| concept | where it already lives | state |
|---|---|---|
| provenance ladder `sourced → inference → unknown → bad` | `grounding_trust::PROVENANCE_VALUES` | **[E]** registered at the seam, both directions |
| weakest-link floor (`floorOf`) | `semantic_rules.provenance_floor` | **[E]** implemented, live values |
| per-claim records on an output | `episodes.assertions[]` — `assertion_id`, `kind`, `value`, `raw`, `extraction.path` | **[E]** 152 episodes carry them |
| **the review queue**, incl. tool-vs-human routing | **`assertion_verifications`** — `assertion_id`, `episode_id`, `verdict`, `source_citation`, `actor`, `actor_kind ∈ {tool, human, platform}` | **[W]** 0 rows |
| correction chain | `episodes.parent_episode_id` · `episode_corrections` | **[W]** 4 of 3,576 · 0 rows |
| the three clocks | `gate_trust::Clock` = `Admission`, `Invocation`, `Standing` | **[E]** |
| durable gate decisions | `gate_decisions` (migration 214) | **[E]** |

Two notes on that table.

`assertion_verifications` is the content review queue the
`Grounded_Multi_Agent_Worlds` paper says *"falls out of the architecture instead
of being built beside it."* It is already the right shape, already keyed to both
the assertion and the episode, and its `actor_kind` maps 1:1 onto
`compositions_v16`'s `pending_tool` / `pending_human`. **It needs a writer, not a
schema.** Adding a second queue table would be the *one dependency, two
resolutions* defect from `FEEDBACK_LOOPS.md` §8.

`gate_trust::Clock::Standing` is what `compositions_v16` calls **sweep**. Same
concept, two names; worth settling on one before either surface ships the other.

---

## The two real gaps

**1. There are no belts.** `workspace_agents.relationship` is membership kind —
`system` 2,513, `owned` 26, `hired` 11. **[E]** Nothing anywhere records *"this
member's `produces:X` feeds that member's `accepts:X`."* A composition is a bag
of members with no declared edges, which is why `compositions_v16` can only be a
simulation: its entire visual is belts between ports, and the platform has no
edge to draw.

Constraining the ambition, and worth knowing before the table is designed: of
512 distinct declared port labels across published agents, **13 appear on both an
`accepts` and a `produces`.** 499 can form no seam with anything. **[E]** So most
belts could not exist even once the table does, and the first useful thing the
table buys is measuring that convergence rather than drawing many edges.

**2. `assertions[].basis` is empty on every assertion.** **[E]** The floor rule
needs to know what a claim rests on. With `basis: []` the floor is uncomputable
per claim, even though the machinery exists for rules. Populating `basis` is
what makes `floorOf` real for output as well as for memory.

---

## A correction to advice we gave earlier

We previously suggested a corpus-eligibility predicate including
`provenance <> 'auto_pass'`. **That was wrong.** `episodes.provenance` is
`auto_pass` on **all 3,576 rows** — a single-valued column carrying no
information. **[E]** The predicate would have excluded everything.

Two consequences worth having on the record:

* `loop_model`'s `loop3.brief` stage counts
  `episodes WHERE provenance = 'coordinator_observation'`, which is 0 — correctly,
  and for the stronger reason that **no episode has ever carried any provenance
  value other than `auto_pass`.** The stage's zero is honest; it is just a
  narrower fact than "coordination notes have not been sent".
* Corpus eligibility has to key on something else. Measured today:

  | | |
  |---|---|
  | episodes | 3,576 |
  | on `test_agent_*` fixtures | 514 |
  | `model_used` present **and** not a fixture | **2,317** |

  So the corpus that can answer *"which model behaved which way"* is 2,317 rows,
  not 3,576. Please serve that number rather than letting each surface derive its
  own — and note the precedent: migration 185 is
  `hide_test_cruft_from_rosters`, the same contamination on a different surface.

---

## ① `GET /api/episodes/:episode_id/trace`

**The one we want most, and it needs no new tables.**

```
{ "episode_id": "…",
  "parent_episode_id": "…" | null,
  "agent":  { "id": "…", "name": "…" },
  "model":  { "provider_used": "…", "model_used": "…",
              "persona_version_at_write": 3 },
  "corpus_eligible": true,

  "input":  { "ref": "…", "hash": "sha256:…" },
  "output": { "ref": "…", "hash": "sha256:…" },

  "rungs": [ { "rung": "grounding", "clock": "invocation",
               "decision": "refused",
               "predicate_id": "field_has_tool_source",
               "expected": "a tool of this agent's could supply `genome`",
               "actual":   "no declared tool returns genome data",
               "reading": "fault" } ],

  "fields": [ { "name": "genome",
                "value": "2.4 Gb",
                "grade": "unavailable_no_tool_source",
                "floor_of": ["taxonomy:tool_verified", "genome:unavailable"],
                "settleable_by": "gbif_lookup" | null } ],

  "routed": [ { "assertion_id": "…", "to": "human",
                "why": "no data source exists that could settle it" } ],

  "reading": "fault", "detail": "…" }
```

**What exists:** everything under `agent`, `model`, `parent_episode_id`, and the
raw claims for `fields` (`episodes.assertions[]`). **[E]**

**What is needed:**

1. `episode_id` on `gate_decisions` **[B]** — one column. This is what makes a
   rung outcome durable and joinable to an episode.
2. Grounding moves from `Retention::Counted` to `Retention::Recorded` **[B]** —
   one line in `gate_trust::GATES`. Today the grounding verdict is a counter and
   the report itself is dropped; the audit records that `enforce` mutates a local
   `doc` discarded at the call site.
3. Artifact `hash` on input and output **[B]** — the genuinely new field. Note
   `AgentCard.declared_prompt_sha256` is the only content hash in the system
   today, so there is a precedent for the shape.
4. A writer for `parent_episode_id` **[B]** — the column exists and every call
   site passes `None`. `tools_legacy.rs` already documents the intent: *"delegation
   tools can stamp it as `parent_episode_id` on the child."*

**Please keep the value.** `fields[].value` carries what the model actually
claimed, marked, never stripped. Nulling it destroys the only evidence that could
ever answer which model fabricates what, and that corpus is what
`learning-loops-and-contractual-gates.md` §2 lever 4 selects training data from.
A null cannot be labelled.

**One request on shape, because this is A2A.** Return the mark *wrapped*, not
adjacent:

```
"genome": { "value": "2.4 Gb", "provenance": "unavailable_no_tool_source",
            "verified": false }
```

not `genome` beside `genome_provenance`. A sibling tag is safe when a human
reads it and unsafe when the consumer is another agent — a naive consumer must
break loudly rather than believe silently. This is the `genome_profiler` shape
exactly: the value was present, internally consistent, and wrong. If `produces`
declares the wrapped type, `port_trust` checks it at the seam, which answers the
requirements doc's §1.2 with an existing rung rather than a new mechanism.

### What the UI does with each field

So you can push back on cost rather than on the diagram.

| field | rendering |
|---|---|
| `rungs[]` in order | **the belt.** Each rung is a checkpoint the token visibly passes or stops at |
| `rungs[].decision` + `reading` | where the token stops, and in which of the three colours |
| `expected` / `actual` | the failure detail on the checkpoint — this is what makes it a diagnosis rather than a red dot |
| `fields[].grade` | per-field provenance mark, using the ladder's own five values |
| `floor_of` | the weakest-link explanation: *why* this field's grade is what it is |
| `settleable_by` | routes the item to `pending_tool`; `null` routes it to `pending_human` |
| `parent_episode_id` | the correction chain — the previous attempt, and what changed |
| `input.hash` vs previous `output.hash` | the seam check. A mismatch is a substituted artifact, mechanically detectable |
| `corpus_eligible` | whether this episode counts toward any claim about model behaviour |

---

## ② `GET /api/workspaces/:workspace_id/composition`

The machine's shape. Everything here exists except `belts`.

```
{ "members": [ { "agent": { "id": "…", "name": "…" },
                 "ports": { "accepts": ["…"], "produces": ["…"] },
                 "calibration": 0.71 | null,
                 "cost_per_run": 0.0159 | null,
                 "runs": 309 } ],
  "belts":   [ { "from": { "agent": "…", "port": "…" },
                 "to":   { "agent": "…", "port": "…" },
                 "verified": false,
                 "declared_by": "owner" | "strategist" } ],
  "strategist": { "agent_id": "…" | null,
                  "mode": "router|pipeline|decompose|debate" | null },
  "budget": { "total": 500, "spent": 118, "remaining": 382 },
  "seams": { "labels": 512, "seam_forming": 13, "orphans": 499 } }
```

`members`, `ports`, `calibration`, `cost_per_run`, `runs`, `strategist.agent_id`
and `budget` all exist and are already served in `/api/bestiary` and on `teams`.
**[E]**

`belts` is the new table **[B]**. Minimal shape:

```
(workspace_id, from_agent_id, from_port, to_agent_id, to_port,
 declared_by, declared_at, verified_at NULL)
```

`verified` should mean *a schema resolved on both sides*, not *the labels
matched* — 3 of 10 published agents carry a resolvable schema in
`output_contract.schema`, so filled and hollow are both real states today. **[E]**

`strategist.mode` has no home we could find. `teams.workflow_meta` may be the
right place; we would rather you decided than we guessed.

---

## ③ `GET /api/workspaces/:workspace_id/verification-queue`

A read over `assertion_verifications` joined to `episodes.assertions`. **No new
table.**

```
{ "items": [ { "assertion_id": "…", "episode_id": "…",
               "agent": { "id": "…", "name": "…" },
               "field": "genome", "raw_claim": "…",
               "state": "pending_tool" | "pending_human" | "settled",
               "settleable_by": "gbif_lookup" | null,
               "age_days": 3.2, "stale": false,
               "verdict": "…" | null, "source_citation": "…" | null } ],
  "tally": { "pending_tool": 0, "pending_human": 0, "settled": 0 },
  "reading": "unknown",
  "detail": "No assertion has ever been submitted for verification. …" }
```

What this needs is the **writer**: when grounding marks a field, enqueue an
`assertion_verifications` row with `actor_kind` chosen by whether a tool exists
that could settle it. **[B]**

Per the paper, `settleable_by = null` is not only a work item for a person — it
is a prioritised request for the data integration that would close it. We would
like to render it as both, so please keep the field even when it is null rather
than omitting it.

**Please follow the handoff's own empty-state rule here.** The queue will be
empty on arrival and `detail` should distinguish *no assertion anywhere has been
submitted* from *others have and this workspace has not* — the same distinction
the coordination-notes endpoint already makes.

---

## ④ `GET /api/agents/:agent_id/rejection-rate`

Derived from ③ once it has rows.

```
{ "refuted": 4, "total": 10, "rate": 0.4, "n": 10,
  "reading": "idle" | "fault" | "unknown",
  "detail": "…" }
```

The paper's framing is *"an agent refuted four times in ten is measurably
different from one refuted twice in a hundred"* — so **`n` must travel beside the
rate** or the number is a lie at low volume. At `n = 0` this is `unknown`, never
a clean record: an agent nobody has checked is not an agent that passed. That is
the same rule as the gates' `never_asked`.

---

## What we are not asking for

* **Not a new review queue.** `assertion_verifications` is it.
* **Not `prior_episode_id`.** `episodes.parent_episode_id` already exists;
  `episode_corrections` already carries a richer chain including
  `synthetic_episode_id` and `persona_version_bump`. A third would be a third
  answer.
* **Not a restructure of the five rungs.** Only Grounding is a filter on agent
  output; Presence, Liveness and Truth are contracts on the platform's own data
  and Binding is about input. The concrete change we need is one gate promoted
  from metric to control, which `command_registry` already declares and pins.
* **Not per-episode rows in `anomaly_events`.** That is the exception channel and
  its threshold is working — 276 flags across 1,431 timeline entries, none in an
  actionable category. Writing a row per marked field would flood the HITL queue
  and destroy the semantics that keep Loop 2 rare. Routine gate outcomes belong
  in `gate_decisions`; exceptions belong in `anomaly_events`.

---

## Open questions we cannot answer from the UI side

1. **Is the artifact hash over the raw model output or the post-grounding
   document?** The seam claim needs it to be whatever crosses the boundary, and
   there are two boundaries. Our guess: hash both, because the difference between
   them *is* what grounding did.
2. **Where does `strategist.mode` live?** `teams.workflow_meta` is our guess.
3. **What settles `verified` on a belt?** Label match is cheap and weak; schema
   resolution is the real thing but only 3 agents can currently satisfy it.
4. **Does promoting grounding to a control change the response contract for
   existing callers?** The demotion is recorded as intentional, so somebody
   decided this once. We would like to know why before it is reversed.
5. **`Standing` or `sweep`?** Pick one before both surfaces ship different words
   for the third clock.

---

## Why ① first

It changes the surface more than the other three combined, and it is the cheapest:
one column on `gate_decisions`, one retention change, one hash, one writer for a
column that already exists. No new tables.

It is also the only one that gives us a **concrete journey to render**, which is
the whole difference between the census we shipped and the machine
`compositions_v16` shows. Everything else on this list is better with it and
thinner without it.
