-- ═══════════════════════════════════════════════════════════════════════
-- 201 — resolve `<agent>_<driver>` statement names in agents_used
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT mig-170 COULD NOT REACH
-- ---------------------------
-- mig-170 stamped `agent_id` onto every `agents_used` element whose `name`
-- matched an `agents.agent_name` exactly. That left a class untouched: elements
-- whose name is the FPL *statement* name rather than the agent name.
--
--   {"name": "weather_oracle_synoptic_pattern_august_2025",
--    "driver_refs": ["synoptic_pattern_august_2025"], ...}
--
-- The agent is `weather_oracle`; the suffix is what the statement computes. No
-- exact match exists, so the element stayed unresolved and every calibration
-- reader — all of which join on name or agent_id — skipped it.
--
-- Measured before this migration: 6 such elements across one forecast (the
-- London 32 °C question), plus 7 forecasts carrying an empty roster. The Loop 5
-- mechanism probe reports these as L5-M04 and L5-M03 respectively and correctly
-- declines to certify the fleet's Brier score while they stand.
--
-- WHAT THIS DOES
-- --------------
-- For each unresolved element, find the LONGEST `agents.agent_name` that is a
-- prefix of the element's name followed by `_`, and stamp its `agent_id`.
--
-- Longest, and the underscore is required. Both matter: with a shorter match
-- winning, `macro` would claim `macro_forecaster_climate_trend_adjustment` and
-- attribute one agent's Brier score to another — a worse outcome than the
-- orphan it replaces. Attribution errors are not symmetric with attribution
-- gaps, so this migration only ever adds a resolution it can justify.
--
-- Mirrors `fermi::attribution::roster::resolve_agents_used`, which now runs on
-- the write path so this cannot re-accumulate. The Rust side is the contract
-- and is unit-tested against these exact six names; this is the one-off repair.
--
-- WHAT THIS DOES NOT DO
-- ---------------------
-- L5-M03 — the 7 forecasts with `agents_used = []` — is NOT addressed and is
-- not addressable. There is no record of who contributed, so any attribution
-- would be invented. They stay orphaned and the probe keeps counting them,
-- which is the honest outcome.
--
-- Idempotent: the `NOT (e ? 'agent_id')` guard plus the `IS DISTINCT FROM` row
-- filter make re-running a no-op.
-- ═══════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    EXECUTE $upd$
        UPDATE fermi_forecasts f
        SET agents_used = sub.new_arr
        FROM (
            SELECT f2.id,
                   jsonb_agg(
                       CASE
                           WHEN NOT (e ? 'agent_id') AND m.agent_id IS NOT NULL
                               THEN e || jsonb_build_object('agent_id', m.agent_id::text)
                           ELSE e
                       END
                       ORDER BY ord
                   ) AS new_arr
            FROM fermi_forecasts f2
            CROSS JOIN LATERAL
                jsonb_array_elements(f2.agents_used) WITH ORDINALITY AS t(e, ord)
            -- Longest qualifying agent_name prefix, or NULL when none applies.
            LEFT JOIN LATERAL (
                SELECT a.agent_id
                  FROM agents a
                 WHERE COALESCE(e->>'name', e->>'agent_name', '')
                       LIKE a.agent_name || '\_%'
                 ORDER BY length(a.agent_name) DESC
                 LIMIT 1
            ) m ON TRUE
            WHERE jsonb_typeof(f2.agents_used) = 'array'
              AND jsonb_array_length(f2.agents_used) > 0
            GROUP BY f2.id
        ) sub
        WHERE f.id = sub.id
          AND f.agents_used IS DISTINCT FROM sub.new_arr
    $upd$;
END $$;
