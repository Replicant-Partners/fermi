-- ═══════════════════════════════════════════════════════════════════════
-- Loop 1 maturity census — the CORRECTED banding — READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- Mirrors `classify_maturity` in src/handlers/dreaming_maturity.rs exactly,
-- so the true fault count can be measured against production before the code
-- ships. KEEP THE TWO IN SYNC — if the Rust bands change, change these CASEs.
--
-- The correction this measures: an agent with completed cycles and an empty
-- ontology used to be reported as `unproductive` (a fault). That conflated two
-- situations. An agent with NO EPISODES had nothing to consolidate, so an empty
-- ontology is the correct outcome, not a failure. Only an agent that
-- consolidated real episodes and still extracted nothing is broken.
--
-- Banding (in order, first match wins):
--   dormant      : no completed cycles
--   unused       : empty ontology AND zero episodes      -> idle, not a fault
--   unproductive : empty ontology WITH episodes          -> REAL FAULT
--   mature       : >=3 cycles, ontology >=25, rules >0
--   developing   : otherwise
--
-- Run: PROBE_FILE=scripts/loop1_maturity_census.sql scripts/run_loop5_probe.sh
-- ═══════════════════════════════════════════════════════════════════════

CREATE TEMP VIEW loop1_census AS
SELECT a.agent_id,
       a.agent_name,
       a.agent_name ~ '^test_agent_' AS is_test_cruft,
       (SELECT count(*) FROM episodes e WHERE e.agent_id = a.agent_id) AS episodes,
       (SELECT count(*) FROM consolidation_jobs j
         WHERE j.agent_id = a.agent_id AND j.status = 'completed') AS completed,
       (SELECT count(*) FROM consolidation_jobs j
         WHERE j.agent_id = a.agent_id AND j.status = 'failed') AS failed,
       (SELECT count(*) FROM entities x       WHERE x.agent_id = a.agent_id) AS entities,
       (SELECT count(*) FROM facts x          WHERE x.agent_id = a.agent_id) AS facts,
       (SELECT count(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS rules
  FROM agents a
 WHERE a.status <> 'archived';

CREATE TEMP VIEW loop1_banded AS
SELECT c.*,
       (c.entities + c.facts + c.rules) AS onto,
       CASE
         WHEN c.completed = 0                                        THEN 'dormant'
         WHEN (c.entities + c.facts + c.rules) = 0 AND c.episodes = 0 THEN 'unused'
         WHEN (c.entities + c.facts + c.rules) = 0                    THEN 'unproductive'
         WHEN c.completed >= 3
          AND (c.entities + c.facts + c.rules) >= 25
          AND c.rules > 0                                            THEN 'mature'
         ELSE                                                             'developing'
       END AS band
  FROM loop1_census c;

\echo ''
\echo '── 1. Census under the CORRECTED banding ───────────────────────────────'
SELECT band,
       count(*) AS agents,
       count(*) FILTER (WHERE is_test_cruft) AS of_which_test_cruft,
       sum(episodes) AS episodes,
       sum(onto) AS ontology_rows
  FROM loop1_banded
 GROUP BY band
 ORDER BY CASE band WHEN 'unproductive' THEN 1 WHEN 'developing' THEN 2
                    WHEN 'mature' THEN 3 WHEN 'unused' THEN 4 ELSE 5 END;

\echo ''
\echo '── 2. Before vs after: how many agents were mislabelled as broken? ─────'
SELECT count(*) FILTER (WHERE onto = 0 AND completed > 0)                  AS old_unproductive_count,
       count(*) FILTER (WHERE band = 'unproductive')                       AS true_unproductive_count,
       count(*) FILTER (WHERE band = 'unused')                             AS reclassified_as_unused
  FROM loop1_banded;

\echo ''
\echo '── 3. THE REAL FAULTS: consolidated real episodes, learned nothing ─────'
\echo '   These are the agents actually worth spending time on.'
SELECT agent_name, episodes, completed, failed, entities, facts, rules,
       CASE WHEN is_test_cruft THEN 'test cruft' ELSE '' END AS note
  FROM loop1_banded
 WHERE band = 'unproductive'
 ORDER BY episodes DESC, agent_name
 LIMIT 30;

\echo ''
\echo '── 4. Real faults excluding test cruft ─────────────────────────────────'
SELECT count(*) AS genuine_unproductive_agents,
       COALESCE(sum(episodes), 0) AS wasted_episodes
  FROM loop1_banded
 WHERE band = 'unproductive' AND NOT is_test_cruft;

\echo ''
\echo '── 5. The healthy end, for scale ───────────────────────────────────────'
SELECT band, count(*) AS agents,
       round(avg(onto)::numeric, 1) AS avg_ontology,
       max(onto) AS largest_ontology
  FROM loop1_banded
 WHERE band IN ('mature', 'developing')
 GROUP BY band;
