//! Loop 5 — measured calibration, shared by the HTTP route and the tool.
//!
//! Lives in the library rather than under `handlers/` because it has two
//! consumers on opposite sides of the crate split: the axum route in
//! `handlers::agents` (bin) and the `get_agent_calibration` tool dispatch in
//! `agent_backend::tools_legacy` (lib). Keeping one implementation is the whole
//! point — the tool was declared on three strategist cards for months while the
//! only implementation sat behind an HTTP handler the agent could not call.

use agent_bestiary_memory::Agent;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize, Default)]
pub struct CalibrationQuery {
    /// `version` enables Doc 12 § Capability 4 partitioning. Any other value
    /// (or the default) keeps the legacy single-aggregate response shape.
    #[serde(default)]
    pub partition_by: Option<String>,
    /// Time window in days for observations; defaults to 90.
    #[serde(default)]
    pub window_days: Option<i64>,
    /// Optional workspace filter. When supplied, only observations whose
    /// session belongs to the workspace are counted.
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

/// Per-version row in the `version_partition.partitions` array.
fn calibration_partition_json(
    version_number: Option<i32>,
    version_deployed_at: Option<chrono::DateTime<chrono::Utc>>,
    n_observations: i64,
) -> Value {
    json!({
        "version_number": version_number,
        "version_deployed_at": version_deployed_at.map(|t| t.to_rfc3339()),
        "n_observations": n_observations,
        // Per-version Brier is intentionally NULL: the existing
        // `fermi_forecasts.agents_used` doesn't yet carry version stamps,
        // so a per-partition Brier mean would be spurious. The field stays
        // present so consumers can light it up later without restructuring.
        "n_resolved": 0,
        "brier_mean": Value::Null,
    })
}

/// The calibration computation, independent of axum.
///
/// Extracted so the `get_agent_calibration` **tool** and the HTTP route are the
/// same code. They were not: the route existed and was correct, the tool was
/// declared on three strategist cards with no dispatch arm, and
/// `moe_router_strategist` Stage 0 got `Unknown tool: get_agent_calibration`
/// every time it asked how well a candidate member forecasts. Loop 5's
/// producer side was complete and its only consumer could not read it.
pub async fn compute_agent_calibration(
    db: &sqlx::PgPool,
    db_agent: &Agent,
    q: &CalibrationQuery,
) -> Result<Value, String> {
    let aid = db_agent.agent_id;

    // ── eval_signals forecast_calibration scores ──────────────────────────────
    let signal_rows = sqlx::query(
        "SELECT score, confidence, created_at
         FROM eval_signals
         WHERE agent_id = $1 AND dimension = 'forecast_calibration'
         ORDER BY created_at DESC
         LIMIT 200",
    )
    .bind(aid)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    let n_eval = signal_rows.len();
    let eval_mean: Option<f64> = if n_eval > 0 {
        let sum: f64 = signal_rows
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum();
        Some(sum / n_eval as f64)
    } else {
        None
    };

    // Trend: compare last 10 vs prior 10 (if enough data)
    let trend = if n_eval >= 20 {
        let recent: f64 = signal_rows[..10]
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum::<f64>()
            / 10.0;
        let older: f64 = signal_rows[10..20]
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum::<f64>()
            / 10.0;
        if recent > older + 0.05 {
            "improving"
        } else if recent < older - 0.05 {
            "degrading"
        } else {
            "stable"
        }
    } else {
        "insufficient_data"
    };

    // ── fermi_forecasts direct Brier scores ───────────────────────────────────
    //
    // Attribution matches all three `agents_used` element shapes, mirroring
    // `handlers::eval_brier::BrierLookupSqlx::latest_for_agent` (which carries
    // the full shape inventory). Keep the two predicates in sync: when this
    // endpoint and the BrierEvaluator disagree about which forecasts belong to
    // an agent, the Observatory reports Loop 5.A as closed on one tab and
    // inactive on another. Matching only `agent_id` — the previous behaviour —
    // relied on mig-170's one-shot backfill and so missed every forecast
    // written by the live path after that backfill ran.
    let forecast_rows = sqlx::query(
        "SELECT brier_score, tags, question_text, actual_outcome, created_at
         FROM fermi_forecasts
         WHERE (
                agents_used @> $1::jsonb
             OR agents_used @> $2::jsonb
             OR agents_used @> $3::jsonb
           )
           AND brier_score IS NOT NULL
           AND status = 'resolved'
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(json!([{"agent_id": aid.to_string()}]))
    .bind(json!([{"agent_name": db_agent.agent_name}]))
    .bind(json!([{"name": db_agent.agent_name}]))
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    let n_resolved = forecast_rows.len();
    let brier_mean: Option<f64> = if n_resolved > 0 {
        // brier_score column is REAL → f32 in sqlx.
        let sum: f64 = forecast_rows
            .iter()
            .filter_map(|r| r.try_get::<f32, _>("brier_score").ok())
            .map(|v| v as f64)
            .sum();
        Some(sum / n_resolved as f64)
    } else {
        None
    };

    // ── Brier skill score against the base-rate reference ───────────────────
    let n_yes = forecast_rows
        .iter()
        .filter(|r| {
            r.try_get::<Option<bool>, _>("actual_outcome")
                .ok()
                .flatten()
                .unwrap_or(false)
        })
        .count();
    let (outcome_base_rate, brier_baseline, brier_skill_score) =
        brier_skill(brier_mean, n_yes, n_resolved);

    // ── Domain decomposition via agent tags ───────────────────────────────────
    // Group forecasts by matching against agent's tag categories.
    // Tags on forecasts are stored in the `tags` JSONB column.
    let mut domain_scores: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();

    for row in &forecast_rows {
        // brier_score is REAL → f32 in sqlx; cast to f64 for downstream math.
        let score: f64 = match row.try_get::<f32, _>("brier_score") {
            Ok(s) => s as f64,
            Err(_) => continue,
        };
        // forecast_calibration = 1 - brier (higher is better)
        let calibration = 1.0 - score.clamp(0.0, 1.0);

        let tags: Vec<String> = row
            .try_get::<serde_json::Value, _>("tags")
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Map forecast tags to domain using agent's own tags as the classifier
        let agent_tags = &db_agent.tags;
        let matched_domain = tags
            .iter()
            .find(|t| {
                agent_tags
                    .iter()
                    .any(|at| at.contains(t.as_str()) || t.contains(at.as_str()))
            })
            .map(|t| t.clone())
            .unwrap_or_else(|| "general".to_string());

        let entry = domain_scores.entry(matched_domain).or_insert((0.0, 0));
        entry.0 += calibration;
        entry.1 += 1;
    }

    let domain_calibration: serde_json::Value = domain_scores
        .iter()
        .map(|(domain, (sum, count))| {
            (
                domain.clone(),
                json!({
                    "calibration_mean": sum / *count as f64,
                    "n": count,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into();

    // ── eval_signals projection_accuracy scores (SimOps hard-verified) ───────
    // Hard-verified signal: deferred comparison against real SOSA observations.
    // Only populated for simops_dynamics_runner / simops_cascade agents.
    // These are epistemically stronger than LLM-judged signals — the batch
    // resolves independently of the prediction.
    let projection_rows = sqlx::query(
        "SELECT score, confidence, flags, created_at
         FROM eval_signals
         WHERE agent_id = $1 AND dimension = 'projection_accuracy'
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(aid)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    let n_projection = projection_rows.len();
    let projection_mean: Option<f64> = if n_projection > 0 {
        let sum: f64 = projection_rows
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum();
        Some(sum / n_projection as f64)
    } else {
        None
    };

    // Per-model breakdown from projection flags
    let mut model_accuracy: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();
    for row in &projection_rows {
        let score: f64 = match row.try_get::<f64, _>("score") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let flags: serde_json::Value = row
            .try_get::<serde_json::Value, _>("flags")
            .unwrap_or(serde_json::json!({}));
        let model_uri = flags
            .get("model_uri")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let entry = model_accuracy.entry(model_uri).or_insert((0.0, 0));
        entry.0 += score;
        entry.1 += 1;
    }
    let model_accuracy_json: serde_json::Value = model_accuracy
        .iter()
        .map(|(model, (sum, count))| {
            (
                model.clone(),
                json!({
                    "accuracy_mean": sum / *count as f64,
                    "n": count,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into();

    // ── Loop 5 contribution: exact Shapley credit (mig-188) ────────────────
    //
    // This is the per-AGENT signal. Everything above it is a per-TEAM signal:
    // `brier_mean` averages the Brier of forecasts this agent participated in,
    // which is a property of the composition, not of the agent. When every
    // member is cited on every forecast those team numbers are identical across
    // members by construction and can never rank them — the failure that
    // motivated all of this.
    //
    // Shapley credit measures how much this agent moved each forecast toward
    // its realised outcome, via counterfactual subset re-runs. It is reported
    // ALONGSIDE the team numbers rather than replacing them: they answer
    // different questions, `moe_router_strategist` already consumes
    // `calibration_score`, and silently redefining a live field is how a
    // measurement problem becomes a routing problem.
    //
    // Gated on both validity checks. An ungated read can act on credit derived
    // from Monte Carlo noise (efficiency_residual) or from a reconstruction of
    // a forecast that never existed (reconstruction_error).
    // Cluster key: what counts as one *independent* group of forecasts.
    //
    // Domain alone is too coarse — an entire World Cup is one domain, so every
    // forecast lands in a single cluster and no interval can ever be estimated.
    // Resolution month splits a long-running domain into genuinely separate
    // episodes of the world, which is the replication a cluster bootstrap needs.
    // Falling back to forecast_id treats an unclassifiable forecast as its own
    // cluster, which is the conservative choice (more clusters = narrower
    // interval), so it is used only when there is nothing better.
    let credit_rows = sqlx::query(
        "SELECT c.shapley_value,
                COALESCE(
                  NULLIF(f.domain, '') || ':' ||
                    to_char(COALESCE(f.resolved_at, f.created_at), 'YYYY-MM'),
                  c.forecast_id
                ) AS cluster_key
           FROM forecast_agent_credit c
           JOIN forecast_attributions a
             ON a.forecast_id = c.forecast_id
            AND a.neutralisation = c.neutralisation
           JOIN fermi_forecasts f ON f.id = c.forecast_id
          WHERE (c.agent_id = $1 OR c.agent_name = $2)
            AND c.neutralisation = 'identity'
            AND a.efficiency_residual < 1e-6
            AND (a.reconstruction_error IS NULL OR a.reconstruction_error < 0.01)",
    )
    .bind(aid)
    .bind(&db_agent.agent_name)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    let mut credits: Vec<f64> = Vec::with_capacity(credit_rows.len());
    let mut clusters: Vec<String> = Vec::with_capacity(credit_rows.len());
    for r in &credit_rows {
        if let Ok(v) = r.try_get::<f64, _>("shapley_value") {
            credits.push(v);
            clusters.push(r.try_get::<String, _>("cluster_key").unwrap_or_default());
        }
    }

    let n_credit = credits.len();
    let contribution_mean: Option<f64> = if n_credit > 0 {
        Some(credits.iter().sum::<f64>() / n_credit as f64)
    } else {
        None
    };
    // Share of forecasts the agent actually helped on. A positive mean driven by
    // one lucky forecast reads very differently from a consistent small edge.
    let positive_rate: Option<f64> = if n_credit > 0 {
        Some(credits.iter().filter(|v| **v > 0.0).count() as f64 / n_credit as f64)
    } else {
        None
    };
    let n_clusters = clusters
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    // Cluster bootstrap, seeded on the agent so the response is reproducible.
    // Returns None below 3 clusters: forecasts from a single tournament carry no
    // information about between-tournament variability, and a tight interval
    // computed from within-cluster spread would be false precision.
    let ci = crate::attribution::cluster_bootstrap_ci(
        &credits,
        &clusters,
        2000,
        crate::attribution::stable_seed(&db_agent.agent_name),
        0.10,
    );

    // ── Composite calibration score ───────────────────────────────────────────
    // Priority order (most authoritative first):
    //   1. Direct Brier from resolved fermi_forecasts
    //   2. projection_accuracy from hard-verified SOSA deltas (SimOps)
    //   3. eval_signals forecast_calibration from LLM-judged evaluators
    let calibration_score = match (brier_mean, projection_mean, eval_mean) {
        (Some(b), _, _) => Some(1.0 - b), // Brier inverted: lower = higher calibration
        (None, Some(p), _) => Some(p),    // projection_accuracy: already 0-1 higher=better
        (None, None, Some(e)) => Some(e),
        _ => None,
    };

    // Confidence: saturates at n=20. Count across all signal sources.
    let n_total = n_resolved.max(n_projection).max(n_eval);
    let confidence = (n_total as f64 / 20.0).min(1.0);

    // ── Doc 12 § Capability 4 — optional version partitioning ────────────────
    //
    // When the caller asks for `?partition_by=version`, attach per-version
    // observation counts from `sosa_observations.produced_by_version_*`
    // (stamped by Doc 12 § Capability 1). Honest about the limit: per-version
    // Brier scores stay NULL because `fermi_forecasts.agents_used` doesn't
    // carry `agent_version_id` yet — wiring that is the prerequisite for
    // version-partitioned Brier and is documented in the response.
    let partition_by = q.partition_by.as_deref().unwrap_or("none");
    let window_days = q.window_days.unwrap_or(90).max(1);

    let partitions_block: Option<Value> = if partition_by == "version" {
        let cutoff_ms =
            (chrono::Utc::now() - chrono::Duration::days(window_days)).timestamp_millis();

        let part_rows = sqlx::query(
            "SELECT o.produced_by_version_number AS version_number,
                    v.created_at                 AS version_deployed_at,
                    COUNT(*)::BIGINT             AS n_observations
             FROM sosa_observations o
             LEFT JOIN agent_versions v
               ON v.agent_id = $1
              AND v.version_number = o.produced_by_version_number
             WHERE (o.produced_by_agent_id = $2 OR o.produced_by_agent_id = $3)
               AND o.phenomenon_time >= $4
               AND ($5::uuid IS NULL OR o.session_id IN (
                     SELECT session_id FROM observation_sessions WHERE platform_id IN (
                       SELECT platform_id FROM sosa_platforms WHERE owner_id IN (
                         SELECT user_id FROM workspaces WHERE workspace_id = $5))))
             GROUP BY o.produced_by_version_number, v.created_at
             ORDER BY o.produced_by_version_number ASC NULLS LAST",
        )
        .bind(aid)
        .bind(&db_agent.agent_name)
        .bind(aid.to_string())
        .bind(cutoff_ms)
        .bind(q.workspace_id)
        .fetch_all(db)
        .await
        .map_err(|e| format!("Query failed: {}", e))?;

        let partitions: Vec<Value> = part_rows
            .iter()
            .map(|r| {
                let vn: Option<i32> = r.try_get("version_number").ok();
                let vt: Option<chrono::DateTime<chrono::Utc>> =
                    r.try_get("version_deployed_at").ok();
                let n: i64 = r.try_get("n_observations").unwrap_or(0);
                calibration_partition_json(vn, vt, n)
            })
            .collect();

        Some(json!({
            "partition_by": "version",
            "window_days": window_days,
            "partitions": partitions,
            // Honest v1 disclosure (Doc 12 § Capability 4). Consumers can
            // read `partitions` for version-stamped observation counts now;
            // per-version Brier requires `agent_version_id` to also be
            // recorded in `fermi_forecasts.agents_used` entries — a
            // downstream change tracked separately from this endpoint.
            "brier_status": "unstamped",
            "brier_note": "Per-version Brier requires version-stamped forecasts in fermi_forecasts.agents_used; current rows surface observation counts only.",
        }))
    } else {
        None
    };

    Ok(json!({
        "agent_id": aid,
        "agent_name": db_agent.agent_name,

        // Primary calibration score (0.0–1.0, higher = better calibrated)
        "calibration_score": calibration_score,
        "confidence": confidence,
        "trend": trend,

        // Source breakdown
        "n_resolved_forecasts": n_resolved,
        "n_eval_signals": n_eval,
        "n_projection_observations": n_projection,
        "brier_mean": brier_mean,                     // direct Brier (lower = better)
        "eval_calibration_mean": eval_mean,           // LLM-judged signals (higher = better)
        "projection_accuracy_mean": projection_mean,  // hard-verified SOSA delta (higher = better)

        // ── Scope warning for every field above ──────────────────────────
        // `calibration_score`, `brier_mean` and `brier_skill_score` are TEAM
        // measurements: they describe the forecasts this agent participated in,
        // not this agent's own contribution. On a composition that cites every
        // member on every forecast they are identical across members and cannot
        // rank them. Use `contribution.mean_shapley` for per-agent skill.
        "score_scope": "team",
        "score_scope_note": "calibration_score/brier_mean/brier_skill_score describe the forecasts this agent participated in, not its individual contribution. For per-agent credit read `contribution`.",

        // Skill decomposition — is `calibration_score` informative, or just
        // base-rate skew? `brier_skill_score` > 0 means the agent beat a
        // forecaster that predicts `outcome_base_rate` on every question.
        // Null when there are no resolved forecasts, or when every outcome
        // resolved the same way (no reference to score against).
        "outcome_base_rate": outcome_base_rate,
        "brier_baseline": brier_baseline,
        "brier_skill_score": brier_skill_score,
        "beats_base_rate": brier_skill_score.map(|s| s > 0.0),

        // How much this signal currently means:
        //   none | undiscriminating | no_skill | provisional | thin | usable
        // Loop 5.A (Brier) closed recently, so most agents sit at
        // provisional/thin. Mechanism soundness is a separate question —
        // verify it with scripts/loop5_brier_mechanical_check.sql.
        "evidence_class": evidence_class(n_resolved, brier_baseline, brier_skill_score),

        // ── Per-agent contribution (Loop 5's real signal) ────────────────
        // Exact Shapley credit from counterfactual subset re-runs, averaged over
        // resolved forecasts that passed both validity gates. Positive means the
        // agent moved forecasts toward their realised outcomes.
        //
        // Only populated for forecasts resolved after the claim ledger
        // (mig-187) shipped: attribution needs to know what each agent
        // individually claimed, and historical claims were overwritten rather
        // than retained, so they cannot be reconstructed.
        "contribution": {
            "scope": "agent",
            "neutralisation": "identity",
            "mean_shapley": contribution_mean,
            "n_forecasts": n_credit,
            // Distinct correlated groups behind those forecasts. The interval
            // below is estimated across these, not across forecasts.
            "n_clusters": n_clusters,
            "positive_rate": positive_rate,
            "ci_low": ci.map(|c| c.0),
            "ci_high": ci.map(|c| c.1),
            "ci_method": "cluster_bootstrap_p90",
            // Actionable rather than merely negative: say how many more
            // independent groups are needed, and what "independent" means here.
            "clusters_required": crate::attribution::MIN_BOOTSTRAP_CLUSTERS,
            "clusters_needed": crate::attribution::MIN_BOOTSTRAP_CLUSTERS
                .saturating_sub(n_clusters),
            "cluster_key": "domain + resolution month",
            "how_to_unblock": if n_clusters >= crate::attribution::MIN_BOOTSTRAP_CLUSTERS {
                Value::Null
            } else {
                json!(format!(
                    "Resolve forecasts in {} more distinct (domain, month) group(s). Volume \
                     within one group does not help: 48 forecasts from a single tournament \
                     are one observation of the world, not 48.",
                    crate::attribution::MIN_BOOTSTRAP_CLUSTERS.saturating_sub(n_clusters)
                ))
            },
            "ci_note": if n_credit == 0 {
                "No attributed forecasts yet."
            } else if ci.is_none() {
                "Interval undefined: fewer than 3 independent clusters. All evidence comes from one correlated group (e.g. a single tournament), which carries no information about between-group variability. Treat the mean as a point observation, not an estimate."
            } else {
                "90% cluster bootstrap over correlated forecast groups."
            },
            "gates_applied": "efficiency_residual < 1e-6 AND (reconstruction_error IS NULL OR < 0.01)",
        },

        // Per-domain decomposition (requires forecast tags to match agent tags)
        "domain_calibration": domain_calibration,

        // Per-model accuracy (SimOps agents: accuracy per dynamics model URI)
        "model_accuracy": model_accuracy_json,

        // Per-version decomposition (Doc 12 § Capability 4). Present only when
        // the caller passed `?partition_by=version`.
        "version_partition": partitions_block,

        // Interpretation
        "interpretation": match calibration_score {
            Some(s) if s >= 0.80 => "well_calibrated",
            Some(s) if s >= 0.65 => "reasonably_calibrated",
            Some(s) if s >= 0.50 => "weakly_calibrated",
            Some(_) => "poorly_calibrated",
            None => "no_data",
        },
        "note": if n_resolved < 5 && n_projection < 5 {
            Some("Fewer than 5 hard-verified observations — calibration estimate is preliminary.")
        } else if matches!(brier_skill_score, Some(s) if s <= 0.0) {
            Some("Raw calibration is not skill: this agent does not beat a forecaster that predicts the base rate on every question. Treat calibration_score as uninformative here.")
        } else {
            None
        },
    }))
}

fn evidence_class(n_resolved: usize, baseline: Option<f64>, skill: Option<f64>) -> &'static str {
    match (n_resolved, baseline, skill) {
        (0, _, _) => "none",
        // One-sided outcomes: no reference to score against, so the raw number
        // is uninformative however large n gets.
        (_, Some(b), _) if b <= 1e-9 => "undiscriminating",
        (_, _, Some(s)) if s <= 0.0 => "no_skill",
        (n, _, _) if n < 5 => "provisional",
        (n, _, _) if n < 20 => "thin",
        _ => "usable",
    }
}

pub fn brier_skill(
    brier_mean: Option<f64>,
    n_yes: usize,
    n_resolved: usize,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    let base_rate: Option<f64> = if n_resolved > 0 {
        Some(n_yes as f64 / n_resolved as f64)
    } else {
        None
    };
    let baseline: Option<f64> = base_rate.map(|b| b * (1.0 - b));
    let skill: Option<f64> = match (brier_mean, baseline) {
        (Some(b), Some(base)) if base > 1e-9 => Some(1.0 - b / base),
        _ => None,
    };
    (base_rate, baseline, skill)
}

#[cfg(test)]
mod brier_skill_tests {
    use super::{brier_skill, evidence_class};

    /// Thin data must never read as authoritative, and "mechanically fine but
    /// informationally empty" must be distinguishable from "actually bad".
    #[test]
    fn evidence_class_separates_thinness_from_badness() {
        // (n_resolved, baseline, skill)
        assert_eq!(evidence_class(0, None, None), "none");
        // One-sided outcome set: baseline 0, so the raw score means nothing
        // no matter how many forecasts accumulate.
        assert_eq!(evidence_class(500, Some(0.0), None), "undiscriminating");
        // Real evidence, genuinely bad performance.
        assert_eq!(evidence_class(50, Some(0.25), Some(-0.3)), "no_skill");
        // Skilled but too little of it to lean on — the current situation.
        assert_eq!(evidence_class(3, Some(0.25), Some(0.8)), "provisional");
        assert_eq!(evidence_class(12, Some(0.25), Some(0.8)), "thin");
        assert_eq!(evidence_class(40, Some(0.25), Some(0.8)), "usable");
    }

    /// The World Cup case that motivated this: 48 tournament-winner forecasts,
    /// 47 resolving NO and one YES. A flat, zero-knowledge p = 1/48 across all
    /// 48 scores a mean Brier equal to the baseline exactly — so skill is 0,
    /// even though the raw score displays as ~98% "calibrated".
    #[test]
    fn uniform_prior_on_skewed_set_has_zero_skill() {
        let b = 1.0 / 48.0;
        // Mean Brier of the flat-1/48 forecaster: 47 misses at b^2, one hit at (1-b)^2.
        let brier = (47.0 * b * b + (1.0 - b) * (1.0 - b)) / 48.0;

        let (base_rate, baseline, skill) = brier_skill(Some(brier), 1, 48);

        assert!((base_rate.unwrap() - b).abs() < 1e-12);
        // Baseline b(1-b) = 0.0204 — i.e. "98%" on the display scale.
        assert!(
            (baseline.unwrap() - 0.020399).abs() < 1e-5,
            "{:?}",
            baseline
        );
        assert!(
            skill.unwrap().abs() < 1e-12,
            "a flat base-rate prior must score exactly zero skill, got {:?}",
            skill
        );
    }

    /// A genuinely skilled forecaster on the same skewed set clears zero.
    #[test]
    fn confident_correct_forecaster_has_positive_skill() {
        // Brier well below the 0.0204 baseline.
        let (_, _, skill) = brier_skill(Some(0.005), 1, 48);
        assert!(skill.unwrap() > 0.7, "{:?}", skill);
    }

    /// Worse than the base rate must read as negative skill, not as a high
    /// raw score. This is the case the Loops tab now refuses to call "closed".
    #[test]
    fn worse_than_base_rate_has_negative_skill() {
        let (_, _, skill) = brier_skill(Some(0.04), 1, 48);
        assert!(skill.unwrap() < 0.0, "{:?}", skill);
    }

    /// Degenerate set: every outcome resolved NO. Baseline is 0, so skill is
    /// undefined rather than infinite — the endpoint reports null and the UI
    /// says "skill undefined" instead of claiming a closed loop.
    #[test]
    fn all_outcomes_alike_leaves_skill_undefined() {
        let (base_rate, baseline, skill) = brier_skill(Some(0.001), 0, 30);
        assert_eq!(base_rate, Some(0.0));
        assert_eq!(baseline, Some(0.0));
        assert_eq!(skill, None);

        let (_, _, skill_all_yes) = brier_skill(Some(0.001), 30, 30);
        assert_eq!(skill_all_yes, None);
    }

    /// No resolved forecasts at all — everything undefined, nothing inferred.
    #[test]
    fn no_forecasts_yields_no_baseline() {
        assert_eq!(brier_skill(None, 0, 0), (None, None, None));
    }

    /// A balanced set has baseline 0.25, the familiar coin-flip reference.
    #[test]
    fn balanced_set_baseline_is_one_quarter() {
        let (base_rate, baseline, skill) = brier_skill(Some(0.125), 25, 50);
        assert_eq!(base_rate, Some(0.5));
        assert!((baseline.unwrap() - 0.25).abs() < 1e-12);
        assert!((skill.unwrap() - 0.5).abs() < 1e-12);
    }
}
