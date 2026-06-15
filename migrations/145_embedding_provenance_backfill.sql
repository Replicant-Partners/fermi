-- 145: Backfill embedding provenance for pre-Spec-22 rows.
--
-- Pure-SQL equivalent of scripts/backfill_embedding_provenance.rs, wrapped
-- in a single DO block so it's PgBouncer-safe (no explicit BEGIN/COMMIT,
-- single round-trip from the application's perspective).
--
-- For each of the five vector-bearing tables:
--   1. Populate the per-row provenance columns introduced by migration 135.
--   2. Insert a corresponding row into the append-only embedding_provenance
--      sidecar event log so the audit trail matches the binary's behaviour.
--
-- Trust discipline (matches the Rust binary):
--   episodes:           trusted = false (lossy source_text reconstruction)
--   semantic_rules:     trusted = true  (rule_content IS what was embedded)
--   entities:           trusted = true  (entity_name IS what was embedded)
--   communities:        trusted = false (centroid; no source text)
--   shopping_profiles:  trusted = false (centroid; no source text)
--
-- Idempotent: every UPDATE is gated on `embedding_model_id IS NULL` so
-- subsequent boots are no-ops. The INSERT into embedding_provenance uses
-- `WHERE NOT EXISTS` to dedupe.
--
-- See docs/EMBEDDING_PROVENANCE.md and Spec 22 §1b for context.

DO $$
DECLARE
    v_model_id      CONSTANT TEXT    := 'anthropic/voyage-2';
    v_model_version CONSTANT TEXT    := 'unknown_pre_provenance';
    v_dim           CONSTANT INTEGER := 1024;
BEGIN

-- ─── episodes ─────────────────────────────────────────────────────
-- Reconstruct source_text from query + (context->>'reasoning'). This is
-- lossy when the original reasoning was empty vs missing, so the row is
-- marked untrusted.
UPDATE episodes
SET embedding_model_id      = v_model_id,
    embedding_model_version = v_model_version,
    embedding_dim           = v_dim,
    source_text             = (query || ' ' || COALESCE(context->>'reasoning', '')),
    source_ref              = jsonb_build_object('kind', 'backfill',
                                                 'original_query', query),
    provenance_trusted      = FALSE
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL;

INSERT INTO embedding_provenance (
    target_table, target_id, agent_id, user_id,
    source_text, source_ref,
    model_id, model_version, dim, embedding,
    trusted, notes
)
SELECT 'episodes', e.episode_id, e.agent_id, e.user_id,
       e.source_text, e.source_ref,
       v_model_id, v_model_version, v_dim, e.embedding,
       FALSE, 'backfill'
FROM episodes e
WHERE e.embedding IS NOT NULL
  AND e.embedding_model_id = v_model_id
  AND e.embedding_model_version = v_model_version
  AND NOT EXISTS (
      SELECT 1 FROM embedding_provenance ep
      WHERE ep.target_table = 'episodes'
        AND ep.target_id    = e.episode_id
        AND ep.notes        = 'backfill'
  );

-- ─── semantic_rules ───────────────────────────────────────────────
-- rule_content IS what was embedded. Trusted.
UPDATE semantic_rules
SET embedding_model_id      = v_model_id,
    embedding_model_version = v_model_version,
    embedding_dim           = v_dim,
    source_text             = rule_content,
    source_ref              = jsonb_build_object('kind', 'backfill'),
    provenance_trusted      = TRUE
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL;

INSERT INTO embedding_provenance (
    target_table, target_id, agent_id, user_id,
    source_text, source_ref,
    model_id, model_version, dim, embedding,
    trusted, notes
)
SELECT 'semantic_rules', r.rule_id, r.agent_id, r.user_id,
       r.source_text, r.source_ref,
       v_model_id, v_model_version, v_dim, r.embedding,
       TRUE, 'backfill'
FROM semantic_rules r
WHERE r.embedding IS NOT NULL
  AND r.embedding_model_id = v_model_id
  AND r.embedding_model_version = v_model_version
  AND NOT EXISTS (
      SELECT 1 FROM embedding_provenance ep
      WHERE ep.target_table = 'semantic_rules'
        AND ep.target_id    = r.rule_id
        AND ep.notes        = 'backfill'
  );

-- ─── entities ─────────────────────────────────────────────────────
-- entity_name IS what was embedded. Trusted.
UPDATE entities
SET embedding_model_id      = v_model_id,
    embedding_model_version = v_model_version,
    embedding_dim           = v_dim,
    source_text             = entity_name,
    source_ref              = jsonb_build_object('kind', 'backfill',
                                                 'source_episodes', source_episodes),
    provenance_trusted      = TRUE
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL;

INSERT INTO embedding_provenance (
    target_table, target_id, agent_id, user_id,
    source_text, source_ref,
    model_id, model_version, dim, embedding,
    trusted, notes
)
SELECT 'entities', ent.entity_id, ent.agent_id, NULL::text,
       ent.source_text, ent.source_ref,
       v_model_id, v_model_version, v_dim, ent.embedding,
       TRUE, 'backfill'
FROM entities ent
WHERE ent.embedding IS NOT NULL
  AND ent.embedding_model_id = v_model_id
  AND ent.embedding_model_version = v_model_version
  AND NOT EXISTS (
      SELECT 1 FROM embedding_provenance ep
      WHERE ep.target_table = 'entities'
        AND ep.target_id    = ent.entity_id
        AND ep.notes        = 'backfill'
  );

-- ─── communities ──────────────────────────────────────────────────
-- Centroid embedding. No reproducible source_text; mark untrusted.
UPDATE communities
SET embedding_model_id      = v_model_id,
    embedding_model_version = v_model_version,
    embedding_dim           = v_dim,
    source_text             = NULL,
    source_ref              = jsonb_build_object('kind', 'backfill',
                                                 'member_entity_ids', member_entity_ids,
                                                 'centroid', TRUE),
    provenance_trusted      = FALSE
WHERE embedding IS NOT NULL
  AND embedding_model_id IS NULL;

INSERT INTO embedding_provenance (
    target_table, target_id, agent_id, user_id,
    source_text, source_ref,
    model_id, model_version, dim, embedding,
    trusted, notes
)
SELECT 'communities', c.community_id, c.agent_id, NULL::text,
       NULL::text, c.source_ref,
       v_model_id, v_model_version, v_dim, c.embedding,
       FALSE, 'backfill'
FROM communities c
WHERE c.embedding IS NOT NULL
  AND c.embedding_model_id = v_model_id
  AND c.embedding_model_version = v_model_version
  AND NOT EXISTS (
      SELECT 1 FROM embedding_provenance ep
      WHERE ep.target_table = 'communities'
        AND ep.target_id    = c.community_id
        AND ep.notes        = 'backfill'
  );

-- ─── shopping_profiles ────────────────────────────────────────────
-- Centroid embedding over already-embedded episodes.
-- composite_embedding is the vector column for this table.
UPDATE shopping_profiles
SET embedding_model_id      = v_model_id,
    embedding_model_version = v_model_version,
    embedding_dim           = v_dim,
    source_text             = NULL,
    source_ref              = jsonb_build_object('kind', 'backfill',
                                                 'centroid', TRUE),
    provenance_trusted      = FALSE
WHERE composite_embedding IS NOT NULL
  AND embedding_model_id IS NULL;

INSERT INTO embedding_provenance (
    target_table, target_id, agent_id, user_id,
    source_text, source_ref,
    model_id, model_version, dim, embedding,
    trusted, notes
)
SELECT 'shopping_profiles', sp.profile_id, sp.agent_id, sp.user_id,
       NULL::text, sp.source_ref,
       v_model_id, v_model_version, v_dim, sp.composite_embedding,
       FALSE, 'backfill'
FROM shopping_profiles sp
WHERE sp.composite_embedding IS NOT NULL
  AND sp.embedding_model_id = v_model_id
  AND sp.embedding_model_version = v_model_version
  AND NOT EXISTS (
      SELECT 1 FROM embedding_provenance ep
      WHERE ep.target_table = 'shopping_profiles'
        AND ep.target_id    = sp.profile_id
        AND ep.notes        = 'backfill'
  );

END
$$;
