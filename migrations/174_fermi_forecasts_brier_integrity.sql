-- ═══════════════════════════════════════════════════════════════════
-- Migration 174 — Brier scoring integrity on fermi_forecasts
--
-- PROBLEM
-- -------
-- `brier_score` is computed against `predicted_probability` at
-- resolution time, but `predicted_probability` stays mutable
-- afterwards. Nine server-side writers update it and NONE of them
-- filter on status:
--
--   src/bin/apply_wc_cascades.rs:178      src/handlers/forecasts.rs:965
--   src/bin/resim_wc.rs:166               src/handlers/forecasts.rs:1648
--   src/handlers/relationships/recompose.rs:158
--   src/handlers/relationships/propagation.rs:214
--   src/handlers/relationships/undo.rs:106
--   src/handlers/bayesops.rs:703          src/handlers/workspace/refit.rs:1039
--
-- Observed damage: all 47 forecasts resolved via the Polymarket path
-- had `predicted_probability` overwritten after `resolved_at` by the
-- World Cup cascade binary, which explicitly pins an eliminated
-- forecast to 0.001 (apply_wc_cascades.rs:140) and clamps survivors
-- to 0.999 (:133). 91 revisions landed post-resolution. The stored
-- pair (predicted_probability, brier_score) became mutually
-- inconsistent: recomputing Brier from the table yields ~1e-6 for
-- every row while the stored scores range up to 0.195.
--
-- FIX
-- ---
-- 1. `scored_probability` — the probability Brier was actually
--    computed against, snapshotted at resolution. This is the audit
--    anchor: Brier stays reproducible regardless of what later
--    mutates the live `predicted_probability`.
--
-- 2. `resolution_source` — structured provenance. Previously the only
--    signal was a magic string in `resolved_by` plus an unindexed
--    metadata JSONB key, which made operator, oracle and
--    backtest-seed outcomes indistinguishable (the seeder writes a
--    real user_id into resolved_by — scripts/brier_backtest_seed.rs:190).
--
-- 3. A BEFORE UPDATE trigger that freezes the scoring tuple once a
--    forecast is resolved. Deliberately non-fatal (pin the old value
--    + RAISE WARNING) rather than RAISE EXCEPTION: nine callers would
--    otherwise start erroring in production. Corruption becomes
--    impossible; the attempt stays observable in the Postgres log.
--
-- BACKFILL
-- --------
-- `scored_probability` is recovered algebraically from the stored
-- Brier score, which is exact because Brier is a square over a
-- bounded domain:
--     actual = true  →  brier = (p-1)^2  →  p = 1 - sqrt(brier)
--     actual = false →  brier = p^2      →  p = sqrt(brier)
-- Both roots are unambiguous for p ∈ [0,1]. Cross-validated against
-- fermi_forecast_updates.previous_probability (the pre-cascade value)
-- for all 47 damaged rows: agreement within 0.0005 on 47/47.
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE fermi_forecasts
    ADD COLUMN IF NOT EXISTS scored_probability REAL;

ALTER TABLE fermi_forecasts
    ADD COLUMN IF NOT EXISTS resolution_source TEXT
        CHECK (resolution_source IS NULL OR resolution_source IN (
            'operator',
            'polymarket_oracle',
            'polymarket_price_heuristic',
            'workspace_upstream',
            'backtest_seed',
            'unknown'
        ));

COMMENT ON COLUMN fermi_forecasts.scored_probability IS
    'The predicted_probability that brier_score was computed against, snapshotted at resolution and immutable thereafter (enforced by trg_fermi_forecasts_freeze_resolved). Always use this — not predicted_probability — when recomputing or auditing a Brier score. predicted_probability remains a live value that cascade/recompose/refit paths may continue to update.';

COMMENT ON COLUMN fermi_forecasts.resolution_source IS
    'How the outcome was established. ''polymarket_price_heuristic'' means the outcome was inferred from a settled price threshold (yes_price > 0.9 / < 0.1 in src/polymarket/mod.rs:688-699), NOT read from an oracle — these are NOT hard-verified settlements. ''polymarket_oracle'' is reserved for genuine UMA lifecycle reads. ''backtest_seed'' rows are synthetic and must be excluded from calibration claims.';

-- ─── Backfill scored_probability (exact algebraic recovery) ──────────

UPDATE fermi_forecasts
   SET scored_probability = CASE
           WHEN actual_outcome THEN 1.0 - sqrt(brier_score::double precision)
           ELSE sqrt(brier_score::double precision)
       END
 WHERE status = 'resolved'
   AND brier_score IS NOT NULL
   AND actual_outcome IS NOT NULL
   AND scored_probability IS NULL;

-- Rows with no Brier to invert: fall back to the live value. These are
-- unverifiable but at least explicit rather than silently absent.
UPDATE fermi_forecasts
   SET scored_probability = predicted_probability
 WHERE status = 'resolved'
   AND scored_probability IS NULL;

-- ─── Backfill resolution_source ─────────────────────────────────────
-- Order matters: the backtest seeder writes a real user_id into
-- resolved_by, so it must be matched on metadata before the generic
-- "resolved_by looks like an operator" branch.

UPDATE fermi_forecasts
   SET resolution_source = CASE
           WHEN metadata->>'source' = 'brier_backtest_seed'
             OR metadata ? 'backtest_id'                    THEN 'backtest_seed'
           WHEN resolved_by = 'polymarket_oracle'           THEN 'polymarket_price_heuristic'
           WHEN resolved_by IS NOT NULL AND resolved_by <> '' THEN 'operator'
           ELSE 'unknown'
       END
 WHERE status = 'resolved'
   AND resolution_source IS NULL;

-- ─── Freeze the scoring tuple once resolved ─────────────────────────

CREATE OR REPLACE FUNCTION fn_fermi_forecasts_freeze_resolved()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    -- Only guard a row that was already resolved and stays resolved.
    -- Moving status away from 'resolved' is a deliberate un-resolve and
    -- is allowed to rewrite the tuple.
    IF OLD.status = 'resolved' AND NEW.status = 'resolved' THEN

        IF NEW.scored_probability IS DISTINCT FROM OLD.scored_probability THEN
            RAISE WARNING 'fermi_forecasts %: scored_probability is immutable once resolved (attempted % -> %); keeping original',
                OLD.id, OLD.scored_probability, NEW.scored_probability;
            NEW.scored_probability := OLD.scored_probability;
        END IF;

        IF NEW.brier_score IS DISTINCT FROM OLD.brier_score THEN
            RAISE WARNING 'fermi_forecasts %: brier_score is immutable once resolved (attempted % -> %); keeping original',
                OLD.id, OLD.brier_score, NEW.brier_score;
            NEW.brier_score := OLD.brier_score;
        END IF;

        IF NEW.actual_outcome IS DISTINCT FROM OLD.actual_outcome THEN
            RAISE WARNING 'fermi_forecasts %: actual_outcome is immutable once resolved (attempted % -> %); keeping original',
                OLD.id, OLD.actual_outcome, NEW.actual_outcome;
            NEW.actual_outcome := OLD.actual_outcome;
        END IF;

        -- predicted_probability is the live/display value. Cascade and
        -- recompose paths have a legitimate reason to keep moving it on
        -- a resolved row, and scored_probability now protects the score.
        -- But an eliminated forecast being pinned to 0.001 is what
        -- destroyed the record in the first place, so warn loudly and
        -- refuse. Callers that genuinely want a live book view should
        -- read from the relationship layer, not overwrite history.
        IF NEW.predicted_probability IS DISTINCT FROM OLD.predicted_probability THEN
            RAISE WARNING 'fermi_forecasts %: predicted_probability is frozen once resolved (attempted % -> %); keeping original. Filter on status = ''active'' in the calling UPDATE.',
                OLD.id, OLD.predicted_probability, NEW.predicted_probability;
            NEW.predicted_probability := OLD.predicted_probability;
        END IF;

    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_fermi_forecasts_freeze_resolved ON fermi_forecasts;

CREATE TRIGGER trg_fermi_forecasts_freeze_resolved
    BEFORE UPDATE ON fermi_forecasts
    FOR EACH ROW
    EXECUTE FUNCTION fn_fermi_forecasts_freeze_resolved();

-- ─── resolve_forecast(): snapshot the scored probability ─────────────
-- Signature is unchanged (text, boolean, text, text) -> real so the
-- schema_trust pin at src/schema_trust.rs:241 still holds. The only
-- change is that it now records scored_probability and stamps
-- resolution_source = 'operator' (this function is reached only from
-- the ACL-gated operator handler at src/handlers/forecasts.rs:1080).

CREATE OR REPLACE FUNCTION resolve_forecast(
    p_forecast_id TEXT,
    p_actual_outcome BOOLEAN,
    p_resolved_by TEXT,
    p_resolution_notes TEXT DEFAULT NULL
) RETURNS REAL AS $$
DECLARE
    v_predicted REAL;
    v_brier REAL;
    v_status TEXT;
BEGIN
    SELECT predicted_probability, status
    INTO v_predicted, v_status
    FROM fermi_forecasts
    WHERE id = p_forecast_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Forecast % not found', p_forecast_id;
    END IF;

    IF v_status != 'active' THEN
        RAISE EXCEPTION 'Forecast % is not active (status: %)', p_forecast_id, v_status;
    END IF;

    v_brier := compute_brier_score(v_predicted, p_actual_outcome);

    UPDATE fermi_forecasts SET
        actual_outcome     = p_actual_outcome,
        brier_score        = v_brier,
        scored_probability = v_predicted,
        resolution_source  = COALESCE(resolution_source, 'operator'),
        status             = 'resolved',
        resolved_at        = NOW(),
        resolved_by        = p_resolved_by,
        resolution_notes   = p_resolution_notes,
        updated_at         = NOW()
    WHERE id = p_forecast_id;

    RETURN v_brier;
END;
$$ LANGUAGE plpgsql;

-- ─── Audit helper ───────────────────────────────────────────────────
-- Any row where this returns false has a Brier score that cannot be
-- reproduced from its own stored inputs.

CREATE OR REPLACE VIEW fermi_brier_audit AS
SELECT
    id,
    question_text,
    resolution_source,
    actual_outcome,
    scored_probability,
    predicted_probability,
    brier_score,
    compute_brier_score(scored_probability, actual_outcome) AS recomputed_brier,
    (brier_score IS NOT NULL
     AND scored_probability IS NOT NULL
     AND actual_outcome IS NOT NULL
     AND abs(brier_score - compute_brier_score(scored_probability, actual_outcome)) < 0.0005
    ) AS brier_reproducible,
    (predicted_probability IS DISTINCT FROM scored_probability) AS live_value_has_drifted,
    resolved_at,
    resolved_by
FROM fermi_forecasts
WHERE status = 'resolved';

COMMENT ON VIEW fermi_brier_audit IS
    'One row per resolved forecast with a reproducibility check. brier_reproducible = false means the stored score cannot be derived from the stored inputs. live_value_has_drifted = true is expected and benign post-mig-174 (predicted_probability is a live value; scored_probability is the frozen audit anchor).';
