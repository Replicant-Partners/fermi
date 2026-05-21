# 12 — Agent Version as a First-Class Dimension

**For:** the ABW maintainer (agent runtime, observations API, workspace activity feed)
**From:** kask team
**Status:** specification; not yet implemented platform-side.
Kask is shipping interim version-stamping on its own writes; the
platform-side capabilities below would supersede that and extend
the benefits to every ABW app.
**Companion piece to:** `08_FILES_API_DIVERGENCE.md`,
`09_RESEARCH_AGENT_OUTPUT_STRIPPED.md` —
this is the third systemic platform-surface request from kask in
the same series.

---

## The framing

An ABW agent is the composite state of its `agent_card.json`:
`agent_id`, `system_prompt`, `model`, `model_ladder`,
`temperature`, `mcp_tools`, `valence`, `capability_gates`,
`output_contract`, plus the accumulated learned state (episode
memory, ontology evolution_commits, consolidated dream synopses,
calibration scores).

When the card changes — owner-driven via `PUT /api/agents/<id>`,
agent-driven via the dreaming budget, strategist-driven via
composition tuning, platform-driven via cognition-tier
reassignment — that is **a state transition the agent itself
undergoes**, not a substitution. Same `agent_id`, same episodic
memory, learning continues forward.

`agent_versions` (the table the platform already maintains; the
`list_agent_versions` endpoint returns rows from it) is the
canonical lineage record of those state transitions.

What this spec proposes: **every artifact the agent produces, and
every artifact produced AGAINST an agent's output, should carry the
agent version it was produced under**. That single change unlocks:

- **RSI Loop 5 calibration becomes version-aware.** Brier scores
  can be partitioned by version. "Did the prompt update help?"
  becomes a query, not a guess.
- **A/B testing of agent configurations becomes a database query.**
  Run v3.1.0 on workspace A, v3.1.1 on workspace B, compare
  resolved-outcome Brier across the partition. Same shape for
  testing model-ladder changes (cognition-tier downgrades) or
  valence adjustments.
- **Regression detection.** If Brier under the latest version is
  worse than under the previous one on the same workload, the
  platform can surface that automatically instead of waiting for
  someone to notice degradation.
- **Audit transparency.** Operators can answer "which version of
  the agent made this decision" for any historical artifact, which
  matters for regulated domains (the SimOps biotech workspaces are
  one such case).
- **Cross-app value.** Every ABW app benefits, not just kask. The
  bioreactor controller, future twin apps, the Rabble ecosystem
  agents, the upcoming forecasting compositions — all get
  version-aware calibration for free.

This belongs on the platform side because (a) the agent runtime is
the only component that knows the version at execution time, (b)
the database is the only place where the join from observations to
versions can be efficient, and (c) RSI Loop 5 is platform-level
infrastructure, not app code.

---

## The four capabilities

### Capability 1 — Versioned observation writes

**Goal:** every `sosa_observation` row in the database carries the
agent version that produced it.

#### Schema change

Add to `sosa_observations`:

```sql
ALTER TABLE sosa_observations
  ADD COLUMN produced_by_agent_id     TEXT NULL,
  ADD COLUMN produced_by_version_id   UUID NULL REFERENCES agent_versions(version_id),
  ADD COLUMN produced_by_version_number INTEGER NULL;

CREATE INDEX idx_obs_produced_by_version
  ON sosa_observations (produced_by_agent_id, produced_by_version_id)
  WHERE produced_by_agent_id IS NOT NULL;
```

`produced_by_agent_id` and `produced_by_version_number` are
denormalised conveniences (saves a join when filtering /
displaying); `produced_by_version_id` is the foreign key.

All three are nullable — observations written by deterministic
tools or directly by users (kask's typing into a field) have no
agent provenance, and that's fine.

#### Write paths

**(a) Agent tool calls write observations.** Today
`simops_write_observation` (in
`src/agent_backend/simops_tools.rs:520`) writes to
`sosa_observations` with the session context but without agent
provenance. Modify to read the active agent context from
`ToolContext` and populate the three columns.

The agent context is already available — `ToolContext` carries the
agent UUID. The current version is queryable via the
`agent_versions` table by `(agent_id, max(version_number))` or by
following the latest pointer the agent service maintains. One
join, cached for the duration of the execution.

**(b) Direct ingest endpoint.** The
`POST /api/observe/sessions/:session_id/observations` endpoint
(`src/handlers/observations.rs:337` `ingest_observations_handler`)
accepts a batch. Extend the request schema to allow optional
provenance per observation:

```rust
pub struct SosaObservation {
    // ... existing fields ...
    pub produced_by: Option<ProducedByAgent>,
}

pub struct ProducedByAgent {
    pub agent_id: String,
    pub version_id: Option<Uuid>,
    pub version_number: Option<i32>,
}
```

If the client provides `agent_id` but not `version_id`/`version_number`,
the server resolves to the agent's current version at write time.
This is the path kask uses (it doesn't track version_ids
client-side; just passes the agent_id and lets the server resolve).

#### Read paths

The existing query endpoint
(`/api/observe/sessions/:session_id/observations`) gains optional
filters:

```
GET .../observations?produced_by_agent_id=simops_predictor
                   &produced_by_version_number=3
```

Returned rows include the three new columns when set.

---

### Capability 2 — Versioned execution_result events

**Goal:** every `execution_result` message in the workspace
message log carries the agent version that produced it.

#### Schema change

`messages.metadata` is already a JSONB column; just add fields to
the `AgentMetadata` struct (`src/agent_backend/executor.rs`,
already extended by fermi #3 with `provider`, `stop_reason`,
`failure_reason`):

```rust
pub struct AgentMetadata {
    // ... existing fields ...
    pub agent_version_id: Option<Uuid>,
    pub agent_version_number: Option<i32>,
}
```

Populated in `tool_executor.rs` and `llm_executor.rs` from the
agent context at execution time.

This is the smallest of the four changes — purely additive on a
struct that's already being extended. No migration needed; just
new keys in existing JSONB.

#### Why this matters as a separate capability

Even when the agent doesn't write a `sosa_observation`, its
`execution_result` is a record of what it produced. Downstream
consumers (kask, other apps) propagate that into artifacts they
write *because of* the agent execution. So this is the source
data for "kask cached the version when it processed the
comparator's narration."

---

### Capability 3 — Versioned activity feed event

**Goal:** when an agent's card changes, every workspace where the
agent is hired sees a `prompt.updated` (or more generally,
`agent_card.updated`) event in its activity feed.

#### Event shape

```json
{
  "kind": "agent_card.updated",
  "body": {
    "agent_id": "simops_companion",
    "from_version_id": "...",
    "from_version_number": 4,
    "to_version_id": "...",
    "to_version_number": 5,
    "changed_fields": ["system_prompt", "model_ladder"],
    "changelog_summary": "<from PUT body's `changelog` field, optional>",
    "changed_by": "owner|dreaming|strategist|platform_tier_reassignment",
    "changed_at": "2026-05-21T19:00:00Z"
  }
}
```

#### Emission point

In `update_agent_handler`
(`src/handlers/agents.rs:1190`), after the `create_agent_version`
+ `update_agent` calls succeed, fan out the event to every
workspace where the agent is hired:

```rust
// pseudocode
let hired_workspaces = state.memory_store
    .find_workspaces_with_agent(db_agent.agent_id).await?;
for ws_id in hired_workspaces {
    state.events.emit(WorkspaceEvent {
        workspace_id: ws_id,
        kind: "agent_card.updated",
        body: serde_json::to_value(&AgentCardUpdatedBody { ... })?,
        // ...
    }).await?;
}
```

(Other change pathways — dreaming-driven self-modification,
composition strategist tuning, ADR-011 tier reassignment — should
fire the same event from their respective entry points. The body's
`changed_by` discriminates the source.)

#### Why this matters

App-level UIs need a single channel to surface "the agent is now
running a different version" without polling the agents API. Same
shape as every other lifecycle event on the platform. Kask's
Activity panel renders it; future apps render it however they
want.

A `prompt.updated` event arriving while a companion turn is
in-flight is a small race; the simplest resolution is "events fire
after the update commits; in-flight turns continue with their
captured version." The companion's next turn will read the new
prompt naturally from the agent card.

---

### Capability 4 — Calibration query API

**Goal:** make Brier-scoring queryable partitioned by agent
version, so "did the update help?" becomes a single API call.

#### Endpoint

```
GET /api/agents/<agent_id>/calibration
    ?partition_by=version|none
    &window_days=90
    &workspace_id=<optional>
```

#### Response

```json
{
  "agent_id": "simops_predictor",
  "window_days": 90,
  "partition_by": "version",
  "partitions": [
    {
      "version_number": 3,
      "version_deployed_at": "2026-04-12T...",
      "n_observations_forecasted": 142,
      "n_resolved": 38,
      "brier_mean": 0.21,
      "brier_std": 0.08,
      "confidence_interval_95": [0.19, 0.23]
    },
    {
      "version_number": 4,
      "version_deployed_at": "2026-04-30T...",
      "n_observations_forecasted": 67,
      "n_resolved": 12,
      "brier_mean": 0.18,
      "brier_std": 0.06,
      "confidence_interval_95": [0.14, 0.22]
    },
    {
      "version_number": 5,
      "version_deployed_at": "2026-05-18T...",
      "n_observations_forecasted": 89,
      "n_resolved": 8,
      "brier_mean": 0.24,
      "brier_std": 0.11,
      "confidence_interval_95": [0.16, 0.32]
    }
  ],
  "interpretation": "v4 improved over v3 (0.21 \u2192 0.18, significant). v5 regressing \u2014 not yet conclusive (n=8, CI overlaps both prior versions)."
}
```

The `interpretation` field is optional convenience — a one-line
narrative that consumers can render directly. Falls out of basic
statistics on the partitions; can be omitted in v1 and added
later.

#### How it's computed

The query joins `sosa_observations` (now version-stamped from
Capability 1) against the platform's existing Brier scoring
machinery. Forecasts whose resolved outcome is known contribute
to the score; the version stamp determines the partition.

This is the surface that closes Loop 5. Without it, every app
implements its own slicing logic (or doesn't, and the loop runs
blind). With it, the platform answers the calibration question
canonically.

#### Without Capability 1, this endpoint can't function

So Capability 1 (the version stamp on observations) is the only
load-bearing one. Capabilities 2, 3, 4 are valuable but build on
the foundation.

---

## What kask is shipping in the interim

While the platform-side work is in flight, kask is shipping a
narrower interim version of the same idea on its own writes:

- **`scripts/sync-agent-prompt.sh`** — extracts the system_prompt
  from the kask repo's spec markdown, diffs against deployed,
  PUTs the update with an incremented version, and records the
  transition in `adaptogen/simops-v2/specs/v3/agent_versions.yaml`
  (the team's audit trail, independent of ABW's `agent_versions`
  table).
- **Interim observation stamping** — kask's
  `recordCascadeAsObservations` and `recordSensorReading`
  functions read the current versions of relevant agents from
  `GET /api/agents/<id>`, cache per-workspace, and pass
  `produced_by` on the ingest body. When the platform's
  Capability 1 lands, this becomes the API the ingest endpoint
  already expects.
- **Interim event emission** — kask's sync script POSTs a
  `chat`-message with kind `agent_card.updated` to every kask
  workspace that has the agent hired. When the platform's
  Capability 3 lands, kask drops its emission and consumes the
  native events.
- **UI surfacing** — Activity panel and Insights maturity panel
  surface agent version transitions and current per-agent
  versions. Reads whichever source is available (native platform
  events if present, kask's own events as fallback).

The intent is for the interim work to be **forward-compatible**:
the kask client uses the same payload shapes as the proposed
platform API, so when the platform support lands, kask flips a
configuration flag from "emit ourselves" to "consume from
platform" without restructuring.

---

## Verification

After the platform-side work lands, this should pass:

```bash
WS=<workspace-id>
API_KEY=$(grep -oP 'api_key = "\K[^"]+' ~/.abw/credentials)

# 1. PUT a small prompt change to simops_predictor
curl -sS -X PUT -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"system_prompt":"...updated...","version":"3.2.0"}' \
  "https://agent-bestiary.world/api/agents/simops_predictor"

# 2. Within seconds, the workspace's message log should have an
#    agent_card.updated event
curl -sS -H "Authorization: Bearer $API_KEY" \
  "https://agent-bestiary.world/api/workspaces/$WS/messages?limit=5" \
  | jq '.messages[] | select(.metadata.kind=="agent_card.updated")'
# \u2192 single event with from_version_number, to_version_number, etc.

# 3. Invoke the agent (writes an execution_result)
abw workspace message $WS "<test query>" -a simops_predictor

# 4. The execution_result carries the agent_version
sleep 30
curl -sS -H "Authorization: Bearer $API_KEY" \
  "https://agent-bestiary.world/api/workspaces/$WS/messages?limit=5" \
  | jq '.messages[] | select(.sender_id=="simops_predictor")
                    | .metadata.agent_version_number'
# \u2192 should print the new version number

# 5. Query calibration partitioned by version
curl -sS -H "Authorization: Bearer $API_KEY" \
  "https://agent-bestiary.world/api/agents/simops_predictor/calibration?partition_by=version&window_days=90"
# \u2192 array of partitions, one per recent version, with brier_mean per partition
```

---

## Estimated cost (rough)

- **Capability 1** (observation stamping): one migration, modest
  changes to the ingest handler and the simops_tools write path.
  Maybe a day's work for someone familiar with the codebase.
- **Capability 2** (execution_result metadata): minimal —
  additive struct fields, populated at known points. A few hours.
- **Capability 3** (activity event emission): moderate — needs a
  fan-out across workspaces; depends on whether the platform
  already has a workspace-events infrastructure to plug into. A
  day.
- **Capability 4** (calibration query API): largest of the four —
  involves joins, statistical computation, possibly caching for
  expensive workspaces. 2-3 days.

Total: roughly a week of focused work to land the whole shape.
None of the four is on a critical-path blocker for an existing kask
flow today (kask's interim work covers the current pitch story), so
this can land at the platform team's cadence.

---

## Why this fits the broader ABW thesis

The platform thesis (per xaman_ek's system prompt and the agentic
infrastructure whitepaper) is that ABW provides infrastructure
agents need to learn, coordinate, and integrate with the physical
world. Five RSI loops, the `sosa_observation` substrate, the
composition primitives.

Agent versioning closes a hole in this story. RSI Loop 5
(Brier-calibrated routing) and Loop 1 (per-agent learning via
consolidation) both **assume** that "the agent" is a stable
identity across calibration windows. In practice, the agent
evolves — owners update prompts, models get tier-swapped, the
dreaming mechanism rewrites the agent's own card. Without version
stamping on the artifacts the loops train on, the loops are
implicitly assuming a stationarity that doesn't exist in
production.

This proposal makes the version axis explicit, which is what makes
the loops correct rather than just statistically lucky.

---

## Cross-references

- `agents/curated/simops_companion/agent_card.json` — example of
  the card structure this spec treats as the agent's evolving state
- `src/agent_backend/executor.rs` — `AgentMetadata` struct already
  extended by fermi #3 with `provider`, `stop_reason`,
  `failure_reason`; this spec extends it further with
  `agent_version_id`, `agent_version_number`
- `src/handlers/observations.rs` — `ingest_observations_handler`
  is the entry point for direct observation writes; needs to
  accept the optional `produced_by` field
- `src/agent_backend/simops_tools.rs::execute_simops_write_observation`
  — agent-driven write path; needs to read agent version from
  `ToolContext`
- `agent-bestiary/memory/src/types.rs::AgentUpdate` — the update
  struct PUT bodies serialise to; already supports `version` as a
  string field
- xaman_ek system prompt \u00a7"The Five Feedback Loops" — RSI loops
  that depend on this versioning being correct
- `08_FILES_API_DIVERGENCE.md`, `09_RESEARCH_AGENT_OUTPUT_STRIPPED.md`
  — prior platform-surface requests from kask; same handoff shape
