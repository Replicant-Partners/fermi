# Doc 5 — App Action Protocol: kask Migration Path

**Audience:** kask client maintainer
**Status:** ABW side shipped (`migrations/125`, `/api/workspaces/:id/actions/*`)
**Depends on:** Doc 4 (App CLI Extension), simops_companion v2.0.0

---

## What changed on the ABW side

The six SimOps action grammar types are now first-class API endpoints
**and** MCP tool calls, not just action blocks the kask dispatcher
applies client-side:

```
POST /api/workspaces/:id/actions/mutate_document
POST /api/workspaces/:id/actions/fork_state
POST /api/workspaces/:id/actions/compare
POST /api/workspaces/:id/actions/invoke_member
POST /api/workspaces/:id/actions/annotate_schema
POST /api/workspaces/:id/actions/annotate

GET  /api/workspaces/:id/actions           — full log
GET  /api/workspaces/:id/actions/pending   — awaiting confirmation
POST /api/workspaces/:id/actions/:id/accept
POST /api/workspaces/:id/actions/:id/reject
GET  /api/workspaces/:id/annotations       — open annotations
DELETE /api/workspaces/:id/annotations/:id — resolve
```

These are also exposed as named MCP tools on `simops_companion`:

```
POST /mcp/agents/simops_companion
{ method: "tools/call", params: { name: "mutate_document", arguments: { ... } } }
```

Everything is recorded in `workspace_action_log` (append-only) and
`workspace_annotations` (queryable, resolvable).

---

## Migration path for kask — three phases

### Phase 0 — No kask changes (already works today)

The companion still emits `__ACTION__` blocks. kask's existing
`simops-actions.js` dispatcher still applies them client-side.
Nothing breaks. The new endpoints exist but kask doesn't call them yet.

The only new thing available: the action log records are empty until
kask starts POSTing. The annotations table is empty.

### Phase 1 — POST to action endpoints after applying (1–2 days)

After kask applies an action client-side, also POST it to the
corresponding endpoint. This is fire-and-forget — don't block the
UI on the response.

```js
// After applying an edit_process action locally:
await fetch(`/api/workspaces/${wsId}/actions/mutate_document`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
  body: JSON.stringify({
    app_schema: 'kask_simops',
    path: 'simops/process.yaml',
    patch: action.patch,
    content: serialisedYaml,   // the full new content
    rationale: action.rationale,
    confirmation: 'auto',      // kask already applied it
    source_message_id: messageId,
  })
})

// After applying an annotate action:
await fetch(`/api/workspaces/${wsId}/actions/annotate`, {
  method: 'POST',
  body: JSON.stringify({
    app_schema: 'kask_simops',
    kind: action.kind,
    target: action.target,
    body: action.body,
    severity: action.severity,
    source_message_id: messageId,
  })
})
```

**What this buys:** audit log, annotations queryable via API,
calibration linkage, CLI parity.

### Phase 2 — Use POST as the source of truth (1–2 days)

Flip the dispatcher: instead of applying locally then POSTing,
POST first and let the server response confirm success.

For `mutate_document` with `confirmation: "ask"`:
1. POST → server returns `{ action_id, confirmation: "pending" }`
2. Render diff modal with `action.patch`
3. User accepts → POST `/actions/:action_id/accept` with `{ content: serialisedYaml }`
4. Server writes to git, returns `{ applied: true, apply_result: { sha } }`

For `annotate`, `fork_state`, `invoke_member` (auto-apply):
1. POST → server returns `{ action_id, applied: true }`
2. Update UI from server response

**What this buys:** git writes are server-authoritative (no drift
between kask state and workspace git), action log is complete,
accept/reject is the diff modal's natural endpoint.

### Phase 3 — Replace action blocks with MCP tool calls (optional)

For calls that don't need the companion's LLM reasoning, bypass the
message → parse → dispatch pipeline entirely:

```js
// Instead of: send message → parse __ACTION__ → dispatch
// Do:
const result = await fetch('/mcp/agents/simops_companion', {
  method: 'POST',
  body: JSON.stringify({
    jsonrpc: '2.0', id: 1, method: 'tools/call',
    params: {
      name: 'fork_state',
      arguments: {
        workspace_id: wsId,
        name: 'co2-capture-75',
        from: 'base',
        patch: { stages: [...] },
        hypothesis: 'Capturing 75% of CO2 improves NER'
      }
    }
  })
})
// Returns: { result: { content: [{ type: 'json', json: { action_id, variant_slug, path } }] } }
```

Use this for:
- User-initiated forks (Scenarios panel "+ Fork" button)
- User-initiated annotations (inline critique/insight buttons)
- Programmatic compares triggered by kask UI (not companion prose)

Keep message → parse → dispatch for:
- Companion-initiated actions (the companion decides what to do)
- Multi-action turns (companion emits 3 actions in one response)

---

## The invariant across all three phases

**The action type names are the same everywhere:**

| Companion block | API endpoint suffix | MCP tool name |
|---|---|---|
| `edit_process` | `mutate_document` | `mutate_document` |
| `fork_variation` | `fork_state` | `fork_state` |
| `compare_variations` | `compare` | `compare` |
| `invoke_agent` | `invoke_member` | `invoke_member` |
| `declare_sosa_contract` | `annotate_schema` | `annotate_schema` |
| `annotate` | `annotate` | `annotate` |

Note: the API/MCP names are the generalized forms. The companion's
system prompt uses the SimOps-specific names (`edit_process` etc.)
for readability — these are synonyms. kask maps them when posting:

```js
const ACTION_TYPE_MAP = {
  'edit_process':         'mutate_document',
  'fork_variation':       'fork_state',
  'compare_variations':   'compare',
  'invoke_agent':         'invoke_member',
  'declare_sosa_contract':'annotate_schema',
  'annotate':             'annotate',
}
```

---

## What the `abw` CLI will do (Doc 4 Phase 1, upcoming)

Once `abw app generate-cli kask_simops` is implemented, the generated
`simops` CLI will call the same endpoints:

```bash
simops process edit --stage fermentation --efficiency 0.78 --auto
# → POST /api/workspaces/:id/actions/mutate_document
#   { path: "simops/process.yaml", patch: {stages:[...]}, confirmation: "auto" }

simops fork --name co2-capture --hypothesis "..."
# → POST /api/workspaces/:id/actions/fork_state

simops annotate --kind insight --target stage:fermentation "Bottleneck here"
# → POST /api/workspaces/:id/actions/annotate
```

kask's dispatcher, the generated CLI, and direct MCP tool calls all
write to the same action log. The action log is the single source of
truth for what has happened to a workspace's canonical document.
