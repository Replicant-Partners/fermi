# Captured: the tool declaration gap

**Status: steps 1 and 2 done. The rest is deliberately blocked on the tool
registry refactor**, because those are decisions about what the registry *is*,
and making them twice would be worse than making them late.

| | | |
|---|---|---|
| ✅ | reconciliation test | `tests/tool_declaration_reconciliation.rs` — a tool named by `FIELD_CONTRACTS` must be declared on the card. Was 2 failures; now 0. |
| ✅ | the two stale cards | `football_analyst` gained `call_football_api`; `weather_oracle` gained its four. |
| ✅ | heuristic report | `scripts/tool_declaration_report.py` — prints both checks. Exits 0 always. |
| ⏸ | does `mcp_tools` grant? | §0, §2 — the registry refactor decides |
| ⏸ | generate the card list from `FIELD_CONTRACTS`? | §0.1 |
| ⏸ | should the trace's per-field reconciliation emit a gate decision? | the prize — see the ordering section |
| ⏸ | `football_analyst` has no grounding map | §3 — an ordinary migration, not blocked, not yet done |

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

---

## 0.1 Three places declare which tool supplies a field. They disagree.

Added after the trace view for episode `386a6248` was pointed out: it prints
`call_football_api` beside seven rows, lists the seven calls the run actually
made with their endpoints and byte counts, and grades each contracted field
`never asked` / `asked, empty` / `tool unused`. **The trace knows the tool. The
card and the builder do not.**

So this is not missing knowledge. It is three sources of the same fact, only
one of which is wired to the gate — and it is the wrong one.

| # | source | says | read by | correct? |
|---|---|---|---|---|
| 1 | `agent_card.json` → `capabilities.mcp_tools` | `execute_agent` | contract builder, publish gate, `invalid_tool_declarations` | **no** |
| 2 | `grounding_trust::FIELD_CONTRACTS` (Rust const) | `league_context` ← `call_football_api`, and 5 more paths | trace view, `field_probe::declared_tool`, hop enforcement | **yes** |
| 3 | the episode record (`tool_calls`) | 7 real calls: `standings`, `teams/statistics` ×2, `players` ×2, `injuries`, `players/topscorers` | trace view (`TOOL_CALLS`) | **ground truth** |

The trace **already reconciles 2 against 3**, per field, per episode. That is
what produces `never asked · call_football_api would close 4 of them` and
`asked, empty`. Nothing reconciles either of them against 1.

### Mechanical reconciliation — **now enforced**

`tests/tool_declaration_reconciliation.rs` reads `FIELD_CONTRACTS` through the
compiler (not a regex over the source — a regex would quietly find nothing if
the formatting changed, and pass by having no work to do) and asserts that every
tool a field contract names is declared on that agent's card. It also asserts
the named tool actually dispatches, which nothing checked before: the trace
offers a `run` button for a contracted tool, so a phantom name there is an
affordance that cannot work.

At the time of writing it failed on **2 of the 9** agents with tool-sourced
entries. Both are now fixed; the failure it produced is recorded here because
it is the thing the test exists to keep catching:

```
football_analyst   contract says  call_football_api
                   card declares  execute_agent

weather_oracle     contract says  polymarket_orderbook, weather_climatology,
                                  weather_dispersion_fit, weather_ensemble_forecast
                   card declares  execute_agent, polymarket_weather_markets,
                                  weather_settlement_spec
```

`weather_oracle` is the uncomfortable one: it is part of the fully-typed
weather composition, migrated deliberately and covered by
`tests/weather_composition.rs`. Being freshly typed did not prevent the
divergence, because nothing compared the two declarations.

The added entries carry `{name, description}` and **no `input_schema`**. The
registry owns the schema, 212 of the 352 existing `mcp_tools` entries already
omit it, and a second copy is a second thing to drift — which is the whole
subject of this document.

### Why this makes the fix cheaper than §0 implies

The declaration does not need to be *authored*. It needs to be *derived and
reconciled*: source 2 already holds the per-field mapping, and source 3 can
falsify it against what actually ran.

And the reconciliation test is **safe to write before the registry refactor**,
because it does not depend on what `mcp_tools` is decided to mean. "This agent
calls `call_football_api`" is true under every candidate semantics — grant,
documentation, or deleted-in-favour-of-registry-class. Only the *consequence*
of the declaration is undecided, not its truth.

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

**Revised after §0.1, and the revision matters.** My first answer framed this as
an epistemic gap: the platform cannot tell what a tool really supplied, so it
checks a declaration instead. That was wrong. The platform *can* tell, does
tell, and shows it on the trace screen — per field, per episode, graded `never
asked` / `asked, empty` / `tool unused` against the actual call log.

So the honest diagnosis is not "the gate checks a weak thing because a strong
thing is unavailable." It is:

> **The strong check exists and runs. It is rendered for human review and is not
> what the gate reads.** The gate reads a third declaration that nothing keeps
> honest.

That is a better problem to have — it is wiring, not epistemics — but it is
also a sharper criticism, because the capability is already paid for.

It also revises `DESIGN_typed_output_contracts.md` §9 item 1, which says
`grounding` is "validated at publish and never read at runtime". Runtime
per-field grounding verification is implemented; it lives in the trace view and
feeds a human settle form. Whether any of it reaches `gate_decisions` I have
**not** verified and should not be assumed.

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

### The ordering this implies

Trusted declarations are a **prerequisite**, not a nice-to-have. A gate over a
declaration nothing keeps honest is a gate over an opinion, and every downstream
verdict inherits that. So:

1. ~~**Now, safe under any registry semantics**~~ — **done.** The reconciliation
   test, and the two card fixes it found.
2. ~~**Now, as a report rather than a gate**~~ — **done.**
   `scripts/tool_declaration_report.py`. Left ungated on purpose, and running it
   shows why: `xaman_ek` accounts for 59 of the hits because it is the navigator
   agent and its prompt *describes* the tool catalogue. A ratchet over that would
   have to grow an exemption list on its first day. The other 21 rows mostly look
   like real omissions (`web_search` is the common one), but "looks like" is the
   correct strength of claim for a regex over prose.
3. **After the refactor** — whether `mcp_tools` grants; whether the card's
   declaration should be *generated* from `FIELD_CONTRACTS` rather than
   maintained beside it; whether the trace's per-field reconciliation should
   emit a gate decision instead of only a review row.

Step 3 is where the real prize is. Steps 1 and 2 exist so that the ground under
step 3 is not still moving when it is attempted.

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
