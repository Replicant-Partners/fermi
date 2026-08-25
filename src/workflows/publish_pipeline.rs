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
///
/// `measured_executions` is the agent's lifetime run count taken from
/// `agent_execution_rollup` (see [`crate::agent_economics`]), and `None`
/// means *not measured* — never "zero". The distinction is the whole of
/// v0.16.1: this function used to read `agent.total_executions`, a column
/// nothing writes, so `has_executions` reported "Zero executions — test
/// your agent before publishing" against agents with hundreds of real
/// episodes. Passing the number in keeps the function pure and sync, which
/// `observatory::fleet_agents_handler` relies on to run it fleet-wide.
pub fn run_publish_checks(agent: &Agent, measured_executions: Option<i64>) -> Vec<PublishCheck> {
    let view = ContractView::from(agent);
    let mut checks = contract_checks(&view);

    // Typed tier: a schema, ports that reference it, and a field-to-tool
    // map for every output field. Blocking, and NOT retroactive — see
    // `agent_contract::TYPED_TIER_EXEMPT` for why the existing corpus is
    // grandfathered and why that list may only shrink.
    //
    // Error severity deliberately: an agent whose output fields nobody has
    // classified is the shape that produced 56 episodes of confidently
    // fabricated genome data. Warning it would have changed nothing.
    for finding in super::agent_contract::typed_tier_violations(&view) {
        checks.push(PublishCheck {
            name: finding.check.into(),
            passed: false,
            severity: CheckSeverity::Error,
            message: finding.message,
        });
    }

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

    // Measured from `episodes`, never from `agents.total_executions` —
    // that column is write-orphaned and permanently zero. `None` is
    // reported as unmeasured rather than as zero, because claiming an
    // agent has never run when we simply failed to look is the exact
    // defect this check shipped for months.
    checks.push(match measured_executions {
        Some(n) if n > 0 => PublishCheck {
            name: "has_executions".into(),
            passed: true,
            severity: CheckSeverity::Warning,
            message: format!("{} executions recorded", n),
        },
        Some(_) => PublishCheck {
            name: "has_executions".into(),
            passed: false,
            severity: CheckSeverity::Warning,
            message: "Zero executions — test your agent before publishing".into(),
        },
        None => PublishCheck {
            name: "has_executions".into(),
            passed: false,
            severity: CheckSeverity::Warning,
            message: "Execution count not measured — could not read the execution \
                      rollup for this agent"
                .into(),
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

    let measured = crate::agent_economics::measured_exec_stats_one(pool, agent.agent_id).await;
    let checks = run_publish_checks(agent, measured.map(|m| m.executions));
    let blocked = !force && !can_publish(&checks);

    // Counted. Note the asymmetry this closes: the admin BYPASS of these checks
    // has always been audited to `admin_bypass_events`, and the refusal itself
    // left no trace at all — so the platform could report how often the gate
    // was overridden and not how often it fired.
    //
    // A forced publish is recorded as `undetermined`: the gate did not approve
    // and did not refuse, it was skipped.
    crate::gate_trust::decided(
        crate::gate_trust::Gate::Admission,
        if force {
            crate::gate_trust::Decision::Undetermined
        } else if blocked {
            crate::gate_trust::Decision::Refused
        } else {
            crate::gate_trust::Decision::Approved
        },
        blocked
            .then(|| {
                let failing: Vec<&str> = checks
                    .iter()
                    .filter(|c| c.severity == CheckSeverity::Error && !c.passed)
                    .map(|c| c.name.as_str())
                    .collect();
                format!("failing: {}", failing.join(", "))
            })
            .as_deref(),
    );

    if blocked {
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
        // Typed tier: this fixture is a NEW agent, so it gets no
        // grandfathering and must declare a schema, a namespaced type its
        // `produces` references, and a disposition for every output field.
        // That the fixture had to grow to keep passing is the gate working.
        a.produces = vec!["probe/evidence_report".into()];
        a.output_contract = Some(json!({
            "produces_schema": "probe/evidence_report",
            "schema": { "type": "object", "properties": { "finding": {}, "notes": {} } },
            "grounding": {
                "finding": {
                    "status": "inferred",
                    "from": "the evidence gathered during the run",
                    "why": "A judgement the agent is commissioned to make; no tool returns it ready-made."
                },
                "notes": {
                    "status": "narrative",
                    "why": "Prose accompanying the finding, constrained to what the evidence supports."
                }
            }
        }));
        a
    }

    /// An agent identical to `agent()` but declaring nothing about its
    /// output. Before the typed tier this was publishable.
    fn untyped_agent() -> Agent {
        let mut a = agent();
        a.output_contract = None;
        a
    }

    #[test]
    fn a_new_agent_without_a_typed_contract_cannot_publish() {
        // The point of the tier. `untyped_agent` satisfies every presence
        // check — name, description, prompt, tags, samples, ports, valence —
        // which is exactly the state genome_profiler was in while it served
        // 56 episodes of fabricated genome data.
        let checks = run_publish_checks(&untyped_agent(), Some(0));
        assert!(!can_publish(&checks));
        assert!(
            checks
                .iter()
                .any(|c| c.name == "output_contract_present" && !c.passed),
            "the block must name the missing contract, not just refuse"
        );
    }

    #[test]
    fn conforming_agent_can_publish() {
        let checks = run_publish_checks(&agent(), Some(0));
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
        let checks = run_publish_checks(&a, Some(0));

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

        let checks = run_publish_checks(&a, Some(0));
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
                "has_valence",
                // Typed tier: empty `produces` also fails resolution, since
                // there is no declared type for a port to reference.
                "produces_resolves",
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
        assert!(can_publish(&run_publish_checks(&a, Some(0))));
    }

    /// The `has_executions` regression, pinned in all three states.
    ///
    /// This check read `agent.total_executions` — a write-orphaned column —
    /// so it reported "Zero executions" on the Observatory conformance panel
    /// for `prey_locator`, which had 93 measured episodes. The number now
    /// arrives from `agent_execution_rollup` via the caller.
    #[test]
    fn has_executions_reports_the_measured_count() {
        let a = agent();

        let measured = run_publish_checks(&a, Some(93));
        let c = measured
            .iter()
            .find(|c| c.name == "has_executions")
            .expect("has_executions must always be reported");
        assert!(c.passed, "93 measured executions must satisfy the check");
        assert!(
            c.message.contains("93"),
            "message should name the count, got: {}",
            c.message
        );

        // A genuine zero still warns — that is the check doing its job.
        let zero = run_publish_checks(&a, Some(0));
        let c = zero.iter().find(|c| c.name == "has_executions").unwrap();
        assert!(!c.passed);
        assert!(c.message.contains("Zero executions"));

        // Unmeasured must NOT be presented as zero. Reporting "never run"
        // when the rollup lookup simply failed is the defect this whole
        // check shipped with for months.
        let unknown = run_publish_checks(&a, None);
        let c = unknown.iter().find(|c| c.name == "has_executions").unwrap();
        assert!(!c.passed);
        assert!(
            !c.message.contains("Zero"),
            "an unmeasured count must not claim zero, got: {}",
            c.message
        );
    }
}
