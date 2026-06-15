-- Migration 136: Enforce embedding-provenance integrity (Spec 22, Phase 1c)
--
-- After the backfill binary (scripts/backfill_embedding_provenance.rs) has run
-- against every environment, this migration converts the soft Spec 22 discipline
-- into a HARD database invariant.
--
-- The constraint says: "if a row carries an embedding, it MUST carry full
-- provenance." After this migration lands, any code path that writes an
-- embedding without provenance fails at the database level, not at code review.
--
-- The `NOT VALID` + `VALIDATE` pattern avoids a full-table lock on production:
--   - ADD CONSTRAINT ... NOT VALID is an O(1) metadata change.
--   - VALIDATE CONSTRAINT does the table scan with only a SHARE lock.
--   - If validation fails, the constraint exists but isn't enforced — fix the
--     offending rows, then re-validate.
--
-- ──────────────────────────────────────────────────────────────────
-- PRE-FLIGHT (verify before running this migration in production):
--
--   -- 1. Confirm backfill has run (no pre-Spec-22 unstamped rows remain):
--   SELECT 'episodes',         COUNT(*) FROM episodes         WHERE embedding IS NOT NULL AND embedding_model_id IS NULL
--   UNION ALL SELECT 'semantic_rules',  COUNT(*) FROM semantic_rules  WHERE embedding IS NOT NULL AND embedding_model_id IS NULL
--   UNION ALL SELECT 'entities',        COUNT(*) FROM entities        WHERE embedding IS NOT NULL AND embedding_model_id IS NULL
--   UNION ALL SELECT 'communities',     COUNT(*) FROM communities     WHERE embedding IS NOT NULL AND embedding_model_id IS NULL
--   UNION ALL SELECT 'shopping_profiles', COUNT(*) FROM shopping_profiles WHERE composite_embedding IS NOT NULL AND embedding_model_id IS NULL;
--   -- Every count must be 0 before VALIDATE will succeed.
--
--   -- 2. Confirm pgvector_dims() is available (pgvector >= 0.5.0):
--   SELECT vector_dims(ARRAY[1.0, 2.0, 3.0]::vector);
-- ──────────────────────────────────────────────────────────────────

-- ─── episodes ─────────────────────────────────────────────
ALTER TABLE episodes
    ADD CONSTRAINT episodes_embedding_has_provenance
    CHECK (
        embedding IS NULL OR (
            embedding_model_id      IS NOT NULL
        AND embedding_model_version IS NOT NULL
        AND embedding_dim           IS NOT NULL
        )
    ) NOT VALID;
ALTER TABLE episodes VALIDATE CONSTRAINT episodes_embedding_has_provenance;

-- ─── semantic_rules ───────────────────────────────────────
ALTER TABLE semantic_rules
    ADD CONSTRAINT semantic_rules_embedding_has_provenance
    CHECK (
        embedding IS NULL OR (
            embedding_model_id      IS NOT NULL
        AND embedding_model_version IS NOT NULL
        AND embedding_dim           IS NOT NULL
        )
    ) NOT VALID;
ALTER TABLE semantic_rules VALIDATE CONSTRAINT semantic_rules_embedding_has_provenance;

-- ─── entities ─────────────────────────────────────────────
ALTER TABLE entities
    ADD CONSTRAINT entities_embedding_has_provenance
    CHECK (
        embedding IS NULL OR (
            embedding_model_id      IS NOT NULL
        AND embedding_model_version IS NOT NULL
        AND embedding_dim           IS NOT NULL
        )
    ) NOT VALID;
ALTER TABLE entities VALIDATE CONSTRAINT entities_embedding_has_provenance;

-- ─── communities ──────────────────────────────────────────
ALTER TABLE communities
    ADD CONSTRAINT communities_embedding_has_provenance
    CHECK (
        embedding IS NULL OR (
            embedding_model_id      IS NOT NULL
        AND embedding_model_version IS NOT NULL
        AND embedding_dim           IS NOT NULL
        )
    ) NOT VALID;
ALTER TABLE communities VALIDATE CONSTRAINT communities_embedding_has_provenance;

-- ─── shopping_profiles (vector column is `composite_embedding`) ───
ALTER TABLE shopping_profiles
    ADD CONSTRAINT shopping_profiles_embedding_has_provenance
    CHECK (
        composite_embedding IS NULL OR (
            embedding_model_id      IS NOT NULL
        AND embedding_model_version IS NOT NULL
        AND embedding_dim           IS NOT NULL
        )
    ) NOT VALID;
ALTER TABLE shopping_profiles VALIDATE CONSTRAINT shopping_profiles_embedding_has_provenance;

-- ─── embedding_provenance sidecar invariants ──────────────
-- The sidecar table's NOT NULL columns were already declared in migration 135.
-- This adds a defensive CHECK: if a vector is stored, its length must equal
-- the declared `dim`. Cheap insurance against future bugs.
ALTER TABLE embedding_provenance
    ADD CONSTRAINT embedding_provenance_dim_matches
    CHECK (
        embedding IS NULL OR vector_dims(embedding) = dim
    ) NOT VALID;
ALTER TABLE embedding_provenance VALIDATE CONSTRAINT embedding_provenance_dim_matches;

COMMENT ON CONSTRAINT episodes_embedding_has_provenance ON episodes IS
    'Spec 22 §1c: if an episode carries an embedding vector, it must also carry '
    'the model identity that produced it. Enforced as a database invariant '
    'so no Rust code path can silently bypass the discipline.';
