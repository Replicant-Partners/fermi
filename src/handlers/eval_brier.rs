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
/// Reads from `fermi_forecasts` — finds forecasts owned by the agent's
/// associated user, filters to resolved forecasts, and returns the most
/// recent rolling Brier score.
///
/// Uses an AgentNameResolver to map agent_id → agent name, which is the
/// key stored in fermi_forecasts.agents_used metadata.
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
        // Resolve agent_id → agent_name for matching against forecast metadata.
        let agent_name = self.resolve_agent_name(agent_id).await;

        // We search for forecasts using two strategies:
        // 1. By agent name in the agents_used JSONB metadata
        // 2. By owner_id relationship (agent's creator)
        //
        // Strategy 1 is preferred when we have the agent name.
        let row = if let Some(ref name) = agent_name {
            sqlx::query_as::<_, (f64, i64, chrono::DateTime<chrono::Utc>)>(
                r#"SELECT
                     AVG(f.brier_score) AS avg_brier,
                     COUNT(*)::int8    AS n_resolved,
                     MAX(f.resolved_at) AS last_resolved
                   FROM fermi_forecasts f
                   WHERE f.status = 'resolved'
                     AND f.brier_score IS NOT NULL
                     AND (f.agents_used @> $2::jsonb
                          OR f.owner_id IN (
                            SELECT owner_id FROM agents WHERE agent_id = $1
                          ))"#,
            )
            .bind(agent_id)
            .bind(serde_json::json!([{"agent_name": name}]))
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| EvalError::Provider(e.to_string()))?
        } else {
            // Fallback: just use owner_id relationship
            sqlx::query_as::<_, (f64, i64, chrono::DateTime<chrono::Utc>)>(
                r#"SELECT
                     AVG(f.brier_score) AS avg_brier,
                     COUNT(*)::int8    AS n_resolved,
                     MAX(f.resolved_at) AS last_resolved
                   FROM fermi_forecasts f
                   JOIN agents a ON a.owner_id = f.owner_id
                   WHERE a.agent_id = $1
                     AND f.status = 'resolved'
                     AND f.brier_score IS NOT NULL"#,
            )
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| EvalError::Provider(e.to_string()))?
        };

        match row {
            Some((brier, n, resolved_at)) => Ok(Some(BrierObservation {
                brier_score: brier,
                n_forecasts: Some(n as u32),
                computed_at: Some(resolved_at),
            })),
            None => Ok(None),
        }
    }
}
