-- ─────────────────────────────────────────────────────────────────────
-- 158 — sim_probability: raw standalone mean, separate from the displayed
--       (post-cascade) predicted_probability.
-- ─────────────────────────────────────────────────────────────────────
--
-- predicted_probability is the value the dashboard/cockpit show. For
-- forecasts in a mutex relationship group it is a DERIVED value:
--
--     predicted_probability = recompose(sim_probability, eliminated mass)
--
-- where sim_probability is the forecast's own Monte-Carlo mean (its
-- standalone strength) and the recompose redistributes resolved siblings'
-- freed mass across survivors (Spec 25 §3.1).
--
-- Keeping the raw mean in its own column makes the recompose idempotent:
-- it always reads sim_probability, never the already-recomposed displayed
-- value, so re-running a sim recomputes the standalone AND re-applies the
-- eliminations every time — instead of resetting the displayed value back
-- to the standalone (the "re-sim drops the cascade" bug).
--
-- Backfill of existing rows is a one-off (scripts/world_cup, run via psql)
-- because it depends on cascade history; this migration only adds the
-- column so it is safe to re-run on every startup.

ALTER TABLE public.fermi_forecasts
    ADD COLUMN IF NOT EXISTS sim_probability REAL;
