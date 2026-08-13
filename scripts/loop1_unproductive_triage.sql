-- ═══════════════════════════════════════════════════════════════════════
-- Triage: why did 88 agents complete consolidation cycles and learn nothing?
-- READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- The maturity classifier calls them "unproductive": cycles completed, empty
-- ontology. But that label bundles two very different situations, and only one
-- of them is a fault:
--
--   NOTHING TO LEARN FROM — the agent was never really executed, so there were
--     no episodes to cluster. Consolidation had nothing to do and correctly did
--     nothing. Not a bug; the agent is simply unused.
--
--   EXTRACTION FAILED — episodes existed and were processed, clusters were
--     found, but the ontologist produced no durable entities/facts/rules. This
--     is a real fault and worth credits to investigate.
--
-- Telling them apart needs `episodes_processed` on the job rows, not just the
-- job count. A third possibility this also surfaces: the completed jobs may be
-- SEEDED fixture rows (memory/src/seed.rs builds consolidation_jobs), in which
-- case no consolidation ever actually ran for those agents at all.
--
-- Run: PROBE_FILE=scripts/loop1_unproductive_triage.sql scripts/run_loop5_probe.sh
-- ═══════════════════════════════════════════════════════════════════════

\echo ''
\echo '── 1. The 88, bucketed by cause ────────────────────────────────────────'
\echo '   no_episodes_ever      = never executed; nothing to consolidate'
\echo '   episodes_but_none_fed = episodes exist, jobs processed 0 of them'
\echo '   fed_but_no_clusters   = episodes processed, DBSCAN found no clusters'
\echo '   clustered_but_no_rules= clusters found, ontologist extracted nothing  <- real fault'
WITH unproductive AS (
  SELECT a.agent_id, a.agent_name, a.total_executions,
         (SELECT count(*) FROM consolidation_jobs j
           WHERE j.agent_id = a.agent_id AND j.status = 'completed') AS completed,
         (SELECT COALESCE(sum(j.episodes_processed), 0) FROM consolidation_jobs j
           WHERE j.agent_id = a.agent_id) AS eps_processed,
         (SELECT COALESCE(sum(j.clusters_identified), 0) FROM consolidation_jobs j
           WHERE j.agent_id = a.agent_id) AS clusters,
         (SELECT count(*) FROM episodes e WHERE e.agent_id = a.agent_id) AS episodes,
         (SELECT count(*) FROM entities x       WHERE x.agent_id = a.agent_id)
       + (SELECT count(*) FROM facts x          WHERE x.agent_id = a.agent_id)
       + (SELECT count(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS onto
    FROM agents a
   WHERE a.status <> 'archived'
)
SELECT CASE
         WHEN episodes = 0                      THEN '1. no_episodes_ever'
         WHEN eps_processed = 0                 THEN '2. episodes_but_none_fed'
         WHEN clusters = 0                      THEN '3. fed_but_no_clusters'
         ELSE                                        '4. clustered_but_no_rules'
       END AS bucket,
       count(*) AS agents,
       sum(episodes) AS total_episodes,
       sum(eps_processed) AS total_eps_processed,
       sum(clusters) AS total_clusters,
       sum(total_executions) AS total_executions
  FROM unproductive
 WHERE completed > 0 AND onto = 0
 GROUP BY 1
 ORDER BY 1;

\echo ''
\echo '── 2. Were those completed jobs real, or seeded fixtures? ──────────────'
\echo '   Real runs have a start/finish spread and non-zero episodes_processed.'
\echo '   Seeded rows tend to cluster on one timestamp with zero work recorded.'
SELECT date_trunc('day', started_at)::date AS day,
       count(*) AS jobs,
       count(*) FILTER (WHERE episodes_processed = 0) AS zero_episode_jobs,
       count(DISTINCT agent_id) AS agents,
       round(avg(episodes_processed)::numeric, 2) AS avg_eps,
       count(*) FILTER (WHERE completed_at IS NULL) AS never_completed
  FROM consolidation_jobs
 GROUP BY 1
 ORDER BY 1 DESC
 LIMIT 12;

\echo ''
\echo '── 3. Sample of the real-fault bucket (clusters found, nothing extracted)'
WITH u AS (
  SELECT a.agent_id, a.agent_name,
         (SELECT COALESCE(sum(j.episodes_processed),0) FROM consolidation_jobs j WHERE j.agent_id=a.agent_id) AS eps,
         (SELECT COALESCE(sum(j.clusters_identified),0) FROM consolidation_jobs j WHERE j.agent_id=a.agent_id) AS cl,
         (SELECT COALESCE(sum(j.rules_extracted),0)    FROM consolidation_jobs j WHERE j.agent_id=a.agent_id) AS rx,
         (SELECT COALESCE(sum(j.rules_rejected),0)     FROM consolidation_jobs j WHERE j.agent_id=a.agent_id) AS rj,
         (SELECT count(*) FROM entities x       WHERE x.agent_id=a.agent_id)
       + (SELECT count(*) FROM facts x          WHERE x.agent_id=a.agent_id)
       + (SELECT count(*) FROM semantic_rules x WHERE x.agent_id=a.agent_id) AS onto
    FROM agents a WHERE a.status <> 'archived'
)
SELECT agent_name, eps AS episodes_processed, cl AS clusters,
       rx AS rules_extracted, rj AS rules_rejected
  FROM u
 WHERE onto = 0 AND cl > 0
 ORDER BY cl DESC
 LIMIT 10;

\echo ''
\echo '── 4. For contrast: what a productive agent looks like ─────────────────'
WITH p AS (
  SELECT a.agent_name,
         (SELECT COALESCE(sum(j.episodes_processed),0) FROM consolidation_jobs j WHERE j.agent_id=a.agent_id) AS eps,
         (SELECT COALESCE(sum(j.clusters_identified),0) FROM consolidation_jobs j WHERE j.agent_id=a.agent_id) AS cl,
         (SELECT count(*) FROM entities x WHERE x.agent_id=a.agent_id)
       + (SELECT count(*) FROM facts x    WHERE x.agent_id=a.agent_id)
       + (SELECT count(*) FROM semantic_rules x WHERE x.agent_id=a.agent_id) AS onto
    FROM agents a WHERE a.status <> 'archived'
)
SELECT agent_name, eps AS episodes_processed, cl AS clusters, onto AS ontology_size
  FROM p WHERE onto > 0 ORDER BY onto DESC LIMIT 8;

\echo ''
\echo '── 5. Episode supply across the fleet ──────────────────────────────────'
\echo '   If most agents have zero episodes, the bottleneck is execution, not'
\echo '   consolidation: nothing has happened for them to dream about.'
SELECT count(*) FILTER (WHERE eps = 0)              AS agents_with_no_episodes,
       count(*) FILTER (WHERE eps BETWEEN 1 AND 4)  AS agents_1_to_4,
       count(*) FILTER (WHERE eps >= 5)             AS agents_5_plus,
       count(*)                                     AS agents_total
  FROM (SELECT a.agent_id,
               (SELECT count(*) FROM episodes e WHERE e.agent_id = a.agent_id) AS eps
          FROM agents a WHERE a.status <> 'archived') s;
