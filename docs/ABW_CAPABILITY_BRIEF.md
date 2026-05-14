# Brief for ABW codebase agent — SimOps v2 unblocking questions

**Reader:** an agent (or engineer) with the `fermi` repo (the ABW backend) checked out at `/home/ilabra/fermi`.
**Purpose:** answer the questions below so the kask team can finalise the SimOps v2 design without making assumptions about ABW's surface.
**Tone of answers:** terse, factual, cite `file:line` whenever you can. If something doesn't exist, say "does not exist" — that's a useful answer.
**Response format:** copy the headings below verbatim and fill in answers underneath. Don't restructure.

---

## Context (so you know why we're asking)

The kask project is building **SimOps v2** — a four-mode workspace (Intake / Compose / Scenarios / Experiments) for designing process pipelines, comparing scenarios, and running forecasts. We've decided to build it as a **thin client on top of ABW** rather than a parallel stack — kask is the project-specific UX, ABW is the platform (identity, workspaces, files, agents, FPL, forecasts, sharing, observability).

We've already mapped most of the ABW surface from `src/api_server.rs`. What we need from you are the answers we can't reliably extract by reading the route list alone: handler-level shapes, behaviour, what's wired vs stubbed, and what's missing.

Where the answer is "look in this file," paste the relevant 5-20 lines so the kask side doesn't need to grep further.

---

## Section 1 — Workspace as the "session" container

We plan to use **one ABW workspace per SimOps Process** (the canonical thing the user is modelling). The workspace gives us, for free: chat log (= conversation history), agent roster, budget, git-backed file storage, and SSE message streaming. We want to confirm that's the right mental model.

### 1.1 What is a workspace, structurally?
File: `crates/<workspace-related>/...` or wherever the `Workspace` struct/table lives.
Give us:
- The struct definition (Rust)
- The DB schema (migration file path + the relevant `CREATE TABLE`)
- Fields, especially: name, description, owner, visibility, type/kind, tags/metadata, created_at, any `kind`/`category`/`tags` column we could use to mark a workspace as a "simops" workspace

### 1.2 What does `POST /api/workspaces` accept?
Handler: `handlers::workspace::list_workspaces_handler` *(wait — that's GET; we mean the create handler)*.
Looking at `src/api_server.rs:1173-1175` only the GET is mounted explicitly. **Is there a separate create endpoint?** If creation happens implicitly (e.g. via hire/add or `/api/me/workspace`), explain how. We need to know: "to create a SimOps workspace named X, the kask client should call _____ with body _____".

### 1.3 Personal workspace vs many workspaces
The route `/api/me/workspace` (singular) at line 1283 suggests every user has a single personal workspace. The other `/api/workspaces` routes are plural. Are these two different concepts, or does `/api/me/workspace` just return the user's default/most-recent one? We need to know if a user can have multiple SimOps workspaces (we hope yes — one per Process they're working on).

### 1.4 Workspace tagging / typing
Can a workspace be tagged or typed so kask can filter "my SimOps workspaces" vs others in the sidebar? Options we're considering, ranked by preference:
1. A `kind` / `type` / `tags` column on the workspace row (best — server-side filter)
2. A convention like a marker file `simops/.kind` inside the workspace (works, but requires fetching file lists to filter)
3. Putting all SimOps state under a `simops/` subdirectory and treating any workspace containing it as "a SimOps workspace" (same as 2)

Which is actually supported today, and if (1), what's the column name / endpoint param to set it?

### 1.5 Workspace messages = session log?
Endpoints: `POST/GET /api/workspaces/:id/messages`, `/poll`, `/stream`.
- What's the message shape? (Sender: user vs agent vs system? Timestamps? Attachments? Message types?)
- Can a message be attributed to a *specific* agent_id when the message is an agent response?
- Can we attach structured metadata to a message (e.g. `{kind: "insight", insight_id: ..., stage_id: ...}`)? If so, where does it live (a JSON column? message_type enum?).
- Is there a message size limit?
- Are messages persisted forever, or pruned?

This matters because we want to log every wizard turn, every "What else?" agent run, and every experiment outcome as workspace messages — so the session view is just a filtered render of `/messages`.

### 1.6 Workflow state
Endpoint: `GET /api/workspaces/:id/workflow`.
What is this? Is it a generic state machine, a specific workflow type (composition, hire, etc.), or something else? Could SimOps reuse it to model the user's current mode (Intake / Compose / Scenarios / Experiments)?

---

## Section 2 — File operations (the storage layer)

We plan to store all SimOps-specific state under `simops/` inside the workspace:
```
simops/
  process.yaml          # canonical ProcessConfig
  scenarios/*.yaml      # parameterised variants
  experiments/*.yaml    # A/B run results
  insights.jsonl        # append-only agent surfacings
  decisions/*.md        # human rationale
  budget.yaml           # continuous-discovery config
  marketing/*.yaml      # claim/evidence packets
  index.yaml            # cheap sidebar metadata
```

### 2.1 Confirm the file endpoints work as documented
For each, give us: handler path, request/response shape, and any limits.
- `GET /api/workspaces/:id/files` — does this return a flat list or a tree? Filter params (path prefix, glob)?
- `GET /api/workspaces/:id/files/*path` — what's the parsed response shape (we're guessing `{path, content, sha, last_modified}`)? What if the file is binary?
- `GET /api/workspaces/:id/files-raw/*path` — raw bytes? Headers?
- `PUT /api/workspaces/:id/files/*path` — request body shape (we currently send `{content, message}`). Does it create commits per write? What's the author attribution?
- `POST /api/workspaces/:id/upload` — multipart shape, 6 MB limit confirmed, return shape?

### 2.2 Append semantics for JSONL?
We want `insights.jsonl` as append-only. Options:
1. Server supports an explicit append mode (preferred — atomic)
2. Client does read-modify-write (cheap but has race conditions if multiple clients write)
3. We model each insight as its own file under `insights/<id>.yaml` (no append needed, but more files)

Which path do you recommend given how the file API behaves today? Is there a content-conflict / optimistic-lock primitive (`If-Match: <sha>`)?

### 2.3 Git endpoints
`GET /api/workspaces/:id/git/log` and `git/diff`:
- Log response shape — commits[] with sha, author, message, timestamp, changed files?
- Diff query params — base/head, single file vs whole tree?
- Are these per-workspace only, or can we diff arbitrary paths/refs?
- Can we tag a commit (for named versions) or create branches via the API?

### 2.4 Filename-derived structure
We rely on convention (everything under `simops/`). Is there anything ABW does that would conflict — special directory names, reserved files, hooks that fire on certain paths? (e.g. is there an `agents/` or `notebooks/` directory inside workspaces that ABW already manages?)

---

## Section 3 — Agent execution

### 3.1 Sync vs stream — exact shapes
- `POST /api/agents/:agent_id/execute` — request body shape, response body shape. Confirm the `{query, context}` envelope from `abw-client.js` is correct.
- `POST /api/agents/:agent_id/execute/stream` — SSE? newline-delimited JSON? Event types? How does the client know when the run is done? Are partial responses streamed token-by-token, or chunked at a higher granularity (tool calls, sub-agent steps)?
- `POST /mcp/agents/:agent_id` — JSON-RPC; do agents accept different methods or only `execute`?

### 3.2 Workspace-scoped execution
When an agent is in a workspace (via `/hire` or `/add`), do subsequent calls to that agent within that workspace context automatically:
- Read/write workspace files via the agent's tools?
- Charge the workspace budget instead of the user's personal balance?
- Log to the workspace message thread?
If yes, give us the exact shape of how to invoke an agent "inside" a workspace (which endpoint, what body).

### 3.3 Per-call cost and budget enforcement
- Where is the credit cost of an agent run determined?
- Does the API return the credit cost in the response? (We want to show running cost in the UI.)
- What happens when the workspace budget is exhausted — 402? 403? A specific error code?
- Can we read the workspace's remaining budget before a call? Endpoint?

### 3.4 Episode log
`/api/agents/:agent_id/episodes` and `/api/episodes/:episode_id` — are these the per-run trace we'd render in a "what did the agent do" inspector? Or something else?

---

## Section 4 — SimOps agents specifically

Agents in the bestiary today: `simops_advisor`, `simops_cascade`, `simops_predictor`, `simops_optimizer`, `simops_narrator`, `supply_chain_oracle`, `bioreactor_modeler`.

### 4.1 Schema contract for `simops_advisor`
What's the exact shape of `context` we should pass to keep the 6-turn dialogue stateful? Today the kask client sends `{project, source}`. Should we also send `{process_yaml, turn_number, prior_turns}` or does the agent expect to read state from a workspace file via tools?

### 4.2 `simops_cascade` invocation
This is the engine that does forward/backward propagation through stages. We want to call it from the Compose mode whenever the user edits a stage, to get refreshed KPIs.
- Input shape: full `ProcessConfig`? Just stages?
- Output shape: per-stage flow numbers, NER, LCC, carbon delta, etc.?
- Is this a *deterministic* call (no LLM in the loop)? If yes, can we expose it as a non-agent endpoint for very low latency? Or is going through `execute` fine?

### 4.3 `simops_predictor` & `simops_optimizer`
- Predictor: trained per-process or globally? Does it need a corpus of observations to be useful, and where do those come from?
- Optimizer: input shape (target output + predictor reference)? Output shape (recommended input quantities)?

### 4.4 `supply_chain_oracle`
Currently described as "called by the SimOps Composer when the user requests agent-resolved pricing." How is it invoked today from the composer code? We want to confirm the wire format before we replicate it.

### 4.5 Is there a `sidestream_miner` agent yet?
We want one. Does anything close to it already exist (perhaps under a different name), or should we propose adding one to the bestiary? Same question for: `product_scout`, `regulatory_scanner`, `valuechain_mapper`, `comparator` (compares two experiment results), `marketing_composer`.

### 4.6 The canonical ProcessConfig schema
We've read `crates/simops/src/process.rs` and see the `Resource / Stage / CapexProfile / ProcessConfig` types. Two questions:
- **Is there a JSON Schema or example YAML in the repo we should reference as the source of truth?** A path to an example file like `examples/algae.yaml` or `crates/simops/fixtures/*.yaml` would be ideal.
- The current kask wizard emits a different shape (stages have `efficiency_pct` instead of `efficiency`, plus `sidestreams[]` and `sensors[]` which don't exist in the Rust struct). **Should kask migrate to the canonical Rust shape, or extend the Rust shape to include kask's extras?** We'd prefer to migrate kask but want your call.

---

## Section 5 — FPL & forecasts

### 5.1 `POST /api/fpl/execute` — the killer endpoint
Handler: `handlers::notebooks::fpl_execute_handler`.
- Request body: full shape with examples. Specifically: how do we send an FPL chain? As source text? As a parsed AST? As a notebook ID?
- Response body: what does a successful run return? Distributions as samples (arrays), distributions as parameters (mean/std), or both? Multiple outputs per run?
- Latency expectations: sub-second? Seconds? Tens of seconds?
- Is there a way to express a Monte Carlo run with N samples?
- Can we attach priors / inject variables from outside (kask passes the Process YAML + scenario overrides → FPL produces an NPV distribution)?
- Cost: does FPL execution burn credits, and if so how is it metered?

### 5.2 Notebooks vs `/api/fpl/execute`
We see `/api/notebooks/:id/execute` as well. Difference?
- Notebook = persisted FPL document; execute runs it
- `/api/fpl/execute` = stateless one-shot
Confirm this is the right framing.

### 5.3 Forecasts as persistence
`/api/forecasts` CRUD. We want to persist the results of an Experiment (one row per scenario × engine run) as Forecast objects so they're queryable, comparable, and inheritable by the Tetlock-style leaderboard.
- Forecast shape: question, probability/distribution, resolution criteria, schedules, portfolio links?
- Can a Forecast carry arbitrary metadata (e.g. `{simops_experiment_id, scenario_id, process_version}`)?
- Can a Forecast be linked to a workspace (so the workspace UI can list its forecasts)?

### 5.4 Forecast schedules — the continuous-discovery primitive
`/api/forecasts/:id/schedules` GET/PUT and `/run` POST.
- Schedule shape (cron expression? interval seconds? next-run timestamp?)
- What happens on a scheduled run — does it re-execute the underlying FPL? Re-run an agent? Both?
- Are schedule executions logged somewhere we can subscribe to?
- Can a schedule trigger arbitrary actions (e.g. re-run `sidestream_miner` on a Process every week) or only forecast-style operations?

### 5.5 Portfolios
`/api/portfolios`. Useful for SimOps? Probably: a "portfolio" of forecasts could be the natural container for an Experiment's N scenarios. Confirm or refute.

### 5.6 Public forecasts
`/api/forecasts/public` — what makes a forecast "public"? Is there a `visibility` field on Forecast, or is publication a separate action?

---

## Section 6 — Sharing, ACL, publication

### 6.1 `POST /api/shares` — the generic ACL primitive
We read the handler at `src/handlers/teams.rs:302-335`. The `ShareObjectRequest` has `object_type`, `object_id`, `share_type` (team|user), `share_target`, `permission` (view|edit|admin).
- **What values does `ObjectType::from_str` accept?** We need to know if `"workspace"` is valid today. If not, what's needed to add it?
- Is there a corresponding `GET /api/shares?object_type=X&object_id=Y` to list shares on an object? (We didn't see one in the route list.)
- How does a viewer/editor actually access a shared workspace — does it appear in their `/api/workspaces` list? Filtered separately as "shared with me"?

### 6.2 Teams
- Team shape, member roles
- Can a team have nested teams or only flat membership?
- Does a workspace owner have to be a team, or can it be a user? Both?

### 6.3 Publication
We see `/api/agents/:id/publish` and `/api/forecasts/public`. We want the same pattern for a SimOps Process.
- Is there a generic workspace-publish primitive? If not, **what would it take on the ABW side to add one?** (We need: a stable public URL, slug, optional fork tracking, read-only public access, optional citation block.)
- If publication is per-object-type, what's the closest existing pattern we can clone? (Agent publish? Forecast public?)

### 6.4 Fork tracking
`/api/agents/:id/fork` exists. Is there a generic `fork` primitive on workspaces or processes, or only on agents?

---

## Section 7 — Activity feed & observability

### 7.1 `/api/feed/events` (write) and `/api/feed/stream` (SSE)
- POST events shape: what fields are required, what's optional?
- Filter params on `/api/feed` and `/stream` (per-workspace? per-user? per-object?)
- Latency on the stream — sub-second push?
- Are events deduplicated, ordered, or replayable from an offset?

### 7.2 Notifications
`/api/notifications`, `/api/notifications/:id/read`, `/read-all`.
- Notification shape
- How is a notification triggered — by feed events? By agents? By a specific server-side rule engine?
- Can the kask client *create* a notification (e.g. "your overnight discovery pass found 3 new sidestreams"), or are notifications system-only?

### 7.3 Observability suite
`/api/observatory/agents/:id/{timeline,dyads,anomalies,scan}` and `/api/observatory/hitl/*`.
- Quick summary: what does each do? We want to know if any of these are reusable by the kask SimOps UI (e.g. to render "what the agent did" with full provenance) or if they're admin-only.

---

## Section 8 — Streaming, async, long-running

### 8.1 SSE format consistency
We see SSE on `/api/workspaces/:id/messages/stream`, `/api/agents/:id/execute/stream`, `/api/feed/stream`, `/api/creatures/:id/stream`, `/api/rabble/:id/stream`. Are they all the same SSE event shape (event-name + data JSON), or do they differ? A single example payload from each would unblock us.

### 8.2 Long-running agent runs
If an agent run takes 60-120 seconds:
- Does the sync `execute` endpoint hold the HTTP connection open the whole time?
- Or is there an episodes-based pattern (kick off → poll episode for completion)?
- Recommended pattern for kask: stream from the start, or poll?

### 8.3 Cancellation
Can a long-running agent run be cancelled by the client? Endpoint? Behaviour (refund credits? abandon mid-step?).

---

## Section 9 — Background / scheduled work

### 9.1 Beyond forecast schedules, is there a general job queue?
We want to run agents on a schedule for **continuous discovery** (e.g. every workspace runs a `sidestream_miner` pass once a week). Options:
1. Reuse `/api/forecasts/:id/schedules` semantics for non-forecast work (questionable — schedules might be tightly coupled to forecasts)
2. A separate generic-scheduler endpoint exists somewhere we haven't found
3. Nothing exists; kask runs a client-side timer when the tab is open and we add a server-side scheduler later

Which path is actually viable today?

### 9.2 What happens server-side after a schedule fires?
- Does the system POST to an internal queue? Use a cron worker? A separate process?
- Are the results emitted as feed events / notifications / workspace messages?
- Failure handling — retries, dead-letter?

---

## Section 10 — Things we don't know we don't know

### 10.1 Pending work
What's in flight on the fermi side right now that overlaps with this design? Specifically — anything in PRs or recent commits touching: workspaces, sharing, publication, FPL, the SimOps agents, the file API?

### 10.2 Known limits, gotchas, footguns
Anything that would bite us if we built naively against the surfaces above? (E.g. "the file API rewrites line endings", "workspace messages are limited to N per workspace", "FPL doesn't support distributions over discrete events yet", "git diff is generated lazily and can be slow", anything else.)

### 10.3 Existing kask integrations on the fermi side
Search for `kask` in the fermi tree — what comes up? Anything we should be aware of before adding more?

### 10.4 Recommended OpenAPI / SDK story
`/openapi.json` returns 404. Is there a plan to publish a spec? Would generating one from the axum router be straightforward? Any preference on how kask should keep its client in sync with the ABW API as it evolves — manual updates, generated client, shared type crate, something else?

---

## Section 11 — Decisions we need to make in this round

For each, give us **your recommendation** plus a one-sentence why. We'll lock these in for SimOps v2.

1. **Workspace-per-Process vs Process-as-file-in-a-shared-workspace.** Recommend.
2. **Append-only JSONL vs file-per-record** for insights. Recommend.
3. **Workspace tagging mechanism** to mark a workspace as "SimOps". Recommend.
4. **Sync vs stream** for agent calls in the SimOps UI as the default. Recommend.
5. **Where to persist Experiment results** — workspace files only, or also as Forecast objects (or both)? Recommend.
6. **Schema source of truth** for ProcessConfig — kask's current shape, the Rust crate's shape, or a unified v2 schema. Recommend.
7. **Sharing object type** for workspaces — extend `ObjectType` if needed, or share at a finer grain (per-file, per-Process)? Recommend.
8. **Publication mechanism** — extend `/api/shares` with a `public` share_type? New `/api/workspaces/:id/publish`? Recommend.

---

## How to deliver your answers

Paste the headings of each section (1.1, 1.2, ...) back with answers underneath. For the most important ones (workspace shape, file endpoint payloads, FPL execute shape, share_object types, schedule semantics), include the actual code excerpt — even 5-10 lines is fine. We need to commit to API shapes in kask within the next iteration, so any ambiguity left after this round becomes our problem.

Thank you. This is the unblocker for everything in SimOps v2.
