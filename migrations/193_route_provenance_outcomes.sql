-- Migration 193: join routing provenance to realised outcomes.
--
-- ## What this closes
--
-- Two halves of "why did this run happen the way it did" were instrumented at
-- different times and never joined to outcome:
--
--   * How the agent was ASKED — `qsrc:*` / `ibind:*` tags, added with the
--     contract-composition work (see `fermi-console/src/negotiate.rs`).
--   * How the agent was CHOSEN — `route:*` / `domain:*` tags, added alongside
--     this migration.
--
-- Without the second half a credit model cannot separate a router coverage gap
-- from agent incompetence: an agent that underperformed as the generalist
-- fallback is indistinguishable, in outcome data, from one deliberately
-- selected as the resident domain expert and found wanting. Pooling those
-- populations teaches the model to distrust whichever agents the router
-- reaches for by default — the same closed world, re-entered through the
-- credit model instead of through a `match` arm.
--
-- These views make the question answerable in SQL, which is the precondition
-- for replacing `routing.rs::domain_specialist()` — a compile-time table — with
-- a ranking measured from outcomes.
--
-- ## The join, and its one honest weakness
--
-- `episodes` (which carries the route tags) and `forecast_agent_claims` (which
-- carries the quantitative claim, and via `forecast_id` the Shapley credit)
-- are written by the same execution but share no correlation id. The episode is
-- persisted by the execution handler; the claim is written by a `tokio::spawn`
-- in the multiplier hook. Neither knows the other's primary key.
--
-- So the join here is (agent_id, driver) within a time window. That is a
-- heuristic, and it is stated as one rather than hidden:
--
--   * It can MISS when an agent is invoked twice for the same driver inside the
--     window (the two claims are indistinguishable).
--   * It cannot mis-attribute across agents or drivers — those are exact.
--
-- The correct fix is a shared correlation id: stamp `episode_id` onto the claim
-- row at write time. That is a code change in the hook, not a view, and is
-- deliberately left for a follow-up so this migration stays reversible and
-- non-blocking. `ROUTE_JOIN_WINDOW` below documents the tolerance in one place.
--
-- Reversible: creates only views and one index. No data is modified.

-- ─── Supporting index ────────────────────────────────────────────────────────
--
-- The claim side already has (workspace_id, driver, claimed_at) and
-- (agent_id, claimed_at). The join below filters on agent_id AND driver
-- together, so give it a composite to avoid re-scanning an agent's whole
-- claim history per episode.
CREATE INDEX IF NOT EXISTS idx_agent_claims_agent_driver_time
    ON public.forecast_agent_claims (agent_id, driver, claimed_at DESC)
    WHERE agent_id IS NOT NULL;

-- ─── 1. Per-run routing record, joined to what actually happened ─────────────
--
-- One row per (episode, claim) pair where routing provenance exists. This is
-- the grain everything else aggregates from; query it directly when you want
-- to look at individual decisions rather than rates.
CREATE OR REPLACE VIEW public.route_outcomes AS
SELECT
    e.episode_id,
    e.agent_id,
    a.agent_name,
    e.created_at                                  AS invoked_at,

    -- ── The routing decision (from the tags stamped by stamp_invocation) ──
    e.context -> 'invocation' ->> 'route_reason'   AS route_reason,
    COALESCE(
        (e.context -> 'invocation' ->> 'route_deliberate')::boolean,
        TRUE
    )                                              AS route_deliberate,
    e.context -> 'invocation' ->> 'route_domain'   AS domain,
    e.context -> 'invocation' ->> 'route_overrode_suggestion'
                                                   AS overrode_suggestion,

    -- ── How it was asked (the other half of the provenance) ──
    e.context -> 'invocation' ->> 'query_source'   AS query_source,
    e.context -> 'invocation' ->> 'input_binding'  AS input_binding,
    (e.context -> 'invocation' ->> 'declared_label_count')::int
                                                   AS declared_label_count,

    -- ── What it claimed ──
    c.driver,
    c.workspace_id,
    c.p50                                          AS claimed_multiplier,
    c.p95 - c.p5                                   AS claimed_spread,

    -- ── What happened ──
    f.id                                           AS forecast_id,
    f.predicted_probability,
    f.actual_outcome,
    f.brier_score,
    f.resolved_at,
    cr.shapley_value,
    cr.neutralisation,

    -- Brier of always predicting the base rate is not available per-row, so
    -- the interpretable per-agent quantity is the signed Shapley credit:
    -- positive = moved the forecast toward the truth. Surfaced as a boolean
    -- for the cheap "win rate" aggregations below.
    CASE
        WHEN cr.shapley_value IS NULL THEN NULL
        WHEN cr.shapley_value > 0     THEN TRUE
        ELSE FALSE
    END                                            AS helped

FROM public.episodes e
JOIN public.agents a
    ON a.agent_id = e.agent_id

-- Heuristic join, documented in the header. 10 minutes comfortably covers a
-- slow agent run plus the spawned hook write, without spanning a plausible
-- re-invocation of the same agent on the same driver.
JOIN public.forecast_agent_claims c
    ON  c.agent_id = e.agent_id
    AND c.driver   = e.context -> 'invocation' ->> 'driver'
    AND c.claimed_at BETWEEN e.created_at - INTERVAL '2 minutes'
                         AND e.created_at + INTERVAL '10 minutes'

LEFT JOIN public.fermi_forecasts f
    ON f.workspace_id = c.workspace_id

LEFT JOIN public.forecast_agent_credit cr
    ON  cr.forecast_id = f.id
    AND cr.agent_name  = a.agent_name

-- Only runs that actually carry a routing decision. Everything before this
-- migration has none, and must not be silently counted as an unknown route.
WHERE e.context -> 'invocation' ->> 'route_reason' IS NOT NULL;

COMMENT ON VIEW public.route_outcomes IS
  'Per-run join of routing provenance (why this agent was chosen) to realised outcome (Brier + signed Shapley credit). Grain: one row per episode-claim pair. The episode-to-claim join is a documented (agent_id, driver, time-window) heuristic — see migration 193 header.';

-- ─── 2. Is a routing reason any good? ────────────────────────────────────────
--
-- THE question this whole chain exists to answer. Group by domain, because
-- "domain_specialist beats default" in aggregate says nothing about whether the
-- specialist chosen for *climate* is the right one.
--
-- Read `avg_shapley` as the headline: it is per-agent and signed, so it is not
-- confounded by how hard the forecast was, the way a raw Brier average is.
CREATE OR REPLACE VIEW public.route_reason_performance AS
SELECT
    domain,
    route_reason,
    route_deliberate,
    COUNT(*)                                         AS runs,
    COUNT(*) FILTER (WHERE resolved_at IS NOT NULL)   AS resolved_runs,
    COUNT(*) FILTER (WHERE shapley_value IS NOT NULL) AS scored_runs,
    ROUND(AVG(shapley_value)::numeric, 6)             AS avg_shapley,
    ROUND(STDDEV(shapley_value)::numeric, 6)          AS stddev_shapley,
    ROUND(AVG(brier_score)::numeric, 6)               AS avg_brier,
    ROUND(
        (COUNT(*) FILTER (WHERE helped)::numeric
         / NULLIF(COUNT(*) FILTER (WHERE helped IS NOT NULL), 0)),
        4
    )                                                 AS help_rate,
    COUNT(DISTINCT agent_name)                        AS distinct_agents
FROM public.route_outcomes
GROUP BY domain, route_reason, route_deliberate;

COMMENT ON VIEW public.route_reason_performance IS
  'Does a routing reason produce better outcomes, per domain? avg_shapley is the headline (per-agent and signed, so unconfounded by forecast difficulty). Compare deliberate reasons against route_reason = ''default'': if a specialist route does not beat the generalist fallback in a domain, the specialist table is wrong for that domain. Beware small scored_runs.';

-- ─── 3. Which agent should own a domain? ─────────────────────────────────────
--
-- The measured replacement for `routing.rs::domain_specialist()`. Once
-- `scored_runs` is large enough per (domain, agent), this ranking is a
-- better answer than the compile-time table, and — unlike the table — a
-- third-party agent can enter it purely by performing well.
CREATE OR REPLACE VIEW public.domain_agent_ranking AS
SELECT
    domain,
    agent_name,
    COUNT(*) FILTER (WHERE shapley_value IS NOT NULL) AS scored_runs,
    ROUND(AVG(shapley_value)::numeric, 6)            AS avg_shapley,
    ROUND(AVG(brier_score)::numeric, 6)              AS avg_brier,
    ROUND(
        (COUNT(*) FILTER (WHERE helped)::numeric
         / NULLIF(COUNT(*) FILTER (WHERE helped IS NOT NULL), 0)),
        4
    )                                                AS help_rate,
    -- How often this agent got here by deliberate selection rather than by
    -- being the default. A high avg_shapley on mostly-fallback routes is a
    -- stronger signal than the same score on hand-picked work.
    ROUND(
        (COUNT(*) FILTER (WHERE route_deliberate)::numeric
         / NULLIF(COUNT(*), 0)),
        4
    )                                                AS deliberate_share,
    MAX(invoked_at)                                  AS last_invoked_at
FROM public.route_outcomes
GROUP BY domain, agent_name;

COMMENT ON VIEW public.domain_agent_ranking IS
  'Measured per-domain agent ranking by signed Shapley credit — the evidence-based replacement for the compile-time domain_specialist() table. An agent enters this ranking by performing, not by being enumerated in Rust.';

-- ─── 4. Was overruling the strategist right? ─────────────────────────────────
--
-- The first feedback signal Fermi's decomposition has ever had. Today Fermi
-- suggests an agent per driver, the router may overrule it on the domain guard,
-- and Fermi never learns whether that was correct.
--
-- Compare the two populations: runs where the router deferred to Fermi
-- (overrode_suggestion IS NULL) against runs where it overruled Fermi. If
-- overruling does not improve avg_shapley, the domain guard is destroying
-- value and should be loosened.
CREATE OR REPLACE VIEW public.router_override_scorecard AS
SELECT
    domain,
    (overrode_suggestion IS NOT NULL)                AS overruled_fermi,
    overrode_suggestion                              AS fermi_suggested,
    agent_name                                       AS router_chose,
    COUNT(*)                                         AS runs,
    COUNT(*) FILTER (WHERE shapley_value IS NOT NULL) AS scored_runs,
    ROUND(AVG(shapley_value)::numeric, 6)            AS avg_shapley,
    ROUND(AVG(brier_score)::numeric, 6)              AS avg_brier,
    ROUND(
        (COUNT(*) FILTER (WHERE helped)::numeric
         / NULLIF(COUNT(*) FILTER (WHERE helped IS NOT NULL), 0)),
        4
    )                                                AS help_rate
FROM public.route_outcomes
GROUP BY domain, (overrode_suggestion IS NOT NULL), overrode_suggestion, agent_name;

COMMENT ON VIEW public.router_override_scorecard IS
  'Was overruling Fermi right? Compares runs where the router deferred to the strategist against runs where the domain guard displaced it. This is the first outcome feedback available to the decomposition layer — if overruling does not improve avg_shapley, the guard is destroying value.';

-- ─── 5. Declaration quality vs outcome ───────────────────────────────────────
--
-- The negotiate.rs module doc names this as "the input the adaptation loop
-- needs" but nothing joined it to outcome. Does an agent that declares a
-- richer contract actually get asked better and perform better?
--
-- If `declared_contract` outperforms `undeclared`, that is a concrete,
-- evidence-backed argument for requiring contracts — and a number to show an
-- agent designer.
CREATE OR REPLACE VIEW public.declaration_quality_outcomes AS
SELECT
    query_source,
    input_binding,
    CASE
        WHEN declared_label_count IS NULL  THEN 'unknown'
        WHEN declared_label_count = 0      THEN 'none'
        WHEN declared_label_count <= 3     THEN 'sparse'
        ELSE 'rich'
    END                                              AS declaration_richness,
    COUNT(*)                                         AS runs,
    COUNT(*) FILTER (WHERE shapley_value IS NOT NULL) AS scored_runs,
    ROUND(AVG(shapley_value)::numeric, 6)            AS avg_shapley,
    ROUND(AVG(brier_score)::numeric, 6)              AS avg_brier,
    ROUND(
        (COUNT(*) FILTER (WHERE helped)::numeric
         / NULLIF(COUNT(*) FILTER (WHERE helped IS NOT NULL), 0)),
        4
    )                                                AS help_rate
FROM public.route_outcomes
GROUP BY query_source, input_binding, declaration_richness;

COMMENT ON VIEW public.declaration_quality_outcomes IS
  'Does declaring a richer contract produce better outcomes? Joins the negotiate.rs composition ladder (query_source, input_binding, declared_label_count) to realised Shapley credit. The evidence for or against requiring contracts.';
