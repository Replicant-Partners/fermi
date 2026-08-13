//! # Agent economics — measured, not remembered
//!
//! One definition of "how much has this agent run, and what did it cost".
//!
//! ## The drift this module exists to prevent
//!
//! `agents` carries five denormalised counters — `total_executions`,
//! `successful_executions`, `failed_executions`, `total_cost_usd`,
//! `avg_execution_time_ms` — added with the table and never wired to the
//! execution path. There is no `UPDATE agents SET total_executions`
//! anywhere in the codebase. At the time this module was written, 3 of
//! 743 agent rows carried a non-zero rollup while 196 agents had real
//! episodes totalling ~$296 of measured spend.
//!
//! Everything that read those columns therefore read a permanent zero:
//! the console marketplace ranked every agent as never-executed and could
//! price none of them, the Dashboard Research card showed "cost n/a"
//! against forecasts that had really cost money, public profiles and
//! orchestra rosters reported 0 runs for everyone, the ecology lens
//! reported 0 runs for the whole population, and
//! `admin_cleanup_test_cruft_handler` used `total_executions = 0` as a
//! deletion-safety criterion — a predicate that was always true.
//!
//! Note the failure mode: **absence would have been caught, emptiness was
//! not.** `schema_trust` declares `("agents", "total_executions")` and
//! would have refused to boot if the column vanished. A column that is
//! present and always zero passes every existence check there is. That is
//! why [`crate::rollup_trust`] exists as a separate contract.
//!
//! ## The source of truth
//!
//! `episodes` — one row per run, written by the execution path, carrying
//! `cost_usd`, `tokens_used`, `execution_time_ms` and `execution_status`.
//! `migrations/192_agent_execution_rollup.sql` exposes the aggregate as
//! the `agent_execution_rollup` view so SQL consumers can `LEFT JOIN` it
//! instead of hand-rolling a sixth copy of the same GROUP BY.
//!
//! **Every** consumer — Rust or SQL — goes through the view. Two copies of
//! an aggregate definition is how "successful runs only" creeps into one
//! of them and the platform starts quietly under-reporting spend.

use std::collections::HashMap;

use sqlx::{PgPool, Row};
use uuid::Uuid;

/// The view that defines agent economics. Named once so a rename shows up
/// as a compile-time grep rather than a runtime 500 in five handlers.
pub const ROLLUP_VIEW: &str = "agent_execution_rollup";

/// Execution stats measured from `episodes` via [`ROLLUP_VIEW`].
///
/// An agent with no episodes is *absent* from the rollup, not present with
/// zeros. Callers should render a missing entry as "never ran" — which is
/// the truth — via [`MeasuredExecStats::default`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MeasuredExecStats {
    pub executions: i64,
    pub successful: i64,
    pub failed: i64,
    pub cost_usd: f64,
    pub tokens_used: i64,
    pub avg_execution_time_ms: i64,
    /// Runs with no `cost_usd` recorded. Non-zero means [`Self::cost_usd`]
    /// is a partial sum; a caller presenting spend should say so rather
    /// than pass it off as complete.
    pub episodes_missing_cost: i64,
}

impl MeasuredExecStats {
    /// Mean USD cost of one run, or `None` when there is nothing to
    /// divide.
    ///
    /// Returning `None` rather than `0.0` is load-bearing: a zero renders
    /// as "$0.00/run", which reads as *free* rather than *unknown*. The
    /// console's marketplace and Research card both branch on this to show
    /// "cost n/a" instead.
    pub fn avg_cost_per_run(&self) -> Option<f64> {
        (self.executions > 0 && self.cost_usd > 0.0).then(|| self.cost_usd / self.executions as f64)
    }

    /// Share of runs that succeeded, in `[0, 1]`. `None` when the agent
    /// has never run — distinct from a measured 0% success rate, which is
    /// a real and alarming number.
    pub fn success_rate(&self) -> Option<f64> {
        (self.executions > 0).then(|| self.successful as f64 / self.executions as f64)
    }
}

/// Batch-load measured stats for a set of agents. One query regardless of
/// how many ids are passed; agents with no episodes are absent from the
/// map.
///
/// Errors are swallowed to an empty map on purpose: these stats decorate a
/// listing, and a catalogue that 500s because a rollup was slow is worse
/// than one that renders without cost figures. Absence is visible in the
/// UI (`cost n/a`) rather than silent.
pub async fn measured_exec_stats(
    pool: &PgPool,
    agent_ids: &[Uuid],
) -> HashMap<Uuid, MeasuredExecStats> {
    if agent_ids.is_empty() {
        return HashMap::new();
    }
    // `cost_usd` is a SUM over DECIMAL(10,6), so it arrives as NUMERIC.
    // Cast in SQL rather than probing three Rust types on the way out
    // (cf. `handlers::economics::f64_of`, which exists because an un-cast
    // COALESCE over this very column decodes as NUMERIC or BIGINT
    // depending on the branch taken).
    let sql = format!(
        "SELECT agent_id, executions, successful, failed,
                cost_usd::float8 AS cost_usd, tokens_used,
                avg_execution_time_ms, episodes_missing_cost
           FROM {ROLLUP_VIEW}
          WHERE agent_id = ANY($1)"
    );
    sqlx::query(&sql)
        .bind(agent_ids)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            let id: Uuid = r.try_get("agent_id").ok()?;
            Some((
                id,
                MeasuredExecStats {
                    executions: r.try_get("executions").unwrap_or(0),
                    successful: r.try_get("successful").unwrap_or(0),
                    failed: r.try_get("failed").unwrap_or(0),
                    cost_usd: r.try_get("cost_usd").unwrap_or(0.0),
                    tokens_used: r.try_get("tokens_used").unwrap_or(0),
                    avg_execution_time_ms: r.try_get("avg_execution_time_ms").unwrap_or(0),
                    episodes_missing_cost: r.try_get("episodes_missing_cost").unwrap_or(0),
                },
            ))
        })
        .collect()
}

/// Measured stats for a single agent. Convenience over
/// [`measured_exec_stats`]; returns `None` when the agent has never run.
pub async fn measured_exec_stats_one(pool: &PgPool, agent_id: Uuid) -> Option<MeasuredExecStats> {
    measured_exec_stats(pool, &[agent_id])
        .await
        .remove(&agent_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_ran_has_no_price_and_no_success_rate() {
        let none = MeasuredExecStats::default();
        assert_eq!(none.avg_cost_per_run(), None);
        assert_eq!(none.success_rate(), None);
    }

    #[test]
    fn failed_runs_are_still_billable() {
        // efra_critical_factor's real numbers when this landed: three
        // runs, every one an error, $1.032012 spent.
        let all_failed = MeasuredExecStats {
            executions: 3,
            successful: 0,
            failed: 3,
            cost_usd: 1.032012,
            ..Default::default()
        };
        let avg = all_failed
            .avg_cost_per_run()
            .expect("an agent whose every run failed has still spent money");
        assert!((avg - 0.344004).abs() < 1e-9, "got {avg}");
        assert_eq!(
            all_failed.success_rate(),
            Some(0.0),
            "0% measured is a real number, not missing data"
        );
    }

    #[test]
    fn zero_cost_is_unknown_not_free() {
        // Local/self-hosted models record cost 0.0 (see
        // agent_backend::registry::calculate_cost). Pricing those at
        // "$0.00/run" would claim they are free rather than unpriced.
        let free_looking = MeasuredExecStats {
            executions: 10,
            successful: 10,
            cost_usd: 0.0,
            ..Default::default()
        };
        assert_eq!(free_looking.avg_cost_per_run(), None);
        assert_eq!(free_looking.success_rate(), Some(1.0));
    }

    #[test]
    fn partial_cost_coverage_is_reported() {
        let partial = MeasuredExecStats {
            executions: 10,
            cost_usd: 5.0,
            episodes_missing_cost: 4,
            ..Default::default()
        };
        assert!(
            partial.episodes_missing_cost > 0,
            "callers need to know the sum is partial"
        );
        // The average is still over ALL runs, not just priced ones — an
        // agent with unpriced runs is cheaper per run on average, and
        // pretending otherwise would overstate spend.
        assert_eq!(partial.avg_cost_per_run(), Some(0.5));
    }
}
