-- ═══════════════════════════════════════════════════════════════════════
-- Loop 1 retrievability census — can each agent actually recall what it
-- learned?
--
--   READ ONLY. Writes nothing.
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT THIS MEASURES, AND WHY IT IS NOT THE SAME AS "DID IT DREAM"
-- ----------------------------------------------------------------
-- `loop1_maturity_census.sql` answers "did consolidation produce anything".
-- This answers the next question, which is the one that actually closes the
-- loop: **can any of it be retrieved into the next execution?**
--
-- KG injection (`src/agent_backend/kg_context.rs`) is embedding-based on both
-- of its paths:
--
--   * ANN path      — pgvector similarity on the `embedding` column
--   * fallback path — `filter_map(|r| r.embedding.as_ref())`, in-memory cosine
--
-- A row with a NULL embedding is invisible to both. An agent can therefore
-- hold hundreds of correct semantic rules and behave exactly as if it had
-- never dreamed. Loop 1 is not closed by writing knowledge; it is closed by
-- reading it back.
--
-- THE ONE EXCEPTION: `cep_%` entities
-- -----------------------------------
-- `get_top_k_entities_with_cep` (memory/src/store.rs) returns CEP seed
-- entities through a second `UNION ALL` branch that is neither
-- similarity-gated nor embedding-filtered. They are always injected. So a
-- `cep_`-typed row with no embedding IS retrievable, and this census counts it
-- as such. Nothing else without an embedding is.
--
-- WHY THAT PREFIX IS LOAD-BEARING
-- -------------------------------
-- Agent cards can declare `seed_facts` — curated reference data with
-- `properties: {value, source, applies_to}`. Those become entities with no
-- `source_episodes` (they came from a card, not an episode) and no embedding.
-- If the declared `entity_type` starts with `cep_`, they are always injected
-- and everything works. If it does not — `field_baseline`,
-- `confederation_coefficient`, `institutional_density` and friends — they are
-- classified as episodic, require an embedding they were never given, and are
-- permanently unreachable. The prefix is the whole difference, and nothing
-- validates it at card-authoring time.
--
-- HISTORICAL NOTE
-- ---------------
-- Until 2026-08-15 the injection gate did not consult these tables at all. It
-- read `card.ontology_stats.entities`, a field that DB-reconstructed cards
-- hardcode to 0, that 31 of 100 curated card JSONs omit, and whose only
-- updater counted `SELECT COUNT(*) FROM kg_entities` — a table that has never
-- existed, with the error swallowed. The gate was closed for effectively every
-- agent, so this census would have read "retrievable" while nothing was being
-- retrieved. The gate now queries `entities` / `semantic_rules` directly, so
-- what this reports is what the runtime will do.
--
-- READING THE OUTPUT
-- ------------------
--   gate     = OPEN        at least one embedded entity or active rule; KG
--                          context will be injected
--   gate     = UNEMBEDDED  rows exist but none carry an embedding. This is a
--                          defect, not a lifecycle stage — something wrote
--                          knowledge without embedding it. The runtime logs a
--                          warning on every execution for these agents.
--   gate     = EMPTY       nothing learned yet. Expected for new agents.
--
-- USAGE
--   psql "$URL" -f scripts/loop1_retrievability_census.sql
-- ═══════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on
\pset pager off

CREATE TEMP VIEW retrievability AS
SELECT
    a.agent_id,
    a.agent_name,
    -- Throwaway fixtures from integration tests. 591 of them on this
    -- deployment against 154 real agents, each holding a single entity. Left
    -- in the totals so the platform view stays honest, but excluded from every
    -- per-agent list below, where they buried the four rows that mattered.
    (a.agent_name LIKE 'test\_agent\_%') AS is_fixture,
    a.last_consolidated_at,
    (SELECT count(*) FROM entities e
      WHERE e.agent_id = a.agent_id
        AND (e.t_invalid IS NULL OR e.t_invalid > NOW()))              AS entities,
    (SELECT count(*) FROM entities e
      WHERE e.agent_id = a.agent_id AND e.embedding IS NOT NULL
        AND (e.t_invalid IS NULL OR e.t_invalid > NOW()))              AS entities_embedded,
    -- Always injected regardless of embedding.
    (SELECT count(*) FROM entities e
      WHERE e.agent_id = a.agent_id AND e.entity_type LIKE 'cep\_%'
        AND (e.t_invalid IS NULL OR e.t_invalid > NOW()))              AS cep_entities,
    -- Card seed_facts that missed the cep_ prefix: no episode of origin, no
    -- embedding, not cep-typed. Unreachable by any retrieval path.
    (SELECT count(*) FROM entities e
      WHERE e.agent_id = a.agent_id
        AND e.entity_type NOT LIKE 'cep\_%'
        AND e.embedding IS NULL
        AND (e.source_episodes IS NULL OR cardinality(e.source_episodes) = 0)
        AND (e.t_invalid IS NULL OR e.t_invalid > NOW()))              AS stranded_seeds,
    (SELECT count(*) FROM semantic_rules r
      WHERE r.agent_id = a.agent_id AND r.is_active)                   AS rules,
    (SELECT count(*) FROM semantic_rules r
      WHERE r.agent_id = a.agent_id AND r.is_active
        AND r.embedding IS NOT NULL)                                   AS rules_embedded,
    (SELECT count(*) FROM ontology_snapshots s
      WHERE s.agent_id = a.agent_id)                                   AS snapshots
  FROM agents a
 WHERE a.status <> 'archived';

-- Mirrors `kg_context::classify_retrievable` exactly. If these disagree, the
-- census is lying and one of them must change.
CREATE TEMP VIEW graded AS
SELECT r.*,
       CASE
         WHEN r.entities_embedded > 0 OR r.rules_embedded > 0
           OR r.cep_entities > 0                              THEN 'OPEN'
         WHEN r.entities > 0 OR r.rules > 0                   THEN 'UNEMBEDDED'
         ELSE 'EMPTY'
       END AS gate
  FROM retrievability r;

\echo ''
\echo '── Platform summary: can agents recall what they learned? ──────────────'
\echo '   Real agents only. Test fixtures are summarised separately below.'
SELECT gate,
       count(*)          AS agents,
       sum(entities)     AS entities,
       sum(rules)        AS rules,
       sum(snapshots)    AS snapshots
  FROM graded
 WHERE NOT is_fixture
 GROUP BY gate
 ORDER BY CASE gate WHEN 'UNEMBEDDED' THEN 1 WHEN 'OPEN' THEN 2 ELSE 3 END;

\echo ''
\echo '── Test fixtures (excluded from every list below) ─────────────────────'
SELECT count(*) AS fixtures, sum(entities) AS entities, sum(rules) AS rules
  FROM graded WHERE is_fixture;

\echo ''
\echo '── DEFECT: learned but cannot recall (should be empty) ───────────────────'
\echo '   These agents log a warning on every execution.'
SELECT agent_name, entities, entities_embedded, rules, rules_embedded,
       last_consolidated_at
  FROM graded
 WHERE gate = 'UNEMBEDDED' AND NOT is_fixture
 ORDER BY (entities + rules) DESC
 LIMIT 25;

\echo ''
\echo '── Platform-wide: where every entity stands ──────────────────────────'
SELECT
  CASE
    WHEN entity_type LIKE 'cep\_%'                     THEN '1 cep seed      (always injected)'
    WHEN embedding IS NOT NULL                         THEN '2 learned+embedded (retrievable)'
    WHEN source_episodes IS NULL
      OR cardinality(source_episodes) = 0              THEN '3 seed, no cep_ prefix (UNREACHABLE)'
    ELSE                                                    '4 learned, unembedded  (UNREACHABLE)'
  END AS class,
  count(*) AS rows
  FROM entities
 WHERE (t_invalid IS NULL OR t_invalid > NOW())
 GROUP BY 1 ORDER BY 1;

\echo ''
\echo '── Stranded seed data: card seed_facts that missed the cep_ prefix ─────'
\echo '   Curated reference knowledge no retrieval path can reach. Fix is'
\echo '   either to re-type these cep_* or to embed them — see script header.'
SELECT agent_name, stranded_seeds, entities, cep_entities
  FROM graded
 WHERE stranded_seeds > 0 AND NOT is_fixture
 ORDER BY stranded_seeds DESC
 LIMIT 25;

\echo ''
\echo '── Which entity_types are stranded (candidates for the cep_ prefix) ────'
SELECT entity_type, count(*) AS stranded
  FROM entities
 WHERE entity_type NOT LIKE 'cep\_%'
   AND embedding IS NULL
   AND (source_episodes IS NULL OR cardinality(source_episodes) = 0)
   AND (t_invalid IS NULL OR t_invalid > NOW())
 GROUP BY 1 ORDER BY 2 DESC LIMIT 20;

\echo ''
\echo '── Partial embedding coverage among LEARNED entities ──────────────────'
\echo '   These came from episodes but were never embedded — a real gap in the'
\echo '   learning path, unlike stranded seeds which never had one.'
SELECT a.agent_name, count(*) AS learned_unembedded
  FROM entities e JOIN agents a ON a.agent_id = e.agent_id
 WHERE e.entity_type NOT LIKE 'cep\_%'
   AND e.embedding IS NULL
   AND cardinality(e.source_episodes) > 0
   AND (e.t_invalid IS NULL OR e.t_invalid > NOW())
   AND a.agent_name NOT LIKE 'test\_agent\_%'
 GROUP BY 1 ORDER BY 2 DESC LIMIT 20;

\echo ''
\echo '── Ontology development: has the snapshot series advanced? ─────────────'
\echo '   Snapshots were CLI-only until 2026-08-15; agents that dreamt through'
\echo '   the API before then will show 0 and start advancing on next dream.'
SELECT gate,
       count(*) FILTER (WHERE snapshots = 0) AS no_snapshots,
       count(*) FILTER (WHERE snapshots > 0) AS has_snapshots,
       max(snapshots)                        AS max_series_length
  FROM graded
 WHERE NOT is_fixture
 GROUP BY gate
 ORDER BY 1;

\echo ''
\echo '── Top learners (sanity check that the loop is producing) ──────────────'
SELECT agent_name, entities, rules, snapshots, last_consolidated_at
  FROM graded
 WHERE gate = 'OPEN' AND NOT is_fixture
 ORDER BY (entities + rules) DESC
 LIMIT 15;
