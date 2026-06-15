-- Migration 135: Embedding Provenance (Spec 22, Phase 1.3)
--
-- Adds per-row provenance columns to the five vector-bearing tables and
-- creates the append-only `embedding_provenance` sidecar event table.
--
-- See docs/specs/22_EMBEDDING_PORTABILITY_SPEC.md for the full design.
--
-- Provenance columns are NULL-able in this migration so existing rows
-- continue to validate. Migration 136 (post-backfill) adds NOT-NULL-via-
-- CHECK constraints that enforce "if embedding IS NOT NULL then full
-- provenance IS NOT NULL".

-- ─────────────────────────────────────────────────────────────
-- Per-row provenance columns (the "current vector" denormalisation)
-- ─────────────────────────────────────────────────────────────

ALTER TABLE episodes
    ADD COLUMN IF NOT EXISTS embedding_model_id      TEXT,
    ADD COLUMN IF NOT EXISTS embedding_model_version TEXT,
    ADD COLUMN IF NOT EXISTS embedding_dim           INTEGER,
    ADD COLUMN IF NOT EXISTS source_text             TEXT,
    ADD COLUMN IF NOT EXISTS source_ref              JSONB,
    ADD COLUMN IF NOT EXISTS provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE semantic_rules
    ADD COLUMN IF NOT EXISTS embedding_model_id      TEXT,
    ADD COLUMN IF NOT EXISTS embedding_model_version TEXT,
    ADD COLUMN IF NOT EXISTS embedding_dim           INTEGER,
    ADD COLUMN IF NOT EXISTS source_text             TEXT,
    ADD COLUMN IF NOT EXISTS source_ref              JSONB,
    ADD COLUMN IF NOT EXISTS provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE entities
    ADD COLUMN IF NOT EXISTS embedding_model_id      TEXT,
    ADD COLUMN IF NOT EXISTS embedding_model_version TEXT,
    ADD COLUMN IF NOT EXISTS embedding_dim           INTEGER,
    ADD COLUMN IF NOT EXISTS source_text             TEXT,
    ADD COLUMN IF NOT EXISTS source_ref              JSONB,
    ADD COLUMN IF NOT EXISTS provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE communities
    ADD COLUMN IF NOT EXISTS embedding_model_id      TEXT,
    ADD COLUMN IF NOT EXISTS embedding_model_version TEXT,
    ADD COLUMN IF NOT EXISTS embedding_dim           INTEGER,
    ADD COLUMN IF NOT EXISTS source_text             TEXT,
    ADD COLUMN IF NOT EXISTS source_ref              JSONB,
    ADD COLUMN IF NOT EXISTS provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

-- shopping_profiles uses composite_embedding (always a centroid).
-- source_text is always NULL for these rows; source_ref carries the
-- constituent episode/agent ids that fed the centroid.
ALTER TABLE shopping_profiles
    ADD COLUMN IF NOT EXISTS embedding_model_id      TEXT,
    ADD COLUMN IF NOT EXISTS embedding_model_version TEXT,
    ADD COLUMN IF NOT EXISTS embedding_dim           INTEGER,
    ADD COLUMN IF NOT EXISTS source_text             TEXT,
    ADD COLUMN IF NOT EXISTS source_ref              JSONB,
    ADD COLUMN IF NOT EXISTS provenance_trusted      BOOLEAN NOT NULL DEFAULT TRUE;

-- ─────────────────────────────────────────────────────────────
-- Append-only sidecar: full re-embed history per (target_table, target_id)
--
-- This table is the system-of-record for "what was true at the time this
-- vector was written." It is append-only — UPDATE and DELETE are revoked
-- from PUBLIC at the bottom of this file. Re-embeds INSERT a new row
-- here AND update the per-row columns on the target table in the same
-- transaction.
-- ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS embedding_provenance (
    provenance_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_table     TEXT NOT NULL CHECK (target_table IN
                       ('episodes','semantic_rules','entities',
                        'communities','shopping_profiles')),
    target_id        UUID NOT NULL,
    agent_id         UUID,                       -- nullable for system-level seeds / shared anchors
    user_id          TEXT,
    source_text      TEXT,                       -- NULL for centroid rows (communities, shopping_profiles)
    source_ref       JSONB,                      -- {"kind":"...", caller-specific keys}
    model_id         TEXT NOT NULL,              -- e.g. "anthropic/voyage-2"
    model_version    TEXT NOT NULL,              -- manual epoch, e.g. "2024-01-01"
    dim              INTEGER NOT NULL,           -- output dimensionality
    embedding        vector(1024),               -- the actual vector at this point
                                                  -- in history (supports Tier 2 translator
                                                  -- anchor recovery if vendor goes dark)
    trusted          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes            TEXT                         -- e.g. "initial_write", "backfill",
                                                  -- "reembed_from:<old_model>:<old_version>",
                                                  -- "client_import", "forked_from:<src_agent>"
);

CREATE INDEX IF NOT EXISTS idx_provenance_target
    ON embedding_provenance (target_table, target_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_provenance_model
    ON embedding_provenance (model_id, model_version, created_at);

CREATE INDEX IF NOT EXISTS idx_provenance_agent
    ON embedding_provenance (agent_id, created_at)
    WHERE agent_id IS NOT NULL;

-- Enforce append-only at the DB level. No UPDATE or DELETE on this table
-- except via an explicit migration with a comment justifying it. The
-- creator role retains full access; only the default app role is restricted.
REVOKE UPDATE, DELETE ON embedding_provenance FROM PUBLIC;

-- vector(1024) is hardcoded in the schema above. If model_version changes
-- to a model with a different dim, the re-embed worker (Phase 3) handles
-- the schema migration. The `dim` column on each provenance row is the
-- per-write truth and is consulted for tier-2 translator inputs.

COMMENT ON TABLE embedding_provenance IS
    'Spec 22 — append-only provenance log for every embedding ever generated. '
    'Re-embeds INSERT new rows here; the per-row columns on the target table '
    'reflect the CURRENT vector. This table reflects the full HISTORY.';

COMMENT ON COLUMN embedding_provenance.trusted IS
    'TRUE = generated by our own pipeline with full provenance captured. '
    'FALSE = client-imported (model identity asserted but unverifiable) OR '
    'backfilled from pre-Spec-22 rows where source_text reconstruction may '
    'be lossy. The re-embed worker treats untrusted rows as eligible for '
    'opportunistic refresh.';
