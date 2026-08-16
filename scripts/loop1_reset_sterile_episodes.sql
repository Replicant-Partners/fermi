-- ═══════════════════════════════════════════════════════════════════════
-- Recover individual episodes that were consumed but produced nothing
--
--   DRY RUN by default.  Pass -v apply=1 to actually write.
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHY THIS EXISTS ALONGSIDE loop1_reset_unlearned_episodes.sql
-- ------------------------------------------------------------
-- The sibling script recovers whole agents, and gates on the agent having a
-- **completely empty ontology**:
--
--     AND NOT EXISTS (SELECT 1 FROM entities       WHERE agent_id = a.agent_id)
--     AND NOT EXISTS (SELECT 1 FROM facts          WHERE agent_id = a.agent_id)
--     AND NOT EXISTS (SELECT 1 FROM semantic_rules WHERE agent_id = a.agent_id)
--
-- That gate is correct and deliberately conservative, but it has a trap: the
-- moment an operator runs ONE successful consolidation on a damaged agent, the
-- agent has a non-empty ontology and is excluded from recovery permanently —
-- while the bulk of its episode history is still marked consumed and still
-- unlearned. Diagnosing the problem by re-running dreaming is the single most
-- natural thing to do, and doing it locks the material out of the fix.
--
-- Observed instance: `fermi` had one unconsolidated episode left, produced
-- 4 entities and 5 rules from it, and thereby disqualified itself from the
-- agent-level script while the rest of its history stayed stranded.
--
-- WHAT THIS DOES DIFFERENTLY
-- --------------------------
-- It works per EPISODE, not per agent, using the episode-provenance arrays the
-- knowledge tables already carry:
--
--     entities.source_episodes        UUID[]
--     facts.source_episodes           UUID[]
--     semantic_rules.source_episode_cluster UUID[]
--
-- An episode is "sterile" when it is marked `consolidated` yet appears in none
-- of those three arrays: the loop consumed it and it contributed to nothing.
-- That is an exact per-row fact, not an inference about the agent, so an agent
-- that has since learned something is still eligible for the parts of its
-- history that were lost.
--
-- SAFETY
-- ------
-- Nothing is deleted. Only `episodes.consolidated` is flipped back to false.
--
-- Three predicates must all hold, and the third is the important one:
--
--   1. consolidated = true                 (there is something to undo)
--   2. sterile                             (it demonstrably produced nothing)
--   3. a completed zero-yield consolidation job exists for the same agent,
--      started AFTER the episode was written
--
-- Predicate 3 is what makes this safe. Without it, any episode the extractor
-- legitimately found nothing quotable in would be reset on every pass, forever.
-- With it, an episode is only recovered when a job that demonstrably learned
-- nothing at all (`entities_created = 0 AND facts_created = 0 AND
-- rules_extracted = 0`) ran after it — i.e. when there is positive evidence of
-- the extractor-less failure mode rather than merely an absence of output.
--
-- RESIDUAL CHURN CAVEAT
-- ---------------------
-- If a re-run is itself zero-yield, the same episodes qualify again on the next
-- pass. That is intended — a zero-yield run means the loop is still broken and
-- the material should stay recoverable. It stops being possible once cycles
-- yield anything, and the handler's preflight gate (FAILED_DEPENDENCY when no
-- extraction model resolves) now refuses to start a run that cannot learn.
--
-- PRECONDITION
-- ------------
-- Verify the extractor is funded FIRST, or the re-run will consume them again:
--   PROBE_FILE=scripts/loop1_extractor_readiness.sql scripts/run_loop5_probe.sh
--
-- USAGE
--   Dry run:    psql "$URL" -f scripts/loop1_reset_sterile_episodes.sql
--   One agent:  psql "$URL" -f scripts/loop1_reset_sterile_episodes.sql -v agent=fermi
--   Apply:      psql "$URL" -f scripts/loop1_reset_sterile_episodes.sql -v apply=1
-- ═══════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on
\pset pager off
\if :{?apply}
\else
  \set apply 0
\endif
\if :{?agent}
\else
  \set agent ''
\endif

-- Agents with positive evidence of a zero-yield consolidation run.
CREATE TEMP VIEW sterile_jobs AS
SELECT j.agent_id, min(j.started_at) AS first_sterile_run
  FROM consolidation_jobs j
 WHERE j.status = 'completed'
   AND j.entities_created = 0
   AND j.facts_created    = 0
   AND j.rules_extracted  = 0
 GROUP BY j.agent_id;

CREATE TEMP VIEW sterile_episodes AS
SELECT e.episode_id, e.agent_id, a.agent_name, e.timestamp_ref
  FROM episodes e
  JOIN agents a       ON a.agent_id = e.agent_id
  JOIN sterile_jobs s ON s.agent_id = e.agent_id
 WHERE e.consolidated
   AND a.status <> 'archived'
   -- The episode predates a run that learned nothing.
   AND s.first_sterile_run > e.timestamp_ref
   -- Optional single-agent scope.
   AND (:'agent' = '' OR a.agent_name = :'agent')
   -- Contributed to no entity, fact or rule.
   AND NOT EXISTS (
         SELECT 1 FROM entities x
          WHERE x.agent_id = e.agent_id
            AND x.source_episodes @> ARRAY[e.episode_id])
   AND NOT EXISTS (
         SELECT 1 FROM facts x
          WHERE x.agent_id = e.agent_id
            AND x.source_episodes @> ARRAY[e.episode_id])
   AND NOT EXISTS (
         SELECT 1 FROM semantic_rules x
          WHERE x.agent_id = e.agent_id
            AND x.source_episode_cluster @> ARRAY[e.episode_id]);

\echo ''
\echo '── Scope: episodes consumed that produced nothing ──────────────────────'
SELECT count(DISTINCT agent_id) AS agents,
       count(*)                 AS episodes_to_reset
  FROM sterile_episodes;

\echo ''
\echo '── Safety check: none of these may appear in any provenance array ──────'
\echo '   (must be 0 — the view already excludes them; this re-derives it)'
SELECT count(*) AS contributing_episodes_must_be_zero
  FROM sterile_episodes se
 WHERE EXISTS (SELECT 1 FROM entities x
                WHERE x.source_episodes @> ARRAY[se.episode_id])
    OR EXISTS (SELECT 1 FROM facts x
                WHERE x.source_episodes @> ARRAY[se.episode_id])
    OR EXISTS (SELECT 1 FROM semantic_rules x
                WHERE x.source_episode_cluster @> ARRAY[se.episode_id]);

\echo ''
\echo '── Per agent: what is recoverable vs. what it already learned ──────────'
SELECT se.agent_name,
       count(*) AS episodes_to_reset,
       (SELECT count(*) FROM entities x       WHERE x.agent_id = se.agent_id) AS entities_now,
       (SELECT count(*) FROM semantic_rules x WHERE x.agent_id = se.agent_id) AS rules_now
  FROM sterile_episodes se
 GROUP BY se.agent_name, se.agent_id
 ORDER BY 2 DESC
 LIMIT 25;

\if :apply
\echo ''
\echo '── APPLYING: flipping consolidated -> false ────────────────────────────'
UPDATE episodes e
   SET consolidated = false
 WHERE e.episode_id IN (SELECT episode_id FROM sterile_episodes);

\echo ''
\echo '── Post-state ──────────────────────────────────────────────────────────'
SELECT count(*) FILTER (WHERE NOT e.consolidated) AS now_eligible,
       count(*) FILTER (WHERE e.consolidated)     AS still_consumed,
       count(*)                                   AS total
  FROM episodes e
 WHERE e.agent_id IN (SELECT DISTINCT agent_id FROM sterile_jobs);

\echo ''
\echo 'Next: re-run dreaming for the affected agents. Consolidation now refuses'
\echo 'to start without a funded extractor, so a run that begins can learn.'
\else
\echo ''
\echo '════════════════════════════════════════════════════════════════════════'
\echo ' DRY RUN — nothing written. Re-run with  -v apply=1  to apply.'
\echo '════════════════════════════════════════════════════════════════════════'
\endif
