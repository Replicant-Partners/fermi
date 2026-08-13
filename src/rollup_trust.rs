//! # Rollup trust contract — the columns that exist but lie
//!
//! Sibling to [`crate::schema_trust`]. That contract asks *"is this column
//! present?"*. This one asks *"is this column telling the truth?"*.
//!
//! ## The failure class
//!
//! `agents.total_executions` and its four siblings (`successful_executions`,
//! `failed_executions`, `total_cost_usd`, `avg_execution_time_ms`) were
//! added with the table and never wired to the execution path. Nothing in
//! the codebase ever ran `UPDATE agents SET total_executions`. Meanwhile
//! `episodes` recorded every run faithfully.
//!
//! The result: 3 of 743 agent rows carried a non-zero rollup while 196
//! agents had real episodes totalling ~$296 of measured spend. Six
//! surfaces read the empty ledger and reported zeros — marketplace
//! pricing, the Dashboard Research card, public profiles, orchestra
//! rosters, the ecology lens, and a deletion-safety guard that was
//! therefore always-true.
//!
//! **Every existing guard passed the whole time.**
//!
//! | Guard | Why it missed this |
//! | --- | --- |
//! | `schema_trust` boot probe | Declares `("agents", "total_executions")` and checks presence. The column was present. |
//! | `SCHEMA_STRICT=1` | Same probe. Nothing to abort on. |
//! | `scripts/lint-schema-consistency.py` | Parses migrations for refs to columns that don't exist. This one existed. |
//! | `scripts/schema_contract_check.sh` | Rebuilds the schema and asserts the contract is satisfiable. It was. |
//! | Type checking | `i32` is `i32` whether it means 253 or 0. |
//! | Unit tests | Fixtures set the counters by hand, so they were never zero in a test. |
//!
//! A column that is present, correctly typed, and permanently zero is
//! invisible to every check that reasons about *shape*. Catching it needs a
//! check that reasons about *content*, which is what this module is.
//!
//! ## The contract
//!
//! Each [`RollupContract`] names a denormalised column, the expression that
//! computes its true value, and a `mismatch_sql` query returning one row
//! with one `bigint` column `mismatches`. Non-zero means the stored value
//! disagrees with the source of truth.
//!
//! A [`RollupContract`] is also how a column gets *retired* honestly: a
//! column declared [`Disposition::WriteOrphaned`] must have **no reader
//! treating it as truth**, which `tests/rollup_contract.rs` enforces by
//! grepping the handlers. Deleting the column later is then a mechanical
//! change rather than an archaeology project.
//!
//! ## Why this isn't in the boot probe
//!
//! `schema_trust::verify` runs on every boot and must stay cheap — it reads
//! `pg_catalog` only. These checks aggregate real tables. They belong in
//! CI and in `scripts/rollup_contract_live.sh`, run against a real
//! database, where a full-table GROUP BY costs nothing anyone is waiting
//! on.

/// What we assert about a denormalised column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The column is written by some code path and must agree with its
    /// source of truth. A mismatch is a bug in the writer.
    Maintained,
    /// Nothing writes the column. It must therefore have **no reader that
    /// treats it as truth** — readers must use `replacement` instead.
    /// Kept in the schema only until the row-mapping change that drops it.
    WriteOrphaned,
}

/// One denormalised column and how to tell whether it's lying.
#[derive(Debug, Clone, Copy)]
pub struct RollupContract {
    /// Table carrying the denormalised column.
    pub table: &'static str,
    /// The column itself.
    pub column: &'static str,
    /// Where the truth actually lives, for the failure message.
    pub source_of_truth: &'static str,
    /// What readers should use instead. Empty for `Maintained` columns.
    pub replacement: &'static str,
    pub disposition: Disposition,
    /// A query returning exactly one row with one `bigint` column named
    /// `mismatches`: the number of rows where the stored value disagrees
    /// with the source of truth.
    ///
    /// For a `WriteOrphaned` column this documents the divergence that
    /// justified retiring it, and is reported as context rather than
    /// asserted to be zero — the whole point is that it is not zero.
    pub mismatch_sql: &'static str,
}

/// Every denormalised column we have an opinion about.
///
/// Extend this when you add a counter that caches something derivable.
/// **Rule of thumb: if you can compute it with a `GROUP BY`, it belongs
/// here** — either as `Maintained` (and then prove your writer keeps it
/// honest) or not at all.
pub const ROLLUP_CONTRACTS: &[RollupContract] = &[
    RollupContract {
        table: "agents",
        column: "total_executions",
        source_of_truth: "COUNT(*) FROM episodes GROUP BY agent_id",
        replacement: "agent_execution_rollup.executions",
        disposition: Disposition::WriteOrphaned,
        mismatch_sql: "SELECT COUNT(*)::bigint AS mismatches \
                         FROM agents a \
                         JOIN agent_execution_rollup r ON r.agent_id = a.agent_id \
                        WHERE a.total_executions <> r.executions",
    },
    RollupContract {
        table: "agents",
        column: "successful_executions",
        source_of_truth: "COUNT(*) FILTER (WHERE execution_status = 'success') FROM episodes",
        replacement: "agent_execution_rollup.successful",
        disposition: Disposition::WriteOrphaned,
        mismatch_sql: "SELECT COUNT(*)::bigint AS mismatches \
                         FROM agents a \
                         JOIN agent_execution_rollup r ON r.agent_id = a.agent_id \
                        WHERE a.successful_executions <> r.successful",
    },
    RollupContract {
        table: "agents",
        column: "failed_executions",
        source_of_truth: "COUNT(*) FILTER (WHERE execution_status = 'failure') FROM episodes",
        replacement: "agent_execution_rollup.failed",
        disposition: Disposition::WriteOrphaned,
        mismatch_sql: "SELECT COUNT(*)::bigint AS mismatches \
                         FROM agents a \
                         JOIN agent_execution_rollup r ON r.agent_id = a.agent_id \
                        WHERE a.failed_executions <> r.failed",
    },
    RollupContract {
        table: "agents",
        column: "total_cost_usd",
        source_of_truth: "SUM(cost_usd) FROM episodes GROUP BY agent_id",
        replacement: "agent_execution_rollup.cost_usd",
        disposition: Disposition::WriteOrphaned,
        // Tolerance of one minor unit: NUMERIC(10,6) summed in a different
        // order can differ in the last place, and a rounding artifact is
        // not the drift we are hunting.
        mismatch_sql: "SELECT COUNT(*)::bigint AS mismatches \
                         FROM agents a \
                         JOIN agent_execution_rollup r ON r.agent_id = a.agent_id \
                        WHERE ABS(a.total_cost_usd - r.cost_usd) > 0.000001",
    },
    RollupContract {
        table: "agents",
        column: "avg_execution_time_ms",
        source_of_truth: "AVG(execution_time_ms) FROM episodes GROUP BY agent_id",
        replacement: "agent_execution_rollup.avg_execution_time_ms",
        disposition: Disposition::WriteOrphaned,
        mismatch_sql: "SELECT COUNT(*)::bigint AS mismatches \
                         FROM agents a \
                         JOIN agent_execution_rollup r ON r.agent_id = a.agent_id \
                        WHERE a.avg_execution_time_ms <> r.avg_execution_time_ms",
    },
];

/// Source files whose SQL and Rust are scanned for reads of a
/// [`Disposition::WriteOrphaned`] column. These are the request-serving
/// surfaces: if one of them reads a dead counter, a user sees a wrong
/// number.
///
/// Deliberately excludes:
///   * `agent-bestiary/memory/` — the row mapper must keep naming the
///     columns until they are dropped from the table.
///   * `migrations/` — the columns are declared and commented there.
///   * `src/schema_trust.rs` — declares presence, not truth.
///   * `src/rollup_trust.rs` — this contract names them by definition.
///   * `scripts/` — diagnostics and seed fixtures, not user-facing reads.
pub const SCANNED_ROOTS: &[&str] = &["src/handlers", "src/api", "src/apps"];

/// Readers of a write-orphaned column that are allowed to remain, with the
/// reason. Anything not on this list is a contract violation.
///
/// `(file_suffix, column, why)`
pub const READER_EXEMPTIONS: &[(&str, &str, &str)] = &[
    (
        "src/handlers/agents.rs",
        "total_executions",
        "build_agent_json's `None` branch reports the row counters when no \
         rollup was loaded, tagged `source: \"agents_row\"` so a consumer \
         can tell an unmeasured zero from a real one. Not a truth claim.",
    ),
    (
        "src/handlers/agents.rs",
        "successful_executions",
        "Same `source: \"agents_row\"` fallback branch.",
    ),
    (
        "src/handlers/agents.rs",
        "failed_executions",
        "Same `source: \"agents_row\"` fallback branch.",
    ),
    (
        "src/handlers/agents.rs",
        "total_cost_usd",
        "Same `source: \"agents_row\"` fallback branch.",
    ),
    (
        "src/handlers/agents.rs",
        "avg_execution_time_ms",
        "Same `source: \"agents_row\"` fallback branch.",
    ),
];

/// Is this file allowed to read this write-orphaned column?
pub fn reader_is_exempt(path: &str, column: &str) -> bool {
    READER_EXEMPTIONS
        .iter()
        .any(|(suffix, col, _)| path.ends_with(suffix) && *col == column)
}

/// Columns that must not be read as truth by [`SCANNED_ROOTS`].
pub fn write_orphaned_columns() -> impl Iterator<Item = &'static RollupContract> {
    ROLLUP_CONTRACTS
        .iter()
        .filter(|c| c.disposition == Disposition::WriteOrphaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_orphan_names_its_replacement() {
        for c in write_orphaned_columns() {
            assert!(
                !c.replacement.is_empty(),
                "{}.{} is write-orphaned but doesn't say what to use instead \
                 — the next person to hit this needs the answer here, not in \
                 a commit message",
                c.table,
                c.column
            );
            assert!(
                !c.source_of_truth.is_empty(),
                "{}.{} must name where the truth lives",
                c.table,
                c.column
            );
        }
    }

    #[test]
    fn mismatch_queries_return_the_expected_shape() {
        for c in ROLLUP_CONTRACTS {
            let sql = c.mismatch_sql.to_lowercase();
            assert!(
                sql.contains("as mismatches"),
                "{}.{}: mismatch_sql must alias its count as `mismatches`",
                c.table,
                c.column
            );
            assert!(
                sql.starts_with("select"),
                "{}.{}: mismatch_sql must be a bare SELECT — the harness runs \
                 it read-only against production",
                c.table,
                c.column
            );
            for forbidden in ["insert", "update", "delete", "drop", "alter", "truncate"] {
                assert!(
                    !sql.contains(forbidden),
                    "{}.{}: mismatch_sql must not contain `{}` — this runs \
                     against a live database",
                    c.table,
                    c.column,
                    forbidden
                );
            }
        }
    }

    #[test]
    fn exemptions_reference_declared_columns() {
        for (path, column, why) in READER_EXEMPTIONS {
            assert!(
                ROLLUP_CONTRACTS.iter().any(|c| c.column == *column),
                "exemption for {path} names `{column}`, which is not a \
                 declared rollup column"
            );
            assert!(
                why.len() > 30,
                "exemption for {path}.{column} needs a real justification, \
                 not `{why}` — an unexplained exemption is how the contract \
                 rots"
            );
        }
    }

    #[test]
    fn reader_exemption_matching_is_path_suffixed() {
        assert!(reader_is_exempt(
            "/home/x/fermi/src/handlers/agents.rs",
            "total_executions"
        ));
        assert!(!reader_is_exempt(
            "/home/x/fermi/src/handlers/profile.rs",
            "total_executions"
        ));
        assert!(
            !reader_is_exempt("/home/x/fermi/src/handlers/agents.rs", "brier_score"),
            "exemptions are per (file, column), not per file"
        );
    }
}
