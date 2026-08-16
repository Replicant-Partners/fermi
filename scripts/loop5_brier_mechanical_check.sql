-- ═══════════════════════════════════════════════════════════════════════
-- Loop 5a (Brier calibration) — MECHANICAL check — READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT THIS IS
--
-- Loop 5a is the chain
--
--   forecast resolves
--     → brier_score written on fermi_forecasts          (resolution path)
--     → attributed to agents via agents_used            (attribution)
--     → forecast_calibration row in eval_signals        (signal emission)
--     → read back by BrierEvaluator + /calibration      (read path)
--     → consumed as moe_router_strategist weights       (design intent)
--
-- This script asks ONE question: does that chain move a signal correctly
-- from end to end? It deliberately does NOT ask whether the resulting
-- number is impressive.
--
-- WHY THE DISTINCTION MATTERS
--
-- The loop closed recently and the data is thin, so every score it produces
-- is provisional. That is fine and expected — but it means "the number looks
-- plausible" is worthless as evidence that the plumbing works, and equally
-- "the number looks odd" is worthless as evidence that it doesn't. The two
-- have to be separated or you can never tell a wiring bug from a small
-- sample.
--
-- So findings carry a CLASS:
--
--   MECHANISM  — must be clean at ANY sample size, including n=1. A
--                violation here is a real bug: the chain is dropping,
--                duplicating, mis-attributing or mis-transforming a signal.
--                These are the checks that answer "is it working".
--
--   INFO       — sample size, skew, and discriminative power. These are
--                EXPECTED to look weak on thin data. They tell you how much
--                the signal currently means, not whether it works. Never
--                treat an INFO finding as a failure.
--
-- Read it as: MECHANISM all-OK ⇒ the loop is sound and simply needs volume.
-- MECHANISM violation ⇒ fix the code before trusting any number it emits.
--
-- SCOPE — fleet, then per agent
--
-- The MECHANISM checks below are fleet-wide. That answers "is the platform's
-- Loop 5 sound". It does NOT answer "is this agent's loop sound", because one
-- tenant's orphaned forecast turns the fleet verdict BROKEN for everybody.
-- The MECHANISM ATTRIBUTION section near the end repeats the eight scopable
-- checks per agent so a fault can be pinned on whoever owns it.
--
-- THREE COPIES, ONE CONTRACT
--
-- These checks exist three times and must not drift:
--
--   1. here                                        (fleet + per-agent, psql)
--   2. LOOP5_MECHANISM_CHECKS  in src/handlers/observatory.rs   (fleet, admin)
--   3. LOOP5_AGENT_CHECKS      in src/handlers/observatory.rs   (per agent)
--
-- Same ids, same severities, same predicates. The two Rust tables are held in
-- step by `agent_and_fleet_checks_declare_the_same_ids` and
-- `agent_and_fleet_checks_agree_on_severity`; this file is held in step by the
-- ids being greppable across all three. Change one, change all three, in one
-- commit — if they disagree, none of them can be trusted, which is worse than
-- having no probe at all.
--
-- SAFETY
--
--   * No INSERT/UPDATE/DELETE/DDL against any application table.
--   * Creates only `pg_temp` objects (session-local, vanish on disconnect).
--   * Every check is guarded on object existence, so it degrades to SKIPPED
--     rather than erroring on a database missing some of these tables.
--
-- USAGE
--
--   psql "$DIRECT_DATABASE_URL" -f scripts/loop5_brier_mechanical_check.sql
--
-- ⚠ Use a DIRECT connection, not the PgBouncer pooler. In transaction-mode
--   pooling the temp table may land on a different backend than the final
--   SELECT and you will get "relation does not exist". On Neon this is the
--   connection string WITHOUT `-pooler` in the host.
--
-- Read the output bottom-up: the per-agent table and summary print last.
-- ═══════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on
\timing off

CREATE TEMP TABLE loop5_findings (
    seq        serial,
    check_id   text,
    class      text,      -- MECHANISM | INFO
    severity   text,
    status     text,      -- OK | VIOLATION | SKIPPED | ERROR
    violations bigint,
    detail     text
);

-- ── Object-existence guard ────────────────────────────────────────────
-- Accepts 'table' or 'table.column'. Returns NULL if all present, else a
-- comma-separated list of what is missing.
CREATE FUNCTION pg_temp.missing_objects(reqs text[]) RETURNS text AS $fn$
DECLARE
    r text; t text; c text; missing text[] := '{}';
BEGIN
    FOREACH r IN ARRAY reqs LOOP
        IF position('.' in r) > 0 THEN
            t := split_part(r, '.', 1);
            c := split_part(r, '.', 2);
            IF NOT EXISTS (
                SELECT 1
                  FROM pg_attribute a
                  JOIN pg_class cl     ON cl.oid = a.attrelid
                  JOIN pg_namespace n  ON n.oid = cl.relnamespace
                 WHERE n.nspname = 'public'
                   AND cl.relname = t
                   AND a.attname = c
                   AND a.attnum > 0
                   AND NOT a.attisdropped
            ) THEN
                missing := missing || r;
            END IF;
        ELSE
            IF to_regclass('public.' || r) IS NULL THEN
                missing := missing || r;
            END IF;
        END IF;
    END LOOP;

    IF array_length(missing, 1) IS NULL THEN
        RETURN NULL;
    END IF;
    RETURN array_to_string(missing, ', ');
END
$fn$ LANGUAGE plpgsql;

-- ── Check runner ──────────────────────────────────────────────────────
-- p_count_sql must return exactly one bigint. For MECHANISM checks that is
-- a violation count and 0 means OK. For INFO checks the number is just an
-- observation, so status is always 'OK' and the count is reported as-is.
CREATE FUNCTION pg_temp.chk(
    p_id         text,
    p_class      text,
    p_severity   text,
    p_reqs       text[],
    p_count_sql  text,
    p_sample_sql text DEFAULT NULL,
    p_note       text DEFAULT NULL
) RETURNS void AS $fn$
DECLARE
    miss text;
    n    bigint;
    smp  text;
BEGIN
    miss := pg_temp.missing_objects(p_reqs);
    IF miss IS NOT NULL THEN
        INSERT INTO loop5_findings(check_id, class, severity, status, violations, detail)
        VALUES (p_id, p_class, p_severity, 'SKIPPED', NULL,
                'cannot check — missing: ' || miss);
        RETURN;
    END IF;

    BEGIN
        EXECUTE p_count_sql INTO n;
    EXCEPTION WHEN OTHERS THEN
        INSERT INTO loop5_findings(check_id, class, severity, status, violations, detail)
        VALUES (p_id, p_class, p_severity, 'ERROR', NULL, SQLERRM);
        RETURN;
    END;

    -- Samples are only worth fetching when there is something to look at.
    IF n > 0 AND p_sample_sql IS NOT NULL THEN
        BEGIN
            EXECUTE p_sample_sql INTO smp;
        EXCEPTION WHEN OTHERS THEN
            smp := '(sample failed: ' || SQLERRM || ')';
        END;
    END IF;

    INSERT INTO loop5_findings(check_id, class, severity, status, violations, detail)
    VALUES (p_id, p_class, p_severity,
            CASE WHEN p_class = 'INFO' THEN 'OK'
                 WHEN n = 0            THEN 'OK'
                 ELSE 'VIOLATION' END,
            n,
            COALESCE(NULLIF(concat_ws(' | ', p_note, smp), ''), p_note));
END
$fn$ LANGUAGE plpgsql;


-- ═══════════════════════════════════════════════════════════════════════
-- STAGE 1 — RESOLUTION writes the score
-- ═══════════════════════════════════════════════════════════════════════

SELECT pg_temp.chk(
  'L5-M01', 'MECHANISM', 'CRITICAL',
  ARRAY['fermi_forecasts.status','fermi_forecasts.actual_outcome','fermi_forecasts.brier_score'],
  $q$
    SELECT count(*) FROM fermi_forecasts
     WHERE status = 'resolved' AND actual_outcome IS NOT NULL AND brier_score IS NULL
  $q$,
  $q$
    SELECT string_agg(id, '; ') FROM (
      SELECT id FROM fermi_forecasts
       WHERE status='resolved' AND actual_outcome IS NOT NULL AND brier_score IS NULL
       ORDER BY resolved_at DESC NULLS LAST LIMIT 10
    ) s
  $q$,
  'Resolution must compute brier_score. A row here means the resolve/oracle path recorded an outcome but never scored it, so Loop 5 can never receive a signal for that forecast. This is the first link in the chain and it is silent when it breaks.'
);

SELECT pg_temp.chk(
  'L5-M02', 'MECHANISM', 'CRITICAL',
  ARRAY['fermi_forecasts.brier_score','fermi_forecasts.scored_probability','fermi_forecasts.actual_outcome'],
  $q$
    SELECT count(*) FROM fermi_forecasts
     WHERE status = 'resolved'
       AND brier_score IS NOT NULL
       AND scored_probability IS NOT NULL
       AND abs(brier_score::float8
               - power(scored_probability::float8
                       - (CASE WHEN actual_outcome THEN 1.0 ELSE 0.0 END), 2)) > 1e-4
  $q$,
  $q$
    SELECT string_agg(t, '; ') FROM (
      SELECT format('%s: stored=%s recomputed=%s', id, brier_score,
                    round(power(scored_probability::float8
                          - (CASE WHEN actual_outcome THEN 1.0 ELSE 0.0 END), 2)::numeric, 6)) AS t
        FROM fermi_forecasts
       WHERE status='resolved' AND brier_score IS NOT NULL AND scored_probability IS NOT NULL
         AND abs(brier_score::float8
                 - power(scored_probability::float8
                         - (CASE WHEN actual_outcome THEN 1.0 ELSE 0.0 END), 2)) > 1e-4
       ORDER BY resolved_at DESC NULLS LAST LIMIT 10
    ) s
  $q$,
  'brier_score must be exactly reproducible from the frozen (scored_probability, actual_outcome) pair — that is what mig-174 created scored_probability for. Divergence means either the freeze trigger was bypassed or the score was computed against a probability that has since changed. Without this, no downstream number is auditable.'
);


-- ═══════════════════════════════════════════════════════════════════════
-- STAGE 2 — ATTRIBUTION: which agents own the score
-- ═══════════════════════════════════════════════════════════════════════
--
-- The join below mirrors, exactly, the three-shape predicate used by
-- src/handlers/eval_brier.rs::latest_for_agent and by
-- src/handlers/agents.rs::get_agent_calibration_handler. If you change one,
-- change all three or the readers will disagree again.

SELECT pg_temp.chk(
  'L5-M03', 'MECHANISM', 'HIGH',
  ARRAY['fermi_forecasts.agents_used','agents.agent_id','agents.agent_name'],
  $q$
    SELECT count(*) FROM fermi_forecasts f
     WHERE f.status='resolved' AND f.brier_score IS NOT NULL
       AND NOT EXISTS (
         SELECT 1
           FROM jsonb_array_elements(
                  CASE WHEN jsonb_typeof(f.agents_used)='array'
                       THEN f.agents_used ELSE '[]'::jsonb END) e
           JOIN agents a
             ON a.agent_id::text = e->>'agent_id'
             OR a.agent_name     = e->>'agent_name'
             OR a.agent_name     = e->>'name'
       )
  $q$,
  $q$
    SELECT string_agg(t, '; ') FROM (
      SELECT format('%s (agents_used=%s)', f.id,
                    left(COALESCE(f.agents_used::text,'null'), 80)) AS t
        FROM fermi_forecasts f
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
         AND NOT EXISTS (
           SELECT 1 FROM jsonb_array_elements(
                    CASE WHEN jsonb_typeof(f.agents_used)='array'
                         THEN f.agents_used ELSE '[]'::jsonb END) e
             JOIN agents a ON a.agent_id::text=e->>'agent_id'
                           OR a.agent_name=e->>'agent_name'
                           OR a.agent_name=e->>'name')
       ORDER BY f.resolved_at DESC NULLS LAST LIMIT 10
    ) s
  $q$,
  'A scored forecast that resolves to no agent is a signal with nowhere to go: the Brier exists but no agent calibration can ever include it. Usually an empty agents_used, or a roster naming agents that no longer exist under that name.'
);

SELECT pg_temp.chk(
  'L5-M04', 'MECHANISM', 'HIGH',
  ARRAY['fermi_forecasts.agents_used','agents.agent_name'],
  $q$
    SELECT count(*) FROM fermi_forecasts f
     CROSS JOIN LATERAL jsonb_array_elements(
            CASE WHEN jsonb_typeof(f.agents_used)='array'
                 THEN f.agents_used ELSE '[]'::jsonb END) e
     WHERE f.status='resolved' AND f.brier_score IS NOT NULL
       AND NOT EXISTS (
         SELECT 1 FROM agents a
          WHERE a.agent_id::text = e->>'agent_id'
             OR a.agent_name     = e->>'agent_name'
             OR a.agent_name     = e->>'name')
  $q$,
  $q$
    SELECT string_agg(DISTINCT COALESCE(e->>'name', e->>'agent_name', e->>'agent_id', e::text), '; ')
      FROM fermi_forecasts f
     CROSS JOIN LATERAL jsonb_array_elements(
            CASE WHEN jsonb_typeof(f.agents_used)='array'
                 THEN f.agents_used ELSE '[]'::jsonb END) e
     WHERE f.status='resolved' AND f.brier_score IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM agents a
                        WHERE a.agent_id::text=e->>'agent_id'
                           OR a.agent_name=e->>'agent_name'
                           OR a.agent_name=e->>'name')
  $q$,
  'Individual roster entries that match no agent row — partial credit loss on forecasts that are otherwise attributable. Typically a rename that skipped the agents_used backfill (see v0.10.20 / mig-170).'
);


-- ═══════════════════════════════════════════════════════════════════════
-- STAGE 3 — SIGNAL EMISSION: the reconciliation that actually proves it
-- ═══════════════════════════════════════════════════════════════════════
--
-- record_forecast_calibration_signals (src/handlers/forecasts.rs) writes one
-- eval_signals row per (contributing agent, resolved forecast), with
-- dimension='forecast_calibration' and
-- rationale='forecast <id> resolved (brier=...)'. That rationale is the join
-- key back to the forecast, so emission is fully reconcilable.

-- L5-M05 is the sharpest test in this file: it is history-independent.
-- Forecasts that resolved BEFORE the emitter shipped legitimately have no
-- signals for anyone (counted separately as INFO in L5-I01). But a forecast
-- where SOME attributable agents got a signal and others did not cannot be
-- explained by history — the emitter ran on that forecast and dropped rows.
SELECT pg_temp.chk(
  'L5-M05', 'MECHANISM', 'CRITICAL',
  ARRAY['eval_signals.agent_id','eval_signals.dimension','eval_signals.rationale',
        'fermi_forecasts.agents_used','agents.agent_id'],
  $q$
    WITH pairs AS (
      SELECT DISTINCT f.id AS forecast_id, a.agent_id
        FROM fermi_forecasts f
        CROSS JOIN LATERAL jsonb_array_elements(
               CASE WHEN jsonb_typeof(f.agents_used)='array'
                    THEN f.agents_used ELSE '[]'::jsonb END) e
        JOIN agents a
          ON a.agent_id::text = e->>'agent_id'
          OR a.agent_name     = e->>'agent_name'
          OR a.agent_name     = e->>'name'
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
    ), emitted AS (
      SELECT p.*, EXISTS (
               SELECT 1 FROM eval_signals s
                WHERE s.agent_id = p.agent_id
                  AND s.dimension = 'forecast_calibration'
                  AND s.rationale LIKE 'forecast ' || p.forecast_id || ' resolved%'
             ) AS has_signal
        FROM pairs p
    )
    SELECT count(*) FROM emitted e
     WHERE NOT e.has_signal
       AND EXISTS (SELECT 1 FROM emitted o
                    WHERE o.forecast_id = e.forecast_id AND o.has_signal)
  $q$,
  $q$
    WITH pairs AS (
      SELECT DISTINCT f.id AS forecast_id, a.agent_id, a.agent_name
        FROM fermi_forecasts f
        CROSS JOIN LATERAL jsonb_array_elements(
               CASE WHEN jsonb_typeof(f.agents_used)='array'
                    THEN f.agents_used ELSE '[]'::jsonb END) e
        JOIN agents a ON a.agent_id::text=e->>'agent_id'
                      OR a.agent_name=e->>'agent_name'
                      OR a.agent_name=e->>'name'
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
    ), emitted AS (
      SELECT p.*, EXISTS (SELECT 1 FROM eval_signals s
                           WHERE s.agent_id=p.agent_id
                             AND s.dimension='forecast_calibration'
                             AND s.rationale LIKE 'forecast '||p.forecast_id||' resolved%') AS has_signal
        FROM pairs p
    )
    SELECT string_agg(t, '; ') FROM (
      SELECT format('%s missing for %s', e.forecast_id, e.agent_name) AS t
        FROM emitted e
       WHERE NOT e.has_signal
         AND EXISTS (SELECT 1 FROM emitted o WHERE o.forecast_id=e.forecast_id AND o.has_signal)
       LIMIT 10
    ) s
  $q$,
  'PARTIAL emission: the emitter demonstrably ran for this forecast (some agents have signals) but skipped others on the same roster. Not explainable by backfill history. This is a genuine drop and means per-agent calibration is understated for the skipped agents.'
);

SELECT pg_temp.chk(
  'L5-M06', 'MECHANISM', 'HIGH',
  ARRAY['eval_signals.score','eval_signals.dimension','eval_signals.rationale',
        'fermi_forecasts.brier_score'],
  $q$
    SELECT count(*)
      FROM eval_signals s
      JOIN fermi_forecasts f
        ON s.rationale LIKE 'forecast ' || f.id || ' resolved%'
     WHERE s.dimension = 'forecast_calibration'
       AND f.brier_score IS NOT NULL
       AND abs(s.score - (1.0 - least(greatest(f.brier_score::float8, 0.0), 1.0))) > 1e-3
  $q$,
  $q$
    SELECT string_agg(t, '; ') FROM (
      SELECT format('%s: signal=%s expected=%s', f.id, round(s.score::numeric,4),
                    round((1.0 - least(greatest(f.brier_score::float8,0.0),1.0))::numeric,4)) AS t
        FROM eval_signals s
        JOIN fermi_forecasts f ON s.rationale LIKE 'forecast '||f.id||' resolved%'
       WHERE s.dimension='forecast_calibration' AND f.brier_score IS NOT NULL
         AND abs(s.score - (1.0 - least(greatest(f.brier_score::float8,0.0),1.0))) > 1e-3
       LIMIT 10
    ) s
  $q$,
  'The stored signal must equal 1 - clamp(brier). This verifies the whole transform end to end: a mismatch means the inversion, the clamp, or the forecast->signal binding is wrong, and every calibration_score derived from it is wrong by the same amount.'
);

SELECT pg_temp.chk(
  'L5-M07', 'MECHANISM', 'MEDIUM',
  ARRAY['eval_signals.dimension','eval_signals.rationale','eval_signals.evaluator_name'],
  $q$
    SELECT count(*) FROM eval_signals s
     WHERE s.dimension='forecast_calibration'
       AND s.evaluator_name = 'brier_forecast_resolver'
       AND substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})') IS NOT NULL
       AND NOT EXISTS (
         SELECT 1 FROM fermi_forecasts f
          WHERE f.id = substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})')
            AND f.status = 'resolved'
            AND f.brier_score IS NOT NULL)
  $q$,
  $q$
    SELECT string_agg(DISTINCT substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})'), '; ')
      FROM eval_signals s
     WHERE s.dimension='forecast_calibration'
       AND s.evaluator_name='brier_forecast_resolver'
       AND substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})') IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM fermi_forecasts f
                        WHERE f.id=substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})')
                          AND f.status='resolved' AND f.brier_score IS NOT NULL)
  $q$,
  'Signals citing a forecast that is not resolved-and-scored. Means a signal outlived an un-resolve, a void, or a delete — the calibration mean is then averaging over evidence that no longer exists.'
);

SELECT pg_temp.chk(
  'L5-M08', 'MECHANISM', 'MEDIUM',
  ARRAY['eval_signals.agent_id','eval_signals.dimension','eval_signals.rationale'],
  $q$
    SELECT count(*) FROM (
      SELECT s.agent_id, s.rationale
        FROM eval_signals s
       WHERE s.dimension='forecast_calibration'
         AND s.rationale LIKE 'forecast %'
       GROUP BY s.agent_id, s.rationale
      HAVING count(*) > 1
    ) x
  $q$,
  $q$
    SELECT string_agg(format('%s x%s', left(rationale,50), c), '; ') FROM (
      SELECT s.rationale, count(*) c
        FROM eval_signals s
       WHERE s.dimension='forecast_calibration' AND s.rationale LIKE 'forecast %'
       GROUP BY s.agent_id, s.rationale HAVING count(*) > 1
       ORDER BY c DESC LIMIT 10
    ) s
  $q$,
  'Duplicate (agent, forecast) signals. The emitter guards with INSERT ... WHERE NOT EXISTS on exactly this pair, so duplicates mean the guard was bypassed — and each duplicate double-weights one forecast in the calibration mean.'
);


-- ═══════════════════════════════════════════════════════════════════════
-- STAGE 4 — READ PATH: do the two readers agree
-- ═══════════════════════════════════════════════════════════════════════

SELECT pg_temp.chk(
  'L5-M09', 'MECHANISM', 'HIGH',
  ARRAY['fermi_forecasts.agents_used','agents.agent_id','agents.agent_name'],
  $q$
    WITH per_agent AS (
      SELECT a.agent_id,
             count(*) FILTER (WHERE f.agents_used @> jsonb_build_array(
                       jsonb_build_object('agent_id', a.agent_id::text))) AS via_id,
             count(*) AS via_any
        FROM agents a
        JOIN fermi_forecasts f
          ON f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
          OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
          OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name))
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
       GROUP BY a.agent_id
    )
    SELECT COALESCE(sum(via_any - via_id), 0) FROM per_agent
  $q$,
  $q$
    SELECT string_agg(t, '; ') FROM (
      SELECT format('%s: id-only=%s all-shapes=%s', a.agent_name,
                    count(*) FILTER (WHERE f.agents_used @> jsonb_build_array(
                              jsonb_build_object('agent_id', a.agent_id::text))),
                    count(*)) AS t
        FROM agents a
        JOIN fermi_forecasts f
          ON f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
          OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
          OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name))
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
       GROUP BY a.agent_id, a.agent_name
      HAVING count(*) <> count(*) FILTER (WHERE f.agents_used @> jsonb_build_array(
                                  jsonb_build_object('agent_id', a.agent_id::text)))
       ORDER BY count(*) DESC LIMIT 10
    ) s
  $q$,
  'Forecasts reachable by name but NOT by agent_id. These are invisible to any reader matching agent_id alone, which is what /calibration did before the three-shape fix. A non-zero count means mig-170''s one-shot backfill is stale — the live write path emits {"name":...} without an agent_id, so this grows with every new forecast until the write path stamps agent_id at creation.'
);


-- ═══════════════════════════════════════════════════════════════════════
-- INFO — how much does the signal currently mean?
-- ═══════════════════════════════════════════════════════════════════════
--
-- None of these are failures. They quantify thinness and skew so the score
-- can be reported honestly rather than confidently.

SELECT pg_temp.chk(
  'L5-I01', 'INFO', 'INFO',
  ARRAY['fermi_forecasts.agents_used','eval_signals.rationale','agents.agent_id'],
  $q$
    SELECT count(*) FROM fermi_forecasts f
     WHERE f.status='resolved' AND f.brier_score IS NOT NULL
       AND NOT EXISTS (
         SELECT 1 FROM eval_signals s
          WHERE s.dimension='forecast_calibration'
            AND s.rationale LIKE 'forecast ' || f.id || ' resolved%')
  $q$,
  NULL,
  'Scored forecasts with no calibration signal for ANY agent — almost certainly resolved before record_forecast_calibration_signals shipped. This is the backfill backlog, not a bug. Re-emitting these is the cheapest way to thicken Loop 5 without waiting for new resolutions.'
);

SELECT pg_temp.chk(
  'L5-I02', 'INFO', 'INFO',
  ARRAY['fermi_forecasts.status','fermi_forecasts.brier_score'],
  $q$ SELECT count(*) FROM fermi_forecasts WHERE status='resolved' AND brier_score IS NOT NULL $q$,
  NULL,
  'Total scored forecasts in the system — the ceiling on Loop 5 evidence. Per-agent confidence saturates at n=20, so treat anything below that as provisional.'
);

SELECT pg_temp.chk(
  'L5-I03', 'INFO', 'INFO',
  ARRAY['fermi_forecasts.agents_used','agents.agent_id'],
  $q$
    WITH pairs AS (
      SELECT DISTINCT f.id AS forecast_id, a.agent_id
        FROM fermi_forecasts f
        CROSS JOIN LATERAL jsonb_array_elements(
               CASE WHEN jsonb_typeof(f.agents_used)='array'
                    THEN f.agents_used ELSE '[]'::jsonb END) e
        JOIN agents a ON a.agent_id::text=e->>'agent_id'
                      OR a.agent_name=e->>'agent_name'
                      OR a.agent_name=e->>'name'
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
    ), fingerprints AS (
      SELECT agent_id,
             md5(string_agg(forecast_id, ',' ORDER BY forecast_id)) AS fp
        FROM pairs GROUP BY agent_id
    )
    SELECT count(*) FROM fingerprints f
     WHERE EXISTS (SELECT 1 FROM fingerprints o
                    WHERE o.fp = f.fp AND o.agent_id <> f.agent_id)
  $q$,
  $q$
    WITH pairs AS (
      SELECT DISTINCT f.id AS forecast_id, a.agent_id, a.agent_name
        FROM fermi_forecasts f
        CROSS JOIN LATERAL jsonb_array_elements(
               CASE WHEN jsonb_typeof(f.agents_used)='array'
                    THEN f.agents_used ELSE '[]'::jsonb END) e
        JOIN agents a ON a.agent_id::text=e->>'agent_id'
                      OR a.agent_name=e->>'agent_name'
                      OR a.agent_name=e->>'name'
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
    ), fingerprints AS (
      SELECT agent_id, agent_name, count(*) n,
             md5(string_agg(forecast_id, ',' ORDER BY forecast_id)) AS fp
        FROM pairs GROUP BY agent_id, agent_name
    )
    SELECT string_agg(t, ' | ') FROM (
      SELECT format('[n=%s] %s', n, string_agg(agent_name, ', ' ORDER BY agent_name)) AS t
        FROM fingerprints GROUP BY fp, n HAVING count(*) > 1 ORDER BY n DESC LIMIT 5
    ) s
  $q$,
  'DESIGN INTENT WARNING. Agents whose attributed forecast set is byte-identical to another agent''s. Loop 5 exists to weight moe_router_strategist routing, which requires RANKING agents against each other — and agents scored on an identical question set with identical outcomes receive identical scores forever, no matter how much data accumulates. This is structural, not thin-data: a 6-factor model that cites all 6 agents on every forecast can never discriminate between them. Discrimination needs either per-agent sub-forecasts or per-agent weights recorded at forecast time.'
);

SELECT pg_temp.chk(
  'L5-I04', 'INFO', 'INFO',
  ARRAY['fermi_forecasts.actual_outcome','fermi_forecasts.agents_used','agents.agent_id'],
  $q$
    WITH pairs AS (
      SELECT DISTINCT f.id AS forecast_id, f.actual_outcome, a.agent_id
        FROM fermi_forecasts f
        CROSS JOIN LATERAL jsonb_array_elements(
               CASE WHEN jsonb_typeof(f.agents_used)='array'
                    THEN f.agents_used ELSE '[]'::jsonb END) e
        JOIN agents a ON a.agent_id::text=e->>'agent_id'
                      OR a.agent_name=e->>'agent_name'
                      OR a.agent_name=e->>'name'
       WHERE f.status='resolved' AND f.brier_score IS NOT NULL
    )
    SELECT count(*) FROM (
      SELECT agent_id,
             avg(CASE WHEN actual_outcome THEN 1.0 ELSE 0.0 END) AS b
        FROM pairs GROUP BY agent_id
    ) x WHERE b * (1.0 - b) < 0.01
  $q$,
  NULL,
  'Agents whose outcome set is so one-sided that the base-rate baseline b(1-b) is under 0.01. On such a set a forecaster that knows nothing still scores ~99%, so the RAW calibration number is uninformative and only brier_skill_score should be read. This is exactly the World Cup tournament-winner shape (47 NO, 1 YES → baseline 0.0204).'
);


-- ═══════════════════════════════════════════════════════════════════════
-- PER-AGENT LEDGER — the numbers, with their own caveats attached
-- ═══════════════════════════════════════════════════════════════════════

\echo ''
\echo '── Per-agent Loop 5 ledger ────────────────────────────────────────────'
\echo '   n_pairs   = attributed scored forecasts'
\echo '   n_signals = forecast_calibration rows actually emitted (should equal n_pairs)'
\echo '   baseline  = b(1-b), the score a zero-knowledge base-rate forecaster gets'
\echo '   skill     = 1 - brier/baseline;  <= 0 means no better than knowing nothing'
\echo ''

WITH pairs AS (
  SELECT DISTINCT f.id AS forecast_id, f.brier_score::float8 AS brier,
         f.actual_outcome, a.agent_id, a.agent_name
    FROM fermi_forecasts f
    CROSS JOIN LATERAL jsonb_array_elements(
           CASE WHEN jsonb_typeof(f.agents_used)='array'
                THEN f.agents_used ELSE '[]'::jsonb END) e
    JOIN agents a ON a.agent_id::text=e->>'agent_id'
                  OR a.agent_name=e->>'agent_name'
                  OR a.agent_name=e->>'name'
   WHERE f.status='resolved' AND f.brier_score IS NOT NULL
), agg AS (
  SELECT p.agent_id, p.agent_name,
         count(*)                                                   AS n_pairs,
         avg(p.brier)                                               AS brier_mean,
         avg(CASE WHEN p.actual_outcome THEN 1.0 ELSE 0.0 END)       AS base_rate,
         count(*) FILTER (WHERE EXISTS (
           SELECT 1 FROM eval_signals s
            WHERE s.agent_id=p.agent_id
              AND s.dimension='forecast_calibration'
              AND s.rationale LIKE 'forecast '||p.forecast_id||' resolved%')) AS n_signals
    FROM pairs p GROUP BY p.agent_id, p.agent_name
)
SELECT agent_name,
       n_pairs,
       n_signals,
       CASE WHEN n_signals = n_pairs THEN 'ok' ELSE 'GAP' END        AS emission,
       round(brier_mean::numeric, 4)                                 AS brier,
       round((1.0 - brier_mean)::numeric, 4)                         AS raw_calib,
       round(base_rate::numeric, 4)                                  AS base_rate,
       round((base_rate * (1.0 - base_rate))::numeric, 4)            AS baseline,
       CASE WHEN base_rate * (1.0 - base_rate) > 1e-9
            THEN round((1.0 - brier_mean / (base_rate * (1.0 - base_rate)))::numeric, 3)
            END                                                      AS skill,
       CASE
         WHEN base_rate * (1.0 - base_rate) <= 1e-9 THEN 'skill undefined (one-sided outcomes)'
         WHEN 1.0 - brier_mean / (base_rate * (1.0 - base_rate)) <= 0 THEN 'NO SKILL over base rate'
         WHEN n_pairs < 5  THEN 'provisional (n<5)'
         WHEN n_pairs < 20 THEN 'thin (n<20)'
         ELSE 'usable'
       END                                                           AS verdict
  FROM agg
 ORDER BY n_pairs DESC, agent_name
 LIMIT 50;


-- ═══════════════════════════════════════════════════════════════════════
-- MECHANISM ATTRIBUTION — whose wiring is broken?
-- ═══════════════════════════════════════════════════════════════════════
--
-- The MECHANISM findings above are fleet-wide counts. That is the right
-- scope for "is the platform's Loop 5 sound", and the wrong scope for the
-- question an agent owner asks: "is MY loop broken, or just young?" A single
-- orphaned forecast belonging to one tenant makes the fleet verdict BROKEN,
-- and read off an agent's own dashboard that is an unattributable accusation.
--
-- So the eight *scopable* MECHANISM checks are repeated here per agent. Each
-- block below is the same predicate as its fleet counterpart with the roster
-- filter added, and is the SQL twin of `LOOP5_AGENT_CHECKS` in
-- src/handlers/observatory.rs.
--
-- L5-M03 IS ABSENT ON PURPOSE. It counts scored forecasts attributable to NO
-- agent; being unattributable is the definition of that fault, so it cannot
-- be filed under anybody. It stays fleet-only, and the Rust side declares it
-- in `LOOP5_UNSCOPABLE` so a per-agent "all clean" does not overclaim.
--
-- ⚠ THREE COPIES, ONE CONTRACT. The check ids here, in
--   `LOOP5_MECHANISM_CHECKS` and in `LOOP5_AGENT_CHECKS` must stay identical.
--   The Rust pair is enforced by `agent_and_fleet_checks_declare_the_same_ids`;
--   this file is enforced by the ids being greppable across all three. If you
--   add, remove or re-scope a check, change all three in the same commit.

\echo ''
\echo '── MECHANISM violations by agent ──────────────────────────────────────'
\echo '   Empty result = no agent-attributable wiring fault.'
\echo '   L5-M03 is fleet-only and cannot appear here (see comment in file).'
\echo ''

WITH roster AS (
  -- The three-shape join, once. Mirrors ROSTER_PREDICATE in observatory.rs and
  -- eval_brier.rs::latest_for_agent.
  SELECT DISTINCT f.id AS forecast_id, a.agent_id, a.agent_name
    FROM fermi_forecasts f
    CROSS JOIN LATERAL jsonb_array_elements(
           CASE WHEN jsonb_typeof(f.agents_used)='array'
                THEN f.agents_used ELSE '[]'::jsonb END) e
    JOIN agents a ON a.agent_id::text = e->>'agent_id'
                  OR a.agent_name     = e->>'agent_name'
                  OR a.agent_name     = e->>'name'
), emitted AS (
  SELECT r.*, EXISTS (
           SELECT 1 FROM eval_signals s
            WHERE s.agent_id = r.agent_id
              AND s.dimension = 'forecast_calibration'
              AND s.rationale LIKE 'forecast ' || r.forecast_id || ' resolved%'
         ) AS has_signal
    FROM roster r
   WHERE EXISTS (SELECT 1 FROM fermi_forecasts f
                  WHERE f.id = r.forecast_id
                    AND f.status='resolved' AND f.brier_score IS NOT NULL)
), findings AS (
  -- L5-M01 — resolved with an outcome but never scored
  SELECT r.agent_name, 'L5-M01' AS check_id, 'CRITICAL' AS severity, count(*) AS violations
    FROM roster r JOIN fermi_forecasts f ON f.id = r.forecast_id
   WHERE f.status='resolved' AND f.actual_outcome IS NOT NULL AND f.brier_score IS NULL
   GROUP BY r.agent_name

  UNION ALL
  -- L5-M02 — brier not reproducible from the frozen pair
  SELECT r.agent_name, 'L5-M02', 'CRITICAL', count(*)
    FROM roster r JOIN fermi_forecasts f ON f.id = r.forecast_id
   WHERE f.status='resolved' AND f.brier_score IS NOT NULL AND f.scored_probability IS NOT NULL
     AND abs(f.brier_score::float8
             - power(f.scored_probability::float8
                     - (CASE WHEN f.actual_outcome THEN 1.0 ELSE 0.0 END), 2)) > 1e-4
   GROUP BY r.agent_name

  UNION ALL
  -- L5-M04 — a forecast this agent is on also names an agent that does not exist
  SELECT r.agent_name, 'L5-M04', 'HIGH', count(*)
    FROM roster r
    JOIN fermi_forecasts f ON f.id = r.forecast_id
    CROSS JOIN LATERAL jsonb_array_elements(
           CASE WHEN jsonb_typeof(f.agents_used)='array'
                THEN f.agents_used ELSE '[]'::jsonb END) e
   WHERE f.status='resolved' AND f.brier_score IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM agents a
                      WHERE a.agent_id::text = e->>'agent_id'
                         OR a.agent_name     = e->>'agent_name'
                         OR a.agent_name     = e->>'name')
   GROUP BY r.agent_name

  UNION ALL
  -- L5-M05 — PARTIAL emission: others on this forecast got a signal, this agent did not
  SELECT e.agent_name, 'L5-M05', 'CRITICAL', count(*)
    FROM emitted e
   WHERE NOT e.has_signal
     AND EXISTS (SELECT 1 FROM emitted o
                  WHERE o.forecast_id = e.forecast_id AND o.has_signal)
   GROUP BY e.agent_name

  UNION ALL
  -- L5-M06 — stored score is not 1 - clamp(brier)
  SELECT a.agent_name, 'L5-M06', 'HIGH', count(*)
    FROM eval_signals s
    JOIN agents a ON a.agent_id = s.agent_id
    JOIN fermi_forecasts f ON s.rationale LIKE 'forecast ' || f.id || ' resolved%'
   WHERE s.dimension='forecast_calibration' AND f.brier_score IS NOT NULL
     AND abs(s.score - (1.0 - least(greatest(f.brier_score::float8,0.0),1.0))) > 1e-3
   GROUP BY a.agent_name

  UNION ALL
  -- L5-M07 — signal outlived the forecast it cites
  SELECT a.agent_name, 'L5-M07', 'MEDIUM', count(*)
    FROM eval_signals s
    JOIN agents a ON a.agent_id = s.agent_id
   WHERE s.dimension='forecast_calibration'
     AND s.evaluator_name='brier_forecast_resolver'
     AND substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})') IS NOT NULL
     AND NOT EXISTS (
       SELECT 1 FROM fermi_forecasts f
        WHERE f.id = substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})')
          AND f.status='resolved' AND f.brier_score IS NOT NULL)
   GROUP BY a.agent_name

  UNION ALL
  -- L5-M08 — duplicate (agent, forecast) signals
  SELECT a.agent_name, 'L5-M08', 'MEDIUM', count(*)
    FROM (
      SELECT s.agent_id, s.rationale
        FROM eval_signals s
       WHERE s.dimension='forecast_calibration' AND s.rationale LIKE 'forecast %'
       GROUP BY s.agent_id, s.rationale
      HAVING count(*) > 1
    ) d
    JOIN agents a ON a.agent_id = d.agent_id
   GROUP BY a.agent_name

  UNION ALL
  -- L5-M09 — reachable by name but not by agent_id
  SELECT a.agent_name, 'L5-M09', 'HIGH', count(*)
    FROM agents a
    JOIN fermi_forecasts f
      ON (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
       OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name)))
   WHERE f.status='resolved' AND f.brier_score IS NOT NULL
     AND NOT (f.agents_used @> jsonb_build_array(
                jsonb_build_object('agent_id', a.agent_id::text)))
   GROUP BY a.agent_name
)
SELECT agent_name, check_id, severity, violations
  FROM findings
 WHERE violations > 0
 ORDER BY CASE severity WHEN 'CRITICAL' THEN 0 WHEN 'HIGH' THEN 1 ELSE 2 END,
          violations DESC, agent_name, check_id;


-- ═══════════════════════════════════════════════════════════════════════
-- SUMMARY
-- ═══════════════════════════════════════════════════════════════════════

\echo ''
\echo '── Findings ───────────────────────────────────────────────────────────'

SELECT check_id, class, severity, status,
       violations AS count,
       detail
  FROM loop5_findings
 ORDER BY class DESC, seq;

\echo ''
\echo '── Verdict ────────────────────────────────────────────────────────────'

SELECT
    count(*) FILTER (WHERE class='MECHANISM' AND status='VIOLATION') AS mechanism_violations,
    count(*) FILTER (WHERE class='MECHANISM' AND status='OK')        AS mechanism_ok,
    count(*) FILTER (WHERE status='SKIPPED')                        AS skipped,
    count(*) FILTER (WHERE status='ERROR')                          AS errored,
    CASE
      WHEN count(*) FILTER (WHERE status='ERROR') > 0
        THEN 'INCONCLUSIVE — a check errored; fix the query before drawing conclusions'
      WHEN count(*) FILTER (WHERE class='MECHANISM' AND status='VIOLATION') = 0
        THEN 'MECHANISM SOUND — the chain moves signals correctly. Any weakness in the numbers is thin data or skew (see INFO), not wiring.'
      ELSE 'MECHANISM BROKEN — do not trust any Loop 5 number until the MECHANISM violations above are resolved'
    END AS verdict
  FROM loop5_findings;
