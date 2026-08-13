-- ═══════════════════════════════════════════════════════════════════════
-- Verify the fleet evolution query, and preview the census — READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- Mirrors `evolution::fleet_evolution` (src/handlers/evolution.rs) and applies
-- the same banding as `compute_evolution`, so the ecology census can be checked
-- against production before it ships. KEEP IN SYNC with the Rust.
--
-- Two things being checked:
--   1. the query runs at all against the real schema
--   2. the resulting population reads honestly — specifically that untried
--      agents come back UNRANKED rather than bottom-ranked
--
-- Run: PROBE_FILE=scripts/evolution_census_check.sql scripts/run_loop5_probe.sh
-- ═══════════════════════════════════════════════════════════════════════

CREATE TEMP VIEW ev AS
SELECT a.agent_id, a.agent_name, a.tier, a.status,
       a.agent_name LIKE 'test\_agent\_%' AS is_test_cruft,
       COALESCE(a.persona_version, 1) AS persona_version,
         (SELECT COUNT(*) FROM entities x       WHERE x.agent_id = a.agent_id)
       + (SELECT COUNT(*) FROM facts x          WHERE x.agent_id = a.agent_id)
       + (SELECT COUNT(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS ontology_size,
       (SELECT COUNT(*) FROM semantic_rules x
         WHERE x.agent_id = a.agent_id AND x.verification_status = 'verified') AS verified_rules,
       (SELECT COUNT(*) FROM anomaly_events e
         WHERE e.agent_id = a.agent_id AND e.resolved_at IS NOT NULL) AS anomalies_resolved,
       (SELECT COUNT(*) FROM anomaly_events e
         WHERE e.agent_id = a.agent_id AND e.resolved_at IS NULL AND e.requires_review) AS anomalies_open,
       (SELECT COUNT(*) FROM anomaly_events e WHERE e.agent_id = a.agent_id) AS anomalies_ever,
       (SELECT COUNT(*) FROM episodes ep WHERE ep.agent_id = a.agent_id) AS total_episodes,
       (SELECT AVG(s.score) FROM eval_signals s WHERE s.agent_id = a.agent_id) AS eval_mean_score,
       (SELECT COUNT(DISTINCT s.dimension) FROM eval_signals s WHERE s.agent_id = a.agent_id) AS eval_dims,
       (SELECT COUNT(*) FROM fermi_forecasts f
         WHERE f.status='resolved' AND f.brier_score IS NOT NULL
           AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
             OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
             OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name)))) AS n_forecasts,
       (SELECT AVG(f.brier_score)::float8 FROM fermi_forecasts f
         WHERE f.status='resolved' AND f.brier_score IS NOT NULL
           AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
             OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
             OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name)))) AS brier_mean,
       (SELECT COUNT(*) FROM fermi_forecasts f
         WHERE f.status='resolved' AND f.brier_score IS NOT NULL AND f.actual_outcome
           AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
             OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
             OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name)))) AS n_yes
  FROM agents a
 WHERE a.status <> 'archived';

-- Tier per dimension, then the breadth-gated ladder, mirroring the Rust.
CREATE TEMP VIEW ev_tiers AS
SELECT e.*,
       CASE WHEN n_forecasts = 0 OR brier_mean IS NULL THEN NULL
            ELSE 1 - (brier_mean / NULLIF(
                   (n_yes::float8/n_forecasts) * (1 - (n_yes::float8/n_forecasts)), 0)) END AS skill,
       CASE WHEN ontology_size = 0 THEN 0
            WHEN ontology_size < 25 THEN 1
            WHEN verified_rules = 0 THEN 2
            WHEN ontology_size < 100 OR verified_rules < 5 THEN 2
            ELSE 3 END AS t_memory,
       CASE WHEN anomalies_open > 0 THEN 0 ELSE LEAST(3, (
              -- route 1: governability (corrections absorbed)
              CASE WHEN anomalies_resolved + GREATEST(persona_version - 1, 0) = 0 THEN 0
                   WHEN anomalies_resolved + GREATEST(persona_version - 1, 0) < 3 THEN 1
                   WHEN anomalies_resolved + GREATEST(persona_version - 1, 0) < 10 THEN 2
                   ELSE 3 END
            + -- route 2: reliability (low incident rate under exposure); ADDS
              CASE WHEN total_episodes > 0
                    AND anomalies_ever::float8 / total_episodes > 0.01 THEN 0
                   WHEN total_episodes >= 500 THEN 3
                   WHEN total_episodes >= 100 THEN 2
                   WHEN total_episodes >= 25 THEN 1
                   ELSE 0 END)) END AS t_conduct_points,
       CASE WHEN eval_mean_score IS NULL THEN 0
            WHEN eval_mean_score < 0.55 THEN 0
            WHEN eval_mean_score < 0.75 OR eval_dims < 2 THEN 1
            WHEN eval_mean_score < 0.90 OR eval_dims < 3 THEN 2
            ELSE 3 END AS t_craft
  FROM ev e;

CREATE TEMP VIEW ev_banded AS
SELECT t.*,
       CASE WHEN t_conduct_points = 0 THEN 0
            WHEN t_conduct_points = 1 THEN 1
            WHEN t_conduct_points <= 3 THEN 2
            ELSE 3 END AS t_conduct,
       CASE WHEN skill IS NULL OR n_forecasts = 0 THEN 0
            WHEN skill <= 0 THEN 0
            WHEN n_forecasts < 5 THEN 1
            WHEN n_forecasts < 20 THEN 2
            ELSE 3 END AS t_judgment,
       (ontology_size = 0 AND n_forecasts = 0 AND eval_mean_score IS NULL
        AND anomalies_resolved = 0 AND anomalies_open = 0 AND total_episodes = 0
        AND persona_version <= 1) AS no_usage_data
  FROM ev_tiers t;

\echo ''
\echo '── 1. Population by rank (test cruft excluded, as the view does) ────────'
\echo '   unranked = never exercised. NOT a rank of zero.'
WITH scored AS (
  SELECT b.*,
         (CASE WHEN t_memory>=1 THEN 1 ELSE 0 END + CASE WHEN t_judgment>=1 THEN 1 ELSE 0 END
        + CASE WHEN t_conduct>=1 THEN 1 ELSE 0 END + CASE WHEN t_craft>=1 THEN 1 ELSE 0 END) AS breadth,
         (CASE WHEN t_memory>=2 THEN 1 ELSE 0 END + CASE WHEN t_judgment>=2 THEN 1 ELSE 0 END
        + CASE WHEN t_conduct>=2 THEN 1 ELSE 0 END + CASE WHEN t_craft>=2 THEN 1 ELSE 0 END) AS solid,
         (CASE WHEN t_memory>=3 THEN 1 ELSE 0 END + CASE WHEN t_judgment>=3 THEN 1 ELSE 0 END
        + CASE WHEN t_conduct>=3 THEN 1 ELSE 0 END + CASE WHEN t_craft>=3 THEN 1 ELSE 0 END) AS deep
    FROM ev_banded b
   WHERE NOT is_test_cruft
)
SELECT CASE
         WHEN no_usage_data THEN 'unranked (pending usage)'
         WHEN breadth = 0 THEN '0 dormant'
         WHEN breadth = 1 THEN '1 hatchling'
         WHEN breadth = 2 AND solid = 0 THEN '1 hatchling'
         WHEN breadth = 2 THEN '2 fledgling'
         WHEN breadth = 3 AND solid < 2 THEN '2 fledgling'
         WHEN breadth = 3 THEN '3 adept'
         WHEN solid >= 4 AND deep >= 1 THEN '5 exemplar'
         WHEN solid >= 3 THEN '4 specialist'
         ELSE '3 adept' END AS rank,
       count(*) AS agents
  FROM scored
 GROUP BY 1 ORDER BY 1;

\echo ''
\echo '── 2. Forecasting credentials (the public part of the badge) ───────────'
SELECT count(*) FILTER (WHERE n_forecasts > 0)                   AS with_record,
       count(*) FILTER (WHERE skill > 0)                          AS beating_base_rate,
       count(*) FILTER (WHERE skill IS NOT NULL AND skill <= 0)   AS no_skill,
       round(max(skill)::numeric, 3)                              AS best_skill
  FROM ev_banded WHERE NOT is_test_cruft;

\echo ''
\echo '── 3. Top specimens by earned dimensions ───────────────────────────────'
SELECT agent_name, t_memory AS memory, t_judgment AS judgment,
       t_conduct AS conduct, t_craft AS craft,
       ontology_size, n_forecasts, round(skill::numeric,3) AS skill
  FROM ev_banded
 WHERE NOT is_test_cruft AND NOT no_usage_data
 ORDER BY (t_memory + t_judgment + t_conduct + t_craft) DESC, ontology_size DESC
 LIMIT 12;

\echo ''
\echo '── 4. How much of the catalogue is test cruft? ─────────────────────────'
SELECT count(*) FILTER (WHERE is_test_cruft) AS test_cruft,
       count(*) FILTER (WHERE NOT is_test_cruft) AS real_agents,
       count(*) AS total
  FROM ev_banded;
