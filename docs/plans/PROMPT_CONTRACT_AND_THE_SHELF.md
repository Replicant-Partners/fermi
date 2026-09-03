# The prompt, the contract, and the shelf

**Written 2026-09-01, from a review of the configuration shelf.** The review was
an unstructured list of complaints; one of them turned out to reorganise the
rest, so this leads with that.

**Companion:** `docs/plans/AGENT_COMPILE_AND_TOOL_REGISTRY.md` — the compile model
(`resolved` / `error` / `pending`) and the tool registry migration. This document
is the same model applied to the one declaration nothing checks.

---

## 1. The finding: the system prompt is a load-bearing API with an undocumented vocabulary

```rust
// src/agent_backend/tool_executor.rs
pub(crate) fn prompt_demands_structured_output(prompt: &str) -> bool {
    prompt.contains("ONLY")
        || prompt.contains("raw JSON")
        || prompt.contains("Return a valid JSON")
        || prompt.contains("return a valid JSON")
        || prompt.contains("no prose outside")
        || prompt.contains("JSON object — no prose")
        || prompt.contains("output valid JSON only")
        || prompt.contains("Return JSON:")
}
```

and in `ToolAwareExecutor::execute`:

```rust
if prompt_demands_format {
    return self.inner.execute(agent, context).await;   // no tool loop
}
```

**A substring in the system prompt decides whether the agent gets tools at all.**
The function is honest about being a heuristic — *"Conservative on purpose:
matches verbatim phrases used in real curated agent cards. Adding a new
JSON-contract agent requires either reusing one of these phrases or wiring the
agent through `LLMExecutor` directly."* — and it exists for a real reason
(`docs/specs/10_RESEARCH_AGENTS_EMPTY_LLM_OUTPUT.md`: the tool loop kept
tool-using past `MAX_ITERATIONS` and returned no assistant text at all).

### Measured on the fleet

| typed contract | prompt bypasses the tool loop | agents |
|---|---|---|
| yes | **yes** | **3** |
| yes | no | 12 |
| no | yes | 18 |
| no | no | 91 |

The three: **`genome_profiler`** (68 pulses), **`supply_chain_oracle`** (83),
**`video_analyst`** (9). Each has a contract with `Sourced` fields naming tools,
and a prompt that removes the tool loop.

### And the platform's own advice does not account for it

`contract-builder.js` generates a block under the heading *"PASTE THIS INTO YOUR
SYSTEM PROMPT"*: an instruction to end every response with one JSON document in a
fence, plus five numbered rules — including *"Only fill a sourced block from that
block's own tool. If you did not call it, the block is null."*

That text contains **none of the eight trigger phrases** (`contains` is
case-sensitive; the block's "Only" is not "ONLY"). So:

* paste it and you keep the tool loop — which is what a sourced contract needs;
* but a prompt that *already* says "output valid JSON only" loses the tool loop
  while its contract demands tools, and pasting the block does not undo that;
* and nothing on any surface tells you which state you are in.

**This is not an ergonomics gap. It is a correctness surface with no display.**

## 2. The thesis

Asked *"what is the first thing I need to understand when I configure an agent?"*,
the answer from the review was: **the system prompt, and how it relates to the
contract.** That is right, and the finding above makes it structural rather than
pedagogical.

Today the prompt is a textarea inside the *Brain* panel, below the model ladder,
and its relationship to the contract exists only as a **copy-paste ritual**. The
platform knows what the prompt should contain, generates it, and asks a human to
transcribe it — which is the defect this project has named repeatedly: *if the
platform can name what would close a gap, the name is the control.*

## 3. The rest of the review, organised

Six groups. The numbers are the review's own points, kept so nothing is lost.

### 3.1 The prompt–contract relationship — §1, §2 above
The prompt is buried; the relationship is invisible; a substring changes
execution; nothing checks agreement.

### 3.2 The shelf's information architecture
* *"the text up top is just documentation… it's a wall of text"* — the panel
  notes were cut and **the rungs' own prose is now the wall**: `unlocks` and
  `without_it`, two to four lines each, four rungs, at the top of the shelf.
  Per-row documentation, which is the thing "explain once" forbids, rebuilt by
  the same hand that removed it.
* *"perhaps this shelf needs tab structure?"* — yes, and **not by field group.**
  Tabs by the questions an author asks in order:

  | tab | question |
  |---|---|
  | Prompt | what does it say, and does that match what it promises |
  | Contract | what can it be trusted about |
  | Runs on | model ladder, policy, cost |
  | Who it is | identity, personality, reach |
  | Costs | spend, credits |
  | History | versions |

### 3.3 The contract builder's empty state
* white input fields against the dark shelf — a theme mismatch;
* the add-a-part flow is awkward;
* *"much better in genome_profiler because it's got a contract already"* — the
  builder is an **editor asked to be a creator.** Empty is the state it handles
  worst and the state every new agent starts in.

### 3.4 The model ladder is a viewer, not a policy surface
Six review points collapse into one: it shows what is configured and gives no
basis for deciding. Missing: which providers are available, which models,
what each costs, what capability each implies, and any way to declare policy
(fallback, refusal, per-tier overrides).

Also: **`capability_gates` is a terminology collision.** On this platform a
*gate* is a checkpoint that can refuse an artifact — `gate_decisions`,
`gate_trust`, the belt. `capability_gates` is a different concept and the name
should change before it is surfaced, or the shelf will teach the wrong meaning of
the platform's most load-bearing noun.

### 3.5 Missing outright
* **Learned things.** Record shows counts. There is no embedding space and no way
  to explore what was learned. (See also: the dream loop extracts nothing for
  most agents because 75% of episodes have no embedding —
  `AGENT_COMPILE_AND_TOOL_REGISTRY.md` §7.)
* **MCP, in and out.** `mcp_servers` (what the agent consumes) and
  `published-tools` (what it exposes) are both writable and neither is in the
  shelf. And the review's own observation, which should be settled first:
  *the output contract already defines what the agent publishes, and every agent
  has an MCP endpoint for that contract* — so `published-tools` may be redundant
  with the contract rather than complementary to it.
* **Version history.** `agent_versions` carries `version_number`, `changed_by`,
  `system_prompt`, `model`, `temperature`. Nothing renders it. This matters more
  once §1 is understood: if the prompt is load-bearing, its history is an audit
  trail rather than a curiosity.

### 3.6 Composition and correctness — the deepest one
> *the strategist agent should be able to combine specialist agents based on
> their patterns*

Which means **view 3 of the contract is asking the wrong actor.** "How a
coordinator combines members" is an authoring-time declaration of a runtime
decision. A contract should describe what is *composable*; the strategist reads
patterns and composes. Declaring the composition in the contract freezes a choice
that the platform is meant to make well.

And *"how correctness is eventually measured"* offers `hitl_review` with **no
LLM-as-judge**, despite the Observatory having a `Judge` control. The one enum
that decides whether an agent is falsifiable at all is missing its cheapest
option — and §213 of `FEEDBACK_LOOPS.md` already mandates the coherence gate for
LLM-judged signals, so the mechanism for accepting such a verdict exists.

## 4. First move: the prompt panel

**One panel, first in the shelf, with three checkable facts between the prompt
and the contract.** All three are computable today with no new backend.

| fact | computed from | state |
|---|---|---|
| **Will this agent get tools?** | `prompt_demands_structured_output(system_prompt)` | a reading, not a fault |
| **Does that contradict the contract?** | bypasses tools **AND** the contract has `Sourced` fields | **error** — the contract requires a tool the executor has removed |
| **Does the prompt name the type it must produce?** | `system_prompt.contains(produces_schema)` | **error** if typed and absent |

Rules this must obey, all of them already paid for elsewhere:

1. **One implementation.** The detector's patterns must not be copied into
   JavaScript. Expose the Rust function and serve the result — a second copy of a
   decision is the drift this repo keeps finding.
2. **Name the matched phrase.** "Your prompt removes the tool loop" is a
   verdict; *"…because it contains `output valid JSON only`"* is actionable.
3. **Insert, do not instruct.** The platform generates the rules block. Offering
   it as a control rather than as text to transcribe is the difference between a
   workbench and a manual.
4. **Absent is not bad.** An agent with no contract cannot contradict one, and an
   agent with no prompt is unconfigured rather than broken.

### Not in the first move, deliberately
Rewriting the prompt automatically. The platform can say *what disagrees* and can
*offer* its own text; choosing the words is the author's, and a system prompt
edited by a machine on a trust surface is the thing the coherence gate exists to
prevent.

## 5. Order after that

1. **Tabs**, per §3.2 — once the prompt panel exists there is a first tab worth
   opening onto.
2. **The rungs' prose**, per §3.2 — `unlocks` / `without_it` behind a per-rung
   disclosure, with the shared sentence said once.
3. **The ladder as a policy surface**, per §3.4, including renaming
   `capability_gates`.
4. **The builder's empty state**, per §3.3 — the create path.
5. **Version history**, per §3.5 — cheap, and newly meaningful.
6. **Composition and the judge**, per §3.6 — needs a decision first, not code.

## 6. The measurement to keep

Three typed agents bypass their own tool loop. That number should go to zero, and
it should go to zero by somebody *deciding* what those agents should do — not by
the check being relaxed. `supply_chain_oracle` has 83 pulses behind it.
