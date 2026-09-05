# Parallel Execution and Coordination Topologies

**Written 2026-09-05. Entry point for the fan-out parallelism + topology work.**

---

## Current state

`coordination_graph.rs` implements level-based execution that CORRECTLY handles
fan-out/fan-in semantics but executes nodes within each level **sequentially**.

For a pipeline: correct and efficient (one node per level).
For MoE fan-out: correct (all nodes get the same input, outputs synthesised) but
the three analysts run one after the other instead of in parallel.

---

## What needs to be built

### 1. True parallel execution within a level

**The change:** Replace the sequential node loop within a level with concurrent dispatch.

**The constraint:** `execute_node` takes `&ToolContext` which is not `'static`, so
`tokio::spawn` (which requires `'static`) can't be used directly.

**Solution options:**

**Option A: `Arc<ToolContext>` (preferred)**
Change `execute_node` signature to `Arc<ToolContext>` and clone for each parallel task.
`ToolContext` fields that are `Arc<_>` (memory_store, registry, embedder, etc.) are
already cheap to clone — the Arc clone is just a reference count increment.

Implementation:
```rust
// In execute_coordination_graph:
let ctx = Arc::new(ctx_ref.clone()); // ToolContext derives Clone

let tasks: Vec<_> = level.iter().map(|node| {
    let ctx = Arc::clone(&ctx);
    let input = node_input.clone();
    let node = node.clone(); // CoordinationNode derives Clone
    tokio::spawn(async move { execute_node(&node, &input, &ctx).await })
}).collect();

let steps: Vec<TraceStep> = futures::future::join_all(tasks)
    .await
    .into_iter()
    .filter_map(|r| r.ok())
    .collect();
```

**Prerequisite:** `ToolContext` must implement `Clone`. Check what fields don't derive
it (they'll need `Arc<>` wrapping or manual Clone).

**Option B: Sequential with `futures::future::join_all` (simpler)**
Use the `futures` crate (add to Cargo.toml) to join non-`'static` futures:
```rust
use futures::future::join_all;

let futures: Vec<_> = level.iter()
    .map(|node| execute_node(node, &input, ctx))
    .collect();
let steps = join_all(futures).await;
```
This doesn't require `ToolContext: Clone` and doesn't spawn OS threads.
The tradeoff: it's concurrent within Tokio's cooperative multitasking, not truly
OS-parallel. For I/O-bound agent execution (network calls to LLMs) this is equivalent
to true parallelism.

**Recommended: Option B first.** It requires only adding `futures = "0.3"` to Cargo.toml
(or using `futures-util` which may already be transitively available). Minimal code change,
correct semantics, no Clone requirement.

**File:** `src/agent_backend/coordination_graph.rs`  
**Change:** The level execution loop (~line 140–165 in the current file)

---

### 2. Additional coordination topologies

#### Debate topology
```text
entry → proposer ─┐
                   ├── judge → verdict
entry → opposer  ─┘
```

The proposer and opposer both receive the same entry input and produce arguments.
The judge receives both arguments (as synthesised input) and produces the verdict.

**Graph template:**
```json
{
  "synthesis": "selection",
  "nodes": [
    {"id": "proposer", "agent": null, "input_schema": "abw/debate-position/1"},
    {"id": "opposer",  "agent": null, "input_schema": "abw/debate-position/1"},
    {"id": "judge",    "agent": null, "input_schema": "abw/debate-arguments/1"}
  ],
  "edges": [
    {"from": "proposer", "to": "judge", "schema": "abw/debate-position/1"},
    {"from": "opposer",  "to": "judge", "schema": "abw/debate-position/1"}
  ]
}
```

Proposer and opposer are at level 0 (no incoming edges from each other).
Judge is at level 1 (depends on both). The fan-in synthesis at judge combines
the two positions into the arguments it needs to evaluate.

**New input_contract needed:** `debate_strategist` → `abw/debate-request/1`

#### Vote topology
```text
         ┌─ voter_1 ─┐
entry ───┼─ voter_2 ─┼──► aggregation
         └─ voter_3 ─┘
```

Identical to MoE fan-out with `synthesis: "aggregation"`. No topology changes needed —
the existing fan-out implementation handles this. What's missing is the vote strategist
updating its workflow_template to use the typed nodes format.

#### Cascade/pipeline with typed seams
```text
fetch → analyse → synthesise
```

Already works. The key addition is typed schema IDs at each seam:
```json
"edges": [
  {"from": "fetch",   "to": "analyse",   "schema": "abw/raw-evidence/1"},
  {"from": "analyse", "to": "synthesise", "schema": "abw/analysis/1"}
]
```

**Pipeline seam validation** (already in pipeline_strategist's prompt):
- SCHEMA MATCH: `output_schema == next_node.input_schema` — verified typed compatibility
- LABEL MATCH: labels match but no schema — author assertion
- UNMATCHED: no correspondence
- OPEN SLOT: no agent bound

---

### 3. Synthesis protocol completeness

Current implementation in `synthesise_outputs()`:

| Protocol | Implemented | Notes |
|---|---|---|
| `selection` | Partially | Returns `{candidates: [...]}` — the LLM picks. Phase 2: platform picks by gate status |
| `aggregation` | ✓ | Returns `{members: [...]}` |
| `cep_weighted` | ✓ | Same shape as aggregation; weighting upstream |
| `max_risk` | ✓ | Picks by risk/severity field |
| `pipeline` | ✓ | Sequential; last output |

**Missing: platform-side `selection` synthesis**

Currently `selection` returns all candidates and lets the LLM choose. The platform should
rank them by gate outcome (prefer `valid` > `unverified_*` > `invalid`) and return the
best single output:

```rust
"selection" => {
    // Prefer valid gate output over unverified over invalid
    let ranked = outputs.iter()
        .map(|o| {
            let gate = o.get("gate_output").and_then(|v| v.as_str()).unwrap_or("");
            let score = match gate { "valid" => 2, "invalid" => 0, _ => 1 };
            (score, o)
        })
        .max_by_key(|(s, _)| *s);
    ranked.map(|(_, o)| o.clone())
}
```

This makes selection a first-class platform operation rather than delegating to the LLM.

---

## Implementation order

1. **Option B parallel execution** — add `futures = "0.3"` to Cargo.toml, replace the
   sequential level loop with `join_all`. 1-2 hours.

2. **Platform-side selection synthesis** — rank by gate status, return best. 30 minutes.

3. **`debate_strategist` typed workflow_template** — add `nodes/edges` for the debate
   pattern, create `input_contract.sketch.json` for `abw/debate-request/1`. 1 hour.

4. **`vote_strategist` typed workflow_template** — same as vote but simpler (pure fan-out).

5. **`ToolContext: Clone`** (if Option A is pursued later) — audit all fields, wrap any
   non-Clone in Arc. Enables true OS-parallel execution.
