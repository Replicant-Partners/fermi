# Composition Feedback Loop — Implementation Plan

**Status:** Defined, not yet started  
**Depends on:** migration 113 (composition_as_first_class — done), valence on Agent (migration 114 — done)  
**Design rationale:** `docs/papers/coherence_improvement_loop.md` §4–5, this conversation 2026-05-13

---

## What this builds

A closed feedback loop in which `cohere_and_coordinate` — as the default workspace strategist — learns from dreaming across sessions and proposes structural changes to the composition (member weights, member roster) that a human owner approves or rejects. Rejections feed back into the strategist's memory as correction episodes.

The loop operates at the **composition level**, not the individual agent level. It is the tune-the-team RSI mode described in `docs/COMPOSITION_AS_FIRST_CLASS.md §4`.

---

## What already exists (foundation)

| Component | Location |
|---|---|
| `teams.coordination_strategist_id` + `composition_versions` table | migration 113 |
| `cohere_and_coordinate` as default strategist, shelf-wired to every workspace | `templates/workspace.html`, curated card |
| `valence` on `Agent` DB column + API write path | migration 114, `AgentUpdate` |
| `cohere_and_coordinate` dreaming budget + episodic memory | live |
| `rsi_modes: ["cascade"]` declared on card | card metadata |
| Coherence shelf: Index / Recommendations / Dream Notes buttons | `templates/workspace.html` |

---

## What is missing

| Component | Status |
|---|---|
| `composition_versions` store methods (create / list / accept / reject) | ❌ |
| API routes for composition version lifecycle | ❌ |
| `propose_composition_change` MCP tool | ❌ |
| `POST /api/workspaces/:id/composition/dream` endpoint | ❌ |
| Valence homophily detection in `cohere_and_coordinate` system prompt | ❌ |
| `"tune_team"` added to `rsi_modes` | ❌ |
| "Dreaming" shelf button | ❌ |
| "Proposals" shelf section (pending composition_versions) | ❌ |
| Version history view in workspace | ❌ |

---

## Phase 1 — Storage layer

### 1a. `CompositionVersion` type (`agent-bestiary/memory/src/types.rs`)

```rust
pub struct CompositionVersion {
    pub composition_version_id: Uuid,
    pub workspace_id: Uuid,
    pub version_number: i32,
    pub mission: Option<String>,
    pub coordination_strategist_id: Option<Uuid>,
    pub member_agent_ids: Option<Vec<Uuid>>,
    pub member_weights: Option<serde_json::Value>,
    pub diff_summary: Option<String>,
    pub proposed_by: Option<String>,   // 'user' or strategist agent_id
    pub accepted_by: Option<String>,   // user_id or None if pending/rejected
    pub rejected_by: Option<String>,
    pub rejection_note: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

### 1b. Three store methods (`agent-bestiary/memory/src/store.rs`)

```rust
create_composition_version(version: &CompositionVersion) -> Result<Uuid>
list_composition_versions(workspace_id: Uuid) -> Result<Vec<CompositionVersion>>
resolve_composition_version(
    version_id: Uuid,
    resolved_by: &str,
    accepted: bool,
    rejection_note: Option<&str>,
) -> Result<()>
// When accepted=true: also updates teams.member_agent_ids + member_weights
// to the version's values (making it the active composition).
```

---

## Phase 2 — API routes

```
GET  /api/workspaces/:id/composition/versions
     → list_composition_versions, newest first
     Auth: workspace member or admin

POST /api/workspaces/:id/composition/versions/:version_id/accept
     → resolve_composition_version(accepted=true)
     Auth: workspace owner or admin only

POST /api/workspaces/:id/composition/versions/:version_id/reject
     Body: { note?: string }
     → resolve_composition_version(accepted=false, rejection_note)
     → store rejection as EpisodeCorrection in strategist's memory
       (scope=agent_wide, provenance=human_corrected, authority_weight=1.0)
     Auth: workspace owner or admin only
```

The rejection path is important: it must write a correction episode into
`cohere_and_coordinate`'s episodic memory so the rejection reason becomes
dreaming material for the next cycle.

---

## Phase 3 — Strategist intelligence

### 3a. New MCP tool on `cohere_and_coordinate` card

```json
{
  "name": "propose_composition_change",
  "description": "Propose a structural change to this workspace's composition. Creates a composition_versions row pending owner approval. Use only when dreaming has identified a persistent pattern (homophily, structural gap, chronic principle weakness) that the current team cannot self-correct. This is a high-stakes action — justify with evidence from consolidated episodes.",
  "input_schema": {
    "type": "object",
    "properties": {
      "diff_summary": {
        "type": "string",
        "description": "Plain-language description of the proposed change and why"
      },
      "member_agent_ids": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Proposed new member roster (agent_ids). Omit to keep current roster."
      },
      "member_weights": {
        "type": "object",
        "description": "Proposed weight adjustments as { agent_id: float 0-1 }. Omit to keep current weights."
      },
      "rationale": {
        "type": "string",
        "description": "Evidence-grounded justification: which episodes, which principle patterns, which valence distribution drove this proposal"
      },
      "homophily_detected": {
        "type": "boolean",
        "description": "Whether this proposal is specifically triggered by valence homophily detection"
      }
    },
    "required": ["diff_summary", "rationale"]
  }
}
```

### 3b. System prompt additions

Add **Stage 4 — Tension Audit** to `cohere_and_coordinate`'s system prompt.
Runs only when invoked via the composition dreaming endpoint, not on every
shelf invocation.

Key behaviours:
1. Read `valence` from all workspace members via `list_workspace_agents`
2. Compute arousal spread and valence spread: flag homophily when spread < 0.25 on either axis
3. Review consolidated episodes for recurring incoherence type patterns (from §5.2 of the paper):
   - Destructive: low P2 + low P7 + low evidence engagement
   - Productive-competitive: low P6 + moderate P2 (good — protect this)
   - Productive-analogical: low P3 + high P2 (good — protect this)
   - Productive-contradictory: low P5 + high P4 (good — protect this)
4. If homophily detected OR destructive incoherence is recurring: call `propose_composition_change`
5. If productive incoherence is being suppressed: issue anti-convergence alert in chat without proposing structural change
6. If Γ(C) rose sharply (> 0.3 in one session) without high P4 (evidence engagement): flag as possible false consensus

Feedback design constraints (from paper §5.4 — non-negotiable):
- **Structural, not prescriptive**: name the pattern, do not pick the replacement agent
- **Evidence-oriented**: cite specific episodes and principle scores
- **Anti-convergence alerts** when consensus appears too easy
- When proposing a member weight change: "Agent X's contributions are being structurally underweighted" not "Agent X is bad"

Add `"tune_team"` to `rsi_modes` in card metadata.

### 3c. New composition dreaming endpoint

```
POST /api/workspaces/:id/composition/dream
```

Constructs a structured prompt with:
- Current team composition + valence distribution
- Last N session Γ(C) scores and principle breakdowns
- `cohere_and_coordinate`'s consolidated dreaming synopsis (if available)

Then invokes `cohere_and_coordinate` with Stage 4 mode active. Response
streams into workspace chat. If the agent calls `propose_composition_change`,
the handler intercepts the tool call and writes the `composition_versions` row.

---

## Phase 4 — Workspace UI

### 4a. "Composition" shelf section

Add to the Coherence shelf, below the existing three buttons, separated by
a divider:

```
─── Composition ──────────────────
[Dreaming]  (5 cr)
```

The Dreaming button calls `POST /api/workspaces/:id/composition/dream`.
Output appears in workspace chat.

### 4b. "Proposals" shelf section

Appears only when `composition_versions` rows exist with `accepted_by IS NULL`
and `rejected_by IS NULL`. Shows pending proposals:

```
─── Proposals ────────────────────
● strategist · 2026-05-13
  "Add high-arousal agent to break..."
  [View]  [Accept]  [Reject ▾]
```

- View: expands `diff_summary` + `rationale` inline
- Accept: calls `POST .../accept`
- Reject: opens a text input for rejection note, then calls `POST .../reject`

After accept/reject the section disappears (or shows "No pending proposals").

### 4c. Version history in workspace detail

Extend workspace detail view to show `composition_versions` as a timeline:
- Version number, proposed_by, date
- diff_summary (one line)
- Status: pending / accepted / rejected + by whom
- Rejection note if present

This is the audit trail for the tune-the-team RSI loop.

---

## The full loop once shipped

```
Sessions run in workspace
    ↓
cohere_and_coordinate observes coherence per session
(Γ(C) + principle scores stored as episodes in strategist's memory)
    ↓
Owner clicks "Dreaming" on shelf
    ↓
cohere_and_coordinate consolidates:
  · recurring principle weaknesses
  · valence homophily patterns
  · productive vs destructive incoherence classification
    ↓
    ├── No structural issue
    │     → anti-convergence alert in chat if false consensus detected
    │
    └── Structural issue detected
          ↓
    propose_composition_change tool called
    composition_versions row created (proposed_by = strategist, status = pending)
          ↓
    "Proposals" section appears in shelf
          ↓
    ├── Owner accepts
    │     → teams.member_agent_ids updated
    │     → composition_versions row: accepted_by = owner
    │     → next sessions run with new team
    │
    └── Owner rejects + note
          → rejection stored as EpisodeCorrection in strategist's memory
          → next dreaming cycle learns from it
```

---

## Implementation order

| # | What | Files touched | Size |
|---|---|---|---|
| 1 | `CompositionVersion` type + 3 store methods | `types.rs`, `store.rs` | S |
| 2 | 3 API routes (list/accept/reject) | `api_server.rs`, new `handlers/composition.rs` | S |
| 3 | `propose_composition_change` tool handler | `handlers/composition.rs` | M |
| 4 | `POST .../composition/dream` endpoint | `handlers/composition.rs` | M |
| 5 | Update `cohere_and_coordinate` card | `agents/curated/cohere_and_coordinate/agent_card.json` | M |
| 6 | Workspace shelf: Dreaming button + Proposals section | `templates/workspace.html` | M |
| 7 | Version history view | `templates/workspace.html` | S |

Steps 1–4 are one PR (storage + API, no UI, no behaviour change).  
Steps 5–7 are one PR (intelligence + UI).  
No new migrations needed — `composition_versions` table exists from migration 113.
