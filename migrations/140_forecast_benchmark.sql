-- Migration 140: Forecast Benchmark Infrastructure
--
-- Three new tables supporting the Fermi benchmark / RSI proof:
--
-- 1. forecast_commitments  — immutable tamper-evident anchor for every
--    probability snapshot at a point in time. The clock that can't be
--    retrofitted. One row per (forecast × anchoring event).
--
-- 2. harness_snapshots     — content-addressed record of the configuration
--    that produced a forecast: conductor version, routing weights, specialist
--    roster, BayesOps params. Required for attributing score changes to
--    specific surface updates rather than to noise.
--
-- 3. forecast_splits       — deterministic held-in/held-out/validation
--    assignment at question ingestion. Immutable. Required for the gate.
--
-- 4. forecast_spacetime    — one row per (forecast × revision) with the
--    full context snapshot at that moment. This is the "spacetime view":
--    every state the forecast ever occupied, what drove the change, and
--    what the harness looked like when it changed. Enables retrospective
--    rate-of-change analysis and cross-loop correlation.
--
-- SimOps reuse: forecast_commitments is also used for SOSA projection
-- commitments — a predicted value committed before the batch completes
-- follows the same tamper-evidence logic as a probability before resolution.

-- ── 1. Harness snapshots ───────────────────────────────────────────────────
-- Content-addressed so two identical configurations produce the same hash.
-- This is what "harness version" means: the triple
--   (conductor_card_hash, routing_weights_hash, specialist_roster_hash)
-- that would reproduce the same forecast behaviour across any workspace.

CREATE TABLE IF NOT EXISTS harness_snapshots (
    snapshot_id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Content hash: sha256 of (conductor_card_hash||routing_weights_hash||specialist_roster_hash||bayesops_params_hash)
    content_hash        TEXT NOT NULL UNIQUE,

    -- Component hashes (each independently auditable)
    conductor_card_hash TEXT NOT NULL,  -- sha256 of fermi agent_card.json at emit time
    routing_weights_hash TEXT,          -- sha256 of calibration profile snapshot; null = cold start
    specialist_roster_hash TEXT NOT NULL, -- sha256 of sorted specialist agent names + versions
    bayesops_params_hash TEXT,          -- null until BayesOps operational

    -- Human-readable provenance
    conductor_version   TEXT NOT NULL,  -- agent card version field
    specialist_roster   JSONB NOT NULL, -- [{agent_id, version, calibration_score, n_resolved}]
    routing_weights     JSONB,          -- per-specialist weights at snapshot time; null = uniform
    bayesops_params     JSONB,          -- fitted distribution params; null until BayesOps

    -- Lineage
    parent_hash         TEXT,           -- null for h_0; content_hash of predecessor
    surface_changed     TEXT,           -- which of the 3 surfaces changed from parent:
                                        --   'routing_weights' | 'decomposition_norms' | 'bayesops_params'
    change_rationale    TEXT,           -- why this transition was accepted

    captured_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_harness_snapshots_hash
    ON harness_snapshots(content_hash);
CREATE INDEX IF NOT EXISTS idx_harness_snapshots_captured
    ON harness_snapshots(captured_at DESC);

-- ── 2. Forecast commitments (the immutable clock) ─────────────────────────
-- One row per (forecast × anchoring event). Anchoring events occur:
--   a) on forecast creation (initial commitment)
--   b) on every revision (each update is independently anchored)
--   c) on the daily cron sweep (catches any forecasts not yet anchored)
--
-- The commitment_hash makes each snapshot tamper-evident:
--   sha256(forecast_id || predicted_probability || fpl_hash || emitted_ts)
--
-- A forecast is "properly benchmarkable" only for revisions where
-- committed_at < resolved_at. The cron job ensures this holds for all
-- active forecasts regardless of whether the operator explicitly triggers it.

CREATE TABLE IF NOT EXISTS forecast_commitments (
    commitment_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    forecast_id         TEXT NOT NULL REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    -- The revision this commitment covers (null = initial, else FK to updates table)
    revision_id         TEXT REFERENCES fermi_forecast_updates(id) ON DELETE SET NULL,

    -- The committed snapshot
    predicted_probability REAL NOT NULL,
    fpl_source_hash     TEXT,           -- sha256 of fpl_source at this moment
    harness_snapshot_id UUID REFERENCES harness_snapshots(snapshot_id),

    -- The anchor
    commitment_hash     TEXT NOT NULL UNIQUE,  -- sha256(forecast_id||prob||fpl_hash||emitted_ts_iso)
    anchor_method       TEXT NOT NULL DEFAULT 'db_timestamp',
                                        -- 'db_timestamp' | 'git_commit' | 'external_log'
    anchor_ref          TEXT,           -- git commit SHA, external log URI, etc.
    anchor_note         TEXT,           -- e.g. "daily cron sweep 2026-06-14"

    -- Timing
    emitted_at          TIMESTAMPTZ NOT NULL,  -- when this probability was computed
    committed_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- For SimOps reuse: a SOSA projection commitment is the same structure
    -- with forecast_id = NULL and sosa_projection_id set
    sosa_projection_id  TEXT,           -- links to sosa_observations.extra->>'projection_id'

    CONSTRAINT commitment_has_subject CHECK (
        forecast_id IS NOT NULL OR sosa_projection_id IS NOT NULL
    )
);

CREATE INDEX IF NOT EXISTS idx_forecast_commitments_forecast
    ON forecast_commitments(forecast_id, committed_at DESC);
CREATE INDEX IF NOT EXISTS idx_forecast_commitments_hash
    ON forecast_commitments(commitment_hash);
CREATE INDEX IF NOT EXISTS idx_forecast_commitments_sosa
    ON forecast_commitments(sosa_projection_id)
    WHERE sosa_projection_id IS NOT NULL;

-- ── 3. Forecast splits (immutable at ingestion) ───────────────────────────
-- Deterministic assignment: last byte of sha256(question_id + salt) mod 10
--   0-4 → held_in (50%), 5-7 → held_out (30%), 8-9 → validation (20%)
-- Salt is pre-registered and committed to the lineage ledger before the run.
-- No human and no loop component ever reassigns.

CREATE TABLE IF NOT EXISTS forecast_splits (
    forecast_id         TEXT PRIMARY KEY REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    split               TEXT NOT NULL CHECK (split IN ('held_in', 'held_out', 'validation')),
    split_hash_input    TEXT NOT NULL,  -- the exact string hashed (forecast_id + salt)
    split_salt          TEXT NOT NULL,  -- pre-registered salt; changing this voids the lineage
    assigned_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Contamination status (updated by the certifier)
    contamination_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (contamination_status IN ('pending', 'clean', 'contaminated', 'exempt')),
    probe_transcript    TEXT,           -- bare-model memorisation probe output (replay only)
    evidence_freeze_cutoff TIMESTAMPTZ  -- evidence corpus cutoff for replay tier
);

CREATE INDEX IF NOT EXISTS idx_forecast_splits_split
    ON forecast_splits(split, contamination_status);

-- ── 4. Forecast spacetime (the rate-of-change view) ───────────────────────
-- One row per (forecast × revision) — every state the forecast ever occupied.
-- This is the primary research object for the adaptive forecast thesis:
-- not just "was the final forecast accurate" but "how did the forecast
-- and its constituent parts evolve, at what rate, in response to what?"
--
-- Populated by: a trigger on fermi_forecast_updates + the creation handler.
-- Retroactively fillable for existing forecasts from the updates table.

CREATE TABLE IF NOT EXISTS forecast_spacetime (
    spacetime_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    forecast_id         TEXT NOT NULL REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    revision_seq        INTEGER NOT NULL DEFAULT 0,  -- 0 = initial, 1 = first update, etc.

    -- Probability at this point in time
    predicted_probability REAL NOT NULL,
    previous_probability  REAL,         -- null for revision_seq=0

    -- What drove this revision
    revision_trigger    TEXT,           -- 'initial' | 'evidence_update' | 'agent_correction'
                                        -- | 'schedule_rerun' | 'manual' | 'bayesops_refit'
    revision_reason     TEXT,           -- human/agent rationale
    triggering_agent    TEXT,           -- agent_id if an agent triggered this
    evidence_delta      JSONB,          -- new evidence items that arrived before this revision

    -- The forecast decomposition at this moment
    drivers_snapshot    JSONB,          -- full driver state: {name, specialist, p50, sobol_weight}
    base_rate_snapshot  JSONB,          -- {value, source, sample_size}
    fpl_snapshot        TEXT,           -- full FPL source at this revision
    sobol_snapshot      JSONB,          -- {driver: first_order_index} — which drivers dominated

    -- The harness at this moment
    harness_snapshot_id UUID REFERENCES harness_snapshots(snapshot_id),

    -- Calibration signal at this moment (populated after resolution)
    brier_at_this_point REAL,           -- what the Brier would have been if resolved here
                                        -- null until resolution; filled retrospectively

    -- Cross-loop context snapshot (the RSI proof data)
    loop1_signal        JSONB,          -- {agent: eval_signals mean per dim at this ts}
    loop3_coherence     REAL,           -- Γ(C) of the workspace at this revision
    loop5_calibration   JSONB,          -- {specialist: calibration_score} at this ts

    -- Rate-of-change metrics (computed, not stored raw)
    -- These are derived in the query layer:
    --   probability_velocity = (prob - prev_prob) / seconds_since_prev
    --   driver_shift = max(|sobol_now - sobol_prev|) across drivers
    --   information_gain = KL(p_now || p_prev) — measures how much the
    --                      forecast moved relative to its own uncertainty

    committed_at        TIMESTAMPTZ,    -- FK to forecast_commitments.committed_at
    revision_ts         TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (forecast_id, revision_seq)
);

CREATE INDEX IF NOT EXISTS idx_spacetime_forecast
    ON forecast_spacetime(forecast_id, revision_seq ASC);
CREATE INDEX IF NOT EXISTS idx_spacetime_harness
    ON forecast_spacetime(harness_snapshot_id);
CREATE INDEX IF NOT EXISTS idx_spacetime_ts
    ON forecast_spacetime(revision_ts DESC);

-- ── Trigger: auto-populate spacetime on forecast update ───────────────────
-- Every write to fermi_forecast_updates also writes a spacetime row so the
-- view is always current without the application layer having to remember.

CREATE OR REPLACE FUNCTION fn_forecast_spacetime_on_update()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO forecast_spacetime (
        forecast_id, revision_seq, predicted_probability, previous_probability,
        revision_trigger, revision_reason, triggering_agent, evidence_delta,
        fpl_snapshot, revision_ts
    )
    SELECT
        NEW.forecast_id,
        COALESCE((
            SELECT MAX(revision_seq) + 1
            FROM forecast_spacetime
            WHERE forecast_id = NEW.forecast_id
        ), 1),
        NEW.new_probability,
        NEW.previous_probability,
        'evidence_update',
        NEW.reason,
        NEW.agent_id,
        NEW.evidence_added,
        (SELECT fpl_source FROM fermi_forecasts WHERE id = NEW.forecast_id),
        NEW.created_at;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_forecast_spacetime ON fermi_forecast_updates;
CREATE TRIGGER trg_forecast_spacetime
    AFTER INSERT ON fermi_forecast_updates
    FOR EACH ROW EXECUTE FUNCTION fn_forecast_spacetime_on_update();

-- ── Backfill spacetime for existing forecasts ─────────────────────────────
-- revision_seq=0 rows for forecasts that have no spacetime row yet.
INSERT INTO forecast_spacetime (
    forecast_id, revision_seq, predicted_probability,
    revision_trigger, revision_ts
)
SELECT
    f.id, 0, f.predicted_probability, 'initial', f.created_at
FROM fermi_forecasts f
WHERE NOT EXISTS (
    SELECT 1 FROM forecast_spacetime s WHERE s.forecast_id = f.id AND s.revision_seq = 0
)
ON CONFLICT (forecast_id, revision_seq) DO NOTHING;

-- Backfill revision_seq>=1 from existing fermi_forecast_updates
INSERT INTO forecast_spacetime (
    forecast_id, revision_seq, predicted_probability, previous_probability,
    revision_trigger, revision_reason, triggering_agent, evidence_delta,
    revision_ts
)
SELECT
    u.forecast_id,
    ROW_NUMBER() OVER (PARTITION BY u.forecast_id ORDER BY u.created_at) AS revision_seq,
    u.new_probability,
    u.previous_probability,
    'evidence_update',
    u.reason,
    u.agent_id,
    u.evidence_added,
    u.created_at
FROM fermi_forecast_updates u
WHERE NOT EXISTS (
    SELECT 1 FROM forecast_spacetime s
    WHERE s.forecast_id = u.forecast_id
      AND s.predicted_probability = u.new_probability
      AND s.revision_ts = u.created_at
)
ON CONFLICT (forecast_id, revision_seq) DO NOTHING;
