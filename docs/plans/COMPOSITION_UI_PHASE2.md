# Composition Graph UI — Phase 2: Visual Editor

**Written 2026-09-05. Entry point for the visual graph editor session.**  
**Phase 1 (form-based editor) is complete and in production.**

---

## What Phase 1 built

The `▷ Graph` tab on the workspace page shows when the workspace has a coordination
strategist. It presents:
- Synthesis protocol picker + candidate scope picker
- Node list: id, agent (blank = open slot), input/output schema IDs, pinned toggle
- Edge list: from → to → schema label
- Save button → PUT to strategist agent's `workflow_template`

Data flows: `cgNodes[]`, `cgEdges[]`, `cgSynthesis`, `cgScope` in-memory → serialised
to `WorkflowTemplate` shape → stored on the strategist agent card via `PUT /api/agents/:name`.

---

## What Phase 2 adds

A canvas/SVG-based visual graph that replaces the form list in `renderCgEditor()`.

### Visual design

```
╔══════════════════════════════════════════════════════════════╗
║  Synthesis [selection ▾]   Scope [workspace ▾]   [Save]      ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║   ┌──────────────────┐     ┌──────────────────┐             ║
║   │ ○ member_1       │     │ ○ member_2       │             ║
║   │ fermi/forecast…  │     │ fermi/forecast…  │             ║
║   │ [open slot]      │     │ [equity_analyst] │             ║
║   └──────────────────┘     └──────────────────┘             ║
║              ╲                    ╱                          ║
║               ╲                  ╱                           ║
║                ╲                ╱                            ║
║                 ▼              ▼                             ║
║              ┌──────────────────────┐                        ║
║              │  ◉ synthesiser       │                        ║
║              │  [selection output]  │                        ║
║              └──────────────────────┘                        ║
║                                                              ║
║  [+ Add node]   [+ Add edge]   Topology: fan-out             ║
╚══════════════════════════════════════════════════════════════╝
```

Click a node → config panel slides in from the right (agent, schemas, pinned flag).
Drag nodes to reposition. Click empty space → deselect.
Edges drawn as SVG arrows between node boxes.

### Implementation approach

**No external library.** Custom SVG overlay on a positioned `<div>`.

Node state: `{ id, x, y, agent, input_schema, output_schema, pinned }` — add `x`, `y`
to the in-memory model. Default positions computed from level assignment (auto-layout).

**Auto-layout** (for initial render and "relayout" button):
- Level 0 nodes: y = 80, x = evenly spaced
- Level 1 nodes: y = 220
- etc. Simple grid, not force-directed.

**Files to create:**
- `static/js/widgets/composition-graph.js` — the visual editor widget
- `static/css/composition-graph.css` — node/edge/panel styles

**Files to modify:**
- `templates/workspace.html` — swap `renderCgEditor()` for `CompositionGraph.render()`
  - The data model (cgNodes, cgEdges, etc.) stays the same
  - Only the rendering and interaction layer changes

---

## Key implementation pieces

### CompositionGraph widget API

```js
window.CompositionGraph = (() => {
  function render(container, { nodes, edges, synthesis, scope, onSave, readOnly }) {
    // Build SVG + node boxes
    // Wire drag, click, keyboard handlers
    // Return { getNodes, getEdges, getSynthesis, getScope }
  }
  return { render };
})();
```

### Node box (SVG foreignObject or plain div)
```html
<div class="cg2-node" style="left:120px;top:80px" data-id="member_1">
  <div class="cg2-node-header">
    <span class="cg2-node-id">member_1</span>
    <span class="cg2-node-slot">open slot</span>
  </div>
  <div class="cg2-node-schema">fermi/forecast-question/1</div>
</div>
```

### SVG edge arrows
```svg
<svg class="cg2-edges" ...>
  <defs>
    <marker id="arrowhead" ...>...</marker>
  </defs>
  <line x1="..." y1="..." x2="..." x2="..." marker-end="url(#arrowhead)"/>
</svg>
```

The SVG is absolutely positioned over the node container. Node positions drive edge
endpoints. On drag, update node position → re-render affected edges only.

### Config panel (slide-in, right side)
When a node is clicked, show a side panel within the graph container:
```
Selected: member_1
────────────────────
Agent     [ leave blank for select_agent ]
Input     [ fermi/forecast-question/1    ]
Output    [ fermi/football-evidence/1    ]
Pinned    [□] bypass select_agent

[Remove node]   [Done]
```

### Edge drawing interaction
Click "Draw edge" button → cursor changes to crosshair → click source node → click target
node → edge created. ESC cancels.

---

## What stays the same from Phase 1

- Data model: `cgNodes[]`, `cgEdges[]`, `cgSynthesis`, `cgScope`
- `cgCollect()` — reads current state from the widget's internal model
- `cgSave()` — unchanged; calls PUT with the collected workflow_template
- `loadCompositionGraph()` — unchanged; fetches from strategist agent specimen
- CSS variables (uses existing `var(--bg0)`, `var(--aqua)`, etc.)

---

## Integration checklist

- [ ] `CompositionGraph.render()` called from `renderCgEditor()` instead of building HTML
- [ ] `cgCollect()` updated to call `CompositionGraph.getNodes()` etc.
- [ ] `cgAddNode()` and `cgAddEdge()` delegate to `CompositionGraph.addNode()` etc.
- [ ] Auto-layout preserves user drag positions (position stored in cgNodes[].x, cgNodes[].y)
- [ ] "Relayout" button recomputes from level assignment
- [ ] `check_specimen_shelf.js` and `check_agent_fields.js` still pass (they don't test workspace.html)

---

## Topology badge

Show in the panel header, updated when nodes/edges change:
- Pipeline (single chain)
- MoE fan-out (no edges, multiple nodes)
- Hybrid (mixed levels)

Computed by `detect_topology(levels)` — already in `coordination_graph.rs`, mirror the
logic in JS for the frontend badge.
