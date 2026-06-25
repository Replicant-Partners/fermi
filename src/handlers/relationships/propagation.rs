//! Propagation logic — dispatch by kind: mutex / at_most_n / implies.
//!
//! Spec 25 §3 + §8. Pure functions of (group, members, current_probs,
//! trigger, trigger_kind, outcome) -> Vec<(forecast_id, prev, new)>.
//! They don't write — `apply.rs` does that.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};

#[derive(Debug, Deserialize)]
pub struct PropagateRequest {
    pub trigger_forecast_id: String,
    pub trigger_kind: String,
    pub outcome: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PropagateResult {
    pub n_updated: usize,
    pub deltas: Vec<DeltaEntry>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DeltaEntry {
    pub forecast_id: String,
    pub previous_probability: f64,
    pub new_probability: f64,
    pub delta_pp: f64,
}

pub async fn dispatch_propagation(
    kind: &str,
    forecast_ids: &[String],
    parameters: &JsonValue,
    req: &PropagateRequest,
    pool: &PgPool,
    dry_run: bool,
) -> Result<PropagateResult, (StatusCode, String)> {
    match kind {
        "mutually_exclusive" | "mutex" => {
            propagate_mutex(forecast_ids, parameters, req, pool, dry_run).await
        }
        "at_most_n" => {
            propagate_at_most_n(forecast_ids, parameters, req, pool, dry_run).await
        }
        "implies" => {
            propagate_implies(forecast_ids, parameters, req, pool, dry_run).await
        }
        "logical_implies" | "conjunction" | "conditional" | "exhaustive_cover" => Err((
            StatusCode::NOT_IMPLEMENTED,
            format!("Legacy kind '{}' — use the new group model instead", kind),
        )),
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown kind: {}", other),
        )),
    }
}

pub async fn dispatch_propagation_group(
    kind: &str,
    group_id: &str,
    parameters: &JsonValue,
    req: &PropagateRequest,
    pool: &PgPool,
    dry_run: bool,
) -> Result<PropagateResult, (StatusCode, String)> {
    let forecast_ids = get_group_member_ids(group_id, pool).await?;
    if forecast_ids.is_empty() {
        return Ok(PropagateResult {
            n_updated: 0,
            deltas: vec![],
            note: Some("No members in this group".into()),
        });
    }
    dispatch_propagation(kind, &forecast_ids, parameters, req, pool, dry_run).await
}

pub async fn get_group_member_ids(
    group_id: &str,
    pool: &PgPool,
) -> Result<Vec<String>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT id FROM public.fermi_forecasts
          WHERE relationship_groups @> ARRAY[$1]
          ORDER BY id",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(rows.iter().filter_map(|r| r.try_get::<String, _>("id").ok()).collect())
}

/// Members that have already resolved (actual_outcome IS NOT NULL).
/// These are factual now — a mutex/at_most_n rebalance must NOT
/// redistribute mass onto them, and they must not be re-zeroed.
async fn read_resolved_ids(
    forecast_ids: &[String],
    pool: &PgPool,
) -> Result<std::collections::HashSet<String>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT id FROM public.fermi_forecasts
          WHERE id = ANY($1) AND actual_outcome IS NOT NULL",
    )
    .bind(forecast_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect())
}

async fn read_current_probs(
    forecast_ids: &[String],
    pool: &PgPool,
) -> Result<std::collections::HashMap<String, f64>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT id, predicted_probability
          FROM public.fermi_forecasts
          WHERE id = ANY($1)",
    )
    .bind(forecast_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut current = std::collections::HashMap::new();
    for r in &rows {
        let id: String = match r.try_get("id") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let p: f64 = r
            .try_get::<f32, _>("predicted_probability")
            .map(|v| v as f64)
            .unwrap_or(0.0);
        current.insert(id, p);
    }
    Ok(current)
}

fn compute_and_build_result(
    updates: Vec<(String, f64, f64)>,
    note: Option<String>,
    dry_run: bool,
    pool: &PgPool,
    reason: String,
) -> PropagateResult {
    let deltas: Vec<DeltaEntry> = updates
        .iter()
        .map(|(fid, prev, new_p)| DeltaEntry {
            forecast_id: fid.clone(),
            previous_probability: *prev,
            new_probability: *new_p,
            delta_pp: (new_p - prev) * 100.0,
        })
        .collect();

    let written = if dry_run { updates.len() } else { 0 };

    PropagateResult {
        n_updated: written,
        deltas,
        note,
    }
}

pub async fn write_deltas(
    updates: &[(String, f64, f64)],
    reason: &str,
    cascade_id: Option<&str>,
    pool: &PgPool,
) -> Result<usize, (StatusCode, String)> {
    let mut written = 0usize;
    for (fid, prev, new_p) in updates {
        let new_p_f32 = *new_p as f32;
        let prev_f32 = *prev as f32;
        let full_reason = match cascade_id {
            Some(cid) => format!("{} (cascade {})", reason, cid),
            None => reason.clone().to_string(),
        };

        sqlx::query(
            "INSERT INTO public.fermi_forecast_updates
                  (id, forecast_id, previous_probability, new_probability,
                   reason, revision_trigger, created_at)
              VALUES (gen_random_uuid()::text, $1, $2, $3, $4, 'cascade', NOW())",
        )
        .bind(fid)
        .bind(prev_f32)
        .bind(new_p_f32)
        .bind(&full_reason)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        sqlx::query(
            "UPDATE public.fermi_forecasts
              SET predicted_probability = $1, updated_at = NOW()
              WHERE id = $2",
        )
        .bind(new_p_f32)
        .bind(fid)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        written += 1;
    }
    Ok(written)
}

async fn propagate_mutex(
    forecast_ids: &[String],
    _parameters: &JsonValue,
    req: &PropagateRequest,
    pool: &PgPool,
    dry_run: bool,
) -> Result<PropagateResult, (StatusCode, String)> {
    let current = read_current_probs(forecast_ids, pool).await?;
    let resolved = read_resolved_ids(forecast_ids, pool).await?;
    let trigger_prev = *current.get(&req.trigger_forecast_id).unwrap_or(&0.0);

    // A member is a redistribution target only if it is neither the
    // trigger nor an already-resolved (factual) sibling.
    let is_survivor = |id: &String| -> bool {
        *id != req.trigger_forecast_id && !resolved.contains(id)
    };

    let mut updates: Vec<(String, f64, f64)> = Vec::new();
    let mut note: Option<String> = None;

    match (req.trigger_kind.as_str(), req.outcome) {
        ("resolved", Some(false)) => {
            let survivors: Vec<&String> = forecast_ids
                .iter()
                .filter(|id| is_survivor(id))
                .collect();
            let survivor_total: f64 = survivors
                .iter()
                .map(|id| current.get(*id).copied().unwrap_or(0.0))
                .sum();

            if survivor_total < 1e-9 {
                note = Some(
                    "Survivor probabilities sum to ~0; cannot redistribute proportionally. \
                     Sibling forecasts left untouched."
                        .into(),
                );
            } else {
                for id in &survivors {
                    let prev = current.get(*id).copied().unwrap_or(0.0);
                    let share = prev / survivor_total;
                    let absorbed = trigger_prev * share;
                    let new_p = (prev + absorbed).clamp(0.001, 0.999);
                    if (new_p - prev).abs() > 1e-5 {
                        updates.push(((*id).clone(), prev, new_p));
                    }
                }
            }
            if trigger_prev > 0.001 {
                updates.push((req.trigger_forecast_id.clone(), trigger_prev, 0.001));
            }
        }

        ("resolved", Some(true)) => {
            for id in forecast_ids.iter().filter(|id| is_survivor(id)) {
                let prev = current.get(id).copied().unwrap_or(0.0);
                if prev > 0.001 {
                    updates.push((id.clone(), prev, 0.001));
                }
            }
            if trigger_prev < 0.999 {
                updates.push((req.trigger_forecast_id.clone(), trigger_prev, 0.999));
            }
        }

        ("resolved", None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "trigger_kind='resolved' requires `outcome` (true|false)".into(),
            ));
        }

        ("updated", _) => {
            // Σ over the live (non-resolved) members only — resolved
            // siblings are factual and don't participate in the mutex.
            let total: f64 = current
                .iter()
                .filter(|(id, _)| !resolved.contains(*id))
                .map(|(_, p)| *p)
                .sum();
            let delta = total - 1.0;
            if delta.abs() < 1e-6 {
                note = Some("Members already sum to 1.0; no redistribution needed.".into());
            } else {
                let siblings: Vec<&String> = forecast_ids
                    .iter()
                    .filter(|id| is_survivor(id))
                    .collect();
                let sibling_total: f64 = siblings
                    .iter()
                    .map(|id| current.get(*id).copied().unwrap_or(0.0))
                    .sum();
                if sibling_total < 1e-9 {
                    note = Some(
                        "Sibling probabilities sum to ~0; cannot redistribute. \
                         Sibling forecasts left untouched."
                            .into(),
                    );
                } else {
                    for id in &siblings {
                        let prev = current.get(*id).copied().unwrap_or(0.0);
                        let share = prev / sibling_total;
                        let new_p = (prev - delta * share).clamp(0.001, 0.999);
                        if (new_p - prev).abs() > 1e-5 {
                            updates.push(((*id).clone(), prev, new_p));
                        }
                    }
                }
            }
        }

        (other, _) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Unknown trigger_kind '{}'. Valid: 'resolved', 'updated'",
                    other
                ),
            ));
        }
    }

    let reason = format!(
        "cascade from {} ({})",
        req.trigger_forecast_id, req.trigger_kind
    );

    let mut result = compute_and_build_result(updates, note, dry_run, pool, reason.clone());

    if !dry_run {
        let written = write_deltas(&result.deltas.iter().map(|d| (d.forecast_id.clone(), d.previous_probability, d.new_probability)).collect::<Vec<_>>(), &reason, None, pool).await?;
        result.n_updated = written;
    } else {
        result.n_updated = result.deltas.len();
    }

    Ok(result)
}

async fn propagate_at_most_n(
    forecast_ids: &[String],
    parameters: &JsonValue,
    req: &PropagateRequest,
    pool: &PgPool,
    dry_run: bool,
) -> Result<PropagateResult, (StatusCode, String)> {
    let n: f64 = parameters
        .get("n")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let current = read_current_probs(forecast_ids, pool).await?;
    let resolved = read_resolved_ids(forecast_ids, pool).await?;
    let trigger_prev = *current.get(&req.trigger_forecast_id).unwrap_or(&0.0);

    let is_survivor = |id: &String| -> bool {
        *id != req.trigger_forecast_id && !resolved.contains(id)
    };

    let mut updates: Vec<(String, f64, f64)> = Vec::new();
    let mut note: Option<String> = None;

    match (req.trigger_kind.as_str(), req.outcome) {
        ("resolved", Some(false)) => {
            let survivors: Vec<&String> = forecast_ids
                .iter()
                .filter(|id| is_survivor(id))
                .collect();
            let survivor_sum: f64 = survivors
                .iter()
                .map(|id| current.get(*id).copied().unwrap_or(0.0))
                .sum();

            let capacity = n;
            let available = capacity - survivor_sum;
            if available <= 0.0 {
                note = Some("Survivor sum already at capacity; no redistribution.".into());
            } else {
                let mass_to_redistribute = trigger_prev.min(available);
                if survivor_sum < 1e-9 {
                    note = Some(
                        "Survivor probabilities sum to ~0; cannot redistribute.".into(),
                    );
                } else {
                    for id in &survivors {
                        let prev = current.get(*id).copied().unwrap_or(0.0);
                        let share = prev / survivor_sum;
                        let absorbed = mass_to_redistribute * share;
                        let new_p = (prev + absorbed).clamp(0.001, 0.999);
                        if (new_p - prev).abs() > 1e-5 {
                            updates.push(((*id).clone(), prev, new_p));
                        }
                    }
                }
            }
            if trigger_prev > 0.001 {
                updates.push((req.trigger_forecast_id.clone(), trigger_prev, 0.001));
            }
        }

        ("resolved", Some(true)) => {
            let survivors: Vec<&String> = forecast_ids
                .iter()
                .filter(|id| is_survivor(id))
                .collect();
            let survivor_sum: f64 = survivors
                .iter()
                .map(|id| current.get(*id).copied().unwrap_or(0.0))
                .sum();

            let new_capacity = n - 1.0;
            if survivor_sum > new_capacity && survivor_sum > 1e-9 {
                let scale = new_capacity / survivor_sum;
                for id in &survivors {
                    let prev = current.get(*id).copied().unwrap_or(0.0);
                    let new_p = (prev * scale).clamp(0.001, 0.999);
                    if (new_p - prev).abs() > 1e-5 {
                        updates.push(((*id).clone(), prev, new_p));
                    }
                }
            }
            if trigger_prev < 0.999 {
                updates.push((req.trigger_forecast_id.clone(), trigger_prev, 0.999));
            }
        }

        ("resolved", None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "trigger_kind='resolved' requires `outcome`".into(),
            ));
        }

        ("updated", _) => {
            let total: f64 = current
                .iter()
                .filter(|(id, _)| !resolved.contains(*id))
                .map(|(_, p)| *p)
                .sum();
            if total <= n {
                note = Some(format!(
                    "Members sum to {:.4} (≤ {}); no redistribution needed.",
                    total, n
                ));
            } else {
                let siblings: Vec<&String> = forecast_ids
                    .iter()
                    .filter(|id| is_survivor(id))
                    .collect();
                let sibling_sum: f64 = siblings
                    .iter()
                    .map(|id| current.get(*id).copied().unwrap_or(0.0))
                    .sum();
                let trigger_p = current
                    .get(&req.trigger_forecast_id)
                    .copied()
                    .unwrap_or(0.0);
                let excess = (trigger_p + sibling_sum) - n;
                if sibling_sum < 1e-9 || excess <= 0.0 {
                    note = Some("No excess to redistribute.".into());
                } else {
                    let scale = (sibling_sum - excess) / sibling_sum;
                    for id in &siblings {
                        let prev = current.get(*id).copied().unwrap_or(0.0);
                        let new_p = (prev * scale).clamp(0.001, 0.999);
                        if (new_p - prev).abs() > 1e-5 {
                            updates.push(((*id).clone(), prev, new_p));
                        }
                    }
                }
            }
        }

        (other, _) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Unknown trigger_kind '{}'", other),
            ));
        }
    }

    let reason = format!(
        "cascade from {} ({}) [at_most_n={}]",
        req.trigger_forecast_id, req.trigger_kind, n
    );

    let mut result = compute_and_build_result(updates, note, dry_run, pool, reason.clone());

    if !dry_run {
        let written = write_deltas(&result.deltas.iter().map(|d| (d.forecast_id.clone(), d.previous_probability, d.new_probability)).collect::<Vec<_>>(), &reason, None, pool).await?;
        result.n_updated = written;
    } else {
        result.n_updated = result.deltas.len();
    }

    Ok(result)
}

async fn propagate_implies(
    forecast_ids: &[String],
    parameters: &JsonValue,
    req: &PropagateRequest,
    pool: &PgPool,
    dry_run: bool,
) -> Result<PropagateResult, (StatusCode, String)> {
    let antecedent = parameters
        .get("antecedent")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let consequent = parameters
        .get("consequent")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if antecedent.is_empty() || consequent.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "implies requires parameters.antecedent and parameters.consequent".into(),
        ));
    }

    let current = read_current_probs(forecast_ids, pool).await?;
    let p_ant = *current.get(&antecedent).unwrap_or(&0.0);
    let p_con = *current.get(&consequent).unwrap_or(&0.0);

    let mut updates: Vec<(String, f64, f64)> = Vec::new();
    let mut note: Option<String> = None;

    let is_antecedent_trigger = req.trigger_forecast_id == antecedent;
    let is_consequent_trigger = req.trigger_forecast_id == consequent;

    match (req.trigger_kind.as_str(), req.outcome) {
        ("resolved", Some(true)) => {
            if is_antecedent_trigger {
                if p_con < 0.999 {
                    updates.push((consequent.clone(), p_con, 0.999));
                }
                if p_ant < 0.999 {
                    updates.push((antecedent.clone(), p_ant, 0.999));
                }
            }
        }

        ("resolved", Some(false)) => {
            if is_consequent_trigger {
                if p_ant > 0.001 {
                    updates.push((antecedent.clone(), p_ant, 0.001));
                }
                if p_con > 0.001 {
                    updates.push((consequent.clone(), p_con, 0.001));
                }
            }
        }

        ("resolved", None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "trigger_kind='resolved' requires `outcome`".into(),
            ));
        }

        ("updated", _) => {
            if is_antecedent_trigger && p_ant > p_con + 1e-9 {
                updates.push((consequent.clone(), p_con, p_ant));
            } else if is_consequent_trigger && p_con < p_ant - 1e-9 {
                updates.push((antecedent.clone(), p_ant, p_con));
            } else {
                note = Some("Implies constraint already satisfied; no propagation needed.".into());
            }
        }

        (other, _) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Unknown trigger_kind '{}'", other),
            ));
        }
    }

    let reason = format!(
        "cascade from {} ({}) [implies: {} => {}]",
        req.trigger_forecast_id, req.trigger_kind, antecedent, consequent
    );

    let mut result = compute_and_build_result(updates, note, dry_run, pool, reason.clone());

    if !dry_run {
        let written = write_deltas(&result.deltas.iter().map(|d| (d.forecast_id.clone(), d.previous_probability, d.new_probability)).collect::<Vec<_>>(), &reason, None, pool).await?;
        result.n_updated = written;
    } else {
        result.n_updated = result.deltas.len();
    }

    Ok(result)
}
