-- ═══════════════════════════════════════════════════════════════════════
-- 200 — coordinator_observation: Loop 3 writes into member agents' memory
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT LOOP 3 ACTUALLY DOES
-- -------------------------
-- The coordination strategist's output is not a file. `cohere_and_coordinate`
-- observes how a member behaved in a session — what coherence role it played,
-- where it duplicated another agent, which evidence it left unengaged — and
-- writes that observation into **that member's episodic memory**. The member
-- picks it up on its next consolidation cycle, distils it into a semantic rule,
-- and carries it into every subsequent execution through KG injection.
--
-- That is what makes Loop 3 adaptive rather than advisory: the correction
-- changes the agent's memory, not just the next few turns of one conversation.
-- It is Loop 3 → Loop 1 cascade, and it is why the strategist's observations
-- must be episodes rather than a `_coordination/brief.md` that dreaming cannot
-- read.
--
-- WHY A NEW PROVENANCE RATHER THAN A TAG
-- --------------------------------------
-- `episodes` is documented as "one row per run" and is the measurement base
-- for agent economics (mig-192, `agent_execution_rollup`). A coordination
-- observation is **not a run**: no model was invoked on that agent's behalf, no
-- tokens were spent by it, and it has no execution outcome. Writing one as
-- `auto_pass` would make every observation look like an execution and inflate
-- `executions`, `avg_execution_time_ms`, and the denominator of every
-- cost-per-run figure the platform reports.
--
-- Tags cannot fix that: the rollup does not read tags, and asking every future
-- consumer to remember a tag filter is how the distinction gets lost. A
-- provenance value is checked by the database and visible to every reader.
--
-- FIXES A PRE-EXISTING INFLATION TOO
-- ----------------------------------
-- The same argument already applied to `synthetic_correction`, and nobody had
-- made it: a HITL correction is authored by a human, consumed no provider
-- tokens (`two_write.rs` sets `input_tokens: None`, `cost_usd: None`
-- deliberately), and was still counted by the rollup as an agent execution.
-- The corrected view excludes both.
--
-- The exclusion is by provenance, not by cost, on purpose. Failed runs stay
-- counted — mig-192 argues that explicitly, and it is right: a run that burned
-- tokens and errored still cost real money. What is being removed here is rows
-- that were never runs at all.
-- ═══════════════════════════════════════════════════════════════════════

-- ── 1. Allow the new provenance value ──────────────────────────────────
DO $$
BEGIN
    ALTER TABLE public.episodes DROP CONSTRAINT IF EXISTS episodes_provenance_check;
    ALTER TABLE public.episodes ADD CONSTRAINT episodes_provenance_check
        CHECK (provenance IN (
            'auto_pass',                -- evaluator registry passed
            'auto_fail',                -- evaluator registry failed (no human seen)
            'human_approved',           -- HITL reviewer confirmed verdict
            'human_relabeled',          -- HITL reviewer corrected dimension scores
            'human_corrected',          -- HITL reviewer ran full intervention
            'synthetic_correction',     -- second write: synthetic corrected episode
            'coordinator_observation'   -- Loop 3: strategist's note in a member's memory
        ));
END $$;

-- ── 2. Keep economics measuring runs, not memory writes ────────────────
--
-- Wrapped in a DO block like the constraint above. The repo's migration lint
-- flags bare DDL here because these run at boot against whatever DATABASE_URL
-- is configured, which on Neon is the pooled host: utility statements issued
-- outside a DO block can land on a different backend under PgBouncer
-- transaction pooling and fail. A from-empty deploy would then silently not
-- get the corrected view.
DO $$
BEGIN
    EXECUTE $view$
        CREATE OR REPLACE VIEW agent_execution_rollup AS
        SELECT
            e.agent_id,
            COUNT(*)::bigint                                          AS executions,
            COUNT(*) FILTER (WHERE e.execution_status = 'success')::bigint
                                                                      AS successful,
            COUNT(*) FILTER (WHERE e.execution_status = 'failure')::bigint
                                                                      AS failed,
            -- Kept NUMERIC here (SUM over DECIMAL(10,6)); consumers cast at
            -- the edge. Failed runs are included on purpose: a run that burned
            -- tokens and returned an error still cost real money, and pricing
            -- off successes alone under-reports exactly the agents that are
            -- wasting budget.
            COALESCE(SUM(e.cost_usd), 0)                              AS cost_usd,
            COALESCE(SUM(e.tokens_used), 0)::bigint                   AS tokens_used,
            COALESCE(AVG(e.execution_time_ms), 0)::bigint             AS avg_execution_time_ms,
            COUNT(*) FILTER (WHERE e.cost_usd IS NULL)::bigint        AS episodes_missing_cost,
            MIN(e.timestamp_ref)                                      AS first_run_at,
            MAX(e.timestamp_ref)                                      AS last_run_at
        FROM episodes e
        -- Rows that are memory writes rather than executions. Both are
        -- authored by someone other than the agent — a human reviewer, or the
        -- coordination strategist — and neither invoked a model on this
        -- agent's behalf.
        WHERE e.provenance NOT IN ('coordinator_observation', 'synthetic_correction')
        GROUP BY e.agent_id
    $view$;

    EXECUTE $cmt$
        COMMENT ON VIEW agent_execution_rollup IS
            'Measured agent economics, derived from episodes (the write-time '
            'record of a run). Excludes coordinator_observation and '
            'synthetic_correction: both are writes INTO an agent''s memory by '
            'another party, not runs BY the agent, and counting them inflates '
            'execution counts and deflates cost-per-run. Failed runs are '
            'included deliberately — see mig-192.'
    $cmt$;
END $$;
