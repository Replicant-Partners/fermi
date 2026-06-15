# Embedding Provenance — Pre-Implementation Notes

Companion to `docs/specs/22_EMBEDDING_PORTABILITY_SPEC.md`. Records Phase 0 verification results and answers to the spec's open questions.

---

## Phase 0.1 — Production embedder wiring

Verified at `src/api_server.rs:722-730`:

```rust
let embedder: Arc<dyn EmbeddingGenerator> =
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        println!("Using Anthropic embeddings (voyage-2)");
        Arc::new(AnthropicEmbeddings::new(api_key))
    } else {
        println!("No ANTHROPIC_API_KEY, using mock embeddings");
        Arc::new(MockEmbeddings::new(1024))
    };
```

- **Production identity (assumed):** `anthropic/voyage-2`, dim 1024.
- **Development / no-key environments:** `mock` with dim 1024.
- **Risk noted:** any environment running on `MockEmbeddings` must NOT be backfilled as if it were producing real Voyage vectors. The backfill script (Phase 1b) must check the runtime embedder identity before tagging existing rows; on a Mock environment it should stamp `model_id="mock/deterministic-hash"` instead.

## Phase 0.2 — Manual epoch versions

Locked epoch dates for the four real providers, recorded in `agent-bestiary/memory/src/embeddings.rs`:

| Provider | Constant | Initial value | Bump policy |
|---|---|---|---|
| Voyage | `VOYAGE_MODEL_VERSION` | `"2024-01-01"` | Bump on observed embedding drift or vendor announcement |
| OpenAI | `OPENAI_EMBED_VERSION` | `"2024-01-01"` | Same |
| Mistral | `MISTRAL_EMBED_VERSION` | `"2024-01-01"` | Same |
| Qwen | `QWEN_EMBED_VERSION` | `"2024-01-01"` | Same |

A version bump in code is the trigger for the re-embed worker (Phase 3) to refresh existing vectors stamped with the old version.

## Phase 0.3 — Row counts (TODO once run against prod DB)

Run the following against production (with read-only credentials) and paste the output here before starting Phase 1b:

```sql
SELECT 'episodes' tbl, COUNT(*) total, COUNT(embedding) with_embedding
FROM episodes
UNION ALL SELECT 'semantic_rules', COUNT(*), COUNT(embedding) FROM semantic_rules
UNION ALL SELECT 'entities', COUNT(*), COUNT(embedding) FROM entities
UNION ALL SELECT 'communities', COUNT(*), COUNT(embedding) FROM communities
UNION ALL SELECT 'shopping_profiles', COUNT(*), COUNT(composite_embedding) FROM shopping_profiles;
```

Baseline captured 2026-06-14 against the Neon prod DB (solo-dev environment):

| Table | Total rows | Rows with embedding | Backfill batch sizing |
|---|---|---|---|
| episodes | 2274 | 352 | trivial — single batch |
| semantic_rules | 51 | 25 | trivial |
| entities | 902 | 750 | trivial |
| communities | 0 | 0 | n/a |
| shopping_profiles | 0 | 0 | n/a |

Total embedded rows to backfill: **1127**. Fits in one default-size batch (500) × 3 tables. Expected runtime: < 30 s.

## Pre-implementation answers

1. **Production embedder is Anthropic Voyage-2 (or Mock if no key).** No tenant-specific override discovered in the survey.
2. **`fermi-memory` crate is dead code.** Listed in `Cargo.toml` workspace members (line 21) but zero consumers across the entire workspace (verified: `rg "fermi_memory|fermi.memory"` returns no matches). **Decision: delete it** in Phase 1.7d.
3. **Row counts:** to be filled in before Phase 1b.
4. **No per-tenant embedder override exists.** All call sites use the shared `state.embedder` constructed at startup.
5. **Reference open model topology (Phase 2.0):** TBD — deferred to Phase 2 planning. Likely a Python sidecar exposing an OpenAI-compatible HTTP endpoint on the same host as the Postgres pool, hidden behind the `EmbeddingGenerator` trait via a `NomicEmbeddings` impl that does HTTP calls.

## pgvector version (TODO)

Run against prod Postgres:

```sql
SELECT extversion FROM pg_extension WHERE extname = 'vector';
```

Expected: `≥ 0.5.0` for HNSW support. If lower, the anchor table (Phase 2) must use IVFFlat or skip ANN indexing.

Result: **0.8.0** (Neon, captured 2026-06-14). HNSW supported; constraints in migration 136 are compatible.

---

## Migration numbering

The spec referenced migrations `200/201/...` for clarity but the actual sequence in this repo is sequential. Migrations landed:

- ✅ `135_embedding_provenance.sql` — Phase 1.3 (per-row columns + sidecar table)
- ✅ `136_embedding_provenance_not_null.sql` — Phase 1c (constraint enforcement, run after backfill)
- ✅ `137_embedding_anchors.sql` — Phase 2.1 (anchor set table)
- ⏳ `138_embedding_cache.sql` — Phase 4.1 (cache table) — deferred

## Phase 2.0 — Nomic deployment topology

Recorded decision (2026-06-12): Nomic reference model deployed via **Ollama** on the same host as the API server.

```bash
# Host setup, one-time:
ollama pull nomic-embed-text
systemctl enable ollama       # or run `ollama serve` under your supervisor

# Verify:
curl http://localhost:11434/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"model": "nomic-embed-text", "input": ["hello"]}'
```

`NomicEmbeddings::from_env()` reads `NOMIC_BASE_URL` (default `http://localhost:11434/v1/embeddings`) and `NOMIC_API_KEY` (optional, unused with Ollama).

For multi-host deployments where the API server is separate from the host running Ollama, set `NOMIC_BASE_URL` to the appropriate internal address. The reference model is NOT used in the hot path; it's only called by `scripts/embedding_anchors.rs` (seed + refresh) and, later, the Tier 2 translator.

Alternative deployments (Python sidecar, vLLM, llama.cpp) work identically — the client speaks the OpenAI embeddings API.
