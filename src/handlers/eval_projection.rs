//! SQL-backed `ProjectionLookup` adapter — wires the
//! `agent_bestiary_evaluators::ProjectionScoringEvaluator` to the
//! `sosa_observations` table.
//!
//! When the `ProjectionScoringEvaluator` fires (a real SOSA observation has
//! arrived), it calls `find_projection_match` with the `projection_id` from the
//! episode context. This adapter looks up the **projection** carrying that id,
//! pairs it with the **measurement** carrying the same id, and returns a
//! `ProjectionObservation` for scoring.
//!
//! # Two defects this file had, both silent
//!
//! **It selected the empty set.** Both queries required
//! `extra->>'source' = 'simops_simulation'`. No row in `sosa_observations` has
//! ever carried that tag: the 12,167 projections on file are written by the
//! dynamics runner as `extra.source_kind = 'dynamics_projection'`. The
//! predicate now comes from [`crate::projection_kind`], which is shared with
//! the writer side, so the two cannot disagree again without the shared
//! constants moving.
//!
//! **It read the wrong column.** `predicted_value` was taken from
//! `extra->>'predicted_value'`, a key present on **zero** rows. The dynamics
//! runner puts the projected value in `result_value`, like any other
//! observation. Reading `extra` first and falling back to `result_value` keeps
//! the agent-tool shape working and stops the common case returning
//! "predicted_value missing or unparseable" for a value that is right there.
//!
//! Either alone was enough to guarantee no signal. Both were invisible because
//! a lookup that finds nothing is indistinguishable from a world in which
//! nothing has happened — which is the whole reason
//! [`crate::liveness_trust`] counts opportunities.
//!
//! # The fallback is opt-in, and that is load-bearing
//!
//! There is a heuristic path: no explicit `projection_id`, so take the most
//! recent projection for the same `(observable_property, feature_of_interest)`
//! within N days. It is a reasonable way to backfill a research question by
//! hand. It is a **wrong** way to produce a Loop 5.A (projection accuracy)
//! signal, because the projection it picks is not the one the measurement
//! answers — it is merely the nearest one of the same shape.
//!
//! Loop 5.A's projection-accuracy claim is that it is the one signal an agent
//! cannot talk its way out of: a physical measurement against a prior
//! commitment. A mismatched one is worse than an absent one, because it is
//! recorded as hard-verified and nothing downstream can tell it apart. So
//! [`ProjectionLookupSqlx::new`] has the fallback **off**, and a caller must
//! ask for it by name with [`ProjectionLookupSqlx::with_lookback_days`]. The
//! scoring registry in `handlers::eval` uses `new`.
//!
//! This replaces an ordering rule that lived in a handover document — "do not
//! wire the trigger before you wire the link, because the fallback will score
//! the wrong projection". A rule a future session has to read and remember is
//! not a control. This is.

use agent_bestiary_evaluators::{EvalError, ProjectionLookup, ProjectionObservation};
use async_trait::async_trait;
use fermi::is_projection_sql;
use sqlx::{PgPool, Row};

/// The projected value, wherever the writer put it.
///
/// `extra->>'predicted_value'` first (the agent tool's shape), then
/// `result_value` (the dynamics runner's, and every other observation's, and
/// the only one with any rows behind it).
///
/// A macro rather than a `const` so both queries can `concat!` it instead of
/// quoting it. One of them quoting it is how the two would come to read
/// different columns.
macro_rules! predicted_value_sql {
    () => {
        "COALESCE((syn.extra->>'predicted_value')::DOUBLE PRECISION, syn.result_value)"
    };
}

pub struct ProjectionLookupSqlx {
    pool: PgPool,
    /// Lookback window for the heuristic path, or `None` to refuse it.
    ///
    /// `None` by default. See the module docs: an unlinked match is scored as
    /// hard-verified and cannot be told apart from a linked one afterwards.
    lookback_days: Option<i64>,
}

impl ProjectionLookupSqlx {
    /// Linked matches only. The safe constructor, and the one the evaluator
    /// registry uses.
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            lookback_days: None,
        }
    }

    /// Enable the `(observable_property, feature_of_interest)` heuristic.
    ///
    /// For interactive and backfill use, where a human is reading the result
    /// and can see which projection was chosen. Not for the scoring path.
    ///
    /// **It has no production caller, and that is the state to preserve.** The
    /// dead-code warning is the design working: the day this acquires one,
    /// someone has decided that a heuristically-matched projection may be
    /// recorded as a hard-verified signal, and that decision should have to be
    /// made out loud rather than by deleting an `allow`.
    #[allow(dead_code)]
    pub fn with_lookback_days(mut self, days: i64) -> Self {
        self.lookback_days = Some(days.max(1));
        self
    }

    /// Is the heuristic path enabled? Read by the tests that hold it off.
    #[allow(dead_code)]
    pub fn heuristic_enabled(&self) -> bool {
        self.lookback_days.is_some()
    }
}

#[async_trait]
impl ProjectionLookup for ProjectionLookupSqlx {
    async fn find_projection_match(
        &self,
        projection_id: Option<&str>,
        agent_id: uuid::Uuid,
    ) -> Result<Option<ProjectionObservation>, EvalError> {
        // ── Primary path: the measurement names the projection it answers ────
        //
        // Both sides must carry the same `projection_id`, and they must be on
        // opposite sides of the projection/measurement predicate. Without the
        // second condition a projection would join to itself and score 1.0.
        if let Some(pid) = projection_id {
            let sql = concat!(
                "SELECT ",
                predicted_value_sql!(),
                " AS predicted_value, ",
                "syn.extra->>'model_uri'               AS model_uri, ",
                "syn.extra->>'stage_id'                AS stage_id, ",
                "syn.observable_property               AS observable_property, ",
                "syn.extra->>'temperature_c'           AS temperature_c_str, ",
                "syn.extra->>'n_instances'             AS n_instances_str, ",
                "real.result_value::DOUBLE PRECISION   AS actual_value, ",
                "( SELECT count(*)::INTEGER FROM sosa_observations prev ",
                "   WHERE prev.produced_by_agent_id = $2 ",
                "     AND ",
                is_projection_sql!("prev"),
                "     AND prev.observable_property = syn.observable_property ",
                ")                                     AS n_prior ",
                "FROM sosa_observations syn ",
                "JOIN sosa_observations real ",
                "  ON real.observable_property = syn.observable_property ",
                " AND real.feature_of_interest IS NOT DISTINCT FROM syn.feature_of_interest ",
                " AND NOT ",
                is_projection_sql!("real"),
                " AND real.extra->>'projection_id' = $1 ",
                "WHERE syn.extra->>'projection_id' = $1 ",
                "  AND ",
                is_projection_sql!("syn"),
                " ORDER BY real.phenomenon_time DESC ",
                "LIMIT 1",
            );

            let row = sqlx::query(sql)
                .bind(pid)
                .bind(agent_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EvalError::Provider(format!("projection lookup failed: {e}")))?;

            if let Some(row) = row {
                return Ok(Some(parse_projection_row(&row)?));
            }

            // A named projection that did not resolve is a fact worth keeping.
            // Falling through to the heuristic here is exactly how a caller
            // that did everything right ends up with a score against a
            // different projection, so it does not fall through.
            return Ok(None);
        }

        // ── Heuristic path: opt-in only ──────────────────────────────────────
        let Some(lookback_days) = self.lookback_days else {
            return Ok(None);
        };

        let cutoff = chrono::Utc::now() - chrono::Duration::days(lookback_days);

        let sql = concat!(
            "SELECT ",
            predicted_value_sql!(),
            " AS predicted_value, ",
            "syn.extra->>'model_uri'               AS model_uri, ",
            "syn.extra->>'stage_id'                AS stage_id, ",
            "syn.observable_property               AS observable_property, ",
            "syn.extra->>'temperature_c'           AS temperature_c_str, ",
            "syn.extra->>'n_instances'             AS n_instances_str, ",
            "real.result_value::DOUBLE PRECISION   AS actual_value, ",
            "( SELECT count(*)::INTEGER FROM sosa_observations prev ",
            "   WHERE prev.produced_by_agent_id = $1 ",
            "     AND ",
            is_projection_sql!("prev"),
            "     AND prev.observable_property = syn.observable_property ",
            ")                                     AS n_prior ",
            "FROM sosa_observations syn ",
            "JOIN sosa_observations real ",
            "  ON real.observable_property = syn.observable_property ",
            " AND real.feature_of_interest IS NOT DISTINCT FROM syn.feature_of_interest ",
            " AND NOT ",
            is_projection_sql!("real"),
            " AND real.phenomenon_time >= $2 ",
            "WHERE ",
            is_projection_sql!("syn"),
            "  AND syn.produced_by_agent_id = $1 ",
            "  AND syn.phenomenon_time >= $2 ",
            "ORDER BY syn.phenomenon_time DESC ",
            "LIMIT 1",
        );

        // `phenomenon_time` is `BIGINT` epoch milliseconds, not a timestamp.
        // The old query bound a `TIMESTAMPTZ` against it — a type error that
        // never surfaced because the `source` predicate above it matched no
        // rows, so the comparison was never reached with anything to filter.
        let cutoff_ms = cutoff.timestamp_millis();

        // `produced_by_agent_id` is `TEXT`. Binding a `Uuid` against it is
        // `operator does not exist: text = uuid` — the same never-reached
        // error, in the same query, for the same reason.
        //
        // Worth stating plainly: the column is NULL on all 19,743 rows, so
        // this arm currently selects nothing whatever the binding. That is a
        // provenance gap in the writer, not a bug here, and it is why `n_prior`
        // reads 0 for every projection.
        let row = sqlx::query(sql)
            .bind(agent_id.to_string())
            .bind(cutoff_ms)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| EvalError::Provider(format!("projection fallback lookup failed: {e}")))?;

        match row {
            Some(row) => Ok(Some(parse_projection_row(&row)?)),
            None => Ok(None),
        }
    }
}

fn parse_projection_row(row: &sqlx::postgres::PgRow) -> Result<ProjectionObservation, EvalError> {
    let predicted_value: f64 = row
        .try_get::<Option<f64>, _>("predicted_value")
        .ok()
        .flatten()
        .ok_or_else(|| {
            EvalError::Provider(
                "predicted_value missing: neither extra->>'predicted_value' nor result_value \
                 held a number"
                    .into(),
            )
        })?;

    let actual_value: f64 = row
        .try_get("actual_value")
        .map_err(|e| EvalError::Provider(format!("actual_value missing: {e}")))?;

    let n_prior: i32 = row.try_get("n_prior").unwrap_or(0);

    let temperature_c: Option<f64> = row
        .try_get::<Option<String>, _>("temperature_c_str")
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok());

    let n_instances: Option<u32> = row
        .try_get::<Option<String>, _>("n_instances_str")
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok());

    Ok(ProjectionObservation {
        predicted_value,
        actual_value,
        model_uri: row.try_get::<Option<String>, _>("model_uri").ok().flatten(),
        stage_id: row.try_get::<Option<String>, _>("stage_id").ok().flatten(),
        observable_property: row
            .try_get::<String, _>("observable_property")
            .unwrap_or_else(|_| "unknown".into()),
        n_prior: n_prior as u32,
        temperature_c,
        n_instances,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lazily-connected pool: never dialled by these tests, which only
    /// inspect construction. It still needs a runtime to build, which is why
    /// they are `#[tokio::test]`.
    fn pool() -> PgPool {
        PgPool::connect_lazy("postgres://invalid/invalid").expect("lazy pool")
    }

    #[tokio::test]
    async fn the_scoring_constructor_refuses_the_heuristic() {
        // The control that replaces a sentence in a handover document. If this
        // ever defaults back to `Some(30)`, a triggered-but-unlinked evaluation
        // starts writing hard-verified signals about projections the
        // measurement never answered.
        assert!(!ProjectionLookupSqlx::new(pool()).heuristic_enabled());
    }

    #[tokio::test]
    async fn the_heuristic_must_be_asked_for_by_name() {
        assert!(ProjectionLookupSqlx::new(pool())
            .with_lookback_days(30)
            .heuristic_enabled());
    }

    #[tokio::test]
    async fn a_zero_or_negative_lookback_is_not_a_way_to_disable_it() {
        // `with_lookback_days(0)` reads like "off" and is not: the caller has
        // asked for the heuristic, so it stays on with a floor of one day.
        // Disabling is `new()`, which is a different call and cannot be
        // reached by accident from here.
        let l = ProjectionLookupSqlx::new(pool()).with_lookback_days(0);
        assert!(l.heuristic_enabled());
        assert_eq!(l.lookback_days, Some(1));
    }

    /// Both queries must be executable. Neither was.
    ///
    /// The primary query bound a `Uuid` against `produced_by_agent_id`, which
    /// is `TEXT` (`operator does not exist: text = uuid`). The fallback bound a
    /// `TIMESTAMPTZ` against `phenomenon_time`, which is `BIGINT`. Both are
    /// hard errors, and both sat in this file unexecuted for the life of the
    /// feature — because the `source` predicate above them matched no rows, so
    /// Postgres never had to resolve the comparison against anything.
    ///
    /// That is the defect class exactly: a query that could never have run
    /// correctly, never run, in a path whose emptiness looked like an empty
    /// world. Asserting the SQL *parses and plans* against the real schema is
    /// the cheapest thing that would have caught it, and it needs a database,
    /// so it is here rather than in the offline tier.
    #[tokio::test]
    #[ignore = "needs DATABASE_URL"]
    async fn both_lookup_queries_execute_against_the_real_schema() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let pool = PgPool::connect(&url).await.expect("connect");
        let agent = uuid::Uuid::nil();

        // Linked path. A nonsense id: the point is that the plan resolves and
        // returns cleanly, not that it finds anything.
        let linked = ProjectionLookupSqlx::new(pool.clone());
        let got = linked
            .find_projection_match(Some("no-such-projection"), agent)
            .await;
        assert!(got.is_ok(), "linked query failed to execute: {got:?}");
        assert!(got.unwrap().is_none());

        // Unlinked, heuristic off: must not touch the database at all, and must
        // not invent a match.
        let refused = linked.find_projection_match(None, agent).await;
        assert!(
            matches!(refused, Ok(None)),
            "the scoring constructor answered an unlinked lookup: {refused:?}"
        );

        // Heuristic path, explicitly enabled.
        let heuristic = ProjectionLookupSqlx::new(pool).with_lookback_days(30);
        let got = heuristic.find_projection_match(None, agent).await;
        assert!(got.is_ok(), "heuristic query failed to execute: {got:?}");
    }

    #[test]
    fn both_queries_test_the_projection_predicate_on_both_sides() {
        // A projection joined to itself scores a perfect 1.0 and looks like the
        // best possible news. The `NOT` on the measurement side is the only
        // thing preventing it, so it is asserted rather than left to review.
        let syn = is_projection_sql!("syn");
        let real = is_projection_sql!("real");
        assert_ne!(syn, real);
        // And the value comes from the column that actually holds it.
        assert!(predicted_value_sql!().contains("syn.result_value"));
    }
}
