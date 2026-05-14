# ABW Capability Brief — Answers for SimOps v2

**Answered by:** codebase inspection of `/home/ilabra/fermi` at `HEAD` (`b06c8ab`).
**Date:** 2026-05-14

---

## Section 1 — Workspace as the "session" container

### 1.1 What is a workspace, structurally?

A workspace **is a team** — there is no separate `workspaces` table.
`migrations/013_workspace_fields.sql` makes this explicit:

```sql
-- Migration 013: Workspace budget fields on teams
-- Every team IS a workspace. No separate table.
ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS workspace_budget INTEGER NOT NULL DEFAULT 0;
ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS workspace_spent  INTEGER NOT NULL DEFAULT 0;
```

The full `teams` schema (`migrations/009_add_teams_and_sharing.sql`):

```sql
CREATE TABLE IF NOT EXISTS public.teams (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,
    description TEXT,
    owner_id    TEXT NOT NULL,          -- users.user_id
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

Additional columns added by later migrations (all on `public.teams`):

| Column | Type | Source | Notes |
|---|---|---|---|
| `workspace_budget` | INTEGER | migration 013 | total allocated credits |
| `workspace_spent` | INTEGER | migration 013 | credits consumed |
| `workflow_mermaid` | TEXT | migration 036 | cached mermaid sequence diagram |
| `workflow_meta` | JSONB | migration 036 | workflow metadata |
| `git_latest_commit` | TEXT | (teams) | sha of last file commit |
| `git_commit_count` | INT | (teams) | total commits |
| `origin` | TEXT DEFAULT `'bestiary_workspace'` | migration 112 | **vertical tag** — see §1.4 |
| `personal_workspace_id` | UUID | users table FK | users table, not teams |

**No `kind`, `type`, or `tags` column on teams today** beyond `origin`. `origin` is the vertical-attribution field (see §1.4).

The Rust struct that comes back from `GET /api/workspaces` is assembled inline in `list_workspaces_handler` (`src/handlers/workspace.rs:46`). It returns:

```json
{
  "id": "uuid",
  "name": "string",
  "slug": "string",
  "description": "string|null",
  "owner_id": "string",
  "budget": 100,
  "spent": 0,
  "origin": "bestiary_workspace",
  "agents": [ { "name": "...", "initial": "...", "type": "..." } ],
  "agent_count": 3,
  "created_at": "ISO8601"
}
```

### 1.2 What does `POST /api/workspaces` accept?

**There is no `POST /api/workspaces`.** That route is GET-only (`src/api_server.rs:1176`).

To create a workspace, call `POST /api/teams`:

```
POST /api/teams
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "SimOps — Process X",
  "slug": "simops-process-x",          // must be unique across all teams
  "description": "optional"
}
```

Handler: `src/handlers/teams.rs:26`. It hardcodes `origin = "bestiary_workspace"`.

To create a SimOps workspace with origin set to `"kask_simops"` (or similar), you currently need to `PUT /api/teams/:id` after creation or patch the handler. The handler does not accept an `origin` parameter in its request body today — you'd need to add that field to `CreateTeamRequest` and pass it through to `create_team()`. One-line change in `teams.rs`.

On successful creation: returns `201 Created` with the `Team` JSON. The workspace is seeded with 100 starter credits automatically.

### 1.3 Personal workspace vs many workspaces

Two separate concepts:

- **`GET /api/me/workspace`** — returns the user's **personal menagerie workspace** (Rabble-specific; created when a user mints their first creature). This is a single auto-created workspace stored in `users.personal_workspace_id`. Not relevant to SimOps. `src/handlers/rabble_workspace.rs:374`.

- **`GET /api/workspaces`** — returns **all teams the authenticated user is a member of** (as owner or as invited member). A user can have arbitrarily many. This is the right list for SimOps: create one workspace per Process, filter by `origin = "kask_simops"`.

**Yes, a user can have multiple SimOps workspaces** — one per Process is the correct model.

### 1.4 Workspace tagging / typing

Option **(1) is supported** via the `origin` column (`migration 112`). It is already filterable server-side:

```
GET /api/workspaces?origin=kask_simops
```

`list_workspaces_handler` applies this filter at `src/handlers/workspace.rs:75`:
```rust
if let Some(ref want) = q.origin {
    if &origin != want { continue; }
}
```

**Recommendation:** use `origin = "kask_simops"` (or any `kask_*` string — migration 112 explicitly reserves the `kask_*` namespace as future external verticals). To write it on creation, add `origin: Option<String>` to `CreateTeamRequest` and pass it through. That's the right server-side filter path.

### 1.5 Workspace messages = session log?

**Yes — this is the correct model.** The messages table (`migration 014`):

```sql
CREATE TABLE IF NOT EXISTS workspace_messages (
    message_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    sender_type  TEXT NOT NULL CHECK (sender_type IN ('user', 'agent', 'system')),
    sender_id    TEXT NOT NULL,    -- user_id or agent_id or "system"
    sender_name  TEXT,
    content      TEXT NOT NULL,
    message_type TEXT NOT NULL DEFAULT 'chat'
                     CHECK (message_type IN ('chat', 'execution_result',
                                             'coherence_update', 'system_event')),
    metadata     JSONB DEFAULT '{}',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

Key answers:

- **Attribution to specific agent:** yes — set `sender_type = "agent"` and `sender_id = <agent_id>`. The `post_workspace_message_handler` accepts this directly via `@agent_name` mention parsing, or you can POST with explicit sender fields.
- **Structured metadata:** yes — `metadata JSONB`. Pass `{ "kind": "insight", "insight_id": "...", "stage_id": "..." }`. No schema enforcement.
- **Message POST shape** (`src/handlers/workspace.rs:552`):
  ```json
  {
    "content": "string",
    "message_type": "chat|execution_result|coherence_update|system_event",
    "metadata": { "any": "json" }
  }
  ```
- **Size limit:** none enforced in the handler beyond Axum's default body limit (~2 MB). No pruning — messages persist indefinitely.
- **Sender attribution when agent responds:** the `post_workspace_message_handler` checks for `@agent_name` mentions, executes the agent, and writes back a message with `sender_type = "agent"`, `sender_id = <agent_id>`. You can also POST directly with `sender_type = "agent"` if you're writing the result yourself.

The `message_to_json` helper (`src/handlers/workspace.rs:584`) produces the wire shape on GET.

### 1.6 Workflow state

`GET /api/workspaces/:id/workflow` returns a **Mermaid sequence diagram** generated from workspace message history, or a pre-saved scaffold injected when a compound agent was hired (`src/handlers/workspace.rs:2737`).

It is **not a generic state machine**. It is a visual summary of the conversation's interaction patterns. Not reusable for SimOps mode tracking in any structural sense.

**Recommendation for SimOps mode state:** store it as `simops/index.yaml` in the workspace file system (§2). The workflow endpoint won't interfere.

---

## Section 2 — File operations (the storage layer)

### 2.1 Confirm the file endpoints work as documented

All handlers live in `src/handlers/workspace.rs`.

**`GET /api/workspaces/:id/files?path=<prefix>`**
- Handler: `list_workspace_files_handler:2211`
- `path` query param is a **path prefix string** — no glob support today
- Returns: `{ "files": [{ "path", "name", "is_dir", "size" }] }`
- No authentication check (auth is on the workspace, enforced by `_principal: AuthPrincipal` — token required, membership not re-checked here)

**`GET /api/workspaces/:id/files/*path`**
- Handler: `read_workspace_file_handler:2243`
- Returns: `{ "path": "simops/process.yaml", "content": "<raw string>" }`
- **No `sha` or `last_modified` in the response** — just `path` and `content`. Binary files return the raw bytes as a UTF-8 string (lossy); use `files-raw` instead.

**`GET /api/workspaces/:id/files-raw/*path`**
- Handler: `read_workspace_file_raw_handler:2267`
- Returns: raw bytes with inferred `Content-Type` from extension (`.yaml` → `application/octet-stream`, `.md` → `text/plain`, `.json` → `application/json`, etc.)
- Use for binary files (images, PDFs)

**`PUT /api/workspaces/:id/files/*path`**
- Handler: `write_workspace_file_handler:2313`
- Request body: `{ "content": "string", "is_base64": false, "message": "optional commit message" }`
- **Every write creates a git commit.** Author is the user_id from the auth token.
- Returns: `{ "path", "commit": { "sha", "message", "timestamp" } }`
- Cost: charges `gas_fees.file_write` credits from the workspace wallet

**`POST /api/workspaces/:id/upload`**
- Handler: `upload_workspace_file_handler:2452`
- Multipart form — field name is `file` (or any field with a filename)
- **Limit is 5 MB** (`MAX_UPLOAD_SIZE = 5 * 1024 * 1024`) — not 6 MB
- Blocked extensions: `.exe .sh .bat .dll .so .cmd .ps1`
- Returns: `{ "uploaded": [{ "path", "size", "commit_sha" }] }`

### 2.2 Append semantics for JSONL?

**No append mode exists.** The write endpoint is PUT (full overwrite).  
**No optimistic locking / `If-Match: sha`.** There is a `sha` in the PUT response but no `If-Match` precondition check on write.

**Recommendation:** use **option (3) — one file per record** under `simops/insights/`. Put each insight as `insights/<ulid>.yaml` or `insights/<timestamp>-<hash>.json`. No race condition, no read-modify-write. Simpler for multi-agent concurrent writes. The `list_files?path=simops/insights/` endpoint gives you the full list cheaply.

### 2.3 Git endpoints

**`GET /api/workspaces/:id/git/log?limit=N`** (`workspace_git_log_handler:2381`):
```json
{
  "commits": [
    {
      "sha": "abc123",
      "message": "user@example.com updated simops/process.yaml",
      "timestamp": "ISO8601",
      "author": "user@example.com"
    }
  ]
}
```
No `changed_files` per commit — just sha/message/timestamp/author.

**`GET /api/workspaces/:id/git/diff?from=<sha>&to=<sha>`** (`workspace_git_diff_handler:2420`):
```json
{ "from": "sha1", "to": "sha2", "diff": "<unified diff text>" }
```
- Both `from` and `to` are **required** query params — no default HEAD
- Returns full unified diff for the workspace tree between those two commits
- No single-file filter, no per-path scoping
- Per-workspace only — no cross-workspace or arbitrary ref support

**Tagging / branches:** no tag or branch creation endpoints exist. Workspace git is a single linear branch (master/main). Named versions: use a dedicated file (`simops/versions.yaml`) or store the sha in your own records.

### 2.4 Filename-derived structure

ABW reserves no directories inside workspace git repos. There are no hooks on specific paths. The only special directory is `context/` — the `load_workspace_context` helper reads from `context/*.md` to inject into agent prompts — but this is a convention, not enforced.

**No conflict with `simops/`.** Go ahead. The `agents/` and `notebooks/` paths inside workspaces are not managed by ABW.

---

## Section 3 — Agent execution

### 3.1 Sync vs stream — exact shapes

**`POST /api/agents/:agent_id/execute`** (`src/handlers/execution.rs:33`):

Request:
```json
{ "query": "string" }
```

Response (`execution.rs:225`):
```json
{
  "agent_id": "string",
  "episode_id": "uuid",
  "status": "Success|Failure|Partial",
  "confidence": 0.85,
  "execution_time_ms": 1200,
  "tokens_used": 840,
  "credits_charged": 3,
  "loop_iterations": 2,
  "tool_invocations": [
    { "tool_name": "string", "duration_ms": 120, "iteration": 1 }
  ],
  "evidence": [
    { "id": "string", "source": "string", "summary": "string",
      "key_findings": "string", "relevance": 0.9 }
  ],
  "metadata": {
    "model_used": "claude-haiku-4-5",
    "reasoning": "string"
  }
}
```

No `context` field — context is not in the request body. Pass everything through `query`.

**`POST /api/agents/:agent_id/execute/stream`** (`src/handlers/execution_stream.rs`):

Request: `{ "query": "string" }` (same)

SSE stream events (each is `data: <json>\n\n`):
- `{ "event": "started", "agent_id": "...", "agent_name": "..." }`
- `{ "event": "progress", "phase": "executing|tool_call|...", "message": "..." }`
- `{ "event": "evidence", "source": "...", "summary": "...", "key_findings": "..." }`
- `{ "event": "complete", <full response JSON same shape as sync> }`
- `{ "event": "error", "message": "...", "agent_id": "..." }`

Stream terminates after `complete` or `error`. The client knows it's done when it receives one of those two events. Granularity: tool-call level — not token-by-token.

**`POST /mcp/agents/:agent_id`**: JSON-RPC 2.0. Only method is `execute`. Body: `{ "jsonrpc": "2.0", "method": "execute", "params": { "query": "string" }, "id": 1 }`. Response wraps the same payload as the sync endpoint.

### 3.2 Workspace-scoped execution

**When an agent is hired into a workspace via `POST /api/workspaces/:id/hire`**, subsequent `@mention` messages in the workspace chat automatically:
- inject workspace file context (reads `context/` directory)
- charge the **workspace budget** (not user personal balance) — `charge_workspace_gas` in `workspace.rs:519`
- write result back as a workspace message with `sender_type = "agent"`

However, **direct calls to `POST /api/agents/:agent_id/execute` always charge the user's personal wallet** — not the workspace. To get workspace-budget charging, you must go through the workspace message path.

To invoke an agent inside a workspace context:
```
POST /api/workspaces/:workspace_id/messages
{ "content": "@simops_cascade analyse this process", "message_type": "chat" }
```
Or to run it with full control over the query and avoid SSE lag, post a message and let the handler detect the @mention and execute inline. The result comes back as a workspace message.

There is no `POST /api/workspaces/:id/execute/:agent_id` endpoint today.

### 3.3 Per-call cost and budget enforcement

- Cost is determined in the execution handler by `charge_gas()` / `credit_charge()` — the amount is `agent.cost_credits` from the DB, defaulting to a small fixed amount (typically 1–5 credits).
- **Yes — `credits_charged` is in the response** (see §3.1 shape above).
- When a user wallet hits 0: `402 Payment Required` with body `"Insufficient credits"` (`execution.rs:56`).
- Workspace budget depletion: also `402` from `charge_workspace_gas`.
- **Read remaining budget:** `GET /api/workspaces/:id` returns `{ "budget": 100, "spent": 40 }` — remaining = budget − spent.

### 3.4 Episode log

`/api/agents/:agent_id/episodes` and `/api/episodes/:episode_id` are the **per-agent episodic memory store** — think of it as the agent's long-term memory of all its past runs, used by the dreaming/consolidation cycle. Each episode stores: query, context, execution result, embedding, provenance, persona_version.

For a "what did the agent do" inspector: yes, this is what you want. Each execute call creates an episode. Query by `agent_id` to get that agent's run history. The `episode_id` returned in every execute response is the primary key.

---

## Section 4 — SimOps agents specifically

### 4.1 Schema contract for `simops_advisor`

The advisor is **stateless** at the API level — it has no memory of prior turns built into the endpoint. To keep 6-turn dialogue stateful, you must pass state in each `query` call.

Current system prompt indicates it builds a ProcessConfig as a "by-product" of a structured conversation. The agent reads `process_json` from its tool context if passed.

Recommended `query` envelope for turns 2+:
```json
{
  "query": "Turn 3: the user said X. Prior turns: [turn1_summary, turn2_summary]. Current partial ProcessConfig: <yaml>. Continue the dialogue."
}
```
Or better: store turn history in `simops/advisor_turns.jsonl` (one file per turn, §2.2 recommendation), then pass the last N turns' content in the query. The agent does **not** automatically read workspace files unless you write tools for it.

The simplest pattern for SimOps v2: accumulate turns in your client state, paste the full conversation into each `query`, and parse the agent's structured output (ProcessConfig JSON/YAML) from the `metadata.reasoning` field of the response.

### 4.2 `simops_cascade` invocation

**Not deterministic in the pure API sense** — the cascade agent uses the `simops` Rust crate deterministically under the hood (`src/agent_backend/simops_tools.rs`) but it's wrapped in an LLM agent (no `executor_type` set, defaults to LLM). The LLM calls the deterministic cascade tools.

For very low latency you can call the Rust functions directly by adding a thin endpoint. The tools are all registered in `ToolRegistry` already. Today the path is:

```
POST /api/agents/simops_cascade/execute
{
  "query": "Forward cascade this process: <process_json>"
}
```

Input to pass in query: the full `ProcessConfig` JSON (or TOML). The agent's system prompt shows it also accepts `process_name` for built-in configs. `input_quantity` and `target_output` are accepted.

Output shape from `cascade_forward` (`simops_tools.rs`): per-stage results with `input_quantity`, `output_quantity`, `efficiency`, `carbon_delta_kg`, `opex_usd`, plus aggregate `total_carbon_delta_kg`, `total_opex_usd`, `ner`, `sec`.

**For sub-100ms latency:** add `POST /api/simops/cascade` calling `cascade_forward`/`cascade_backward` directly from the Rust crate with no LLM in the loop. That's the right call for Compose mode's real-time stage-edit refresh. Worth adding.

### 4.3 `simops_predictor` & `simops_optimizer`

**Predictor** (`agents/curated/simops_predictor`): globally trained per-session using SOSA observations stored in the DB. Feed it with `simops_fetch_training_data` tool which queries `sosa_observations` for the session. Needs observations to be useful — minimum ~10 for a stable OLS fit. Observations come from `simops_write_observation` calls during real process runs or from kask's experiment data.

**Optimizer** (`agents/curated/simops_optimizer`): takes `target_output` + a `ProcessConfig` reference and returns required input quantities. Uses `single_input_solve` (one free variable) or `scale_from_reference` (proportional scale). Input shape: pass `target_output` (float), `process_json` or `process_name`, and optionally a trained predictor reference. Output: `OptimizationResult` with `input_quantity`, `output_quantity`, `efficiency_factor`, `cost_per_unit_output`.

### 4.4 `supply_chain_oracle`

Invoked via standard execute endpoint. Accepts `accepts: ['bom_items_json', 'ingredient_name_list', 'free_text_ingredient_query']` — pass any of these in the query. Example:

```
POST /api/agents/supply_chain_oracle/execute
{ "query": "[{\"item\": \"ashwagandha root extract\", \"qty_kg\": 50}, {\"item\": \"kombucha SCOBY\", \"qty_kg\": 5}]" }
```

Returns: mid-market price per unit + supply risk flags per item. Shape is in `metadata.reasoning` as structured JSON.

### 4.5 Is there a `sidestream_miner` agent yet?

**Does not exist** in `agents/curated/`. The closest existing agents:
- `supply_chain_oracle` — covers BOM pricing and supply risk
- `entity_investigator` — maps relationships and supply chain networks
- `macro_forecaster` — market trends for inputs/outputs

**None of these is a sidestream miner.** `product_scout`, `regulatory_scanner`, `valuechain_mapper`, `comparator`, `marketing_composer` — **none exist**.

Recommend proposing all of these as new bestiary additions. `sidestream_miner` and `comparator` are the most SimOps-critical gaps.

### 4.6 The canonical ProcessConfig schema

**No example YAML or JSON fixture file exists in the repo.** The only examples are inline Rust test data in `crates/simops/src/process.rs:tests`.

The canonical Rust schema from `crates/simops/src/process.rs`:

```rust
ProcessConfig {
  name: String,
  description: Option<String>,
  feature_of_interest: Option<String>,  // SOSA URI
  stages: Vec<Stage>,
  elec_price_per_kwh: Option<f64>,
  maintenance_cost_usd: Option<f64>,
}

Stage {
  id: String,
  efficiency: f64,           // NOT efficiency_pct — fraction 0..1
  carbon_intensity: f64,     // kg CO2-eq / kg output; negative = sequestration
  input: Resource,
  output: Resource,
  capex: Option<CapexProfile>,
  opex_per_input_unit: Option<f64>,
}

Resource {
  name: String,
  unit: String,              // "kg", "kWh", "L"
  energy_density: Option<f64>,
  density_unit: Option<String>,  // "kcal/g", "kWh/kg", "MJ/kg"
}
```

**Recommendation: kask should migrate to the Rust shape.** The Rust crate is the compute engine — if kask sends `efficiency_pct: 85` but the crate expects `efficiency: 0.85`, every cascade call is wrong by 100×. `sidestreams[]` and `sensors[]` should be added to the Rust struct, not maintained in parallel. Propose a `ProcessConfig v2` PR that adds those fields as `Option<Vec<Sidestream>>` and `Option<Vec<Sensor>>` so the Rust engine can handle them. This gives you a single source of truth, serialisable to/from YAML, passable directly to all simops tools.

---

## Section 5 — FPL & forecasts

### 5.1 `POST /api/fpl/execute` — the killer endpoint

Handler: `src/handlers/notebooks.rs::fpl_execute_handler`.

**Request:**
```json
{
  "fpl_source": "question \"will X happen\" ~ 0.3\ndriver market_size ~ triangular(80, 120, 200)\nmodel market_size",
  "iterations": 10000,   // optional, default 10000, max 100000
  "seed": 42             // optional uint32 for reproducible results
}
```
Source text only — no AST, no notebook ID.

**Response:**
```json
{
  "mean": 0.512,
  "median": 0.498,
  "std_dev": 0.142,
  "p5": 0.241,
  "p25": 0.401,
  "p75": 0.618,
  "p95": 0.791,
  "min": 0.001,
  "max": 0.999,
  "base_rate": 0.3,          // null if no base_rate in FPL
  "divergence_relative": 0.71,   // null if no base_rate
  "divergence_absolute": 0.212,  // null if no base_rate
  "iterations": 10000,
  "execution_time_ms": 45,
  "credits_charged": 11          // 1 base + 10 for 10k iterations
}
```

The engine returns **distributional statistics** — mean, median, std_dev, percentiles. No sample arrays. One output (the model output) per run.

**Latency:** sub-100ms for 10K iterations locally. On Railway, expect 50–200ms including network. Well under a second.

**Inject variables:** pass them inline in the FPL `fpl_source`. FPL supports named drivers and scenario parameters as numeric literals — override them per scenario by generating different FPL source text with different driver values. There is no variable-injection API; generate the FPL string with your scenario overrides embedded.

**Credits:** yes — metered at 1 credit base + 1 per 1000 iterations. A 10K run costs 11 credits.

**For NPV distribution:** write an FPL program with drivers for revenue, cost, discount rate, etc. and a model expression that computes NPV. Pass the process YAML parameters as driver literal values in the generated FPL. The engine runs Monte Carlo and returns the distribution of NPV.

### 5.2 Notebooks vs `/api/fpl/execute`

Correct framing confirmed:
- `POST /api/notebooks/:id/execute` — runs a **persisted** notebook (cells stored in DB, converted to FPL, then executed)
- `POST /api/fpl/execute` — **stateless one-shot** execution from raw FPL source text

For SimOps v2: use `/api/fpl/execute`. Notebooks are legacy. Generate FPL source programmatically from your ProcessConfig + scenario parameters and POST it directly.

### 5.3 Forecasts as persistence

`CreateForecastRequest` (`src/handlers/forecasts.rs:32`):
```json
{
  "question_text": "Will Experiment A NPV exceed €500K?",
  "predicted_probability": 0.72,
  "domain": "simops",
  "resolution_criteria": "NPV > 500000 EUR at project end",
  "target_date": "2026-12-31T00:00:00Z",
  "fpl_source": "...",
  "simulation_results": { "mean": 0.72, "p5": 0.45, "p95": 0.91, ... },
  "drivers": [ ... ],
  "evidence": [ ... ],
  "visibility": "private|shared|public",
  "tags": ["simops", "experiment-a", "scenario-baseline"],
  "portfolio_id": "uuid",    // auto-add to portfolio
  "status": "draft|active"
}
```

**Arbitrary metadata:** the `fermi_forecasts` table has `metadata JSONB NOT NULL DEFAULT '{}'::jsonb` but `CreateForecastRequest` does not expose it. You can store `simops_experiment_id`, `scenario_id`, `process_version` in `simulation_results` (which is JSONB and fully passthrough) as a workaround today.

**Link to workspace:** no `workspace_id` field on forecasts. Workaround: tag with the workspace UUID as a tag, e.g. `tags: ["workspace:uuid"]`, then query by tag. Or store the forecast ID in `simops/experiments/<id>.yaml` in the workspace.

### 5.4 Forecast schedules — the continuous-discovery primitive

Schema (`migration 109`):
```sql
fermi_forecast_schedules:
  id, forecast_id, agent_id, driver_name, query,
  interval_hours INT,   -- e.g. 168 = weekly
  last_run_at, next_run_at, enabled
```

**Upsert shape** (`PUT /api/forecasts/:id/schedules`):
```json
{
  "agent_id": "simops_cascade",
  "driver_name": "cultivation_stage",
  "query": "Re-run the cultivation stage cascade with latest observations",
  "interval_hours": 168
}
```

**What happens on a scheduled run:** the schedule table records `next_run_at`. There is **no server-side scheduler** — no cron worker, no background process. The thick client (Fermi console) reads schedules on load and fires agents manually. `POST /api/forecasts/:id/schedules/:schedule_id/run` is a manual trigger that records `last_run_at` and advances `next_run_at`. **Nothing fires automatically server-side today.**

Implications for SimOps v2: option (3) — you run a client-side timer (or cron job on kask's side) that polls `next_run_at` and calls the run endpoint when due. Server-side scheduling is planned but not built.

**Are results logged?** Not automatically. The `record_schedule_run_handler` only updates timestamps. You'd need to call the agent separately and post the result as a workspace message or forecast update yourself.

### 5.5 Portfolios

Confirmed useful for SimOps. A portfolio is a named collection of forecasts. Create one portfolio per Experiment, add each scenario's forecast to it. `GET /api/portfolios/:id/forecasts` returns all scenarios in one call with their probabilities. Portfolio stats endpoint gives aggregate calibration/Brier across the set.

### 5.6 Public forecasts

`fermi_forecasts.visibility TEXT CHECK (visibility IN ('private', 'shared', 'public'))`. Set at creation or via `PUT /api/forecasts/:id`. `GET /api/forecasts/public` returns all forecasts with `visibility = 'public'`. No separate publication action.

---

## Section 6 — Sharing, ACL, publication

### 6.1 `POST /api/shares` — the generic ACL primitive

`ObjectType::from_str` accepts (`fermi-auth/src/types.rs:134`):

```
"agent" | "capability" | "forecast" | "index" | "repo" | "file"
```

**`"workspace"` is NOT valid today.** The DB constraint is:
```sql
CHECK (object_type IN ('agent', 'capability', 'forecast', 'index', 'repo', 'file'))
```

To add `"workspace"`: change the enum in `fermi-auth/src/types.rs`, update `from_str`, update the DB constraint in a new migration. The migration needs to `DROP CONSTRAINT` and re-add it with the new value set. Two-line code change + one migration.

**`GET /api/shares?object_type=X&object_id=Y`:** does not exist. There is no list-shares endpoint. The only share read path is implicit: `teams::get_user_teams` returns workspaces the user is a member of.

**How a shared workspace appears:** shared workspaces appear in the sharer's `GET /api/workspaces` list because `get_user_teams` returns all teams where `team_members.member_id = user_id`. When you add a user as a team member, the workspace appears in their list automatically. There is no "shared with me" filter today.

### 6.2 Teams

From `migration 009`:
- Shape: `{ id, name, slug, description, owner_id, created_at, updated_at }`
- Member roles: `owner | admin | member | viewer`
- **Flat membership only** — no nested teams
- Owner can be a user. A workspace's `owner_id` is a `user_id` string, not a team. Both user-owned and team-owned (conceptually) are supported, but the schema makes the creator the owner.

### 6.3 Publication

**No generic workspace-publish primitive exists.** The closest patterns:

- **Agent publish:** `POST /api/agents/:id/publish` — sets `visibility = "public"` and writes a version snapshot. Source: `src/handlers/lifecycle.rs`.
- **Forecast public:** set `visibility = "public"` on the forecast resource.

For a SimOps Process publication (stable public URL, slug, fork tracking, read-only access, citation block):

The minimal viable path today: set `teams.origin = "kask_simops_public"` and rely on a kask-controlled public listing endpoint that queries workspaces by origin. ABW has no concept of a workspace public URL yet.

**To add properly:** add a `published_at TIMESTAMPTZ` and `public_slug TEXT UNIQUE` to `teams`, expose `GET /api/workspaces/published/:slug` as a read-only endpoint. The pattern is identical to agent publication. Estimated: 1 migration + 2 handler functions.

### 6.4 Fork tracking

`POST /api/agents/:id/fork` exists (sets `forked_from` UUID on the new agent). **No fork primitive on workspaces or processes.** The `teams` table has no `forked_from` field. Would need a new migration + handler to add it.

---

## Section 7 — Activity feed & observability

### 7.1 `/api/feed/events` (write) and `/api/feed/stream` (SSE)

`GET /api/feed/events` is a **read** endpoint (paginated activity feed), not write. There is no `POST /api/feed/events` to write events — the feed is populated by server-side triggers (social events, creature actions).

Query params on `/api/feed/events`: `{ "limit": 50, "before": "ISO8601" }`. Filters by the authenticated user's social graph (their creatures, contacts, joined rabbles). Not per-workspace.

`GET /api/feed/stream` SSE format — each event is:
```
data: {"event_id":"uuid","event_type":"string","actor_user_id":"...","title":"...","body":"...","metadata":{...},"created_at":"ISO8601",...}
```
Sub-second push: poll-based internally (5s tick), so effectively 0–5s latency. Events are not deduplicated or replayable beyond the `?since=<ISO8601>` backfill param.

**This feed is social/creature-centric, not workspace-centric.** Not directly useful for SimOps session logging. Use workspace messages for that (§1.5).

### 7.2 Notifications

Triggered by server-side `create_notification()` calls in handler code — not by feed events and not by a rule engine. Clients **cannot create notifications** via API today (no `POST /api/notifications`).

Shape (from `create_notification` call pattern):
```json
{
  "notification_id": "uuid",
  "user_id": "string",
  "type": "string",    // e.g. "agent_complete", "low_balance"
  "title": "string",
  "body": "string",
  "metadata": {},
  "read": false,
  "created_at": "ISO8601"
}
```

To surface SimOps-specific notifications ("your overnight discovery pass found 3 new sidestreams"): you'd need to call `create_notification()` from a server-side handler after the agent run. Since kask has no server-side process today (schedule execution is client-side), you can't trigger this automatically yet. Add it when you add server-side scheduling.

### 7.3 Observability suite

| Endpoint | What it does | SimOps reuse? |
|---|---|---|
| `GET /observatory/agents/:id/timeline?window=N` | Per-agent scored episode history, drift metrics, dimension trend | Yes — "what did this agent do" inspector for any simops agent |
| `GET /observatory/agents/:id/dyads` | Per-(agent,user) rapport/trust/reciprocity | Low value for SimOps |
| `GET /observatory/agents/:id/anomalies?limit=N` | Flagged anomaly events (drift, safety, conflict) | Useful for alerting when a simops agent drifts |
| `POST /observatory/agents/:id/scan` | Trigger observability worker for this agent | Admin-useful |
| `GET /observatory/hitl` | HITL review queue (agent owner or platform admin) | Owner-accessible, not admin-only |
| `POST /observatory/hitl/:event_id/action` | Approve / relabel / intervene on anomaly | Owner-accessible |

**All routes are owner-or-admin** — if the kask team owns the simops agents in the bestiary, these endpoints are fully usable for a "what the agent did" panel. The timeline endpoint is particularly useful: it returns per-dimension scores (relevance, accuracy, persona_consistency) per run alongside drift_norm and provenance.

---

## Section 8 — Streaming, async, long-running

### 8.1 SSE format consistency

All SSE endpoints push `data: <json>\n\n` — they share the same low-level format but the JSON payloads differ per domain:

| Endpoint | Event shape |
|---|---|
| `/api/workspaces/:id/messages/stream` | `{"message_id":...,"sender_type":...,"content":...,"created_at":...}` |
| `/api/agents/:id/execute/stream` | `{"event":"started|progress|evidence|complete|error",...}` |
| `/api/feed/stream` | `{"event_id":...,"event_type":...,"title":...,"body":...}` |
| `/api/creatures/:id/stream` | Creature presence/state events (Rabble-specific) |
| `/api/rabble/:id/stream` | Flock event stream (Rabble-specific) |

All use `Event::default().data(json_string)` with no named event type (no `event: name` line). The `"event"` field is inside the JSON payload, not a SSE `event:` directive.

### 8.2 Long-running agent runs

The sync `POST /api/agents/:id/execute` **holds the HTTP connection open** for the full duration. No timeout is set in the handler — Axum's default connection timeout applies (Railway's load balancer may close at 30–60s).

For 60–120 second runs: **use the stream endpoint** (`/execute/stream`). It keeps the SSE connection alive and pushes progress events during execution. The `complete` event arrives when done.

**Recommended pattern for kask:** stream from the start. Open the SSE connection, render progress events in the UI, close when `complete` arrives.

### 8.3 Cancellation

**No cancellation endpoint exists.** Once `execute` or `execute/stream` is called, the run cannot be cancelled via API. Credits are charged whether or not the client disconnects. If the client closes the SSE connection, the server-side `tokio::spawn`'d task continues to completion.

---

## Section 9 — Background / scheduled work

### 9.1 Beyond forecast schedules, is there a general job queue?

**Option (3) — nothing exists server-side for general scheduling.** The `fermi_forecast_schedules` table stores schedule metadata and fires on demand when `record_schedule_run_handler` is called, but there is no background cron worker reading `next_run_at` and auto-firing.

For SimOps continuous discovery: kask must run its own scheduler (client tab timer, external cron, or a kask server process) that:
1. Reads schedules from `/api/forecasts/:id/schedules`
2. Calls `POST /api/forecasts/:id/schedules/:schedule_id/run` when `next_run_at` is past
3. Then calls `/api/agents/:agent_id/execute` with the scheduled query
4. Posts the result back as a workspace message

This is viable today. Add server-side scheduling to the ABW roadmap as a Phase 9 item.

### 9.2 What happens server-side after a schedule fires?

Nothing automatic. `record_schedule_run_handler` only updates `last_run_at` and advances `next_run_at` by `interval_hours`. No agent execution, no notifications, no feed events are triggered automatically. All execution is client-initiated.

---

## Section 10 — Things we don't know we don't know

### 10.1 Pending work

Recent commits touching SimOps-relevant surfaces:
- `b06c8ab` (today): admin agent-ownership audit/reassign endpoints — curated agents now correctly owned by sys-admin, not random users
- `e547bb8`: Railway build fix (Dockerfile/nixpacks)
- `8780480`: Phase 5+6+8 observability + console UX + `/api/fpl/execute` added
- `42480b0`: Dockerfile fix

Nothing in-flight currently touching: workspaces, sharing, publication, FPL core, SimOps agents, file API.

The `origin` column on teams (`migration 112`) landed recently — that's the right hook for kask workspace tagging.

### 10.2 Known limits, gotchas, footguns

1. **File write charges credits.** Every `PUT /api/workspaces/:id/files/*path` charges `gas_fees.file_write` from the workspace wallet. For SimOps with frequent state writes (every stage edit), this adds up. Budget accordingly or consider batching writes.

2. **No `sha` in file read response.** The GET file endpoint returns `{ path, content }` only. You cannot implement optimistic locking without adding sha to the response. Use one-file-per-record to avoid the race.

3. **Workspace budget must be pre-funded.** New workspaces get 100 starter credits. Agent runs and file writes burn these. If the workspace runs out, all workspace-charged operations return 402. Ensure your SimOps workspace creation flow adds sufficient budget via `POST /api/workspaces/:id/budget`.

4. **`simops_cascade` goes through LLM.** Even though the underlying math is deterministic, the agent wraps it in an LLM call. For real-time Compose mode updates (every stage edit), this is too slow. Add a direct `POST /api/simops/cascade` endpoint.

5. **SSE agent stream has no heartbeat.** If a run takes >30s with no events, proxies may close the connection. Railway's load balancer has a 60s idle timeout. For long agents, send periodic `progress` events.

6. **`/api/workspaces` returns all teams including Rabble swarms.** Filter by `origin` to avoid listing Rabble workspaces in your SimOps UI.

7. **`simops_predictor` needs training data.** Zero observations = meaningless predictions. Gate the predictor UI behind "N observations collected" check.

8. **No file size limit for reads.** Large files (e.g. a fat `insights/` directory read as tree) will load the entire git tree. Keep `simops/` shallow or paginate with `path` prefix filter.

9. **Workspace slug must be globally unique.** If kask creates workspaces programmatically, use a UUID-based slug like `kask-{user_slug}-{ulid}` to avoid collisions.

10. **The `metadata` field on `fermi_forecasts` is not exposed in `CreateForecastRequest`.** Store your `simops_experiment_id` in `simulation_results` or `tags` for now.

### 10.3 Existing kask integrations on the fermi side

From a grep of `kask` in the repo:

- `migrations/112_workspace_origin.sql` — explicitly reserves `kask_*` namespace in the `origin` field
- `agents/curated/simops_advisor/agent_card.json` — tagged `"kask"`, accepts `kask`-style context
- `agents/curated/simops_cascade/agent_card.json` — tagged `"kask"`
- `agents/curated/simops_optimizer/agent_card.json` — tagged `"kask"`
- `agents/curated/simops_predictor/agent_card.json` — tagged `"kask"`
- `agents/curated/simops_narrator/agent_card.json` — tagged `"kask"`
- `agents/curated/supply_chain_oracle/agent_card.json` — tagged `"kask"`
- `agents/curated/bioreactor_modeler/agent_card.json` — tagged `"kask"`
- `agents/curated/adaptogen_curator/agent_card.json` — deep kask-specific system prompt referencing the `kask_adaptogen_db` MCP server

The `adaptogen_curator` has an MCP server reference (`kask_adaptogen_db`) — this appears to be a kask-side MCP server not yet wired into ABW. Worth clarifying whether this MCP server is live and whether ABW should proxy it.

### 10.4 Recommended OpenAPI / SDK story

`/openapi.json` does not exist — confirmed. No spec generation is planned.

**Recommendation:** generate a TypeScript client from the route list. The axum router is not annotated with `utoipa` or `aide`, so auto-generation would require adding proc macros. Pragmatic near-term approach:

1. Maintain a hand-written `openapi.yaml` in `docs/` for the subset of routes kask uses — it's ~20 routes, not 200.
2. Generate a typed TypeScript client with `openapi-typescript` from that YAML.
3. Keep it in sync with a linting step that checks the handler shapes against the spec in CI (manual but fast).

A shared Rust types crate (`agent-bestiary-api-types`) exporting `serde`-annotated request/response structs and compiled to WASM/TS via `tsify` would be the cleanest long-term path, but is a significant investment.

---

## Section 11 — Decisions we need to make in this round

1. **Workspace-per-Process vs Process-as-file-in-a-shared-workspace.**
   **Recommend: workspace-per-Process.** You get budget isolation, agent roster, message history, and git-backed file storage per Process for free. The `/api/workspaces?origin=kask_simops` filter makes the sidebar clean. A shared workspace would require you to build your own ACL on top of files.

2. **Append-only JSONL vs file-per-record for insights.**
   **Recommend: file-per-record under `simops/insights/<ulid>.yaml`.** No read-modify-write, no race conditions, cheap list via `GET /files?path=simops/insights/`. JSONL append requires a lock the file API doesn't provide.

3. **Workspace tagging mechanism to mark a workspace as "SimOps".**
   **Recommend: `origin = "kask_simops"`** (already reserved). Add `origin: Option<String>` to `CreateTeamRequest`, pass `"kask_simops"` on creation, filter on `GET /api/workspaces?origin=kask_simops`. One-line change on the ABW side.

4. **Sync vs stream for agent calls in the SimOps UI.**
   **Recommend: stream by default.** Agent runs are typically 5–30s. Stream keeps the UI live (progress events), avoids proxy timeouts, and the `complete` event has the same payload as sync. Use sync only for the `simops_cascade` direct endpoint once that exists.

5. **Where to persist Experiment results.**
   **Recommend: both.** Store rich result detail in `simops/experiments/<id>.yaml` in the workspace (cheap, local, offline-capable). Also create a `Forecast` object per scenario for leaderboard/Brier/portfolio features. Link them via the workspace tag workaround until a proper `workspace_id` field is added to forecasts.

6. **Schema source of truth for ProcessConfig.**
   **Recommend: migrate kask to the Rust crate shape.** `efficiency` not `efficiency_pct`, add `sidestreams` and `sensors` as optional fields to the Rust struct in a PR. The compute engine must be authoritative — mismatches produce silently wrong physics.

7. **Sharing object type for workspaces.**
   **Recommend: extend `ObjectType` to include `"workspace"`.** One migration + one enum variant. This is the correct primitive. Sharing at the file level is too granular for SimOps; sharing at the workspace level maps cleanly to "share this Process with a colleague."

8. **Publication mechanism.**
   **Recommend: add `POST /api/workspaces/:id/publish` that sets a `public_slug` and `published_at` on the teams row.** Mirror the agent publication pattern exactly. This is a 1-migration + 2-handler addition. The `POST /api/shares` with `share_type: "public"` workaround is architecturally messy — publication is a first-class action, not just a share.
