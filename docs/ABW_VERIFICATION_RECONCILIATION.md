# Reconciliation: ABW Verification Remediation vs. the drift/testing work already shipped

**Date:** 2026-08-15 · **HEAD:** `67066e4a` (post-v0.17.0)
**Reconciles:** `docs/abw_verification_prompts.md` (prompts 1–4)
**Prior art:** `docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md` (same exercise, DB layer)

---

## TL;DR

The four prompts name a real defect class and name it correctly. It is the **same class**
the v0.16.1 → v0.17.0 run has been working on for two weeks — *a check that reasons about
shape passing while content is wrong* — one layer up, at the agent card rather than the
database column.

Three things follow from that:

1. **Prompt 1 is real and under-scoped.** The Genome Profiler prompt does not merely
   *invite* fabrication, it **supplies the values**: `agent_card.json:6` hands the model
   "Lepidoptera ~400-500Mb, Coleoptera ~200-800Mb, Orthoptera can exceed 6Gb" and a schema
   example whose `ploidy` field is pre-filled with `"diploid"`. Meanwhile the one guard the
   prompt *does* carry — "Do not invent taxonomy" — protects the single field family that
   has a tool. Guard present, guard correct, guard aimed at the wrong fields. And the fix's
   backfill target is wrong: the copy users actually see is
   `creature_conditions.genome_profile` (migration 083), not `episodes`.

2. **Prompt 4's premise is stale, and the truth is worse than it says.** The caveat it
   quotes (`templates/ecology.html:407`) is not an accidental discovery — the comment
   twenty lines above it says the same thing deliberately. But the panel it guards is
   itself cosmetic: **7 of 100 cards declare an `output_contract`, and 0 of the 7 contain a
   schema.** They carry `produces_schema`, a *string naming* a schema
   (`"weather_oracle/raw_predictive_distribution"`), which `ecology.html:399` renders under
   the heading **"Schema"** — and suppresses the caveat for. The surface that flags
   cosmetic contracts has a cosmetic contract in it.

3. **The classification pass prompt 4 commissions has a knowable answer before it runs.**
   There is **no JSON Schema validation library anywhere in the workspace** — zero hits for
   `jsonschema`, `valico`, `schemars` across `Cargo.toml`, `crates/*/Cargo.toml`,
   `agent-bestiary/*/Cargo.toml`. Nothing can enforce a declared card schema, so every card
   port classifies `COSMETIC` or `UNTESTABLE` by construction. Spending an adversarial
   testing pass to discover that is the expensive way to read a dependency list.

---

## Part 1 — Premise audit

| Prompt premise | Reality | Verdict |
|---|---|---|
| Genome Profiler has only taxonomy tools but is asked for genome/phylogeny/conservation | `agent_card.json:9-55` — `gbif_species_search`, `gbif_taxonomy_tree`, `execute_agent`. The JSON shape at `:6` demands all four blocks. | **TRUE** |
| "deep knowledge of…" framing invites parametric fill | Present verbatim, **and** followed by a genome-size lookup table and a pre-filled `"ploidy": "diploid"` example. Stronger than the prompt claims. | **TRUE, understated** |
| "no anomaly flagged" | Correct, and structural: `anomaly_events.kind` is `CHECK (kind IN ('drift','rolling_conflict','rupture','safety'))` (`migrations/105:145-146`). There is no kind this could have been filed under. | **TRUE** |
| A post-generation "schema validator" slot exists | One seam: `parse_agent_json` (`src/handlers/creatures/agent_modules.rs:14`, called at `:613`). It is a *parser with a fallback*, not a validator, and its fallback emits `{"taxonomy":{},"genome":{},"phylogeny":{}}` — `conservation` isn't even in it. | **PARTIALLY TRUE** |
| CRAFT / CONDUCT anomaly taxonomy | **Zero occurrences in the repo.** That vocabulary is from another system. The real surface is `anomaly_events` (`migrations/105:135`) with a 4-value CHECK and HITL routing via `requires_review`. | **FALSE as named** |
| "Update the app UI layer for the Genome Profile card" | The Rabble client is Flutter; `rabble-web/` contains only build artifacts (`main.dart.js`). **The card's source is not in this repo.** | **FALSE (cross-repo)** |
| "flag all 56 existing episodes" | Three stores hold the fabricated values, not one: `creature_conditions.genome_profile` (the cache, migration 083), `episodes` (via `dispatch_rabble_action`), and `creature_transitions.metadata.result` (`agent_modules.rs:657`). Episode count unverified from here — no DB access in this pass. | **FALSE scope, real task** |
| Observatory ports are "labels, not schemas" | True, and already stated on purpose at `templates/ecology.html:385-389` and `src/pipeline.rs:18-29`. | **TRUE, already known** |
| The 7 typed contracts are the exception to the label problem | They are not. 0 of 7 carry a schema; all carry `produces_schema`, a string identifier. | **FALSE** |
| Contracts might be enforced somewhere and need testing to find out | No schema validator exists in the workspace to enforce with. | **FALSE** |

### The number that settles prompt 4

Across the 100 curated cards: 99 declare `accepts`, 99 declare `produces`. That is **238
distinct accept labels and 289 distinct produce labels — 513 distinct labels total, of
which only 14 appear on both sides.** 191 accept labels and 257 produce labels occur
exactly once in the entire corpus.

(All figures in this document come from `scripts/port_census.py`, label-set fingerprint
`d9fc503bdf753a79`. Earlier drafts carried 508/494 and a bare 56, derived by hand at a
shell prompt; both were wrong and both were self-consistent, which is why the script has a
`--self-check` mode. See its module docstring.)

So the port vocabulary is ~97% singleton. Even the *weak* claim — "these two agents compose
because a label matches" — is unavailable for 499 of 513 labels. `src/pipeline.rs:20-24`
already records the same shape for stage-level ports ("267 distinct values, 234 of them
appearing once"); this is the top-level card measurement of the same thing.

---

## Part 2 — What the recent work already covers

The last two weeks built three things that overlap the prompts directly. None of them is
mentioned in the prompts, and two of them are the pattern the prompts should copy rather
than duplicate.

| Shipped | What it does | Which prompt it overlaps |
|---|---|---|
| `src/pipeline.rs` (2a44afd9) | Plans a declared `workflow_template` and classifies every seam as `MatchedByLabel` / `MatchedWithEntryInputs` / `Unmatched`. Refuses to present an asserted match as a verified one; API says `matched_by_label`. Found 9 declaration defects across 6 cards on first run. | **#4**, substantially |
| `crates/fermi-console/src/negotiate.rs` `bind_input` (7b768a08) | Answers "does this agent declare a free-text input at all" → `Declared` / `NoTextInput` / `Undeclared`, matching on the *shape* of the label because four curated cards call it `forecast-question` / `factor-x1-query` rather than `query`. Explicitly treats `Undeclared` as absence, not contradiction. | **#4**, the input half |
| `src/rollup_trust.rs` + `tests/rollup_contract.rs` (f7814a0e) | The template. A declared contract in code, a Tier-1 offline tripwire in CI **and the pre-commit hook**, a Tier-2 live content check. Catches exactly the class both prompts describe: present, correctly typed, and wrong. | **#3 and #4**, as method |
| `episodes.cost_basis` / `cost_rate_key` (227008d7) | Per-row provenance for a computed figure: `measured_split` vs `assumed_split` vs `unknown_model`. Two rows reading `$0.31` are no longer indistinguishable when one measured and one assumed. | **#1**, as method |
| `build_agent_json`'s `source: "agents_row"` tag (`rollup_trust.rs:180-183`) | The existing, working precedent for prompt 1's `_provenance` key: a consumer can tell an unmeasured zero from a real one. | **#1**, as method |

**Method lesson from f7814a0e, which both audit prompts should absorb:** the audit that
found six wrong surfaces was worthless a month later; the *tripwire* is what persists. That
commit also records the detector being wrong twice before it was right — v1 flagged 23
sites, nearly all legitimate; v2 certified the exact line it existed to catch. Prompts 3
and 4 both commission **markdown tables**. A markdown table of 100 agents is stale the day
a card is edited, and nothing will notice. Convert both to a declared const contract plus a
test, in the `schema_trust` / `rollup_trust` house style.

---

## Part 3 — The keystone that is still missing

`docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md` Phase 1 specified an append-only
`integrity_log` as the shared timeline every later deliverable needs. **It does not exist**
— zero hits for `integrity_log` in `src/`, `crates/`, `migrations/`.

Both new prompts want it and neither knows it is missing:

- Prompt 1's "log a CONDUCT anomaly event … so this becomes visible in observability".
- Prompt 3's and 4's findings, which otherwise land in a markdown file nothing reads.
- Prompt 4's follow-up ("build this into CI so any new agent is tested before it's
  considered done") — a gate with nowhere to record its verdict is a gate whose history you
  can't query.

For prompt 1 specifically there is a cheaper interim: `anomaly_events` already exists, is
append-only, has an `episode_id` FK, and is already read by the observatory
(`src/handlers/observatory.rs:159`), the HITL queue (`:219`), and `anomaly_triager`. Filing
provenance violations there gets visibility today. **Cost: a migration widening the `kind`
CHECK** (`migrations/105:145-146`), which is a single-statement `ALTER` under the PgBouncer
rules `scripts/lint-migrations.sh` enforces. Note `agent_id` is `NOT NULL REFERENCES
agents(agent_id)`, so the writer needs the DB agent row, not the card id.

---

## Part 4 — Deliverable disposition

### Prompt 1 — Genome Profiler fabrication

| Item | Disposition |
|---|---|
| 1. Rewrite system prompt, split by provenance tier | **Build.** Also delete the genome-size lookup table and the pre-filled `"ploidy": "diploid"` example — instructing the model not to guess while handing it the guess is the failure mode, not a mitigation of it. |
| 2. Post-generation validator | **Build, but not at `parse_agent_json`.** That function is shared by every rabble creature module and is a parser with a fallback. Put the provenance check in its own function so `enemy_sensor` / `prey_locator` can adopt it, and so it is unit-testable without a DB. |
| 2b. "Log a CONDUCT anomaly" | **Substitute:** `anomaly_events` with a widened `kind` CHECK. See Part 3. |
| 3. UI card gating | **Out of repo.** Flutter source is elsewhere; `rabble-web/` is build output. File it there. The API-side half — never emitting an unsourced non-null — is item 2 and is sufficient to make the UI's job trivial. |
| 4. Backfill 56 episodes | **Build, retargeted.** The load-bearing store is `creature_conditions.genome_profile`. Note `agent_modules.rs:517-535` already invalidates the cache on empty `taxonomy` — a prior fix for a prior symptom. That predicate is exactly wrong for this bug: a profile with real GBIF taxonomy and fabricated genome data is `cache_is_valid = true` **forever**. |
| — | **Also:** the test fixture at `src/agent_backend/tool_executor.rs:1221` asserts against `"estimated_size_mb": "480"`. The test corpus has normalised the fabrication. Fix it in the same change or the new validator will be written to keep an existing test green. |

### Prompt 2 — Real data sources

**Defer, unchanged.** The prompt's own coverage caveats are accurate and it correctly
sequences itself behind prompt 1. One correction: it treats "Not Evaluated" as a real IUCN
value distinct from a gap — that is right, and it is the same distinction
`episodes.cost_basis` already draws. Reuse that vocabulary rather than inventing a second
one.

### Prompt 3 — Ungrounded output fields

**Build, as a contract, not as a table.**

The prompt's per-agent table is the right *analysis* and the wrong *artifact*. Proposed
substitution, mirroring `rollup_trust.rs`:

```
src/grounding_trust.rs
  FieldGrounding { agent_id, output_field, status: Sourced|Derived|Unsourced|Unclear,
                   source_tool, source_response_field, why }

tests/grounding_contract.rs
  Tier 1 (offline, blocking) — every field declared Sourced names a tool that
    appears in that card's mcp_tools; every Unsourced field's card prompt must
    contain an explicit null instruction. No DB, hook-eligible.
  Tier 2 (live) — sample recent episodes for each agent; an Unsourced field
    that is non-null in a real response is a violation.
```

Tier 2 is what distinguishes this from the shape checks that already pass. Note the
prompt's own trust check ("spot-check 3-4 `SOURCED` rows") becomes free: Tier 1 *is* that
check, run on every commit, for every row.

Scope discipline: do not attempt 100 agents. Seed it with the agents that have tools and a
structured output contract — the creature modules and the weather chain — the same way
Phase 6.1 of the prior reconciliation says to seed the rule registry.

### Prompt 4 — Declared vs. enforced contracts

**Reduce, then build.**

The adversarial pass as specified is the expensive way to establish a floor that a static
pass establishes in an afternoon:

- **Step 1 (cheap, do first):** a static classifier over the card corpus. Every port with
  no schema and no validator → `COSMETIC` by construction. Given no schema library exists,
  this classifies all 513 labels and all 7 `output_contract`s without running an agent.
  **Shipped as `scripts/port_census.py`.**
  Output as a `#[test]`-backed count, so the number moves when someone fixes a card.
- **Step 2 (adversarial, where it can bite):** reserve real adversarial testing for the
  contracts that *do* have an enforcement path, because those are the ones where the answer
  is not knowable in advance. The honest list: `stamp_invocation`'s slug validation
  (`src/api_server.rs`, already has a test asserting a caller can't hang `status:success`
  on a failed run), `pipeline::plan`'s `Unmatched` blocking, the coherence gate
  (`src/handlers/observatory.rs:367-381`), `schema_trust::verify`, and the rollup tripwire.
  Prompt 4's trust check applies here and should be honoured — the coherence gate is a
  known fail-open on `gamma == None` (`agent-bestiary/coherence-gate/src/gate.rs:104-114`,
  logged in the prior reconciliation and still open), so at least one `ENFORCED` candidate
  will not survive contact.
- **Step 3:** fix `templates/ecology.html:399-407` regardless of the audit. Rendering a
  schema *name* under the heading "Schema", and suppressing the "not verified" caveat for
  doing so, is the exact error the caveat exists to prevent. This is a 5-line change and
  should not wait for a classification pass.

The prompt's stated follow-up — fold the check into CI so new agents are classified at
generation time — is the correct end state and is already the pattern for three other
contracts. It should not be deferred to a separate task; it is the only version of this
work that survives.

---

## Part 5 — Corrections to carry back to whoever wrote the prompts

1. `CRAFT` / `CONDUCT` anomaly kinds do not exist here. The vocabulary is
   `anomaly_events.kind ∈ {drift, rolling_conflict, rupture, safety}`, CHECK-constrained.
2. The Rabble UI is not in this repo.
3. The genome profile cache, not the episode log, is what users read.
4. The observatory caveat was written deliberately, and is *too weak*, not absent.
5. There is no schema validation library, so "is this contract enforced" has a
   one-line answer for the whole card corpus.
6. `docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md` Phase 1 (`integrity_log`) is a
   prerequisite these prompts inherit without knowing it.

---

## Part 6 — Recommended immediate scope

1. **Prompt 1 items 1, 2, 4** (retargeted per Part 4) plus the `tool_executor.rs:1221`
   fixture. This is a day's work and stops an active fabrication path that costs users
   2 credits a call (`src/gas.rs:114`).
2. **Prompt 4 step 3** — the `ecology.html` caveat fix. Five lines, and it removes a
   surface that currently certifies seven cards as typed when none are.
3. **Prompt 4 step 1** — the static classifier, as a test. Establishes the floor and gives
   every later fix a number that moves.

Defer prompt 2 entirely. Defer prompt 3's Tier 2 until `integrity_log` or the widened
`anomaly_events` exists to record it in — a violation detected and printed to stdout is the
`tracing`-with-no-subscriber bug from the last reconciliation, repeated at a new layer.

---

## Part 7 — Target state (settled 2026-08-15)

Parts 1–6 reconcile four prompts that are explicitly **detection-only** ("audit-only", "do
not fix anything in this pass"). That is not the goal. The goal is **enforcement by
default**: agents carry typed I/O contracts of the kind WSDL gave web services, and the
runtime rejects messages that violate them. A report is not the product; the gate is.

This section overrides the audit framing wherever they conflict.

### 7.1 The contract language already exists, unread

`apps/kask_wild.json`, `apps/kask_simops.json` and `apps/fermi_forecast.json` each carry a
`schema_json` that is a hand-rolled WSDL:

| WSDL part | `schema_json` equivalent |
|---|---|
| namespace + version | `$schema: "kask-wild/1"` |
| portType / operations | `action_types[].type` |
| binding + address | `action_types[].api_endpoint` |
| message types (XSD) | `action_types[].fields.*.{type,required,enum}` |

It is served at `/api/apps/:slug/schema` (`src/handlers/apps.rs:1022`) and referenced by two
cards via `output_contract.schema_endpoint`. **`grep -rn "action_types" src/ crates/`
returns zero hits** — no Rust reads it, nothing parses the `__ACTION__` blocks it
describes, nothing checks `required` or `enum`.

So this is a promote-and-enforce job, not a design-from-scratch job. Do not invent a second
contract format.

Related asymmetry: tool **inputs** already carry real JSON Schema in every card
(`input_schema`, 304 tools), but it is passed straight through to the provider
(`llm_executor.rs:472`, `mcp_client.rs:384`) — nothing of ours validates against it. Typed
inputs enforced by someone else; untyped outputs enforced by nobody.

### 7.2 Settled decisions

1. **Type identity via a registry**, not per-card inline schemas. `produces: "X"` →
   `accepts: "X"` becomes a check on a registered type, not a string comparison. The corpus
   already namespaces this way (`kask_wild/action_block`, `weather_oracle/...`).
2. **Remediate the existing corpus**, do not merely ratchet. A ratchet is anti-regression
   and says nothing about the 100 cards already shipped. See 7.4.
3. **On an output that violates its declared type: one repair-retry, then reject.** The
   validator's error is fed back to the model once. Costs a second LLM call on the failure
   path; record it in `episodes` alongside `cost_basis`.
4. **Multi-operation agents.** One implicit operation (free text in, blob out) is what
   forces every contract to degrade into "a blob labelled `phylogenetic_profile`".
   `action_types` already assumes multiple named operations.

### 7.3 Labels are contract objects, not metadata

`accepts`/`produces` are the I/O contract as it exists today, and they are fake in exactly
the way the Genome Profiler's fields are fake: a plausible name with nothing behind it.
They are in scope. Every label must resolve to a registered type or carry an explicit
unresolved disposition.

Four mechanical Tier-1 checks follow — no DB, no LLM, no schema library:

| Check | Catches | `genome_profiler` |
|---|---|---|
| **a. Resolves** — label names a registered type | the 499-of-513 unbridged swamp | all 6 labels unresolved |
| **b. Backed** — every `produces` label maps to a field in the declared output | ports with nothing behind them | `tree_visualization_description` — no such field in the declared shape |
| **c. Grounded** — a `produces` label whose backing fields are all `Unsourced` is a fabrication claim | typed-but-still-fake | `genome_summary` — advertises exactly the data it cannot source |
| **d. Bound** — the invocation matches the declared input, checked at the boundary | interface binding | `NoTextInput` — card accepts `species_data`/`taxonomy`/`gbif_key`, `agent_modules.rs:571` sends prose |

Check **c** is what stops the campaign ending with 100 agents that are typed and still
lying. Nulling the Genome Profiler's genome fields while leaving `produces:
["genome_summary"]` on the card would leave the composition layer advertising a summary
that is now guaranteed null — the lie relocated, not removed.

**Check d already exists and is orphaned.** `negotiate::bind_input` (7b768a08) returns
`Declared` / `NoTextInput` / `Undeclared`, and `is_mismatch()` is true for `NoTextInput`.
Its only callers are `crates/fermi-console/src/cockpit.rs:3188` and `:6088` — the desktop
console. The API server never calls it, so every HTTP execute path is unchecked, including
the creature module that charges 2 credits a call. Correct detector, wired to one client
instead of to the boundary; failure mode #3 from the business-rule prompt, at the agent
layer.

**Measured across the 100 curated cards: 44 declare a text input, 0 declare nothing, 48
would be flagged `NoTextInput`, and 8 more are disputed.** The disputed eight are plainly
free-text ports that `is_text_input` misses — `sensor_advisor`'s
`free_text_stage_description`, `simops_advisor`'s `free_text_process_description`, and the
`*_task` convention used by `comparator`, `marketing_composer`, `product_scout`,
`regulatory_scanner`, `sidestream_miner` and `valuechain_mapper`. `port_census.py` reports
them as their own class rather than folding them into either verdict.

Those false positives are **the argument for the registry, not a bug to patch first.**
Widen the heuristic and it swallows non-text ports; leave it narrow and it misses real
declarations. A regex divining intent from 513 uncontrolled strings has no correct setting.
The exit is one registered `fermi/free_text_query` that agents reference — at which point
`is_text_input` is deleted rather than tuned.

### 7.4 Finding the legacy agents — evidence, not intent

A legacy agent **cannot be typed from its card**. That is precisely how `output_contract`
acquired seven entries naming schemas that do not exist: someone read a card, formed a view
of what it ought to produce, and wrote down a name. Typing from intent reproduces the bug
one layer up, and a schema makes a wrong field look *more* trustworthy.

The honest source is what each agent actually produced — `episodes`. Induce a candidate type
from response history, report its conformance rate against that history, then ratify. The
conformance rate is doing double duty: evidence the induced type is right, and a measure of
how unstable the agent is. Same method as ed69b1a4 (fit the weather spread from verified
station error rather than an assumed prior).

Induction over-fits — twenty samples that all carry `notable_genes` will mark it `required`.
So an induced type records its own sample size and marks `required` only above a support
threshold *and* a sample-size floor. A type that does not say how well-evidenced it is, is
just another assertion.

Triage dispositions, in the `Disposition::WriteOrphaned` house style. The point of the
taxonomy is that **"fix" means something different in each**, and only two are mechanical:

| Disposition | Evidence | What "fix" costs |
|---|---|---|
| `Typed` | schema registered and enforced | done |
| `Inducible` | stable shape across history, conformance above threshold | **mechanical** — ratify the induced type, batch it |
| `Divergent` | history disagrees with itself | prompt work *first*; typing a divergent agent enforces an arbitrary one of its shapes |
| `Ungrounded` | shape stable, fields have no tool source | grounding contract, sometimes new tools (prompt 2). Typing as-is **certifies the fabrication** |
| `Prose` | genuinely conversational | declare no typed port; explicitly non-composable. An honest outcome, not a defect |
| `Unrun` | zero or near-zero episodes | **cannot be typed from evidence.** Delist, or type-on-first-use with the first N runs quarantined |

`Prose` and `Unrun` must be first-class or the burn-down gets cleared by reclassification
rather than by work. Two counters, not one: unresolved ports (513 → registry size) and
ungrounded ports. Note the registry will not be small: normalising for spelling (`-` vs
`_`, plurals, `_json`/`_data` suffixes) collapses only **11 of 513** labels, so the spread
is genuine vocabulary invention rather than punctuation drift. There is no cheap
normalisation win here.

### 7.5 Revised order

| # | Step | State |
|---|---|---|
| 0 | **Census + triage** — offline over the card corpus, live over the database | **done** — `scripts/port_census.py` (§7.6), `scripts/port_census_live.sh` (§7.7) |
| 1 | `templates/ecology.html` caveat keys on a resolvable schema | **done** — caveat now renders 100/100, was 93/100 |
| 2 | `genome_profiler` as the pilot: field provenance + port typing + port grounding + input binding, and the first registered type | **done** — `src/grounding_trust.rs`, migration 200, card v1.2.0. See §7.8 |
| 2b | **Retain raw agent output** — forced by §7.7.1 | **done** — migration 199, `AgentOutput::raw_response` → `Episode::response_text`. Ordered before the pilot because evidence accrues only from the moment the column exists |
| 3 | `Inducible` batch — blocked on 2b, and on a DB-backed census mode (§7.7.2) | **blocked, correctly** — mig-199 only began retaining output at deploy; the corpus accrues from now. DB-backed census shipped (`port_census_live.sh dbports`) |
| 4 | Move `bind_input` to the execute boundary; delete `is_text_input` once labels resolve | **done** — `src/port_trust.rs`, both boundaries, parity-pinned. See §7.9. Deletion still pending on the registry |
| 5 | Burn-down as scoreboard, over **both** populations | **done** — `port_census.py --gate`, see §7.10 |
| 6 | Extend `grounding_trust` to the sibling creature agents | **done** — `enemy_sensor`, `prey_locator`. See §7.10 |

Step 0's live half reads `episodes` against the production database in the pattern of the
existing `scripts/*_live.sh` tiers. Read-only, but it is production, so it wants an explicit
decision rather than an assumption.

### 7.8 Pilot: `genome_profiler` (step 2, done)

What the pattern looks like on one agent, end to end.

**`src/grounding_trust.rs`** — the third trust contract. `Grounding` is
`Sourced { tool, response_field }` / `Unsourced` / `Narrative`, declared per
output field with a mandatory justification (a test rejects a `why` under 40
characters). `enforce(agent_id, &mut doc)` nulls ungrounded fields, stamps
`<block>_provenance` from a closed three-value vocabulary, and returns what it
removed. 13 unit tests, no DB, no LLM.

Four design points that turned out to matter more than expected:

1. **The narrative is the leak channel.** Nulling `genome.estimated_size_mb`
   is not enough: the `summary` restates the number in prose, and
   `parse_evidence_text` lifts `summary` out as the episode's `evidence` — so
   it is the sentence a user actually reads. `Grounding::Narrative` fields are
   scanned for claims the sourced blocks cannot support, and a leaking summary
   is nulled, not merely counted, because a validator cannot rewrite prose
   into honesty.
2. **The leak scanner's first draft was wrong in the predicted way.** A bare
   `" gb"` needle matches **"GBIF"**, so an honest taxonomy-only summary
   citing its own source was flagged as leaking a genome size. `LeakRule` now
   distinguishes `Word` from `Quantity`, where a quantity only counts with a
   number in front of it. A check that fires on correct output gets switched
   off, and the switching-off looks like cleanup — the same trap f7814a0e
   documents hitting twice.
3. **`phylogeny.sister_taxa` is deliberately left alone.** `gbif_taxonomy_tree`
   really does return sibling taxa, so it is `Sourced`. Stripping it along with
   its neighbours would be an over-reaching check, and there is a test whose
   only job is to stop that.
4. **Enforcement is idempotent, and there is a test for it.** Otherwise every
   read of a cached profile would raise a fresh anomaly.

**The card (v1.2.0).** The genome-size lookup table and the `"ploidy":
"diploid"` example are gone — instructing a model not to guess while handing
it the guess is not a mitigation. The prompt now names what its tools do and
do not return, and says so about the summary explicitly.

Ports, per §7.3:

| Before | After | Why |
|---|---|---|
| `accepts: [species_data, taxonomy, gbif_key]` | `accepts: [query]` | The only caller sends one prose string (`agent_modules.rs:571`). `bind_input` now returns `Declared("query")` instead of `NoTextInput` |
| `produces: [phylogenetic_profile, genome_summary, tree_visualization_description]` | `produces: [rabble/phylogenetic_profile]` | `genome_summary` advertised precisely the data the agent cannot source; `tree_visualization_description` had no field and the prompt never mentioned visualisation, rendering or diagrams |
| no `output_contract` | inline JSON Schema, `$id: rabble/phylogenetic_profile` | **The first typed output contract in the corpus (0 → 1).** `genome_provenance` and `conservation_provenance` are `const`, and the unsourced fields are `"type": "null"` — the constraint is in the type, not only in the prompt. A prompt is a request; a schema is a rejection |

**Runtime.** `enforce` runs on the write path before anything caches or
renders, and on the **read** path too — which retires the 13 fabricated cached
profiles immediately, with no data migration and without re-running the agent
at 2 credits a call. The old `cache_is_valid` predicate could never have caught
them: it asks only whether `taxonomy` is non-empty, and taxonomy is the one
block that has a tool.

**Migration 200.** `anomaly_events.kind` gains `'grounding'`. It had to:
the CHECK permitted only `drift|rolling_conflict|rupture|safety`, so the
observability system was **closed to the class of defect nobody had thought of
yet**. Part 2 tags the 13 rows with `_grounding_review` rather than
overwriting them — the read path already protects users, and a model's guess is
a free calibration signal once a real tool lands. `lint-migrations.sh` rejected
the first draft for a bare DROP+ADD CONSTRAINT, correctly: through PgBouncer a
lost second statement turns a widening into the removal of the constraint
altogether.

**The fixture that specified the bug.** `tool_executor.rs`'s
`parse_evidence_text_genome_profiler_response` asserted against
`"estimated_size_mb": "480"` and a summary reading *"with a ~480 Mb genome
typical for Lepidoptera"*. The test corpus had normalised the fabrication, so
any validator written later would have been written to keep it green. Replaced
with a post-enforcement document, plus a second test asserting the fixture
itself satisfies the contract — because a fixture is a specification, and that
one specified the defect.

**Census movement:** `output_contract: typed` 0 → 1, ports `registered` → 3
(the check also found `simops_companion` and `wild_companion` were already
using type-reference names), `input_binding: declared` 44 → 45, `unbacked`
45 → 42.

One measurement caveat: total distinct labels fell 513 → 510 and bridging
14 → 13, because retiring three fake ports shrinks the denominator. **The
burn-down metric must therefore be `registered` rising, not `unbridged`
falling** — otherwise the scoreboard rewards keeping fake labels.

**A fake port was doing more damage than composability.** Retiring
`tree_visualization_description` immediately broke
`scripts/taxonomy.py audit --gate derived`: the agent's order derived to
`Imaginales`, and `Imaginales` matches `image|avatar|scene|render|art|visual`
(`taxonomy.py:106`). That single fabricated label — naming a visualisation the
agent has no field for and whose prompt never mentions rendering — was the
**only** token in the whole card matching the imaging rule. `genome_profiler`
has been filed as an imaging agent, in the taxonomy that drives discovery and
routing, purely on the strength of an output it never produced.

Corrected to `Evidentiales`. Worth generalising: a fake port is not inert
decoration. It is an input to every downstream system that reads the card, and
those systems fail quietly because a well-formed label is indistinguishable
from a true one — which is this document's thesis, arriving from an unexpected
direction.

### 7.9 Port binding moved to the boundary (step 4, done)

`src/port_trust.rs` — fourth in the family, asking *is the caller sending
what the agent said it takes?*

**The finding was worse than "unchecked".** `negotiate::bind_input` shipped in
v0.16.0, answered correctly, and its only callers were two sites in
`cockpit.rs`. Meanwhile `stamp_invocation` read `input_binding` out of the
**request body** and wrote it onto the episode as a queryable tag — so the
platform's record of whether the interface matched was the *caller's claim*
about the match. A client that had never seen the card could assert
`declared:query` against an agent accepting only `gbif_key`, and it would be
filed as fact.

So this was not "add a check". It was **stop recording an assertion as a
finding**, then compute the finding:

* `stamp_invocation` no longer reads `input_binding` at all, with a comment
  saying why. A test asserts it emits no `ibind:` tag.
* New `stamp_input_binding(episode, verified, claimed)` writes the server's
  verdict, computed from the resolved card at both execute boundaries.
* A caller-supplied claim is **compared**, not trusted. Disagreement adds
  `ibind:claim-disagreed` — the only available signal that client and server
  have drifted apart.
* Tag vocabulary is unchanged from v0.16.0, so the time series does not split
  at this deploy. There is a test for that.

**The rule widened, and the widening is the argument against the rule.**
The census found the console's heuristic would flag 56 of 100 cards, eight of
them wrongly — `sensor_advisor`'s `free_text_stage_description`,
`comparator`'s `compare_experiment_task`, and six more. `port_trust` adds
`free_text*` and `*_task`, taking 56 → 47. Bare `description` is deliberately
excluded: posting a research prompt into `ar_beacon`'s caption field is a real
mismatch. That line is judgement, drawn in a named test rather than buried in
a regex. **Every adjustment to `is_text_input` is evidence for the registry,
not progress toward a good rule** — its deletion is the success condition, and
the module docs say so.

**Three implementations existed within a week**, which is the drift this repo
keeps finding:

| # | Where | Status |
|---|---|---|
| 1 | `crates/fermi-console/src/negotiate.rs` | pre-flight hint; **not** machine-checked — known gap |
| 2 | `scripts/port_census.py` | the scoreboard |
| 3 | `src/port_trust.rs` | the gate |

(2) and (3) are pinned by `agents/port_binding_expected.json` +
`tests/port_binding_parity.rs`, on the `taxonomy_derived_expected.json`
precedent — Python authors the fixture, Rust must agree. If they drift, the
burn-down number stops describing what the boundary does, which is worse than
either being wrong alone. (1) is aligned by hand with a comment naming the
canonical module; it cannot be pinned without the console depending on the
API-server crate, which would drag sqlx and axum into a GPUI desktop binary.
Recorded here rather than pretended away.

**The parity test was verified to fail.** Temporarily removing the widening
from the Rust rule produced a named disagreement on all eight agents
(`comparator: python says declared_by_convention, rust says no_text_input`)
before being restored. A green contract test that cannot go red is the thing
this document exists to complain about.

**CI:** four new blocking steps — the grounding contract, the parity test, the
census self-check, alongside the existing schema and rollup tiers.

**Current binding census:** 45 `declared`, 8 `declared_by_convention`, 47
`no_text_input`. Those 47 are now visible per-episode as `ibind:mismatch`
rather than invisible — reported, not yet refused. Refusal waits until the
registry makes the verdict unambiguous; blocking on a heuristic is how a gate
earns the right to be switched off.

### 7.10 The rest (steps 5 and 6, done)

**Two more agents under the grounding contract — and the important result is
that one of them passes.**

`enemy_sensor` has **nothing Unsourced**. Its `scan_nearby_creatures` tool
returns the creatures it reports on, and the risk rating is a judgement it is
asked to make. Applying the `genome_profiler` treatment would have nulled the
agent's entire product. That forced a fourth variant:

| `Grounding` | Meaning |
|---|---|
| `Sourced { tool, response_field }` | a tool returned it |
| `Unsourced` | no tool could; must be null |
| **`Inferred { from }`** | **a judgement the agent is asked to make; kept, labelled `model_inference`** |
| `Narrative` | prose; checked for leaks |

The distinction is **retrieval versus judgement**. A genome size is a fact
sitting in a database the agent did not query. A threat level is in no
database; producing it is the job. Without `Inferred`, a contract under which
every agent looks guilty would be indistinguishable from a broken checker, and
would rightly be switched off. `a_well_grounded_agent_passes_completely` is
the test that pins this, and it is the most important one in the module.

`prey_locator` is the opposite case, and the sharpest in the corpus. Reading
`execute_scan_nearby_creatures` (`tools_legacy.rs:2800-2809`) rather than its
name: nearby creatures come back as `h3_cell` and nothing else — **no
latitude, no longitude, no distance**. Only the target creature carries
coordinates. So every waypoint `lat`/`lng`/`altitude_m`,
`estimated_distance_m` and `distance_cells` is a number the agent was never
given, **in a flight plan** — a document meant to be flown rather than read.
The strategy, difficulty and vulnerability ratings survive; the geometry does
not.

Notably these are *missing derivations*, not impossible ones: H3 cell centres
and grid distances are exactly computable and `h3o` is already a dependency.
The contract entry for `distance_cells` says so, naming it the cheapest
`Unsourced` field in the corpus to retire.

Supporting changes: `Grounding` paths gained `[]` array support (these agents
keep their interesting fields inside arrays, and a contract that can only
address top-level scalars would pass by being unable to look); `block_of`
strips the marker so `threats[].species` provenance lands on `threats`; and
`gbif_verified` became `tool_verified`, because a status string naming one
specific tool stopped being true the moment a second agent joined.

**The burn-down ratchet** — `scripts/port_census.py --gate` against
`agents/port_burndown_baseline.json`, blocking in CI. Same mechanism as the
migration baseline that went 26 → 6: it does not demand progress, only that
nothing regresses unobserved, and loosening requires regenerating the baseline
in the same commit so it appears in review.

```
input_binding.no_text_input     47   (down)
port_resolution.registered       3   (up)
port_resolution.unresolved     336   (down)
produces_status.unbacked        42   (down)
shape.output_contract            1   (up)
shape.prose_only                74   (down)
```

`registered` leads deliberately. **`unresolved` falling is not evidence of
progress**: deleting a fake port shrinks it exactly as well as typing a real
one, and the pilot did precisely that — retiring two invented ports took the
corpus from 513 labels to 510 without typing anything. `registered` is the
only counter deletion cannot fake. Verified to bite by temporarily claiming a
better baseline; it named both regressions and explained the direction.

**The DB-backed census** — `port_census_live.sh dbports` covers the population
the offline census cannot see, reported separately rather than merged because
the two need opposite fixes: card agents have ports that are *fake*, community
agents have none at *all*. Averaging would hide both.

It also sizes the problem down sharply. Of the 51 portless community agents,
**only 3 have ever run** (`efra_valuation` 34, `efra_critical_factor` 7,
`efra_intel` 7). The fourth portless agent with traffic is `prey_locator`
itself — curated, 77 runs, and portless **in the database while its card
declares ports**. That disagreement is benign today: `resolve_agent_card`
(`api_server.rs:5564`) starts from the file card and overrides `accepts` only
when the DB value is non-empty, so the runtime reads the card. Worth knowing
that the precedence is implicit — unlike `mcp_servers` directly below it,
which documents an explicit three-way NULL/`[]`/non-empty table, `accepts` has
no way to express "deliberately none".

### 7.11 The provenance floor (step 7, done)

**The laundering path.** Everything in §7.8–§7.10 governs what an agent may say
in one response. It says nothing about what happens to that response next, and
what happens next is this: the `ontologist` reads an agent's episodes during a
dream cycle, writes `semantic_rules` from them, and `kg_context` retrieves
those rules and appends them to an agent's system prompt under **"Learned
Knowledge"**. At that point a stored sentence has become a premise another
agent reasons from.

Nothing in that path recorded how well-grounded the episodes were. A rule
extracted from ten `tool_verified` lookups and a rule extracted from ten
paragraphs of prose were stored in the same table, retrieved by the same query,
and rendered in the same line of the same prompt. The second is **worse than a
bare hallucination**, because its citation is real: `source_episode_cluster`
genuinely points at episodes that genuinely said that.

The prompt line read:

```
- (72% match, 90% confidence) <rule content>
```

Both numbers are real and neither is a measurement. `confidence_score` is the
extraction model's self-report about a generalisation it had just written;
`match` is cosine similarity. Labelled "confidence" and set side by side, they
read as calibration, and to a model `90% confidence` is licence to assert the
content downstream. It now reads:

```
- (72% match, 90% self-rated, UNGROUNDED - no tool could confirm this) <rule content>
```

**Two rules, not one.**

| rule | statement | why |
|---|---|---|
| floor | the **weakest** verdict among the sources | nine sourced episodes and one guess is a guess; averaging lets volume launder a fabrication |
| ceiling | never stronger than `model_inference` | reading well-sourced episodes and generalising is *judgement*, and judgement does not inherit retrieval |

So the best value an extracted rule can ever hold is `model_inference`. That is
not a defect, it is the honest ceiling for the class of operation, and it is
asserted by `no_rule_can_ever_render_as_tool_verified` over every value the
vocabulary permits.

**Unknown is not a rung on the ladder.** The subtle part, and the one that took
two attempts. Verdicts are ordered, but *unknown* is the absence of information
about an order and cannot participate in the `min`. Nine `tool_verified`
episodes and one whose response was never retained does **not** floor at
`tool_verified` — the tenth could be anything. Nor at `unavailable`: the tenth
is not known to be bad either. The answer is unknown.

But nine `tool_verified`, one *known* ungrounded, and one unretained floors at
`unavailable`, and the unknown changes nothing: no verdict it could turn out to
hold would lower a floor already resting on the bottom. **An unknown source
poisons the result only when it could still move it.** Implemented as
`FloorAccumulator` in `src/provenance_oracle.rs` and tested both ways, because
getting it wrong in the lenient direction lets one ungradeable episode in a
cluster of ten manufacture a clean floor for the other nine.

**Why the memory crate declares a trait.** `fermi` depends on
`agent-bestiary-memory`, so the memory crate cannot call `grounding_trust`
without a cycle. Copying the arithmetic across was the obvious alternative and
would have produced two answers to the same question — and the one that
disagrees is the one that gets believed, because it is the one nearest the
writer. This module has already had that bug once (`gbif_verified` for
`tool_verified`). So `ProvenanceOracle` is declared where it is needed and
implemented where the contracts live, exactly as `LLMProvider` already is.

**The site that was missed.** There were three production `ConsolidationWorker`
construction sites and the first pass wired one. The one missed —
`handlers/creatures/agent_modules.rs` — is the **highest-volume rule writer on
the platform**, because creature dreams run on a timer while the HTTP handler
runs when somebody asks. Wiring the path you are looking at and missing the
path that runs by itself is the normal shape of this mistake and it does not
announce itself: the rules still get written, just ungraded. Now enforced by
`tests/provenance_floor_coverage.rs`, which scans for construction sites and
requires each to be wired or named in `EXEMPT` with a reason. Verified able to
fail by unwiring the creature path.

**Measured state of the corpus** (165 active rules, live, read-only):

| state | rules |
|---|---|
| ungradeable — evidence not retained (pre-migration-199) | 159 |
| fully retained, gradeable | 5 |
| dangling — cited episodes have no rows | 1 |

and the 5 gradeable rules — all `genome_profiler`, all about tool ordering —
floor at **`unavailable_no_tool_source`**. Correctly: the weakest block in a
`genome_profiler` response is its `genome` block, which no tool can source, so
an extraction over that document can only be as good as the weakest part it
might have read. Those five rules are live in production and being injected
into `genome_profiler`'s own prompt. They will rise to `model_inference` when
`ncbi_genome_search` covers the field — which is the point: **the floor is the
demand signal for tool integration, expressed in the one place it changes
behaviour.**

The 96% ungradeable figure is not a failure of this work, it is its first
finding, and it must be read as missing coverage rather than as ungrounded
rules — the remedy is retention and contracts, not retraction. `NULL` therefore
means *unknown* everywhere: in the column, in `SemanticRule`, in the oracle, and
in the prompt ("grounding unknown", deliberately distinct wording from
"UNGROUNDED"). A report that counted `NULL` as grounded would show the corpus
getting cleaner as coverage got worse.

**Artifacts.** Migration 203 (`provenance_floor` + `provenance_floor_basis`,
CHECK kept in step with `PROVENANCE_VALUES` by a test that parses the SQL);
`agent_bestiary_memory::provenance`; `src/provenance_oracle.rs`;
`grounding_trust::{strength, floor, extracted_floor, response_floor,
EXTRACTION_CEILING}`; `kg_context::grounding_note`;
`tests/provenance_floor_coverage.rs`; corpus report and live oracle run in
`scripts/grounding_contract_live.sh`.

### 7.6 Census findings — offline half

`scripts/port_census.py`, 100 curated cards, label-set fingerprint `d9fc503bdf753a79`.

| Measure | Result |
|---|---|
| Distinct labels / bridging / unbridged | 513 / 14 / **499** |
| Machine-readable output shape declared | **26** of 100 (`declared`); 74 `prose_only` |
| `produces` labels: backed / unbacked / unresolvable | 48 / 45 / **248** |
| Input binding: declared / no-text-input / disputed | 44 / 48 / 8 |
| `output_contract`: typed / named-only / absent | **0** / 7 / 93 |

Three of these change the plan:

**1. Check (b) is uncomputable for three quarters of the corpus.** Only 26 cards contain a
strictly-parseable JSON object in their system prompt, which is the only machine-readable
statement of output shape a card has today. For the other 74 the output shape exists solely
as prose, so 248 `produces` labels return `unresolvable` — the question cannot even be
asked. This is the finding, not a limitation of the extractor: a deliberately tolerant
parser would raise coverage while making the result unfalsifiable. **The registry's first
job is to move output shape out of prompt prose**, before "is this port backed?" is a
question the corpus can answer.

**2. `unbacked` needs human ratification and cannot be automated.** A `produces` label may
legitimately name the whole output document rather than a field in it, and nothing in a free
string distinguishes those. The census reports the ambiguity instead of guessing: of 45
`unbacked`, 42 do not end in a document-shaped noun and are the strongest candidates;
`genome_profiler`'s two both do, and are flagged for ratification rather than convicted.
Which is right — `tree_visualization_description` is genuinely unbacked (the prompt never
mentions visualisation, rendering or diagrams, and no field exists), while
`phylogenetic_profile` plausibly names the document. **Two identical-looking labels, opposite
verdicts, no mechanical way to tell.** That is the clearest possible statement of why ports
must become types.

**3. Zero typed output contracts, corpus-wide.** Confirms Part 1: the seven
`output_contract`s are `named_only`.

The census is advisory by construction — it exits 0 on every finding. It becomes a gate when
the registry exists and `resolves` is answerable; at that point its checks graduate into
`agent_contract::requirements()` so there stays exactly one definition of a well-formed
agent.

### 7.7 Census findings — live half

`scripts/port_census_live.sh`, read-only against production, 2026-08-15. Three findings
invalidate parts of §7.4 and must be absorbed before step 2.

**1. The induction corpus does not exist.** §7.4 proposes inducing types from response
history. `episodes` has a `query` column and **no response column**. What is retained in
`context` is a parsed digest — `reasoning`, `evidence`, `sources_consulted`,
`tool_invocations` — produced by `parse_evidence_text`, which is itself per-agent
(`tool_executor.rs:1216` special-cases `genome_profiler`). The agent's own document is
discarded.

| Store | Rows with retained output |
|---|---|
| `episodes` | 3233 total, **no response column** |
| `episodes.source_text` | 496 |
| `workspace_messages` (sender_type='agent') | **66** |
| `creature_conditions.genome_profile` | 13 |

So `Inducible` is not a general disposition. It is available for `genome_profiler` (13
cached documents) and essentially nowhere else. **Retaining raw output is a prerequisite for
the campaign, not a detail of it** — and it should be added to `episodes` before the
`Inducible` batch is scheduled, or the batch has nothing to read. Note the irony: the fix
for "we cannot verify what agents produce" starts with "begin keeping what agents produce."

**2. The card corpus is not the corpus.** 748 agent rows exist; **107 agents have run and
have no card on disk.** `port_census.py` globs `agents/*/*/agent_card.json` and is blind to
every agent admitted through the API path — which is the path with the worse ports:

| Tier | Agents | Empty `accepts` | Empty `produces` |
|---|---|---|---|
| `test` | 591 | 591 | 591 |
| `curated` | 85 | 1 | 1 |
| `community` | **51** | **51** | **51** |
| `system` | 21 | 6 | 6 |

**Every community-tier agent has no ports at all**, exactly as `agent_contract.rs`'s
docstring describes — all of it predating the gate that now blocks it. Eleven of the 51 are
`visibility='public'`. So the remediation population is two disjoint groups needing opposite
fixes: 100 cards with ports that are *fake*, and 51 community agents with ports that are
*absent*. The offline census only sees the first. It needs a DB-backed mode before the
burn-down means anything.

(Incidental: `is_public` and `visibility` disagree — 11 rows are `is_public=f,
visibility='public'`. Not in scope here, but it is a second declared-vs-actual pair and
wants its own look.)

**3. `Unrun` is small — my estimate was wrong.** §7.4 predicted a large `Unrun` fraction.
Measured: **90 of 100 curated cards have ≥5 episodes, 10 have none, 0 are in between.** The
corpus is genuinely exercised, so "delist the dead ones" is not the cheap win I suggested it
might be. The campaign is close to full-size on the card side.

**Pilot evidence.** `genome_profiler`: 56 episodes (the prompt's figure is correct) and
**13 cached profiles**, of which 11 carry populated `genome`, `phylogeny` and
`conservation`. Sample of what is on screen now, none of which GBIF can supply:

| species | estimated_size_mb | chromosome_count | iucn_status |
|---|---|---|---|
| `Reclavaspis evexa` | 200-400 | typically 10-20 (variable in scale insects) | Not formally assessed |
| `Sphingonotus personatus` | 800–1200 | 2n = 16–24 (typical for Acrididae) | Not Evaluated (NE) |
| `Apatura iris` | 420-480 | n=31 (diploid 2n=62) | Not Evaluated (presumed Least Concern) |

**Two incidental findings from running the suite** (neither caused by this work, both worth
their own ticket):

* `agent-bestiary-memory`'s lib tests call `dotenvy::dotenv()` (`store.rs:4838`) and take
  `DATABASE_URL` from `.env` — **so the documented test command writes to production.**
  `get_test_store()` upserts a `test_agent_<uuid>` row before each episode assertion. The
  **591 `test`-tier agent rows** counted in §7.7.2 are the accumulated residue of that, which
  is also why `scripts/clean_test_agents.sql` exists. Running the suite twice during this
  work created 8 more; they were deleted by explicit id and the count is back to 591.
* Four of those tests currently fail against production with `column "parent_episode_id" of
  relation "episodes" does not exist` — the untracked migration 198 is not deployed. They
  are failing *safe* only by accident: the INSERT aborts after the agent row is written.

The `iucn_status` column deserves specific attention. "Not Evaluated" is a **real IUCN
value** — §prompt 2 correctly distinguishes it from a gap. Here the model has fabricated the
*appearance of having consulted the Red List*, with a confident parenthetical gloss. That is
strictly worse than a null, because it is indistinguishable from a successful lookup. Any
provenance scheme must be able to tell `not_evaluated_iucn` (queried, answer was NE) from
this (never queried, answer invented).
