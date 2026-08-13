-- ═══════════════════════════════════════════════════════════════════════
-- Loop 1 root cause: why do 62 agents consolidate real episodes and extract
-- exactly nothing? READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- The distribution is the clue. Across 62 agents and 1035 episodes the yield is
-- entities=0, facts=0, rules=0 for every single one. A content problem (episodes
-- too thin to cluster, prompts too vague) would produce a gradient — some agents
-- scraping a few entities, others none. A clean binary across the whole set
-- points at a shared dependency that was absent.
--
-- HYPOTHESIS
-- ----------
-- `consolidate_agent_handler` builds the worker two ways
-- (src/handlers/consolidation.rs):
--
--     match build_extraction_llm(&spawn_state).await {
--         Some(llm) => ConsolidationWorker::with_llm(...),   // extracts
--         None      => ConsolidationWorker::new(...),        // cannot extract
--     }
--
-- With no extraction LLM the cycle still runs, still marks episodes
-- consolidated, still completes the job, still debits a dreaming credit — and
-- produces no entities, facts or rules. It reports success and learns nothing.
--
-- If that is what happened, the unproductive runs should cluster in time
-- (before the ontologist was funded/configured) and the productive ones should
-- be recent. That is what this checks.
--
-- Run: PROBE_FILE=scripts/loop1_root_cause.sql scripts/run_loop5_probe.sh
-- ═══════════════════════════════════════════════════════════════════════

CREATE TEMP VIEW banded AS
SELECT a.agent_id, a.agent_name,
       (SELECT count(*) FROM episodes e WHERE e.agent_id = a.agent_id) AS episodes,
       (SELECT count(*) FROM consolidation_jobs j
         WHERE j.agent_id = a.agent_id AND j.status = 'completed') AS completed,
       (SELECT count(*) FROM entities x       WHERE x.agent_id = a.agent_id)
     + (SELECT count(*) FROM facts x          WHERE x.agent_id = a.agent_id)
     + (SELECT count(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS onto
  FROM agents a WHERE a.status <> 'archived';

\echo ''
\echo '── 1. WHEN did productive vs unproductive cycles run? ──────────────────'
\echo '   If unproductive runs are old and productive ones recent, the'
\echo '   difference is environmental (extraction LLM), not per-agent content.'
SELECT CASE WHEN b.onto > 0 THEN 'productive' ELSE 'unproductive' END AS kind,
       count(DISTINCT b.agent_id) AS agents,
       count(j.job_id)            AS jobs,
       min(j.started_at)::date    AS first_run,
       max(j.started_at)::date    AS last_run,
       round(avg(j.episodes_processed)::numeric, 1) AS avg_eps_processed,
       sum(j.rules_extracted)     AS rules_extracted,
       sum(j.entities_created)    AS entities_created
  FROM banded b
  JOIN consolidation_jobs j ON j.agent_id = b.agent_id AND j.status = 'completed'
 WHERE b.completed > 0 AND b.episodes > 0
 GROUP BY 1;

\echo ''
\echo '── 2. Yield by run date \u2014 the smoking gun if it is environmental ──────'
\echo '   entities_created/rules_extracted are recorded ON THE JOB ROW, so this'
\echo '   shows what each run actually produced at the time it ran.'
SELECT date_trunc('day', j.started_at)::date AS run_day,
       count(*) AS jobs,
       count(DISTINCT j.agent_id) AS agents,
       sum(j.episodes_processed)  AS eps_processed,
       sum(j.entities_created)    AS entities,
       sum(j.facts_created)       AS facts,
       sum(j.rules_extracted)     AS rules,
       count(*) FILTER (WHERE j.entities_created = 0
                          AND j.facts_created = 0
                          AND j.rules_extracted = 0
                          AND j.episodes_processed > 0) AS zero_yield_despite_work
  FROM consolidation_jobs j
 WHERE j.status = 'completed'
 GROUP BY 1
 ORDER BY 1;

\echo ''
\echo '── 3. Did ANY agent ever produce yield on an early run? ────────────────'
\echo '   A single early success would falsify the "extraction was unavailable"'
\echo '   hypothesis and point back at per-agent content instead.'
SELECT date_trunc('month', j.started_at)::date AS month,
       count(*) FILTER (WHERE j.entities_created > 0
                           OR j.facts_created > 0
                           OR j.rules_extracted > 0) AS jobs_with_yield,
       count(*) AS jobs_total
  FROM consolidation_jobs j
 WHERE j.status = 'completed' AND j.episodes_processed > 0
 GROUP BY 1 ORDER BY 1;

\echo ''
\echo '── 4. Are the unproductive agents'' episodes still available to retry? ──'
\echo '   Consolidation marks episodes consolidated whether or not anything was'
\echo '   extracted, so a failed extraction leaves them ineligible for a rerun.'
\echo '   If so, the fix is not just "re-run dreaming" \u2014 the episodes need'
\echo '   resetting first.'
SELECT count(DISTINCT b.agent_id) AS unproductive_agents,
       count(e.episode_id)        AS episodes_total,
       count(e.episode_id) FILTER (WHERE e.consolidated)     AS already_consolidated,
       count(e.episode_id) FILTER (WHERE NOT e.consolidated) AS still_eligible
  FROM banded b
  JOIN episodes e ON e.agent_id = b.agent_id
 WHERE b.onto = 0 AND b.completed > 0 AND b.episodes > 0;
