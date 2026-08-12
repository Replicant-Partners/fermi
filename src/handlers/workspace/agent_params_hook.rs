//! Post-agent hook: extract multiplier recommendations from agent evidence
//! and write them to the workspace's params output.
//!
//! Called from the execution handler after a successful agent run. Scans the
//! agent's output evidence for `[MULTIPLIER] Suggested p50: X (p5: Y, p95: Z)`
//! patterns and maps them to the correct driver param keys via the agent's
//! `driver_refs` declarations.
//!
//! If the agent ran in a workspace context AND produced a multiplier, this
//! hook writes `{ <driver>_p5, <driver>_p50, <driver>_p95 }` to the
//! workspace's `params` output and triggers a refit.

use regex::Regex;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::LazyLock;
use uuid::Uuid;

// Matches: Suggested p50: 1.15 (p5: 1.05, p95: 1.28)
static MULTIPLIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Suggested\s+p50:\s+([\d.]+)\s*\(p5:\s+([\d.]+),\s+p95:\s+([\d.]+)\)")
        .expect("invalid MULTIPLIER_RE")
});

/// Agent → driver param prefix mapping.
/// Each FPL agent declares `driver_refs` in the template. The prefixes
/// below are the param naming convention used in the WC team_prior template.
fn driver_prefix_for_agent(agent_name: &str) -> &[&str] {
    match agent_name {
        n if n.contains("macro_data") => &["socio"],
        n if n.contains("institution") => &["institutional"],
        n if n.contains("analyst") => &["dynamic", "squad", "tactical"],
        n if n.contains("fixture") => &["fixture"],
        _ => &[],
    }
}

/// Try to extract a (p5, p50, p95) multiplier from an evidence summary.
pub fn extract_multiplier(summary: &str) -> Option<(f64, f64, f64)> {
    let caps = MULTIPLIER_RE.captures(summary)?;
    let p50 = caps.get(1)?.as_str().parse::<f64>().ok()?;
    let p5 = caps.get(2)?.as_str().parse::<f64>().ok()?;
    let p95 = caps.get(3)?.as_str().parse::<f64>().ok()?;
    Some((p5, p50, p95))
}

/// Write an agent's multiplier evidence into the workspace's params output.
///
/// Called after a successful agent execution. Returns true if params were
/// actually updated.
pub async fn apply_agent_multipliers(
    pool: &PgPool,
    registry: &posterior::ExtractorRegistry,
    workspace_id: Uuid,
    agent_name: &str,
    evidence: &[fermi::ast::EvidenceStmt],
) -> Result<bool, String> {
    let driver_prefixes = driver_prefix_for_agent(agent_name);
    if driver_prefixes.is_empty() {
        return Ok(false);
    }

    // Scan evidence for the first MULTIPLIER match. `EvidenceStmt.summary`
    // is `Option<String>` — skip rows without a summary (they can't
    // carry a multiplier match anyway).
    let mut multiplier: Option<(f64, f64, f64)> = None;
    for ev in evidence {
        let Some(summary) = ev.summary.as_deref() else {
            continue;
        };
        if let Some(m) = extract_multiplier(summary) {
            // The macro_data_agent and fixture_context_agent each cover ONE
            // driver, so the first match is the right one.
            // The football_analyst covers three drivers — same multiplier
            // applies to all three (dynamic, squad, tactical).
            multiplier = Some(m);
            break;
        }
    }

    let (p5, p50, p95) = match multiplier {
        Some(m) => m,
        None => return Ok(false),
    };

    // ── Retain the claim itself (mig-187) ─────────────────────────────────
    //
    // The params UPSERT below is CURRENT STATE: the next agent's write, or the
    // next run, overwrites it. That made every resolved forecast permanently
    // unattributable at the agent level, because the per-agent inputs that
    // produced it no longer existed.
    //
    // This ledger is what makes per-agent credit possible at all: knowing what
    // each agent individually claimed lets the attribution engine synthesise
    // the forecast for any SUBSET of agents (applying that subset's claims,
    // neutralising the rest) and so compute exact Shapley credit from a single
    // real forecast — no need for real-world composition permutations. See
    // src/attribution/ and migrations/187_forecast_agent_claims.sql.
    //
    // Append-only and best-effort: a failure here must never fail the agent
    // run, but it is logged at warn because a silent gap here is unrecoverable
    // later — claims cannot be reconstructed after the fact.
    let claim_agent_id: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1")
            .bind(agent_name)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let raw_evidence: Option<&str> = evidence
        .iter()
        .filter_map(|e| e.summary.as_deref())
        .find(|s| extract_multiplier(s).is_some());

    for prefix in driver_prefixes {
        let res = sqlx::query(
            "INSERT INTO forecast_agent_claims
                 (workspace_id, agent_id, agent_name, driver,
                  p5, p50, p95, neutral_value, source, raw_evidence)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 1.0, 'multiplier_hook', $8)",
        )
        .bind(workspace_id)
        .bind(claim_agent_id)
        .bind(agent_name)
        .bind(prefix)
        .bind(p5 as f32)
        .bind(p50 as f32)
        .bind(p95 as f32)
        .bind(raw_evidence)
        .execute(pool)
        .await;

        if let Err(e) = res {
            tracing::warn!(
                workspace = %workspace_id, agent = %agent_name, driver = %prefix, error = %e,
                "[claims] failed to record agent claim — this forecast will not be \
                 attributable per-agent and cannot be backfilled"
            );
        }
    }

    // Build the update: { <driver>_p5, <driver>_p50, <driver>_p95 } for each driver.
    let mut update = JsonValue::Object(serde_json::Map::new());
    for prefix in driver_prefixes {
        update[format!("{}_p5", prefix)] = json!(p5);
        update[format!("{}_p50", prefix)] = json!(p50);
        update[format!("{}_p95", prefix)] = json!(p95);
    }

    // Read current params.
    let current = sqlx::query_as::<_, (JsonValue,)>(
        "SELECT value FROM workspace_outputs WHERE workspace_id = $1 AND key = 'params'",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut merged = match current {
        Some((val,)) => val.as_object().cloned().unwrap_or_default(),
        None => serde_json::Map::new(),
    };

    // Merge: agent's values take precedence.
    if let Some(obj) = update.as_object() {
        for (k, v) in obj {
            merged.insert(k.clone(), v.clone());
        }
    }

    let merged_val = JsonValue::Object(merged);

    // UPSERT the params output.
    sqlx::query(
        r#"INSERT INTO workspace_outputs (workspace_id, key, value, updated_at)
           VALUES ($1, 'params', $2, NOW())
           ON CONFLICT (workspace_id, key)
           DO UPDATE SET value = $2, updated_at = NOW()"#,
    )
    .bind(workspace_id)
    .bind(&merged_val)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Trigger a refit so the FPL re-evaluates with the new driver params.
    // Fire-and-forget — the caller doesn't need to wait for the refit to complete.
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        let trigger = crate::handlers::workspace::refit::TriggerReason::Manual {
            user_id: "agent_hook".into(),
        };
        if let Err(e) = crate::handlers::workspace::refit::refit_workspace(
            &pool_clone,
            &Default::default(),
            workspace_id,
            trigger,
        )
        .await
        {
            tracing::warn!(workspace = %workspace_id, error = %e, "post-agent refit failed");
        }
    });

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_multiplier_simple() {
        let summary = "Some text [MULTIPLIER] Suggested p50: 1.25 (p5: 1.10, p95: 1.45) more text";
        let result = extract_multiplier(summary);
        assert_eq!(result, Some((1.10, 1.25, 1.45)));
    }

    #[test]
    fn test_extract_multiplier_no_match() {
        let summary = "No multiplier here";
        assert_eq!(extract_multiplier(summary), None);
    }

    #[test]
    fn test_extract_multiplier_different_spacing() {
        let summary = "[MULTIPLIER] Suggested p50:1.15(p5:1.05,p95:1.28)";
        let result = extract_multiplier(summary);
        assert_eq!(result, None); // space required after colon
    }
}
