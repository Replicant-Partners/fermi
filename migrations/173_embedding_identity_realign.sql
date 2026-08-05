-- Migration 173: realign per-agent embedding identity to the active embedder.
--
-- The platform's live embedder is OpenAIEmbeddings (openai/text-embedding-3-large
-- @ 1024) since the embedder fix. But agents were seeded/created with the stale
-- `anthropic`/`voyage-2` identity (mig-019 defaults + seed/import/workspace
-- hardcodes). That identity never matched any real vector — Anthropic serves no
-- embeddings API — and it mislabelled the correct OpenAI vectors in the
-- embedding-portability view (agents.embedding_model drives `embedding_intent`).
--
-- This is a METADATA realignment only. Both voyage-2 and text-embedding-3-large
-- are 1024-dim, and per-vector provenance (embedding_model_id/version on
-- episodes) is untouched and authoritative — so NO re-embedding is required.
-- Only agents still carrying the stale default are flipped; agents a user set
-- to a genuinely different model are left alone.
--
-- Single DO block => one statement => PgBouncer-safe + idempotent.

DO $$
BEGIN
    UPDATE agents
       SET embedding_provider = 'openai',
           embedding_model    = 'text-embedding-3-large'
     WHERE embedding_provider = 'anthropic'
       AND embedding_model    = 'voyage-2';

    -- Stop mig-019's defaults from reintroducing the stale identity on raw INSERT.
    ALTER TABLE public.agents ALTER COLUMN embedding_provider SET DEFAULT 'openai';
    ALTER TABLE public.agents ALTER COLUMN embedding_model    SET DEFAULT 'text-embedding-3-large';
END $$;
