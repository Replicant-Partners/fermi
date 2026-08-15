-- Migration 196: make delegated executions visible in the cost ledger.
--
-- ## What was missing
--
-- `execute_delegate_to_agent` and `execute_execute_agent`
-- (`src/agent_backend/tools_legacy.rs`) ran a child agent, read its
-- `metadata.reasoning` and `evidence`, posted a workspace message — and then
-- **discarded the whole `AgentOutput`**. No episode was written. So a delegated
-- run's tokens, cost, provider and model were not mis-attributed; they did not
-- exist. Nothing in the platform could see the spend.
--
-- Two consequences for the numbers migrations 194 and 195 exist to produce:
--
--   1. **Compound agents under-report their true cost by their fan-out.** A
--      coordinator delegating to three members recorded only the coordinator's
--      own tokens (`tool_executor.rs:412` sums one loop). Forecast research runs
--      almost entirely through compound agents, so this is not an edge case —
--      it is the common path.
--   2. **Sub-agents earned no economic record at all.** No cost, no episode, no
--      route to Brier credit. An agent that only ever runs as a delegate was
--      invisible to attribution, which is the opposite of what a marketplace
--      built on per-agent performance requires.
--
-- ## Why a child episode rather than folding tokens into the parent
--
-- Folding the child's tokens into the parent would make the parent's *total*
-- correct and destroy the attribution: you could not say which member cost what,
-- and the member could not be credited or paid. Since per-agent economic
-- attribution is the point, each delegated run now writes its OWN episode,
-- priced by the same `AgentOutput::cost()` path as any other, linked to its
-- caller by `parent_episode_id`.
--
-- The parent therefore keeps reporting only its own tokens. That is correct and
-- deliberate: total cost is the SUM OVER THE TREE, never the root's own figure.
-- `forecast_cost_attribution` below is updated to walk it.
--
-- ## Scope note
--
-- This makes delegated spend MEASURABLE. It does not bill it — delegation is
-- still unpriced (no charge call), and whether it should be charged is the open
-- pricing question. Measuring first is deliberate: you cannot set a price for
-- something you cannot see.
--
-- Additive: one nullable column, one index, one view redefinition.

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS parent_episode_id UUID;

COMMENT ON COLUMN public.episodes.parent_episode_id IS
    'Episode of the agent that delegated this run. NULL = a root execution '
    '(directly invoked). Total cost of a compound execution is the sum over '
    'the tree, never the root row alone. No FK: parent and child are written '
    'by different code paths and a missing parent must not discard a real '
    'cost record.';

-- Supports the recursive descent below, and "what did this run spawn?".
CREATE INDEX IF NOT EXISTS idx_episodes_parent
    ON public.episodes(parent_episode_id)
    WHERE parent_episode_id IS NOT NULL;

-- ─── forecast_cost_attribution: now sums over the delegation tree ────────────
--
-- A delegated child has no claim of its own — sub-agents do research, they
-- rarely emit a `[MULTIPLIER]` — so it cannot reach a forecast the way its
-- parent does (mig-197, via `forecast_agent_claims.episode_id`). It reaches one
-- by DESCENT: a run belongs to whichever forecast its nearest claiming ancestor
-- belongs to.
--
-- Without the recursive step this view silently under-reports every compound
-- execution, which is most of them — the failure mode being fixed here.
--
-- The `DISTINCT (forecast_id, episode_id)` de-duplication from mig-197 still
-- matters and still applies: the claim table fans out one row per driver, so a
-- single execution can be reached by several claims. It now has to survive a
-- second fan-out (one root can have many descendants), which is why the
-- recursion collects episode ids into a set BEFORE any cost is summed.
-- DROP + CREATE, for the same reason migration 197 does: migrations re-run on
-- every boot in file order, so 197 recreates this view in its 3-column shape
-- moments before this file recreates it with `delegated_executions`. Replacing
-- in place would fail in one direction or the other depending on boot order.
-- Safe because nothing depends on this view.
DROP VIEW IF EXISTS public.forecast_cost_attribution;
CREATE VIEW public.forecast_cost_attribution AS
WITH RECURSIVE claimed_roots AS (
    -- Executions a forecast can claim directly (mig-197's exact join).
    SELECT DISTINCT
        f.id         AS forecast_id,
        c.episode_id AS episode_id
    FROM public.fermi_forecasts f
    JOIN public.forecast_agent_claims c
        ON c.workspace_id = f.workspace_id
    WHERE c.episode_id IS NOT NULL
),
forecast_tree AS (
    -- Base: the claimed executions themselves, at depth 0.
    SELECT forecast_id, episode_id, 0 AS depth
    FROM claimed_roots

    UNION      -- UNION, not UNION ALL: set semantics collapse any diamond
               -- (two claims reaching the same episode) rather than
               -- double-counting its subtree's cost.

    -- Step: everything those executions delegated to, transitively.
    SELECT t.forecast_id, e.episode_id, t.depth + 1
    FROM forecast_tree t
    JOIN public.episodes e
        ON e.parent_episode_id = t.episode_id
    -- Depth guard. `with_workspace_no_delegation` currently caps real depth at
    -- 2, but a cycle introduced by bad data must not hang the view.
    WHERE t.depth < 10
),
forecast_episodes AS (
    SELECT DISTINCT forecast_id, episode_id FROM forecast_tree
),
unlinked AS (
    -- Claims with no episode_id (pre-195). Real spend, unlocatable.
    SELECT f.id AS forecast_id, COUNT(*) AS unlinked_claims
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

    CASE
        WHEN f.brier_score IS NULL OR f.brier_score = 0 THEN NULL
        ELSE COALESCE(SUM(e.cost_usd) FILTER (
                 WHERE e.cost_basis IN ('measured_split', 'no_charge')
             ), 0) / f.brier_score
    END                                             AS usd_per_brier_point,

    -- Surfaced so a reader can see how much of a forecast's work happened
    -- below the agents that were directly invoked. A zero here on a compound
    -- run means delegated episodes are not being written or not being linked.
    COUNT(e.episode_id) FILTER (
        WHERE e.parent_episode_id IS NOT NULL
    )                                               AS delegated_executions

FROM public.fermi_forecasts f
LEFT JOIN forecast_episodes fe
    ON fe.forecast_id = f.id
LEFT JOIN public.episodes e
    ON e.episode_id = fe.episode_id
LEFT JOIN unlinked u
    ON u.forecast_id = f.id
GROUP BY f.id, f.question_text, f.status, f.brier_score, f.resolved_at;

COMMENT ON VIEW public.forecast_cost_attribution IS
    'Cost per forecast, summed over the DELEGATION TREE (mig-198) and counting '
    'only spend with a measured cost basis (mig-194) reached by an exact '
    'episode-claim join (mig-197). De-duplicates both fan-outs: one claim per '
    'driver, and one parent to many delegated children. Check '
    'unattributed_cost_usd and unlinked_claims before trusting '
    'usd_per_brier_point.';
