-- ═══════════════════════════════════════════════════════════════════
-- Migration 175 — backfill forecast_spacetime calibration columns
--
-- CONTEXT
-- -------
-- `forecast_spacetime` (mig-140) is the append-only trajectory behind
-- the console's Trajectory tab — one row per forecast revision. Four
-- columns were declared and never written by anything in the repo:
--
--   brier_at_this_point  (mig-140:173, "filled retrospectively")
--   loop5_calibration    (mig-140:179, "{specialist: calibration_score}")
--   loop1_signal, loop3_coherence
--
-- The only writer, trigger `fn_forecast_spacetime_on_update`, inserts
-- exactly ten columns and none of these. So
-- `GET /api/forecasts/:id/spacetime` returned
-- `brier_if_resolved_here: null` and `loop5_calibration: null` for every
-- row, always — the "RSI proof data" the table exists for was never
-- produced.
--
-- v0.11.4 adds a writer (`backfill_spacetime_calibration` in
-- src/handlers/forecasts.rs, invoked from both resolution paths). This
-- migration applies the same computation to forecasts that resolved
-- before it existed.
--
-- WHAT IS AND ISN'T FILLED
-- ------------------------
-- `brier_at_this_point` — what the Brier *would* have been had the
-- forecast resolved at that revision: (p_at_revision - actual)^2. This
-- is the point of the trajectory: it shows whether successive revisions
-- moved toward or away from the truth.
--
-- `loop5_calibration` — snapshot of the contributing roster's calibration,
-- {agent_name: avg_score}, stamped on the terminal revision (the state
-- the forecast was actually scored in).
--
-- `loop1_signal` and `loop3_coherence` are deliberately left NULL: they
-- are not derivable from resolution, and inventing values would be worse
-- than an honest absence.
--
-- SAFETY
-- ------
-- Derived data only. Does not touch `fermi_forecasts`, so mig-174's
-- immutable `scored_probability` / `brier_score` audit anchors are
-- unaffected and the freeze trigger is not engaged. Idempotent —
-- recomputes from stored per-revision probabilities on every run.
-- ═══════════════════════════════════════════════════════════════════

-- ─── brier_at_this_point, pre-resolution revisions only ─────────────
--
-- Scoped to revisions at or before `resolved_at`. "What the Brier would
-- have been had it resolved here" is only meaningful for a revision that
-- predates the outcome. Post-resolution revisions exist (the WC cascade
-- wrote 91 of them before mig-174 stopped it) and scoring them produces
-- actively misleading values: a forecast pinned to 0.001 after being
-- resolved NO shows a spurious 0.0000, which reads as a perfect call.
--
-- The CASE (rather than a WHERE filter) makes this self-correcting: a
-- re-run clears any post-resolution value a previous pass wrote.

UPDATE forecast_spacetime st
   SET brier_at_this_point =
         CASE
           WHEN f.resolved_at IS NOT NULL
            AND st.revision_ts IS NOT NULL
            AND st.revision_ts > f.resolved_at
           THEN NULL
           ELSE power(
                  st.predicted_probability::double precision
                  - (CASE WHEN f.actual_outcome THEN 1.0 ELSE 0.0 END),
                  2
                )
         END
  FROM fermi_forecasts f
 WHERE f.id = st.forecast_id
   AND f.status = 'resolved'
   AND f.actual_outcome IS NOT NULL
   AND st.predicted_probability IS NOT NULL;

-- ─── loop5_calibration on the terminal revision ─────────────────────
-- Scores come from the eval_signals rows emitted by
-- record_forecast_calibration_signals (dimension='forecast_calibration',
-- score = 1 - brier).

UPDATE forecast_spacetime st
   SET loop5_calibration = roster.snapshot
  FROM (
        SELECT f.id AS forecast_id,
               jsonb_object_agg(a.agent_name, cal.avg_score) AS snapshot
          FROM fermi_forecasts f
          JOIN LATERAL jsonb_array_elements(f.agents_used) AS au ON TRUE
          JOIN agents a
            ON a.agent_name = au->>'name'
          JOIN (
                SELECT agent_id, AVG(score) AS avg_score
                  FROM eval_signals
                 WHERE dimension = 'forecast_calibration'
                 GROUP BY agent_id
          ) cal ON cal.agent_id = a.agent_id
         WHERE f.status = 'resolved'
           AND jsonb_typeof(f.agents_used) = 'array'
         GROUP BY f.id
  ) AS roster
 WHERE st.forecast_id = roster.forecast_id
   AND st.revision_seq = (
       SELECT MAX(inner_st.revision_seq)
         FROM forecast_spacetime inner_st
        WHERE inner_st.forecast_id = st.forecast_id
   );
