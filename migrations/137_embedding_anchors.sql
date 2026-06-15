-- Migration 137: Closed-model anchor set (Spec 22, Phase 2.1)
--
-- Standing insurance against vendor model deprecation. The anchor set is a
-- fixed collection of texts co-embedded against:
--   (a) each vendor model in active use (vector(1024))
--   (b) our reference open model: nomic-embed-text-v1.5 (vector(768))
--
-- If a vendor goes dark, the anchor pairs are SUFFICIENT to fit a Tier 2
-- translator post-hoc and carry orphaned vendor vectors forward to the
-- reference space. This is the cheap hedge for the closed-model 5% case
-- the spec calls out.
--
-- Refresh policy: a vendor's side is recomputed when (a) the vendor's
-- model_version is bumped in code (model drift suspected), (b) on a 7-day
-- schedule, or (c) on demand. The reference side is recomputed only when
-- NOMIC_EMBED_VERSION is bumped.
--
-- See docs/specs/22_EMBEDDING_PORTABILITY_SPEC.md §2 for context.

CREATE TABLE IF NOT EXISTS embedding_anchors (
    anchor_id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The literal text being co-embedded. Same text → same anchor across all
    -- vendor models. SHA-256 of the text is stored too, for fast lookup and
    -- to dedupe near-misses at seeding time.
    anchor_text          TEXT NOT NULL,
    anchor_text_hash     BYTEA NOT NULL,
    -- Composition bucket: where this anchor came from. Useful for diversity
    -- analysis and for staged refresh (e.g. "refresh only the 'rules' bucket").
    -- Values: 'episode', 'rule', 'entity', 'external' (matches spec §2.1).
    anchor_source        TEXT NOT NULL CHECK (anchor_source IN
                                              ('episode','rule','entity','external')),
    anchor_set_version   INTEGER NOT NULL DEFAULT 1,

    -- ── Reference side (open model) ──────────────────────────────
    -- The reference embedding exists for EVERY anchor row. If
    -- NOMIC_EMBED_VERSION bumps, this column is recomputed.
    reference_model_id      TEXT NOT NULL,
    reference_model_version TEXT NOT NULL,
    reference_embedding     vector(768) NOT NULL,
    reference_refreshed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- ── Vendor side (closed model) ───────────────────────────────
    -- NULL until the anchor has been embedded against this specific vendor
    -- model. One row per (anchor_text, vendor_model_id, vendor_model_version)
    -- is the unique key — see index below — so different vendor models live
    -- in separate rows that share the SAME reference embedding via
    -- anchor_text_hash join.
    vendor_model_id      TEXT,
    vendor_model_version TEXT,
    vendor_embedding     vector(1024),
    vendor_dim           INTEGER,
    vendor_refreshed_at  TIMESTAMPTZ,

    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Uniqueness:
--   - The reference-only seed row has vendor_model_id IS NULL.
--   - Vendor rows must be unique per (anchor_text_hash, vendor_model_id,
--     vendor_model_version) — so refreshing a vendor model_version creates
--     a NEW row, preserving the OLD vendor embedding as historical anchor
--     evidence (the spec's append-only ethic).
CREATE UNIQUE INDEX IF NOT EXISTS idx_anchors_unique_vendor_pair
    ON embedding_anchors (anchor_text_hash, vendor_model_id, vendor_model_version)
    WHERE vendor_model_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_anchors_unique_reference_seed
    ON embedding_anchors (anchor_text_hash)
    WHERE vendor_model_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_anchors_vendor_pair
    ON embedding_anchors (vendor_model_id, vendor_model_version)
    WHERE vendor_model_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_anchors_source
    ON embedding_anchors (anchor_source, anchor_set_version);

CREATE INDEX IF NOT EXISTS idx_anchors_vendor_refresh_age
    ON embedding_anchors (vendor_refreshed_at)
    WHERE vendor_model_id IS NOT NULL;

COMMENT ON TABLE embedding_anchors IS
    'Spec 22 Phase 2 — closed-model anchor set. Co-embedded against each '
    'vendor model in active use AND against the open reference model '
    '(nomic-embed-text-v1.5). Enables Tier 2 translator fitting if a vendor '
    'goes dark. Refresh via cargo run --bin refresh-embedding-anchors.';

COMMENT ON COLUMN embedding_anchors.anchor_source IS
    'Provenance bucket of the anchor text. ''episode'' / ''rule'' / ''entity'' = '
    'sampled from production corpus; ''external'' = sampled from a diverse '
    'external source (e.g. C4, Wikipedia) for domain breadth.';
