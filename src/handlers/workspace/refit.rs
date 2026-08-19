//! BayesOps Refit Hook — Phase R-1 of Spec 23.
//!
//! The bridge between resolved workspace observations and BayesOps-fitted
//! distribution parameters. Called from:
//!
//!   1. The post-commit `tokio::spawn` block at
//!      `src/handlers/workspace/resolution.rs:286` — every successful
//!      workspace resolution triggers a refit of itself and its upstreams.
//!   2. `POST /api/workspaces/:id/refit` — manual operator-triggered refit.
//!
//! See `docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md` §3 for the design.
//!
//! ## Flow
//!
//! Given a workspace_id:
//!   1. Find the linked fermi_forecast (if any). Parse its FPL.
//!   2. For each `driver continuous foo { learnable: true ... }` declaration:
//!        a. Collect observations via the driver's `feeds_from` block —
//!           apply the named extractor to each upstream resolution outcome,
//!           plus any explicit `workspace_outputs[ws].observations.<foo>`.
//!        b. Call `posterior::fit_marginal()` to get a `FittedDistribution`.
//!        c. Run the impact gate: Monte Carlo the FPL twice (current params
//!           vs proposed params), compute the rate-of-interest delta.
//!        d. Persist a snapshot to `bayesops_posterior_snapshots` regardless
//!           of decision.
//!        e. Either auto-accept (write `params.<foo>_fitted`, post
//!           `bayesops_fit_accepted`), stage (insert pending row, post
//!           `bayesops_fit_pending`), or hard-block (post fail event).
//!
//! ## Failure policy
//!
//! Per `WORKSPACE_RESOLUTION.md` §"What you should change when wiring this":
//! refit failures MUST be log-and-continue. The caller's resolution is
//! already committed; the hook is value-add, never on the critical path.
//! Every public function returns `Result` but the resolution handler logs
//! errors and moves on.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use fermi::{ast::Statement, Lexer, Parser};
use posterior::{
    fit_marginal, DistFamily, Extractor, ExtractorRegistry, FittedDistribution, WorkspaceContext,
};

// ═════════════════════════════════════════════════════════════════════════════
// PUBLIC TYPES
// ═════════════════════════════════════════════════════════════════════════════

/// What triggered a refit. Persisted on every snapshot row so the spacetime
/// view can attribute each fit to its cause.
#[derive(Debug, Clone)]
pub enum TriggerReason {
    /// Fired by the resolution handler's post-commit hook. The workspace_id
    /// is whichever upstream workspace's resolution triggered the chain.
    UpstreamResolution { upstream_workspace_id: Uuid },
    /// Fired by `POST /api/workspaces/:id/refit`.
    Manual { user_id: String },
    /// Reserved for future scheduled refits (cron, batch jobs).
    Scheduled { job_id: String },
}

impl TriggerReason {
    fn as_provenance_string(&self) -> String {
        match self {
            Self::UpstreamResolution {
                upstream_workspace_id,
            } => format!("resolution:upstream:{}", upstream_workspace_id),
            Self::Manual { user_id } => format!("manual:{}", user_id),
            Self::Scheduled { job_id } => format!("scheduled:{}", job_id),
        }
    }
}

/// Aggregate result of a single `refit_workspace()` call. Reported back to the
/// HTTP endpoint and logged from the resolution hook.
#[derive(Debug, Clone, Serialize)]
pub struct RefitOutcome {
    pub workspace_id: Uuid,
    pub drivers_considered: usize,
    pub auto_accepted: usize,
    pub staged: usize,
    pub hard_blocked: usize,
    pub skipped: usize,
    pub per_driver: Vec<DriverOutcome>,
}

/// What happened for one learnable driver during a refit.
#[derive(Debug, Clone, Serialize)]
pub struct DriverOutcome {
    pub driver_name: String,
    pub decision: DriverDecision,
    pub n_observations: usize,
    pub note: Option<String>,
    /// snapshot_id if a snapshot was written.
    pub snapshot_id: Option<Uuid>,
    /// pending_id if the fit was staged for review.
    pub pending_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverDecision {
    AutoAccepted { delta_pp: f64 },
    Staged { delta_pp: f64 },
    HardBlocked { delta_pp: f64 },
    Skipped { reason: String },
    Errored { reason: String },
}

/// Errors that can escape from `refit_workspace`. The caller (resolution hook
/// or manual endpoint) is responsible for log-and-continue policy.
#[derive(Debug, Error)]
pub enum RefitError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("no fermi_forecast linked to workspace {0} (refit applies to fermi-forecast workspaces only)")]
    NoForecast(Uuid),

    #[error("workspace {0} has no FPL source on its forecast")]
    NoFplSource(Uuid),

    #[error("FPL parse error: {0}")]
    FplParse(String),

    #[error("workspace {0} not found")]
    WorkspaceNotFound(Uuid),

    #[error("internal: {0}")]
    Internal(String),
}

// ═════════════════════════════════════════════════════════════════════════════
// IMPACT GATE THRESHOLDS
// ═════════════════════════════════════════════════════════════════════════════

/// Global default auto-accept threshold in percentage points of the forecast
/// rate. Per-driver overrides via `feeds_from.auto_accept_threshold_pp`.
const DEFAULT_AUTO_ACCEPT_PP: f64 = 2.0;

/// Above this delta, the fit is hard-blocked — the impact is implausibly
/// large and is more likely to indicate a fitting bug than a real signal.
const HARD_BLOCK_PP: f64 = 20.0;

/// Monte Carlo iterations for impact-gate runs. Two runs per learnable
/// driver per refit. 10K is fast (≪1s) and gives stable rate estimates.
const IMPACT_GATE_ITERATIONS: u32 = 10_000;

// ═════════════════════════════════════════════════════════════════════════════
// PUBLIC ENTRY POINT
// ═════════════════════════════════════════════════════════════════════════════

/// Fit every learnable driver on `workspace_id`, gate by impact, write params
/// or stage for review. Idempotent for read paths; writes are gated to avoid
/// duplicate work.
///
/// Visits upstream workspaces recursively per the resolution doc's spec,
/// with cycle detection.
pub async fn refit_workspace(
    pool: &PgPool,
    registry: &ExtractorRegistry,
    workspace_id: Uuid,
    triggered_by: TriggerReason,
) -> Result<RefitOutcome, RefitError> {
    refit_workspace_with_visited(
        pool,
        registry,
        workspace_id,
        &triggered_by,
        &mut HashSet::new(),
    )
    .await
}

async fn refit_workspace_with_visited(
    pool: &PgPool,
    registry: &ExtractorRegistry,
    workspace_id: Uuid,
    triggered_by: &TriggerReason,
    visited: &mut HashSet<Uuid>,
) -> Result<RefitOutcome, RefitError> {
    // Cycle detection — workspace dependencies should be a DAG (the
    // add_dependency_handler enforces this) but defence in depth is cheap.
    if !visited.insert(workspace_id) {
        return Ok(RefitOutcome::empty(workspace_id));
    }

    let workspace = load_workspace(pool, workspace_id).await?;
    let linked = load_forecast_fpl(pool, workspace_id).await?;
    let program = parse_fpl(&linked.fpl_source)?;

    let learnable_drivers = collect_learnable_drivers(&program);
    let mut outcome = RefitOutcome::new(workspace_id, learnable_drivers.len());

    // Pull the current params blob once. We'll merge into it for each
    // auto-accepted fit.
    let current_params = load_params(pool, workspace_id).await?;

    let ws_context = WorkspaceContext {
        entity_id: workspace.entity_id.clone(),
        metadata: HashMap::new(),
    };

    for driver in &learnable_drivers {
        let outcome_driver = refit_one_driver(
            pool,
            registry,
            workspace_id,
            &workspace,
            &linked,
            &program,
            driver,
            &ws_context,
            &current_params,
            triggered_by,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                workspace = %workspace_id,
                driver = %driver.name,
                error = %e,
                "refit_one_driver failed; recording as Errored"
            );
            DriverOutcome {
                driver_name: driver.name.clone(),
                decision: DriverDecision::Errored {
                    reason: e.to_string(),
                },
                n_observations: 0,
                note: Some(e.to_string()),
                snapshot_id: None,
                pending_id: None,
            }
        });

        outcome.tally(&outcome_driver);
        outcome.per_driver.push(outcome_driver);
    }

    // Recurse to upstream workspaces. Per the resolution doc, upstream
    // priors should update against the observed downstream outcome.
    let upstreams: Vec<Uuid> = sqlx::query_scalar(
        "SELECT upstream_id FROM workspace_dependencies WHERE downstream_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for up_id in upstreams {
        // Each upstream refit gets its own provenance tag pointing to *this*
        // workspace as the trigger.
        let upstream_trigger = TriggerReason::UpstreamResolution {
            upstream_workspace_id: workspace_id,
        };
        if let Err(e) = Box::pin(refit_workspace_with_visited(
            pool,
            registry,
            up_id,
            &upstream_trigger,
            visited,
        ))
        .await
        {
            tracing::warn!(
                upstream = %up_id,
                downstream = %workspace_id,
                error = %e,
                "upstream refit failed (log-and-continue)"
            );
        }
    }

    Ok(outcome)
}

// ═════════════════════════════════════════════════════════════════════════════
// PER-DRIVER FLOW
// ═════════════════════════════════════════════════════════════════════════════

/// Internal: fit one driver, gate it, persist, emit event. Returns a
/// `DriverOutcome` whether the path succeeded or recoverably failed.
#[allow(clippy::too_many_arguments)]
async fn refit_one_driver(
    pool: &PgPool,
    registry: &ExtractorRegistry,
    workspace_id: Uuid,
    workspace: &WorkspaceRow,
    linked: &LinkedForecast,
    program: &fermi::ast::Program,
    driver: &LearnableDriver,
    ws_context: &WorkspaceContext,
    current_params: &JsonValue,
    triggered_by: &TriggerReason,
) -> Result<DriverOutcome, RefitError> {
    // ── Step 1: collect observations ─────────────────────────────────────
    let observations =
        collect_observations(pool, registry, workspace_id, driver, ws_context).await?;

    if observations.is_empty() {
        return Ok(DriverOutcome {
            driver_name: driver.name.clone(),
            decision: DriverDecision::Skipped {
                reason: "no observations collected (no upstream resolutions matching the extractor, and no explicit observations array)".to_string(),
            },
            n_observations: 0,
            note: None,
            snapshot_id: None,
            pending_id: None,
        });
    }

    // ── Step 2: fit ──────────────────────────────────────────────────────
    //
    // When fit_marginal can't produce a posterior (e.g. n=1 with no
    // variance, all observations identical, etc.), we still leave a
    // durable trace so the demo's "why isn't there a posterior yet?"
    // story is answerable. Without this, failed fits are invisible —
    // the HTTP response says `errored` but workspace_messages stays
    // empty and the Trajectory tab has nothing to render.
    let family = guess_family(&observations);
    let (fitted, fit_metadata) = match fit_marginal(&observations, None, family) {
        Ok(r) => r,
        Err(e) => {
            let reason = format!("fit_marginal: {}", e);
            let _ = emit_event(
                pool,
                workspace_id,
                "bayesops_fit_failed",
                &format!(
                    "⏳ Refit waiting on more data: driver '{}' has {} observation{} (need ≥2 for a parametric fit). Will re-try after the next resolution.",
                    driver.name,
                    observations.len(),
                    if observations.len() == 1 { "" } else { "s" }
                ),
                json!({
                    "event": "bayesops_fit_failed",
                    "reason": "fit_marginal_error",
                    "driver_name": driver.name,
                    "n_observations": observations.len(),
                    "family_attempted": family_label(family),
                    "error_detail": reason,
                    "triggered_by": triggered_by.as_provenance_string(),
                }),
            )
            .await;

            return Ok(DriverOutcome {
                driver_name: driver.name.clone(),
                decision: DriverDecision::Errored { reason },
                n_observations: observations.len(),
                note: None,
                snapshot_id: None,
                pending_id: None,
            });
        }
    };

    // ── Step 3: impact gate ──────────────────────────────────────────────
    let proposed_params = merge_params(
        current_params,
        &driver.name,
        &serde_json::to_value(&fitted).map_err(|e| RefitError::Internal(e.to_string()))?,
    );

    let (impact_before, impact_after) = compute_impact(
        &linked.fpl_source,
        program,
        current_params,
        &proposed_params,
    );
    // The snapshot keeps recording the CENTRE, so the stored series stays
    // comparable across this change even though the decision now uses more.
    let rate_before = impact_before.map(|s| s.mean);
    let rate_after = impact_after.map(|s| s.mean);
    // Impact is the LARGEST move across the mean and both tails, not the move in
    // the mean alone.
    //
    // A distribution fit changes a distribution. `run_with_params` returned only
    // `results.mean`, so a posterior that left the centre alone and doubled the
    // interval scored an impact of exactly zero and was auto-accepted as harmless
    // — on a forecasting platform where the interval is half the answer. All 55
    // production snapshots recorded a mean delta of 0.0000, and nothing had ever
    // looked at what happened to their p5 and p95.
    let delta_pp = match (impact_before, impact_after) {
        (Some(b), Some(a)) => [
            (a.mean - b.mean).abs(),
            (a.p5 - b.p5).abs(),
            (a.p95 - b.p95).abs(),
        ]
        .into_iter()
        .fold(0.0_f64, f64::max)
            * 100.0,
        // FAIL CLOSED. This was `0.0` with the comment "skip the gate, default to
        // AutoAccept" — so a fit whose impact could not be MEASURED was adopted
        // without review, which is the same defect as a cross-check that cannot run
        // reporting healthy forever. An unmeasurable impact is not a small one.
        // `f64::INFINITY` would hard-block; staging is right, because the failure is
        // in the assessment rather than in the fit.
        _ => {
            tracing::warn!(
                driver = %driver.name,
                "impact could not be assessed; staging rather than auto-accepting"
            );
            f64::NAN
        }
    };

    let threshold_pp = driver
        .auto_accept_threshold_pp
        .unwrap_or(DEFAULT_AUTO_ACCEPT_PP);

    // NaN compares false against everything, so an unassessable impact falls
    // through both magnitude branches and lands on Stage. Asserted by
    // `an_unassessable_impact_stages_rather_than_auto_accepting` rather than left
    // to a reader's knowledge of IEEE 754.
    let decision = classify_decision(delta_pp, threshold_pp, fit_metadata.quality);

    // ── Step 4: write the snapshot regardless ────────────────────────────
    let snapshot_id = write_snapshot(
        pool,
        workspace_id,
        &driver.name,
        &fitted,
        &fit_metadata,
        observations.len(),
        rate_before,
        rate_after,
        &decision,
        triggered_by,
    )
    .await?;

    // ── Step 5: act on the decision ──────────────────────────────────────
    match decision {
        DecisionKind::AutoAccept => {
            write_fitted_params(pool, workspace_id, &driver.name, &fitted, &workspace.slug).await?;

            // Spec 23 R-3 Piece 1: write a fermi_forecast_updates row so the
            // forecast_spacetime trigger fires with revision_trigger =
            // 'bayesops_refit'. The previous probability is the forecast's
            // currently-stored probability; the new one is the impact-gate's
            // rate_after when available, else the same as before.
            let new_probability = rate_after.unwrap_or(linked.current_probability);
            let _ = write_spacetime_update(
                pool,
                &linked.forecast_id,
                linked.current_probability,
                new_probability,
                &driver.name,
                snapshot_id,
                rate_before,
                rate_after,
                observations.len(),
            )
            .await;

            emit_event(
                pool,
                workspace_id,
                "bayesops_fit_accepted",
                &format!(
                    "📊 Refit accepted automatically (Δrate={:+.1}pp): driver '{}' now {} fit from {} observations.",
                    delta_pp,
                    driver.name,
                    family_label(family),
                    observations.len()
                ),
                json!({
                    "event": "bayesops_fit_accepted",
                    "snapshot_id": snapshot_id,
                    "driver_name": driver.name,
                    "delta_pp": delta_pp,
                    "rate_before": rate_before,
                    "rate_after": rate_after,
                    "n_observations": observations.len(),
                    "fitted": fitted,
                }),
            )
            .await?;

            Ok(DriverOutcome {
                driver_name: driver.name.clone(),
                decision: DriverDecision::AutoAccepted { delta_pp },
                n_observations: observations.len(),
                note: None,
                snapshot_id: Some(snapshot_id),
                pending_id: None,
            })
        }
        DecisionKind::Stage => {
            let pending_id = stage_pending(pool, workspace_id, &driver.name, snapshot_id).await?;
            emit_event(
                pool,
                workspace_id,
                "bayesops_fit_pending",
                &format!(
                    "📊 Refit staged for review (Δrate={:+.1}pp > {:.1}pp threshold): driver '{}' has a proposed fit from {} observations. Accept or dismiss in the editor.",
                    delta_pp,
                    threshold_pp,
                    driver.name,
                    observations.len()
                ),
                json!({
                    "event": "bayesops_fit_pending",
                    "pending_id": pending_id,
                    "snapshot_id": snapshot_id,
                    "driver_name": driver.name,
                    "delta_pp": delta_pp,
                    "threshold_pp": threshold_pp,
                    "rate_before": rate_before,
                    "rate_after": rate_after,
                    "n_observations": observations.len(),
                    "fitted": fitted,
                }),
            )
            .await?;

            Ok(DriverOutcome {
                driver_name: driver.name.clone(),
                decision: DriverDecision::Staged { delta_pp },
                n_observations: observations.len(),
                note: None,
                snapshot_id: Some(snapshot_id),
                pending_id: Some(pending_id),
            })
        }
        DecisionKind::HardBlock => {
            emit_event(
                pool,
                workspace_id,
                "bayesops_fit_failed",
                &format!(
                    "⚠️ Refit hard-blocked: driver '{}' produced an implausible Δrate of {:+.1}pp (>{:.1}pp limit). Likely a fitting bug; snapshot kept for diagnostics, no params written.",
                    driver.name, delta_pp, HARD_BLOCK_PP
                ),
                json!({
                    "event": "bayesops_fit_failed",
                    "reason": "hard_block",
                    "snapshot_id": snapshot_id,
                    "driver_name": driver.name,
                    "delta_pp": delta_pp,
                    "hard_block_threshold_pp": HARD_BLOCK_PP,
                    "rate_before": rate_before,
                    "rate_after": rate_after,
                }),
            )
            .await?;

            Ok(DriverOutcome {
                driver_name: driver.name.clone(),
                decision: DriverDecision::HardBlocked { delta_pp },
                n_observations: observations.len(),
                note: Some(format!("delta {:.2}pp exceeds hard-block limit", delta_pp)),
                snapshot_id: Some(snapshot_id),
                pending_id: None,
            })
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: WORKSPACE / FPL LOADING
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct WorkspaceRow {
    #[allow(dead_code)]
    id: Uuid,
    slug: String,
    /// Computed entity_id — derived from `slug` via the convention
    /// `team_prior_<entity>` or by trimming common workspace prefixes.
    /// Fallback: the slug itself.
    entity_id: Option<String>,
}

async fn load_workspace(pool: &PgPool, workspace_id: Uuid) -> Result<WorkspaceRow, RefitError> {
    let row = sqlx::query("SELECT id, slug FROM teams WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?
        .ok_or(RefitError::WorkspaceNotFound(workspace_id))?;

    let id: Uuid = row.get("id");
    let slug: String = row.get("slug");
    let entity_id = derive_entity_id(&slug);
    Ok(WorkspaceRow {
        id,
        slug,
        entity_id,
    })
}

/// Convention-based entity_id derivation from a workspace slug.
///
/// Examples:
///   "team-arg-2026"   → "ARG"
///   "team_prior_arg"  → "ARG"
///   "team-arg"        → "ARG"
///   "h2h-arg-vs-bra"  → None (multi-entity workspace)
///   "random-slug"     → None
///
/// Heuristic: split the slug by `-` and `_`, drop any segment that's a
/// known scaffolding word (`team`, `prior`) or all-digit (year), and if
/// exactly one segment remains, that's the entity_id. Otherwise None.
///
/// A more rigorous approach would store entity_id in teams.metadata
/// explicitly. For the demo this heuristic is enough.
fn derive_entity_id(slug: &str) -> Option<String> {
    let scaffolding = ["team", "prior", "workspace", "ws"];
    let segments: Vec<&str> = slug
        .split(|c: char| c == '-' || c == '_')
        .filter(|seg| {
            !seg.is_empty()
                && !scaffolding.contains(&seg.to_lowercase().as_str())
                && !seg.chars().all(|c| c.is_ascii_digit())
        })
        .collect();

    if segments.len() == 1 {
        Some(segments[0].to_uppercase())
    } else {
        None
    }
}

/// Minimal context the refit hook needs from `fermi_forecasts` to do its
/// job: the FPL to parse, the forecast_id to attribute spacetime rows to,
/// and the current `predicted_probability` so an auto-accept can compute
/// the new rate and write a `fermi_forecast_updates` row with
/// `previous_probability = current` (Spec 23 R-3 Piece 1).
#[derive(Debug, Clone)]
struct LinkedForecast {
    forecast_id: String,
    fpl_source: String,
    current_probability: f64,
}

async fn load_forecast_fpl(
    pool: &PgPool,
    workspace_id: Uuid,
) -> Result<LinkedForecast, RefitError> {
    let row = sqlx::query(
        "SELECT id, fpl_source, predicted_probability
         FROM fermi_forecasts
         WHERE workspace_id = $1
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?
    .ok_or(RefitError::NoForecast(workspace_id))?;

    let forecast_id: String = row.get("id");
    let fpl_source = row
        .try_get::<Option<String>, _>("fpl_source")
        .ok()
        .flatten()
        .ok_or(RefitError::NoFplSource(workspace_id))?;
    let current_probability: f32 = row.try_get("predicted_probability").unwrap_or(0.5);
    Ok(LinkedForecast {
        forecast_id,
        fpl_source,
        current_probability: current_probability as f64,
    })
}

fn parse_fpl(source: &str) -> Result<fermi::ast::Program, RefitError> {
    let tokens = Lexer::new(source)
        .tokenize()
        .map_err(|errs| RefitError::FplParse(format!("{:?}", errs)))?;
    Parser::new(tokens)
        .parse()
        .map_err(|e| RefitError::FplParse(e.to_string()))
}

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: LEARNABLE DRIVERS
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct LearnableDriver {
    name: String,
    feeds_from: Option<fermi::ast::FeedsFrom>,
    auto_accept_threshold_pp: Option<f64>,
}

fn collect_learnable_drivers(program: &fermi::ast::Program) -> Vec<LearnableDriver> {
    let mut out = Vec::new();
    for stmt in &program.statements {
        if let Statement::Driver(d) = stmt {
            if d.learnable {
                out.push(LearnableDriver {
                    name: d.name.clone(),
                    feeds_from: d.feeds_from.clone(),
                    auto_accept_threshold_pp: d
                        .feeds_from
                        .as_ref()
                        .and_then(|f| f.auto_accept_threshold_pp),
                });
            }
        }
    }
    out
}

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: OBSERVATION COLLECTION
// ═════════════════════════════════════════════════════════════════════════════

async fn collect_observations(
    pool: &PgPool,
    registry: &ExtractorRegistry,
    workspace_id: Uuid,
    driver: &LearnableDriver,
    ws_context: &WorkspaceContext,
) -> Result<Vec<f64>, RefitError> {
    let mut observations: Vec<f64> = Vec::new();

    // Source 1: explicit observations array on the workspace's outputs.
    // Lives at workspace_outputs[ws].observations.<driver_name> as a
    // JSON array of numbers.
    if let Some(arr) = read_observations_array(pool, workspace_id, &driver.name).await? {
        observations.extend(arr);
    }

    // Source 2: derive from upstream resolutions via the declared extractor.
    if let Some(feeds_from) = &driver.feeds_from {
        if feeds_from.source == "upstream_resolutions" {
            let extractor = registry.get(&feeds_from.extractor).ok_or_else(|| {
                RefitError::Internal(format!(
                    "extractor '{}' not registered (driver '{}')",
                    feeds_from.extractor, driver.name
                ))
            })?;
            let upstream_outcomes = read_upstream_resolutions(pool, workspace_id).await?;
            for outcome in upstream_outcomes {
                match extractor.extract(&outcome, ws_context, &feeds_from.config) {
                    Ok(Some(v)) => observations.push(v),
                    Ok(None) => {} // legitimate skip
                    Err(e) => {
                        tracing::warn!(
                            workspace = %workspace_id,
                            driver = %driver.name,
                            extractor = %feeds_from.extractor,
                            error = %e,
                            "extractor failed on one upstream outcome; skipping"
                        );
                    }
                }
            }
        }
    }

    Ok(observations)
}

async fn read_observations_array(
    pool: &PgPool,
    workspace_id: Uuid,
    driver_name: &str,
) -> Result<Option<Vec<f64>>, RefitError> {
    let row = sqlx::query(
        "SELECT value FROM workspace_outputs
         WHERE workspace_id = $1 AND key = 'observations'",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else { return Ok(None) };
    let value: JsonValue = row.get("value");
    let Some(driver_obj) = value.get(driver_name) else {
        return Ok(None);
    };
    let Some(arr) = driver_obj.as_array() else {
        return Ok(None);
    };
    let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
    Ok(Some(nums))
}

async fn read_upstream_resolutions(
    pool: &PgPool,
    workspace_id: Uuid,
) -> Result<Vec<JsonValue>, RefitError> {
    // Find all upstream workspaces, then for each pull their
    // workspace_outputs.resolution.outcome.
    let upstream_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT upstream_id FROM workspace_dependencies WHERE downstream_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    if upstream_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT value FROM workspace_outputs
         WHERE workspace_id = ANY($1) AND key = 'resolution'",
    )
    .bind(&upstream_ids)
    .fetch_all(pool)
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

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: FAMILY SELECTION
// ═════════════════════════════════════════════════════════════════════════════

/// Heuristic family selection: Beta for observations in (0,1), Normal otherwise.
/// Auto is rejected because we want deterministic behaviour during the demo.
fn guess_family(observations: &[f64]) -> DistFamily {
    if observations.iter().all(|x| *x > 0.0 && *x < 1.0) {
        DistFamily::Beta
    } else if observations.iter().all(|x| *x > 0.0) {
        // Could be Lognormal but Normal is safer default for the demo
        DistFamily::Normal
    } else {
        DistFamily::Normal
    }
}

fn family_label(family: DistFamily) -> &'static str {
    match family {
        DistFamily::Beta => "Beta",
        DistFamily::Normal => "Normal",
        DistFamily::Lognormal => "Lognormal",
        DistFamily::Triangular => "Triangular",
        DistFamily::Auto => "auto",
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: IMPACT GATE
// ═════════════════════════════════════════════════════════════════════════════

enum DecisionKind {
    AutoAccept,
    Stage,
    HardBlock,
}

/// Decide what to do with a proposed fit.
///
/// ## The gate was not gating
///
/// This took `delta_pp` alone, so the only question asked was "does adopting this
/// change the forecast much?". Measured over every snapshot the platform had
/// produced:
///
/// ```text
/// 55 snapshots · 55 auto_accepted · 55 quality = insufficient
/// mean n_observations 2.0 · n_eff 2.0 · ci_width 4.441 · rate delta 0.0000
/// ```
///
/// Every fit was built on two observations, self-labelled `Insufficient`, and
/// adopted — **because** it changed nothing. A small immediate impact is not
/// evidence that a posterior is trustworthy; it is the absence of evidence, and
/// the gate was reading the two as the same thing.
///
/// `DataQuality::classify` already draws the line at `n_eff < 5.0`. The platform
/// knew these fits were insufficient, wrote it on all 55 rows, and never consulted
/// it. So the quality verdict now reaches the decision.
///
/// `Insufficient` STAGES rather than blocks: the fit may well be right, and the
/// snapshot plus a pending row keeps it reviewable. What it must not do is silently
/// replace a considered prior. `Sparse` (5–20 effective observations) may still
/// auto-accept on small impact — that is the loop doing its job as data arrives,
/// and holding it to the same bar as `Insufficient` would stall learning
/// permanently on any driver with a modest history.
fn classify_decision(
    delta_pp: f64,
    threshold_pp: f64,
    quality: posterior::DataQuality,
) -> DecisionKind {
    if delta_pp > HARD_BLOCK_PP {
        return DecisionKind::HardBlock;
    }
    // Checked BEFORE the impact threshold, deliberately. Ordering the other way
    // would auto-accept an insufficient fit whenever its impact was small, which
    // is the exact case all 55 production snapshots fell into.
    if matches!(quality, posterior::DataQuality::Insufficient) {
        return DecisionKind::Stage;
    }
    if delta_pp < threshold_pp {
        DecisionKind::AutoAccept
    } else {
        DecisionKind::Stage
    }
}

/// Run the FPL twice (current params vs proposed params) and return
/// `(rate_before, rate_after)`. Each rate is the `mean` of `ExecutionResults`,
/// which for binary forecasts is the predicted probability. Returns
/// `(None, None)` if either run fails — the gate caller treats this as
/// "no impact assessment possible" and defaults to AutoAccept.
fn compute_impact(
    _fpl_source: &str,
    program: &fermi::ast::Program,
    current_params: &JsonValue,
    proposed_params: &JsonValue,
) -> (Option<ImpactSample>, Option<ImpactSample>) {
    let before = run_with_params(program, current_params, IMPACT_GATE_ITERATIONS).ok();
    let after = run_with_params(program, proposed_params, IMPACT_GATE_ITERATIONS).ok();
    (before, after)
}

/// The part of a simulation the impact gate compares.
///
/// Carries the tails as well as the centre so a fit that only widens the interval
/// cannot read as zero impact. `rate_before`/`rate_after` on the snapshot still
/// persist `mean`, so the stored series is unchanged and comparable across this
/// change.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImpactSample {
    pub mean: f64,
    pub p5: f64,
    pub p95: f64,
}

fn run_with_params(
    program: &fermi::ast::Program,
    params: &JsonValue,
    iterations: u32,
) -> Result<ImpactSample, fermi::ExecutionError> {
    let mut executor = fermi::Executor::with_seed(iterations as usize, 0xBA1E_50);
    // Split params into numeric (set_params) and JSON (set_json_params).
    let mut numeric = HashMap::new();
    let mut json_params = HashMap::new();
    if let Some(obj) = params.as_object() {
        for (k, v) in obj {
            match v.as_f64() {
                Some(n) => {
                    numeric.insert(k.clone(), n);
                }
                None => {
                    json_params.insert(k.clone(), v.clone());
                }
            }
        }
    }
    executor.set_params(numeric);
    executor.set_json_params(json_params);

    let results = executor.execute(program)?;
    Ok(ImpactSample {
        mean: results.mean,
        p5: results.p5,
        p95: results.p95,
    })
}

/// Build a proposed params blob by merging `<driver_name>_fitted: fitted`
/// into the current params object.
fn merge_params(current: &JsonValue, driver_name: &str, fitted_value: &JsonValue) -> JsonValue {
    let mut obj = current.as_object().cloned().unwrap_or_default();
    obj.insert(format!("{}_fitted", driver_name), fitted_value.clone());
    JsonValue::Object(obj)
}

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: PARAMS LOAD / WRITE
// ═════════════════════════════════════════════════════════════════════════════

async fn load_params(pool: &PgPool, workspace_id: Uuid) -> Result<JsonValue, RefitError> {
    let row = sqlx::query(
        "SELECT value FROM workspace_outputs
         WHERE workspace_id = $1 AND key = 'params'",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|r| r.get::<JsonValue, _>("value"))
        .unwrap_or(JsonValue::Object(serde_json::Map::new())))
}

async fn write_fitted_params(
    pool: &PgPool,
    workspace_id: Uuid,
    driver_name: &str,
    fitted: &FittedDistribution,
    updated_by: &str,
) -> Result<(), RefitError> {
    // Read-modify-write the params output. Concurrent refits on the same
    // workspace are serialised by Postgres row locks via the ON CONFLICT
    // UPDATE — last writer wins on the merge, which is acceptable for the
    // demo (one writer per refit; refits are rare events).
    let current = load_params(pool, workspace_id).await?;
    let fitted_json =
        serde_json::to_value(fitted).map_err(|e| RefitError::Internal(e.to_string()))?;
    let merged = merge_params(&current, driver_name, &fitted_json);

    sqlx::query(
        "INSERT INTO workspace_outputs
            (workspace_id, key, value, version, updated_at, updated_by)
         VALUES ($1, 'params', $2, 1, NOW(), $3)
         ON CONFLICT (workspace_id, key) DO UPDATE SET
            value      = EXCLUDED.value,
            version    = workspace_outputs.version + 1,
            updated_at = NOW(),
            updated_by = EXCLUDED.updated_by",
    )
    .bind(workspace_id)
    .bind(&merged)
    .bind(updated_by)
    .execute(pool)
    .await?;

    // Manually fan out the upstream_output_updated event, mirroring
    // set_output_handler:87-115.
    let downstream: Vec<Uuid> = sqlx::query_scalar(
        "SELECT downstream_id FROM workspace_dependencies
         WHERE upstream_id = $1
           AND (key_filter IS NULL OR key_filter = 'params')",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for ds_id in downstream {
        let _ = sqlx::query(
            "INSERT INTO workspace_messages
                (workspace_id, sender_type, sender_id, sender_name, content, message_type, metadata)
             VALUES ($1, 'system', $2, 'BayesOps Refit', $3, 'system_event', $4)",
        )
        .bind(ds_id)
        .bind(workspace_id.to_string())
        .bind(format!(
            "Upstream workspace updated 'params' (BayesOps fit accepted)"
        ))
        .bind(json!({
            "event": "upstream_output_updated",
            "upstream_workspace_id": workspace_id,
            "key": "params",
        }))
        .execute(pool)
        .await;
    }

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// SPACETIME WIRE-UP (Spec 23 R-3 Piece 1)
// ═════════════════════════════════════════════════════════════════════════════
//
// When the refit hook auto-accepts a fit, it writes a fermi_forecast_updates
// row tagged with revision_trigger = 'bayesops_refit'. The trigger
// fn_forecast_spacetime_on_update (migration 149) reads that tag and writes
// a forecast_spacetime row with the correct revision_trigger value, instead
// of the generic 'evidence_update'.
//
// This is what makes BayesOps refits visible alongside agent runs, evidence
// additions, and schedule re-runs in the GET /api/forecasts/:id/spacetime
// endpoint that already exists.

/// Insert a `fermi_forecast_updates` row attributed to a BayesOps refit.
/// Returns Ok(()) on success; the caller logs errors and continues per the
/// refit hook's log-and-continue failure policy.
#[allow(clippy::too_many_arguments)]
async fn write_spacetime_update(
    pool: &PgPool,
    forecast_id: &str,
    previous_probability: f64,
    new_probability: f64,
    driver_name: &str,
    snapshot_id: Uuid,
    rate_before: Option<f64>,
    rate_after: Option<f64>,
    n_observations: usize,
) -> Result<(), RefitError> {
    // Skip the write if the probability didn't materially change — keeps
    // the spacetime view from getting polluted with no-op revisions.
    if (new_probability - previous_probability).abs() < 1e-4 {
        return Ok(());
    }

    let update_id = Uuid::new_v4().to_string();
    let reason = format!(
        "BayesOps refit accepted: driver '{}' fitted from {} observations",
        driver_name, n_observations
    );

    // The `evidence_added` JSONB captures the BayesOps-specific context so
    // the spacetime endpoint can reconstruct what drove this revision when
    // it renders.
    let evidence_added = json!({
        "kind": "bayesops_refit",
        "driver_name": driver_name,
        "snapshot_id": snapshot_id,
        "rate_before": rate_before,
        "rate_after": rate_after,
        "n_observations": n_observations,
    });

    sqlx::query(
        "INSERT INTO fermi_forecast_updates
            (id, forecast_id, previous_probability, new_probability,
             reason, agent_id, evidence_added, revision_trigger, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'bayesops_refit', NOW())",
    )
    .bind(&update_id)
    .bind(forecast_id)
    .bind(previous_probability as f32)
    .bind(new_probability as f32)
    .bind(&reason)
    .bind(Option::<String>::None) // no agent attribution; refits are systemic
    .bind(&evidence_added)
    .execute(pool)
    .await?;

    // Also update fermi_forecasts.predicted_probability so future refits see
    // the new baseline. Without this every refit would compare against the
    // original probability and the deltas would inflate.
    sqlx::query(
        "UPDATE fermi_forecasts
         SET predicted_probability = $2, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(forecast_id)
    .bind(new_probability as f32)
    .execute(pool)
    .await?;

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: SNAPSHOT + PENDING WRITES
// ═════════════════════════════════════════════════════════════════════════════

#[allow(clippy::too_many_arguments)]
async fn write_snapshot(
    pool: &PgPool,
    workspace_id: Uuid,
    driver_name: &str,
    fitted: &FittedDistribution,
    metadata: &posterior::FitMetadata,
    n_observations: usize,
    rate_before: Option<f64>,
    rate_after: Option<f64>,
    decision: &DecisionKind,
    triggered_by: &TriggerReason,
) -> Result<Uuid, RefitError> {
    let snapshot_id = Uuid::new_v4();
    let fitted_json =
        serde_json::to_value(fitted).map_err(|e| RefitError::Internal(e.to_string()))?;
    let metadata_json =
        serde_json::to_value(metadata).map_err(|e| RefitError::Internal(e.to_string()))?;
    let quality = match metadata.quality {
        posterior::DataQuality::Sufficient => "sufficient",
        posterior::DataQuality::Sparse => "sparse",
        posterior::DataQuality::Insufficient => "insufficient",
    };
    let decision_str = match decision {
        DecisionKind::AutoAccept => "auto_accepted",
        DecisionKind::Stage => "staged",
        DecisionKind::HardBlock => "hard_blocked",
    };

    sqlx::query(
        "INSERT INTO bayesops_posterior_snapshots
            (snapshot_id, workspace_id, driver_name, fitted, metadata,
             n_observations, synthetic_n, ci_width, n_eff, quality,
             rate_before, rate_after, decision, triggered_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(snapshot_id)
    .bind(workspace_id)
    .bind(driver_name)
    .bind(&fitted_json)
    .bind(&metadata_json)
    .bind(n_observations as i32)
    .bind(0i32) // synthetic_n: 0 for real-data refits
    .bind(fitted.ci_width())
    .bind(fitted.n_eff())
    .bind(quality)
    .bind(rate_before)
    .bind(rate_after)
    .bind(decision_str)
    .bind(triggered_by.as_provenance_string())
    .execute(pool)
    .await?;

    Ok(snapshot_id)
}

async fn stage_pending(
    pool: &PgPool,
    workspace_id: Uuid,
    driver_name: &str,
    snapshot_id: Uuid,
) -> Result<Uuid, RefitError> {
    // Auto-expire any existing pending fit for this driver. The EXCLUDE
    // constraint on bayesops_pending_fits prevents concurrent pendings;
    // this is the polite handling.
    sqlx::query(
        "UPDATE bayesops_pending_fits
         SET status='expired', decided_at=NOW(),
             decision_notes='superseded by newer refit'
         WHERE workspace_id=$1 AND driver_name=$2 AND status='pending'",
    )
    .bind(workspace_id)
    .bind(driver_name)
    .execute(pool)
    .await?;

    let pending_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO bayesops_pending_fits
            (pending_id, workspace_id, driver_name, snapshot_id, status)
         VALUES ($1, $2, $3, $4, 'pending')",
    )
    .bind(pending_id)
    .bind(workspace_id)
    .bind(driver_name)
    .bind(snapshot_id)
    .execute(pool)
    .await?;

    Ok(pending_id)
}

// ═════════════════════════════════════════════════════════════════════════════
// HELPERS: EVENT EMISSION
// ═════════════════════════════════════════════════════════════════════════════

async fn emit_event(
    pool: &PgPool,
    workspace_id: Uuid,
    event_kind: &str,
    content: &str,
    metadata: JsonValue,
) -> Result<(), RefitError> {
    sqlx::query(
        "INSERT INTO workspace_messages
            (workspace_id, sender_type, sender_id, sender_name, content, message_type, metadata)
         VALUES ($1, 'system', 'bayesops', 'BayesOps', $2, 'system_event', $3)",
    )
    .bind(workspace_id)
    .bind(content)
    .bind(metadata)
    .execute(pool)
    .await?;
    let _ = event_kind; // currently embedded in metadata; reserved for future routing
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// REFIT OUTCOME HELPERS
// ═════════════════════════════════════════════════════════════════════════════

impl RefitOutcome {
    fn new(workspace_id: Uuid, drivers_considered: usize) -> Self {
        Self {
            workspace_id,
            drivers_considered,
            auto_accepted: 0,
            staged: 0,
            hard_blocked: 0,
            skipped: 0,
            per_driver: Vec::with_capacity(drivers_considered),
        }
    }

    fn empty(workspace_id: Uuid) -> Self {
        Self::new(workspace_id, 0)
    }

    fn tally(&mut self, outcome: &DriverOutcome) {
        match outcome.decision {
            DriverDecision::AutoAccepted { .. } => self.auto_accepted += 1,
            DriverDecision::Staged { .. } => self.staged += 1,
            DriverDecision::HardBlocked { .. } => self.hard_blocked += 1,
            DriverDecision::Skipped { .. } | DriverDecision::Errored { .. } => self.skipped += 1,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// MANUAL ENDPOINT: POST /api/workspaces/:id/refit
// ═════════════════════════════════════════════════════════════════════════════

use axum::{extract::State, http::StatusCode, Json};
use fermi_auth::AuthPrincipal;

use crate::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct RefitRequest {
    #[serde(default)]
    pub notes: Option<String>,
}

/// POST /api/workspaces/:workspace_id/refit
pub async fn refit_workspace_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
    Json(_req): Json<RefitRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;

    // Membership check — same pattern as resolution.rs
    fermi_auth::teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".into()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".into()))?;

    let outcome = refit_workspace(
        &state.db,
        &state.extractor_registry,
        ws_uuid,
        TriggerReason::Manual { user_id },
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!(outcome)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_from_team_slug() {
        assert_eq!(derive_entity_id("team-arg-2026"), Some("ARG".to_string()));
        assert_eq!(derive_entity_id("team_prior_arg"), Some("ARG".to_string()));
        assert_eq!(derive_entity_id("team-arg"), Some("ARG".to_string()));
        // Multi-entity slugs return None (can't disambiguate).
        assert_eq!(derive_entity_id("h2h-arg-vs-bra"), None);
        // Slugs without scaffolding words but multiple non-digit segments
        // are also ambiguous → None.
        assert_eq!(derive_entity_id("random-slug"), None);
    }

    #[test]
    fn classify_decision_thresholds() {
        use posterior::DataQuality::Sufficient;
        // Below default threshold: auto-accept
        assert!(matches!(
            classify_decision(1.5, DEFAULT_AUTO_ACCEPT_PP, Sufficient),
            DecisionKind::AutoAccept
        ));
        // At/above threshold but below hard-block: stage
        assert!(matches!(
            classify_decision(5.0, DEFAULT_AUTO_ACCEPT_PP, Sufficient),
            DecisionKind::Stage
        ));
        // Above hard-block: hard-block (takes precedence)
        assert!(matches!(
            classify_decision(25.0, DEFAULT_AUTO_ACCEPT_PP, Sufficient),
            DecisionKind::HardBlock
        ));
        // Per-driver threshold override
        assert!(matches!(
            classify_decision(2.5, 3.0, Sufficient),
            DecisionKind::AutoAccept
        ));
        assert!(matches!(
            classify_decision(3.5, 3.0, Sufficient),
            DecisionKind::Stage
        ));
    }

    /// The production state, as a test: two observations, no impact, accepted.
    ///
    /// Every snapshot the platform had produced looked like this — 55 of them,
    /// `quality: insufficient`, `n_eff 2.0`, mean delta `0.0000`, all
    /// `auto_accepted`. The fit was adopted BECAUSE it changed nothing, which
    /// treats "no evidence of harm" as "evidence of safety".
    #[test]
    fn an_insufficient_fit_is_staged_however_small_its_impact() {
        use posterior::DataQuality::Insufficient;
        // Impact of exactly zero — the case that auto-accepted all 55.
        assert!(matches!(
            classify_decision(0.0, DEFAULT_AUTO_ACCEPT_PP, Insufficient),
            DecisionKind::Stage
        ));
        // ...and well under the threshold, in case zero is special-cased later.
        assert!(matches!(
            classify_decision(0.5, DEFAULT_AUTO_ACCEPT_PP, Insufficient),
            DecisionKind::Stage
        ));
        // A wildly implausible impact is still hard-blocked: quality does not
        // rescue a fit that moves the forecast 25 points.
        assert!(matches!(
            classify_decision(25.0, DEFAULT_AUTO_ACCEPT_PP, Insufficient),
            DecisionKind::HardBlock
        ));
    }

    /// `Sparse` still auto-accepts, so the loop is not stalled by the fix.
    ///
    /// Holding 5-20 effective observations to the same bar as fewer than 5 would
    /// mean a driver with a modest history could never adopt anything, and the
    /// learning loop would be permanently pending review. The point of the change
    /// is to stop `Insufficient` being silently adopted, not to stop learning.
    #[test]
    fn a_sparse_fit_with_small_impact_still_auto_accepts() {
        use posterior::DataQuality::Sparse;
        assert!(matches!(
            classify_decision(1.0, DEFAULT_AUTO_ACCEPT_PP, Sparse),
            DecisionKind::AutoAccept
        ));
    }

    /// An impact that could not be assessed must not be treated as a small one.
    ///
    /// The previous code set `delta_pp = 0.0` when either simulation failed, with
    /// the comment "skip the gate, default to AutoAccept" — so a fit whose effect
    /// was UNKNOWN was adopted without review. Same defect as a cross-check that
    /// cannot run reporting healthy forever.
    ///
    /// The mechanism is NaN falling through both magnitude comparisons, which is
    /// correct and non-obvious, so it is pinned here rather than left to a reader's
    /// knowledge of IEEE 754.
    #[test]
    fn an_unassessable_impact_stages_rather_than_auto_accepting() {
        use posterior::DataQuality::Sufficient;
        assert!(matches!(
            classify_decision(f64::NAN, DEFAULT_AUTO_ACCEPT_PP, Sufficient),
            DecisionKind::Stage
        ));
        // Not hard-blocked either: the failure is in the assessment, not the fit.
        assert!(!matches!(
            classify_decision(f64::NAN, DEFAULT_AUTO_ACCEPT_PP, Sufficient),
            DecisionKind::HardBlock
        ));
    }

    /// A fit that leaves the centre alone and widens the interval has impact.
    ///
    /// `run_with_params` returned only `results.mean`, so this scored zero and was
    /// auto-accepted as harmless — on a platform where the interval is half the
    /// answer. The gate now takes the largest move across mean, p5 and p95.
    #[test]
    fn widening_the_interval_counts_as_impact() {
        let before = ImpactSample {
            mean: 0.30,
            p5: 0.28,
            p95: 0.32,
        };
        let after = ImpactSample {
            mean: 0.30,
            p5: 0.10,
            p95: 0.50,
        };
        let delta_pp = [
            (after.mean - before.mean).abs(),
            (after.p5 - before.p5).abs(),
            (after.p95 - before.p95).abs(),
        ]
        .into_iter()
        .fold(0.0_f64, f64::max)
            * 100.0;

        assert_eq!(
            (after.mean - before.mean).abs() * 100.0,
            0.0,
            "the mean is unchanged, which is why this used to score zero"
        );
        assert!(
            delta_pp > DEFAULT_AUTO_ACCEPT_PP,
            "an 18-point tail move must exceed the auto-accept threshold, got {delta_pp}"
        );
        assert!(matches!(
            classify_decision(delta_pp, DEFAULT_AUTO_ACCEPT_PP, posterior::DataQuality::Sufficient),
            DecisionKind::Stage
        ));
    }

    #[test]
    fn family_guess_beta_for_unit_interval() {
        assert!(matches!(
            guess_family(&[0.3, 0.5, 0.7, 0.42]),
            DistFamily::Beta
        ));
    }

    #[test]
    fn family_guess_normal_for_negative_or_unbounded() {
        assert!(matches!(
            guess_family(&[1.5, 2.0, -0.5]),
            DistFamily::Normal
        ));
        assert!(matches!(
            guess_family(&[10.0, 20.0, 30.0]),
            DistFamily::Normal
        ));
    }

    #[test]
    fn merge_params_adds_fitted_key() {
        let current = json!({ "static": 1.0 });
        let merged = merge_params(&current, "yield", &json!({ "family": "beta" }));
        assert_eq!(merged.get("static"), Some(&json!(1.0)));
        assert_eq!(
            merged.get("yield_fitted"),
            Some(&json!({ "family": "beta" }))
        );
    }

    #[test]
    fn merge_params_handles_empty_current() {
        let current = json!({});
        let merged = merge_params(&current, "x", &json!(42));
        assert_eq!(merged.get("x_fitted"), Some(&json!(42)));
    }

    #[test]
    fn merge_params_handles_non_object_current() {
        // Defensive: even if `params` is somehow not an object, the merge
        // produces a fresh object rather than crashing.
        let current = json!(null);
        let merged = merge_params(&current, "x", &json!(42));
        assert_eq!(merged.get("x_fitted"), Some(&json!(42)));
    }

    #[test]
    fn trigger_reason_as_provenance() {
        let t = TriggerReason::Manual {
            user_id: "alice".to_string(),
        };
        assert_eq!(t.as_provenance_string(), "manual:alice");

        let t = TriggerReason::Scheduled {
            job_id: "job-1".to_string(),
        };
        assert_eq!(t.as_provenance_string(), "scheduled:job-1");

        let t = TriggerReason::UpstreamResolution {
            upstream_workspace_id: Uuid::nil(),
        };
        assert!(t.as_provenance_string().starts_with("resolution:upstream:"));
    }
}
