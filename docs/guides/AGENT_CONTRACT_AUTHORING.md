# Authoring an agent contract

**Audience:** anyone publishing an agent to ABW, human or agent.
**Enforced by:** `src/card_contract.rs`, at publish. Not advice — a gate.
**Check your work:** `scripts/port_migrate.py --propose <agent_id>`, or ask
`xaman_ek` to validate a draft.

> ## Do not write this by hand
>
> Read the next two sections so you know what the parts *mean*, then write a
> **sketch** and compile it. You declare the three things that need
> judgement — the blocks, their fields, and where each block's value comes
> from plus why — and `src/contract_sketch.rs` emits the JSON Schema, the
> narrowed `<block>_provenance` enums, the grounding map and the rewritten
> `produces`.
>
> ```bash
> cargo run --bin contract-sketch -- <agent_id>     # or the build_output_contract tool
> ```
>
> Two reasons this is not just convenience:
>
> - Schema and `grounding` are emitted from **one traversal of one block
>   list**, so the bijection between them — the rule below that costs the most
>   to satisfy by hand — cannot be violated. It is unrepresentable rather
>   than merely checked.
> - The compiler runs `card_contract::validate` over its own output and
>   **refuses to emit anything that would not publish**. There is no state in
>   which you are holding something that looks finished and is not.
>
> Worked example: `agents/curated/equity_analyst/output_contract.sketch.json`
> — six authored blocks, thirteen emitted properties. Design notes and the
> migration recipe: `docs/DESIGN_typed_output_contracts.md`.
>
> The one field the compiler will never write for you is `why`, because its
> subject is where *your agent's* data comes from. See §"Choosing a
> `status`".

---

## What you must declare, and why

Three things, in `capabilities.output_contract`:

| | | |
|---|---|---|
| `produces_schema` | a namespaced type name | so another agent can match on **identity**, not on a string that happens to look familiar |
| `schema` | a JSON Schema for the document you return | so the name resolves to something, and your output can be validated |
| `grounding` | one entry per top-level field, saying where its value comes from | so a value nobody could have looked up cannot be served as if someone had |

Miss any one and publish is refused, with a message naming what to add.

### Why this exists

An agent in this catalogue was asked for genome size, chromosome count,
divergence date and IUCN status. It had two tools, both returning taxonomy.
Three of its four output blocks had no possible source.

It filled them in — confidently, for 56 episodes, with values like
`"200-400"` megabases for a species nobody has sequenced. Thirteen of those
documents were cached and shown to users.

Nothing caught it. The fields were present, the JSON parsed, the types
checked, the ports were non-empty. **Every check reasoned about shape; the
failure was in content.** This contract is the check that reasons about
content, and `grounding` is the part that does the work.

---

## The shape

```json
"output_contract": {
  "domain": "phylogenetics",
  "produces_schema": "rabble/phylogenetic_profile",

  "schema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "rabble/phylogenetic_profile",
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "taxonomy": { "type": ["object", "null"] },
      "genome":   { "type": ["object", "null"] },
      "summary":  { "type": "string" }
    }
  },

  "grounding": {
    "taxonomy": {
      "status": "sourced",
      "tool": "gbif_taxonomy_tree",
      "response_field": "hierarchy (kingdom..species)",
      "why": "GBIF returns the full rank ladder with stable keys for the queried name."
    },
    "genome": {
      "status": "unavailable",
      "why": "No genome database is wired up. NCBI Assembly would be needed, and most insects are unsequenced even then."
    },
    "summary": {
      "status": "narrative",
      "why": "Prose over what was retrieved; must not assert anything the sourced blocks cannot support."
    }
  }
}
```

And your ports reference the type rather than describing it:

```json
"produces": ["rabble/phylogenetic_profile"]
```

---

## Choosing a `status`

Four values. The set is closed — `"estimated"` is not available, because an
estimate presented in a data field is the problem this contract exists to
stop.

### `sourced` — a tool returns it

```json
{ "status": "sourced", "tool": "gbif_taxonomy_tree",
  "response_field": "hierarchy", "why": "…" }
```

The named tool **must be one your agent declares**. This is the check with
teeth: marking a field `sourced` against a tool you cannot call is the
original defect restated inside the mechanism built to catch it, so it is
rejected and the error lists the tools you *do* have.

`response_field` names the part of the response that supplies it, so the
claim is checkable against the tool's actual output rather than taken on
trust.

### `inferred` — a judgement you are asked to make

```json
{ "status": "inferred", "from": "taxonomy, size differential, proximity",
  "why": "…" }
```

**This is not a lesser status.** A threat-assessment agent rating predation
risk is doing its job; no database holds that rating. The distinction that
matters is **retrieval versus judgement**:

> A genome size is a fact sitting in a source you did not query.
> A threat level is not in any source. Producing it is the work.

If every field looked like a fabrication, this contract would be
indistinguishable from a broken checker and would rightly be switched off.
Use `inferred` freely and honestly, and name what you reason *from*.

### `narrative` — prose

```json
{ "status": "narrative", "why": "…" }
```

Permitted, and **checked**. Prose is scanned for claims the sourced blocks
cannot support, because clearing a fabricated number from a structured field
while leaving it in the summary just moves it into the sentence a human
actually reads. `parse_evidence_text` lifts your summary out as the
episode's evidence, so it is the most-read string you produce.

### `unavailable` — nothing can supply it

```json
{ "status": "unavailable", "why": "…" }
```

The field will be **forced to null** at runtime and the block stamped
`unavailable_no_tool_source`. Any attempt to populate it is recorded as a
`grounding` anomaly.

This is the honest answer, not a failure. Declaring `unavailable` is how a
gap becomes visible instead of becoming an invention.

---

## Rules that trip people up

**Every `why` needs 40+ characters.** Not bureaucracy: the next author cannot
tell a considered `unavailable` from a lazy one, so they copy whichever is
nearest. Two of the entries in our own Rust table were caught by this rule.

**A name is not a schema.** Seven curated cards declared `produces_schema`
and no `schema`, and the observatory rendered the *name* under a heading
saying "Schema". A name is a contract only once something can resolve it.

**Namespace your type.** `summary` will collide; `acme/summary` will not.

**`grounding` and `schema` must agree exactly.** Every schema property needs
a disposition, and a disposition for a field that does not exist protects
nothing. Both directions are checked.

**Fix everything in one pass.** The validator returns *all* findings, not
the first, so you are not playing whack-a-mole with a gate.

---

## If your agent returns prose

Some agents genuinely should. `anomaly_triager` is told to "surface a triage
summary" in a stated communication style — it narrates, it does not emit a
document. Its four `produces` labels name things it *mentions*.

For those, the honest outcome is to stay untyped and be **explicitly
non-composable**. Do not invent a schema to clear a gate; a fabricated type
is worse than an absent one, because it invites other agents to compose
against it.

If you *want* it typed, that is authoring work: decide the document shape,
write it into the prompt, then re-run the proposer. Your prompt very likely
already contains the vocabulary — `anomaly_triager`'s carries a full
severity ladder (`L0`–`L3`), event types and action enumerations. What no
label can tell you is the **structure**: whether a triage plan is one
document with four sections or four separate outputs. That is a decision,
and only you can make it.

---

## Migrating an existing agent

```sh
scripts/port_migrate.py --triage                  # where every agent sits
scripts/port_migrate.py --propose <agent_id>      # draft + evidence
```

The proposer emits a draft annotated with the evidence behind every value,
and marks the rest `NEEDS_AUTHOR`. **Those markers deliberately fail
validation** — a draft cannot be pasted in and published. A migration tool
whose output passes the gate it migrates toward is a fabrication engine with
good manners.

It refuses because the evidence usually is not there. Measured over the 100
curated cards:

```
accepts  labels  330 | match a declared tool input:        18   (5%)
produces labels  339 | match a key in a prompt JSON shape: 17   (5%)
cards with any parseable output shape at all:              25 / 100
```

For ~95% of labels there is nothing in the card corroborating them.
Generating schemas from labels would invent the type system for almost the
whole corpus, and it would look entirely convincing — a plausible schema is
exactly as well-formed as a true one.

**Evidence, in descending order of trust:**

| Source | Trust | Availability |
|---|---|---|
| `episodes.response_text` — what the agent actually returned | highest | accruing since migration 199 (#34) |
| a JSON example in the prompt | high | 25 cards |
| a declared tool's `input_schema` | high, for `accepts` | 5% of accept labels |
| an enumeration stated in the prompt | real, but unattached to a field | common |
| the port label | **none** | everywhere |

---

## Existing agents

86 agents predate this contract and are grandfathered in
`agent_contract::TYPED_TIER_EXEMPT`. They keep working; a hard cutover would
have failed the next republish of essentially every agent in the catalogue,
and the gate would have been switched off within a week.

**The list may only shrink** — a test pins its length. Removing your agent
from it is the migration.

**Anything created from now on is not on that list** and gets the full
contract. That is what "enforcement by default" means here: new work is held
to the standard immediately; old work burns down on a ratchet.
