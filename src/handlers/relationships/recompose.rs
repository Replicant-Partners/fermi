//! Idempotent recomposition of a mutex group's displayed probabilities.
//!
//! Spec 25 §3.1, extended for the "smart sims" contract: a forecast's
//! displayed `predicted_probability` is a DERIVED value —
//!
//!     predicted_probability = recompose(sim_probability, eliminated mass)
//!
//! where `sim_probability` is the forecast's own standalone Monte-Carlo
//! mean. Recompose redistributes the freed mass of resolved-NO siblings
//! across the surviving members, proportional to their standalone
//! strength. Because it always reads `sim_probability` (the raw mean) and
//! never the already-displayed value, it is idempotent: re-running a sim
//! recomputes the standalone AND re-applies the eliminations every time,
//! instead of resetting the displayed value back to the standalone.
//!
//! This is the holistic counterpart to the per-trigger `propagate_mutex`:
//! same end-state for "all eliminations applied", but computed in one shot
//! from current group state so it survives arbitrary re-sims.

use std::collections::HashMap;

use axum::http::StatusCode;
use sqlx::{PgPool, Row};

const FLOOR: f64 = 0.001;
const CEIL: f64 = 0.999;

/// Mutex groups (kind='mutex', not archived) that `forecast_id` belongs to.
pub async fn mutex_groups_for_forecast(
    forecast_id: &str,
    pool: &PgPool,
) -> Result<Vec<String>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT g.group_id
           FROM public.forecast_relationship_groups g
           JOIN public.fermi_forecasts f
             ON f.relationship_groups @> ARRAY[g.group_id]
          WHERE f.id = $1
            AND g.kind = 'mutex'
            AND g.archived_at IS NULL",
    )
    .bind(forecast_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("group_id").ok())
        .collect())
}

/// Recompose every mutex group `forecast_id` is in, writing the derived
/// `predicted_probability` for all members. Returns this forecast's
/// displayed value (None if it is in no mutex group → caller keeps the
/// raw standalone).
pub async fn recompose_forecast_groups(
    forecast_id: &str,
    pool: &PgPool,
) -> Result<Option<f64>, (StatusCode, String)> {
    let groups = mutex_groups_for_forecast(forecast_id, pool).await?;
    let mut displayed: Option<f64> = None;
    for group_id in &groups {
        let map = recompose_mutex_group(group_id, pool).await?;
        if let Some(v) = map.get(forecast_id) {
            displayed = Some(*v);
        }
    }
    Ok(displayed)
}

/// Recompose a single mutex group. Writes `predicted_probability` for all
/// members and returns the id→displayed map.
pub async fn recompose_mutex_group(
    group_id: &str,
    pool: &PgPool,
) -> Result<HashMap<String, f64>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT id, sim_probability, predicted_probability, actual_outcome
           FROM public.fermi_forecasts
          WHERE relationship_groups @> ARRAY[$1]",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // raw_i = standalone strength: sim_probability, falling back to the
    // current displayed value when a forecast has never been simmed.
    struct Member {
        id: String,
        raw: f64,
        outcome: Option<bool>,
    }
    let members: Vec<Member> = rows
        .iter()
        .filter_map(|r| {
            let id: String = r.try_get("id").ok()?;
            let raw = r
                .try_get::<Option<f32>, _>("sim_probability")
                .ok()
                .flatten()
                .or_else(|| r.try_get::<f32, _>("predicted_probability").ok())
                .unwrap_or(0.0) as f64;
            let outcome: Option<bool> = r.try_get("actual_outcome").ok().flatten();
            Some(Member { id, raw, outcome })
        })
        .collect();

    let any_winner = members.iter().any(|m| m.outcome == Some(true));
    let survivor_raw_sum: f64 = members
        .iter()
        .filter(|m| m.outcome.is_none())
        .map(|m| m.raw)
        .sum();
    let eliminated_mass: f64 = members
        .iter()
        .filter(|m| m.outcome == Some(false))
        .map(|m| m.raw)
        .sum();

    // factor scales survivors so they absorb the eliminated mass while
    // preserving relative ranking. If a member resolved YES, the mutex is
    // decided: every survivor collapses to the floor.
    let factor = if survivor_raw_sum > 1e-9 {
        (survivor_raw_sum + eliminated_mass) / survivor_raw_sum
    } else {
        1.0
    };

    let mut displayed: HashMap<String, f64> = HashMap::new();
    for m in &members {
        let value = match m.outcome {
            Some(true) => CEIL,
            Some(false) => FLOOR,
            None => {
                if any_winner {
                    FLOOR
                } else {
                    (m.raw * factor).clamp(FLOOR, CEIL)
                }
            }
        };
        displayed.insert(m.id.clone(), value);
    }

    // Batch-write displayed values via UNNEST (one round-trip).
    let ids: Vec<String> = displayed.keys().cloned().collect();
    let vals: Vec<f32> = ids.iter().map(|id| displayed[id] as f32).collect();
    sqlx::query(
        "UPDATE public.fermi_forecasts f
            SET predicted_probability = t.p, updated_at = NOW()
           FROM UNNEST($1::text[], $2::real[]) AS t(fid, p)
          WHERE f.id = t.fid
            AND f.predicted_probability IS DISTINCT FROM t.p",
    )
    .bind(&ids)
    .bind(&vals)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(displayed)
}
