# v0.10.26 — fix the embedder: OpenAI @ 1024, unbreak dreaming/search

## Why

Root cause of a 6-week platform-wide outage of Loop 1 (agent
dreaming/consolidation) and all runtime embedding.

Since the Spec-22 embedding-portability work, the server built
`AnthropicEmbeddings`, which POSTs to
`https://api.anthropic.com/v1/embeddings` — **an endpoint
Anthropic does not serve** (live probe: 404). Every embedding call
errored or hung; consolidation jobs wedged at
`episodes_processed = 0`. The reqwest client had no timeout, so a
dead endpoint stalled instead of failing loud.

Symptom in production data at the moment of this release:

- **89% of episodes unembedded** (2737 / 3089).
- The 4 football factor agents (`macro_data_agent`,
  `football_institution_agent`, `fixture_context_agent`, plus
  `football_analyst`) have **0 embedded episodes**.
- `search_knowledge` returns empty because there's nothing to
  vector-search.
- Every agent's Loop 1 diagnostic reads "consolidation stuck at 0
  episodes processed".

Silent because there was no timeout and no loud fallback — the
mock embedder just returned deterministic zero-vectors when the
"real" client errored out.

## Change

Smallest fix that closes the loop.

### `src/api_server.rs`

- Embedder now built from `OPENAI_API_KEY` → `OpenAIEmbeddings`
  (model `text-embedding-3-large`, `dimensions=1024`).
- **1024 matches the existing pgvector column + HNSW indices**, so
  no schema migration required. All existing dimension-1024 rows
  stay valid; new rows land at the same dimension.
- Mock fallback now warns loudly (`eprintln!`) when it's engaged.
  A silent fallback is how this hid for 6 weeks.
- Register `migrations/170_backfill_agents_used_agent_id.sql` in
  `run_migrations()` (was in-flight, now wired).

### `agent-bestiary/memory/src/embeddings.rs`

- `OpenAIEmbeddings` client gets a 30-second timeout so a bad
  endpoint fails fast/visibly instead of wedging a consolidation
  job forever.

## Post-deploy verification

```bash
# Search returns real hits.
curl -s -H "Authorization: Bearer $TOKEN" \
     "https://agent-bestiary.world/api/agents/<any>/knowledge/search?q=probability"
# → non-empty vector matches (was: empty)

# Consolidation processes episodes instead of wedging.
psql -c "SELECT status, episodes_processed, duration_ms
         FROM consolidation_jobs
         WHERE started_at > NOW() - INTERVAL '1 hour'
         ORDER BY started_at DESC LIMIT 5;"
# → status='completed', episodes_processed > 0

# Backfill: schedule a re-embed sweep of the 2737 unembedded rows
# (separate job, not this release).
```

## Related

- Spec 22 (embedding portability) — the substrate that introduced
  the pluggable `EmbeddingGenerator` trait. Correct architectural
  layer; wrong default at boot.
- v0.10.27 — `agents.updated_at` fix via `ensure_critical_schema`
  (mig-166 was eaten by PgBouncer). Landed in the same push
  window; documented separately.
