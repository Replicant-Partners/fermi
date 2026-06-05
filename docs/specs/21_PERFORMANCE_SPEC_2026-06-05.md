# Spec 21 — Performance Hardening
**Status:** Ready for implementation  
**Date:** 2026-06-05  
**Priority:** High  
**Implementer note:** Phases are ordered by expected latency impact per unit of effort. Do not skip Phase 0.

---

## Context

A code audit against the production codebase (commit HEAD, 2026-06-05) identified
the following performance characteristics:

- Every agent execution triggers two separate embedding API calls that could be one.
- KG context retrieval loads the **entire** rules + entities corpus into memory
  (potentially megabytes) to use 13 rows. The cut is done in Rust, not in Postgres.
- The `/execute` endpoint holds the HTTP connection open for the full LLM round-trip
  (5–30 s). There is no async job pattern for non-streaming execution.
- The consolidation HTTP handler blocks the response for the full DBSCAN + multi-LLM
  cycle. DBSCAN itself runs synchronously on a Tokio executor thread.
- Every SSE subscriber opens a raw Postgres connection (bypasses pool). Under moderate
  load this exhausts the connection budget.
- The `workspace_git.commit_file` call in the workspace message background task is a
  blocking filesystem + git operation executed without `spawn_blocking`.
- The `at-mention` regex is recompiled on every workspace message.
- Consolidation inserts rules, entities, and facts one row at a time (N+1 pattern).
- The `episodes` table has no partial composite index on `(agent_id) WHERE NOT consolidated`,
  causing full agent-episode scans to find the unconsolidated subset.

This spec addresses these issues in four phases. The ABW runtime-separation spec
(ABW Runtime Separation — UX Latency & Loop SOC) is superseded in its Phase 0 findings
by the audit here; Phases 1–4 of that spec remain valid but are lower priority than
the items in this document and should only be pursued if profiling after Phase 3 below
shows the single Tokio runtime is actually saturated.

---

## Phase 0 — Baseline instrumentation (do before any code change)

Add `tracing::info!` timing spans at the following points so the impact of each
subsequent phase is measurable. Use `tokio::time::Instant::now()` (not `std::time`).

```
// src/handlers/execution.rs — around enrich_with_kg_context call
let t0 = tokio::time::Instant::now();
let card = enrich_with_kg_context(...).await;
tracing::info!(elapsed_ms = t0.elapsed().as_millis(), "kg_context_enrich");

// src/agent_backend/kg_context.rs — total function time
// src/handlers/execution.rs — around tool_executor.execute
let t1 = tokio::time::Instant::now();
let output = tool_executor.execute(...).await;
tracing::info!(elapsed_ms = t1.elapsed().as_millis(), "llm_execute");

// src/handlers/workspace/messages.rs — inside the background tokio::spawn
// mark: embed_query, llm_call, embed_episode, store_episode individually

// agent-bestiary/memory/src/consolidation.rs — around clusterer.cluster()
let tc = tokio::time::Instant::now();
clusters = clusterer.cluster(...)?;
tracing::info!(elapsed_ms = tc.elapsed().as_millis(), episodes = failure_episodes.len(), "dbscan_cluster");
```

Record baseline p50/p95/p99 for `kg_context_enrich` and `llm_execute` from logs
before touching any code.

---

## Phase 1 — Eliminate the second embedding call on every execution
**Expected gain: ~300–800 ms off every agent execution and workspace message**

### 1.1 The problem

`enrich_with_kg_context` generates a query embedding (Voyage-2 API call, ~300–800 ms).
After execution returns, `execute_agent_handler` generates a *second* embedding of
`query + response` to store with the episode. Two serial API calls where one would do.

Call sites:
- `src/handlers/execution.rs:70` — KG enrich; then again at `:141`
- `src/handlers/execution_stream.rs:82` — KG enrich; then at `:206`
- `src/handlers/workspace/messages.rs:386` — KG enrich; then at `:499`

### 1.2 Change `enrich_with_kg_context` to return the query embedding

**`src/agent_backend/kg_context.rs`** — change return type:

```rust
// Before
pub async fn enrich_with_kg_context(...) -> AgentCard { ... }

// After — return both the card and the embedding used
pub async fn enrich_with_kg_context(
    store: &MemoryStore,
    embedder: &Arc<dyn EmbeddingGenerator>,
    agent_id: Uuid,
    query: &str,
    card: AgentCard,
) -> (AgentCard, Option<Vec<f32>>) {
    // ... existing logic ...
    // after generating query_embedding, thread it through to the return:
    (enriched_card, Some(query_embedding))
}
```

If KG is skipped early (empty ontology), return `(card, None)`.

### 1.3 Thread the cached embedding to episode storage

In each of the three call sites, receive the embedding from step 1.2 and pass it
to the episode struct directly, skipping the second `embedder.generate()` call:

```rust
// execution.rs
let (card, cached_query_embedding) = enrich_with_kg_context(...).await;
// ...
// When building episode:
episode.embedding = cached_query_embedding;
// Do NOT call embedder.generate() again — removed.
```

The episode embedding will be of the query only (not query+response). This is
acceptable — the primary use of the episode embedding is clustering for DBSCAN,
and query semantics dominate.

If you want to keep the richer query+response embedding for future retrieval, make
episode embedding generation fire-and-forget:

```rust
// After storing episode (which works with embedding = None initially):
let episode_id = store.store_episode(episode).await?;
let embedder_clone = state.embedder.clone();
let embed_text = format!("{} {}", body.query, output.metadata.reasoning.as_deref().unwrap_or(""));
tokio::spawn(async move {
    if let Ok(emb) = embedder_clone.generate(&embed_text).await {
        let _ = store.update_episode_embedding(episode_id, emb).await;
    }
});
```

This requires a `MemoryStore::update_episode_embedding(id, Vec<f32>)` method (trivial
`UPDATE episodes SET embedding = $2 WHERE episode_id = $1`).

**Decision:** Use the fire-and-forget pattern (richer episode embedding, zero hot-path
cost). The KG query embedding is still needed for KG retrieval so it cannot be skipped
entirely — but reusing it saves one API call per execution.

---

## Phase 2 — Push KG retrieval into the database
**Expected gain: eliminate MB of over-fetching; cut KG enrich time by 60–90%**

### 2.1 The problem

`enrich_with_kg_context` (kg_context.rs:45–91) currently:
1. Loads **all** active semantic rules for the agent with no LIMIT — an agent with
   1,000 rules transfers ~4 MB of embedding vectors per execution.
2. Loads **all** valid entities for the agent with no LIMIT — an agent with 2,000
   entities transfers ~8 MB per execution.
3. Computes cosine similarity in Rust and truncates to top 5 rules, top 8 entities.

The correct pattern is to push the similarity search to pgvector and transfer only
13 rows.

### 2.2 Pre-flight checks before writing any code

Run these two queries against the production database before writing a single line
of Rust. Both affect which code you write.

**Check 1 — pgvector version:**

```sql
SELECT extversion FROM pg_extension WHERE extname = 'vector';
```

- If `>= 0.5.0`: use HNSW (better query latency, correct choice here).
- If `< 0.5.0`: use IVFFlat with `lists = 100` instead. The SQL differs.
  Neon has been shipping pgvector ≥ 0.5.0 since late 2023 — this check is a
  guard, not an expectation.

**Check 2 — entity validity column name:**

```sql
SELECT column_name FROM information_schema.columns
WHERE table_name = 'kg_entities'
  AND column_name IN ('t_invalid', 't_expired');
```

The existing partial index on `kg_entities` is on `t_expired` but
`get_agent_entities` (store.rs:1441) filters on `t_invalid`. Confirm which column
name exists in production before writing the HNSW index `WHERE` clause and the
ANN query. Use the column that `get_agent_entities` actually queries — do not
guess.

### 2.3 Enable pgvector ANN on the retrieval queries

#### Add HNSW indices (run as a migration, not ad hoc)

This project's migrations are numbered sequentially. The last migration file is
`132_forage_observations.sql`. Create `migrations/133_hnsw_kg_indices.sql`:

```sql
-- Migration 133: HNSW vector indices for KG hot-path retrieval
--
-- Prereq: pgvector >= 0.5.0 (verify: SELECT extversion FROM pg_extension WHERE extname = 'vector')
-- Both indexes are created CONCURRENTLY — no table lock, safe in production.
-- ef_construction=64 / m=16 is a conservative starting point. Tune upward
-- if recall is insufficient (check via EXPLAIN ANALYZE on a real agent query).

-- Rules: only index active rules (matches the WHERE clause in every query)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_semantic_rules_embedding_hnsw
  ON semantic_rules
  USING hnsw (embedding vector_cosine_ops)
  WITH (m = 16, ef_construction = 64)
  WHERE is_active = true;

-- Entities: full index (validity filter varies, handled in query WHERE clause)
-- Replace <validity_column> with the actual column name from Check 2 above.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kg_entities_embedding_hnsw
  ON kg_entities
  USING hnsw (embedding vector_cosine_ops)
  WITH (m = 16, ef_construction = 64);
```

**Note:** `CONCURRENTLY` means no table lock. Build time is proportional to rows
with non-NULL embeddings. With < 10k rows it completes in seconds on Neon.

#### Add `MemoryStore` methods that use the ANN operator

In `agent-bestiary/memory/src/store.rs`, add two new methods. Read the existing
`get_agent_semantic_rules` (line 1300) and `get_agent_entities` (line 1441) before
writing these — match the exact column names, `row_to_*` deserialization helpers,
and `MemoryError` conversions already in use.

```rust
/// Top-k active rules for an agent, ranked by cosine similarity to query_embedding.
/// Uses pgvector HNSW ANN — transfers only k rows, not the full corpus.
pub async fn get_top_k_semantic_rules(
    &self,
    agent_id: Uuid,
    query_embedding: &[f32],
    k: i64,
    min_similarity: f32,
) -> Result<Vec<SemanticRule>> {
    let query_vec = pgvector::Vector::from(query_embedding.to_vec());
    let rows = sqlx::query(
        r#"
        SELECT rule_id, agent_id, rule_content, rule_description, confidence_score,
               verification_status, verification_method, source_episode_cluster,
               episode_count, embedding, is_active, created_at
        FROM semantic_rules
        WHERE agent_id = $2
          AND is_active = true
          AND 1 - (embedding <=> $1) >= $3
        ORDER BY embedding <=> $1
        LIMIT $4
        "#,
    )
    .bind(&query_vec)
    .bind(agent_id)
    .bind(min_similarity)
    .bind(k)
    .fetch_all(&self.pool)
    .await?;

    rows.iter().map(Self::row_to_semantic_rule).collect()
}
```

For entities, there are **two distinct populations** in the current code that must
be preserved exactly:

- **CEP seed entities** (`entity_type` starts with `"cep_"`) — always included in
  the prompt regardless of similarity score. These are validated reference data,
  not episodic knowledge. They must not be similarity-gated.
- **Episodic entities** (all others) — similarity-gated, top 8.

The current Rust code partitions on this at kg_context.rs:77–79. The ANN query
must replicate this logic. Use a `UNION ALL` to handle both populations in one
round-trip:

```rust
/// Top-k episodic entities (similarity-gated) PLUS all CEP seed entities
/// (always included). Uses a UNION ALL so both populations arrive in one query.
///
/// IMPORTANT: CEP entities (entity_type LIKE 'cep_%') are reference data and
/// must always be included regardless of similarity score. Removing or gating
/// them will silently degrade agent output quality with no error signal.
pub async fn get_top_k_entities_with_cep(
    &self,
    agent_id: Uuid,
    query_embedding: &[f32],
    k_episodic: i64,      // top-k for episodic entities (pass 8)
    min_similarity: f32,  // similarity floor for episodic (pass 0.30)
    validity_col: &str,   // "t_invalid" or "t_expired" — confirm with Check 2
) -> Result<Vec<Entity>> {
    // NOTE: validity_col cannot be parameterised in SQL — it is a column name,
    // not a value. Hard-code whichever column Check 2 confirms exists.
    // The query below uses t_invalid; replace if Check 2 says t_expired.
    let query_vec = pgvector::Vector::from(query_embedding.to_vec());
    let rows = sqlx::query(
        r#"
        -- Episodic entities: similarity-gated, top k
        SELECT entity_id, agent_id, entity_name, entity_type, summary,
               properties, embedding, t_valid, t_invalid, created_at
        FROM kg_entities
        WHERE agent_id = $2
          AND entity_type NOT LIKE 'cep_%'
          AND (t_invalid IS NULL OR t_invalid > NOW())
          AND 1 - (embedding <=> $1) >= $3
        ORDER BY embedding <=> $1
        LIMIT $4

        UNION ALL

        -- CEP seed entities: always included, not similarity-gated
        SELECT entity_id, agent_id, entity_name, entity_type, summary,
               properties, embedding, t_valid, t_invalid, created_at
        FROM kg_entities
        WHERE agent_id = $2
          AND entity_type LIKE 'cep_%'
          AND (t_invalid IS NULL OR t_invalid > NOW())
        "#,
    )
    .bind(&query_vec)
    .bind(agent_id)
    .bind(min_similarity)
    .bind(k_episodic)
    .fetch_all(&self.pool)
    .await?;

    rows.iter().map(Self::row_to_entity).collect()
}
```

**Do not merge CEP and episodic entities into a single `ORDER BY embedding <=> $1
LIMIT k` query.** If you do, CEP entities will only appear when they happen to be
among the top-k by similarity, which is not the contract. They must always appear.

#### Rewrite `enrich_with_kg_context` to use the new methods

Replace the current load-all + rank-in-Rust pipeline in `kg_context.rs`:

```rust
// After generating query_embedding:
let (rules_res, entities_res) = tokio::join!(
    store.get_top_k_semantic_rules(agent_id, &query_embedding, 5, MIN_SIMILARITY),
    store.get_top_k_entities_with_cep(agent_id, &query_embedding, 8, MIN_SIMILARITY),
);

let top_rules = rules_res.unwrap_or_default();
let all_entities = entities_res.unwrap_or_default();

// Partition for the prompt builder — CEP entities still render separately.
// The UNION ALL query returns both populations; re-partition here for display:
let (cep_entities, episodic_entities): (Vec<_>, Vec<_>) = all_entities
    .iter()
    .partition(|e| e.entity_type.starts_with("cep_"));

// Delete the old cosine scoring loops, sort_by calls, and truncate calls.
// top_rules is already ordered by similarity and limited to 5.
// episodic_entities is already ordered by similarity and limited to 8.
// cep_entities contains all CEP seeds with no limit.
```

The in-memory `cosine_similarity` function, the two `scored_rules`/`scored_entities`
Vec constructions, the `sort_by` calls, and the `truncate` calls are all deleted.
The prompt-building logic (kg_context.rs:97–177) is unchanged — it still receives
`top_rules`, `cep_entities`, and `scored_entities` (now `episodic_entities`) in
the same shape.

### 2.4 Add the missing partial composite index on `episodes`

Add to the same migration or a separate one:

```sql
-- Drop the two separate indices if they exist and replace with one composite partial index
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_episodes_agent_unconsolidated
  ON episodes(agent_id, timestamp_ref DESC)
  WHERE NOT consolidated;
```

This is the index used by `get_unconsolidated_episodes`. Without it, Postgres scans
all episodes for an agent to find the unconsolidated subset.

Also add a composite partial index on semantic_rules to replace the two separate
indices that currently force Postgres to choose between a full-agent scan + filter
or a global partial index:

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_semantic_rules_agent_active_conf
  ON semantic_rules(agent_id, confidence_score DESC)
  WHERE is_active = true;
```

---

## Phase 3 — Consolidation: async job + bulk inserts + spawn_blocking
**Expected gain: consolidation no longer blocks HTTP responses; DBSCAN off executor threads**

### 3.1 Make the consolidation endpoint async (202 pattern)

**This is a breaking API contract change.** Read the full note before implementing.

Currently `POST /api/agents/:id/consolidate` (consolidation.rs:296) returns the
full consolidation result synchronously in the response body:

```json
{
  "status": "completed",
  "result": {
    "episodes_processed": 14,
    "clusters_identified": 3,
    "rules_extracted": 7,
    ...
  },
  "dreaming_credits_remaining": 4
}
```

After this change it will return immediately with:

```json
{
  "status": "accepted",
  "job_id": "<uuid>",
  "message": "Consolidation started. Poll GET /api/agents/:id/consolidation/jobs/:job_id for status."
}
```

**Any caller that reads `result.episodes_processed` etc. from the POST response
body will break.** Before merging this change, audit every caller:

- `src/handlers/creatures/agent_modules.rs:1383` — calls `worker.consolidate_agent`
  directly (not via HTTP), so it is **not affected** by the endpoint change.
- The frontend (`static/` and `templates/`) — search for `consolidate` and
  `episodes_processed`. If any template reads the POST response body fields,
  it must switch to polling the job status endpoint.
- Any external scripts or tooling that hit this endpoint.

The job row already exists in the memory store (`consolidation_jobs` table). The
`MemoryStore::get_consolidation_job(job_id)` method (if it exists) or a direct
`sqlx::query` on `consolidation_jobs WHERE job_id = $1` is sufficient for the
polling endpoint — no new table needed.

**In `src/handlers/consolidation.rs`:**

```rust
pub async fn trigger_consolidation_handler(...) -> Result<Json<Value>, ...> {
    // All existing validation (agent lookup, budget check, gas charge) unchanged.
    // The gas is charged before spawn so a failed/never-started job does not
    // silently consume credits without a visible charge.

    // Reuse the job_id from the ConsolidationWorker's create_consolidation_job call,
    // or generate one here and pass it in — match however the existing job tracking works.
    let job_id = Uuid::new_v4(); // or use the id returned by worker internals

    let spawn_state = state.clone();
    let spawn_agent_id = db_agent.agent_id;
    tokio::spawn(async move {
        // Build worker (same logic as before — LLM if API key available)
        match worker.consolidate_agent(spawn_agent_id, 0.5, 2).await {
            Ok(result) => {
                // Update job status to complete in DB
                // Spawn dream narrator (same tokio::spawn block as before, moved here)
            }
            Err(e) => {
                // Update job status to failed in DB
                tracing::error!(agent_id = %spawn_agent_id, error = %e, "consolidation failed");
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "accepted",
            "job_id": job_id,
            "message": "Consolidation started.",
            "poll": format!("/api/agents/{}/consolidation/jobs/{}", agent_id, job_id),
        })),
    ))
}
```

Add a `GET /api/agents/:id/consolidation/jobs/:job_id` route returning the job
row. Register it in `api_server.rs` next to the existing consolidation routes.

### 3.2 Wrap DBSCAN in `spawn_blocking`

In `agent-bestiary/memory/src/consolidation.rs:123`:

```rust
// Before
let clusterer = DBSCANClustering::new(epsilon, min_samples);
clusters = clusterer.cluster(failure_episodes)?;

// After
let failure_episodes_for_cluster = failure_episodes.clone();
clusters = tokio::task::spawn_blocking(move || {
    let clusterer = DBSCANClustering::new(epsilon, min_samples);
    clusterer.cluster(failure_episodes_for_cluster)
})
.await
.map_err(|join_err| MemoryError::InternalError(join_err.to_string()))??;
```

DBSCAN is O(n²·d). `spawn_blocking` hands it to Tokio's dedicated blocking thread
pool, keeping both the UX runtime and any loop runtime free during the cluster phase.

### 3.3 Batch inserts for rules, entities, and facts

Currently consolidation issues one `INSERT` per row. Replace with bulk inserts.

**Before writing the batch methods, read the existing single-row insert methods:**
- `store_semantic_rule` at store.rs:1253 — gives you the exact column names and
  bind order to replicate in the batch version.
- `store_entity` at store.rs:1381
- `store_fact` at store.rs:1491

The column names in the spec's example code (`rule_text`, `rule_type`) may not
match the actual schema. Use the column names from the existing `INSERT` statements,
not the names in this document.

**`agent-bestiary/memory/src/store.rs`** — add batch methods:

```rust
pub async fn store_semantic_rules_batch(&self, rules: Vec<SemanticRule>) -> Result<usize> {
    if rules.is_empty() { return Ok(0); }
    // Column list must exactly match store_semantic_rule's INSERT (store.rs:1255-1259)
    let mut qb = sqlx::QueryBuilder::new(
        "INSERT INTO semantic_rules \
         (rule_id, agent_id, rule_content, rule_description, confidence_score, \
          verification_status, verification_method, source_episode_cluster, \
          episode_count, embedding, is_active) "
    );
    qb.push_values(rules.iter(), |mut b, r| {
        b.push_bind(r.rule_id)
         .push_bind(r.agent_id)
         .push_bind(&r.rule_content)
         .push_bind(&r.rule_description)
         .push_bind(r.confidence_score)
         .push_bind(r.verification_status.to_string())
         .push_bind(&r.verification_method)
         .push_bind(&r.source_episode_cluster)
         .push_bind(r.episode_count)
         .push_bind(r.embedding.as_ref().map(|e| pgvector::Vector::from(e.clone())))
         .push_bind(r.is_active);
    });
    // ON CONFLICT DO NOTHING preserves existing rules if somehow duplicate IDs arise.
    // This matches the intent of the single-row version (which would error on conflict).
    qb.push(" ON CONFLICT (rule_id) DO NOTHING");
    let n = qb.build().execute(&self.pool).await?.rows_affected() as usize;
    Ok(n)
}
```

**Parameter limit guard:** `sqlx::QueryBuilder` with Postgres uses `$1`-style
numbered parameters. Postgres supports up to 65535 parameters per query. With 11
columns per rule, a single batch can hold at most `65535 / 11 = 5957` rules before
hitting the limit. A consolidation run producing > 5957 rules is currently
impossible (LLM budget is far smaller), but add a chunk guard for safety:

```rust
// In consolidation.rs — chunk the batch just in case
for chunk in all_rules.chunks(500) {
    self.store.store_semantic_rules_batch(chunk.to_vec()).await?;
}
```

Apply the same pattern for `store_entities_batch` and `store_facts_batch`, and
add equivalent chunked calls in `consolidation.rs`.

In `consolidation.rs`, collect all rules/entities/facts from all clusters into
`Vec`s before any inserts, then call the batch methods once each:

```rust
let mut all_rules: Vec<SemanticRule> = Vec::new();
for cluster in &clusters {
    all_rules.extend(self.extract_rules_from_cluster(agent_id, cluster).await?);
}
// Single batch insert — was N individual round-trips
for chunk in all_rules.chunks(500) {
    self.store.store_semantic_rules_batch(chunk.to_vec()).await?;
}
result.rules_extracted = all_rules.len(); // update result counter after collection
```

### 3.4 Batch embed entities in the heuristic fallback path

In `consolidation.rs:462`, entity embedding currently calls `embedder.generate()`
once per capitalized word sequentially. Replace with parallel futures:

```rust
// Collect all candidate entity names first
let candidate_names: Vec<String> = text
    .split_whitespace()
    .filter(|w| w.len() > 3 && w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
    .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
    .filter(|s| s.len() > 3)
    .collect();

// Embed all in parallel
let embed_futures: Vec<_> = candidate_names.iter()
    .map(|name| embedder.generate(name))
    .collect();
let embeddings = futures::future::join_all(embed_futures).await;
```

---

## Phase 4 — Connection pool and blocking I/O fixes
**Expected gain: prevent SSE from exhausting Postgres connections; remove git blocking**

### 4.1 Replace per-SSE-client raw Postgres connections with shared listener

**Problem:** `messages.rs:944` opens a new raw `PgListener` connection per SSE client.
With ≥26 concurrent SSE subscribers (26 + pool's 25 = 51 connections), Neon's free
tier (100 connections) is at ~50% capacity; any concurrent consolidation or LLM
burst can tip it over.

**Semantics difference — read this before implementing:**

The current per-client `PgListener` gives each SSE client its own Postgres
connection. Postgres delivers notifications on that connection starting from when
it was opened — there is no buffering, no lag, no drops. The client either receives
the notification or it misses it (e.g. disconnect).

`tokio::broadcast` has different semantics: it is a ring buffer. If a slow receiver
falls behind by more than `capacity` messages, it receives `RecvError::Lagged(n)`
where `n` is the number of dropped messages. You **must** handle this case or the
SSE stream will silently stall.

**Required handling for `RecvError::Lagged`:**

When a receiver lags, treat it as a reconnect: re-fetch the backfill window from
the database and re-emit any messages the client missed. The existing backfill
logic (`messages.rs:963–969`) already does this on initial connect — factor it into
a reusable `fetch_backfill(pool, ws_uuid, since_message_id)` function and call it
from both the initial connect path and the lag recovery path.

```rust
// Inside the SSE stream loop, on the shared broadcast path:
match rx.recv().await {
    Ok((channel, payload)) => {
        // filter + emit as before
    }
    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
        tracing::warn!(
            workspace_id = %ws_uuid,
            dropped = n,
            "SSE receiver lagged — re-fetching backfill"
        );
        // Re-fetch messages since the last message_id this client confirmed.
        // Yield each missed message into the SSE stream before resuming.
        let backfill = fetch_backfill(&pool, ws_uuid, &last_seen_id).await;
        for msg in backfill {
            // dedup via seen_ids, then yield
        }
        // Continue — do NOT break the stream.
    }
    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
        // Sender dropped (server shutting down). Close gracefully.
        break;
    }
}
```

**Broadcast channel capacity:** Set to `2048`. With a 30 s keepalive interval and
a realistic message rate of ~10 msg/s across all workspaces, a receiver would need
to stall for ~3 minutes before lagging. In practice a stalled SSE client will
either reconnect or be dropped by the proxy timeout (Railway/Neon default ~60 s).
The lag path is a safety net, not the common case.

**Implementation:**

Add to `AppState` in `api_server.rs`:

```rust
pub(crate) struct AppState {
    // ... existing fields ...
    /// Broadcast channel for all Postgres NOTIFY events.
    /// Payload is (channel_name, json_payload_string).
    /// Subscribe via state.pg_notify.subscribe() in SSE handlers.
    pub(crate) pg_notify: tokio::broadcast::Sender<(String, String)>,
}
```

Start the single shared listener in `main()` before building `AppState`:

```rust
let (pg_notify_tx, _pg_notify_rx) = tokio::broadcast::channel::<(String, String)>(2048);

// Spawn background listener — ONE connection total, reconnects on error
let pg_notify_tx_bg = pg_notify_tx.clone();
let db_url_for_listener = database_url.clone();
tokio::spawn(async move {
    loop {
        match sqlx::postgres::PgListener::connect(&db_url_for_listener).await {
            Ok(mut listener) => {
                // Listen on all channels used by SSE handlers.
                // Add channels here if new NOTIFY call sites are added.
                if let Err(e) = listener
                    .listen_all(&["workspace_messages", "creature_events", "rabble_events"])
                    .await
                {
                    tracing::error!("pg_listener listen_all failed: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
                tracing::info!("shared pg_listener connected");
                loop {
                    match listener.recv().await {
                        Ok(notif) => {
                            // Send to all subscribers. Ignore send errors
                            // (RecvError::Closed means no subscribers yet, which is fine).
                            let _ = pg_notify_tx_bg.send((
                                notif.channel().to_string(),
                                notif.payload().to_string(),
                            ));
                        }
                        Err(e) => {
                            tracing::warn!("pg_listener recv error: {e}, reconnecting");
                            break; // break inner loop → reconnect outer loop
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("pg_listener connect failed: {e}");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
});

let state = AppState {
    // ...
    pg_notify: pg_notify_tx,
};
```

In `workspace_messages_stream_handler` (messages.rs ~line 940), replace the
per-client `PgListener::connect` block with:

```rust
// Before (per-client connection):
let pg_listener = {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_default();
    sqlx::postgres::PgListener::connect(&db_url).await.ok()
};

// After (subscribe to shared broadcast):
let mut pg_rx = state.pg_notify.subscribe();
// No Postgres connection opened here at all.
```

Update the `tokio::select!` inside the stream to use `pg_rx.recv()` instead of
`listener.recv()`, and handle `RecvError::Lagged` as described above. The
per-workspace channel name filter (`if event.workspace_id == ws_uuid`) continues
to work — it now filters the broadcast payload by deserializing the JSON and
checking the workspace ID field, same as the existing in-process broadcast path.

This reduces Postgres LISTEN connections from O(active SSE subscribers) to 1,
regardless of how many users are connected.

### 4.2 Wrap `workspace_git.commit_file` in `spawn_blocking`

**First, read `WorkspaceGitManager::commit_file` in full.** The audit shows it is
called both as a synchronous function (`workspace/actions.rs:193`, `apps.rs:483`)
and with `spawn_blocking` in `tools_legacy.rs:4847`. Determine whether `commit_file`
is currently `fn` (sync) or `async fn` before wrapping it.

If it is a synchronous `fn`: it uses `git2` crate calls and `std::fs::write`
internally, both of which are blocking syscalls. Wrap every call site that is
inside an `async` context.

The following call sites are inside async contexts and currently execute blocking
FS/git work without `spawn_blocking`:
- `messages.rs:545` — inside `tokio::spawn` background task
- `messages.rs:1227` — inside HTTP handler directly  
- `messages.rs:1433` — inside HTTP handler directly
- `workspace/core.rs:542` — inside HTTP handler directly
- `workspace/actions.rs:193`, `:306`, `:714`, `:997`, `:1006` — inside HTTP handlers
- `workspace/coherence.rs:655`, `:660`, `:861` — inside HTTP handlers

`tools_legacy.rs:4847` already wraps in `spawn_blocking` — use it as the reference
pattern.

```rust
// Pattern (from tools_legacy.rs:4847 — already correct):
tokio::task::spawn_blocking(move || {
    git.commit_file(&slug, &path, &content, &message)
})
.await
.ok();
```

Apply this pattern to every call site listed above. Do not wrap calls that are
already inside a `spawn_blocking` closure or inside `std::thread::spawn`.

### 4.3 Fix the at-mention regex recompilation

In `src/handlers/workspace/messages.rs`, the regex is currently constructed inside
the handler on every call. Move it to a module-level `Lazy`:

```rust
use std::sync::OnceLock;
use regex::Regex;

static AT_MENTION_RE: OnceLock<Regex> = OnceLock::new();

fn at_mention_regex() -> &'static Regex {
    AT_MENTION_RE.get_or_init(|| Regex::new(r"@(\w+)").expect("valid regex"))
}
```

Replace `Regex::new(...)` call sites with `at_mention_regex()`.

---

## Phase 5 — AppState clone cost (minor, do last)

**Findings 8.1 / 5.1** — `AppState::clone()` heap-copies `String` fields at 31 call sites.

Wrap config strings in `Arc<str>`:

```rust
// api_server.rs AppState definition
pub(crate) struct AppState {
    // ... Arc<T> fields unchanged ...
    pub(crate) gemini_api_key: Arc<str>,
    pub(crate) jwt_secret: Arc<str>,
    pub(crate) oauth: Arc<OAuthConfig>,
    pub(crate) stripe: Arc<StripeConfig>,
}
```

Construction sites pass `api_key.into()` / `Arc::new(oauth_config)`. No call-site
changes needed — `Deref` still gives `&str`.

This is a minor allocation win (saves ~200–400 bytes per `state.clone()` call) and
is worth doing as cleanup but should not be prioritised over Phases 1–4.

---

## Phase 6 — N+1 query patterns in list endpoints (UX latency for management views)

These do not affect the agent execution hot path but do affect perceived UX latency
on pages that list workspaces or agents.

### 6.1 `list_workspaces_handler` — 3 queries per workspace

`src/handlers/workspace/core.rs:58–137` issues 3 queries per workspace in a loop.
Replace with a single query using lateral joins or a CTE:

```sql
WITH user_workspaces AS (
    SELECT t.id, t.workspace_budget, t.workspace_spent, t.origin, t.name, t.description,
           t.created_at
    FROM teams t
    JOIN team_members tm ON t.id = tm.team_id
    WHERE tm.user_id = $1
),
agent_counts AS (
    SELECT workspace_id, COUNT(*) AS agent_count
    FROM workspace_agents
    WHERE workspace_id = ANY(SELECT id FROM user_workspaces)
    GROUP BY workspace_id
),
agent_names AS (
    SELECT wa.workspace_id, a.agent_name, a.display_alias, a.avatar_emoji
    FROM workspace_agents wa
    JOIN agents a ON a.agent_id = wa.agent_id
    WHERE wa.workspace_id = ANY(SELECT id FROM user_workspaces)
)
SELECT uw.*, ac.agent_count, 
       COALESCE(json_agg(an.*) FILTER (WHERE an.agent_name IS NOT NULL), '[]') AS agents
FROM user_workspaces uw
LEFT JOIN agent_counts ac ON ac.workspace_id = uw.id
LEFT JOIN agent_names an ON an.workspace_id = uw.id
GROUP BY uw.id, uw.workspace_budget, uw.workspace_spent, uw.origin, uw.name,
         uw.description, uw.created_at, ac.agent_count
```

This is one round-trip instead of 1 + 3N.

### 6.2 `hire_agent_handler` — 2 queries per dependency

`src/handlers/workspace/messages.rs:1254` issues 2 queries per dependency in a loop.
Batch both lookups:

```rust
// Fetch all agents by name in one query
let dep_agents = sqlx::query_as::<_, DbAgent>(
    "SELECT * FROM agents WHERE agent_name = ANY($1)"
)
.bind(&dep_names as &[String])
.fetch_all(&state.db).await?;

let dep_ids: Vec<Uuid> = dep_agents.iter().map(|a| a.agent_id).collect();

// Check all membership in one query
let already_in: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(
    "SELECT agent_id FROM workspace_agents WHERE workspace_id = $1 AND agent_id = ANY($2)"
)
.bind(ws_uuid)
.bind(&dep_ids as &[Uuid])
.fetch_all(&state.db).await?
.into_iter().collect();
```

---

## Validation checklist

For each phase, before marking complete:

**Phase 1 (embedding reuse):**
- [ ] Log output shows `kg_context_enrich` span appears once per execution, not twice
- [ ] Episode embeddings are eventually populated (fire-and-forget pattern working)
- [ ] No regression in KG retrieval quality (same top-5 rules returned)

**Phase 2 (pgvector ANN):**
- [ ] `EXPLAIN ANALYZE` on `get_top_k_semantic_rules` shows index scan, not seq scan
- [ ] Total rows transferred per execution for KG = ≤13 rows (not hundreds/thousands)
- [ ] `kg_context_enrich` span reduced by ≥60% vs Phase 0 baseline

**Phase 3 (async consolidation):**
- [ ] `POST /consolidate` returns within 500 ms
- [ ] Job status endpoint returns `complete` after full consolidation cycle
- [ ] `dbscan_cluster` span no longer appears on Tokio executor threads
      (verify with `tokio-console` or thread name logging)
- [ ] Consolidation DB writes reduced from N INSERT round-trips to ≤4 batch queries

**Phase 4 (connections + blocking I/O):**
- [ ] `pg_stat_activity` shows ≤2 LISTEN connections (1 for the shared listener +
      1 for tests) regardless of SSE subscriber count
- [ ] `workspace_git.commit_file` no longer blocks tokio threads
      (visible in `tokio-console` as a `spawn_blocking` call)
- [ ] At-mention regex: no `regex::Regex::new` in hot path profiling

**Phase 5 (AppState):**
- [ ] `cargo clippy` passes with no new warnings
- [ ] All 31 `state.clone()` sites compile and tests pass

---

## What this spec does not change

- The five loop hierarchy and their timescales — unchanged
- Loop corrective mechanisms and human gates — unchanged
- The provisional prior status of all thresholds — unchanged
- The append-only event store and CQRS architecture — unchanged
- Agent identity, episodic memory, dreaming cycles — unchanged
- The ABW single-runtime decision — unchanged pending profiling evidence
  that a second runtime would help (Phase 1–4 are likely to resolve the
  observable latency without it)

---

## Pre-implementation checklist for the implementing agent

Before writing any code, confirm the following. Document answers as inline comments
in the relevant files.

**[ ] 1. pgvector version**  
`SELECT extversion FROM pg_extension WHERE extname = 'vector';`  
Must be ≥ 0.5.0 for HNSW. If older, use IVFFlat in migration 133.

**[ ] 2. Entity validity column name**  
`SELECT column_name FROM information_schema.columns WHERE table_name = 'kg_entities' AND column_name IN ('t_invalid', 't_expired');`  
The ANN query (Phase 2) uses this column. Use whichever name exists. Do not assume.

**[ ] 3. `commit_file` signature**  
Read `WorkspaceGitManager::commit_file`. Is it `fn` or `async fn`? This determines
whether the `spawn_blocking` wrapper in Phase 4.2 is needed or whether the function
is already async (but still blocking internally).

**[ ] 4. `enrich_with_kg_context_by_name` call sites**  
`tools_legacy.rs:4190` and `:4381` call the `_by_name` variant. Confirm whether
either is on a hot path (called per-execution) or only in management/tool contexts.
If hot path: apply the Phase 2 ANN rewrite to it as well. If management only: leave
it using the existing load-all path — it is not performance-critical.

**[ ] 5. Phase 1 signature change — 7 call sites**  
The `enrich_with_kg_context` return type changes from `AgentCard` to
`(AgentCard, Option<Vec<f32>>)`. Every call site must be updated. The compiler
will catch missed sites. Confirm all 7 compile before merging:
- `src/handlers/execution.rs:70`
- `src/handlers/execution_stream.rs:82`
- `src/handlers/workspace/messages.rs:386`
- `src/handlers/rabble_workspace.rs:249`
- `src/handlers/workspace/core.rs` (find exact line with `rg enrich_with_kg`)
- `src/handlers/workspace/coherence.rs`
- `src/agent_backend/tools_legacy.rs:4381`

For call sites that do not use the episode embedding (e.g. `tools_legacy.rs:4381`,
which dispatches within the tool executor and has no episode storage path), discard
the second return value with `let (card, _) = enrich_with_kg_context(...).await;`.

**[ ] 6. Consolidation frontend contract**  
Search `static/` and `templates/` for `episodes_processed`, `clusters_identified`,
`rules_extracted`. If any template or JS reads these from the POST response body,
it must switch to polling before Phase 3.1 is deployed.

**[ ] 7. `ObservabilityWorker::scan_agent` call context**  
`eval.rs:744` calls it inside `tokio::spawn`. `observatory.rs:159` calls it — confirm
whether that handler spawns or awaits directly. The settling engine CPU work inside
`scan_agent` needs `spawn_blocking` only if it runs on the async executor; if it is
already dispatched from within a `tokio::spawn`, it still blocks a worker thread
but does not block the HTTP handler.
