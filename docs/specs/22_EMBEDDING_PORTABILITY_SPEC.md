# Spec 22 — Embedding Portability (Provenance-First Anti-Lock-In)
**Status:** Ready for implementation
**Date:** 2026-06-11
**Priority:** High — every embedding written before this ships is unrecoverable in the spec's sense.
**Implementer note:** Phase 1 is mandatory before any further embedding writes. Phases 2–6 are ordered by trigger (build when needed), not by sequence. Do not skip Phase 0 design decisions.

---

## Context

ABW currently writes ~14 distinct embedding generation paths into 5 vector-bearing tables (`episodes`, `semantic_rules`, `entities`, `communities`, `shopping_profiles`). An audit against HEAD (2026-06-11) found:

- **No row records `model_id`, `model_version`, or `dim`.** All 5 tables have `vector(1024)` columns with zero provenance metadata.
- **No row reliably persists the exact `source_text` that was embedded.** `episodes.query` holds the user query, but the actual embedded string is `format!("{} {}", query, reasoning)` where `reasoning` lives inside `context JSONB` — reconstruction is lossy.
- **The `EmbeddingGenerator` trait exposes `provider_name()`** returning hardcoded strings like `"anthropic/voyage-2"`, but nothing reads it at write time. The `QwenEmbeddings::provider_name()` returns `"qwen/text-embedding-v2"` while the default model is `text-embedding-v3` — a latent mismatch.
- **No embedding cache exists.** Same `(model, text)` pairs are re-embedded across the day.
- **No re-embed worker exists.** `scripts/migrate_embedding_dimensions.rs` references `cargo run --bin re_embed_episodes` (line 262), but that binary is not declared anywhere.
- **The only existing migration path is destructive:** `NULL` every embedding then `ALTER COLUMN` the dim. It does not regenerate vectors, and it silently skips `shopping_profiles.composite_embedding`.
- **Bycatch bugs:** `src/workflows/fork.rs:152-188` references non-existent table `rules` and wrong column names on `episodes`; `fermi-memory/src/store.rs:46-75, :151-185` accepts an `Episode`/`SemanticRule` with `embedding` but never binds the vector column; `MemoryStore::store_semantic_rule` never binds `user_id` despite the column existing.

The spec's central warning applies: vectors written before provenance ships **cannot be reproduced losslessly**. Every day of delay enlarges the set of permanently-irreproducible vectors.

This spec implements **Tier 0 (provenance store) in full** as a mandatory pre-condition for any further embedding writes, builds **Tier 1 (re-embed worker)** as a deferred-but-scoped follow-up, stands up the **closed-model anchor set** as cheap standing insurance, and reserves **Tier 2 (translator)** as an optional hedge.

---

## Design decisions (locked in Phase 0)

These were resolved before drafting; recording them here so the implementer does not relitigate them.

1. **Provenance shape: hybrid.** Both per-row columns on each of the 5 vector tables (denormalised, cheap query-time access, single source of truth for "what produced *this current* vector") AND an append-only sidecar `embedding_provenance` event table (full re-embedding history per logical chunk, matches the existing event-store / CQRS architecture). Re-embeds INSERT a new provenance row AND UPDATE the columns on the target row in the same transaction.
2. **`model_version` policy: manual epoch strings.** Voyage, OpenAI, Mistral, and Qwen do not expose stable embedding-model version strings via their APIs. We capture `model_version` as a `"YYYY-MM-DD"` epoch managed by a single Rust constant per provider (`VOYAGE_MODEL_VERSION = "2024-01-01"`). Bumped manually in code when we suspect or observe vendor drift. This is honest about what we know.
3. **Reference open model for anchors: `nomic-embed-text-v1.5`** (768d, fully open, self-hostable). Not currently running — Phase 2.0 stands it up as part of the anchor-set work.
4. **Scope of Phase 1: all 5 vector tables in one go.** Partial coverage leaves the same insurance gap the spec warns against; the migration is the expensive part regardless of how many tables it touches.
5. **Drive-by fixes bundled:** Qwen `provider_name` mismatch, `fork.rs` schema mismatch, `fermi-memory` silent-drop of vectors, `semantic_rules.user_id` never bound. All in files we are already editing; cleaning them up is cheaper now than tracking them as separate work.

---

## Phase 0 — Baseline instrumentation and inventory verification (do before any code change)

### 0.1 Confirm the provider/model wiring at runtime

Run the server with `RUST_LOG=info` and confirm the startup banner reports the embedder identity (`src/api_server.rs:722-730`). Record:
- Provider (`anthropic` or `mock`)
- Model (`voyage-2` expected)
- Dim (`1024` expected)

If `MockEmbeddings` is in use, abort Phase 1 implementation until `ANTHROPIC_API_KEY` is set — re-embedding mocks vs real vectors is a different problem and a separate spec.

### 0.2 Pin the current production model identity in a header comment

In `agent-bestiary/memory/src/embeddings.rs`, top of file, add a comment block recording the production model identity at the time of this work (so future readers know what "unknown_pre_provenance" rows came from):

```rust
// Production embedding identity (as of 2026-06-11, pre-Spec-22):
//   provider: anthropic
//   model:    voyage-2
//   dim:      1024
//   version:  unknown (vendor does not expose; assumed stable since deployment)
// All embeddings in the database written before Spec 22 lands are stamped
// (model_id="anthropic/voyage-2", model_version="unknown_pre_provenance",
//  provenance_trusted=false) by the backfill migration.
```

### 0.3 Verify pre-existing row counts (so backfill cost is known)

```sql
SELECT 'episodes' tbl, COUNT(*) total, COUNT(embedding) with_embedding
FROM episodes
UNION ALL SELECT 'semantic_rules', COUNT(*), COUNT(embedding) FROM semantic_rules
UNION ALL SELECT 'entities', COUNT(*), COUNT(embedding) FROM entities
UNION ALL SELECT 'communities', COUNT(*), COUNT(embedding) FROM communities
UNION ALL SELECT 'shopping_profiles', COUNT(*), COUNT(composite_embedding) FROM shopping_profiles;
```

Document the counts in `docs/EMBEDDING_PROVENANCE_BACKFILL_NOTES.md` so Phase 1b sizing is known.

### 0.4 Add timing spans to the embedding hot path

At each of the 14 `.generate(...)` call sites listed in §1 of the survey, add a `tracing::info!` timing span. After Phase 1 ships, these spans will reveal whether the cache (Phase 4) is hitting.

```rust
let t0 = tokio::time::Instant::now();
let provenanced = state.embedder.generate_provenanced(&source_text).await?;
tracing::info!(
    elapsed_ms = t0.elapsed().as_millis(),
    model = %provenanced.model_id,
    site = "execution_handler",
    "embed_call"
);
```

**Do not proceed to Phase 1 until 0.1–0.4 are complete.**

---

## Phase 1 — Provenance store (Tier 0, mandatory pre-MVP)

This is the only rung the spec requires before launch. Everything else is deferred.

### 1.1 Extend the `EmbeddingGenerator` trait

`agent-bestiary/memory/src/embeddings.rs`. Replace `provider_name(&self) -> &str` with three explicit methods, and add a high-level convenience method that returns provenance bundled with the vector.

```rust
#[async_trait]
pub trait EmbeddingGenerator: Send + Sync {
    /// Globally unique model identifier in the form "<provider>/<model>".
    /// MUST be stable across restarts. MUST match what is persisted in
    /// embedding_provenance.model_id.
    fn model_id(&self) -> &str;

    /// Manual epoch version string, format "YYYY-MM-DD" or "YYYY-MM-DD/<note>".
    /// Bumped in code when we suspect or observe vendor drift.
    fn model_version(&self) -> &str;

    /// Output dimensionality. MUST match the dim of every vector returned.
    fn dim(&self) -> i32;

    async fn generate(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    async fn generate_batch(&self, texts: &[String])
        -> Result<Vec<Vec<f32>>, EmbeddingError>;

    /// Convenience: bundles the vector with full provenance. Prefer this over
    /// `generate` at call sites that persist the result.
    async fn generate_provenanced(&self, text: &str)
        -> Result<ProvenancedEmbedding, EmbeddingError>
    {
        let vector = self.generate(text).await?;
        Ok(ProvenancedEmbedding {
            vector,
            source_text: text.to_string(),
            model_id: self.model_id().to_string(),
            model_version: self.model_version().to_string(),
            dim: self.dim(),
        })
    }
}
```

`provider_name()` is removed. Implementers must update each of the four real impls + the mock:

| Impl | `model_id()` | `model_version()` | `dim()` |
|---|---|---|---|
| `AnthropicEmbeddings` | `"anthropic/voyage-2"` (or configured model with `anthropic/` prefix) | const `VOYAGE_MODEL_VERSION = "2024-01-01"` | self.dimensions |
| `OpenAIEmbeddings` | `"openai/text-embedding-3-large"` (or configured) | const `OPENAI_EMBED_VERSION = "2024-01-01"` | self.dimensions |
| `MistralEmbeddings` | `"mistral/mistral-embed"` (or configured) | const `MISTRAL_EMBED_VERSION = "2024-01-01"` | self.dimensions |
| `QwenEmbeddings` | `"qwen/text-embedding-v3"` (or configured) | const `QWEN_EMBED_VERSION = "2024-01-01"` | self.dimensions |
| `MockEmbeddings` | `"mock/deterministic-hash"` | `"mock-v1"` | self.dim |

**Drive-by fix:** `QwenEmbeddings::provider_name()` previously returned `"qwen/text-embedding-v2"` while the default model was `text-embedding-v3` (`embeddings.rs:381` vs `:290`). The new `model_id()` derives from `self.model` so the mismatch is impossible.

The version constants live at the top of `embeddings.rs`:

```rust
// Manual epoch versions for embedding models. Vendors do not expose stable
// model-version strings via API. Bump these manually when:
//   (a) we observe quality drift on benchmarks
//   (b) a vendor announces a model update
//   (c) we switch to a measurably different snapshot
// Bumping a version string is a signal to the re-embed worker (Phase 3) that
// existing vectors stamped with the old version are eligible for refresh.
const VOYAGE_MODEL_VERSION:  &str = "2024-01-01";
const OPENAI_EMBED_VERSION:  &str = "2024-01-01";
const MISTRAL_EMBED_VERSION: &str = "2024-01-01";
const QWEN_EMBED_VERSION:    &str = "2024-01-01";
```

### 1.2 Introduce `ProvenancedEmbedding`

`agent-bestiary/memory/src/embeddings.rs`:

```rust
/// A vector bundled with the full provenance required by Spec 22.
///
/// This type is the only way to obtain an embedding intended for persistence.
/// Storing fns (`store_episode`, `store_semantic_rule`, etc.) accept this type;
/// they do not accept a bare `Vec<f32>`. The compiler enforces the discipline
/// that the spec calls "no code path that skips provenance."
#[derive(Debug, Clone)]
pub struct ProvenancedEmbedding {
    pub vector: Vec<f32>,
    /// The EXACT text passed to the embedder. Not a reconstruction.
    pub source_text: String,
    /// "<provider>/<model>", e.g. "anthropic/voyage-2".
    pub model_id: String,
    /// Manual epoch, e.g. "2024-01-01".
    pub model_version: String,
    /// Output dimensionality. Guards against silent model swaps.
    pub dim: i32,
}
```

The bare `generate()` is retained for two read-only call sites (`kg_context.rs:57`, `tools_legacy.rs:2124`) that embed a query without persisting it. All persisting call sites MUST use `generate_provenanced()`.

### 1.3 Migration `200_embedding_provenance.sql`

Add columns to the 5 vector-bearing tables. Nullable initially; Phase 1c enforces NOT NULL after backfill.

```sql
-- migrations/200_embedding_provenance.sql

-- ─────────────────────────────────────────────────────────────
-- Per-row provenance columns (the "current vector" denormalisation)
-- ─────────────────────────────────────────────────────────────

ALTER TABLE episodes
    ADD COLUMN embedding_model_id      TEXT,
    ADD COLUMN embedding_model_version TEXT,
    ADD COLUMN embedding_dim           INTEGER,
    ADD COLUMN source_text             TEXT,
    ADD COLUMN source_ref              JSONB,
    ADD COLUMN provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE semantic_rules
    ADD COLUMN embedding_model_id      TEXT,
    ADD COLUMN embedding_model_version TEXT,
    ADD COLUMN embedding_dim           INTEGER,
    ADD COLUMN source_text             TEXT,
    ADD COLUMN source_ref              JSONB,
    ADD COLUMN provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE entities
    ADD COLUMN embedding_model_id      TEXT,
    ADD COLUMN embedding_model_version TEXT,
    ADD COLUMN embedding_dim           INTEGER,
    ADD COLUMN source_text             TEXT,
    ADD COLUMN source_ref              JSONB,
    ADD COLUMN provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE communities
    ADD COLUMN embedding_model_id      TEXT,
    ADD COLUMN embedding_model_version TEXT,
    ADD COLUMN embedding_dim           INTEGER,
    ADD COLUMN source_text             TEXT,
    ADD COLUMN source_ref              JSONB,
    ADD COLUMN provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE shopping_profiles
    ADD COLUMN embedding_model_id      TEXT,
    ADD COLUMN embedding_model_version TEXT,
    ADD COLUMN embedding_dim           INTEGER,
    ADD COLUMN source_text             TEXT,         -- always NULL for centroids
    ADD COLUMN source_ref              JSONB,        -- contains member_episode_ids
    ADD COLUMN provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

-- ─────────────────────────────────────────────────────────────
-- Append-only sidecar: full re-embed history per (target_table, target_id)
-- ─────────────────────────────────────────────────────────────

CREATE TABLE embedding_provenance (
    provenance_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_table     TEXT NOT NULL CHECK (target_table IN
                       ('episodes','semantic_rules','entities',
                        'communities','shopping_profiles')),
    target_id        UUID NOT NULL,
    agent_id         UUID,                       -- nullable for system-level seeds
    user_id          TEXT,
    source_text      TEXT,                       -- may be NULL for centroid rows
    source_ref       JSONB,
    model_id         TEXT NOT NULL,
    model_version    TEXT NOT NULL,
    dim              INTEGER NOT NULL,
    embedding        vector(1024),               -- the actual vector at this point
                                                  -- in history; supports Tier 2
                                                  -- translator anchor recovery
    trusted          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes            TEXT                         -- e.g. "backfill", "reembed_v2"
);

CREATE INDEX idx_provenance_target
    ON embedding_provenance (target_table, target_id, created_at DESC);

CREATE INDEX idx_provenance_model
    ON embedding_provenance (model_id, model_version, created_at);

CREATE INDEX idx_provenance_agent
    ON embedding_provenance (agent_id, created_at)
    WHERE agent_id IS NOT NULL;

-- Enforce append-only at the DB level. No UPDATE, no DELETE on this table
-- except via an explicit migration with a comment justifying it.
REVOKE UPDATE, DELETE ON embedding_provenance FROM PUBLIC;

-- ─────────────────────────────────────────────────────────────
-- vector(1024) is hardcoded in the schema above. If model_version changes
-- to a model with a different dim, the re-embed worker (Phase 3) handles
-- the schema migration. The dim column on each provenance row is the
-- per-write truth.
-- ─────────────────────────────────────────────────────────────
```

**pgvector version check before running:** confirm `pgvector >= 0.5.0` on the target Postgres (`SELECT extversion FROM pg_extension WHERE extname = 'vector'`). The schema assumes vector type is available; the existing `migrations/010` already loads it.

**Concurrent index build:** The `CREATE INDEX` statements should be run with `CONCURRENTLY` in production environments. Use a separate companion migration `200a_embedding_provenance_indexes.sql` containing the index creation with `CONCURRENTLY` for production deployment; the migration above is the dev/test variant.

### 1.4 Refactor the 14 generation call sites

Each call site builds the `source_text` *once*, passes it to `generate_provenanced`, and threads the returned `ProvenancedEmbedding` into the corresponding storing fn. Pattern:

```rust
// Before:
let embedding_text = format!("{} {}", body.query,
    output.metadata.reasoning.as_deref().unwrap_or(""));
let embedding = state.embedder.generate(&embedding_text).await.ok();
let episode = Episode { embedding, /* ... */ };
state.memory_store.store_episode(&episode).await?;

// After:
let embedding_text = format!("{} {}", body.query,
    output.metadata.reasoning.as_deref().unwrap_or(""));
let provenance = state.embedder
    .generate_provenanced(&embedding_text)
    .await
    .ok();   // None on transient embedder failure; episode is stored without vector
let source_ref = serde_json::json!({
    "kind": "execute_handler",
    "execution_id": body.execution_id,
});
let episode = Episode { embedding: None, /* ... */ };  // vector now in provenance
state.memory_store.store_episode_with_provenance(
    &episode, provenance.as_ref(), Some(source_ref)
).await?;
```

**The 14 call sites (file:line):**

Embedding writes (12):
1. `src/handlers/execution.rs:146`
2. `src/handlers/execution_stream.rs:211`
3. `src/handlers/eval.rs:546`
4. `src/handlers/rabble_workspace.rs:377`
5. `src/handlers/workspace/messages.rs:504`
6. `src/handlers/observations.rs:646`
7. `src/handlers/swarm_telemetry.rs:472`
8. `agent-bestiary/memory/src/consolidation.rs:367` (LLM rule)
9. `agent-bestiary/memory/src/consolidation.rs:419` (pattern rule)
10. `agent-bestiary/memory/src/consolidation.rs:462` (heuristic entity)
11. `agent-bestiary/memory/src/consolidation.rs:569` (LLM entity)
12. `agent-bestiary/memory/src/consolidation.rs:775` (dream rule)

Read-only embedding (2) — these MUST continue using the bare `generate()`, no source persistence; but **they must use the same `model_id`/`model_version` as the rows they compare against** or cosine similarity is meaningless:
13. `src/agent_backend/kg_context.rs:57` (query embedding for KG cosine)
14. `src/agent_backend/tools_legacy.rs:2124` (search_knowledge query)

For (13) and (14): add a debug assertion at startup that `state.embedder.model_id()` matches the `embedding_model_id` of a sample row from `episodes` / `semantic_rules` / `entities`. If they diverge, log a loud warning — query embeddings are being compared against vectors from a different model.

### 1.5 Update the 5 storing functions

`agent-bestiary/memory/src/store.rs`. Each fn grows a `Option<&ProvenancedEmbedding>` parameter and a `Option<serde_json::Value>` for `source_ref`. Both must be set in a single transaction with the row INSERT, AND with an `embedding_provenance` row INSERT.

```rust
impl MemoryStore {
    /// Stores an episode plus a provenance event in a single transaction.
    ///
    /// If `provenance` is None, the episode is stored with NULL embedding and
    /// NULL provenance columns. This is intentional for paths that defer
    /// embedding (TwoWriteMemory, composition rejections, simops planning).
    pub async fn store_episode_with_provenance(
        &self,
        episode: &Episode,
        provenance: Option<&ProvenancedEmbedding>,
        source_ref: Option<serde_json::Value>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let (model_id, model_version, dim, source_text, embedding) = match provenance {
            Some(p) => (
                Some(&p.model_id), Some(&p.model_version),
                Some(p.dim), Some(&p.source_text), Some(&p.vector),
            ),
            None => (None, None, None, None, None),
        };

        sqlx::query(r#"
            INSERT INTO episodes (
                episode_id, agent_id, /* ...existing cols... */,
                embedding,
                embedding_model_id, embedding_model_version, embedding_dim,
                source_text, source_ref, provenance_trusted
            ) VALUES ($1, $2, /* ... */, $N, $N+1, $N+2, $N+3, $N+4, $N+5, $N+6)
        "#)
            .bind(episode.episode_id)
            .bind(episode.agent_id)
            // ... existing binds ...
            .bind(embedding.map(|v| pgvector::Vector::from(v.clone())))
            .bind(model_id)
            .bind(model_version)
            .bind(dim)
            .bind(source_text)
            .bind(&source_ref)
            .bind(true)  // provenance_trusted - new writes are always trusted
            .execute(&mut *tx)
            .await?;

        if let Some(p) = provenance {
            sqlx::query(r#"
                INSERT INTO embedding_provenance (
                    target_table, target_id, agent_id, user_id,
                    source_text, source_ref,
                    model_id, model_version, dim, embedding,
                    trusted, notes
                ) VALUES (
                    'episodes', $1, $2, $3, $4, $5, $6, $7, $8, $9, true, 'initial_write'
                )
            "#)
                .bind(episode.episode_id)
                .bind(episode.agent_id)
                .bind(&episode.user_id)
                .bind(&p.source_text)
                .bind(&source_ref)
                .bind(&p.model_id)
                .bind(&p.model_version)
                .bind(p.dim)
                .bind(pgvector::Vector::from(p.vector.clone()))
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
```

Apply the same pattern to:
- `store_semantic_rule_with_provenance` (`store.rs:1253`) — **drive-by fix:** also bind `user_id` (previously never bound; the column exists but rows always have NULL).
- `store_entity_with_provenance` (`store.rs:1381`)
- `store_community_with_provenance` (`store.rs:1802`)
- `upsert_shopping_profile_with_provenance` (`store.rs:2433`) — `source_text` stays NULL; `source_ref` is `{"kind":"centroid","member_episode_ids":[...]}` derived from the constituent episodes.

**Keep the old fn signatures as thin wrappers that delegate with `provenance=None`, marked `#[deprecated(note = "use *_with_provenance")]`.** This lets the compiler flag every remaining un-provenanced write while the refactor lands incrementally.

### 1.6 `import_embeddings_handler` — client-supplied provenance

`src/handlers/agents.rs:956-1067`. The handler currently accepts client-supplied vectors with no model proof. The spec's frame is "consented, scoped, logged" — import is allowed but explicitly marked untrusted unless the client provides matching `model_id`/`model_version`.

Request body extension:

```rust
struct ImportEmbeddingItem {
    episode_id: Uuid,
    embedding: Vec<f32>,
    source_text: String,             // NEW: required
    source_ref: Option<serde_json::Value>,
    model_id: String,                // NEW: required
    model_version: String,           // NEW: required
    dim: i32,                        // NEW: required, must equal embedding.len()
}
```

Handler logic:

1. Reject if `dim != embedding.len()` (silent model swap guard).
2. Reject if `model_id` or `model_version` is empty.
3. Stamp `provenance_trusted = FALSE` on the row AND on the `embedding_provenance` event row. The client said what model produced it; we believe them but flag it.
4. Insert into `embedding_provenance` with `notes = "client_import"` and an explicit `source_ref` field `{"kind":"client_import","caller":<jwt-subject>}`.

This makes the import path safe-by-default for the re-embed worker: untrusted rows are eligible for re-embed at any time (the re-embed worker treats them as "needs verification") while trusted rows are only re-embedded on model migration.

### 1.7 Fix broken/lossy write paths

These all surfaced in the survey and are blocking for clean Phase 1 closure.

**1.7a. `agent-bestiary/coherence-gate/src/two_write.rs:68-145`**

`TwoWriteMemory::execute` currently writes an `Episode { embedding: None }` with a comment claiming the consolidation worker will backfill. The worker does not. Two fixes:

- Either embed eagerly in `execute` and stamp full provenance (preferred — adds one embedder call per two_write but matches the rest of the system).
- Or write NULL provenance explicitly (all 6 provenance columns NULL) AND ensure the consolidation worker actually backfills on next pass, embedding `source_text` reconstructed from `episode.query || ' ' || (episode.context->>'reasoning')`.

**Implementer chooses based on whether two_write is in the latency-critical path.** If it is, eager embedding adds one round-trip; if not, lazy is acceptable but the worker fix is mandatory.

**1.7b. `src/handlers/composition.rs:140-169` and `src/agent_backend/simops_tools.rs:1179-1205`**

Both write `Episode { embedding: None }` for synthetic episodes (rejections, plans). These episodes are never used for retrieval, so NULL embedding is fine. Fix: pass `provenance=None` explicitly and stamp `source_ref = {"kind":"composition_rejection"}` or `{"kind":"simops_actuation"}` so they're identifiable for later cleanup.

**1.7c. `src/workflows/fork.rs:140-189` — broken schema references**

Currently references non-existent table `rules` (line 152) and wrong column names on `episodes` (line 179: `id`, `content`, `role`, `source`). The fix is mechanical but the embedding-portability angle matters: when forking an agent, we either copy the existing embeddings forward (carrying their provenance forward — the new agent inherits "unknown_pre_provenance" rows if any) or we re-embed under the new agent's identity. Recommendation: copy forward with provenance preserved.

Replace the broken statements with:

```sql
-- entities
INSERT INTO entities (
    entity_id, agent_id, entity_name, entity_type, summary,
    t_valid, t_invalid, source_episodes, extraction_confidence,
    embedding, properties,
    embedding_model_id, embedding_model_version, embedding_dim,
    source_text, source_ref, provenance_trusted
)
SELECT gen_random_uuid(), $new_agent_id, entity_name, entity_type, summary,
    t_valid, t_invalid, source_episodes, extraction_confidence,
    embedding, properties,
    embedding_model_id, embedding_model_version, embedding_dim,
    source_text, source_ref, provenance_trusted
FROM entities WHERE agent_id = $source_agent_id AND t_invalid IS NULL;

-- semantic_rules (not "rules")
INSERT INTO semantic_rules (
    rule_id, agent_id, rule_content, /* ... */,
    embedding, embedding_model_id, embedding_model_version, embedding_dim,
    source_text, source_ref, provenance_trusted
)
SELECT gen_random_uuid(), $new_agent_id, rule_content, /* ... */,
    embedding, embedding_model_id, embedding_model_version, embedding_dim,
    source_text, source_ref, provenance_trusted
FROM semantic_rules WHERE agent_id = $source_agent_id AND is_active = true;

-- episodes (with correct column names)
INSERT INTO episodes (
    episode_id, agent_id, timestamp_ref, query, context,
    execution_status, embedding,
    embedding_model_id, embedding_model_version, embedding_dim,
    source_text, source_ref, provenance_trusted,
    created_at
)
SELECT gen_random_uuid(), $new_agent_id, timestamp_ref, query, context,
    execution_status, embedding,
    embedding_model_id, embedding_model_version, embedding_dim,
    source_text, source_ref, provenance_trusted,
    NOW()
FROM episodes WHERE agent_id = $source_agent_id;
```

Also INSERT into `embedding_provenance` a `notes = "forked_from:<source_agent_id>"` row per copied vector, so the fork is auditable.

**1.7d. `fermi-memory/src/store.rs:46-75, :151-185` — silent vector drop**

`fermi-memory` is the smaller legacy crate. `store_episode` and `store_semantic_rule` accept structs with `embedding: Option<Vec<f32>>` fields but never bind the `embedding` column in SQL. Two paths:

- **Preferred:** delete `fermi-memory` entirely if it's dead code (verify no callers in `Cargo.toml` of the workspace members).
- **Else:** bring it up to parity with `agent-bestiary/memory/src/store.rs` — bind embedding + all provenance columns.

Check first: `rg "fermi_memory" --type rust -l` and `rg 'fermi-memory =' Cargo.toml`. If the only consumer is a test or example, delete it. Document the decision in the PR.

**1.7e. `MemoryStore::store_semantic_rule` never binds `user_id`**

`store.rs:1253-1280`. The column exists, the field exists on `SemanticRule`, but the INSERT never includes it. Mechanical fix as part of the `_with_provenance` refactor.

### 1.8 `verify_reproducible` CI test

A standalone test binary that proves the provenance is sufficient to reproduce any stored vector. This is the spec's literal acceptance criterion.

`tests/embedding_reproducibility.rs`:

```rust
#[tokio::test]
#[ignore]  // run via `cargo test --test embedding_reproducibility -- --ignored`
async fn verify_reproducible_sample() {
    let pool = test_pool().await;
    let embedder = real_embedder_from_env();   // uses production embedder

    // Sample N=50 trusted rows from the last 24h, across all 5 tables
    let samples = sqlx::query!(r#"
        SELECT target_table, target_id, source_text,
               model_id, model_version, dim, embedding
        FROM embedding_provenance
        WHERE trusted = true
          AND source_text IS NOT NULL
          AND created_at > NOW() - INTERVAL '24 hours'
        ORDER BY random()
        LIMIT 50
    "#).fetch_all(&pool).await.unwrap();

    assert!(!samples.is_empty(), "no recent trusted samples to verify");

    let mut mismatches = Vec::new();
    for s in &samples {
        // Only verify rows matching the CURRENT production embedder
        if s.model_id != embedder.model_id() ||
           s.model_version != embedder.model_version() {
            continue;
        }

        let regenerated = embedder.generate(&s.source_text.as_ref().unwrap())
            .await.unwrap();
        let original: Vec<f32> = s.embedding.as_ref().unwrap().to_vec();
        let cos = cosine_similarity(&regenerated, &original);
        if cos < 0.9999 {
            mismatches.push((s.target_table.clone(), s.target_id, cos));
        }
    }

    assert!(mismatches.is_empty(),
        "provenance insufficient to reproduce {} vectors: {:?}",
        mismatches.len(), mismatches);
}
```

Wire into CI as a nightly job, not a PR gate (it calls the real embedder API). PR gate uses `MockEmbeddings` with deterministic hash output for fast verification.

### 1.9 Documentation

Add `docs/EMBEDDING_PROVENANCE.md`:
- The four required fields per vector and why each matters.
- The relationship between per-row columns and the `embedding_provenance` event table.
- The `provenance_trusted` flag and what it means (untrusted = client-imported or backfill-reconstructed; eligible for opportunistic re-embed).
- The `model_version` epoch policy (manual, bumped on observed drift).
- Pointer to this spec for the implementation details.

---

## Phase 1b — Backfill (one-shot, owner of the "untrusted" flag)

After Phase 1 ships and before Phase 1c enforces NOT NULL, run a one-shot backfill against existing rows.

`scripts/backfill_embedding_provenance.rs` (new binary, declared in `Cargo.toml`):

For each of the 5 tables, in agent_id-sized batches with a `--dry-run` flag:

```rust
// episodes: source_text reconstructed (lossy)
UPDATE episodes
SET embedding_model_id      = 'anthropic/voyage-2',
    embedding_model_version = 'unknown_pre_provenance',
    embedding_dim           = 1024,
    source_text             = query || ' ' || COALESCE(context->>'reasoning', ''),
    source_ref              = jsonb_build_object('kind','backfill','original_query',query),
    provenance_trusted      = false
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL
  AND agent_id = $1;

-- semantic_rules: source_text = rule_content (perfect)
UPDATE semantic_rules
SET embedding_model_id      = 'anthropic/voyage-2',
    embedding_model_version = 'unknown_pre_provenance',
    embedding_dim           = 1024,
    source_text             = rule_content,
    source_ref              = jsonb_build_object('kind','backfill'),
    provenance_trusted      = true   -- rule_content IS what was embedded
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL
  AND agent_id = $1;

-- entities: source_text = entity_name (perfect though semantically thin)
UPDATE entities
SET embedding_model_id      = 'anthropic/voyage-2',
    embedding_model_version = 'unknown_pre_provenance',
    embedding_dim           = 1024,
    source_text             = entity_name,
    source_ref              = jsonb_build_object('kind','backfill',
                                'source_episodes', source_episodes),
    provenance_trusted      = true
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL
  AND agent_id = $1;

-- communities: no clean source_text; mark untrusted
UPDATE communities
SET embedding_model_id      = 'anthropic/voyage-2',
    embedding_model_version = 'unknown_pre_provenance',
    embedding_dim           = 1024,
    source_text             = NULL,
    source_ref              = jsonb_build_object('kind','backfill',
                                'member_entity_ids', member_entity_ids),
    provenance_trusted      = false
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL
  AND agent_id = $1;

-- shopping_profiles: centroid, no source_text by design
UPDATE shopping_profiles
SET embedding_model_id      = 'anthropic/voyage-2',
    embedding_model_version = 'unknown_pre_provenance',
    embedding_dim           = 1024,
    source_text             = NULL,
    source_ref              = jsonb_build_object('kind','backfill','centroid',true),
    provenance_trusted      = false
WHERE composite_embedding IS NOT NULL
  AND embedding_model_id IS NULL
  AND agent_id = $1;
```

Each UPDATE also inserts a corresponding `embedding_provenance` row with `notes='backfill'` and `trusted=<same as row>`.

**The honesty discipline:** `episodes` are marked `trusted=false` because the source_text reconstruction is lossy (the original `format!()` string used `unwrap_or("")` for missing reasoning; we can't tell which rows had empty reasoning vs which had reasoning we can't find). `semantic_rules` and `entities` are trusted because their embedded text is recoverable verbatim. `communities` and `shopping_profiles` are centroids — there is no source text.

### 1b.1 `shopping_profiles.composite_embedding` schema-drift fix

While we're touching this table, also fix the gap in `scripts/migrate_embedding_dimensions.rs`: it currently does NOT include `shopping_profiles.composite_embedding` in its NULL pass or ALTER pass. Add it explicitly. This is unrelated to provenance but is bycatch surfaced by the survey.

---

## Phase 1c — Enforce NOT NULL (the discipline becomes a database invariant)

After Phase 1b completes for every agent in the database. Done as a follow-up migration `201_embedding_provenance_not_null.sql`:

```sql
-- For each table, add NOT VALID constraint then VALIDATE (no full-table lock).
-- The constraint says: if there is an embedding, there must be provenance.

ALTER TABLE episodes
    ADD CONSTRAINT episodes_embedding_has_provenance
    CHECK (embedding IS NULL OR (
        embedding_model_id IS NOT NULL
        AND embedding_model_version IS NOT NULL
        AND embedding_dim IS NOT NULL
    )) NOT VALID;
ALTER TABLE episodes VALIDATE CONSTRAINT episodes_embedding_has_provenance;

-- Same pattern for semantic_rules, entities, communities, shopping_profiles
-- (substituting composite_embedding for shopping_profiles).
```

After this lands, **any new code path that writes an embedding without provenance fails at the database level.** The spec's "no code path that skips it" rule is now structurally enforced, not policed.

Also tighten the embedding_provenance integrity:

```sql
ALTER TABLE embedding_provenance
    ALTER COLUMN model_id      SET NOT NULL,
    ALTER COLUMN model_version SET NOT NULL,
    ALTER COLUMN dim           SET NOT NULL;

-- Defensive: the dim recorded MUST equal vector length when both are present
-- (caught by application code, but a CHECK is cheap insurance):
ALTER TABLE embedding_provenance
    ADD CONSTRAINT provenance_dim_matches
    CHECK (embedding IS NULL OR vector_dims(embedding) = dim) NOT VALID;
ALTER TABLE embedding_provenance VALIDATE CONSTRAINT provenance_dim_matches;
```

---

## Phase 2 — Closed-model anchor set (cheap standing insurance)

Build immediately after Phase 1. Small footprint, no hot-path changes, closes the closed-model caveat the spec calls out.

### 2.0 Stand up `nomic-embed-text-v1.5` as the reference open model

Self-host via a small Rust wrapper or a Python sidecar exposing an HTTP endpoint compatible with the `EmbeddingGenerator` trait. Add `NomicEmbeddings` impl to `embeddings.rs` with `model_id = "nomic/embed-text-v1.5"`, `dim = 768`.

The reference model is NOT used in the production hot path. It is only used by the anchor refresh job (Phase 2.2) and the Tier 2 translator (Phase 6, if ever built).

### 2.1 Anchor set table

```sql
-- migrations/202_embedding_anchors.sql

CREATE TABLE embedding_anchors (
    anchor_id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    anchor_text          TEXT NOT NULL,
    anchor_set_version   INTEGER NOT NULL,  -- bump on set composition change
    -- Reference (open) model side
    reference_model_id      TEXT NOT NULL,
    reference_model_version TEXT NOT NULL,
    reference_embedding     vector(768) NOT NULL,
    reference_refreshed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Vendor model side (NULL until that vendor model is in use)
    vendor_model_id      TEXT,
    vendor_model_version TEXT,
    vendor_embedding     vector(1024),
    vendor_refreshed_at  TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_anchors_vendor_pair
    ON embedding_anchors (vendor_model_id, vendor_model_version)
    WHERE vendor_model_id IS NOT NULL;
```

Per the spec's recommendation, target 2000–5000 anchor texts. Compose them from:
- ~1000 diverse chunks sampled from our own production `episodes.source_text` (post-Phase-1, so we have it)
- ~500 chunks from `semantic_rules.rule_content`
- ~500 from `entities.entity_name` plus surrounding context
- ~500 from an external diverse corpus (e.g. C4 or Wikipedia samples) for domain breadth

Anchor set composition is one-time work; once seeded, it's frozen unless we bump `anchor_set_version`.

### 2.2 Anchor refresh cron

`scripts/refresh_embedding_anchors.rs` — runs nightly (or on demand). Logic:

1. Discover which `(model_id, model_version)` pairs currently appear in `embedding_provenance` with `trusted = true` (these are models we're actively writing).
2. For each `(vendor_model, version)` pair NOT yet anchored OR last refreshed > 7 days ago:
   - Embed every `anchor_text` on that vendor model.
   - INSERT/UPDATE the vendor side of `embedding_anchors`.
3. If the reference model's own version is bumped, re-embed all anchors on it (rare).

Cost ceiling: 5000 anchors × N vendor models × 7 days refresh = roughly N × 5000 API calls per week. At Voyage-2 prices this is <$1/week per vendor model. Standing insurance.

### 2.3 Detection of vendor model deprecation

A separate small periodic check: try to `.generate("ping")` on each vendor model in use. If it fails with a specific deprecation signature (vendor-specific HTTP code / error message), alert. The anchors are already in place; the Tier 2 translator can be fit post-hoc from them.

---

## Phase 3 — Tier 1 re-embed worker (deferred until first model switch)

**Trigger:** when we decide to change `model_id` or `model_version` on any production embedder.

### 3.1 The missing `re_embed_episodes` binary

`scripts/re_embed_episodes.rs` (declared in `Cargo.toml`). Actually a general re-embed binary; the name is kept for backwards-compat with the comment in `migrate_embedding_dimensions.rs:262`.

Args:
- `--target-model-id <id>` — new model_id to embed under
- `--target-model-version <version>` — new model_version
- `--from-model-id <id>` `--from-model-version <version>` — filter rows to re-embed
- `--table <episodes|semantic_rules|entities|communities|shopping_profiles|all>`
- `--agent-id <uuid>` (optional, owner-scoped batch)
- `--batch-size 100`
- `--dry-run`
- `--resume-from <provenance_id>` (idempotency)

Behavior:

```
for each batch:
    1. SELECT target_id, source_text, source_ref
       FROM <table>
       WHERE embedding_model_id = $from_id
         AND embedding_model_version = $from_version
         AND source_text IS NOT NULL
         AND provenance_trusted = true   -- untrusted rows handled separately
         AND (agent_id = $agent_id OR $agent_id IS NULL)
       ORDER BY target_id
       LIMIT batch_size
    2. For each row:
         new_vector = new_embedder.generate(&source_text).await?
    3. In a transaction:
         UPDATE <table> SET embedding=new_vector,
             embedding_model_id=$target_id,
             embedding_model_version=$target_version,
             embedding_dim=new_embedder.dim()
         WHERE target_id IN (...);
         INSERT INTO embedding_provenance (
             target_table, target_id, source_text, source_ref,
             model_id, model_version, dim, embedding, trusted, notes
         ) VALUES (..., 'reembed_from:' || $from_id || ':' || $from_version);
    4. Resumable checkpoint: write last processed target_id to a state file
```

**Untrusted rows are handled separately** — they go through a parallel path that re-embeds AND flips `provenance_trusted` to `true` only if the new vector cosine-matches a re-generation of the same source_text to within ε on the CURRENT model (i.e. we're verifying that the claimed source_text actually produces a sensible vector under our embedder; if it doesn't, the row stays untrusted and is flagged for review).

### 3.2 Index handling

The existing `migrate_embedding_dimensions.rs` drops and recreates the ivfflat indexes when `dim` changes. The re-embed worker reuses that logic for any dim change. For same-dim model changes (most common case — switching Voyage-2 to a hypothetical Voyage-2.1), indexes do not need to be dropped; the vectors are simply overwritten in place and the index is still valid post-update (ivfflat doesn't care about cluster validity after row updates — it's lossy by design).

For new HNSW indexes (Spec 21 introduced them), the re-embed worker should run with `SET hnsw.iterative_scan = strict_order` if precise neighbor agreement matters during the migration window.

### 3.3 Acceptance gate

After re-embed completes, run the Similarity Lab (Phase 5) `kNN overlap@10` test on a sample of 1000 rows. Require ≥ 0.90 to auto-accept. Below 0.90 flags for review — almost certainly indicates the source_text was reconstructed lossily, or the new model is genuinely incompatible.

---

## Phase 4 — Embedding cache (cheap win, build after Phase 1)

The provenance discipline makes the cache key well-defined for the first time: `(model_id, model_version, sha256(source_text))`.

### 4.1 Table

```sql
-- migrations/204_embedding_cache.sql

CREATE TABLE embedding_cache (
    cache_key        BYTEA PRIMARY KEY,    -- sha256(model_id || model_version || source_text)
    model_id         TEXT NOT NULL,
    model_version    TEXT NOT NULL,
    source_text      TEXT NOT NULL,        -- for debugging; can be removed if privacy-sensitive
    embedding        vector(1024) NOT NULL,
    dim              INTEGER NOT NULL,
    hit_count        BIGINT NOT NULL DEFAULT 1,
    last_hit_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_cache_model ON embedding_cache (model_id, model_version);
CREATE INDEX idx_cache_lru   ON embedding_cache (last_hit_at);
```

### 4.2 Cache-aside wrapper

`agent-bestiary/memory/src/embedding_cache.rs`:

```rust
pub struct CachedEmbedder<E: EmbeddingGenerator> {
    inner: E,
    pool: PgPool,
}

#[async_trait]
impl<E: EmbeddingGenerator> EmbeddingGenerator for CachedEmbedder<E> {
    fn model_id(&self) -> &str { self.inner.model_id() }
    fn model_version(&self) -> &str { self.inner.model_version() }
    fn dim(&self) -> i32 { self.inner.dim() }

    async fn generate(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let key = compute_cache_key(self.model_id(), self.model_version(), text);

        // Cache lookup
        if let Some(row) = sqlx::query!(
            "UPDATE embedding_cache
             SET hit_count = hit_count + 1, last_hit_at = NOW()
             WHERE cache_key = $1
             RETURNING embedding",
            &key[..]
        ).fetch_optional(&self.pool).await? {
            return Ok(row.embedding.to_vec());
        }

        // Miss: generate, cache, return
        let vector = self.inner.generate(text).await?;
        let _ = sqlx::query!(
            "INSERT INTO embedding_cache
             (cache_key, model_id, model_version, source_text, embedding, dim)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (cache_key) DO UPDATE
             SET hit_count = embedding_cache.hit_count + 1,
                 last_hit_at = NOW()",
            &key[..], self.model_id(), self.model_version(),
            text, pgvector::Vector::from(vector.clone()), self.dim()
        ).execute(&self.pool).await;  // best-effort: cache write failure must not fail the call

        Ok(vector)
    }
}
```

### 4.3 LRU eviction

Background job, runs hourly: `DELETE FROM embedding_cache WHERE last_hit_at < NOW() - INTERVAL '30 days' AND hit_count < 5`. Tuning constants live in config; defaults conservative.

### 4.4 Wire in at runtime

`src/api_server.rs:722-730` wraps the constructed embedder:

```rust
let raw_embedder: Arc<dyn EmbeddingGenerator> = /* existing logic */;
let embedder: Arc<dyn EmbeddingGenerator> = Arc::new(CachedEmbedder {
    inner: raw_embedder,
    pool: pool.clone(),
});
```

Query-only paths (`kg_context.rs`, `tools_legacy.rs`) automatically benefit. Consolidation paths also benefit — rule_content strings often recur across consolidations.

---

## Phase 5 — Similarity Lab (build when first re-embed runs)

The measurement instrument. Per the spec, this is NOT a translator; it only measures.

### 5.1 Metrics

`agent-bestiary/similarity-lab/src/lib.rs` (new crate):

- `knn_overlap_at_k(old_space, new_space, k) -> f32` — for each item, top-k neighbors in old vs new; report mean fraction preserved.
- `rank_correlation(old_pairs, new_pairs) -> f32` — Spearman on a held-out probe set.
- `cluster_retention(old_clusters, new_clusters) -> f32` — ARI / community membership preservation.

### 5.2 Reporting

Per-pair, never averaged. Output format:

```json
{
  "from_model": "anthropic/voyage-2 @ 2024-01-01",
  "to_model":   "anthropic/voyage-3 @ 2026-06-01",
  "sample_size": 1000,
  "knn_overlap_at_10": 0.94,
  "rank_correlation_spearman": 0.91,
  "cluster_retention_ari": 0.87,
  "tolerance_met": true,
  "tolerance_policy": "knn@10 >= 0.90 auto-accept"
}
```

### 5.3 CI gate for Phase 3

The re-embed worker's acceptance test calls into Similarity Lab. If `knn_overlap_at_10 < 0.90`, the migration halts and a human reviews.

---

## Phase 6 — Tier 2 translator (deferred hedge; build only if needed)

**Trigger:** Tier 1 too slow at scale (corpus too large to re-embed within deprecation window) OR a closed vendor goes dark with un-re-embedded vectors AND the anchor set is available.

### 6.1 Algorithm ladder (per spec)

1. **Procrustes (linear/orthogonal map):** fit on `embedding_anchors` pairs. Closed-form, fast, surprisingly effective for same-family models.
2. **Linear with allowed scaling/shear:** if Procrustes underfits.
3. **Small MLP:** only if linear underfits measurably. Fit on anchor pairs; trained on a tiny set so it can't overfit the corpus.

**Explicitly NOT in scope:** unpaired adversarial translation (vec2vec). The spec is clear — that solves the adversary's problem, not ours.

### 6.2 Application

Translated vectors are written to a separate column (`embedding_translated vector(1024)`) with provenance `notes='tier2_translated:procrustes:from:<vendor>:<version>'` and `trusted=false`. The original orphaned vector is preserved alongside (in the `embedding_provenance` event row). Queries fall back to translated only when the original model can't be called for the query side.

---

## Cross-cutting concerns

### Append-only discipline

`embedding_provenance` is the system-of-record for "what was true at the time this vector was written." Application code MUST NOT UPDATE or DELETE rows in this table. The `REVOKE` in §1.3 enforces this at the database level for the default app role. Migrations that need to touch it require a separate role and a justifying comment.

### Security note (the Magna Carta collision the spec calls out)

`embedding_provenance.embedding` is a vector. Anyone with access to it can run an inversion attack to recover much of `source_text` even without our help. Therefore:

- `embedding_provenance` is **agent-scoped read access** (RLS or app-layer): an agent can only see its own provenance.
- The export endpoint (`/api/agents/:id/export`) defaults to source + structure (rungs 1–2 of "what's owned" per the spec). Raw-vector export is a separate consented action that returns a warning in the response envelope: `"warning": "Exported embeddings are invertible — anyone holding them can recover substantial source content."`.
- Audit: every raw-vector export INSERTs a row into a new `export_log` table.

This is out of scope for the implementation crate but in scope for the spec; the API handlers need updating in a follow-up PR.

### Schema drift forward-protection

After Phase 1c lands, write a sqlx compile-time test (`tests/embedding_schema_invariants.rs`) that asserts every vector-bearing table has all 6 provenance columns. A future migration that adds a vector column without provenance fails compilation. This is the structural enforcement of the discipline.

---

## Implementation order summary

1. **Phase 0** (instrumentation + decision verification) — 0.5 day
2. **Phase 1** (provenance plumbing + 14 call sites + 5 storing fns + drive-by fixes) — 3–5 days
3. **Phase 1b** (backfill) — 1 day (script + run)
4. **Phase 1c** (NOT NULL enforcement) — 0.5 day
5. **Phase 2** (anchor set + reference model) — 2 days
6. **Phase 3** (re-embed worker) — deferred (~3 days when triggered)
7. **Phase 4** (embedding cache) — 1 day, any time after Phase 1
8. **Phase 5** (Similarity Lab) — deferred (~2 days when triggered)
9. **Phase 6** (translator) — deferred indefinitely; only when forced

Phases 1, 1b, 1c are sequential and mandatory. Phase 2 can begin immediately after 1c. Phase 4 can begin immediately after 1. Phases 3, 5, 6 are triggered, not scheduled.

---

## Risk summary

| Phase | Risk | Main gotcha |
|---|---|---|
| 0 | None | Verify production embedder is Anthropic, not Mock |
| 1.1 (trait) | Low | Remove `provider_name`, update 5 impls; compiler-enforced |
| 1.2 (`ProvenancedEmbedding`) | Low | New type; nothing breaks |
| 1.3 (migration) | Low | Use `CONCURRENTLY` for indexes in prod |
| 1.4 (call sites) | **Medium** | 14 sites change; source_text MUST be built once, passed to embedder + storage from the same variable, never reconstructed; type system catches misses |
| 1.5 (storing fns) | **Medium** | Transaction discipline — row + provenance event in one tx; the `_with_provenance` wrapper pattern allows incremental migration with `#[deprecated]` shims |
| 1.6 (import handler) | Low | Breaking API change: new required fields on import; client must update |
| 1.7a–e (drive-bys) | Low | Fork.rs schema mismatch is a latent bug already; fermi-memory is likely dead code — verify before deleting |
| 1.8 (CI test) | Low | Nightly job, real API; PR gate uses mock |
| 1b (backfill) | **Medium** | `episodes.source_text` reconstruction is lossy → stamp `provenance_trusted=false`; honest about it |
| 1c (NOT NULL) | Low | NOT VALID then VALIDATE; no full-table lock |
| 2 (anchors) | Low | New tables, isolated; reference model self-hosting is the only operational cost |
| 3 (re-embed) | Medium-High | Deferred. Worker is mechanical but acceptance-gate (Phase 5) must be in place first when triggered |
| 4 (cache) | Low | Cache miss must never fail the embed call — best-effort write |
| 5 (Similarity Lab) | Low | Pure measurement; per-pair reporting (never averaged) is the only non-obvious requirement |
| 6 (translator) | Medium | Deferred. Procrustes-first, MLP only if needed; vec2vec explicitly prohibited |

**The dominant risk is not implementing Phase 1.** Every embedding written before this lands is permanently marked `provenance_trusted=false` and cannot be losslessly re-embedded. The longer Phase 1 takes, the larger that set.

---

## Pre-implementation checklist

Before opening the first PR:

1. [ ] Verify production embedder is real (not mock): `grep "Using mock" logs/api_server.log` should return nothing.
2. [ ] Run §0.3 row counts and record in `docs/EMBEDDING_PROVENANCE_BACKFILL_NOTES.md`.
3. [ ] Confirm pgvector version on prod Postgres ≥ 0.5.0: `SELECT extversion FROM pg_extension WHERE extname='vector';`.
4. [ ] Verify `fermi-memory` dead/alive: `rg "fermi_memory" --type rust -l` and `rg 'fermi-memory =' Cargo.toml`. Decide delete vs fix in §1.7d.
5. [ ] Confirm `nomic-embed-text-v1.5` deployment plan for Phase 2.0 (self-hosted endpoint, Python or Rust wrapper, target host).
6. [ ] Confirm with API consumers that the `import_embeddings_handler` breaking change (§1.6) is communicable.
7. [ ] Reserve a maintenance window for the migration if `episodes` table is large enough that the column ADD takes > 30s (rare for ALTER ADD COLUMN of nullable columns, but verify).

When all 7 boxes are checked, begin with §1.1.

---

## Open questions (for the implementer to resolve in Phase 0)

1. Is the production embedder currently the Anthropic Voyage-2 path, or is it accidentally on Mock in any environment? If Mock anywhere, Phase 1 must NOT touch that environment until a real key is provided — Mock vectors are deterministic noise and shouldn't be backfilled as if they were real.
2. Is `fermi-memory` crate actually used by any binary in the workspace? If yes, fix per §1.7d. If no, delete it.
3. How many `episodes`, `semantic_rules`, `entities` rows exist in production? (Drives backfill batch sizing.)
4. Is there a per-tenant embedder override anywhere I missed? The audit said no, but double-check `agent_id`-keyed configuration.
5. The Nomic reference-model deployment target — same Postgres host? Separate sidecar? Document the topology before Phase 2.

Document answers in `docs/EMBEDDING_PROVENANCE_BACKFILL_NOTES.md` alongside the row counts.
