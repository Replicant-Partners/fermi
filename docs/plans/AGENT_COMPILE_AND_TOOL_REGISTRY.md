# The agent compiles — tool registry migration and the UX that depends on it

**Written 2026-09-01 as a fresh-session entry point.** Everything needed to start
is here; nothing in it requires the conversation it came out of.

**Companion:** `docs/plans/TOOL_REGISTRY_REFACTOR.md` — the mechanical migration,
already written, plus two amendments (§2.1.1 and §3.6) added at the same time as
this document. Read that for *how*. Read this for *why now*, *what not to touch*,
and *what the UX needs from it*.

---

## 1. The goal, in one paragraph

An author gives an agent some tools and selects, from those tools' declared
responses, which fields the agent will produce. The platform then **compiles**
the agent: every declaration is resolved against what actually exists, and the
author is told what resolved, what is wrong, and what is merely waiting on a
source that does not exist yet. No wizard, no forms — source and diagnostics, the
way a compiler works. An agent that compiles is an agent ready to produce
information somebody else can trust.

## 2. Measured facts

Re-derived at the time of writing. **Do not re-measure these to start; do
re-measure before claiming any of them fixed.**

| fact | value | why it matters |
|---|---|---|
| tools with a declared response shape | **12** of ~100 | the other 88 fall back to extracting nouns from description prose, marked `unconfirmed` |
| `BuiltinToolDef` literals | 93 in `tools_legacy.rs`, 7 in `weather_tools.rs` | why response shapes live in a side table today |
| `FIELD_CONTRACTS` entries | 105, across **10** distinct agents | the counts in them are per *agent*; this bit us once already (46s endpoint) |
| `genome_profiler` fields | 6 `Sourced`, 7 `Unsourced`, 1 `Derived`, 1 `Narrative` | the richest honest declaration on the platform; **do not prune it** |
| `Reading` variants | `Idle`, `Fault`, `Unknown` | **there is no green.** An agent that works reads `idle` |
| `Finding` fields | `check`, `message` | no severity, therefore no `pending` state |
| `PUT /api/agents/:agent_id` accepts | ~20 fields incl. `system_prompt`, `model`, `llm_provider`, `temperature`, `model_params`, `valence`, `fork_pricing`, `output_contract`, `accepts`, `produces` | **no missing endpoints.** Panels are UI work, not backend work |
| `episodes.workspace_id` | exists (mig-226), **0 of 3,688** attributed | forward-only; attribution starts from the next workspace run |
| episodes with no embedding | **2,770 of 3,688** | see §7, "the dream thing" |

## 3. What already exists — read this before building anything

The single largest risk to this work is rebuilding something that is already
here. Inventory, with paths:

| thing | where | state |
|---|---|---|
| **contract compiler** | `src/contract_sketch.rs::compile(&self, tool_names) -> Result<Compiled, Vec<Finding>>` | works. Server-side on purpose |
| compile HTTP seam | `src/handlers/contracts.rs::compile_handler` | works |
| **contract editor widget** | `static/js/widgets/contract-builder.js` (1,756 lines) + `static/css/contract-builder.css` | works. `ContractBuilder.mount({ container, agentId })`. **Three hosts:** the create wizard step 4, `/contracts`, and the specimen shelf |
| **tool response shapes** | `src/tool_response_shapes.rs` — `ToolResponse { tool, evidence, fields }`, `Evidence::Constructed \| Vendor`, `response_for(tool)` | right in every respect except where it lives. 12 tools |
| tools endpoint for authoring | `GET /api/contracts/tools` → `{ tools, response_shapes, note }` | works, and its `note` states that absence means *unread*, not *empty* |
| decompile (contract → sketch) | `GET /api/contracts/decompile/:id` | works |
| **declaration ladder** | `src/declaration_ladder.rs::LADDER` — `rung`, `declares`, `owner`, `unlocks`, `without_it` | works. Served per agent by `/api/specimen/:name` as `declaration.rungs` |
| **the configuration shelf** | `templates/specimen.html` — drag handle, persisted width, three groups | Declaration group live and ranked; Intelligence and Manage are read-only |
| create wizard | `/agents/new`, `templates/agent_create.html` (1,691 lines) | 6 steps: Identity, Ontology, Capabilities, Contract, Economics, Review |
| legacy agent page | `/agent/:id`, `templates/agent_detail.html` (~6,500 lines) | **stays reachable** until the shelf absorbs it |
| grounding enforcement | `src/grounding_trust.rs` | `Grounding::Sourced \| Unsourced \| Inferred \| Derived \| Narrative` |
| the reader for legacy prose | `src/field_probe.rs` — `parse_hint`, `tool_takes_endpoint`, `search` | keep. It reads the 105 hand-written `response_field` strings. New contracts should not need it |

## 4. The parallel work, and the seam

**A typed schema builder is in flight in the same tree.** At the time of writing
these files are modified or new and uncommitted:

```
src/workflows/agent_contract.rs          src/declaration_ladder.rs
src/grounding_trust.rs                   src/agent_backend/envelope.rs
src/agent_backend/tools_legacy.rs        src/api_server.rs   src/lib.rs
static/js/widgets/contract-builder.js    agents/port_binding_expected.json
agents/curated/*/output_contract.sketch.json      (new: per-agent sketch files)
src/a2a_card.rs  src/a2a_task.rs  src/a2a_webhook.rs  src/handlers/a2a.rs
```

**The seam, stated so both sides can move:**

1. **The typed-schema side owns**: the sketch format, the compiler, the schema
   registry, `enforce_from_output_contract`, and the contract-builder widget's
   internals.
2. **This work owns**: the `PlatformTool` trait, the registry, domain modules,
   and `response_shape()`.
3. **The contract between them is one function signature** —
   `ContractBuilder.mount({ container, agentId })` — and one data shape:
   `GET /api/contracts/tools` returning `response_shapes`. The shelf depends on
   nothing else about the widget. If `mount`'s signature changes, exactly one
   line in `templates/specimen.html` changes.
4. **`response_shape()` is additive to the typed-schema work, not a competitor.**
   The sketch compiler asks *"is this contract well-formed?"*. `response_shape()`
   answers a question it currently cannot: *"does the tool you named actually
   return the field you claimed?"* One is syntax, the other is a fact about the
   world.
5. **Staging discipline, learned the hard way.** `tools_legacy.rs`,
   `api_server.rs`, `grounding_trust.rs`, `lib.rs` and `declaration_ladder.rs`
   are edited by both sides. Committing whole files there commits the other
   author's uncommitted work and has broken the release build once. Rebuild the
   file as `git show HEAD:<f>` plus only your hunks, stage the blob with
   `git hash-object -w` + `git update-index --cacheinfo`, then assert their
   symbols are absent from the staged blob before committing.

## 5. Is this blocking UX?

**Partly, and the blocked part is the part that matters.** Itemised, so the two
tracks can run at once:

### Blocked on `response_shape()` coverage

* **Selecting fields from a tool.** The core authoring interaction. Possible for
  12 tools today; for the other 88 the builder extracts nouns from description
  prose and marks them `unconfirmed`, which is honest labelling of a method that
  cannot be trusted.
* **The interesting half of the compile.** *Does the tool you named return the
  field you claimed?* Answerable for 12% of tools.
* **"This agent is ready."** A positive statement needs sourced fields with
  named evidence. Without declared responses the platform can say a contract is
  well-formed and cannot say it is *true*.

### Not blocked — can proceed in parallel, today

* **Intelligence and Manage panels, editable.** `PUT /api/agents/:agent_id`
  already accepts every field. Pure UI work. This is the largest immediately
  available UX win.
* **`pending` as a first-class state.** `Grounding::Unsourced` already means *no
  tool exists, so the field must be null*. The vocabulary exists; the surfaces
  collapse it into failure. Fixing that is independent of the registry.
* **A fourth `Reading`.** See §6.1. Independent.
* **The shelf's other rungs** getting their editors (`ports` is `accepts` /
  `produces` — two arrays and a `PUT`).
* **Pulse views by workspace and app.** Unblocked by mig-226; waiting on
  attribution to accumulate, not on this.

**Conclusion:** start the registry migration in a fresh session, and keep the
non-blocked UX moving in parallel. They meet at Phase 5.

## 6. The UX requirements this must satisfy

Written as requirements rather than designs, except where the design is already
settled by a rule this project holds.

### 6.1 There must be a way to say an agent works

```rust
pub enum Reading { Idle, Fault, Unknown }   // src/panel_absence.rs
```

Three readings, none meaning *working*. `genome_profiler`'s health today is 8
`idle`, 2 `unknown`, nothing at fault — so no surface can state the true thing,
which is that this agent functions.

The module name is the confession: `panel_absence` was built to explain absences
honestly, and does. It was never built to assert presence.

**Requirement.** A positive reading, and it must be *earned* rather than default:
resolved declarations, sourced fields with named evidence, and pulses that
carried grades. Not "no fault found" — a statement with a subject. Whatever it is
called, `Idle` must keep meaning *has had no occasion*, because that distinction
was expensive to win.

### 6.2 The compile has three states, and the third is load-bearing

| state | meaning | who acts |
|---|---|---|
| **resolved** | a tool exists, dispatches, and declares a response containing this field | nobody |
| **error** | the named tool does not exist, or cannot supply the named field | the author, now |
| **pending** | no tool can supply it **yet** | nobody — a standing request for an integration |

**Green means zero errors, not zero pending.** An agent with seven pending fields
and no errors compiles.

`Finding { check, message }` has no severity today, so the compiler can only say
*wrong*. It needs a third outcome — and `pending` must not be a lesser `resolved`
or a milder `error`; it is a different kind of thing with a different owner.

Why this is not a nicety: without it the only route to a green agent is deleting
ambition. `genome_profiler` declares seven fields no tool can supply, which is
"it should eventually be richer" made machine-readable, and it is the best
example on the platform of what the contract system is for.

### 6.3 Field selection, not field typing

Given an agent's tools, the union of their declared response fields is the
**candidate set**. Selecting from it yields a `Sourced { tool, response_field }`
that is correct by construction. No prose, no parser, no heuristic.

`field_probe::parse_hint` and `tool_takes_endpoint` exist because the 105
existing contracts are prose. They stay, as a reader for those. **A newly
authored contract must not need them.**

A tool with no declared shape must be **visibly** unread rather than silently
absent — `Evidence` already carries this and the tools endpoint already says so
in its `note`. Do not lose it.

### 6.4 No wizard

A wizard asks a fixed sequence and tells you nothing until the end, which is
exactly the reported experience of `/agents/new`. The replacement is not a
better sequence, it is a different model: **edit, compile, read the diagnostics,
edit again.**

Creation and management are then the same components in two arrangements:

| wizard step | shelf panel | state |
|---|---|---|
| 4 · Contract | Declaration → `field_contract` | **done** — `ContractBuilder`, three hosts |
| 3 · Capabilities | Intelligence | extract as a mountable widget |
| 1 · Identity | Manage | extract as a mountable widget |

Extract each the way `ContractBuilder` already was, and *"create an agent by
walking it up the declaration ladder"* stops being a flow to build. It becomes
the same editors mounted in the ladder's order. **Any design that produces a
third creation path is wrong** — there are already two.

### 6.5 The row grammar is settled; reuse it

The diagnostics list is a list of declarations, and the trace already settled
what such a row looks like: **`value · condition · act`**, positionally fixed,
where the condition is one token from a closed vocabulary and the act is the
control that closes it. Learn one row, read a hundred.

The rules that go with it, all of them paid for:

1. A table cell holds a value or a token, never a sentence.
2. **Explain once, not per row.** A reason belonging to a *state* goes in one
   legend keyed by the token the rows print.
3. **Absent must look different from bad.**
4. A fold hides detail, never a finding, and its summary carries the count it is
   hiding.
5. **If the platform can name what would close a gap, the name is the control.**
   Never print the name of a remedy you do not offer.
6. A lens changes columns and sort, not the page.

### 6.6 Embedding is a declaration with a visible consequence

The create wizard collects embedding provider/model as configuration. It is not
configuration; it decides whether the agent can ever learn. See §7.

**Requirement.** Wherever the embedding choice is made, the consequence is stated
next to it, and the platform's own default is honest about the cost.

### 6.7 Also outstanding, from the same review

* **Compound agents are agents** — no new noun. `cohere_and_coordinate` needs a
  **roster** on its specimen page; its pulse children already render in the
  trace's flow strip.
* **`/flow/:id`** needs the agent roster and a link to the actual work surface.
* **The agent page** needs the reverse join: which workspaces and apps it belongs
  to.
* **The specimen Health tab** is eleven panels answering a question about the
  platform, and is the only reason a single-agent page computes a fleet census
  (46s → 9.4s so far; the remaining ~9s is that census). Replace with one line
  linking into the Observatory, and put the census on a clock with a cache.
* **`/loops` and `/gates`** route to the same handler. Two routes to one render.

## 6.8 Live defect found while wiring the shelf: the compiler deletes port labels

**Measured 2026-09-01. Not fixed here, because both files are in flight on the
other side of §4's seam.**

`ContractBuilder.saveTo` PUTs two columns:

```js
body: JSON.stringify({
  output_contract: cbCompiled.output_contract,
  produces: cbCompiled.produces,          // <- one entry
})
```

and `Compiled` documents the intent plainly: *"Replaces the card's `produces`.
One entry, the declared type."*

But `agents.produces` is not the declared type. It is the **port label set** that
`port_trust::bind_input` and the seam census match on — the ladder's own `ports`
rung says so, and cites 289 distinct `produces` labels across the fleet. Measured:

```text
14 agents have output_contract.produces_schema
10 agree with produces
 4 disagree, and the disagreement is not noise:

  football_analyst      schema: fermi/football_evidence
                        produces: {fermi/football_evidence, evidence,
                                   win-probability, elo-analysis,
                                   match-prediction, form-analysis,
                                   league-analysis}

  condition_forecaster  schema: kask_wild/condition_forecast
                        produces: {condition_forecast, species_probability,
                                   brier_forecast}   ← none is the schema name
```

**Recompiling and saving `football_analyst` deletes six labels other agents can
be matched against on a seam.** `condition_forecaster` loses all three and gains
one it never declared.

The decision this needs, and it is a decision rather than a bug fix: does
`produces` mean *the type I emit* or *the labels I can be matched on*? If both,
they cannot share a column, or the compile must **merge** rather than replace —
and a merge needs a rule for which labels are the contract's to remove.

Until then, the shelf's field editor deliberately offers neither `produces` nor
`output_contract`, and `scripts/check_agent_fields.js` asserts it — with this
measurement as the reason, so the guard cannot be removed as pedantry.

### Which rungs one editor closes

Worth stating because the shelf got it wrong first:

| declaration | rung | written by |
|---|---|---|
| `agents.accepts` | ports (half) | **nobody yet** — the one genuinely missing editor |
| `agents.produces` | ports (half) | ContractBuilder, derived from `produces_schema` |
| `output_contract.produces_schema` | output_type | ContractBuilder |
| `output_contract.schema` | output_schema | ContractBuilder |
| `output_contract.grounding` | field_contract | ContractBuilder |

So **one save closes three rungs and half of a fourth**, and the only declaration
with no editor at all is `accepts`.

### 6.8.0 DECIDED: the compile is additive

**`Compiled::merge_produces` — a compile adds the declared type at the front and
removes nothing.**

§6.8 asked for "a rule for which labels are the contract's to remove". The
answer is *none of them*, and the route to it is worth keeping because the first
answer was cleverer and wrong.

The clever version: `card_contract` **enforces** a namespaced `produces_schema`,
so a type name always contains `/`, while port nouns are conventionally bare.
That gives a syntactic rule needing no similarity matching — the compiler owns
namespaced labels, the author owns bare ones. `port_census.py::port_resolution`
already draws the same line from the other end (`registered` vs `unresolved`).
Measured over the fleet: **314 labels, 14 namespaced and every one its own
card's declared type, 300 bare, zero exceptions.**

The test written to pin that measurement **failed on its first run**, against a
card committed while it was being written: `simops_companion` declares
`kask_simops/action_block` *and* `kask_simops/prose_response`. Two namespaced
output types, both real — it answers with an action block or with prose. The
clever rule would have deleted the second, silently, which is the same defect
§6.8 opened on.

So: additive. The cost is a stale type name lingering after a `produces_schema`
rename — a deliberate act whose leftover an author can delete by hand. The cost
avoided was deleting a declared output type nobody was asked about. For a column
that is also a match surface, *never loses a label* is the property worth having,
and it makes the merge trivially idempotent.

One implementation, three callers — the `contract-sketch` binary, the corpus
test, and `POST /api/contracts/compile` (which now takes `existing_produces`,
so `ContractBuilder.saveTo` needs no rule of its own). A second spelling of this
rule would be a second answer to the question §6.8 was open on.

`a_recompile_never_drops_a_produces_label` asserts the property over the whole
corpus. It replaced the test that asserted the clever rule's premise — kept as a
note there, because a premise that was measured true and was false within the
day is worth remembering.

**Still open:** whether `produces` should carry both meanings at all. This stops
the compiler damaging it while that is decided, and unblocks the migration below.

### 6.8.1 What it was blocking, and the cost measured

*(added from the contract-authoring side, same day.)*

`football_analyst` was next for migration — §3 of
`docs/ISSUES_tool_declaration_gap.md`: it is one of three cards typed by hand
before the compiler existed and has **no `grounding` map at all**, so the hop
checks its document's shape and nothing about where its values came from.

**Landed** once §6.8.0 settled the rule. `produces` came out byte-identical to
what the card already had, all seven labels, so there was no taxonomy rank churn
either.

Diffed stamp-for-stamp against the live hand-written contract before splicing:

| | |
|---|---|
| provenance stamps byte-identical | **7 of 9** |
| schema properties added or removed | **none**; `required` identical |
| grounding entries | **0 → 19** — the entire point |
| stamps narrowed on purpose | 2 — `ratings`, `squad_value` (see below) |
| **`produces` labels lost** | **0 of 7** under §6.8.0 — was 6 of 7, and was the blocker |

Which labels, and who would notice:

```
evidence           named in 8 other cards
win-probability    1
elo-analysis       1
match-prediction   0
form-analysis      0
league-analysis    0
```

So the loss is real but small and countable, which is the useful thing to know
when deciding: a merge rule does not have to be clever to be safe here. Nothing
about the rest of the migration is blocked — it is one column.

The two deliberate narrowings are unrelated to §6.8 and worth recording:
`ratings` and `squad_value` currently admit the human-settlement ladder
(`human_sourced`, `human_endorsed`, `pending_human_check`, `rejected`) in their
**document** stamps, which lets the agent write into its own output that a
person verified an Elo it recalled from training data. Settlement is recorded in
`assertion_verifications` and read from there — `assessment` is settled
`human_sourced` in production today while carrying a bare
`const: model_inference` stamp, which is the proof that the schema enum was
never what made settling work. Removing them costs nothing and closes that.

### 6.8.2 One language gap found by writing it, already fixed

Every sourced block on that card is `coverage: deferred`, not `complete` — the
hand-written stamps all admitted `pending_tool_check` and were right to, because
the trace for a real episode grades contracted fields `never asked` while the
run made seven other calls.

`advanced_metrics` needed both `partial` and `deferred` at once and could have
neither: `xg` is in `fixtures/statistics` but often not requested, while `ppda`
and `progressive_passes` are Opta event-data metrics API-Football will never
carry. Dropping `pending_tool_check` collapses "never asked" into
`tool_no_match` — the exact pair the trace exists to separate. Dropping
`unavailable_no_tool_source` leaves the contract unable to say `ppda` is
unobtainable.

`Coverage::PartialDeferred` now exists, with `Coverage::TOKENS` so the guidance
prompt and its test read the vocabulary instead of keeping a third copy. The
decompiler tested `has(UNAVAILABLE)` first and so read a four-verdict stamp back
as three — the same silent narrowing already caught once on `macro_data_agent`,
which would have hit this card on its first recompile.

## 7. The dream thing, so a fresh session does not rediscover it

Not this plan's work, but it is the same defect class and it will come up.

```
no embedding → find_neighbors returns [] → no neighbours → min_samples unmet
→ DBSCAN noise → never joins a cluster → the ontologist is handed nothing
→ job completes, charges a credit, advances last_consolidated_at, learns nothing
```

`EpisodeClusterer::find_neighbors` in `agent-bestiary/memory/src/clustering.rs`.
Every zero-yield consolidation cycle on the fleet had `clusters_identified = 0`;
**52 of 52** zero-yield agents are explained by unembedded episodes and **0** have
any other cause. 2,770 of 3,688 episodes have no embedding.

The diagnosis now names it (`dreaming_maturity::MaturityInputs.episodes_without_embedding`).
**The cause is untouched, because it is a decision:** `episode_boundary::Write`
takes `provenance: Option<&ProvenancedEmbedding>` and several call sites pass
`None` — the delegation hop deliberately, documented as *"a per-fan-out cost
decision and not a bug to slip into a refactor."* That is correct, and it is also
why Loop 1 does not turn for most of the fleet. Embedding every episode costs an
embedding call per pulse; not embedding them costs the whole dreaming loop.

## 8. Order of work

**Phase 0 — amend the trait design.** `docs/plans/TOOL_REGISTRY_REFACTOR.md`
§2.1.1 is already written. Confirm `response_shape()` is on `PlatformTool` before
any tool is migrated. *Phase 2 touches every tool exactly once; without this it
touches every tool twice, and the second pass is the one that does not happen.*

**Phase 1 — trait and registry alongside the old.** No deletion, no behaviour
change. Invariant test: names unique, and every registered name dispatchable.

**Phase 2 — migrate domain by domain, declaring responses as you go.** Fold
`tool_response_shapes` into the impls, keeping `Evidence` exactly as it is.
Report coverage per domain; absent stays visible.

**Phase 3 — switch dispatch.** As written in the companion doc.

**Phase 4 — delete the legacy file.** Only after `ARMS_WITHOUT_DEFS` is empty and
stays empty.

**Phase 5 — the compile surface.** Now answerable, because the tools declare what
they return. Diagnostics in the settled row grammar; the three states from §6.2;
green from §6.1.

**In parallel, not blocked:** Intelligence and Manage as mountable widgets
(§6.4), `pending` stopping being rendered as failure (§6.2), the fourth `Reading`
(§6.1).

## 9. How to verify, and the traps

* **`node --check` is a syntax check.** It has passed over three runtime failures
  in `templates/trace.html` alone. Render checks live in `scripts/check_*.js` and
  run in CI: `check_trace_probe_render.js`, `check_pulse_row.js`,
  `check_specimen_shelf.js`. Add one for any new surface, and **assert on the
  distinctions, not the markup**.
* **Fixtures are the bug twice so far** — a document under the wrong key made six
  lens checks vacuous, and a lineage stub of the wrong shape meant four cells
  rendered their "nothing" branch. Prove a check can fail by breaking the thing
  it guards.
* **Verify the committed tree, not the working tree.** Your tree holds the other
  author's uncommitted files. `git worktree add /tmp/fv HEAD --detach` with
  `CARGO_TARGET_DIR=` pointed at the main target dir, then **`touch src/lib.rs`**
  — the shared target dir serves stale artifacts that look like real errors, and
  it has done so three times in one session.
* **Ask Postgres about SQL.** `scripts/lint-schema-consistency.py` resolves a
  qualified name against a *global* set of columns, not the aliased table, so
  `e.error_message` passed while the column is `error_details` — 500s on
  `/api/stream` and a silent empty list on the specimen. Guard projections with a
  `LIMIT 0` execution: `the_pulse_projection_resolves_against_the_schema`,
  `the_workspace_attribution_resolves_against_the_schema`.
* **`.unwrap_or_default()` on a query is a lie.** It turned a broken read into
  "No pulse recorded" for an agent with 218. Let the error travel as a value.
* **Pre-existing red tests, not yours:** `every_decision_function_is_registered_or_exempted`
  (5 unregistered, incl. the parallel `enforce_from_output_contract`),
  `every_source_scan_declares_the_test_that_proves_it_can_fire`
  (`tests/inline_js_syntax.rs` not in `SCANS`).
* Migrations are registered in `run_migrations` in `src/api_server.rs`, and CI
  parses that list rather than sorting the directory. `DATABASE_URL` in `.env` is
  **production** — additive and read-only only.

## 10. What success looks like

An author opens a specimen, drags the shelf wider, and sees:

* which of its declarations resolved, which are wrong, and which are waiting on
  the world — each with the control that closes it;
* the fields it produces, selected from what its tools actually return, with the
  evidence for each;
* its pulses, in the same grammar as everywhere else, showing who addressed whom
  and whether anything checked the answer.

And the platform can say, of an agent that has earned it, that it works.
