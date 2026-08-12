//! sqlx-backed BrierLookup — Phase 2 of the evaluator integration.
//!
//! Connects the BrierEvaluator (in agent-bestiary/evaluators) to the
//! fermi_forecasts table via sqlx. After agent execution, the evaluator
//! reads the agent's resolved forecast Brier scores from the database.
//!
//! Registered in AppState at server boot as an Arc<dyn BrierLookup>.

use agent_bestiary_evaluators::{BrierLookup, BrierObservation, EvalError};
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Agent name resolver trait — allows the BrierLookup to find forecasts
/// by agent name (which is how they're recorded in fermi_forecasts metadata).
#[async_trait]
pub trait AgentNameResolver: Send + Sync {
    async fn resolve(&self, agent_id: Uuid) -> Option<String>;
}

/// Database-backed implementation of BrierLookup.
///
/// Reads from `fermi_forecasts` — finds resolved forecasts that *cite this
/// agent* in `agents_used`, and returns the rolling mean Brier over them.
/// Attribution is strictly per-agent: an agent that contributed to no
/// resolved forecast returns `None` (→ `Inapplicable`) rather than
/// inheriting its owner's aggregate.
///
/// Uses an AgentNameResolver to map agent_id → agent name, because two of
/// the three `agents_used` element shapes are keyed by name rather than id.
/// See `latest_for_agent` for the full shape inventory.
pub struct BrierLookupSqlx {
    pool: PgPool,
    agent_name_resolver: Option<Arc<dyn AgentNameResolver>>,
}

impl BrierLookupSqlx {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            agent_name_resolver: None,
        }
    }

    pub fn with_agent_name_resolver(mut self, resolver: Arc<dyn AgentNameResolver>) -> Self {
        self.agent_name_resolver = Some(resolver);
        self
    }

    /// Try to get the agent name: first from the resolver, then fall back
    /// to querying the agents table directly.
    async fn resolve_agent_name(&self, agent_id: Uuid) -> Option<String> {
        if let Some(ref resolver) = self.agent_name_resolver {
            let name = resolver.resolve(agent_id).await;
            if name.is_some() {
                return name;
            }
        }
        // Fallback: query the agents table
        sqlx::query_scalar::<_, String>("SELECT agent_name FROM agents WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
    }
}

#[async_trait]
impl BrierLookup for BrierLookupSqlx {
    async fn latest_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<BrierObservation>, EvalError> {
        // Resolve agent_id → agent_name: two of the three `agents_used`
        // element shapes are keyed by name, not id.
        let agent_name = self.resolve_agent_name(agent_id).await;

        // `agents_used` elements exist in three shapes in production, and a
        // forecast may carry any one of them:
        //
        //   {"name": "football_analyst", ...}   ← the live forecast/orchestra
        //                                         write path (forecasts.rs
        //                                         :1727), which is what every
        //                                         World Cup row uses
        //   {"agent_name": ..., "agent_id": ...} ← scripts/brier_backtest_seed.rs
        //   {"agent_id": "<uuid>", ...}          ← added by mig-170's one-shot
        //                                         backfill (not a trigger, so
        //                                         it does not cover new writes)
        //
        // Matching only `agent_name` — the previous behaviour — made every
        // forecast written by the live path invisible here, while
        // `GET /api/agents/:id/calibration` (which matches `agent_id`) saw
        // them fine. That split is why the Observatory could show Loop 5a
        // "closed" on the Loops tab and `brier: inactive` on the Overview for
        // the same agent at the same moment. Matching all three closes the
        // gap; all three predicates use `@>` so the mig-168 GIN index still
        // serves the lookup.
        //
        // The former `OR f.owner_id IN (SELECT user_id FROM agents ...)`
        // fallback is deliberately gone. It sat inside a single un-grouped
        // aggregate, so whenever the name match missed, `AVG(brier_score)`
        // silently spanned every resolved forecast belonging to the agent's
        // OWNER — forecasts this agent never contributed to — and that
        // owner-wide mean was then written into the agent's
        // `forecast_calibration` dimension as if it were agent-specific. An
        // agent with no attributed forecasts must report "no data" rather
        // than borrow its owner's track record.
        let by_agent_id = serde_json::json!([{ "agent_id": agent_id.to_string() }]);
        let by_agent_name = agent_name
            .as_ref()
            .map(|n| serde_json::json!([{ "agent_name": n }]));
        let by_name = agent_name
            .as_ref()
            .map(|n| serde_json::json!([{ "name": n }]));

        // Aggregates are decoded as `Option`: a no-GROUP-BY aggregate always
        // returns exactly one row, so a zero-match lookup yields
        // `(NULL, 0, NULL)`. Decoding that into non-Option `f64`/`DateTime`
        // (the previous signature) failed and surfaced as
        // `EvalError::Provider` — which made `BrierEvaluator`'s `Inapplicable`
        // branch unreachable and reported "this agent has no forecasts" as a
        // hard evaluator failure.
        //
        // The `IS NOT NULL` guards keep `@> NULL` (which evaluates to NULL,
        // not false) out of the OR chain when the name didn't resolve.
        let (avg_brier, n_resolved, last_resolved) =
            sqlx::query_as::<_, (Option<f64>, i64, Option<chrono::DateTime<chrono::Utc>>)>(
                r#"SELECT
                 AVG(f.brier_score)::float8 AS avg_brier,
                 COUNT(*)::int8             AS n_resolved,
                 MAX(f.resolved_at)         AS last_resolved
               FROM fermi_forecasts f
               WHERE f.status = 'resolved'
                 AND f.brier_score IS NOT NULL
                 AND (
                      f.agents_used @> $1::jsonb
                   OR ($2::jsonb IS NOT NULL AND f.agents_used @> $2::jsonb)
                   OR ($3::jsonb IS NOT NULL AND f.agents_used @> $3::jsonb)
                 )"#,
            )
            .bind(by_agent_id)
            .bind(by_agent_name)
            .bind(by_name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| EvalError::Provider(e.to_string()))?;

        // No attributed forecasts → genuinely no observation. `BrierEvaluator`
        // turns this into `Inapplicable`, which the aggregator skips rather
        // than recording as a failure.
        match avg_brier {
            Some(brier) if n_resolved > 0 => Ok(Some(BrierObservation {
                brier_score: brier,
                n_forecasts: Some(n_resolved as u32),
                computed_at: last_resolved,
            })),
            _ => Ok(None),
        }
    }
}
