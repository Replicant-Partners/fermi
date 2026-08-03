# v0.10.26 — Fix the embedder: OpenAI @ 1024, unbreak dreaming & search

## Why

Loop 1 (individual-agent dreaming/consolidation) had been **dead
platform-wide for ~6 weeks** — last successful consolidation anywhere
was 2026-06-22, and 76/705 agents had ever consolidated. Investigation
traced it to the embedding layer, not the consolidation logic.

Since the Spec-22 embedding-portability work, the API server
constructed `AnthropicEmbeddings`, whose `generate_batch` POSTs to:

```
https://api.anthropic.com/v1/embeddings
```

**Anthropic does not serve an embeddings endpoint** (live probe: `404`;
Voyage — the intended `voyage-2` model — lives at `api.voyageai.com`
and we never had a key for it). So every embedding call failed. Worse,
`reqwest::Client::new()` has no timeout, so consolidation jobs
triggered via `POST /api/agents/:id/consolidate` **hung** at
`episodes_processed=0` rather than erroring — 50 jobs wedged in
`running` with orphaned locks, and the failure was invisible.

Impact was platform-wide, not just dreaming: **2,737 of 3,089 episodes
(89%) are unembedded**, and the 4 football factor agents have **0**
embedded episodes. Anything needing a fresh embedding (ingestion,
semantic search, KG retrieval, clustering) was silently broken.

## Change

Smallest change that closes the loop.

### 1. `src/api_server.rs` — select a working embedder

```rust
if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
    Arc::new(OpenAIEmbeddings::new(api_key))   // text-embedding-3-large @ 1024
} else {
    eprintln!("⚠ NO OPENAI_API_KEY … MOCK embeddings …"); // loud, not silent
    Arc::new(MockEmbeddings::new(1024))
}
```

- `OpenAIEmbeddings` was already correct — right endpoint, `Bearer`
  auth, and it sends `dimensions: 1024`. It simply was never selected.
- **1024 dims matches the existing pgvector column + HNSW indices → no
  schema migration, no re-embed required to close the loop.**
- The mock fallback now warns on stderr. A *silent* fallback is exactly
  how this outage hid for six weeks.
- `OPENAI_API_KEY` is set in Railway.

### 2. `agent-bestiary/memory/src/embeddings.rs` — timeout

`OpenAIEmbeddings`'s client gets a 30s timeout. The lesson from this
incident: an unbounded client turns a dead endpoint into a silent hang
instead of a loud failure.

### 3. `migrations/170_backfill_agents_used_agent_id.sql` — registered

Backfills `agent_id` into `fermi_forecasts.agents_used` so resolved
forecasts' brier scores attribute to agents (Loop 5 join is on
`agent_id`; the data was keyed by `name` only). Already applied to prod
out-of-band on 2026-08-03; idempotent; now in the boot sequence so
fresh environments get it.

## Why no re-embed pass is needed (yet)

Consolidation embeds episodes on-demand via the shared embedder. So the
first `consolidate` call after this deploys will embed each agent's
episodes, cluster them, extract rules, and write a snapshot. A bulk
re-embed of the 2,737 backlog episodes (for semantic search coverage)
is a good follow-up but is **not** required to close Loop 1.

## Post-deploy verification

```bash
# 1. Trigger a dream cycle on an agent with unconsolidated episodes.
curl -s -X POST -H "Authorization: Bearer $IVAN_TOKEN" \
  "https://agent-bestiary.world/api/agents/macro_data_agent/consolidate"

# 2. Confirm it actually closes (poll endpoint is unreliable — see
#    v0.10.27 job-id fix — so check state directly):
psql -c "SELECT last_consolidated_at,
                (SELECT COUNT(*) FROM episodes e
                  WHERE e.agent_id=a.agent_id AND e.consolidated=false) AS unconsolidated
         FROM agents a WHERE agent_name='macro_data_agent';"
# → last_consolidated_at recent, unconsolidated dropped.

psql -c "SELECT version, dream_synopsis IS NOT NULL AS has_synopsis
         FROM ontology_snapshots
         WHERE agent_id=(SELECT agent_id FROM agents WHERE agent_name='macro_data_agent')
         ORDER BY version DESC LIMIT 1;"
# → a fresh snapshot with a dream synopsis.
```

## Deferred to v0.10.27 (verify-first, then polish)

- **Consolidation job-id unification** — the handler returns/locks under
  a `job_id` that never matches the worker's persisted row, so
  `GET …/consolidation/jobs/:id` always 404s and errors are recorded on
  the wrong row (making failures look like eternal `running`).
- **Manual-resolve → learning bridge** — `record_forecast_calibration_signals`
  is only called from the Polymarket oracle path; the console "Resolve"
  modal never feeds Loop 1.
- **Observatory honesty** — Loop-5 panel shows a placeholder `%` at
  `n=0`; should show "cold / no data".
- **`agents_used` gets `agent_id` at write time** so mig-170 never needs
  re-running.
- **Owner/RBAC check** on `consolidate_agent_handler`.

## Related

- Spec 22 — Embedding Portability (2026-06-11), where the embedder was
  left pointing at a non-existent Anthropic endpoint.
- v0.10.19 — REAL/f64 resolve fix (the FLOAT4/FLOAT8 error that first
  surfaced this thread).
- The design intent stands: an OpenAI-compatible `/v1/embeddings` wire
  format keeps providers swappable; this release just points it at a
  provider we actually have a key for.
