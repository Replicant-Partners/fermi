//! SQL-backed `ProjectionLookup` adapter — wires the
//! `agent_bestiary_evaluators::ProjectionScoringEvaluator` to the
//! `sosa_observations` table.
//!
//! When the `ProjectionScoringEvaluator` fires (a real SOSA observation
//! has arrived), it calls `find_projection_match` with the `projection_id`
//! from the episode context.  This adapter:
//!
//! 1. Looks up the **synthetic** observation tagged with that
//!    `projection_id` in `sosa_observations.extra` to get the predicted
//!    value.
//! 2. Looks up the **real** observation that triggered evaluation — the
//!    caller passes it via `projection_id` being present in the bundle
//!    context along with `real_observation_id`.
//! 3. Returns a `ProjectionObservation` that the evaluator uses to compute
//!    the score.
//!
//! Fallback (no explicit `projection_id`): look back 30 days for the most
//! recent synthetic observation for the same
//! `(observable_property, feature_of_interest)` produced by this agent.

use agent_bestiary_evaluators::{EvalError, ProjectionLookup, ProjectionObservation};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

pub struct ProjectionLookupSqlx {
    pool: PgPool,
    /// Lookback window in days for the fallback path (no explicit projection_id).
    lookback_days: i64,
}

impl ProjectionLookupSqlx {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            lookback_days: 30,
        }
    }

    pub fn with_lookback_days(mut self, days: i64) -> Self {
        self.lookback_days = days.max(1);
        self
    }
}

#[async_trait]
impl ProjectionLookup for ProjectionLookupSqlx {
    async fn find_projection_match(
        &self,
        projection_id: Option<&str>,
        agent_id: uuid::Uuid,
    ) -> Result<Option<ProjectionObservation>, EvalError> {
        // ── Primary path: explicit projection_id ─────────────────────────────
        if let Some(pid) = projection_id {
            let row = sqlx::query(
                r#"
                SELECT
                    syn.extra->>'predicted_value'           AS predicted_str,
                    syn.extra->>'model_uri'                 AS model_uri,
                    syn.extra->>'stage_id'                  AS stage_id,
                    syn.observable_property                 AS observable_property,
                    syn.extra->>'temperature_c'             AS temperature_c_str,
                    syn.extra->>'n_instances'               AS n_instances_str,
                    real.result_value::DOUBLE PRECISION     AS actual_value,
                    (
                        SELECT COUNT(*)::INTEGER
                        FROM sosa_observations prev
                        WHERE prev.produced_by_agent_id = $2
                          AND prev.extra->>'source' = 'simops_simulation'
                          AND prev.observable_property = syn.observable_property
                    )                                       AS n_prior
                FROM sosa_observations syn
                JOIN sosa_observations real
                  ON real.observable_property = syn.observable_property
                 AND real.feature_of_interest = syn.feature_of_interest
                 AND real.extra->>'source' IS DISTINCT FROM 'simops_simulation'
                 AND real.extra->>'projection_id' = $1
                WHERE syn.extra->>'projection_id' = $1
                  AND syn.extra->>'source' = 'simops_simulation'
                ORDER BY real.phenomenon_time DESC
                LIMIT 1
                "#,
            )
            .bind(pid)
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| EvalError::Provider(format!("projection lookup failed: {e}")))?;

            if let Some(row) = row {
                return Ok(Some(parse_projection_row(&row)?));
            }
            // If no match via explicit id, fall through to fallback
        }

        // ── Fallback path: (observable_property, feature_of_interest) lookback
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.lookback_days);

        let row = sqlx::query(
            r#"
            SELECT
                syn.extra->>'predicted_value'           AS predicted_str,
                syn.extra->>'model_uri'                 AS model_uri,
                syn.extra->>'stage_id'                  AS stage_id,
                syn.observable_property                 AS observable_property,
                syn.extra->>'temperature_c'             AS temperature_c_str,
                syn.extra->>'n_instances'               AS n_instances_str,
                real.result_value::DOUBLE PRECISION     AS actual_value,
                (
                    SELECT COUNT(*)::INTEGER
                    FROM sosa_observations prev
                    WHERE prev.produced_by_agent_id = $1
                      AND prev.extra->>'source' = 'simops_simulation'
                      AND prev.observable_property = syn.observable_property
                )                                       AS n_prior
            FROM sosa_observations syn
            JOIN sosa_observations real
              ON real.observable_property = syn.observable_property
             AND real.feature_of_interest = syn.feature_of_interest
             AND real.extra->>'source' IS DISTINCT FROM 'simops_simulation'
             AND real.phenomenon_time >= $2
            WHERE syn.extra->>'source' = 'simops_simulation'
              AND syn.produced_by_agent_id = $1
              AND syn.phenomenon_time >= $2
            ORDER BY syn.phenomenon_time DESC
            LIMIT 1
            "#,
        )
        .bind(agent_id)
        .bind(cutoff)
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
    let predicted_str: Option<String> = row.try_get("predicted_str").ok().flatten();
    let predicted_value: f64 = predicted_str
        .as_deref()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| EvalError::Provider("predicted_value missing or unparseable".into()))?;

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
