# Captured: the tool declaration gap

**Status: recorded, not fixed. Deliberately blocked on the tool registry
refactor**, because three of the four issues below are decisions about what the
registry *is*, and making them twice would be worse than making them late.

Found while loading `football_analyst` in the contract builder. Nothing here is
a builder bug — the builder reported all of it accurately. It is what the
report turned out to mean.

---

## 0. The root finding

**`capabilities.mcp_tools` is descriptive, not restrictive. It does not grant,
and it does not withhold.**

`ToolRegistry::to_claude_tools_with_card_and_remote` (`tools/registry.rs:125`)
builds the tool list the model sees as:

```rust
let mut tools: Vec<ClaudeTool> = self
    .tools
    .values()
    .filter(|t| t.is_llm_visible())      // EVERY builtin in this registry class
    .map(...)
    .collect();

for mcp in &card.capabilities.mcp_tools {
    if let Some(ref schema) = mcp.input_schema {
        if claimed.insert(mcp.name.clone()) {   // only ADDS what is not already there
            tools.push(...)
        }
    }
}
```

The registry is constructed by capability class — `ToolRegistry::standard()`,
`ToolRegistry::with_workspace()` — never per agent card. So every agent gets
every LLM-visible builtin for its class, and `mcp_tools` can only *add* tools
the registry does not already have (and only those carrying an `input_schema`).

This is not necessarily wrong. It may well be the intended design. What is
wrong is that four other things read it as if it were a grant.

### How it surfaced

`football_analyst` declares exactly one tool:

```json
"capabilities": { "mcp_tools": [{ "name": "execute_agent" }] }
```

Its 4,000-word system prompt instructs it to call `call_football_api` — with a
full API-Football v3 endpoint reference and a `DATA PROVENANCE` section reading
*"`call_football_api` is a pass-through to API-Football v3 and nothing else is
wired."* The tool is real and dispatchable (`tools_legacy.rs:2056`).

**218 runs, 99.1% success.** The tool was genuinely called. Nothing was
fabricated on this account — the runtime simply never consulted the
declaration.

---

## 1. Prompt and declaration disagree on 22 of 101 curated cards

Not a `football_analyst` quirk. A scan of every curated card for builtin names
(length > 6, to avoid false hits) appearing in `system_prompt` but not in
`capabilities.mcp_tools`:

```
cards whose prompt names an undeclared builtin: 22
  … prompt: ['web_search']                                      declares: ['execute_agent']
  … prompt: ['execute_agent','list_workspace_agents','read_workspace_file']  declares: []
  … prompt: ['edit_image','write_workspace_file']                declares: []
  … prompt: ['search_knowledge']                                 declares: ['check_conflicts', …]
```

13 of 101 cards declare **no** tools at all while their prompts name several.

Nothing detects this. `invalid_tool_declarations`
(`tools_legacy.rs:266`) checks the *other* direction only — that declared names
exist in the registry, so there are no phantom tools. There is no check that a
tool the prompt tells the model to call is one the card admits to.

**Decision to make:** is `mcp_tools` (a) the authoritative grant, (b) a
documentation field, or (c) to be deleted in favour of the registry class? Each
gives a different fix, and the registry refactor is when the answer becomes
cheap.

---

## 2. The contract cross-check states something false

The builder tells an author:

> A part marked `sourced` must name one of these — a field sourced from a tool
> the agent cannot call is the exact defect this contract exists to catch.

The agent **can** call it. Given §0, this sentence is untrue as written, and
the check behind it is wrong in both directions:

- **Over-strict.** A `sourced` block on `football_analyst` naming
  `call_football_api` — a true statement about where the value comes from —
  would be rejected, because the card does not list the tool.
- **No protection.** Declaring a tool does not mean it was the one used, and
  omitting one does not prevent its use. The check cannot catch what it claims
  to catch.

This one is mine and it is the most misleading, because it is phrased as a
safety property.

**Decision:** either `mcp_tools` becomes enforcing (and the sentence becomes
true), or the cross-check is re-grounded on `platform_tool_names()` — "a tool
that exists" rather than "a tool this agent declared" — and the sentence is
rewritten to claim only that.

---

## 3. `football_analyst`'s contract has no grounding map

`capabilities.output_contract` has keys `domain`, `produces_schema`, `schema`,
`calibration`. **No `grounding`.** It is one of the three cards typed by hand
before the sketch compiler existed.

So `sketch_from_contract` can only recover a block's *kind* from the narrowed
provenance enum (`source_from_stamp`), and returns `tool: ""` — the stamp says a
tool was involved but not which one. All ten blocks come back owing a `why`,
because `why` lives in the grounding map and the compiler refuses to invent
one. The builder's "10 without a why / unfalsifiable" is correct.

The schema is real and enforced; the *sourcing* of its values is undeclared.
`grounding_trust::enforce` resolves from the Rust const `FIELD_CONTRACTS`, so
for this agent the hop checks document shape and not value provenance.

**Not blocked on the refactor.** This is a migration: write
`agents/curated/football_analyst/output_contract.sketch.json`, per
`DESIGN_typed_output_contracts.md` §10. It is a good early candidate — real
data tool, 218 runs, an existing prompt that already names what is retrievable
and what must be null.

---

## 4. `call_football_api` offers no fields to pick, and that is the honest fallback

Typing it into the Tool box produced no field picker, only:

> No declared response shape for `call_football_api` — nobody has read it.
> Field names here are unchecked, and the `response_field` claim cannot be
> verified against the tool's actual output.

Correct behaviour: `tool_response_shapes::TOOL_RESPONSES` declares **12** tools.
A crude scan of `tools_legacy.rs` finds **91** builtin `name:` literals
(`platform_tool_names()` is authoritative — and note the "~140" I wrote into
`DESIGN_typed_output_contracts.md` §9 is unverified and probably wrong; it
should be corrected to the real figure).

So ~87% of tools fall back to extracting nouns from a description and marking
them `unconfirmed`. The system is honest about it — `coverage()` returns `None`
for an unread tool, never "covered" — but the affordance that makes the builder
worth using is missing for almost every tool.

**Tie-in to the refactor, and the reason this is worth waiting for:** if the
registry gains a uniform response-construction path, the response shape can
plausibly move onto the tool definition itself, and a registry-walk test can
require every LLM-visible tool to declare a shape or be explicitly exempted —
the ratchet this codebase already uses for `TYPED_TIER_EXEMPT`, `KNOWN_SILENT`
and the swallowed-write baselines. That converts a 12/91 backlog from a chore
nobody owns into a thing you cannot add a tool without doing.

The module doc in `tool_response_shapes.rs` argues for a side table partly on
the grounds that `BuiltinToolDef` literals spell out every field, so adding one
would touch a hundred definitions in a file two sessions are editing. **If the
refactor fixes that, half of that argument expires.** The other half stands and
should be decided deliberately: this is contract-authoring metadata, not
dispatch metadata, and putting it on the dispatch struct invites the assumption
that `ToolRegistry::execute` validates against it — which it does not, and
should not be quietly taken to.

---

## Is the gates model flimsy because of this?

No — but it is **narrower than it reads**, and this finding is a clean example
of the difference.

**What holds.** The platform is consistent where it matters most: `unverified`
never scores as a pass. `unverified_*` maps to `Decision::Undetermined`;
`schema_conformance` writes *no* loop4 signal rather than a neutral one;
`tool_response_shapes::coverage()` returns `None` for an unread tool rather than
an empty "covered" list; the builder says "nobody has read it" instead of
offering a guess. That discipline is the load-bearing thing, it is rare, and
nothing here dents it.

**What does not hold.** A gate over a declaration is only as strong as the
runtime's respect for that declaration. `invalid_tool_declarations` validates
`mcp_tools` against the registry and *reads* like tool access control. It is
not: it checks that a declared name is spellable, not that an undeclared tool is
unreachable. That is a gate measuring **intent**, presented in the vocabulary of
**constraint**.

The failure mode to name precisely is not "we did not check." It is **"we
checked something adjacent, and the name of the check implies the stronger
thing."** That is more dangerous than an absent check, because an absent check
prompts a question and a misnamed one closes it.

Which is the same shape as the bug that started all of this: `genome_profiler`
put a recalled genome size in the same field as a retrieved one. Not a missing
value — an indistinguishable one. Here: not a missing gate — an
indistinguishable one.

So the honest statement of the platform's current position is narrower than the
one the UI currently makes:

> A declared schema is validated at every hop. Which tool supplied a value is
> declared by the author and **not verified against what the agent actually
> called**, and the tool list on a card does not constrain what the agent can
> call.

If that sentence is unacceptable, the fix is to make `mcp_tools` enforcing. If
it is acceptable, the fix is to stop implying otherwise. Either is fine. Doing
neither is what makes it flimsy.

---

## What is NOT broken

Worth stating, so this reads as scoped rather than as alarm:

- **No fabrication is implied.** `football_analyst` really did call the API. The
  declaration was ignored, not the tool.
- **The builder is correct throughout.** Every element of that screen —
  `execute_agent` in Declared tools, the empty tool names, "10 without a why",
  "no declared response shape", `unfalsifiable` — was an accurate report. The
  typed-contract work is what surfaced all of this; that is the system working.
- **Schema validation is real** and now runs on all three execute routes with
  `Gate::OutputSchema` behind it.
- **This is 22 cards out of 101, on one field.** It is a systemic gap in one
  declaration, not a systemic failure of declaration.

---

## Also captured while here (cosmetic, same session)

- **`specimen.html` mounts the contract builder without `class="cb-standalone"`
  on `<body>`.** Every input rule in `contract-builder.css` is scoped to that
  class, and `components.css` gives `.form-group` only a `margin-bottom`. So on
  `/specimen/:id` every input, select and textarea falls through to browser
  defaults — the white boxes. Same class as the standalone page that forgot
  `common.css`, one level down: a widget whose styling depends on the host
  setting something, mounted by a host that did not.
  `scripts/check_pages_headless.js` missed it because `/specimen/:id` is not in
  its page list.
- **The Response field placeholder is `priceToEarningsRatio, priceToBookRatio`**
  — hardcoded from the `equity_analyst` worked example, so a football contract
  prompts you with equity field names. Reads as a value rather than a
  placeholder precisely because the field is unstyled white.
- **`#cb-ontology` is a raw-JSON textarea** in the middle of an otherwise
  structured form — the one control that asks the author to hand-write the
  thing every other control exists to generate.
