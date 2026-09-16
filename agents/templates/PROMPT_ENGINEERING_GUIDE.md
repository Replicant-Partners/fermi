# Prompt Engineering Guide for Agent Development

Fill-in-the-bracket prompts for an AI assistant helping you author an agent.
Copy a block, fill the brackets, paste. Every prompt below is calibrated to the
gates that actually run — not to the card shape as it was documented.

> **Last reconciled with:** `src/agent_backend/agent_card.rs`,
> `src/card_contract.rs`, `src/contract_sketch.rs`, `src/grounding_trust.rs`,
> `src/agent_backend/tool_executor.rs`
> **Date:** 2026-09-12

## Why this was rewritten

The previous version was reconciled on 2026-05-13, before the typed output
contract, the sketch compiler and runtime grounding enforcement existed. It
taught three things that now fail:

| Old advice | What happens now |
|---|---|
| "system_prompt must specify the exact JSON output structure" (prompts #1, #2, #7) | If the wording lands on one of eight substrings, `ToolAwareExecutor` removes **every tool** and the agent answers from memory while appearing to have searched. See trap 1. |
| "accepts and produces must be specific typed strings" | `produces` must reference the declared schema type. A free-text label is a `produces_resolves` finding at publish (`card_contract.rs`). |
| "valence must be filled deliberately" — shown as a top-level block | `valence` deserializes at `metadata.valence` only (`AgentMetadata`). A top-level block is a legacy shape nothing reads. |

It also taught authors to hand-write an output contract, which is now the
wrong unit of work: you write a **sketch** and compile it.

If you followed the old guide, the two things to check first are your
`system_prompt` (trap 1) and your `produces` array (§7).

---

## Read this before you write a prompt

Three traps. Each has caused a measured production failure, each is silent, and
each is something an assistant will walk you straight into if you do not tell
it otherwise.

### Trap 1 — eight substrings that delete every tool

`src/agent_backend/tool_executor.rs` lines 29–86. If the system prompt
`contains` any of `STRUCTURED_OUTPUT_PATTERNS`:

```
"ONLY"   "raw JSON"   "Return a valid JSON"   "return a valid JSON"
"no prose outside"   "JSON object — no prose"   "output valid JSON only"
"Return JSON:"
```

…then `ToolAwareExecutor` skips the tool loop and delegates to the inner
executor, which sends `tools: None`. The agent runs single-shot with **zero
tools**, and does not fail loudly. `genome_profiler`'s card said *"report where
it sits in the tree of life USING ONLY DATA YOUR TOOLS RETURN"* — a sentence
whose plain meaning is the opposite of what it did, because of the substring
`ONLY`. 12 of its first 68 production pulses wrote `<function_calls>` blocks
into their own prose, wrote "Based on the tool responses:", and continued with a
rank ladder from memory. The documents were correctly shaped, the nulls were in
the right places, and the grounding gate passed them — the gate checks whether a
`Sourced` field is populated, not whether the tool it names was ever reached.

**What this does and does not forbid.** Not the document shape: a typed agent's
prompt *must* name its type, or `specimen_handler`'s
`prompt_check.names_its_type` reports `does not name its type` as an error and
nothing tells the agent what to emit. What is forbidden is the *phrasings*. The
contract builder's generated wording is trigger-free by construction and
pinned by `the_generated_prompt_block_keeps_the_tool_loop`; the strictly safer
route, where a handler drives the agent, is to put the shape in Rust beside the
parser that reads it and pass it in the query
(`src/handlers/workspace/claim_evaluation.rs::output_shape`). Prompt 3 carries
the exact wording to use.

Build-time guards:
`grounding_trust::tests::sourced_field_contracts_must_not_pair_with_no_tool_loop_prompts`
and `tool_executor::trigger_tests::no_typed_card_removes_the_tool_loop_its_own_contract_needs`.
Both carry shrink-only `KNOWN` lists (`video_analyst`, `supply_chain_oracle`),
so a new agent fails immediately.

### Trap 2 — do not hand-write the output contract

`capabilities.output_contract` needs four things, and publish is refused by
`src/card_contract.rs::validate` if any is missing:

| | |
|---|---|
| `domain` | human-readable, e.g. `equity-research` |
| `produces_schema` | a **namespaced** type name — `summary` collides, `acme/summary` does not |
| `schema` | a JSON Schema for the document you return. A name without a schema is `output_contract_typed`: seven cards declared one and the observatory rendered the *name* under a heading saying "Schema" |
| `grounding` | one entry per top-level schema property, both directions checked |

Write `agents/curated/<id>/output_contract.sketch.json` and compile:

```bash
cargo run --bin contract-sketch -- <agent_id>       # or the build_output_contract tool
```

There is deliberately no `--write`: `serde_json::Map` is a `BTreeMap`, so
serialising a card through it alphabetises every key and turns a twelve-line
contract change into a whole-file diff. The splice recipe is a `python3`
snippet in the header of `scripts/contract_sketch.rs` that preserves your key
order.

Two reasons this is not just convenience, both stated in
`src/contract_sketch.rs`: the compiler emits `schema.properties` and
`grounding` from **one traversal of one block list**, so the bijection between
them — the check that scales with field count and the one hand-authors fail
most — is unrepresentable rather than merely checked; and it runs
`card_contract::validate` over its own output and refuses to emit anything that
would not publish, so `compile() == Ok` ⟹ the Admission gate passes
(`contract_compiles_to_something_the_gate_accepts`).

The sketch is the source of truth:
`tests/contract_sketch_corpus.rs::every_sketch_compiles_and_the_card_matches_it`
fails if the card drifts from it. Seventeen sketches exist today. Worked
examples worth reading first:

| Sketch | Why |
|---|---|
| `agents/curated/equity_analyst/output_contract.sketch.json` | five sourced blocks and one judgement block → 12 properties, 12 grounding entries, 5 narrowed enums |
| `agents/curated/regulatory_lens_translator/output_contract.sketch.json` | a dispatcher agent, mostly `inferred` blocks |
| `agents/curated/carbon_accountant/output_contract.sketch.json` | newest; its architecture note explains why a `Derived` tier lives *inside* a `sourced` block |

### Trap 3 — a declared tool with no dispatch arm

`no_curated_card_declares_a_phantom_tool` (`src/agent_backend/weather_tools.rs`,
~line 3805) separates two questions that get conflated:

- **dispatchable** — is there a match arm in `ToolRegistry::execute`? If not,
  the model is advertised the tool, calls it, and receives `Unknown tool: X`,
  which reads to an operator as the model misbehaving rather than the platform
  lying about its capabilities. `moe_router_strategist`, `debate_strategist`
  and `vote_strategist` all declared `get_agent_calibration` with no arm, and
  Loop 5's router silently could not read calibration.
- **declarable** — is it in `builtin_tools()`? `equity_analyst`'s nine `fmp_*`
  tools run perfectly (`tools_legacy.rs::execute_fmp_api`) but were never
  registered as defs, so the card cannot be re-saved through the API.

The test carries a shrink-only `known_debt` list of 92 pre-existing phantom
declarations. **Do not add to it.** Add the arm, or remove the declaration.

---

## 1. Generate an agent card from a concept

```
I'm designing an agent for the Agent Bestiary platform. Help me produce a
complete agent_card.json.

Agent concept: [2–3 sentence description of what this agent does]
Domain: [e.g. equity research, product carbon, regulatory translation]
Executor: [llm | mcp | manual | skill]
Where its data comes from: [named tools | web_search | LLM reasoning only]

The live card shape (src/agent_backend/agent_card.rs):

Top-level: agent_id, agent_type, version, tier, capabilities, accepts,
produces, dependencies, system_prompt, prompt_template, requires_secrets,
workflow_template, metadata, wallet, performance, usage, ontology_stats

capabilities: executor, provider, model, temperature, min_tier, model_ladder
(array of {tier, provider, model, note, params?}), capability_gates, model_params
(max_tokens, temperature, random_seed at minimum), mcp_tools (objects with a
name, never bare strings), skills, output_contract, input_contract

metadata: created, author, description, tags, sample_queries, valence

Hard constraints — each is a gate, not a preference:

1. `produces` is Vec<String>: an ARRAY of schema identities. A map of
   {name: description} makes the card fail to deserialize, and
   AgentRegistry::load_from_directory handles that by printing a warning to
   stderr and SKIPPING the agent — so an auto-hired agent can simply not
   exist at runtime.
2. `valence` goes at `metadata.valence`, with AgentValence's four fields
   (primary_affect, arousal, valence, personality_traits). A top-level
   `valence` block deserializes into nothing.
3. `wallet` is required, and `metadata.description` must not start with
   "Agent: " — the generated placeholder, which is worse than no description
   because it looks filled in
   (agent_card.rs::test_all_cards_have_card_specific_fields).
4. tags, `accepts`, `produces`, `metadata.sample_queries` and
   `metadata.valence` are enforced by test_all_cards_satisfy_agent_contract
   via the shared `workflows::agent_contract` — the same requirement set the
   API publish gate applies.
5. Do NOT write `capabilities.output_contract` by hand; leave it out and
   leave `produces` a placeholder. I will author a sketch and compile it,
   and the compiler writes the type into `produces` (prompt 2).
6. Do NOT put the output document shape, or any JSON-contract instruction,
   into the system_prompt yet. Eight substrings in
   tool_executor::STRUCTURED_OUTPUT_PATTERNS silently remove every tool from
   this agent. Prompt 3 handles the prompt.
7. Declare no tool without a dispatch arm in ToolRegistry::execute.
8. `requires_secrets` is Vec<SecretRequirement> — objects with {name, label,
   description, is_required}, never bare strings. And for a CURATED agent,
   omit it entirely: resolve_agent_owner_secrets returns None for the curated
   and system tiers by design, so a declared secret prompts a user for a key
   the platform will never read. A platform credential belongs in the
   credential store and is checked by the handler's preflight instead — see
   claim_evaluation::resolve_search_credential. `["brave_search"]` on a
   curated card is both errors at once: it fails to deserialize, so
   load_from_directory SKIPS the whole agent (constraint 1), and it would
   have asked for the wrong thing if it had parsed.
9. performance, usage and ontology_stats are zero/empty (system-managed);
   `min_tier` and `capability_gates` are documentation only — nothing
   compares against them, and the model that runs comes from the DB
   `agents.model` column via `resolve_agent_card`.

Reference template: agents/templates/agent_card.json
```

---

## 2. Author the output-contract sketch

Do this **second**, before the persona. The document the agent returns decides
what the prompt has to ask for; the reverse order produces a prompt full of
fields nothing can source.

```
Help me write agents/curated/[agent_id]/output_contract.sketch.json for the
ABW contract compiler (src/contract_sketch.rs). Do not write JSON Schema —
write the sketch, and I will compile it.

Agent: [id and one-sentence description]
Tools it declares: [exact tool names from capabilities.mcp_tools]
What each tool returns: [per tool: the response fields that matter]
What a caller wants back: [the blocks, in plain English]

Sketch shape:
  domain              human-readable, e.g. "product-carbon"
  produces_schema     namespaced type name; becomes $id and the produces entry
  title, description  optional prose
  synthesis           how a coordinator combines members' documents (optional)
  calibration         passed through verbatim (optional)
  blocks[]            the work

Each block has: name, source, why, and EITHER `fields` (object block) or
`value` (single-valued block). `required` defaults to true, `description` is
optional. A block name may not end in `_provenance` — the compiler owns that
namespace and writes one sibling stamp per block.

Type grammar for fields (src/contract_sketch.rs lines 117–250):
  string | integer | number | boolean | object | null
  enum:a|b|c (two values minimum) | const:x
  suffixes `[]` then `?`, in that order. `string[]?` is a nullable array;
  `?[]` is rejected, because letting both orders mean one thing makes the
  other type unwritable.

Two things to get right:
  * `null` is NOT nullable. `string?` says "a value or nothing"; `null` pins
    the field to null forever, and is the field-level form of `unavailable`.
  * An array of objects is `object[]` with a `description` ENUMERATING the
    keys — the mini-language does not type the inner shape. See
    carbon_accountant's `inventory.items`.

Do NOT ask for `minimum`, `pattern` or `format`. Only keywords
`src/schema_validate.rs` implements are emitted, and one unsupported keyword
makes every document from this agent report `unverified_unsupported_schema` at
the delegation hop — not a pass, and strictly worse than declaring less. Put
the constraint in a `description`, where a reader gets it and the validator is
not asked to lie about it.
(tests/contract_sketch_corpus.rs::no_compiled_schema_can_defeat_the_validator)

For each block give me `source` (prompt 5 covers choosing a status) and a `why`
of 40+ characters (card_contract::MIN_WHY). The `why` is the one field the
compiler will NEVER generate, because its subject is where THIS AGENT's data
comes from. Write it as if the next author will copy whichever nearby `why` is
nearest — because they will.

Model it on agents/curated/equity_analyst/output_contract.sketch.json.
```

Then: `cargo run --bin contract-sketch -- <id>`, splice with the `python3`
snippet in `scripts/contract_sketch.rs`, and keep the sketch beside the card.
The compiler ADDS the declared type at the front of `produces` and removes
nothing (`Compiled::merge_produces`) — `produces` doubles as the port-label
match surface, and an earlier version that replaced the column deleted six of
`football_analyst`'s labels, which other agents match on. For a *new* agent,
keep `produces` to exactly the declared type: `card_contract`'s
`produces_resolves` check requires every entry to equal `produces_schema`, and
you have no legacy labels to protect.

---

## 3. Design the persona and valence

```
I'm writing the system_prompt and metadata.valence for an ABW agent.

Agent: [name and one-sentence description]
Domain: [what it works with]
Tools it can call: [exact names]
Declared output type: [produces_schema from the compiled contract]
Blocks and their grounding statuses: [paste the sketch's blocks + statuses]

Produce:

1. system_prompt. It must:
   - Name the agent in the first sentence and state its specific role
   - Tell it to CALL ITS TOOLS, by name, and that they are real server-side
     function calls. Add: "Never write a tool call out as text — a
     <function_calls> block typed into your reply is a sentence, and nothing
     comes back from it." genome_profiler did exactly that in 12 of its first
     68 runs and narrated retrievals that never happened.
   - State, per block, what IS and IS NOT sourceable by the tools it has, and
     that every unsourceable field must be null — not estimated, not a
     typical range, not hedged with "~" or "typically". An unverifiable value
     in a data field is worse than an absent one: a reader cannot tell it
     from a measurement.
   - Name the declared type verbatim. prompt_check.names_its_type reports
     "does not name its type" as an ERROR when the prompt never mentions
     produces_schema, and a prompt naming no type is how a correctly-shaped
     hallucination gets written.
   - Say the prose summary is not an exception: a number moved out of a null
     field into a sentence is still a fabrication, and parse_evidence_text
     lifts the summary out as the episode's evidence, the most-read string
     the agent produces.
   - Define scope boundaries, and be behavioural enough to serve as a persona
     baseline the drift monitor can measure against

   HARD CONSTRAINT on wording: the prompt must contain none of these
   substrings, or tool_executor::prompt_demands_structured_output removes
   EVERY tool from this agent and it answers from memory while appearing to
   have searched. The match is case-sensitive substring containment.

     "ONLY", "raw JSON", "Return a valid JSON", "return a valid JSON",
     "no prose outside", "JSON object — no prose",
     "output valid JSON only", "Return JSON:"

   Write the document instruction as: "End every response with one JSON
   document in a ```json fence, conforming exactly to type
   <produces_schema>." Express the meaning of "only" as "using no data other
   than what your tools return". If a handler drives this agent, put the
   shape in the handler and pass it in the query — see
   src/handlers/workspace/claim_evaluation.rs::output_shape.

2. metadata.valence (AgentValence's four fields, at that path):
   primary_affect [alignment | curious | vigilant | analytical | diplomatic |
   integrative]; arousal 0.0 calm → 1.0 urgent; valence 0.0 critical → 1.0
   affirming; personality_traits, 2–4 adjectives.

   Justify each value by how this agent should behave inside a composition —
   anchor, challenger, or synthesizer. A generic 0.5/0.5 is a decision
   deferred. `has_valence` is a publish requirement because the affective
   signature drives the valence-diversity check that stops a composition
   becoming an echo chamber.

Length: 150–400 words for a toolless agent; longer is normal for a typed
tool-using one — see agents/curated/genome_profiler/agent_card.json.
```

---

## 4. Design the model ladder and the gates you actually face

```
I'm configuring the cognition economy for an ABW agent.

Agent: [name and description]
Task complexity: [simple classification | analytical reasoning | frontier reasoning]
Budget sensitivity: [cost-sensitive | balanced | quality-first]

Design:

1. model_ladder — ordered array of {tier, provider, model, note, params?}:
   - At minimum one 'free' rung; optionally 'standard' and 'premium'
   - Justify each model choice and any per-rung params override
   - Providers: anthropic | mistral | openrouter | qwen | glm

2. min_tier and capability_gates — and say out loud in your answer that
   these are DOCUMENTATION. src/command_registry.rs records them as "typed,
   persisted, exposed, and never compared against anything". The model that
   runs comes from the DB agents.model column via resolve_agent_card, which
   also overrides the card's provider, temperature and system_prompt. Fill
   them if they document intent; never design a safety property on them.

3. model_params:
   - max_tokens sized for the typed document, not for the prose
   - temperature 0.0–0.3 for retrieval and classification, 0.4–0.7 for
     analysis. Temperature is a collaboration knob, not a creativity dial:
     low temperature is what makes an agent-to-agent interface stable.
   - random_seed: 42, so repeated eval runs on the same input are comparable
   - extended_thinking only for Anthropic multi-step reasoning

4. Then list the gates this agent must clear, and which my draft fails:
   - agent_contract::contract_violations — tags, sample_queries, accepts,
     produces, valence, description, persona
   - agent_contract::typed_tier_violations → card_contract::validate — the
     full typed contract. New agents are NOT on TYPED_TIER_EXEMPT (78
     grandfathered legacy names today, down from 86; the list may only shrink
     and a test pins its length), so anything created now gets all of it.
   - no_curated_card_declares_a_phantom_tool, and the structured-output
     trigger check from prompt 3

Both requirement sets are shared with the API publish path on purpose:
`run_publish_checks` and the on-disk tests used to encode "well-formed"
separately, which is how community agents reached the public catalogue with
no sample_queries and no valence.
```

---

## 5. Grounding: choose a status per block

```
Help me choose a grounding status for each block of my output contract, and
the coverage for each sourced one. This is the part of a sketch that decides
what the runtime will let the agent say.

Agent: [id]
Tools it declares: [exact names] — and what each returns, field by field
Blocks: [names + what each holds]
Platform code that computes anything here: [file::function, or none]

The status set is CLOSED (card_contract::GROUNDING_STATUSES):

  sourced      A declared tool returns it. Needs `tool` + `response_field` +
               `coverage`. The tool name is cross-checked against
               capabilities.mcp_tools — marking a field sourced against a
               tool the agent cannot call is the original defect restated
               inside the mechanism built to catch it, and is rejected.
  inferred     A judgement the agent is COMMISSIONED to make. Needs `from`.
               NOT a lesser status: a threat level or a GHG scope allocation
               is in no source, and producing it is the work. If every field
               looked like a fabrication the contract would be
               indistinguishable from a broken checker and would be switched
               off. Use it freely and name what you reason from.
  narrative    Prose. Permitted and scanned (prompt 6). Gets no provenance
               sibling — a stamp on prose is a retrieval claim about a
               sentence.
  unavailable  Nothing can supply it, so it is forced to null at runtime and
               stamped unavailable_no_tool_source. Add `would_need`: it turns
               a null into a to-do. The honest answer, not a failure.

There is no `estimated`, because an estimate presented in a data field is the
problem this contract exists to stop. There is also no `derived` authoring
token (card_contract::PLATFORM_ASSIGNED_ONLY): `platform_derived` asserts the
PLATFORM computed a value reproducibly, which an agent's author cannot claim
about the agent's own output, so the runtime assigns it in
grounding_trust::enforce. Never declare a stamp the runtime will not write for
that block — schema_validate::the_pilot_agents_declared_schema_validates_its_own_output
documents a fixture asserting phylogeny_provenance: "platform_derived" on a
block with a Sourced field. The card's schema declared it too, so card and
schema agreed with each other, neither agreed with the platform, and the test
stayed green the whole time.

COVERAGE is the part authors skip, and it is the question that narrows the
provenance enum:

  complete          → [tool_verified, tool_no_match]
  partial           → + unavailable_no_tool_source
  deferred          → + pending_tool_check
  partial_deferred  → both

`partial` means part of the block has no source at all; `deferred` means the
check exists but may not have run when the document was built. "Never asked"
and "asked, nothing exists" need different fixes, which is why they are
different verdicts — get this wrong and you either overstate coverage or make
an honest gap read as a failed lookup. `partial_deferred` is rare and usually
means the block wants splitting; football_analyst.advanced_metrics needs it
only because it is a live document shape with consumers.

For each block, tell me: status, tool and response_field if sourced, the
coverage and WHY that coverage, and a `why` of 40+ characters whose subject is
where this agent's data comes from. Background: docs/guides/AGENT_CONTRACT_AUTHORING.md.
```

---

## 6. The Rust-side contract: per-field grounding, cross-checks, leak needles

Most agents need only the sketch. Reach for `src/grounding_trust.rs` when a
block is **mixed** (sourced as a whole, with individual fields that have no
source), or when you need `Derived`.

```
Help me write the src/grounding_trust.rs entries for [agent_id].

Blocks and statuses from my sketch: [paste]
Fields within a sourced block that have NO source: [list]
Values platform code computes: [file::function per value, or none]
The agent's narrative field(s): [names]
Its domain vocabulary: [authority names, units, dataset names and provision
formats a reader would take as proof of retrieval]

What goes where, and why:

* PRECEDENCE. FIELD_CONTRACTS is per-FIELD and WINS over the card's per-BLOCK
  grounding map — enforce_from_output_contract checks for Rust entries first.
  That is the opposite of what it looks like it should be: the card map cannot
  express a mixed block, so preferring it LOSES enforcement. Measured on
  genome_profiler the moment its card gained a grounding map: ploidy
  "diploid", notable_genes, divergence_mya 45.0 and defining_traits all
  survived the hop, and `conservation` was replaced wholesale by null, which
  its own schema forbids. So write Rust entries only for per-field granularity
  or Derived, and know it turns the card map off for this agent. The card path
  also skips NARRATIVE_LEAKS entirely (TODO in enforce_from_grounding_map).

* EVERY Sourced field needs a `cross_check_sql` or a CROSS_CHECK_EXEMPTIONS
  entry whose reason is 60+ characters and says why not AND what would fix it
  (every_sourced_field_is_verifiable_or_admits_it_is_not). The claim is
  deliberately weak and therefore keepable: not that every sourced field is
  verified, but that none is SILENTLY unverified. It is the answer to "why did
  the verification system miss a bush-cricket reported as a beetle" — Sourced
  asserted a tool COULD supply the value and nothing ever compared it to
  anything.

* EVERY Derived field must be computed by us or checked by us, never merely
  asserted (every_derived_field_is_computed_or_checked): a DERIVATIONS entry,
  a cross_check_sql, or a DERIVED_ELSEWHERE entry naming the file that keeps
  the promise. phylogeny.superorder declared Derived, its transform
  (ncbi_tools::superorder_of) was written and unit-tested, and NOTHING EVER
  CALLED IT — the field was null in every document the agent ever produced,
  and Derived outranks Inferred in strength, so the unkept promise scored
  higher than an honest judgement.

* An agent with Sourced blocks AND a Narrative field must bring its own
  NARRATIVE_LEAKS needles (narrative_leak_coverage_only_shrinks; do NOT extend
  UNPOLICED_PROSE). Needles are agent-scoped (agent, block, rule) and matched
  against a LOWERCASED haystack, so an uppercase needle can never fire. Two
  rules, both learned the hard way:
    1. Choose for DISTINCTIVENESS, not coverage. A plain " gb" needle matched
       "GBIF", so an honest taxonomy summary citing its own source was
       reported as leaking a genome size — hence LeakRule::Quantity for a unit
       that only implies a claim when a number precedes it, Word for a
       distinctive word. "gras" is absent from regulatory_lens_translator's
       set (substring of "grassroots"); "scope 2" is absent from
       carbon_accountant's because allocating a line to a scope needs no
       factor, while "scope 3" is present because the claims people write
       about it are magnitude claims.
    2. NEVER add a needle that can fire on honest output. Before the table was
       agent-scoped, "divergence" — a `phylogeny` needle belonging to
       genome_profiler — nulled regulatory_lens_translator's central prose,
       whose whole subject is how a claim diverges across markets, and
       "vulnerable" did the same via `conservation` while "vulnerable consumer
       groups" is ordinary food-labelling language. A check that fires on
       correct output gets switched off, and the switching-off looks like
       cleanup.

* An agent with FIELD_CONTRACTS entries MUST have an
  output_contract.sketch.json
  (agents_with_field_contracts_must_have_output_contract_sketches), or the
  promise is invisible to the typing system and the specimen page reports the
  agent as compiling vacuously — zero declared fields, zero errors, "compiles".

Give me the FieldContract entries, the cross-check SQL or exemption reasons,
and the leak needles, with a one-line justification each.
```

`enforce_from_output_contract` is called by `envelope::build` at **every
delegation hop**: it nulls ungrounded fields, stamps `<block>_provenance`, and
scans narrative blocks for leaks. It runs *before* schema validation, so a
field pinned to `null` is cleaned before it is checked rather than the agent
being blamed for a null the platform was about to write.

---

## 7. The identity contract: accepts, produces, dependencies

```
Define accepts, produces, dependencies and (optionally) input_contract for an
ABW agent.

Agent: [name and description]
Declared output type: [produces_schema]
What it takes in: [inputs in plain English]
Other agents it works with: [known collaborators and their produces]

1. `produces` — an ARRAY of schema identities, not a map, not free text.
   card_contract::validate's `produces_resolves` check requires every entry
   to equal the declared `produces_schema`, so for a new agent it is exactly
   one element. equity_analyst's four free-text labels (`evidence`,
   `financial-analysis`, `valuation`, `equity-research`) were the exact
   produces_resolves failure its migration fixed.

   The live tension, so you are not surprised: a recompile ADDS the type and
   removes nothing (contract_sketch::Compiled::merge_produces), because
   `agents.produces` is also the port-label match surface and replacing it
   deleted six of football_analyst's labels. Migrated legacy cards therefore
   carry both. Do not create that state on a new agent.

2. `accepts` — typed input labels. To have them enforced, write
   agents/curated/<id>/input_contract.sketch.json and compile it with
   `cargo run --bin input-contract-sketch`. It needs no grounding map — there
   is no provenance claim to make about where the CALLER sourced their data.
   envelope::validate_input checks a caller's query against it before
   dispatch and is never fatal; absence means `unverified_no_schema`, which
   must never fold into Approved. Worked example:
   agents/curated/equity_analyst/input_contract.sketch.json.

3. dependencies.required / .optional — agent_ids that must, or may, exist.
   If either is non-empty, declare `execute_agent` or `delegate_to_agent`
   (test_compound_agents_have_execute_agent_tool, which carries a shrink-only
   list of pre-existing gaps; new cards must not join it).

The composition planner, the eval framework and xaman_ek all read these.
`produces` in particular is how a downstream agent matches on identity rather
than on a string that happens to look familiar.
```

---

## 8. Design the ontology

```
I'm designing the ontology for an ABW agent.

Agent: [name and description]
Domain concepts it tracks: [5–10 nouns the agent reasons about]
Key relationships: [how those concepts connect]

Produce a Mermaid erDiagram with:
- 5–15 core entities (5–10 for a new agent)
- Correct cardinality (||--||, ||--o{, }o--||, }o--o{)
- Relevant attributes per entity (id PK, typed fields, timestamps, scores)
- Relationship labels as verbs

Design principles:
- Entities should emerge from the agent's actual query/response transcripts.
  If the agent never mentions an entity, the dreaming worker will never
  consolidate it. Start simple; the worker extends this from episodes.
- Normalize; each entity needs at least an id PK and a name string

Then produce the same vocabulary as an `ontology.json` with an `entities`
array, each entity having `id`, `name` and `properties` (holding
`definition`, and `scale` or `categories` where the value set is closed).
contract_sketch::Ontology reads that form, so a sketch field can say
`"@sentiment"` and take the type, the closed value set and the definition
from the entity. The point is selection over invention: an author typing
"enum:positive|negative" from memory is minting a second, slightly different
vocabulary that nothing reconciles with the first. A numeric `scale` resolves
to `number` with the RANGE IN THE DESCRIPTION, never minimum/maximum, for the
reason in prompt 2. An unknown @id resolves to None rather than falling back
to string, so a typo cannot become a type.

Validate the Mermaid at https://mermaid.live/. Reference shapes:
ontologies/samples/*.json.
```

---

## 9. Generate sample queries for eval

```
Generate 5 sample_queries for an ABW agent.

Agent: [name and description]
Tools: [names]
Declared output type and blocks: [paste]

Rules:
- These become the default eval test cases for the observability stack's
  EvaluatorRegistry, and `has_sample_queries` is a publish requirement:
  without one, nobody can tell what to ask this agent.
- Each query must be answerable by this agent alone
- Cover a range: simple → complex → edge case
- Be specific. Not "What is market share?" but "What is AMD's Q1 2026
  datacenter GPU market share, and how has it trended over four quarters?"
- Include at least one query whose honest answer is a NULL or an
  `unavailable_no_tool_source` stamp — a subject the agent's tools provably
  do not cover. That is the case that distinguishes an agent obeying its
  contract from one filling gaps from memory, and it is the one nobody
  writes. genome_profiler shipped 56 episodes before anyone asked it about a
  species nobody has sequenced.
- Include at least one that exercises a `partial` or `deferred` coverage
  path, if the contract declares one.

Format: numbered list of 5, each followed by a one-line note on what
capability it tests and what the honest document looks like.
```

---

## 10. Design a compound agent / composition

```
I'm designing a compound agent (composition) for the ABW platform.

Goal: [what this composition accomplishes]
Member agents available: [agent_ids and their produces_schema values]

Design:

1. workflow_template: a `mermaid` graph TD of the stage flow, `stages` as an
   array of {name, agent, accepts, produces, description}, and a
   `description` of what is orchestrated end-to-end.

2. dependencies.required / .optional, plus execute_agent or
   delegate_to_agent in mcp_tools. `accepts` and `produces` for the compound
   agent as a whole — its external interface, not the internal stage I/O.
   The coordinator's `produces` is its own declared type, same rule as any
   other agent.

3. The seam check, which is the whole value of typing a composition: does
   the coordinator read only fields its members DECLARE? weather_oracle plus
   its three members was the first fully-typed composition and
   tests/weather_composition.rs asserts exactly that — a property no single
   card could have. Tell me, per stage, which leaf the next stage lifts out
   by name, so renaming one is a visible break rather than a silent null.

4. A strategist. The four that exist on disk: pipeline_strategist
   (sequential deterministic stages), debate_strategist (opposing positions
   plus judge), vote_strategist (N-of-M consensus), moe_router_strategist
   (input classifier routes to a specialist). cohere_and_coordinate and
   coherence_consultant handle discourse coherence; there is no
   `coherence_strategist` agent.

5. rsi_modes. `cascade` is implemented. `tune_team` is SPECIFIED and NOT
   BUILT — `grep tune_team --include=*.rs` returns nothing, nothing measures
   whether a decomposition was good and nothing proposes a better one
   (docs/fermi/BUILDING_FERMI_TEAMS.md §7). If you declare it, mark it
   aspirational in the card's note. The mitigation available today is to
   ship the template as one candidate structure, keep a second variant, and
   Brier-compare them by hand.

6. A halted pipeline must be a VALID document. Model the stage blocks so a
   coordinator can say where the chain stopped instead of returning a
   confident answer built on a stage that never ran: null the stages that
   did not run and let their stamps say pending_tool_check. Halting
   correctly is the system working.

Reference: docs/COMPOSITION_AS_FIRST_CLASS.md (note it predates the typed
tier — read the strategist patterns, not the contract shape).
```

---

## 11. Improve an agent based on observatory data

```
Help me improve an agent using observability data.

Agent: [name and current system_prompt]
Current data:
  - Eval dimensions: [/api/observatory/agents/:id/timeline]
  - Anomalies, last 30 days: [/api/observatory/agents/:id/anomalies — drift |
    rolling_conflict | rupture | safety | grounding], and trend direction
  - HITL actions: [approve | relabel | intervene, and what they corrected]
  - Specimen prompt_check: [gets_tools, trigger, sourced_fields,
    contradicts_contract, names_its_type]
  - Grounding violations per block: [which fields are being nulled]

Suggest, in this order:

1. Does prompt_check show `contradicts_contract: true`? If so nothing else
   matters: the contract names tools for N fields and the prompt has removed
   the loop that could call them, so those fields can only come from memory.
   Fix the wording first and name the exact trigger substring.
2. Grounding violations are a CONTRACT question before a prompt question. A
   field nulled every run means either the prompt asks for something no tool
   can supply — regrade the block to `unavailable` or `inferred` — or the
   tool is not being reached. Do not "improve" a prompt into filling a field
   that has no source.
3. system_prompt: what is it failing to SPECIFY that causes the dimension
   drops?
4. valence: is arousal or affect contributing to the dyad ruptures?
5. model_ladder: is the model right for the observed complexity? (min_tier
   and capability_gates change nothing at runtime.)
6. sample_queries: which query types expose weaknesses the current set does
   not cover?

Address the specific anomaly patterns, not generic improvements. If a leak
rule is firing on output I believe is honest, say so — that is a needle to
remove or narrow, not an agent to correct, and a check that fires on correct
output will be switched off.
```

Prompt 11 is only worth running against real data. Run the agent, read the
observatory and the specimen page, then come back.

---

## 12. Generate an agent README

```
Write a README.md for an ABW agent.

Agent card (key fields): [agent_id, agent_type, metadata.description,
accepts, produces, sample_queries, metadata.valence, mcp_tools]
Output contract: [paste capabilities.output_contract]
Sketch: [paste output_contract.sketch.json]
Ontology summary: [entity count, key entities, key relationships]

Sections:

1. Overview — 2–3 sentences: what it does, for whom, in what context
2. Capabilities — the declared type and its blocks in plain English
3. The contract — a table of block → grounding status → tool (if sourced) →
   the provenance stamps that block can hold. This is the section a would-be
   composer actually needs: which of my fields are retrievals and which are
   judgements.
4. Sample queries — each with an example document showing REALISTIC values:
   nulls where nulls belong, confidence not always 0.95, and at least one
   with an `unavailable_no_tool_source` or `tool_no_match` stamp.
5. Ontology — entity summary, link to ontology.mermaid
6. Performance targets — accuracy, avg confidence, response time, cost/run
7. Known limitations — specific and honest. Name every `unavailable` block
   and what would have to be wired up to change it; that is what the
   sketch's `would_need` is for.
8. Observability notes — which anomaly events this agent is likely to
   generate and how to read them

Length: 200–400 lines. Technical, direct, no marketing language.
```

---

## Tips for better results

**Contract first, prompt second.** The document decides what the prompt has to
ask for. The other order produces a prompt full of fields nothing can source,
which is exactly the shape `genome_profiler` shipped: four blocks requested,
two tools, both returning taxonomy.

**Paste the live constraints into every card prompt.** Prompt #1's constraint
list is the part worth copying verbatim — an assistant working from a
remembered card shape will give you a `produces` map, a top-level `valence`,
and a system prompt containing `"Return a valid JSON"`.

**Check the trigger before you check anything else.** After any prompt edit on
a tool-using agent, run `structured_output_trigger` over it; the specimen page
prints which pattern matched and the whole set. A prompt that reads as stricter
about grounding and has silently removed every tool is worse than the one you
started with.

**`compile() == Ok` is the only reliable signal that a contract will publish.**
Not a review, not a reading of `card_contract.rs`. The compiler validates its
own output and refuses to emit anything the gate would refuse.

**Prefer declaring less.** `unavailable` with a good `would_need` is a
publishable, honest state that turns a gap into a to-do. An unsupported schema
keyword, an unbacked `Derived`, and a `sourced` field naming a tool the agent
cannot call are all worse than declaring nothing at that field: each converts
an absence into a false claim, which is what every gate in this stack exists to
catch.

**If your agent genuinely returns prose, stay untyped.** `anomaly_triager`
narrates a triage summary in a stated communication style; its `produces`
labels name things it *mentions*. The honest outcome is to stay untyped and be
explicitly non-composable. Do not invent a schema to clear a gate — a
fabricated type is worse than an absent one, because it invites other agents to
compose against it. Typing it later is real authoring work, and your prompt
likely already carries the vocabulary (`anomaly_triager` has a full `L0`–`L3`
severity ladder). What no label can tell you is the *structure*: whether a
triage plan is one document with four sections or four separate outputs. Only
you can decide that.

**Read the gate's own words.** `card_contract::validate` returns *all* findings
rather than the first, and each message says what to add. You are not playing
whack-a-mole with it. Set `random_seed` in `model_params` while you are there,
so repeated eval runs on the same input are comparable and a regression is
visible.
