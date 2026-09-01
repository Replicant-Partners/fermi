# Generating a typed output contract for an agent

**Status:** implemented — `src/contract_sketch.rs`, first migration landed
(`equity_analyst`).
**Enforced by:** `src/card_contract.rs` at publish; `src/schema_validate.rs`
at the delegation hop.
**Guides:** `docs/guides/AGENT_CONTRACT_AUTHORING.md` for the statuses.

---

## 1. The problem is cost, not disagreement

```text
curated agents                              101
  with a typed output_contract                3
  grandfathered by TYPED_TIER_EXEMPT         86  →  85
```

That is not 86 authors who rejected the contract. It is a contract nobody
paid for twice. Count the work on `hud_field_scout`, the one card that
satisfies `card_contract::validate` in full:

| Authored | Emitted |
|---|---|
| 6 evidence blocks | 14 schema properties |
| | 14 grounding entries (bijection, enforced) |
| | 6 narrowed provenance enums |
| | 1 `required` list of 13 names |

Six decisions, thirty-five artefacts. And **eight of the fourteen grounding
entries are platform-stamp boilerplate** whose prose is near-identical block
to block, each needing 40+ characters of `why` to clear
`card_contract::grounding_explained`. Read them in the card: five of them say
the same thing five times, in five slightly different ways.

An author who writes that once writes it correctly. An author who writes it
six times copies the nearest neighbour — which is the exact failure
`grounding_explained` exists to catch and structurally cannot, because a
copied justification is 40 characters long too.

**So the lever is not a better rule. It is a smaller thing to author.**

---

## 2. What is a decision, and what is arithmetic

Three things need a human, or an agent that can be held to account:

1. What blocks does the document have?
2. What fields, of what type, does each block hold?
3. Where does each block's value come from, and **why**?

Everything else follows mechanically from (3):

| Derived | From |
|---|---|
| the `<block>_provenance` sibling | whether the source is a retrieval at all |
| that sibling's narrowed enum | the status, plus how completely the tool covers the block |
| that sibling's grounding entry | fixed: the platform writes the stamp |
| `required` | the block list |
| `additionalProperties: false`, `$id`, `$schema` | house style |
| nullable unions | the field's type expression |
| `produces` | the declared type name |

`src/contract_sketch.rs` authors (1)–(3) as a **sketch** and computes the
rest. For `equity_analyst`: six authored blocks → 13 schema properties, 13
grounding entries, 6 narrowed enums. Six of the thirteen properties were
written by the compiler.

---

## 3. The property that makes it worth having

`Sketch::compile` emits `schema.properties` and `grounding` **from one
traversal of one block list**.

The bijection between them — `grounding_declared`, the check an author fails
most because it is the only one that scales with field count — is therefore
not checked and reported. It is **unrepresentable**. This is the same move
`football_analyst`'s narrowed provenance enums make: not "discourage the
dishonest claim" but "leave it no spelling".

The compiler then runs `card_contract::validate` over its own output and
refuses to return anything that would not publish:

```text
compile() returned Ok   ⟹   the Admission gate accepts it
```

`contract_sketch::tests::contract_compiles_to_something_the_gate_accepts`
holds that line. Without it the compiler would have *moved* the authoring
cost — author writes sketch, publish refuses, author debugs generated JSON —
which is worse than not having one.

### And the line it will not cross

> **A generated `why` may only describe what the platform does. It may never
> describe where the agent's data comes from.**

The provenance-sibling entries are generated *with prose*, because their
subject is `grounding_trust::enforce` — platform behaviour the compiler knows
for certain. Every entry describing an agent's own value requires an authored
`why`, and a missing one is an error, never a default
(`a_short_why_is_refused_and_never_filled_in`).

This is the boundary that keeps `contract_sketch` from becoming what
`scripts/port_migrate.py` deliberately refuses to be. That tool emits
`NEEDS_AUTHOR` — not a valid status — precisely so a draft cannot be pasted
into a card and published, because its input contains no evidence for the
type it is asked to invent. `contract_sketch`'s input is *authored*, so it is
allowed to emit something publishable. Blur the distinction and it is a
fabrication engine with good manners.

---

## 4. The sketch

```json
{
  "domain": "equity-research",
  "produces_schema": "fermi/equity_evidence",
  "title": "Equity evidence",
  "synthesis": "cep_weighted",
  "calibration": { "signal": "brier_forecast", "comparison": "brier_score" },

  "blocks": [
    {
      "name": "profile",
      "source": {
        "status": "sourced",
        "tool": "fmp_company_profile",
        "response_field": "symbol, companyName, sector, price, marketCap, beta",
        "coverage": "complete"
      },
      "why": "FMP's /stable/profile returns every field of this block for a resolved ticker, or an empty array for a symbol it does not carry. Those are the only two outcomes, which is why the stamp admits exactly two verdicts.",
      "fields": {
        "symbol": "string?",
        "sector": "string?",
        "market_cap_usd": "number?",
        "beta": "number?"
      }
    },
    {
      "name": "summary",
      "source": { "status": "narrative" },
      "why": "Prose for a human, and the field with the least protection: the one place a number the sourced blocks could not supply can reappear as a sentence.",
      "value": "string"
    }
  ]
}
```

### The type mini-language

```text
string  integer  number  boolean  object      base types
enum:up|down|flat                             a closed set
const:platform_derived                        exactly one value
@sentiment                                    take the type from the ontology
  suffix []   an array          (first)
  suffix ?    nullable          (second)
```

`string[]?` is a nullable array of strings. `string?[]` is **refused** rather
than accepted as a synonym: if both spellings meant the same thing, "array of
nullables" would have no spelling at all.

### `coverage`: the question that narrows the enum

This is why the provenance enums across the corpus differ from one another,
and the one question worth asking an author explicitly.

| `coverage` | The stamp admits | Because |
|---|---|---|
| `complete` | `tool_verified`, `tool_no_match` | the tool answers, or has nothing for this subject |
| `partial` | + `unavailable_no_tool_source` | the tool answered, and *this field* has no source at all |
| `deferred` | + `pending_tool_check` | the check exists but had not run |

`genome_profiler.taxonomy` is `complete`; `genome_profiler.genome` is
`partial`. Emitting the widest set for both would have been "safe" and would
have destroyed the distinction — which is the distinction that tells a
consumer whether to go and wire up a source.

### What you deliberately cannot write

`minimum`, `maximum`, `pattern`, `format`. `src/schema_validate.rs`
implements seven keywords, and **an unsupported keyword is not a pass**: a
schema carrying one makes the whole document report
`unverified_unsupported_schema` at the delegation hop. So `{"minimum": 0}`
looks like a tightening and is a loosening — you would declare more and
verify nothing.

The parse error says so, and points the author at a `description` instead.
`equity_analyst`'s multiplier bound `[0.1, 3.0]` lives in prose for exactly
this reason, and `the_schema_uses_only_keywords_the_validator_implements`
keeps it honest.

---

## 5. Where the ontology comes in

Two roles, and they are different.

### 5a. The ontology as field vocabulary — *selection over invention*

`Ontology::field` resolves `@id` against the agent's own ontology:

| Ontology | Compiles to |
|---|---|
| `"scale": ["very_negative", … , "very_positive"]` | `enum` of five |
| `"categories": ["joy", "anger", …]` | `enum` of eight |
| `"scale": [0.0, 1.0]` | `number`, **range in the `description`** |
| `"definition": "…"` | the field's `description` |

An author writing `"enum:positive|negative"` from memory is minting a second,
subtly different vocabulary that nothing reconciles with the first. An author
writing `@sentiment` is choosing a concept the agent already reasons in, with
its closed set and its definition attached.

An unknown id is an **error**, never a silent fallback to `string`: a fallback
would let a typo become a type
(`an_unknown_entity_is_an_error_not_a_string`).

`the_real_sentiment_ontology_resolves_to_types` runs against
`ontologies/samples/sentiment_analyzer_ontology.json` on disk, so this is not
a story told with a fixture.

### 5b. The ontologist as sketch author — the division of labour

`ontologist` already reads an agent's episodic memory and extracts durable
entities and rules (Loop 1). That is the same faculty a contract needs: *what
does this agent actually talk about, in what vocabulary?*

So the composition is:

| The model does | Rust does |
|---|---|
| propose blocks, fields, vocabulary | the JSON Schema |
| draft the `why` for each block | the narrowed provenance enums |
| suggest a status and a tool | the grounding bijection, `required`, `produces` |
| — | **verify** the tool exists, and refuse if not |

A model is good at the part needing judgement and is exactly the wrong thing
to trust with a bijection over thirteen keys. So it writes the small thing and
Rust writes the large one.

Note what the model *cannot* do through this path: fabricate a `sourced`
claim. `compile` cross-checks every tool name against the agent's declared
`mcp_tools` and refuses
(`a_sourced_claim_against_a_tool_the_agent_lacks_is_refused`). The
authoring surface is safe to hand to an LLM specifically because the
dangerous claim is the one the compiler checks hardest.

The tool is `build_output_contract`, declared in `tools_legacy.rs` and
implemented in `contract_sketch::execute_build_tool` — next to the compiler,
for the same reason `validate_agent_card` lives next to the rules: an agent
working from a *description* of the expansion would drift from the expansion.

---

## 6. What was taken from `amir9480/json-schema-builder`, and what was not

The repo is a React/Vite visual builder: drag-and-drop fields, AI generation
from a prompt, form preview, sample-data generation, and export to JSON
Schema / Pydantic / Zod / cURL.

**Taken — three ideas, all load-bearing:**

1. **Author an intermediate representation, not JSON Schema.** Its field list
   (`name`, `type`, `isArray`, `required`, advanced options) is far smaller
   than the schema it compiles to. That is the whole insight, and it is the
   one that transfers: our sketch is ~150 lines of mostly prose against a
   ~390-line contract.
2. **Reusable types.** Its `$ref` registry becomes our ontology binding —
   with the twist that ours resolves against a vocabulary the agent already
   uses, rather than one defined for the schema's convenience.
3. **Generate sample data from the schema.** Its preview feature is, for us,
   the *test corpus*: the conforming and fabricated documents in
   `tests/equity_analyst_contract.rs` are exactly that, and they are what
   turns a declared contract into a checked one without spending a token.

**Not taken, and why:**

| Its feature | Why not |
|---|---|
| Drag-and-drop UI | The authoring bottleneck here is not typing structure, it is deciding grounding. A visual field builder makes the easy 15% easier and leaves the hard 85% untouched. |
| "Generate schema from a natural-language prompt" | Its input is a wish; ours must be evidence. A schema invented from a description of an agent is exactly what `port_migrate.py` measured as impossible for ~95% of this corpus. The model may draft a *sketch*, where every `sourced` claim is then cross-checked against real tools. |
| AI "refine this field" | Same reason, at field granularity, and it is where a generated `why` would creep in. |
| Pydantic / Zod / cURL export | Nothing downstream consumes them. `schema_validate` is the consumer, and it takes JSON Schema. |
| `minimum`, `pattern`, `format` support | Our validator implements seven keywords and an unsupported one is not a pass. See §4. |
| localStorage save/load | The card is the store, the sketch is the source, and a test holds them together. |

The honest summary: the builder solves *composing a JSON Schema*, which was
not the expensive part. The expensive part was the grounding map, and the fix
for that is a narrower authoring surface plus a compiler that refuses.

---

## 7. Worked example: `equity_analyst`

Chosen from the 86 grandfathered agents because it is the corpus's most
common shape and the most testable:

- **nine real data tools** (`fmp_*`), so `sourced` grounding is honest rather
  than aspirational;
- a **Fermi orchestra member**, so it is reached by `execute_agent` — the one
  call site of `envelope::build` — and its output is eventually Brier-scored;
- `produces` was four free-text labels (`evidence`, `financial-analysis`,
  `valuation`, `equity-research`), the exact `produces_resolves` failure.

| | Before | After |
|---|---|---|
| `produces` | 4 free-text labels | `["fermi/equity_evidence"]` |
| typed schema | none | 13 properties, 6 compiler-derived |
| grounding | none | 13 entries, bijective by construction |
| `TYPED_TIER_EXEMPT` | listed | removed; `BASELINE` 86 → 85 |

The contract splits **retrieval from judgement**, which is the thing the
delegation envelope was built to carry and had nothing typed to carry over:
five FMP blocks stamped from a narrowed enum, and one `assessment` block
stamped `const: "model_inference"` — so no run can present a reasoned
multiplier as a looked-up one.

Two details worth copying:

- `fundamentals` is `coverage: partial`, because free-cash-flow yield is
  absent for whole classes of filer. That is the tool answering and the field
  having no source — a third verdict, and the reason its stamp admits
  `unavailable_no_tool_source` while `profile`'s does not.
- `intrinsic_value` holds FMP's DCF and the price, and **not** the implied
  upside, though it is one subtraction away. The moment the agent computes it,
  the value stops being retrieved. It lives in `assessment`, where the stamp
  says so.

### The contract is not decoration

The system prompt was extended to ask for that exact document, and
`the_prompt_actually_asks_for_the_document_the_card_declares` asserts every
schema property appears in the prompt. Without it the schema would be checked
against prose forever, report `unverified_no_payload`, and read as "fine"
from any distance — which is the failure mode this whole line of work exists
to remove.

---

## 8. How it participates in the loop and gate infrastructure

| Surface | Mechanism | Test |
|---|---|---|
| **Admission gate** (publish) | `card_contract::validate` via `publish_pipeline`, reported to `Gate::Admission` | `the_card_would_pass_the_admission_gate`, `the_agent_no_longer_takes_the_grandfathering_discount` |
| **Delegation hop** (per composition) | `envelope::build` → `schema_validate::validate` | `a_sketch_compiled_contract_validates_at_the_hop`, `a_reasoned_block_claiming_to_be_retrieved_is_refused_at_the_hop` |
| **Declaration census** (`/api/declarations`) | `declaration_ladder::CENSUS_SQL` reads `output_contract ? 'produces_schema'` and `jsonb_typeof(schema) = 'object'` | `the_contract_satisfies_the_declaration_census_predicates` |
| **Forecast calibration** (loop 5a) | `calibration.signal = "brier_forecast"` on `assessment.multiplier_p50` | `the_contract_declares_how_it_gets_scored` |

The hop tests check both directions, because only one of them is usually
written: a conforming document reports `valid`, **and** the specific
fabrications this contract exists to stop report `invalid` rather than
`unverified`. Those are different verdicts needing different fixes, and
treating the second as a pass is the defect `envelope.rs` was written to
close.

---

## 9. Known gaps

Recorded rather than fixed, because each is a decision someone should make
deliberately.

> **Four of the original gaps are now closed.** Left as struck text rather than
> deleted, because the reasoning for each is what makes the replacement legible
> — and because a gap list that quietly loses entries is one nobody trusts.
>
> - ~~**No gate counts schema validation.**~~ `Gate::OutputSchema` exists
>   (`src/gate_trust.rs`), with the CHECK widened by migration.
> - ~~**Nothing consumes `envelope.validation`.**~~ The coordinator reads it
>   per hop (`agent_backend/tools_legacy.rs`), and `schema_conformance` writes
>   a loop4 signal — and deliberately writes *no* signal when the status is
>   `unverified_*`, so "not checked" never scores as "checked and fine".
> - ~~**The public execute path builds no envelope.**~~ All three execute
>   routes validate.
> - ~~**No sketch decompiler.**~~ `contract_sketch::sketch_from_contract`,
>   behind `/api/contracts/decompile/:id` and the Contract tab.

1. **`grounding` is validated at publish and never read at runtime.**
   `grounding_trust::enforce` resolves from the Rust const `FIELD_CONTRACTS`,
   which has no `equity_analyst` entry — so the hop checks the *shape* of its
   document and not the *sourcing* of its values.
   `a_sketch_compiled_contract_validates_at_the_hop` asserts
   `grounding_enforced: false` on purpose, so the day someone wires the
   card's `output_contract.grounding` into `enforce`, the test tells them to
   update the claim.

2. **12 of ~140 tools have a declared response shape.**
   `src/tool_response_shapes.rs` is what makes the builder's field picker a
   choice among keys that exist, and the reverse lookup (field name → which
   tools return it) gets strictly better with each one added. Absence is
   meaningful and handled — `coverage()` returns `None` for an unread tool
   rather than "covered" — but 128 tools still fall back to extracting nouns
   from a description and marking them `unconfirmed`. Adding one means reading
   its implementation; the table is deliberately hostile to guessing.

3. **The UI checks are three, and each is blind to what the others see.**
   This was the worst gap in the toolchain and is now the best-instrumented,
   so it is worth writing down what each layer can and cannot see:

   | layer | sees | blind to |
   |---|---|---|
   | `tests/inline_js_syntax.rs` | a template whose script does not parse | anything that parses and misbehaves |
   | `scripts/check_contract_builder.js` (DOM stub) | the widget's logic and rendered markup | layout, CSS, a page ever loading |
   | `scripts/check_pages_headless.js` (Chrome, CDP) | console, network, computed style, what a click shows | any page not in its list |

   The DOM stub is blind *by construction*: it answers every
   `getElementById` with an element, because otherwise the renders bail early
   and nothing is exercised. That is exactly what hid `cbSketch` reading
   `#agent-name`, an element only the wizard has — so nothing on `/contracts`
   could compile, and the only symptom was a status chip that never left
   "Compiling…". `the_contract_builder_only_reads_dom_it_owns` now pins the
   class without needing a browser.

   Remaining: only `/contracts` and `/agent/:id` are loaded. Every other page
   is unchecked, and adding one is a fixture list plus a page function.

---

## 10. Burning down the rest

<!-- Not a count. This heading said 85, then 86, then 80; the number moves
     every time anyone migrates an agent, and a stale one in a doc reads as
     authoritative. The live figure is `TYPED_TIER_EXEMPT.len()` in
     `src/workflows/agent_contract.rs`, and its `BASELINE` is enforced by a
     test that only lets it shrink. -->

Per agent:

```bash
# 1. write agents/curated/<id>/output_contract.sketch.json
# 2. compile — this refuses rather than emitting something almost right
cargo run --bin contract-sketch -- <id>

# 3. splice into the card (python preserves the card's key order)
cargo run --bin contract-sketch -- <id> > /tmp/oc.json
python3 - <<'PY'
import json, collections
p = "agents/curated/<id>/agent_card.json"
card = json.load(open(p), object_pairs_hook=collections.OrderedDict)
oc   = json.load(open("/tmp/oc.json"), object_pairs_hook=collections.OrderedDict)
card["capabilities"]["output_contract"] = oc["output_contract"]
card["produces"] = oc["produces"]
json.dump(card, open(p, "w"), indent=2, ensure_ascii=False); open(p, "a").write("\n")
PY

# 4. extend the system prompt to ask for the document
# 5. remove <id> from TYPED_TIER_EXEMPT, lower BASELINE in the same commit
# 6. verify
cargo run --bin contract-sketch -- --all --check
cargo test --test contract_sketch_corpus
```

Step 6 is why no new test file is needed per migration.
`tests/contract_sketch_corpus.rs` walks every sketch on disk and asserts it
compiles, that its card has not drifted from it, that the agent is no longer
grandfathered, and that no compiled schema can defeat the validator. Dropping
a sketch beside a card is enough to be covered.

**Ordering advice:** take agents with real data tools first. They are the ones
where `sourced` is honest, and therefore where the contract does work rather
than documenting the absence of work. An agent whose every block is
`inferred` or `unavailable` is still worth typing — `hud_field_scout`'s
`edibility` refusal is the most valuable field in that card — but it teaches
the reviewer less about whether the machinery holds.
