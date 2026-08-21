# v0.20.0 — three reasons dreaming learned nothing, and none of them was the dreaming

Loop 1 is the loop everything else depends on: an agent executes, the cycle
distils what happened into semantic memory, and the next execution reasons over
it. It has been running for months. On this deployment it had produced, in
total, **one ontology snapshot — inserted by a migration.**

Three separate defects caused that, in three different components. Each was
sufficient on its own. Each presented identically: *"dreaming ran and extracted
0 entities"*, with the cycle reporting success.

---

## 1. `create_snapshot` had never once succeeded

```sql
SELECT MAX(version) FROM ontology_snapshots WHERE agent_id = $1
```

No `GROUP BY`, so it always returns exactly one row — `NULL` when the agent has
no snapshots yet. It was decoded into `(i32,)` rather than `(Option<i32>,)`. The
NULL fails to decode, `?` propagates, and **the first snapshot for any agent
always errored.** No agent ever reached a second, so the path never ran.

The proof is in the data: the single row in `ontology_snapshots` carries
`git_commit_sha = 'seed-034'`. Migration 034 put it there. Nothing in the
platform's history was created by `create_snapshot`.

`fetch_optional` is what made it read as correct — *"there may be no row"* is
the plausible mental model for this query and the wrong one for an aggregate.
The row always exists; only its value is NULL.

Wiring `create_snapshot` into the API dreaming path earlier in this sequence
therefore produced nothing: seven consolidations completed since, each calling
it, each failing the same way. The call site was right; the callee had never
worked. And the failure was invisible **by design** — snapshot failure is
deliberately non-fatal, so it logged a warning and consolidation reported
success.

The regression test requires a database, because this is a decode contract and
a mock cannot fail the way Postgres did. It asserts both directions: the old
`(i32,)` shape still errors on a NULL aggregate, and `query_scalar::<Option<i32>>`
returns `None` and yields version 1.

## 2. The extractors never saw the agent's answers

Both `extract_entities_with_llm` and `extract_knowledge_rules` built their
prompts from `Query` plus a truncated `Context` preview. `response_text`
appeared in `consolidation.rs` only in test fixtures. Migration 199 added that
column so this would be available, and nothing then read it.

That is backwards for entity extraction especially. A question names few
entities; the answer is where they are. Measured: queries average 487
characters, responses 3,645. One episode asked *"Will Arsenal beat Manchester
City in their next Premier League match?"* and answered *"Arsenal won the
2024-25 Premier League title with 85 points"* — an entity and a fact, in the
half being discarded. `context`, which **was** included, is execution telemetry:
stop reason, token counts, evidence ids.

Both extractors now share `episode_digest`, with the response at a 1,200-char
budget — twenty unabridged 5k responses would be ~25k tokens of prompt against a
2,048-token completion.

This does not retroactively help the 3,298 episodes stored before retention
began. It changes what every cycle from here learns.

## 3. The ontologist's answers were correct and discarded at the JSON parse

`gpt-4o-mini` wraps JSON in a markdown fence whenever it feels like it, and
`generate_structured_with_usage` called `serde_json::from_str` on the raw
content. That fails at line 1 column 1, because line 1 column 1 is a backtick.

Every consolidation extractor funnels through that one function, and every
caller treats a parse failure as non-fatal. Measured: 100% of entity-extraction
calls failing, 52 agents and 829 episodes left unlearned.

A cosmetic formatting habit in a model presented as an empty knowledge graph,
fleet-wide, for months.

## 4. Creature dreaming resolved its extractor from an env var nobody sets

`prey_locator` has 93 episodes and no semantic memory at all, after three
completed cycles reporting 77, 10 and 6 episodes processed. None of the 93 was
consumed.

The creature path built its model from `std::env::var("ANTHROPIC_API_KEY")` with
a hardcoded model. The API path resolves the ontologist's card, its provider,
and a credential from the owning principal's store. Two definitions of "how the
extractor is funded", and on a deployment that funds agents through the
credential store — which this one does — the env-var one always resolves to
`None`.

The two paths ran side by side, which makes the evidence unusually clean:

| Path | Result |
|---|---|
| API (credential store) | 5 rules extracted, every cycle, 8 agents |
| Creature (env var) | 0 rules, 0 entities, every cycle, all 3 agents |

`build_extraction_llm` is now shared, so there is one answer to that question.
The creature path also refuses rather than running when it resolves `None`: the
data-loss guard already prevents episodes being consumed, but a cycle that
cannot learn still costs a dreaming credit and reports success. The API path has
refused for exactly this reason since the 91-cycle incident; this path was the
one that did not.

---

## The pattern

Four defects, four components, one shape: **a write path that worked, and a read
path pointing somewhere else.** The snapshot writer never ran, the extractors
read the wrong half of the episode, the parser read past the answer, and the
creature path read a variable nobody sets.

None produced an error. Three were *documented as working*. All four were
protected by the same property — the failure was non-fatal, so the cycle
completed and every surface reported success.

That is the argument for the checks added alongside them. A guard that cannot
fail loudly is not a guard, and a cycle that reports success having learned
nothing is worse than one that crashes.

---

## Verification

```
fermi lib          591    api-server        195
ontology            16    memory             58
```

The database-backed regression test is skipped when `DATABASE_URL_UNPOOLED` is
absent, and asserts the pre-fix shape still fails when it is present.

**What is not yet proven:** none of this has been observed producing a snapshot
in production, because that requires a deploy and then a consolidation. The
first agent to dream after this ships should produce version 1 — the first
snapshot the platform has ever created.

---

## Also in this release

This tag also contains a substantial concurrent workstream that is not described
above and was not reviewed here: FPL driver constraints and unit-space checking,
the assertion layer, card contracts and port typing, the Wild app split, glasses
shell generation, and several console fixes. See `git log v0.19.0..v0.20.0`.
