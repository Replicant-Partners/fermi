//! Platform feed implementations.
//!
//! The [`Feed`] trait and its contract types live in `crates/posterior` with
//! the rest of the BayesOps vocabulary. Implementations live here, because they
//! read Postgres and that crate is transport-neutral by
//! `docs/specs/14_BAYESOPS_SPEC.md` §9.
//!
//! ## What this module replaced
//!
//! Both feeds below are ports, not new behaviour. Before spec 35 they were two
//! hardcoded branches inside `refit_workspace::collect_observations`:
//!
//! - `if feeds_from.source == "upstream_resolutions"` — a single `if`, which is
//!   the entire reason BayesOps could only learn from Fermi forecast
//!   workspaces. Any other `source` value collected nothing and said nothing.
//! - an unconditional read of `workspace_outputs.observations.<name>`, which
//!   ran *before* `feeds_from` was consulted. A parameter that declared no
//!   source at all could still be fitted, invisibly. It is now a declared feed
//!   like any other.
//!
//! ## Adding a feed
//!
//! Implement [`Feed`], register it in [`build_registry`]. Nothing else changes:
//! the fitting, the gate, the ledger and the UI are all source-agnostic. The
//! next ones are `workspace_file` (CSV), `sosa`, and `domain_agent_ranking`
//! (Loop 4.B) — see spec 35 §13.
//!
//! See `docs/specs/35_BAYESOPS_PLATFORM_LAYER.md` §4.1.

use std::sync::Arc;

use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use posterior::{
    ExtractorRegistry, Feed, FeedError, FeedRegistry, ObservationRow, Series, WorkspaceContext,
};

/// Build the registry the server holds in `AppState`.
///
/// Both feeds need a pool; the upstream feed also needs the extractor registry,
/// because its underlying records are shaped JSON rather than scalars and the
/// binding names which extractor flattens them.
pub fn build_registry(pool: PgPool, extractors: ExtractorRegistry) -> FeedRegistry {
    let mut r = FeedRegistry::new();
    r.register(Arc::new(UpstreamResolutionsFeed {
        pool: pool.clone(),
        extractors,
    }));
    r.register(Arc::new(WorkspaceOutputFeed { pool }));
    r
}

/// Parse the workspace id a feed was handed, or fail with the feed's name on it.
fn workspace_uuid(context: &WorkspaceContext, feed: &str) -> Result<Uuid, FeedError> {
    let raw = context.require_workspace_id(feed)?;
    Uuid::parse_str(raw).map_err(|e| {
        FeedError::Internal(format!(
            "feed '{}': workspace_id '{}' is not a uuid ({})",
            feed, raw, e
        ))
    })
}

// ═════════════════════════════════════════════════════════════════════════════
// upstream_resolutions
// ═════════════════════════════════════════════════════════════════════════════

/// Observations derived from the resolutions of upstream workspaces.
///
/// This is the feed the World Cup rail runs on. Rows are the `outcome` object
/// of each upstream workspace's `resolution` output; the configured extractor
/// flattens each one to a scalar, or declines it.
pub struct UpstreamResolutionsFeed {
    pool: PgPool,
    extractors: ExtractorRegistry,
}

#[async_trait::async_trait]
impl Feed for UpstreamResolutionsFeed {
    fn name(&self) -> &str {
        "upstream_resolutions"
    }

    fn description(&self) -> &str {
        "Outcomes of workspaces this one depends on, as they resolve. Requires an \
         extractor to flatten each outcome to a number."
    }

    fn config_schema(&self) -> Option<JsonValue> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "extractor": {
                    "type": "string",
                    "description": "Registered extractor name. Flattens one outcome to a scalar."
                }
            },
            "required": ["extractor"],
            "additionalProperties": true
        }))
    }

    /// Upstream resolutions cannot enumerate their own series.
    ///
    /// A resolution outcome is an arbitrary JSON object whose useful fields
    /// depend on which extractor is applied — so there is no fixed column list
    /// to offer. Rather than invent one, this feed reports the *shape of the
    /// evidence available* as a single pseudo-series, and the picker sends the
    /// user to the extractor affordance.
    ///
    /// This is the honest answer, and it is why `describe` returns a list
    /// rather than being mandatory: a feed may legitimately not be pickable.
    async fn describe(&self, context: &WorkspaceContext) -> Result<Vec<Series>, FeedError> {
        let ws = workspace_uuid(context, self.name())?;
        let outcomes = self
            .read_upstream_outcomes(ws)
            .await
            .map_err(|e| FeedError::Unreachable(e.to_string()))?;
        if outcomes.is_empty() {
            return Ok(vec![]);
        }
        Ok(vec![Series::new(
            "__outcome__",
            format!(
                "{} resolved upstream outcome(s) — choose an extractor",
                outcomes.len()
            ),
            outcomes.len(),
        )])
    }

    async fn fetch(
        &self,
        context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Vec<ObservationRow>, FeedError> {
        let ws = workspace_uuid(context, self.name())?;

        let extractor_name = config
            .get("extractor")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FeedError::MissingConfig {
                key: "extractor".into(),
            })?;

        let extractor =
            self.extractors
                .get(extractor_name)
                .ok_or_else(|| FeedError::BadConfig {
                    key: "extractor".into(),
                    got: format!("'{}' (not registered)", extractor_name),
                    want: "a registered extractor name".into(),
                })?;

        let outcomes = self
            .read_upstream_outcomes(ws)
            .await
            .map_err(|e| FeedError::Unreachable(e.to_string()))?;

        let mut rows = Vec::new();
        for outcome in outcomes {
            // Per-row extractor failures stay non-fatal and are logged, exactly
            // as they were before this port. A malformed upstream outcome
            // should cost one observation, not the whole fit.
            match extractor.extract(&outcome, context, config) {
                Ok(Some(v)) => rows.push(
                    ObservationRow::real(v, format!("upstream_resolutions/{}", extractor_name))
                        .entity(context.entity_id.clone()),
                ),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        workspace = %ws,
                        extractor = %extractor_name,
                        error = %e,
                        "extractor failed on one upstream outcome; skipping"
                    );
                }
            }
        }
        Ok(rows)
    }
}

impl UpstreamResolutionsFeed {
    /// Verbatim port of the former `read_upstream_resolutions` helper.
    async fn read_upstream_outcomes(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<JsonValue>, sqlx::Error> {
        let upstream_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT upstream_id FROM workspace_dependencies WHERE downstream_id = $1",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;

        if upstream_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            "SELECT value FROM workspace_outputs
             WHERE workspace_id = ANY($1) AND key = 'resolution'",
        )
        .bind(&upstream_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut outcomes = Vec::with_capacity(rows.len());
        for row in rows {
            let v: JsonValue = row.get("value");
            if let Some(outcome) = v.get("outcome") {
                outcomes.push(outcome.clone());
            }
        }
        Ok(outcomes)
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// workspace_output
// ═════════════════════════════════════════════════════════════════════════════

/// Observations written directly onto the workspace as a numeric array, at
/// `workspace_outputs.observations.<series_key>`.
///
/// This is the former undeclared side door. It behaved as a hidden third source
/// that fired for every parameter whether or not one was declared; making it a
/// registered feed means a fit can no longer draw on data nobody bound.
///
/// Unlike `upstream_resolutions` this feed *can* enumerate itself — every key
/// under `observations` whose value is an array of numbers is a series — which
/// makes it the first pickable source and a useful shape reference for the CSV
/// feed that follows it.
pub struct WorkspaceOutputFeed {
    pool: PgPool,
}

#[async_trait::async_trait]
impl Feed for WorkspaceOutputFeed {
    fn name(&self) -> &str {
        "workspace_output"
    }

    fn description(&self) -> &str {
        "Numeric arrays published on this workspace under the `observations` output."
    }

    fn config_schema(&self) -> Option<JsonValue> {
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "series_key": {
                    "type": "string",
                    "description": "Key under `observations`. Defaults to the parameter name."
                }
            },
            "additionalProperties": true
        }))
    }

    async fn describe(&self, context: &WorkspaceContext) -> Result<Vec<Series>, FeedError> {
        let ws = workspace_uuid(context, self.name())?;
        let Some(obj) = self
            .read_observations_object(ws)
            .await
            .map_err(|e| FeedError::Unreachable(e.to_string()))?
        else {
            return Ok(vec![]);
        };
        let Some(map) = obj.as_object() else {
            return Ok(vec![]);
        };

        let mut out = Vec::new();
        for (key, value) in map {
            let Some(arr) = value.as_array() else {
                continue;
            };
            let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
            // A key whose array holds no numbers is not a series. Skipping it
            // rather than offering an empty one keeps the picker honest.
            if nums.is_empty() {
                continue;
            }
            out.push(Series::new(key.clone(), key.clone(), nums.len()).with_stats(&nums, 24));
        }
        out.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(out)
    }

    async fn fetch(
        &self,
        context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Vec<ObservationRow>, FeedError> {
        let ws = workspace_uuid(context, self.name())?;
        let series_key = config
            .get("series_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FeedError::MissingConfig {
                key: "series_key".into(),
            })?;

        let Some(obj) = self
            .read_observations_object(ws)
            .await
            .map_err(|e| FeedError::Unreachable(e.to_string()))?
        else {
            return Ok(vec![]);
        };

        let Some(arr) = obj.get(series_key).and_then(|v| v.as_array()) else {
            return Ok(vec![]);
        };

        Ok(arr
            .iter()
            .filter_map(|v| v.as_f64())
            .map(|v| {
                ObservationRow::real(v, format!("workspace_output/{}", series_key))
                    .entity(context.entity_id.clone())
            })
            .collect())
    }
}

impl WorkspaceOutputFeed {
    async fn read_observations_object(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<JsonValue>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT value FROM workspace_outputs
             WHERE workspace_id = $1 AND key = 'observations'",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<JsonValue, _>("value")))
    }
}
