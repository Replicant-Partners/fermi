//! Platform economics — cost vs. revenue by funding principal.
//!
//! # The question this answers
//!
//! "Platform-service agents are funded by `abw-system`'s provider keys.
//! What do they cost me in real dollars, and what do they earn in
//! credits?" Until now that was unanswerable without a psql session,
//! which is why the platform's own margin was invisible.
//!
//! # Where the numbers come from
//!
//! | Figure | Source | Nature |
//! |---|---|---|
//! | funding principal | `episodes.context->>'funding_principal'` | **measured**, stamped at execution (SPEC_28) |
//! | tokens | `episodes.tokens_used` | measured |
//! | USD cost | `episodes.cost_usd` | **modelled** — see below |
//! | credit revenue | `credit_ledger` `execution_fee` rows | measured |
//! | royalties out | `credit_ledger` `agent_royalty_in` rows | measured |
//! | USD margin | derived, at an assumed credit rate | **modelled** |
//!
//! Attribution uses the funding principal recorded *at execution time*,
//! not one re-derived from the agent's current tier. That matters: when
//! the P5 ownership migration runs, history stays attributed to whoever
//! actually paid, instead of silently retconning itself.
//!
//! # Two honest caveats, surfaced in every response
//!
//! 1. **Cost is an estimate.** It is `tokens × a per-model rate`
//!    (`agent_backend::registry::calculate_cost`) with no input/output
//!    split, where real provider pricing differs several-fold between
//!    the two. Rows written before that rate card was wired to the
//!    persistence path are flat $3/Mtok and are therefore wrong for
//!    Opus (5x low), Haiku (12x high) and Ollama (should be zero).
//!    `cost_basis.mixed_history_before` marks the boundary.
//! 2. **Credits are not dollars.** Credits sell at 2.0¢ (250-credit
//!    tier) down to 1.0¢ (5000-credit tier), so any credits→USD figure
//!    depends on which tier a user bought. The rate is a parameter,
//!    echoed in the response, overridable with `?credit_usd=`.
//!
//! Reporting a single authoritative "margin" number would be more
//! satisfying and less true. The response therefore keeps measured
//! quantities (tokens, credits) separate from modelled ones (USD), so a
//! reader can apply their own assumptions.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use super::admin::require_admin;
use crate::AppState;

/// Blended default: the midpoint of the 2.0¢ and 1.0¢ ends of
/// `CREDIT_TIERS`. Deliberately a round, obviously-approximate number
/// rather than a spuriously precise weighted average — it is an
/// assumption, and should look like one.
const DEFAULT_CREDIT_USD: f64 = 0.015;

/// Date the per-model rate card was wired into the episode write path.
/// Before this, `cost_usd` is a flat $3/Mtok estimate.
const RATE_CARD_WIRED_ON: &str = "2026-08-12";

// The three queries live in `.sql` files rather than inline strings so
// that `scripts/smoke_economics.sh` can execute the *same bytes* against
// a throwaway database. An inline copy in the script would drift from
// this handler the first time either changed, and a smoke test that
// silently stops testing the real query is worse than none.
const SQL_BY_PRINCIPAL: &str = include_str!("sql/economics_by_principal.sql");
const SQL_BY_AGENT: &str = include_str!("sql/economics_by_agent.sql");
const SQL_ROYALTIES: &str = include_str!("sql/economics_royalties.sql");

#[derive(Deserialize)]
pub struct EconomicsQuery {
    /// Window in days. Default 30, max 365.
    #[serde(default)]
    pub days: Option<i64>,
    /// Assumed USD value of one credit. Defaults to `DEFAULT_CREDIT_USD`.
    #[serde(default)]
    pub credit_usd: Option<f64>,
    /// Restrict to one funding principal (e.g. `abw-system`).
    #[serde(default)]
    pub principal: Option<String>,
}

/// `GET /api/admin/economics/platform`
///
/// Cost/margin by funding principal, plus a per-agent breakdown.
pub async fn platform_economics_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<EconomicsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;

    let days = q.days.unwrap_or(30).clamp(1, 365);
    let credit_usd = q.credit_usd.unwrap_or(DEFAULT_CREDIT_USD);
    if !(credit_usd.is_finite() && credit_usd >= 0.0) {
        return Err((
            StatusCode::BAD_REQUEST,
            "credit_usd must be a non-negative number".into(),
        ));
    }

    // ─── Cost side: episodes, attributed by recorded funding principal ──
    let by_principal = sqlx::query(SQL_BY_PRINCIPAL)
        .bind(days.to_string())
        .bind(&q.principal)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ─── Per-agent breakdown, joined to revenue ─────────────────────────
    let by_agent = sqlx::query(SQL_BY_AGENT)
        .bind(days.to_string())
        .bind(&q.principal)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ─── Royalties paid out, by recipient ───────────────────────────────
    let royalties = sqlx::query(SQL_ROYALTIES)
        .bind(days.to_string())
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ─── Shape the response ─────────────────────────────────────────────
    let principals: Vec<Value> = by_principal
        .iter()
        .map(|r| {
            let cost = f64_of(r, "cost_usd");
            json!({
                "funding_principal": r.try_get::<String, _>("funding_principal").unwrap_or_default(),
                "executions": r.try_get::<i64, _>("executions").unwrap_or(0),
                "tokens": r.try_get::<i64, _>("tokens").unwrap_or(0),
                "cost_usd": round2(cost),
                "episodes_missing_cost": r.try_get::<i64, _>("missing_cost").unwrap_or(0),
            })
        })
        .collect();

    let mut total_cost = 0.0_f64;
    let mut total_fee_credits = 0.0_f64;

    let agents: Vec<Value> = by_agent
        .iter()
        .map(|r| {
            let cost = f64_of(r, "cost_usd");
            let fee_credits = f64_of(r, "fee_credits");
            total_cost += cost;
            total_fee_credits += fee_credits;
            let revenue_usd = fee_credits * credit_usd;
            json!({
                "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
                "tier": r.try_get::<String, _>("tier").unwrap_or_default(),
                "owner_id": r.try_get::<Option<String>, _>("owner_id").ok().flatten(),
                "funding_principal": r.try_get::<String, _>("funding_principal").unwrap_or_default(),
                "provider": r.try_get::<Option<String>, _>("provider").ok().flatten(),
                "model": r.try_get::<Option<String>, _>("model").ok().flatten(),
                "executions": r.try_get::<i64, _>("executions").unwrap_or(0),
                "tokens": r.try_get::<i64, _>("tokens").unwrap_or(0),
                "cost_usd": round2(cost),
                "revenue_credits": fee_credits as i64,
                "revenue_usd_modelled": round2(revenue_usd),
                "margin_usd_modelled": round2(revenue_usd - cost),
            })
        })
        .collect();

    let total_revenue_usd = total_fee_credits * credit_usd;
    let margin = total_revenue_usd - total_cost;

    Ok(Json(json!({
        "window_days": days,
        "filter_principal": q.principal,

        "totals": {
            "cost_usd": round2(total_cost),
            "revenue_credits": total_fee_credits as i64,
            "revenue_usd_modelled": round2(total_revenue_usd),
            "margin_usd_modelled": round2(margin),
            // Null rather than 0 when there is no revenue: a 0% margin
            // and "no revenue to have a margin on" are different facts.
            "margin_pct_modelled": if total_revenue_usd > 0.0 {
                json!(round2(margin / total_revenue_usd * 100.0))
            } else {
                Value::Null
            },
        },

        "by_funding_principal": principals,
        "by_agent": agents,
        "royalties_out": royalties.iter().map(|r| json!({
            "recipient": r.try_get::<String, _>("owner_id").unwrap_or_default(),
            "credits": f64_of(r, "royalty_credits") as i64,
        })).collect::<Vec<_>>(),

        // Every modelled figure above is only as good as these. Returned
        // inline so a consumer cannot read the margin without also
        // reading what it assumes.
        "cost_basis": {
            "cost_is_estimated": true,
            "method": "tokens × per-model rate card (agent_backend::registry::calculate_cost)",
            "no_input_output_split": true,
            "mixed_history_before": RATE_CARD_WIRED_ON,
            "mixed_history_note":
                "Episodes written before this date use a flat $3/Mtok estimate: \
                 Opus understated ~5x, Haiku overstated ~12x, local Ollama runs \
                 charged as if paid.",
            "credit_usd_assumed": credit_usd,
            "credit_usd_source": if q.credit_usd.is_some() { "caller" } else { "default" },
            "credit_usd_range": {"low": 0.01, "high": 0.02,
                "note": "5000-credit tier vs 250-credit tier in CREDIT_TIERS"},
            "attribution":
                "funding_principal as recorded on the episode at execution time \
                 (SPEC_28), not re-derived from the agent's current tier.",
        },
    })))
}

/// Read a numeric column that may arrive as NUMERIC, BIGINT or INT.
///
/// `SUM(cost_usd)` over a `DECIMAL(10,6)` yields NUMERIC, while
/// `SUM(tokens_used)` yields BIGINT — and a `COALESCE(..., 0)` can land
/// on either depending on the branch taken. Probing in order keeps the
/// handler from 500-ing on a type it could perfectly well read.
fn f64_of(row: &sqlx::postgres::PgRow, col: &str) -> f64 {
    if let Ok(v) = row.try_get::<rust_decimal::Decimal, _>(col) {
        return rust_decimal::prelude::ToPrimitive::to_f64(&v).unwrap_or(0.0);
    }
    if let Ok(v) = row.try_get::<i64, _>(col) {
        return v as f64;
    }
    if let Ok(v) = row.try_get::<i32, _>(col) {
        return v as f64;
    }
    row.try_get::<f64, _>(col).unwrap_or(0.0)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_credit_rate_sits_inside_the_published_tier_range() {
        // CREDIT_TIERS spans 2.0¢ (250 for $5) to 1.0¢ (5000 for $50).
        // A default outside that band would be indefensible.
        assert!(DEFAULT_CREDIT_USD >= 0.01 && DEFAULT_CREDIT_USD <= 0.02);
    }

    #[test]
    fn round2_is_stable_for_money() {
        assert_eq!(round2(1.005_9), 1.01);
        assert_eq!(round2(0.0), 0.0);
        assert_eq!(round2(-2.345), -2.35);
    }
}
