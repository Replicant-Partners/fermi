//! Publish pipeline — opinionated checks before an agent goes public.

use super::agent_state::validate_transition;
use super::types::{AgentLifecycleStatus, CheckSeverity, PublishCheck, TransitionResult};
use crate::gas::GasFees;
use agent_bestiary_memory::types::Agent;
use fermi_auth::{credit_charge, get_or_create_wallet};
use sqlx::PgPool;

/// Run publish readiness checks on an agent. Returns all checks (pass and fail).
pub fn run_publish_checks(agent: &Agent) -> Vec<PublishCheck> {
    let mut checks = Vec::new();

    // Errors — block publish
    checks.push(PublishCheck {
        name: "name_set".into(),
        passed: !agent.agent_name.is_empty(),
        severity: CheckSeverity::Error,
        message: if agent.agent_name.is_empty() {
            "Agent must have a name".into()
        } else {
            format!("Name: {}", agent.agent_name)
        },
    });

    checks.push(PublishCheck {
        name: "description_present".into(),
        passed: agent
            .description
            .as_ref()
            .map_or(false, |d| !d.trim().is_empty()),
        severity: CheckSeverity::Error,
        message: if agent
            .description
            .as_ref()
            .map_or(true, |d| d.trim().is_empty())
        {
            "Description is required for publication".into()
        } else {
            "Description present".into()
        },
    });

    checks.push(PublishCheck {
        name: "system_prompt_present".into(),
        passed: agent
            .system_prompt
            .as_ref()
            .map_or(false, |p| !p.trim().is_empty()),
        severity: CheckSeverity::Error,
        message: if agent
            .system_prompt
            .as_ref()
            .map_or(true, |p| p.trim().is_empty())
        {
            "System prompt is required for publication".into()
        } else {
            "System prompt present".into()
        },
    });

    let has_tags = !agent.tags.is_empty();
    checks.push(PublishCheck {
        name: "has_tags".into(),
        passed: has_tags,
        severity: CheckSeverity::Error,
        message: if has_tags {
            format!("Tags: {}", agent.tags.join(", "))
        } else {
            "At least one tag is required".into()
        },
    });

    // Warnings — don't block but worth fixing
    let has_sample_queries = !agent.sample_queries.is_empty();
    checks.push(PublishCheck {
        name: "sample_queries".into(),
        passed: has_sample_queries,
        severity: CheckSeverity::Warning,
        message: if has_sample_queries {
            format!("{} sample queries", agent.sample_queries.len())
        } else {
            "No sample queries — users won't know how to use your agent".into()
        },
    });

    let default_temp = (agent.temperature - 0.3).abs() < f64::EPSILON;
    checks.push(PublishCheck {
        name: "custom_temperature".into(),
        passed: !default_temp,
        severity: CheckSeverity::Warning,
        message: if default_temp {
            "Temperature is default (0.3) — consider tuning for your use case".into()
        } else {
            format!("Temperature: {}", agent.temperature)
        },
    });

    let execs = agent.total_executions;
    checks.push(PublishCheck {
        name: "has_executions".into(),
        passed: execs > 0,
        severity: CheckSeverity::Warning,
        message: if execs > 0 {
            format!("{} executions recorded", execs)
        } else {
            "Zero executions — test your agent before publishing".into()
        },
    });

    checks
}

/// Check if all error-severity checks pass
pub fn can_publish(checks: &[PublishCheck]) -> bool {
    checks
        .iter()
        .filter(|c| c.severity == CheckSeverity::Error)
        .all(|c| c.passed)
}

/// Run the full publish pipeline: check, charge, transition.
///
/// `force` is v0.10.5. When true, error-severity check failures do
/// not block the publish. Only platform admins should set this;
/// enforcement lives in the handler (not here), so callers must
/// gate before threading `force = true` through. The returned
/// `checks` vector still reports every check's status — forced
/// publishes surface the skipped errors in the response body so the
/// operator has a paper trail.
///
/// This function does not write to `admin_bypass_events`. The
/// handler owns that audit surface because it knows the admin's
/// `user_id`, the reason string, and the exact resource id. Keeps
/// the workflow layer free of RBAC state.
pub async fn publish_agent(
    pool: &PgPool,
    agent: &Agent,
    user_id: &str,
    gas_fees: &GasFees,
    force: bool,
) -> Result<(TransitionResult, Vec<PublishCheck>), String> {
    let current = AgentLifecycleStatus::from_str(&agent.status)?;
    validate_transition(&current, &AgentLifecycleStatus::Published)?;

    let checks = run_publish_checks(agent);
    if !force && !can_publish(&checks) {
        return Err("Publish blocked by failing checks".into());
    }

    // Charge publish fee
    let wallet = get_or_create_wallet(pool, "user", user_id)
        .await
        .map_err(|e| format!("Wallet error: {}", e))?;

    credit_charge(
        pool,
        wallet.wallet_id,
        gas_fees.publish_fee,
        "publish_fee",
        &format!("Publish agent {}", agent.agent_name),
        Some(&agent.agent_id.to_string()),
    )
    .await
    .map_err(|e| format!("Insufficient credits: {}", e))?;

    // Set status = published, visibility = public
    sqlx::query("UPDATE agents SET status = 'published', visibility = 'public', updated_at = NOW() WHERE agent_id = $1")
        .bind(agent.agent_id)
        .execute(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    Ok((
        TransitionResult {
            agent_id: agent.agent_id,
            from: current.as_str().to_string(),
            to: "published".to_string(),
        },
        checks,
    ))
}

/// Archive an agent (Published/Draft -> Archived). Sets visibility to private.
pub async fn archive_agent(pool: &PgPool, agent: &Agent) -> Result<TransitionResult, String> {
    let current = AgentLifecycleStatus::from_str(&agent.status)?;
    validate_transition(&current, &AgentLifecycleStatus::Archived)?;

    sqlx::query("UPDATE agents SET status = 'archived', visibility = 'private', updated_at = NOW() WHERE agent_id = $1")
        .bind(agent.agent_id)
        .execute(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    Ok(TransitionResult {
        agent_id: agent.agent_id,
        from: current.as_str().to_string(),
        to: "archived".to_string(),
    })
}

/// Restore an archived agent to draft.
pub async fn restore_agent(pool: &PgPool, agent: &Agent) -> Result<TransitionResult, String> {
    let current = AgentLifecycleStatus::from_str(&agent.status)?;
    validate_transition(&current, &AgentLifecycleStatus::Draft)?;

    sqlx::query("UPDATE agents SET status = 'draft', updated_at = NOW() WHERE agent_id = $1")
        .bind(agent.agent_id)
        .execute(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    Ok(TransitionResult {
        agent_id: agent.agent_id,
        from: current.as_str().to_string(),
        to: "draft".to_string(),
    })
}
