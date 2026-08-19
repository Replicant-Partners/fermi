-- ═══════════════════════════════════════════════════════════════════════
-- Loop 1: is the ontologist's output being THROWN AWAY? READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- The failure this exists to catch is not "the extractor never ran" and not
-- "the extractor produced nothing". It is the third, invisible case: the model
-- ran, answered correctly, and the response was discarded at the JSON parse.
--
--   External API error: Failed to parse structured output: expected value at
--   line 1 column 1. Response was: ```json [ {"name": "Asilidae", ...
--
-- `gpt-4o-mini` wraps JSON in a markdown fence whenever it feels like it.
-- `generate_structured_with_usage` called `serde_json::from_str` on the raw
-- content, which fails at line 1 column 1 because line 1 column 1 is a
-- backtick. Every consolidation extractor funnels through that one function,
-- and every caller treats a parse failure as non-fatal — it logs, continues,
-- and the cycle completes reporting success. So a cosmetic formatting habit
-- presented as "dreaming ran and extracted 0 entities", fleet-wide.
--
-- The entities were in the error message the whole time.
--
-- Fixed by `parse_lenient` in agent-bestiary/memory/src/llm.rs (fence-tolerant
-- with a strict-first fast path). This probe is the ongoing monitor: a non-zero
-- parse_failures count after that fix means a NEW shape of disobedience, and
-- the salvage path needs widening again.
--
-- Run: PROBE_FILE=scripts/loop1_extraction_parse_failures.sql scripts/run_loop5_probe.sh
-- ═══════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on

\echo ''
\echo '── 1. Extraction calls by outcome, per week ────────────────────────────'
\echo '   parse_failures should be 0. Anything else is output being discarded.'
SELECT date_trunc('week', e.created_at)::date AS week,
       count(*) AS extraction_calls,
       count(*) FILTER (WHERE e.error_details LIKE '%Failed to parse structured output%')
         AS parse_failures,
       count(*) FILTER (WHERE e.error_details IS NOT NULL
                          AND e.error_details NOT LIKE '%Failed to parse structured output%')
         AS other_errors,
       count(*) FILTER (WHERE e.error_details IS NULL) AS clean
  FROM episodes e
  JOIN agents a ON a.agent_id = e.agent_id
 WHERE a.agent_name = 'ontologist'
 GROUP BY 1
 ORDER BY 1 DESC
 LIMIT 12;

\echo ''
\echo '── 2. Which extraction role loses its output most? ─────────────────────'
SELECT CASE
         WHEN e.query LIKE 'Extract named entities%'      THEN 'entities'
         WHEN e.query LIKE 'Analyze these%'               THEN 'knowledge_rules'
         WHEN e.query LIKE 'Extract entities, facts%'     THEN 'cycle_summary'
         ELSE 'other'
       END AS role,
       count(*) AS calls,
       count(*) FILTER (WHERE e.error_details LIKE '%Failed to parse structured output%')
         AS parse_failures
  FROM episodes e
  JOIN agents a ON a.agent_id = e.agent_id
 WHERE a.agent_name = 'ontologist'
 GROUP BY 1
 ORDER BY parse_failures DESC;

\echo ''
\echo '── 3. Proof the model was answering correctly all along ────────────────'
\echo '   These are discarded responses. If they contain well-formed JSON,'
\echo '   the extraction worked and only the parse failed.'
SELECT e.created_at,
       left(replace(e.error_details, E'\n', ' '), 150) AS discarded_response
  FROM episodes e
  JOIN agents a ON a.agent_id = e.agent_id
 WHERE a.agent_name = 'ontologist'
   AND e.error_details LIKE '%Failed to parse structured output%'
 ORDER BY e.created_at DESC
 LIMIT 10;

\echo ''
\echo '── 4. Blast radius: agents left with an empty ontology ─────────────────'
\echo '   Episodes are marked consolidated whether or not anything was'
\echo '   extracted, so these need scripts/loop1_reset_unlearned_episodes.sql'
\echo '   before a re-run can rebuild anything.'
SELECT count(*) AS unproductive_agents,
       sum((SELECT count(*) FROM episodes e WHERE e.agent_id = a.agent_id)) AS episodes_to_recover
  FROM agents a
 WHERE a.status <> 'archived'
   AND EXISTS (SELECT 1 FROM consolidation_jobs j
                WHERE j.agent_id = a.agent_id AND j.status = 'completed')
   AND EXISTS (SELECT 1 FROM episodes e WHERE e.agent_id = a.agent_id)
   AND NOT EXISTS (SELECT 1 FROM entities x       WHERE x.agent_id = a.agent_id)
   AND NOT EXISTS (SELECT 1 FROM facts x          WHERE x.agent_id = a.agent_id)
   AND NOT EXISTS (SELECT 1 FROM semantic_rules x WHERE x.agent_id = a.agent_id);

\echo ''
\echo '── 5. Secondary limiter: episodes with no retained response ────────────'
\echo '   episode_digest omits the Response entirely when response_text is'
\echo '   NULL (pre mig-199), leaving Query + a 200-char context preview as'
\echo '   the whole extraction prompt. Even a working parser learns little'
\echo '   from those.'
SELECT count(*)                                     AS episodes,
       count(e.response_text)                       AS with_response,
       round(100.0 * count(e.response_text) / NULLIF(count(*), 0), 1) AS pct_with_response
  FROM episodes e
  JOIN agents a ON a.agent_id = e.agent_id
 WHERE a.tier <> 'system';
