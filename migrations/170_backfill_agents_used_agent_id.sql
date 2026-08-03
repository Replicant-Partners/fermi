-- 169: Backfill agent_id into fermi_forecasts.agents_used elements.
--
-- Why
-- ---
-- Forecasts store their contributing agents as
--   agents_used = [{"name": "football_analyst", "query": ..., "driver_refs": [...]}, ...]
-- keyed by NAME only. But every observability/calibration query that
-- attributes a resolved forecast's brier_score to an agent joins by
-- agent_id:
--
--   loop_health_handler (Loop 5):
--     f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
--
-- Name != agent_id, so the containment never matched — 50 resolved
-- forecasts carrying real brier_score values were invisible to Loop 5,
-- which reported n=0 for every agent even though the data existed.
-- (The brier_scores themselves committed fine; the v0.10.19 FLOAT4/f64
-- error was a client-side decode failure that did not roll back the
-- server-side UPDATE.)
--
-- What
-- ----
-- For every agents_used element whose `name` resolves to a real
-- `agents.agent_name`, add an `agent_id` key alongside the existing
-- `name`. Elements whose name doesn't resolve, or that already carry an
-- agent_id, are left untouched. `agent_name` is UNIQUE so the lookup is
-- 1:1. Order is preserved via WITH ORDINALITY.
--
-- Idempotent: re-running is a no-op (the `NOT (e ? 'agent_id')` guard +
-- the `IS DISTINCT FROM` row filter). Applies to all statuses so active
-- forecasts are already attributable the moment they resolve.
--
-- Forward fix (separate, code side): the forecast write path should emit
-- agent_id into agents_used at creation time so this backfill never
-- needs to run again.

UPDATE fermi_forecasts f
SET agents_used = sub.new_arr
FROM (
    SELECT f2.id,
           jsonb_agg(
               CASE
                   WHEN a.agent_id IS NOT NULL AND NOT (e ? 'agent_id')
                       THEN e || jsonb_build_object('agent_id', a.agent_id::text)
                   ELSE e
               END
               ORDER BY ord
           ) AS new_arr
    FROM fermi_forecasts f2
    CROSS JOIN LATERAL
        jsonb_array_elements(f2.agents_used) WITH ORDINALITY AS t(e, ord)
    LEFT JOIN agents a ON a.agent_name = e->>'name'
    WHERE jsonb_typeof(f2.agents_used) = 'array'
      AND jsonb_array_length(f2.agents_used) > 0
    GROUP BY f2.id
) sub
WHERE f.id = sub.id
  AND f.agents_used IS DISTINCT FROM sub.new_arr;
