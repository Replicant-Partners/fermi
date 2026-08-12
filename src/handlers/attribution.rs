//! Resolution-time attribution job — turns a resolved forecast into per-agent
//! Shapley credit and pairwise interaction indices.
//!
//! Runs alongside `record_forecast_calibration_signals` on both resolution paths
//! (the API `/resolve` handler and the Polymarket oracle). Best-effort: a
//! failure here must never fail a resolution, but it is logged, because a
//! missing attribution is a missing Loop 5 signal.
//!
//! ## What it does
//!
//! 1. Loads the forecast's model (`fpl_source`), outcome, and scored probability.
//! 2. Reconstructs what each agent individually claimed, from the append-only
//!    `forecast_agent_claims` ledger (mig-187).
//! 3. Re-runs the model over all `2^n` agent subsets under a fixed seed derived
//!    from the forecast id, producing the counterfactual value function.
//! 4. Computes exact Shapley credit and pairwise interactions
//!    (`fermi::attribution`).
//! 5. **Gates on two validity checks** before persisting anything (below).
//! 6. Writes header + per-agent credit + interactions (mig-188), idempotently.
//!
//! ## The two gates, and why writing without them would be worse than nothing
//!
//! Loop 4 and Loop 5 are optimisers. A confounded credit signal does not merely
//! fail to help — they will faithfully optimise toward it, concentrating work on
//! agents that were lucky and pruning agents whose contribution was real but
//! misattributed. So the job refuses to write a number it cannot vouch for.
//!
//! - **Efficiency residual** — `|Σφᵢ - (v(N)-v(∅))|`. Exact Shapley makes this
//!   ~1e-12. Larger means the value function was not deterministic across subset
//!   evaluations, so Monte Carlo noise has been redistributed as agent credit.
//!   This is not hypothetical: the executor used to sample drivers in `HashMap`
//!   order, so `with_seed` did not reproduce a run, and an agent claiming the
//!   neutral value — provably worth zero — earned ~1e-4 of phantom credit.
//!
//! - **Reconstruction error** — `|p_full - scored_probability|`. Attribution is
//!   only *about the real forecast* if applying every agent's claim reproduces
//!   the probability resolution actually scored. If it does not, we have the
//!   wrong claims, the wrong model snapshot, or the wrong params, and the φ
//!   values describe a forecast that never existed. Checked as a warning rather
//!   than a hard refusal because a legitimate gap exists today (see below), but
//!   always recorded.

use std::collections::BTreeMap;

use fermi::attribution::{
    counterfactual::{
        attribute_forecast, AgentClaims, CounterfactualModel, DriverClaim, Neutralisation,
    },
    stable_seed,
};
use sqlx::{PgPool, Row};

/// Monte Carlo iterations per subset evaluation. Every subset uses the same
/// seed, so this trades wall-clock against the sampling error *within* a single
/// counterfactual, not against comparability between them.
const ATTRIBUTION_ITERATIONS: usize = 10_000;

/// Refuse to persist above this. Exact Shapley over a deterministic value
/// function lands ~1e-12; 1e-6 is loose enough to tolerate f64 summation over a
/// few hundred subsets and tight enough that any real nondeterminism trips it.
const MAX_EFFICIENCY_RESIDUAL: f64 = 1e-6;

/// Above this, the reconstruction does not describe the scored forecast. Warned
/// and recorded rather than refused — see `reconstruction_error` note below.
const RECONSTRUCTION_WARN_THRESHOLD: f64 = 0.01;

/// Outcome of an attribution attempt, for logging and tests.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributionOutcome {
    /// Written. Carries the per-agent credit for logging.
    Written { n_agents: usize, residual: f64 },
    /// Nothing to attribute — not an error. A forecast with no recorded claims
    /// predates the claim ledger, and cannot be reconstructed after the fact.
    NoClaims,
    /// Computed but rejected by a validity gate; nothing was written.
    Rejected { reason: String },
}

/// Attribute one resolved forecast and persist the result.
///
/// `mode` selects the counterfactual question: [`Neutralisation::Identity`]
/// measures credit against silence, [`Neutralisation::Reference`] against an
/// average replacement. The two are not comparable, and a forecast may carry one
/// attribution per mode.
pub async fn attribute_resolved_forecast(
    pool: &PgPool,
    forecast_id: &str,
    mode: Neutralisation,
) -> Result<AttributionOutcome, String> {
    // ── 1. The forecast: model, outcome, and the probability actually scored ──
    //
    // `scored_probability` is mig-174's frozen audit anchor; `predicted_
    // probability` stays mutable after resolution and so cannot be used as the
    // reconstruction target. COALESCE only as a fallback for pre-mig-174 rows.
    let row = sqlx::query(
        "SELECT fpl_source,
                actual_outcome,
                COALESCE(scored_probability, predicted_probability) AS target_probability,
                workspace_id,
                COALESCE(resolved_at, updated_at, created_at)       AS as_of
           FROM fermi_forecasts
          WHERE id = $1 AND status = 'resolved' AND brier_score IS NOT NULL",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("forecast lookup failed: {e}"))?
    .ok_or_else(|| format!("forecast {forecast_id} is not resolved-and-scored"))?;

    let fpl_source: Option<String> = row.try_get("fpl_source").ok().flatten();
    let Some(fpl_source) = fpl_source.filter(|s| !s.trim().is_empty()) else {
        // No model, no counterfactual. Attribution is inherently model-based:
        // it measures contribution *through* a specified decomposition.
        return Ok(AttributionOutcome::NoClaims);
    };
    let outcome: bool = row
        .try_get::<Option<bool>, _>("actual_outcome")
        .ok()
        .flatten()
        .ok_or_else(|| "resolved forecast has no actual_outcome".to_string())?;
    let target_probability: Option<f64> = row
        .try_get::<Option<f32>, _>("target_probability")
        .ok()
        .flatten()
        .map(|v| v as f64);
    let workspace_id: Option<uuid::Uuid> = row.try_get("workspace_id").ok().flatten();
    let as_of: Option<chrono::DateTime<chrono::Utc>> = row.try_get("as_of").ok().flatten();

    // ── 2. What each agent individually claimed ──────────────────────────────
    let agents = load_agent_claims(pool, forecast_id, workspace_id, as_of).await?;
    if agents.is_empty() {
        tracing::info!(
            forecast = %forecast_id,
            "[attribution] no recorded agent claims — predates the claim ledger, \
             cannot be reconstructed"
        );
        return Ok(AttributionOutcome::NoClaims);
    }

    // ── 3-4. Counterfactual enumeration + exact Shapley ──────────────────────
    //
    // Seed is a pure function of the forecast id so a recomputation months from
    // now reproduces this attribution byte-for-byte.
    let seed = stable_seed(forecast_id);
    let model = CounterfactualModel::from_source(&fpl_source, ATTRIBUTION_ITERATIONS, seed)?;
    let att = attribute_forecast(&model, &agents, outcome, mode)?;

    // ── 5. Validity gates ────────────────────────────────────────────────────
    let residual = att.shapley.efficiency_residual();
    if !residual.is_finite() || residual > MAX_EFFICIENCY_RESIDUAL {
        let reason = format!(
            "efficiency residual {residual:.3e} exceeds {MAX_EFFICIENCY_RESIDUAL:.0e} — the \
             value function was not deterministic, so credit is polluted by Monte Carlo noise"
        );
        tracing::error!(forecast = %forecast_id, "[attribution] refusing to write: {reason}");
        return Ok(AttributionOutcome::Rejected { reason });
    }

    let reconstruction_error = target_probability.map(|t| (att.p_full - t).abs());
    if let Some(err) = reconstruction_error {
        if err > RECONSTRUCTION_WARN_THRESHOLD {
            // Recorded, not refused. A known legitimate cause exists: the claim
            // ledger binds by as-of time, so a forecast whose params were also
            // touched by something other than an agent claim (a BayesOps refit,
            // a manual edit) will not reconstruct exactly from claims alone.
            // Refusing outright would suppress attribution for every
            // BayesOps-fitted forecast; the column lets consumers filter.
            tracing::warn!(
                forecast = %forecast_id, error = err, p_full = att.p_full,
                "[attribution] reconstruction does not match the scored probability — \
                 φ describes a forecast that differs from the one resolved"
            );
        }
    }

    // Interactions come from the subset probabilities already computed above —
    // no further model runs, and guaranteed to share the same randomness as the
    // marginals, so the two cannot disagree.
    let interactions = att.interactions();

    // ── 6. Persist ───────────────────────────────────────────────────────────
    persist_attribution(
        pool,
        forecast_id,
        mode,
        &att,
        target_probability,
        reconstruction_error,
        interactions.as_ref(),
    )
    .await?;

    tracing::info!(
        forecast = %forecast_id, agents = agents.len(), mode = mode.as_str(),
        residual = residual, team_improvement = att.shapley.total,
        "[attribution] wrote per-agent Shapley credit"
    );

    Ok(AttributionOutcome::Written {
        n_agents: agents.len(),
        residual,
    })
}

/// Reconstruct each agent's claims for a forecast.
///
/// Prefers claims already bound to the forecast (`forecast_id` stamped) because
/// an explicit binding beats a temporal inference. Falls back to the as-of join:
/// for each `(workspace, driver)`, the most recent claim at or before the moment
/// the forecast was scored.
///
/// Grouping is by agent, so an agent covering several drivers is a single
/// Shapley player and receives one credit value. Group by driver instead for the
/// finer decomposition — the machinery is identical.
async fn load_agent_claims(
    pool: &PgPool,
    forecast_id: &str,
    workspace_id: Option<uuid::Uuid>,
    as_of: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<AgentClaims>, String> {
    // DISTINCT ON (driver) with ORDER BY claimed_at DESC gives the latest claim
    // per driver — one claimant per driver, which is what the model expects
    // since a driver has exactly one parameter triple.
    let rows = sqlx::query(
        "SELECT DISTINCT ON (driver)
                driver, agent_id, agent_name, p5, p50, p95, neutral_value
           FROM forecast_agent_claims
          WHERE ( forecast_id = $1
               OR ($2::uuid IS NOT NULL AND workspace_id = $2
                   AND ($3::timestamptz IS NULL OR claimed_at <= $3)) )
          ORDER BY driver,
                   (forecast_id = $1) DESC,   -- explicit binding wins
                   claimed_at DESC",
    )
    .bind(forecast_id)
    .bind(workspace_id)
    .bind(as_of)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("claim lookup failed: {e}"))?;

    // BTreeMap so player order is deterministic — the agent→bit assignment must
    // be stable or a recomputation would permute the credit vector.
    let mut by_agent: BTreeMap<String, (Option<uuid::Uuid>, Vec<DriverClaim>)> = BTreeMap::new();
    for r in &rows {
        let agent_name: String = r.try_get("agent_name").unwrap_or_default();
        if agent_name.is_empty() {
            continue;
        }
        let agent_id: Option<uuid::Uuid> = r.try_get("agent_id").ok().flatten();
        let driver: String = r.try_get("driver").unwrap_or_default();
        let p50: f64 = match r.try_get::<f32, _>("p50") {
            Ok(v) => v as f64,
            Err(_) => continue,
        };
        // p5/p95 are nullable; a point claim degrades to a degenerate triangular
        // at p50 rather than being dropped.
        let p5 = r
            .try_get::<Option<f32>, _>("p5")
            .ok()
            .flatten()
            .map(|v| v as f64)
            .unwrap_or(p50);
        let p95 = r
            .try_get::<Option<f32>, _>("p95")
            .ok()
            .flatten()
            .map(|v| v as f64)
            .unwrap_or(p50);
        let neutral = r
            .try_get::<Option<f32>, _>("neutral_value")
            .ok()
            .flatten()
            .map(|v| v as f64)
            .unwrap_or(1.0);

        let entry = by_agent.entry(agent_name).or_insert((agent_id, Vec::new()));
        entry.1.push(DriverClaim {
            driver,
            p5,
            p50,
            p95,
            neutral_value: neutral,
            reference: None,
        });
    }

    Ok(by_agent
        .into_iter()
        .map(|(agent_name, (_, drivers))| AgentClaims {
            agent_name,
            drivers,
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
async fn persist_attribution(
    pool: &PgPool,
    forecast_id: &str,
    mode: Neutralisation,
    att: &fermi::attribution::counterfactual::ForecastAttribution,
    scored_probability: Option<f64>,
    reconstruction_error: Option<f64>,
    interactions: Option<&std::collections::HashMap<(usize, usize), f64>>,
) -> Result<(), String> {
    // One transaction: a header without its credit rows, or credit rows without
    // a header, would both be misleading to the Loop 5 read path.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("begin failed: {e}"))?;

    sqlx::query(
        "INSERT INTO forecast_attributions
             (forecast_id, neutralisation, seed, iterations, n_players, outcome,
              p_baseline, p_full, scored_probability, team_improvement,
              efficiency_residual, reconstruction_error, computed_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12, NOW())
         ON CONFLICT (forecast_id, neutralisation) DO UPDATE SET
             seed = EXCLUDED.seed,
             iterations = EXCLUDED.iterations,
             n_players = EXCLUDED.n_players,
             outcome = EXCLUDED.outcome,
             p_baseline = EXCLUDED.p_baseline,
             p_full = EXCLUDED.p_full,
             scored_probability = EXCLUDED.scored_probability,
             team_improvement = EXCLUDED.team_improvement,
             efficiency_residual = EXCLUDED.efficiency_residual,
             reconstruction_error = EXCLUDED.reconstruction_error,
             computed_at = NOW()",
    )
    .bind(forecast_id)
    .bind(mode.as_str())
    .bind(att.seed as i64)
    .bind(ATTRIBUTION_ITERATIONS as i32)
    .bind(att.agent_names.len() as i32)
    .bind(att.outcome)
    .bind(att.p_baseline as f32)
    .bind(att.p_full as f32)
    .bind(scored_probability.map(|v| v as f32))
    .bind(att.shapley.total)
    .bind(att.shapley.efficiency_residual())
    .bind(reconstruction_error)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("header upsert failed: {e}"))?;

    // Replace rather than merge: the roster may have shrunk between runs, and a
    // stale credit row for an agent no longer on the forecast would be read as
    // real evidence.
    sqlx::query("DELETE FROM forecast_agent_credit WHERE forecast_id = $1 AND neutralisation = $2")
        .bind(forecast_id)
        .bind(mode.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("credit clear failed: {e}"))?;

    for (name, phi) in att.agent_names.iter().zip(att.shapley.values.iter()) {
        sqlx::query(
            "INSERT INTO forecast_agent_credit
                 (forecast_id, neutralisation, agent_id, agent_name, shapley_value)
             SELECT $1, $2, (SELECT agent_id FROM agents WHERE agent_name = $3 LIMIT 1), $3, $4",
        )
        .bind(forecast_id)
        .bind(mode.as_str())
        .bind(name)
        .bind(*phi)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("credit insert failed for {name}: {e}"))?;
    }

    sqlx::query(
        "DELETE FROM forecast_agent_interactions WHERE forecast_id = $1 AND neutralisation = $2",
    )
    .bind(forecast_id)
    .bind(mode.as_str())
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("interaction clear failed: {e}"))?;

    if let Some(ix) = interactions {
        for ((i, j), value) in ix {
            let (Some(a), Some(b)) = (att.agent_names.get(*i), att.agent_names.get(*j)) else {
                continue;
            };
            // The table's CHECK requires agent_a < agent_b so each unordered
            // pair is stored once regardless of player indexing.
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            if a == b {
                continue;
            }
            sqlx::query(
                "INSERT INTO forecast_agent_interactions
                     (forecast_id, neutralisation, agent_a, agent_b, interaction_index)
                 VALUES ($1,$2,$3,$4,$5)
                 ON CONFLICT (forecast_id, neutralisation, agent_a, agent_b)
                 DO UPDATE SET interaction_index = EXCLUDED.interaction_index",
            )
            .bind(forecast_id)
            .bind(mode.as_str())
            .bind(a)
            .bind(b)
            .bind(*value)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("interaction insert failed: {e}"))?;
        }
    }

    tx.commit()
        .await
        .map_err(|e| format!("commit failed: {e}"))?;
    Ok(())
}

/// Fire-and-forget wrapper for the resolution paths.
///
/// Attribution is expensive relative to a resolution response (2^n model runs),
/// and must never delay or fail one. Errors are logged, not propagated.
pub fn spawn_attribution(pool: &PgPool, forecast_id: &str) {
    let pool = pool.clone();
    let forecast_id = forecast_id.to_string();
    tokio::spawn(async move {
        match attribute_resolved_forecast(&pool, &forecast_id, Neutralisation::Identity).await {
            Ok(AttributionOutcome::Written { n_agents, .. }) => {
                tracing::debug!(forecast = %forecast_id, agents = n_agents, "[attribution] done");
            }
            Ok(AttributionOutcome::NoClaims) => {}
            Ok(AttributionOutcome::Rejected { reason }) => {
                tracing::error!(forecast = %forecast_id, "[attribution] rejected: {reason}");
            }
            Err(e) => {
                tracing::warn!(forecast = %forecast_id, error = %e, "[attribution] failed");
            }
        }
    });
}
