//! Phase 2 — adapter implementing `agent_bestiary_evaluators::BrierLookup`
//! against the existing `fermi_forecasts` table.
//!
//! Per decision D8: this is a **read-only** thin wrapper. It does not
//! recompute Brier scores; it surfaces the most recent rolling Brier
//! observation for an agent based on what `src/handlers/forecasts.rs`
//! has already resolved.
//!
//! Agent ↔ forecast linkage is via `fermi_forecasts.agents_used`
//! (JSONB array of `{agent_id, ...}`). The stored `agent_id` may be
//! either the bestiary `agent_id` UUID or the human-readable
//! `agent_name`; this adapter accepts both.

use agent_bestiary_evaluators::{BrierLookup, BrierObservation, EvalError};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use std::sync::Arc;

/// Number of most-recent resolved forecasts to include in the rolling
/// average. Tunable via `BrierLookupSqlx::with_window`.
pub const DEFAULT_WINDOW: i64 = 50;

/// SQL-backed `BrierLookup`.
///
/// Phase 2 ships the simple "mean Brier across the most recent N
/// resolved forecasts where `agents_used` mentions this agent" query.
/// Phase 3 may extend this to time-windowed views; the lookup trait is
/// stable so future evolution can swap the impl behind it.
pub struct BrierLookupSqlx {
    pool: PgPool,
    window: i64,
    /// Optional resolver of `agent_id (uuid)` → `agent_name (text)`.
    /// When `Some`, both forms are searched in `agents_used`. When
    /// `None`, only the UUID form is searched.
    agent_name_resolver: Option<Arc<dyn AgentNameResolver>>,
}

/// Bridge so this adapter doesn't take a hard dependency on
/// `agent-bestiary-memory` — the eval pipeline already has the agent
/// loaded and can supply its name through this trait.
#[async_trait]
pub trait AgentNameResolver: Send + Sync {
    async fn resolve(&self, agent_id: uuid::Uuid) -> Option<String>;
}

impl BrierLookupSqlx {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            window: DEFAULT_WINDOW,
            agent_name_resolver: None,
        }
    }

    pub fn with_window(mut self, window: i64) -> Self {
        self.window = window.max(1);
        self
    }

    pub fn with_agent_name_resolver(
        mut self,
        resolver: Arc<dyn AgentNameResolver>,
    ) -> Self {
        self.agent_name_resolver = Some(resolver);
        self
    }
}

#[async_trait]
impl BrierLookup for BrierLookupSqlx {
    async fn latest_for_agent(
        &self,
        agent_id: uuid::Uuid,
    ) -> Result<Option<BrierObservation>, EvalError> {
        // Build the JSONB containment matchers. We try both the UUID
        // string and the agent name (when available).
        let uuid_str = agent_id.to_string();
        let agent_name: Option<String> = match &self.agent_name_resolver {
            Some(resolver) => resolver.resolve(agent_id).await,
            None => None,
        };

        // We can't use parameter substitution inside a JSONB literal,
        // so build the matcher arrays defensively. The query takes the
        // matchers as JSONB parameters and the @> operator does the
        // containment check.
        let uuid_matcher = serde_json::json!([{ "agent_id": uuid_str }]);
        let name_matcher = agent_name
            .as_ref()
            .map(|n| serde_json::json!([{ "agent_id": n }]));

        // Look up most-recent N resolved forecasts that mention the
        // agent in either form.
        let query_str = if name_matcher.is_some() {
            r#"WITH recent AS (
                SELECT brier_score, resolved_at
                FROM fermi_forecasts
                WHERE status = 'resolved'
                  AND brier_score IS NOT NULL
                  AND (agents_used @> $1 OR agents_used @> $2)
                ORDER BY resolved_at DESC
                LIMIT $3
            )
            SELECT
                AVG(brier_score)::DOUBLE PRECISION AS mean_brier,
                COUNT(*)::INTEGER                  AS n_forecasts,
                MAX(resolved_at)                   AS latest_resolved_at
            FROM recent"#
        } else {
            r#"WITH recent AS (
                SELECT brier_score, resolved_at
                FROM fermi_forecasts
                WHERE status = 'resolved'
                  AND brier_score IS NOT NULL
                  AND agents_used @> $1
                ORDER BY resolved_at DESC
                LIMIT $2
            )
            SELECT
                AVG(brier_score)::DOUBLE PRECISION AS mean_brier,
                COUNT(*)::INTEGER                  AS n_forecasts,
                MAX(resolved_at)                   AS latest_resolved_at
            FROM recent"#
        };

        let row = match name_matcher {
            Some(name_m) => sqlx::query(query_str)
                .bind(&uuid_matcher)
                .bind(&name_m)
                .bind(self.window)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| EvalError::Provider(format!("brier lookup failed: {}", e)))?,
            None => sqlx::query(query_str)
                .bind(&uuid_matcher)
                .bind(self.window)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| EvalError::Provider(format!("brier lookup failed: {}", e)))?,
        };

        let n: i32 = row.try_get("n_forecasts").unwrap_or(0);
        if n == 0 {
            return Ok(None);
        }

        let mean: Option<f64> = row.try_get("mean_brier").ok();
        let Some(mean_brier) = mean else {
            return Ok(None);
        };

        let computed_at = row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("latest_resolved_at")
            .ok()
            .flatten();

        Ok(Some(BrierObservation {
            brier_score: mean_brier,
            n_forecasts: Some(n as u32),
            computed_at,
        }))
    }
}
