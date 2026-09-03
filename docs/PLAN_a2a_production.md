# Plan: A2A provider production-ready

**Goal:** Every A2A capability declared in agent cards (`streaming: true`,
`pushNotifications: true`) actually works end-to-end, including async
execution, retries, and input validation. The port trust heuristic is
replaced by a real schema registry.

**Current state:** 
- Sync + SSE streaming: working
- Push notification config: stored, delivered once (no retries)
- `returnImmediately: true`: reserves episode but does NOT run the agent
- Input validation: `is_text_input` heuristic (explicitly temporary)
- xaman_ek: not updated

---

## Dependencies

```
Step 1: async task execution     (standalone — only touches a2a.rs)
Step 2: push notification retries (builds on async — retries need real task status)
Step 3: port trust registry      (standalone — only touches port_trust.rs + agent cards)
Step 4: xaman_ek update          (always last)
```

---

## Step 1 — Async task execution (`returnImmediately: true`)

**Current state:** `send_message_handler` mints an episode ID, returns
`TASK_STATE_SUBMITTED`, and exits. The agent never runs.

**What needs to change:** spawn the full execute pipeline as a background task
before returning. The episode is already reserved (`Pulse::open` writes the
row). The background task fills it in when done.

### Implementation

**`src/handlers/a2a.rs` — `send_message_handler`:**

Replace:
```rust
if req.configuration.return_immediately {
    return Ok(Json(fermi::a2a_task::submitted_task(episode_id, &caller_id)));
}
```

With:
```rust
if req.configuration.return_immediately {
    // Spawn the full execute pipeline. The episode row is already reserved.
    // Polling GET /tasks/:id will return WORKING until close() completes.
    let push_cfg = req.configuration.push_config.clone();
    spawn_background_execute(
        state.clone(), db_agent.clone(), card.clone(), slug.clone(),
        query.clone(), episode_id, caller_id.clone(), wallet.wallet_id,
        push_cfg,
    );
    return Ok(Json(fermi::a2a_task::submitted_task(episode_id, &caller_id)));
}
```

**New function `spawn_background_execute`:**

```rust
fn spawn_background_execute(
    state: AppState,
    db_agent: Agent,
    card: AgentCard,
    slug: String,
    query: String,
    episode_id: Uuid,
    caller_id: String,
    wallet_id: Uuid,
    push_cfg: Option<InlinePushConfig>,
) {
    tokio::spawn(async move {
        // Steps: credentials → context → tool_context → executor
        // → execute → grade → close → charge
        // Identical to the synchronous path but in a background task.
        //
        // The episode row exists (reserved). close() updates its status
        // from the Pulse. get_task_handler polls execution_status.
        
        let result = run_a2a_execute(
            &state, &db_agent, card, &slug, &query, episode_id,
            &caller_id, wallet_id,
        ).await;
        
        // Fire push notification on completion (success or failure).
        let task_payload = match result {
            Ok(ref raw) => fermi::a2a_task::completed_task(episode_id, &caller_id, raw.as_deref()),
            Err(ref e)  => fermi::a2a_task::failed_task(episode_id, &caller_id, e),
        };
        
        // Inline config from the request.
        if let Some(pc) = push_cfg {
            // register + deliver (same pattern as sync path)
        }
        // Any configs registered separately.
        crate::a2a_webhook::fire_for_task(state.db.clone(), episode_id, task_payload);
    });
}
```

**`run_a2a_execute` function:** extract the shared execute core from
`send_message_handler` (steps 6–13: card enrichment through credit charging).
Both the sync and async paths call it. This removes the code duplication
between the two paths.

### Task status visibility

`get_task_handler` calls `state.memory_store.get_episode(episode_id)`.

The episode's `execution_status` transitions:
- After `Pulse::open`: row exists with initial state (check what `reserve_episode` writes)
- During execution: row unchanged (execution is in-progress)
- After `episode_boundary::close`: row updated with `ExecutionStatus::Success`/`Failure`

**Gap to check:** confirm `reserve_episode` writes a recognisable "running"
state vs the final state written by `close`. If both write the same initial
value, the task will show as COMPLETED before the agent finishes.

Look at `agent_bestiary_memory::MemoryStore::reserve_episode` to understand
what status it sets. The `get_task_handler` should map:
- "reserved but not yet graded" → `TASK_STATE_WORKING`
- `ExecutionStatus::Success` | `ExecutionStatus::Partial` → `TASK_STATE_COMPLETED`
- `ExecutionStatus::Failure` → `TASK_STATE_FAILED`

The mapping needs a way to distinguish "reserved" from "completed Success".
One option: a `started_at`/`completed_at` timestamp pair on episodes
(migration). Another: a separate `task_status` column in `a2a_push_configs`.
Simplest: check if `response_text IS NULL` → still running.

**File:** `src/handlers/a2a.rs`  
**Test:** integration test calling `message:send` with `returnImmediately:true`,
then polling `GET /tasks/:id` until COMPLETED.

---

## Step 2 — Push notification retries

**Current state:** delivery is best-effort, one attempt, no retries.

**What needs to change:** a sweeper that retries failed deliveries up to
N times with exponential backoff.

### Schema addition

`migrations/225_a2a_push_config_retry.sql`:

```sql
-- Add retry fields to the push configs table.
ALTER TABLE a2a_push_configs
  ADD COLUMN IF NOT EXISTS next_retry_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS max_attempts   INT NOT NULL DEFAULT 5;

CREATE INDEX IF NOT EXISTS a2a_push_configs_retry_idx
    ON a2a_push_configs (next_retry_at)
    WHERE delivered_at IS NULL AND delivery_attempts < max_attempts;
```

### Sweeper

**`src/a2a_webhook.rs` — new function:**

```rust
/// Retry all push configs that are due for another attempt.
///
/// Called by a background task spawned at server boot. Runs every 60 seconds.
/// Exponential backoff: attempt N waits 2^(N-1) minutes before retry.
pub async fn sweep_pending_retries(pool: PgPool) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        
        // Find configs due for retry.
        let rows = sqlx::query(
            "SELECT config_id, webhook_url, auth_scheme, auth_credentials, task_id
             FROM a2a_push_configs
             WHERE delivered_at IS NULL
               AND delivery_attempts < max_attempts
               AND (next_retry_at IS NULL OR next_retry_at <= NOW())"
        )
        .fetch_all(&pool).await.unwrap_or_default();
        
        for row in rows {
            // Load the Task payload for this task_id from the episode.
            // Deliver. Record success/failure. Set next_retry_at for next attempt.
        }
    }
}
```

**`src/api_server.rs` — spawn at boot:**

```rust
// Spawn A2A push notification retry sweeper.
{
    let pool = state.db.clone();
    tokio::spawn(crate::a2a_webhook::sweep_pending_retries(pool));
}
```

**`record_delivery` update:** on failure, set
`next_retry_at = NOW() + (interval '1 minute' * power(2, delivery_attempts))`.

**File:** `src/a2a_webhook.rs`, `migrations/225_a2a_push_config_retry.sql`,
`src/api_server.rs`

---

## Step 3 — Port trust registry

**Current state:** `port_trust::is_text_input` is an explicit heuristic.
The module docs say "deleting it is the success condition."

**What needs to change:** a registry mapping type IDs to their shape
(text-accepting or structured). `bind_input` looks up the type, not the label.

### Design

A type ID is either:
- A schema ID (contains `/`, not a MIME type): `"scro/bom-query/1"`, `"simops/process_config"` → structured
- A registered text type: `"fermi/free_text_query"` → text-accepting
- A MIME type: `"text/plain"`, `"application/json"` → look at the MIME type

The registry is a `const` — one definition, no DB — for the same reason
the provenance vocabulary is a const: a second list that silently drifts
is worse than the heuristic.

### Implementation

**`src/port_trust.rs` — add:**

```rust
/// Known text-shaped type IDs. Agents that accept free text declare
/// `accepts: ["fermi/free_text_query"]` instead of `accepts: ["query"]`.
/// The heuristic (is_text_input) exists until every agent uses this.
pub const TEXT_TYPE_IDS: &[&str] = &[
    "fermi/free_text_query",
    "fermi/forecast_question",
    "fermi/research_prompt",
];

/// Is this type ID (or label) text-shaped?
///
/// Replaces the heuristic once all agents use registered type IDs.
/// Currently falls back to `is_text_input` for legacy labels.
pub fn is_text_shaped(label: &str) -> bool {
    // Registered text types — exact match.
    if TEXT_TYPE_IDS.contains(&label) {
        return true;
    }
    // MIME type — text/* accepts text.
    if label.starts_with("text/") {
        return true;
    }
    // Schema ID — structured, not text.
    if a2a_card::is_schema_id(label) {
        return false;
    }
    // Legacy label — fall back to heuristic.
    is_text_input(label)
}
```

**Update `bind_input`** to call `is_text_shaped` instead of `is_text_input`.

**Agent card migration:** any agent whose `accepts` currently uses text labels
(`"query"`, `"forecast-question"`, etc.) should update to
`["fermi/free_text_query"]` when its card is next touched. This is
incremental — the fallback to `is_text_input` covers unminitgrated agents.

**Deletion condition:** `is_text_input` can be deleted when
`port_binding_expected.json` has zero `"binding": "declared"` entries that
use a legacy label (i.e., all declared bindings reference a registered type ID).

**`port_binding_expected.json`** — update entries for agents whose
`accepts` is updated to use registered type IDs.

**File:** `src/port_trust.rs`, `agents/curated/*/agent_card.json` (incremental)

---

## Step 4 — xaman_ek card update

Same as `PLAN_simops_verification.md §4`, with the addition of:

1. **A2A provider capabilities** — all endpoints, auth model, scope convention
2. **`returnImmediately: true`** — now actually runs the agent (after Step 1)
3. **Push notification retry sweeper** — spawned at boot (after Step 2)
4. **Port trust registry** — `fermi/free_text_query` registered type (after Step 3)
5. **TYPED_TIER_EXEMPT BASELINE** — current count after all migrations

---

## Validation sequence

```bash
# Step 1: async execution
cargo build --bin api-server                        # compiles
# integration test: POST message:send returnImmediately:true, poll, confirm COMPLETED

# Step 2: retries
cargo check                                         # migration + sweeper compile

# Step 3: port trust
cargo test --lib -p fermi port_trust                # all 9 tests pass
cargo test --lib -p fermi port_binding              # fixtures match new labels

# All steps together
cargo test --lib -p fermi                           # full lib suite
cargo test --test contract_sketch_corpus            # corpus unchanged
```

---

## What is NOT in this plan

- **Rate limit configuration** — the values are env vars, not hardcoded.
  Documenting them belongs in operator runbooks, not here.
- **Stripe billing for external callers** — the credit mechanism works;
  the user-facing purchase flow is a product decision.
- **Framework client library integrations** (ADK, LangGraph, etc.) —
  untested. When a specific framework integration is needed, it gets its
  own plan.
- **Async task cancellation** (`POST /tasks/:id:cancel`) — not implemented.
  Background tasks cannot be cancelled in the current architecture (no
  task handle stored). Requires a cancellation token registry.
