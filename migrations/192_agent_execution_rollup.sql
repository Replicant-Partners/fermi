-- ─────────────────────────────────────────────────────────────────────
-- 192 — agent_execution_rollup: one definition of agent economics
-- ─────────────────────────────────────────────────────────────────────
--
-- ## The drift this closes
--
-- `agents` carries five denormalised counters, added with the table in
-- 001 and never wired to the execution path:
--
--   total_executions, successful_executions, failed_executions,
--   total_cost_usd, avg_execution_time_ms
--
-- Nothing in the codebase writes them. There is no `UPDATE agents SET
-- total_executions`. At the time this migration was written, 3 of 743
-- agent rows had a non-zero rollup, while 196 agents had real episodes
-- totalling ~$296 of measured spend.
--
-- Meanwhile `episodes` — which the execution path DOES write, on every
-- run — carries `cost_usd`, `tokens_used`, `execution_time_ms` and
-- `execution_status` per run. The ABW web UI's EXECUTION HISTORY panel
-- reads it and has been showing correct per-run cost the whole time.
--
-- So the platform had two ledgers and trusted the empty one. Every
-- consumer of the `agents` counters read a permanent zero:
--
--   * the console marketplace ranked all agents as never-executed and
--     could not price a single one ("cost n/a" across the board);
--   * the Dashboard Research card could not cost a forecast's research;
--   * public profiles and orchestra rosters reported 0 runs for
--     everyone;
--   * the ecology lens reported 0 runs for the whole population;
--   * `admin_cleanup_test_cruft_handler` used `total_executions = 0` as
--     a DELETION SAFETY criterion — a guard that was always true, i.e.
--     no guard at all.
--
-- ## Why a view and not a backfill
--
-- Backfilling the counters would create a second thing to keep in sync
-- and a new way to drift: the counters would be right on the day of the
-- backfill and wrong forever after, which is strictly worse than
-- visibly zero. A view has exactly one definition and cannot go stale.
--
-- Deliberately a plain VIEW, not a MATERIALIZED VIEW. A matview needs a
-- refresh path, and an unrefreshed matview is the same silent-staleness
-- bug wearing a different hat (cf. `fermi_leaderboard`, which needs
-- `refresh_fermi_leaderboard()` and is declared in SCHEMA_FUNCTIONS
-- precisely so the refresh can't be forgotten). `episodes` is indexed on
-- `(agent_id, timestamp_ref DESC)` and consumers scope to a page of
-- agents, so the GROUP BY is cheap. If it ever isn't, promote to a
-- matview WITH a refresh function and add both to the schema contract.
--
-- ## Why the counters aren't dropped here
--
-- `agents.total_executions` is declared in `SCHEMA_COLUMNS`
-- (src/schema_trust.rs) and appears in `Agent` (memory/src/types.rs),
-- whose SELECT list names all five. Dropping them is a follow-up that
-- touches the memory crate's row mapping. Until then they are marked
-- deprecated below, and `tests/rollup_contract.rs` fails if any handler
-- starts reading them as truth again.

-- No BEGIN/COMMIT: this deploys through PgBouncer, which manages
-- transactions itself, and an explicit transaction block fails there.
-- (`scripts/lint-migrations.sh` rejects it; see memory/MEMORY.md →
-- PgBouncer Pitfalls.) Safe to run unwrapped and safe to re-run: every
-- statement below is idempotent — `CREATE OR REPLACE VIEW` and `COMMENT
-- ON` both overwrite rather than conflict — so there is no partial state
-- worth rolling back.

CREATE OR REPLACE VIEW agent_execution_rollup AS
SELECT
    e.agent_id,
    COUNT(*)::bigint                                          AS executions,
    COUNT(*) FILTER (WHERE e.execution_status = 'success')::bigint
                                                              AS successful,
    COUNT(*) FILTER (WHERE e.execution_status = 'failure')::bigint
                                                              AS failed,
    -- Kept NUMERIC here (SUM over DECIMAL(10,6)); consumers cast at the
    -- edge. Failed runs are included on purpose: a run that burned
    -- tokens and returned an error still cost real money, and pricing
    -- off successes alone under-reports exactly the agents that are
    -- wasting budget.
    COALESCE(SUM(e.cost_usd), 0)                              AS cost_usd,
    COALESCE(SUM(e.tokens_used), 0)::bigint                   AS tokens_used,
    COALESCE(AVG(e.execution_time_ms), 0)::bigint             AS avg_execution_time_ms,
    -- Episodes with no cost recorded. A caller showing spend needs to
    -- know how much of the population it is missing, rather than
    -- presenting a partial sum as a total.
    COUNT(*) FILTER (WHERE e.cost_usd IS NULL)::bigint        AS episodes_missing_cost,
    MIN(e.timestamp_ref)                                      AS first_run_at,
    MAX(e.timestamp_ref)                                      AS last_run_at
FROM episodes e
GROUP BY e.agent_id;

COMMENT ON VIEW agent_execution_rollup IS
    'Measured agent economics, derived from episodes (the write-time record '
    'of every run). THE source of truth for run counts, cost and latency. '
    'Do not read agents.total_executions / total_cost_usd / '
    'successful_executions / failed_executions / avg_execution_time_ms — '
    'nothing writes them; see migrations/192 and tests/rollup_contract.rs.';

-- Mark the dead counters in the catalog itself, so someone reading
-- `\d agents` at 3am sees it without having to find this file.
COMMENT ON COLUMN agents.total_executions IS
    'DEPRECATED / WRITE-ORPHANED — nothing updates this. Use '
    'agent_execution_rollup.executions. See migrations/192.';
COMMENT ON COLUMN agents.successful_executions IS
    'DEPRECATED / WRITE-ORPHANED — nothing updates this. Use '
    'agent_execution_rollup.successful. See migrations/192.';
COMMENT ON COLUMN agents.failed_executions IS
    'DEPRECATED / WRITE-ORPHANED — nothing updates this. Use '
    'agent_execution_rollup.failed. See migrations/192.';
COMMENT ON COLUMN agents.total_cost_usd IS
    'DEPRECATED / WRITE-ORPHANED — nothing updates this. Use '
    'agent_execution_rollup.cost_usd. See migrations/192.';
COMMENT ON COLUMN agents.avg_execution_time_ms IS
    'DEPRECATED / WRITE-ORPHANED — nothing updates this. Use '
    'agent_execution_rollup.avg_execution_time_ms. See migrations/192.';
