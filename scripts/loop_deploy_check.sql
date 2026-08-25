-- ═══════════════════════════════════════════════════════════════════════
-- Post-deploy loop verification — READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- Answers three questions the Loop 5 probe does not:
--
--   1. Did the new migrations actually apply? `run_migrations` logs a warning
--      and boots anyway on failure, so a missing table is silent until an
--      endpoint 500s or a dashboard card goes quietly blank.
--   2. Which forecasts are unattributable, and why?
--   3. Is Loop 1 (dreaming) recording jobs correctly, or leaving orphans?
--
-- Run:  PROBE_FILE=scripts/loop_deploy_check.sql scripts/run_loop5_probe.sh
-- ═══════════════════════════════════════════════════════════════════════

\echo ''
\echo '── 1. Migration state: do the new tables exist? ────────────────────────'
SELECT t.name AS expected_table,
       CASE WHEN to_regclass('public.' || t.name) IS NOT NULL
            THEN 'present' ELSE 'MISSING' END AS status,
       t.introduced_by
  FROM (VALUES
          ('forecast_agent_claims',       'mig-187'),
          ('forecast_attributions',       'mig-188'),
          ('forecast_agent_credit',       'mig-188'),
          ('forecast_agent_interactions', 'mig-188')
       ) AS t(name, introduced_by);

\echo ''
\echo '── 2. The unattributable forecasts (Loop 5.A / L5-M03) ─────────────────'
\echo '   A scored forecast with an empty roster: the Brier exists but no agent'
\echo '   can ever be credited for it. Not recoverable retrospectively unless we'
\echo '   know who contributed.'
SELECT f.id,
       left(COALESCE(f.question_text, '(none)'), 58) AS question,
       f.domain,
       f.resolution_source,
       f.resolved_at::date AS resolved,
       round(f.brier_score::numeric, 4) AS brier
  FROM fermi_forecasts f
 WHERE f.status = 'resolved'
   AND f.brier_score IS NOT NULL
   AND NOT EXISTS (
     SELECT 1
       FROM jsonb_array_elements(
              CASE WHEN jsonb_typeof(f.agents_used) = 'array'
                   THEN f.agents_used ELSE '[]'::jsonb END) e
       JOIN agents a ON a.agent_id::text = e->>'agent_id'
                     OR a.agent_name     = e->>'agent_name'
                     OR a.agent_name     = e->>'name')
 ORDER BY f.resolved_at DESC NULLS LAST;

\echo ''
\echo '── 3. Loop 1: consolidation job health ─────────────────────────────────'
\echo '   `running` rows older than an hour are orphans: a worker died, or the'
\echo '   pre-fix handler updated a job id that never existed so the real row'
\echo '   was never completed.'
SELECT status,
       count(*) AS jobs,
       count(*) FILTER (WHERE started_at < NOW() - interval '1 hour'
                          AND completed_at IS NULL) AS stale_over_1h,
       max(started_at)::date AS most_recent
  FROM consolidation_jobs
 GROUP BY status
 ORDER BY jobs DESC;

\echo ''
\echo '── 4. Loop 1: did dreaming actually teach anything? ────────────────────'
\echo '   cycles = machinery ran.  ontology = what persisted.  A high cycle'
\echo '   count with an empty ontology is a loop running and learning nothing.'
SELECT a.agent_name,
       (SELECT count(*) FROM consolidation_jobs j
         WHERE j.agent_id = a.agent_id AND j.status = 'completed') AS completed,
       (SELECT count(*) FROM consolidation_jobs j
         WHERE j.agent_id = a.agent_id AND j.status = 'failed') AS failed,
       (SELECT count(*) FROM entities x       WHERE x.agent_id = a.agent_id) AS entities,
       (SELECT count(*) FROM facts x          WHERE x.agent_id = a.agent_id) AS facts,
       (SELECT count(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS rules,
       a.last_consolidated_at::date AS last_dreamt
  FROM agents a
 WHERE EXISTS (SELECT 1 FROM consolidation_jobs j WHERE j.agent_id = a.agent_id)
 ORDER BY completed DESC, a.agent_name
 LIMIT 15;

\echo ''
\echo '── 5. Fleet-wide dreaming summary ──────────────────────────────────────'
SELECT count(*) FILTER (WHERE completed = 0)                        AS never_completed_a_cycle,
       count(*) FILTER (WHERE completed > 0 AND onto = 0)           AS ran_but_learned_nothing,
       count(*) FILTER (WHERE completed > 0 AND onto > 0)           AS productive
  FROM (
    SELECT a.agent_id,
           (SELECT count(*) FROM consolidation_jobs j
             WHERE j.agent_id = a.agent_id AND j.status = 'completed') AS completed,
           (SELECT count(*) FROM entities x       WHERE x.agent_id = a.agent_id)
         + (SELECT count(*) FROM facts x          WHERE x.agent_id = a.agent_id)
         + (SELECT count(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS onto
      FROM agents a
     WHERE a.status <> 'archived'
  ) s;
