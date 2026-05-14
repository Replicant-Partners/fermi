# Doc 2 — SimOps as the first App

**Audience:** kask development (you and the kask-side agent) and whoever ships SimOps v2.
**Status:** spec — ready to implement after Doc 1 lands on ABW.
**Depends on:** Doc 1 (App primitive on ABW).
**Goal:** ship SimOps v2 as the first concrete instance of the App primitive — a four-mode workspace (Intake / Compose / Scenarios / Experiments) for designing process pipelines, persistent across sessions, with real forecasting and A/B comparison.

---

## 1. The SimOps App manifest

This is the JSON we POST to `/api/apps` once, on first deploy after Doc 1 lands. It registers SimOps as a first-class App on ABW.

```jsonc
{
  "slug": "kask_simops",
  "name": "SimOps",
  "tagline": "Design, simulate, and compare process pipelines.",
  "homepage_url": "https://kask.bio/projects/simops",
  "icon_url": "https://kask.bio/assets/simops-icon.svg",
  "description": "Agent-led process modelling: interview to design, edit to compose, run to forecast. Built on the SimOps engine (ProcessConfig, cascade, predictor, optimizer) and the Fermi forecasting layer.",
  "composition_slug": "simops_fleet",
  "schema_slug": "kask-simops/2",
  "schema_json": { /* the JSON Schema for ProcessConfig — see §3 */ },
  "workspace_template": {
    "initial_budget": 250,
    "default_name_pattern": "SimOps — {date} process",
    "auto_hire": ["simops_advisor", "simops_cascade", "simops_narrator"],
    "initial_files": [
      {
        "path": "simops/process.yaml",
        "content": "name: New Process\ndescription: ''\nstages: []\n"
      },
      {
        "path": "simops/budget.yaml",
        "content": "discovery:\n  enabled: false\n  daily_cap_credits: 25\n  weekly_cap_credits: 100\n"
      },
      {
        "path": ".app/manifest.yaml",
        "content": "app_slug: kask_simops\nschema: kask-simops/2\nui: https://kask.bio/projects/simops\n"
      },
      {
        "path": "context/readme.md",
        "content": "# SimOps workspace\nThis workspace was created by the SimOps app on kask.bio.\nAll process state lives under `simops/`.\n"
      }
    ]
  },
  "visibility": "public",
  "metadata": {
    "kask_version": "2.0",
    "icon_color": "#1a3833"
  }
}
```

### 1.1 The `simops_fleet` composition

We also register (or document) a composition called `simops_fleet` in the catalogue. It's the fleet for SimOps:

```
Primary (used in every workspace):
  simops_advisor       — the 6-turn interviewer
  simops_cascade       — energy/mass balance engine
  simops_narrator      — plain-language interpreter

On-demand (hired or invoked per use):
  simops_predictor     — OLS regression on observations
  simops_optimizer     — what-if solver
  supply_chain_oracle  — BoM pricing + supply risk

To be authored (Phase 2+):
  sidestream_miner     — surfaces sidestream candidates per stage
  comparator           — narrates A/B experiment deltas
  product_scout        — proposes end-products from accepted sidestreams
  regulatory_scanner   — flags jurisdiction-specific risks
  valuechain_mapper    — upstream input substitutes + downstream offtakers
```

The composition is referenced by slug in the App manifest. ABW's existing composition_patterns catalogue is the right home for it.

---

## 2. The four-mode UX

The user interacts with SimOps in four modes. All modes share the same workspace (same chat thread, same budget, same agent roster).

### 2.1 Intake — agent-led interview

**What:** the existing 6-turn wizard, evolved. The `simops_advisor` agent runs an interview that produces a draft Process. Each turn:
- shows the agent's interpretation + insight
- asks a question
- accepts the user's free-text answer
- writes the turn (both sides) as workspace messages with `metadata.kind: "intake_turn"`

**When complete:** writes a draft `simops/process.yaml` and transitions the user to Compose mode.

**Affordances added vs v1:**
- "Branch this turn" — open a sub-thread for a specific sub-question (sidestreams, regulatory, scale, anything). Sub-threads are also workspace messages but tagged `metadata.kind: "intake_branch"` and `metadata.parent_turn: <turn_id>`.
- "Skip to compose" — if the user already has a draft, jump straight in.

### 2.2 Compose — visual editor

**What:** the existing composer, evolved. Edits the `ProcessConfig` directly.

**Layout:**
- Left rail: workspace switcher + mode switcher
- Top: process name, current version, save button (explicit save — NOT autosave, because file writes cost credits)
- Center: visual pipeline (stages flow left to right, sidestreams hanging below, sensors as small chips)
- Right rail: stage inspector + "What else?" buttons + Insights panel

**Per-stage affordances:**
- Edit stage parameters (efficiency as fraction 0..1, carbon intensity, input/output Resources, capex, opex)
- Add/remove sidestreams
- Add/remove sensors
- "Run cascade" — calls `POST /api/simops/cascade` (the direct, non-LLM endpoint added by Doc 1 §6.3). Sub-100ms. Shows updated flows + KPIs.
- "What else?" — runs `sidestream_miner` (once authored) or `supply_chain_oracle` for the stage. Returns Insights, written to `simops/insights/<ulid>.yaml`.

**Insights panel:**
- Reads `simops/insights/*.yaml` files (one per insight — see Doc 1 §6 of the capability brief on append semantics)
- Each Insight has `status: suggested | accepted | rejected | snoozed`
- Accept/reject/snooze writes back a new file revision

**Save model:**
- Explicit "Save" button writes `simops/process.yaml` (one PUT, one commit, one charge)
- Versions = git commits. List via `GET /api/workspaces/:id/git/log`
- Optionally: name a save with a commit message

### 2.3 Scenarios — A/B parameter variants

**What:** create named Scenarios on top of a Process. A Scenario is a full `ProcessConfig` snapshot — not a diff (simpler, lets us diff via git).

**Layout:**
- List of scenarios in left rail (Baseline + named variants)
- Side-by-side editor: pick 2-4 scenarios, see their parameter tables aligned column-by-column
- Diff highlighting via `GET /api/workspaces/:id/git/diff?from=<scenario_a_sha>&to=<scenario_b_sha>`

**Storage:**
- `simops/scenarios/<scenario_slug>.yaml` — full ProcessConfig
- `simops/scenarios/index.yaml` — list with name, slug, base_sha, created_at, hypothesis

**Affordances:**
- "Clone from current process" — make a new scenario starting from `process.yaml`
- "Promote to experiment" — pass a set of scenarios to the Experiment mode

### 2.4 Experiments — forecasts & comparison

**What:** run N scenarios through a forecast engine. Show distributions side-by-side. Record a Decision.

**Engines (v1):**
- **FPL** via `POST /api/fpl/execute` — for parametric forecasts (NPV distribution, payback time, sidestream value)
- **simops_predictor + simops_optimizer** via workspace mention — for observation-grounded what-if

**Flow:**
1. User picks 2-4 scenarios from §2.3
2. User picks an engine and a question (e.g. "What is the 12-month NPV?")
3. kask generates an FPL source string for each scenario by templating in the scenario's parameters
4. POST each to `/api/fpl/execute` (cheap, fast)
5. Render distributions in a comparison view (overlaid sparklines + delta table + p5/median/p95)
6. Invoke `comparator` (once authored) on the results for a narrative
7. User records a "Decision" — what they're going to do, why. Stored as `simops/decisions/<ulid>.md`

**Storage:**
- `simops/experiments/<experiment_id>.yaml` — scenarios used, engine, question, raw results, comparator narrative
- `simops/decisions/<ulid>.md` — markdown with frontmatter linking back to the experiment

**Marketing handoff preview:**
- "Export to marketing" on a completed experiment writes `simops/marketing/<packet_id>.yaml` with claims + evidence (see §6)
- Renders in SimOps as a preview only — no actual marketing delivery in v1

---

## 3. Schema — `kask-simops/2`

We migrate kask to the Rust ProcessConfig shape. The schema below is the JSON Schema we register on the App manifest. The Rust types in `crates/simops/src/process.rs` (with the additions from Doc 1 §6.4) are the source of truth.

```yaml
# Example ProcessConfig in YAML — what lives in simops/process.yaml
name: "Kombucha — Craft Batch (200L)"
description: "200L batch kombucha fermentation with sidestream capture exploration"
feature_of_interest: null  # SOSA URI, optional
elec_price_per_kwh: 0.12
maintenance_cost_usd: 1500
stages:
  - id: sweet_tea_prep
    efficiency: 0.96         # fraction, NOT percentage
    carbon_intensity: 0.18
    input:
      name: water
      unit: L
      energy_density: null
      density_unit: null
    output:
      name: sweet_tea
      unit: L
      energy_density: null
      density_unit: null
    capex:
      total_usd: 1200
      lifespan_years: 8
    opex_per_input_unit: 0.02
    sidestreams: []
    sensors:
      - id: tea_temp
        name: "Brew temperature"
        measures: temperature
        unit: degC
        sosa_property_uri: null
  - id: primary_fermentation
    efficiency: 0.82
    carbon_intensity: 0.05
    input:
      name: sweet_tea
      unit: L
    output:
      name: kombucha_raw
      unit: L
    capex: null
    opex_per_input_unit: null
    sidestreams:
      - id: co2
        name: "CO2"
        resource:
          name: CO2
          unit: L
        capture_fraction: 0.05
        value_per_unit_usd: null
        current_disposition: vented
      - id: pellicle
        name: "SCOBY pellicle"
        resource:
          name: scoby_pellicle
          unit: kg
        capture_fraction: 0.20
        value_per_unit_usd: 60
        current_disposition: discarded
    sensors: []
```

**Migration from kask v1 shape:**
- `efficiency_pct: 85` → `efficiency: 0.85` (divide by 100)
- Add `description`, `feature_of_interest`, `elec_price_per_kwh`, `maintenance_cost_usd` at top level
- Wrap kask's flat `input_name/output_name` into `Resource` objects
- Move `sidestreams[]` to be per-stage (matches the new Rust struct)
- Move `sensors[]` to be per-stage (same)

We ship a one-time migration utility in the kask client: `kaskV1ToV2(oldYaml) -> newYaml`. It runs once per existing wizard output, then v1 shape disappears.

---

## 4. kask client — `KaskApp` + `KaskSim`

We extend `abw-client.js` with a generic App layer (`KaskApp`) and a SimOps-specific layer (`KaskSim`) on top of it.

### 4.1 `KaskApp` — generic App client (reusable by future apps)

In `abw-client.js`, add a new namespace `ABW.app`:

```js
ABW.app = {
  // App registry
  async list({ visibility, owner, slug_prefix } = {}) { /* GET /api/apps */ },
  async get(slug)                                      { /* GET /api/apps/:slug */ },
  async register(manifest)                             { /* POST /api/apps */ },
  async update(slug, patch)                            { /* PUT /api/apps/:slug */ },
  async publish(slug)                                  { /* POST /api/apps/:slug/publish */ },
  async archive(slug)                                  { /* POST /api/apps/:slug/archive */ },

  // Workspace spawning
  async spawnWorkspace(slug, { name, description, extra_budget, auto_hire_override } = {}) {
    /* POST /api/apps/:slug/workspaces */
  },
  async listWorkspaces(slug) {
    /* GET /api/apps/:slug/workspaces */
  },
};
```

### 4.2 `KaskSim` — SimOps-specific client

A new file `kask-sim-client.js` loaded by the SimOps pages:

```js
const KaskSim = (function () {
  const APP_SLUG = 'kask_simops';

  // ─── Workspace lifecycle ─────────────────────────────────────────
  async function createWorkspace({ name, description, extra_budget } = {}) {
    return ABW.app.spawnWorkspace(APP_SLUG, { name, description, extra_budget });
  }

  async function listMyWorkspaces() {
    return ABW.app.listWorkspaces(APP_SLUG);
  }

  // ─── Process YAML (the canonical doc) ────────────────────────────
  async function loadProcess(wsId) {
    const r = await ABW.readWorkspaceFile(wsId, 'simops/process.yaml');
    return parseYaml(r.content);  // jsyaml
  }
  async function saveProcess(wsId, processConfig, commitMessage) {
    const yaml = stringifyYaml(processConfig);
    return ABW.writeWorkspaceFile(wsId, 'simops/process.yaml', yaml, commitMessage || 'update process');
  }
  async function getProcessHistory(wsId, limit = 50) {
    return ABW.workspaceGitLog(wsId, limit);
  }
  async function diffProcessVersions(wsId, fromSha, toSha) {
    return ABW.workspaceGitDiff(wsId, fromSha, toSha);
  }

  // ─── Scenarios ───────────────────────────────────────────────────
  async function listScenarios(wsId) {
    const list = await ABW.listWorkspaceFiles(wsId, 'simops/scenarios/');
    return list.files.filter(f => f.path.endsWith('.yaml') && !f.path.endsWith('/index.yaml'));
  }
  async function loadScenario(wsId, slug) {
    const r = await ABW.readWorkspaceFile(wsId, `simops/scenarios/${slug}.yaml`);
    return parseYaml(r.content);
  }
  async function saveScenario(wsId, slug, processConfig, hypothesis) {
    const yaml = stringifyYaml({ ...processConfig, _hypothesis: hypothesis });
    return ABW.writeWorkspaceFile(wsId, `simops/scenarios/${slug}.yaml`, yaml, `scenario: ${slug}`);
  }

  // ─── Insights (file-per-record) ──────────────────────────────────
  async function listInsights(wsId, { status } = {}) {
    const list = await ABW.listWorkspaceFiles(wsId, 'simops/insights/');
    const insights = [];
    for (const f of list.files) {
      if (!f.path.endsWith('.yaml')) continue;
      const r = await ABW.readWorkspaceFile(wsId, f.path);
      const insight = parseYaml(r.content);
      if (!status || insight.status === status) insights.push(insight);
    }
    return insights;
  }
  async function appendInsight(wsId, insight) {
    const id = ulid();
    const full = { id, status: 'suggested', created_at: new Date().toISOString(), ...insight };
    const yaml = stringifyYaml(full);
    await ABW.writeWorkspaceFile(wsId, `simops/insights/${id}.yaml`, yaml, `insight: ${insight.title || id}`);
    return full;
  }
  async function setInsightStatus(wsId, insightId, status) {
    const path = `simops/insights/${insightId}.yaml`;
    const r = await ABW.readWorkspaceFile(wsId, path);
    const insight = parseYaml(r.content);
    insight.status = status;
    insight.updated_at = new Date().toISOString();
    return ABW.writeWorkspaceFile(wsId, path, stringifyYaml(insight), `insight ${status}: ${insightId}`);
  }

  // ─── Experiments ─────────────────────────────────────────────────
  async function runExperiment(wsId, { scenarios, engine, question, fpl_template }) {
    const id = ulid();
    const results = [];
    for (const scen of scenarios) {
      if (engine === 'fpl') {
        const fplSource = renderFpl(fpl_template, scen);
        const r = await ABW.fplExecute(fplSource, { iterations: 10000 });
        results.push({ scenario: scen, fpl_source: fplSource, result: r });
      } else if (engine === 'agent') {
        const msg = await invokeInWorkspace(wsId, 'simops_optimizer', JSON.stringify(scen));
        results.push({ scenario: scen, agent_response: msg });
      }
    }
    const experiment = {
      id, engine, question, scenarios: scenarios.map(s => s.name),
      results, created_at: new Date().toISOString(),
    };
    await ABW.writeWorkspaceFile(
      wsId,
      `simops/experiments/${id}.yaml`,
      stringifyYaml(experiment),
      `experiment: ${question}`,
    );
    // Optionally also create Forecast objects per scenario for the leaderboard
    for (const r of results) {
      if (engine === 'fpl' && r.result?.mean != null) {
        await ABW.createForecast({
          question_text: `${question} — scenario: ${r.scenario.name}`,
          predicted_probability: r.result.mean,
          domain: 'simops',
          fpl_source: r.fpl_source,
          simulation_results: r.result,
          tags: ['simops', `workspace:${wsId}`, `experiment:${id}`, `scenario:${r.scenario.name}`],
          visibility: 'private',
          status: 'active',
        });
      }
    }
    return experiment;
  }

  // ─── Compose-mode live cascade (the fast path) ───────────────────
  async function runCascade(processConfig, direction, quantity) {
    return ABW.simopsCascade({ process: processConfig, direction, quantity });
  }

  // ─── Workspace-budget agent invocation ───────────────────────────
  // CRITICAL: direct /api/agents/:id/execute charges the user's personal wallet.
  // Workspace-budget charging requires going through workspace messages with @mention.
  async function invokeInWorkspace(wsId, agentId, query, metadata = {}) {
    // Post a message with @mention; ABW will execute and write the agent response back.
    const msg = await ABW.postWorkspaceMessage(wsId, {
      content: `@${agentId} ${query}`,
      message_type: 'chat',
      metadata: { ...metadata, kind: metadata.kind || 'agent_invocation' },
    });
    // Subscribe to messages and wait for the agent response.
    return await waitForAgentResponse(wsId, agentId, msg.message_id);
  }

  async function waitForAgentResponse(wsId, agentId, afterMessageId) {
    // Implementation: open SSE on /messages/stream, resolve on first
    // message where sender_type === 'agent' && sender_id === agentId && created_at > afterMessageId.
    // Timeout: 120s.
    /* ... */
  }

  // ─── Sessions = message timeline ─────────────────────────────────
  async function getSessionTimeline(wsId, { kinds } = {}) {
    const all = await ABW.getWorkspaceMessages(wsId);
    if (!kinds) return all.messages;
    return all.messages.filter(m => kinds.includes(m.metadata?.kind));
  }

  function streamSession(wsId, onMessage) {
    return ABW.streamWorkspaceMessages(wsId, onMessage);
  }

  return {
    createWorkspace, listMyWorkspaces,
    loadProcess, saveProcess, getProcessHistory, diffProcessVersions,
    listScenarios, loadScenario, saveScenario,
    listInsights, appendInsight, setInsightStatus,
    runExperiment, runCascade,
    invokeInWorkspace,
    getSessionTimeline, streamSession,
  };
})();
```

### 4.3 Extensions needed on `abw-client.js`

The methods `KaskSim` calls that aren't in `abw-client.js` today:

- `readWorkspaceFile(wsId, path)` → `GET /api/workspaces/:id/files/*path`
- `listWorkspaceFiles(wsId, pathPrefix)` → `GET /api/workspaces/:id/files?path=...`
- `workspaceGitLog(wsId, limit)` → `GET /api/workspaces/:id/git/log?limit=...`
- `workspaceGitDiff(wsId, from, to)` → `GET /api/workspaces/:id/git/diff?from=...&to=...`
- `postWorkspaceMessage(wsId, body)` → `POST /api/workspaces/:id/messages`
- `getWorkspaceMessages(wsId)` → `GET /api/workspaces/:id/messages`
- `streamWorkspaceMessages(wsId, cb)` → SSE on `/api/workspaces/:id/messages/stream`
- `fplExecute(source, opts)` → `POST /api/fpl/execute`
- `simopsCascade({process, direction, quantity})` → `POST /api/simops/cascade` (added by Doc 1 §6.3)
- `createForecast(body)` → `POST /api/forecasts`
- `executeAgentStream(agentId, query, callbacks)` → SSE on `/api/agents/:id/execute/stream`

All are thin wrappers around the routes already in ABW. Plan to ship `abw-client.js` v2 with these as part of the SimOps Phase 1 PR.

---

## 5. UI file layout

```
kask/projects/
  simops.html                  ← NEW: the four-mode shell
  simops-intake.html           ← evolution of simops-wizard.html
  simops-compose.html          ← evolution of existing composer
  simops-scenarios.html        ← NEW
  simops-experiments.html      ← NEW

kask/
  kask-sim-client.js           ← NEW: KaskSim module
  abw-client.js                ← extended with the methods in §4.3

kask/adaptogen/simops-v2/
  specs/                       ← these spec docs
  fixtures/
    kombucha-example.yaml      ← canonical example ProcessConfig
    bioreactor-example.yaml    ← second example using the SOSA URI
```

### 5.1 `simops.html` shell — wireframe

```
┌─────────────────────────────────────────────────────────────────────┐
│ KASK · SIMOPS                            user_pill · budget meter   │
├──────────────┬──────────────────────────────────────────────────────┤
│ MY PROCESSES │  Process: Kombucha 200L          [Intake][Compose][..│
│ ─────────────│                                                       │
│ ▸ Kombucha   │   {mode-specific content fills here}                  │
│ ▸ Skincare   │                                                       │
│ ▸ + New      │                                                       │
│              │                                                       │
│ TIMELINE     │                                                       │
│ ─────────────│                                                       │
│ ● msg        │                                                       │
│ ◆ insight    │                                                       │
│ ● msg        │                                                       │
│              │                                                       │
└──────────────┴──────────────────────────────────────────────────────┘
```

URL pattern: `/projects/simops?workspace=<id>&mode=<intake|compose|scenarios|experiments>`

The shell:
- Renders the left rail (workspace list filtered by `origin=kask_simops`, timeline filter)
- Renders the mode tabs at top
- Lazily loads the mode-specific pane (an iframe initially, eventually inlined)
- Subscribes to `streamSession(wsId, ...)` once and feeds messages into the timeline view

### 5.2 First-run flow

1. User clicks SimOps on `kask.bio`
2. `simops.html` loads, calls `KaskSim.listMyWorkspaces()`
3. If empty: shows a "Start a new process" CTA → calls `KaskSim.createWorkspace({...})` → opens Intake mode
4. If not empty: shows the list, auto-resumes the most recently active workspace

---

## 6. Marketing handoff (preview only in v1)

When an experiment completes, "Export to marketing" writes:

```yaml
# simops/marketing/<packet_id>.yaml
schema: kask-marketing-input/1
experiment_id: <id>
process_name: "Kombucha 200L"
winning_scenario: "co2-capture-75pct"
claims:
  - claim: "Captures 360L of CO₂ per batch"
    evidence:
      stage: primary_fermentation
      metric: co2_captured_l_per_batch
      scenario_value: 360
      confidence: 0.82
      source_experiment: <id>
target_audiences:
  - { segment: "craft fermenters EU", size_est: 12000 }
forecast_summary:
  npv_12mo: { median: 34000, p10: 18000, p90: 58000, currency: EUR }
  payback_months: { median: 9, p10: 6, p90: 14 }
status: draft
```

Rendered in a preview view inside SimOps. No agent invocation in v1 — when `marketing_composer` is authored later, it will accept this packet as input.

---

## 7. Implementation phases

### Phase 1 — Persistence backbone (this PR)

**Dependencies:** Doc 1 must be deployed first.

Ships:
- `KaskSim` module with everything in §4.2
- `abw-client.js` v2 with the new methods in §4.3
- One-time bootstrap: POST the SimOps App manifest to `/api/apps` (manual step, documented in repo README)
- `simops.html` shell with mode switcher
- Migrated `simops-intake.html` (replaces wizard)
- Migrated `simops-compose.html` (reads/writes the workspace process.yaml)
- Schema migration utility `kaskV1ToV2()` and a one-time bulk-migrate script for existing kask test data
- Scenarios mode skeleton (list + clone, no diff yet)
- Experiments mode skeleton (placeholder: shows "coming soon")

**What works at end of Phase 1:**
- A user can create a SimOps workspace, do a full intake, edit in compose, save versions
- All state persists across page reloads, devices, sessions
- Workspace budget meter is visible
- The session timeline shows every turn, save, and agent call

### Phase 2 — Insights + Compose live updates

Ships:
- Direct cascade endpoint integration (real-time stage edits) — needs Doc 1 §6.3
- "What else?" per stage → calls `supply_chain_oracle` (existing) and `sidestream_miner` (TBD — see §8)
- Insights panel with accept/reject/snooze
- Scenarios diff view via git/diff

### Phase 3 — Scenarios + Experiments

Ships:
- Full scenario editor (side-by-side compare)
- Experiment runner: FPL engine, distribution comparison view
- Decision recorder

### Phase 4 — Sharing + collaboration

Ships:
- Share modal calling `POST /api/shares` with `object_type: "workspace"` (needs Doc 1 §6.2)
- Workspace member roles in UI

### Phase 5 — Marketing preview + public publication

Ships:
- Marketing packet view
- Workspace publish flow (depends on ABW publication primitive — TBD)

### Phase 6 — Continuous discovery + server-side scheduling

Ships:
- Budget UI for discovery
- Scheduled discovery passes (initially client-timer, eventually server-side)

---

## 8. New agents to author

These don't exist in the bestiary yet. They are not blockers for Phase 1 but become important from Phase 2 onward. Each gets its own `agent_card.json` and system prompt. Recommend authoring them with `xaman_ek`'s `companion_builder_coach` agent.

| Agent | Phase | Notes |
|---|---|---|
| `sidestream_miner` | 2 | Given a stage, propose sidestream candidates with capture %, market value, regs |
| `comparator` | 3 | Compare 2-N experiment results, produce delta narrative |
| `sensor_advisor` | 2-3 | Given stage type + variance, recommend sensors + cost |
| `product_scout` | 3 | Given accepted sidestreams + pipeline, propose end-products with TAM hints |
| `regulatory_scanner` | 3-4 | Flag jurisdiction-specific risks for a Process |
| `valuechain_mapper` | 3-4 | Upstream input substitutes + downstream offtakers |
| `marketing_composer` | 5 | Takes a marketing packet, produces A/B copy variants |

For Phase 1, kask only uses existing agents: `simops_advisor`, `simops_cascade`, `simops_narrator`, `simops_predictor`, `simops_optimizer`, `supply_chain_oracle`.

---

## 9. Migration of existing SimOps users

The current SimOps wizard at `kask.bio/projects/simops-wizard.html` has no persisted state — every visit starts fresh. So there's nothing to migrate user-side.

What we do need:
- Update the homepage link from `simops-wizard.html` → `simops.html`
- Redirect `simops-wizard.html` to `simops.html?mode=intake` for backwards compatibility on shared URLs
- The OAuth/auth refactor already landed in earlier work — no auth migration needed

---

## 10. Acceptance criteria

Phase 1 is done when:

1. A user can land on `kask.bio/projects/simops`, sign in (existing flow), and create a new SimOps workspace in one click
2. The workspace appears on `/api/workspaces?origin=kask_simops` and on the SimOps app's `/api/apps/kask_simops/workspaces` list
3. The user completes a 6-turn intake; each turn appears in the session timeline; a draft `simops/process.yaml` is written
4. The user switches to Compose, edits stages, runs a cascade (slow LLM path for Phase 1, fast direct path in Phase 2), saves a version
5. The user can return tomorrow, find their workspace in the sidebar, and resume editing
6. The workspace budget meter accurately reflects credits spent on agent calls (charged to workspace, not personal wallet)
7. All Process YAML conforms to the `kask-simops/2` schema; no `efficiency_pct` artifacts remain
8. Sharing UI exists (Phase 4) but is not required for Phase 1 sign-off

---

## 11. Open items requiring decisions

- **Slug format for workspace names** — auto-generated (`kask-simops-{ulid}`) vs user-chosen? Recommend auto.
- **Initial budget seed of 250 credits** — confirm or adjust based on how many agent calls a typical full session burns
- **Are existing kask test workspaces cleared, or migrated v1→v2?** Recommend cleared — no real users yet
- **Workspace name pattern** — `SimOps — {date} process` vs let user name it on create? Recommend let user name it, fall back to pattern if blank
- **Compose-mode autosave debounce** — given write-cost, suggest explicit save only; confirm
