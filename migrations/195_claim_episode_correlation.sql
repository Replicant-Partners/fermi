-- Migration 195: give claims and episodes a shared correlation id.
--
-- ## The follow-up migration 193 asked for
--
-- `193_route_provenance_outcomes.sql` joined `episodes` to
-- `forecast_agent_claims` on `(agent_id, driver)` within a time window, and
-- said so plainly in its own header:
--
--     "The correct fix is a shared correlation id: stamp `episode_id` onto
--      the claim row at write time. That is a code change in the hook, not a
--      view, and is deliberately left for a follow-up."
--
-- This is that follow-up. The heuristic was honest about its failure mode —
-- it MISSES when an agent is invoked twice for the same driver inside the
-- window, because the two claims are then indistinguishable. That is
-- precisely the case that matters most: an agent re-run after a correction is
-- exactly when you most want to know which run produced which claim.
--
-- ## Why this is a costing problem, not just a routing one
--
-- Cost per resolved forecast is `SUM(episodes.cost_usd)` over every execution
-- that contributed to a forecast, divided into its Brier score. That sum needs
-- an exact episode → forecast path. Via the claim, that path is
-- `episodes → forecast_agent_claims → fermi_forecasts`, and a heuristic first
-- hop makes the whole total approximate — it can attribute one forecast's
-- spend to another. A marketplace cannot settle on a total assembled from a
-- time-window guess.
--
-- ## Why there is deliberately NO foreign key
--
-- The claim is written by a `tokio::spawn`ed hook; the episode is written
-- later by the request handler. The two races, and the claim frequently lands
-- first. An FK to `episodes(episode_id)` would therefore reject valid claims
-- nondeterministically, converting a benign ordering detail into lost
-- attribution data — and claims cannot be reconstructed after the fact.
--
-- The id is allocated by the handler BEFORE the hook is spawned and passed in,
-- so both rows agree on it regardless of which lands first. Referential
-- integrity is enforced by construction rather than by constraint, which is
-- the correct trade when the alternative is dropping the row.
--
-- Additive and reversible: one nullable column, two indexes, one view
-- redefinition that preserves every existing column.

ALTER TABLE public.forecast_agent_claims
    ADD COLUMN IF NOT EXISTS episode_id UUID;

COMMENT ON COLUMN public.forecast_agent_claims.episode_id IS
    'Execution that produced this claim. Allocated by the handler before the '
    'claim hook is spawned, so it is exact rather than inferred. No FK: the '
    'claim often lands before the episode row exists (see migration header). '
    'NULL for claims written before migration 195 — those still resolve via '
    'the heuristic fallback in route_outcomes.';

CREATE INDEX IF NOT EXISTS idx_agent_claims_episode
    ON public.forecast_agent_claims (episode_id)
    WHERE episode_id IS NOT NULL;

-- ─── Redefine route_outcomes to prefer the exact join ────────────────────────
--
-- Same column list plus one appended column (`join_method`), which is what
-- CREATE OR REPLACE VIEW permits — so the four dependent views
-- (route_reason_performance, domain_agent_ranking, router_override_scorecard,
-- declaration_quality_outcomes) keep working untouched and inherit the fix.
--
-- The join is exact when the claim carries an episode_id, and falls back to
-- migration 193's window for historical rows. `join_method` is surfaced so a
-- reader can tell which rows are trustworthy instead of having to know the
-- migration date — the same reasoning as `episodes.cost_basis` in mig-194.
-- Filter on `join_method = 'exact'` for anything that settles money.
CREATE OR REPLACE VIEW public.route_outcomes AS
SELECT
    e.episode_id,
    e.agent_id,
    a.agent_name,
    e.created_at                                  AS invoked_at,

    e.context -> 'invocation' ->> 'route_reason'   AS route_reason,
    COALESCE(
        (e.context -> 'invocation' ->> 'route_deliberate')::boolean,
        TRUE
    )                                              AS route_deliberate,
    e.context -> 'invocation' ->> 'route_domain'   AS domain,
    e.context -> 'invocation' ->> 'route_overrode_suggestion'
                                                   AS overrode_suggestion,

    e.context -> 'invocation' ->> 'query_source'   AS query_source,
    e.context -> 'invocation' ->> 'input_binding'  AS input_binding,
    (e.context -> 'invocation' ->> 'declared_label_count')::int
                                                   AS declared_label_count,

    c.driver,
    c.workspace_id,
    c.p50                                          AS claimed_multiplier,
    c.p95 - c.p5                                   AS claimed_spread,

    f.id                                           AS forecast_id,
    f.predicted_probability,
    f.actual_outcome,
    f.brier_score,
    f.resolved_at,
    cr.shapley_value,
    cr.neutralisation,

    CASE
        WHEN cr.shapley_value IS NULL THEN NULL
        WHEN cr.shapley_value > 0     THEN TRUE
        ELSE FALSE
    END                                            AS helped,

    -- Appended by migration 195.
    CASE
        WHEN c.episode_id IS NOT NULL THEN 'exact'
        ELSE 'heuristic_window'
    END                                            AS join_method

FROM public.episodes e
JOIN public.agents a
    ON a.agent_id = e.agent_id

-- Exact when the claim was stamped (mig-195), else migration 193's window.
-- Written as one predicate rather than a UNION so the dependent views need no
-- change and a claim can never match twice.
JOIN public.forecast_agent_claims c
    ON  (
            c.episode_id = e.episode_id
        )
        OR (
            c.episode_id IS NULL
            AND c.agent_id = e.agent_id
            AND c.driver   = e.context -> 'invocation' ->> 'driver'
            AND c.claimed_at BETWEEN e.created_at - INTERVAL '2 minutes'
                                 AND e.created_at + INTERVAL '10 minutes'
        )

LEFT JOIN public.fermi_forecasts f
    ON f.workspace_id = c.workspace_id

LEFT JOIN public.forecast_agent_credit cr
    ON  cr.forecast_id = f.id
    AND cr.agent_name  = a.agent_name

WHERE e.context -> 'invocation' ->> 'route_reason' IS NOT NULL;

-- ─── Cost per forecast: the query this whole chain exists to enable ──────────
--
-- Spend attributable to a forecast, with the honesty of that spend stated
-- rather than assumed. Two independent trust dimensions, and BOTH must hold
-- for a row to be usable in a cost-per-Brier-point comparison:
--
--   * `cost_basis` (mig-194)  — was the run priced against a known rate with
--                               a measured token split?
--   * `join_method` (mig-195) — do we know this execution belongs to THIS
--                               forecast, or did a time window guess it?
--
-- `attributed_cost_usd` counts only spend where both are sound.
-- `unattributed_cost_usd` is the rest — deliberately reported rather than
-- dropped, because it is real spend and hiding it understates cost.
--
-- ## The de-duplication that makes this correct
--
-- `apply_agent_multipliers` writes ONE CLAIM PER DRIVER PREFIX
-- (`for prefix in &driver_prefixes`), all sharing one episode_id. So an agent
-- covering three drivers produces three claim rows for a single execution.
-- Summing `episodes.cost_usd` across a naive forecast→claim→episode join would
-- therefore multiply that execution's cost by its driver count — inflating
-- exactly the broad-coverage agents that cost the most.
--
-- The CTE collapses to DISTINCT (forecast_id, episode_id) first, so each
-- execution contributes its cost exactly once no matter how many drivers it
-- claimed. Any future consumer joining these tables directly must do the same.
CREATE OR REPLACE VIEW public.forecast_cost_attribution AS
WITH forecast_episodes AS (
    -- One row per (forecast, execution). DISTINCT is load-bearing, not tidiness.
    SELECT DISTINCT
        f.id         AS forecast_id,
        c.episode_id AS episode_id
    FROM public.fermi_forecasts f
    JOIN public.forecast_agent_claims c
        ON c.workspace_id = f.workspace_id
    WHERE c.episode_id IS NOT NULL
),
-- Claims from before mig-195 (or written by a path that passed no id). Their
-- spend is REAL but unlocatable: there is no episode to price. Counted so the
-- view can say how much of the picture is missing rather than implying none.
unlinked AS (
    SELECT
        f.id           AS forecast_id,
        COUNT(*)       AS unlinked_claims
    FROM public.fermi_forecasts f
    JOIN public.forecast_agent_claims c
        ON c.workspace_id = f.workspace_id
    WHERE c.episode_id IS NULL
    GROUP BY f.id
)
SELECT
    f.id                                            AS forecast_id,
    f.question_text,
    f.status,
    f.brier_score,
    f.resolved_at,

    COUNT(e.episode_id)                             AS executions,
    COALESCE(SUM(e.tokens_used), 0)                 AS tokens,

    COALESCE(SUM(e.cost_usd) FILTER (
        WHERE e.cost_basis IN ('measured_split', 'no_charge')
    ), 0)                                           AS attributed_cost_usd,

    COALESCE(SUM(e.cost_usd) FILTER (
        WHERE e.cost_basis IS NULL
           OR e.cost_basis NOT IN ('measured_split', 'no_charge')
    ), 0)                                           AS unattributed_cost_usd,

    COUNT(e.episode_id) FILTER (
        WHERE e.cost_basis IS NULL
           OR e.cost_basis = 'unknown_model'
    )                                               AS untrusted_cost_rows,

    COALESCE(MAX(u.unlinked_claims), 0)             AS unlinked_claims,

    -- The metric this whole chain exists to produce. NULL until resolution,
    -- and NULL when a Brier of 0 would divide by zero. Computed from
    -- attributed spend only — a cost-effectiveness number built on guessed
    -- attribution is worse than no number.
    CASE
        WHEN f.brier_score IS NULL OR f.brier_score = 0 THEN NULL
        ELSE COALESCE(SUM(e.cost_usd) FILTER (
                 WHERE e.cost_basis IN ('measured_split', 'no_charge')
             ), 0) / f.brier_score
    END                                             AS usd_per_brier_point

FROM public.fermi_forecasts f
LEFT JOIN forecast_episodes fe
    ON fe.forecast_id = f.id
LEFT JOIN public.episodes e
    ON e.episode_id = fe.episode_id
LEFT JOIN unlinked u
    ON u.forecast_id = f.id
GROUP BY f.id, f.question_text, f.status, f.brier_score, f.resolved_at;

COMMENT ON VIEW public.forecast_cost_attribution IS
    'Cost per forecast, split into spend we can stand behind and spend we '
    'cannot. attributed_cost_usd requires a measured cost basis (mig-194) and '
    'an exact episode-claim join (mig-195). De-duplicates the one-claim-per-'
    'driver fan-out, so an execution is counted once regardless of how many '
    'drivers it claimed. Check unattributed_cost_usd and unlinked_claims '
    'against it before trusting usd_per_brier_point.';
