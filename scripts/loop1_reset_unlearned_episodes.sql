-- ═══════════════════════════════════════════════════════════════════════
-- Recover episodes consumed by extractor-less consolidation runs
--
--   DRY RUN by default.  Pass -v apply=1 to actually write.
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT HAPPENED
-- -------------
-- Two batch consolidation runs (2026-05-16, 2026-06-22) executed without an
-- extraction LLM. The facts and semantic-rules paths are both gated on that
-- LLM, so those runs could not learn anything — but the worker marked every
-- episode consolidated regardless. Result: 62 agents, 1,035 episodes marked
-- consumed, empty ontologies, and nothing eligible for a retry, because
-- `get_unconsolidated_episodes` correctly returns none.
--
-- The code fixes stop this recurring (the handler now refuses to run without an
-- extractor, and the worker no longer consumes episodes when it has none). This
-- script recovers the material already lost to it.
--
-- WHAT IT DOES
-- ------------
-- Sets `consolidated = false` on episodes belonging to agents that satisfy ALL
-- of:
--   * at least one COMPLETED consolidation job   (dreaming genuinely ran)
--   * a completely empty ontology                (it learned nothing)
--   * at least one episode                       (there was material to learn from)
--
-- That triple is deliberately narrow. It cannot touch an agent that learned
-- anything at all, and it cannot touch an agent that never ran.
--
-- WHAT IT DOES NOT DO
-- -------------------
-- Nothing is deleted. Entities, facts, rules and job history are untouched;
-- only the `consolidated` flag on episodes is flipped back, which makes them
-- eligible for consolidation again. Re-running dreaming afterwards is what
-- actually rebuilds the ontology.
--
-- PRECONDITION
-- ------------
-- Verify the extractor is funded FIRST, or the re-run will consume them again:
--   PROBE_FILE=scripts/loop1_extractor_readiness.sql scripts/run_loop5_probe.sh
--
-- USAGE
--   Dry run:  psql "$URL" -f scripts/loop1_reset_unlearned_episodes.sql
--   Apply:    psql "$URL" -f scripts/loop1_reset_unlearned_episodes.sql -v apply=1
-- ═══════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on
\if :{?apply}
\else
  \set apply 0
\endif

CREATE TEMP VIEW reset_targets AS
SELECT a.agent_id, a.agent_name
  FROM agents a
 WHERE a.status <> 'archived'
   AND EXISTS (SELECT 1 FROM consolidation_jobs j
                WHERE j.agent_id = a.agent_id AND j.status = 'completed')
   AND EXISTS (SELECT 1 FROM episodes e WHERE e.agent_id = a.agent_id)
   AND NOT EXISTS (SELECT 1 FROM entities x       WHERE x.agent_id = a.agent_id)
   AND NOT EXISTS (SELECT 1 FROM facts x          WHERE x.agent_id = a.agent_id)
   AND NOT EXISTS (SELECT 1 FROM semantic_rules x WHERE x.agent_id = a.agent_id);

\echo ''
\echo '── Scope: agents whose episodes would be made re-dreamable ─────────────'
SELECT count(*) AS agents,
       (SELECT count(*) FROM episodes e
         WHERE e.agent_id IN (SELECT agent_id FROM reset_targets)) AS episodes_total,
       (SELECT count(*) FROM episodes e
         WHERE e.agent_id IN (SELECT agent_id FROM reset_targets)
           AND e.consolidated) AS would_be_reset
  FROM reset_targets;

\echo ''
\echo '── Safety check: none of these may have ANY ontology (must be 0) ───────'
SELECT count(*) AS targets_with_ontology_must_be_zero
  FROM reset_targets t
 WHERE EXISTS (SELECT 1 FROM entities x       WHERE x.agent_id = t.agent_id)
    OR EXISTS (SELECT 1 FROM facts x          WHERE x.agent_id = t.agent_id)
    OR EXISTS (SELECT 1 FROM semantic_rules x WHERE x.agent_id = t.agent_id);

\echo ''
\echo '── Sample of affected agents ───────────────────────────────────────────'
SELECT t.agent_name,
       (SELECT count(*) FROM episodes e
         WHERE e.agent_id = t.agent_id AND e.consolidated) AS episodes_to_reset
  FROM reset_targets t
 ORDER BY 2 DESC
 LIMIT 10;

\if :apply
\echo ''
\echo '── APPLYING: flipping consolidated -> false ────────────────────────────'
UPDATE episodes e
   SET consolidated = false
 WHERE e.agent_id IN (SELECT agent_id FROM reset_targets)
   AND e.consolidated;

\echo ''
\echo '── Post-state: episodes now eligible for consolidation ─────────────────'
SELECT count(*) FILTER (WHERE NOT consolidated) AS now_eligible,
       count(*) FILTER (WHERE consolidated)     AS still_consumed,
       count(*)                                 AS total
  FROM episodes e
 WHERE e.agent_id IN (SELECT agent_id FROM reset_targets);
\else
\echo ''
\echo '════════════════════════════════════════════════════════════════════════'
\echo ' DRY RUN — nothing written. Re-run with  -v apply=1  to apply.'
\echo '════════════════════════════════════════════════════════════════════════'
\endif
