//! Publish pipeline — opinionated checks before an agent goes public.

use super::agent_contract::{contract_checks, ContractView};
use super::agent_state::validate_transition;
use super::types::{AgentLifecycleStatus, CheckSeverity, PublishCheck, TransitionResult};
use crate::gas::GasFees;
use agent_bestiary_memory::types::Agent;
use fermi_auth::{credit_charge, get_or_create_wallet};
use sqlx::PgPool;

/// Run publish readiness checks on an agent. Returns all checks (pass and fail).
///
/// The Error-severity set is [`super::agent_contract`] — the single
/// definition of a well-formed ABW agent, shared with the curated-card
/// conformance tests so the API path and the on-disk path cannot drift.
/// It previously lived here as four inline checks (name, description,
/// system_prompt, tags), which is how agents with no `accepts`,
/// `produces`, `sample_queries` or `valence` reached the public
/// catalogue: they satisfied every error the gate knew how to raise.
///
/// Tightening this is **not retroactive**. Checks run on the transition
/// *into* `published`, so already-published agents are unaffected until
/// someone archives and re-publishes them. See
/// `publish_checks_handler` for read-only inspection of any agent.
///
/// Everything below the contract block is advisory — `Warning` severity,
/// reported but never blocking.
pub fn run_publish_checks(agent: &Agent) -> Vec<PublishCheck> {
    let mut checks = contract_checks(&ContractView::from(agent));

    // Warnings — don't block but worth fixing
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Minimal conforming agent. Tests break one field at a time.
    fn agent() -> Agent {
        let mut a: Agent = serde_json::from_value(json!({
            "agent_id": "00000000-0000-0000-0000-000000000000",
            "agent_name": "probe",
            "agent_type": "research",
            "version": "1.0.0",
            "tier": "community",
            "executor_type": "llm",
            "model": "claude",
            "temperature": 0.3,
            "author": "tester",
            "visibility": "private",
            "tags": ["research"],
            "total_executions": 0,
            "successful_executions": 0,
            "failed_executions": 0,
            "avg_execution_time_ms": 0,
            "dreaming_budget_credits": 0,
            "dreaming_credits_used": 0,
            "education_budget_credits": 0,
            "education_credits_used": 0,
            "auto_collect_pct": 0,
            "llm_provider": "anthropic",
            "embedding_provider": "openai",
            "embedding_model": "text-embedding-3-large",
            "embedding_dimension": 1024,
            "sample_queries": ["What is the base rate for X?"],
            "status": "draft",
            "fork_count": 0,
            "accepts": ["forecast-question"],
            "produces": ["evidence"],
            "model_ladder": [],
            "min_tier": "free",
            "capability_gates": {},
            "persona_version": 1,
            "model_params": {}
        }))
        .expect("Agent fixture should deserialize");
        a.description = Some("Does a thing, carefully.".into());
        a.system_prompt = Some("You are a probe.".into());
        a.valence = Some(json!({ "primary_affect": "analytical" }));
        a
    }

    #[test]
    fn conforming_agent_can_publish() {
        let checks = run_publish_checks(&agent());
        assert!(
            can_publish(&checks),
            "blocked by: {:?}",
            checks
                .iter()
                .filter(|c| !c.passed && c.severity == CheckSeverity::Error)
                .map(|c| &c.name)
                .collect::<Vec<_>>()
        );
    }

    /// The advisory checks are reported but must never block. A brand-new
    /// agent has zero executions and a default temperature by definition,
    /// so treating either as an error would make first publish impossible.
    #[test]
    fn warnings_do_not_block_publish() {
        let a = agent();
        let checks = run_publish_checks(&a);

        let warnings: Vec<_> = checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Warning)
            .collect();
        assert!(
            warnings
                .iter()
                .any(|c| c.name == "has_executions" && !c.passed),
            "expected an unmet has_executions warning"
        );
        assert!(
            warnings
                .iter()
                .any(|c| c.name == "custom_temperature" && !c.passed),
            "expected an unmet custom_temperature warning"
        );
        assert!(can_publish(&checks));
    }

    /// The regression this whole change exists for: an agent with a
    /// description, a prompt and one tag — but no sample queries, no I/O
    /// contract and no valence — passed the old four-item gate and
    /// reached the public catalogue. It must not.
    #[test]
    fn efra_shaped_agent_cannot_publish() {
        let mut a = agent();
        a.sample_queries = vec![];
        a.accepts = vec![];
        a.produces = vec![];
        a.valence = None;

        let checks = run_publish_checks(&a);
        assert!(!can_publish(&checks));

        let blocking: Vec<_> = checks
            .iter()
            .filter(|c| !c.passed && c.severity == CheckSeverity::Error)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(
            blocking,
            vec![
                "has_sample_queries",
                "declares_accepts",
                "declares_produces",
                "has_valence"
            ]
        );
    }

    /// A deterministic MCP agent publishes without a system prompt.
    /// `coherence_evaluator` is the live case.
    #[test]
    fn deterministic_agent_publishes_without_prompt() {
        let mut a = agent();
        a.executor_type = "mcp".into();
        a.system_prompt = Some(String::new());
        assert!(can_publish(&run_publish_checks(&a)));
    }
}
