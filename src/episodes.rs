//! Episode construction — the single constructor that turns an
//! `AgentOutput` into a persisted `Episode`.
//!
//! This lives in the library rather than the `api-server` binary so that
//! both sides of the platform build episodes through ONE constructor: the
//! HTTP handlers (which reach it via a `pub(crate) use` re-export at the
//! bin root) and the in-library delegation tools in
//! `agent_backend::tools_legacy` (which previously had to duplicate
//! episode construction because the function was unreachable from the
//! lib). Two hand-maintained copies of this logic is exactly the drift
//! that loses cost basis, provider attribution, and failure provenance on
//! whichever copy is forgotten.

use crate::agent_backend::executor::{AgentOutput, AgentStatus};
use agent_bestiary_memory::{Episode, ExecutionStatus};
use serde_json::json;

/// Compose a diagnostic `error_details` string for a non-successful run.
///
/// Returns `None` for a clean success. For anything else it leads with the
/// executor's own `failure_reason` when present (it is the most specific
/// thing anyone knows about the run) and appends the execution shape that
/// makes it actionable: how the LLM stopped, how many round-trips it took,
/// how many tool calls it made, and how many tokens it burned.
///
/// The token count matters because a failed run is still billed. A caller
/// looking at a $0.61 charge on a FAILURE row needs to see that the money
/// went on 5 iterations and 15 tool calls that never produced final text —
/// not just the word "failed".
pub fn build_error_details(output: &AgentOutput) -> Option<String> {
    let base = match (&output.status, output.metadata.failure_reason.as_deref()) {
        (AgentStatus::Success, _) => return None,
        (_, Some(reason)) => reason.to_string(),
        (AgentStatus::Failed, None) => "execution failed (executor gave no reason)".to_string(),
        (AgentStatus::Timeout, None) => "execution timed out".to_string(),
        (AgentStatus::BelowConfidenceThreshold, None) => format!(
            "below confidence threshold (confidence={:.2})",
            output.confidence
        ),
    };

    let mut facts = vec![format!(
        "stop_reason={}",
        output.metadata.stop_reason.as_deref().unwrap_or("unknown")
    )];
    facts.push(format!("iterations={}", output.loop_iterations));
    facts.push(format!("tool_calls={}", output.tool_invocations.len()));
    if let Some(t) = output.tokens_used {
        facts.push(format!("tokens={}", t));
    }
    facts.push(format!("confidence={:.2}", output.confidence));
    if let Some(ref m) = output.metadata.model_used {
        facts.push(format!("model={}", m));
    }
    if let Some(ref p) = output.metadata.provider {
        facts.push(format!("provider={}", p));
    }

    Some(format!("{} [{}]", base, facts.join(", ")))
}

pub fn agent_output_to_episode(
    agent_db_id: uuid::Uuid,
    query: &str,
    output: &AgentOutput,
) -> Episode {
    // Generate tags from execution metadata
    let mut tags = Vec::new();

    // Status tag
    match output.status {
        AgentStatus::Success => tags.push("status:success".to_string()),
        AgentStatus::Failed => tags.push("status:error".to_string()),
        AgentStatus::Timeout => tags.push("status:timeout".to_string()),
        AgentStatus::BelowConfidenceThreshold => tags.push("status:low-confidence".to_string()),
    }

    // Tool usage tags
    let mut tool_names: Vec<String> = output
        .tool_invocations
        .iter()
        .map(|t| t.tool_name.clone())
        .collect();
    tool_names.sort();
    tool_names.dedup();
    for name in &tool_names {
        tags.push(format!("tool:{}", name));
    }

    // Iteration count tag
    match output.loop_iterations {
        0 | 1 => tags.push("iterations:1".to_string()),
        2..=4 => tags.push("iterations:2+".to_string()),
        _ => tags.push("iterations:5+".to_string()),
    }

    // Cost tier tag (based on token count)
    match output.tokens_used {
        None | Some(0) => tags.push("cost:free".to_string()),
        Some(t) if t < 500 => tags.push("cost:low".to_string()),
        Some(t) if t < 5000 => tags.push("cost:medium".to_string()),
        _ => tags.push("cost:high".to_string()),
    }

    // Model tag
    if let Some(ref model) = output.metadata.model_used {
        let short = if model.contains("sonnet") {
            "claude-sonnet"
        } else if model.contains("haiku") {
            "claude-haiku"
        } else if model.contains("opus") {
            "claude-opus"
        } else if model.contains("mistral") {
            "mistral"
        } else if model.contains("qwen") {
            "qwen"
        } else {
            model.as_str()
        };
        tags.push(format!("model:{}", short));
    }

    // Confidence tag
    let conf = output.confidence;
    if conf >= 0.7 {
        tags.push("confidence:high".to_string());
    } else if conf >= 0.4 {
        tags.push("confidence:medium".to_string());
    } else if conf > 0.0 {
        tags.push("confidence:low".to_string());
    }

    // Why the LLM stopped. The executors already know this (issue #3 /
    // docs/specs/10_RESEARCH_AGENTS_EMPTY_LLM_OUTPUT.md) and it is the
    // single most diagnostic field for a failure — `max_tokens` and
    // `tool_use` mean completely different remediations. Tagging it makes
    // the distinction visible in the episode list without an expand.
    if let Some(ref sr) = output.metadata.stop_reason {
        if !sr.is_empty() {
            tags.push(format!("stop:{}", sr));
        }
    }

    // A run that succeeded but not cleanly (e.g. tool loop capped out and
    // the answer came from the flush turn) still carries a
    // `failure_reason`. That is a degradation, not an error, and it was
    // previously invisible in every surface.
    if matches!(output.status, AgentStatus::Success) && output.metadata.failure_reason.is_some() {
        tags.push("degraded:true".to_string());
    }

    // Recover the agent's quantified judgements from its own output. Prose for
    // now, which is why the recovered assertions are capped at
    // `model_inference`: `ExtractionPath::Prose` records that the number came
    // out of a sentence rather than a typed field, and that gap is the only
    // standing reason for an agent to emit structured output.
    //
    // A malformed spread is dropped rather than repaired — reordering p5 and p95
    // to make them fit would put a number into a forecast no agent asserted —
    // and the rejection is logged, because a silently discarded claim is what
    // this whole change exists to stop.
    let (recovered, rejected) = match output.raw_response.as_deref() {
        Some(text) => crate::assertions::extract_from_prose(text),
        None => (Vec::new(), Vec::new()),
    };
    if !rejected.is_empty() {
        tracing::warn!(
            agent_id = %agent_db_id,
            rejected = ?rejected,
            "assertion_rejected — an agent stated a quantity that is not a \
             coherent spread; dropped rather than repaired"
        );
        // ── Why a log line was not enough ────────────────────────────────
        //
        // A `tracing::warn!` satisfies whoever is tailing the process and
        // nobody else. It is not queryable, not retained, and not countable,
        // so "how many quantified judgements has this platform thrown away?"
        // had no answer — which is the same shape as the defect the comment
        // above says this code exists to stop, one level up.
        //
        // Measured the moment it became askable: of 507 `[MULTIPLIER]` lines
        // in `episodes`, 23 fall outside the range their own card declares and
        // were dropped here. `weather_oracle` is 6 of its 10 — the worst rate
        // on the platform, because a bucket on an 11-way ladder honestly needs
        // an adjustment near 0.03 and its card's floor is 0.1. Every one of
        // those runs recorded `execution_status = success` with an empty
        // `assertions` array, which reads identically to an agent that
        // quantified nothing.
        //
        // A tag rather than a column because `tags` is already the queryable
        // surface every episode carries and the verification scripts already
        // read it with `= ANY(e.tags)`; a column would need a migration to say
        // something a tag says today. The count goes in the tag because the
        // difference between one malformed line and eleven is the difference
        // between a typo and a format the agent has not understood.
        tags.push("assertion:rejected".to_string());
        tags.push(format!("assertion_rejected:{}", rejected.len()));
    }
    let assertions_json = serde_json::to_value(&recovered).unwrap_or_else(|_| json!([]));

    Episode {
        response_text: output.raw_response.clone(),
        // mig-205 — what the agent quantified, captured unconditionally.
        //
        // This is the fix for the loss `forecast_agent_claims` could not avoid:
        // that table needs a workspace and a driver, so `execution.rs` gated the
        // write on having one, and a standalone evaluation — which is how agents
        // are mostly exercised — threw the judgement away. Measured before this
        // landed: 22 quantified judgements, 22 discarded, 0 claims.
        //
        // Captured HERE rather than in the hook because this constructor is the
        // one place every episode passes through, so a new execution path cannot
        // silently skip it. A claim remains an assertion bound to a driver, and
        // binding stays the hook's job.
        //
        // `Some([])` when nothing was quantified, never `None`: `None` means
        // "this writer does not extract", and an agent that ran and asserted
        // nothing is a different fact from one nobody looked at.
        assertions: Some(assertions_json),
        episode_id: uuid::Uuid::new_v4(),
        agent_id: agent_db_id,
        // Defaults to a root execution. `record_delegated_episode`
        // (agent_backend/tools_legacy.rs) overwrites this for delegated runs;
        // this constructor has no way to know who called it (mig-198).
        parent_episode_id: None,
        timestamp_ref: output.timestamp,
        query: query.to_string(),
        context: json!({
            "evidence": output.evidence.iter().map(|e| json!({
                "id": e.id,
                "source": e.source,
                "summary": e.summary,
                "key_findings": e.key_findings,
                "relevance": e.relevance,
            })).collect::<Vec<_>>(),
            "sources_consulted": output.sources_consulted,
            "model_used": output.metadata.model_used,
            // SPEC_28 — funding audit trail. Which principal's key paid for
            // this run, and how it was resolved. Persisted per episode so
            // "who paid for this?" is answerable from data rather than
            // reconstructed from deploy-time env.
            "provider": output.metadata.provider,
            "funding_principal": output.metadata.funding_principal,
            "credential_source": output.metadata.credential_source,
            "reasoning": output.metadata.reasoning,
            // Failure provenance. Persisted in context (not only folded into
            // `error_details`) so the raw executor verdict survives
            // independently of the human-readable summary, and so the
            // degraded-but-successful case has somewhere to live.
            "stop_reason": output.metadata.stop_reason,
            "failure_reason": output.metadata.failure_reason,
            "loop_iterations": output.loop_iterations,
            "tool_invocations": output.tool_invocations.iter().map(|t| json!({
                "tool_name": t.tool_name,
                "input": t.input,
                "output": t.output,
                "duration_ms": t.duration_ms,
                "iteration": t.iteration,
            })).collect::<Vec<_>>(),
        }),
        execution_status: match output.status {
            AgentStatus::Success => ExecutionStatus::Success,
            AgentStatus::Failed | AgentStatus::Timeout => ExecutionStatus::Failure,
            AgentStatus::BelowConfidenceThreshold => ExecutionStatus::Partial,
        },
        // The executors compute a precise reason — "tool loop produced empty
        // content (stop_reason=tool_use, iterations=5, hit_iteration_cap)",
        // "llm hit max_tokens; response is truncated" — and this function
        // used to throw it away and substitute the constant "Execution
        // failed". That constant is why every failure in the UI reads
        // identically, why the failure notification's "check the execution
        // history for details" pointed at no details, and why the
        // consolidation pass (which clusters on `error_details`) saw one
        // pattern where there were several. Compose the real thing.
        error_details: build_error_details(output),
        execution_time_ms: output.execution_time_ms as i64,
        tokens_used: output.tokens_used.map(|t| t as i32),
        // Cost is priced by `AgentOutput::cost()` — the single pricing
        // entry point — against a `(provider, model)` rate card with
        // separate input and output rates.
        //
        // Two earlier generations of this line were wrong in different
        // directions. It first hardcoded $3/Mtok for everything. Wiring
        // the registry's per-model card fixed Anthropic but still keyed on
        // the model string alone with `_ => 3.0`, which priced a DeepSeek
        // agent at Anthropic Sonnet's rate (~6.9x over) while the missing
        // input/output split understated real Anthropic runs (~1.8x under).
        // Two dominant errors pointing opposite ways made cross-provider
        // comparison directionally wrong, not merely imprecise — and that
        // comparison is the whole question the marketplace has to answer.
        //
        // `cost_basis` (recorded alongside, below) states whether the
        // split was measured or assumed, so a consumer never has to infer
        // trustworthiness from a deploy date.
        cost_usd: output
            .cost()
            .and_then(|est| rust_decimal::Decimal::from_f64_retain(est.usd)),
        // Migration 194 — persist the pricing INPUTS, not just the result,
        // so a corrected rate card can re-derive history instead of
        // leaving known-wrong rows permanently uncorrectable.
        input_tokens: output.input_tokens.map(|t| t as i32),
        output_tokens: output.output_tokens.map(|t| t as i32),
        cost_basis: output.cost().map(|est| est.basis.as_str().to_string()),
        cost_rate_key: output
            .cost()
            .map(|est| est.rate_key)
            .filter(|k| !k.is_empty()),
        embedding: None,
        consolidated: false,
        tags,
        provenance: agent_bestiary_memory::Provenance::AutoPass,
        authority_weight: 0.5,
        // Left unset here because this constructor has no notion of *who*
        // the agent was talking to. Call sites that represent a real
        // human↔agent exchange must stamp this with
        // `agent_bestiary_memory::dyad_id(agent_id, user_id)` immediately
        // after construction, otherwise the episode is invisible to the
        // companion loop (social tracker, dyad_state, relationships tab).
        // System-spawned platform agents (observations, swarm telemetry)
        // legitimately leave it `None` — there is no human counterpart.
        dyad_id: None,
        persona_version_at_write: None,
        // Phase 2: tag execution provenance so the observatory can filter
        // by provider and per-provider calibration can work (Loop 5).
        model_used: output.metadata.model_used.clone(),
        // The executor already resolved the provider authoritatively and
        // put it on `metadata.provider`. This used to re-derive it by
        // pattern-matching the model string, which got the proxied case
        // exactly backwards: a Claude model served *via* OpenRouter was
        // labelled `anthropic`, while an OpenRouter-namespaced Claude id
        // was labelled `openrouter`. Since the rate card is keyed on
        // (provider, model), a guessed provider silently mis-prices the
        // run. Fall back to the old inference only for legacy outputs that
        // carry no provider at all.
        provider_used: output
            .metadata
            .provider
            .clone()
            .filter(|p| !p.trim().is_empty())
            .or_else(|| {
                output
                    .metadata
                    .model_used
                    .as_deref()
                    .map(provider_from_model_name)
            }),
    }
}

/// Legacy fallback: infer a provider from a model id.
///
/// Only for outputs written before executors stamped
/// `AgentMetadata.provider`. It is a heuristic and cannot resolve the
/// proxied case — `claude-*` served through OpenRouter is indistinguishable
/// from a direct Anthropic call by model name alone. New code must read
/// `metadata.provider`.
fn provider_from_model_name(m: &str) -> String {
    if m.starts_with("claude") {
        "anthropic".to_string()
    } else if m.starts_with("gpt") || m.starts_with("o1") || m.starts_with("o3") {
        "openai".to_string()
    } else if m.starts_with("mistral") || m.starts_with("open-mistral") {
        "mistral".to_string()
    } else if m.starts_with("qwen") {
        "qwen".to_string()
    } else if m.starts_with("deepseek") {
        "deepseek".to_string()
    } else if m.starts_with("glm") {
        "glm".to_string()
    } else if m.contains("openrouter") || m.contains('/') {
        "openrouter".to_string()
    } else {
        "ollama".to_string()
    }
}

#[cfg(test)]
mod failure_provenance_tests {
    use super::*;
    use crate::agent_backend::executor::{AgentMetadata, ToolInvocation};

    fn output(
        status: AgentStatus,
        failure_reason: Option<&str>,
        stop: Option<&str>,
    ) -> AgentOutput {
        AgentOutput {
            agent_name: "efra_critical_factor".into(),
            agent_type: "research".into(),
            timestamp: chrono::Utc::now(),
            status,
            evidence: vec![],
            confidence: 0.0,
            sources_consulted: vec![],
            execution_time_ms: 41_000,
            tokens_used: Some(120_000),
            // A realistic tool-loop split: long accumulated tool results in,
            // comparatively little prose out. Exercises the measured-split
            // pricing path rather than the assumed one.
            input_tokens: Some(102_000),
            output_tokens: Some(18_000),
            raw_response: Some("{\"verdict\": \"inconclusive\"}".into()),
            metadata: AgentMetadata {
                model_used: Some("claude-sonnet-4".into()),
                provider: Some("anthropic".into()),
                stop_reason: stop.map(str::to_string),
                failure_reason: failure_reason.map(str::to_string),
                ..Default::default()
            },
            tool_invocations: (1..=15)
                .map(|i| ToolInvocation {
                    tool_name: "web_search".into(),
                    input: json!({}),
                    output: String::new(),
                    duration_ms: 574,
                    iteration: i,
                })
                .collect(),
            loop_iterations: 5,
        }
    }

    #[test]
    fn clean_success_records_no_error_details() {
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        assert!(build_error_details(&out).is_none());
    }

    #[test]
    fn failure_carries_the_executor_reason_not_a_constant() {
        let out = output(
            AgentStatus::Failed,
            Some("tool loop produced empty content (stop_reason=tool_use, iterations=5, hit_iteration_cap)"),
            Some("tool_use"),
        );
        let details = build_error_details(&out).expect("a failure must record why");
        assert!(
            details.contains("hit_iteration_cap"),
            "executor reason must survive: {details}"
        );
        assert_ne!(details, "Execution failed");
        // The execution shape a caller needs to act on it.
        assert!(details.contains("stop_reason=tool_use"), "{details}");
        assert!(details.contains("iterations=5"), "{details}");
        assert!(details.contains("tool_calls=15"), "{details}");
        assert!(details.contains("tokens=120000"), "{details}");
    }

    #[test]
    fn failure_without_an_executor_reason_still_says_what_is_known() {
        let out = output(AgentStatus::Failed, None, None);
        let details = build_error_details(&out).unwrap();
        assert!(details.contains("executor gave no reason"), "{details}");
        assert!(details.contains("stop_reason=unknown"), "{details}");
    }

    #[test]
    fn timeout_and_low_confidence_are_distinguishable() {
        let t = build_error_details(&output(AgentStatus::Timeout, None, None)).unwrap();
        let c = build_error_details(&output(AgentStatus::BelowConfidenceThreshold, None, None))
            .unwrap();
        assert!(t.contains("timed out"), "{t}");
        assert!(c.contains("below confidence threshold"), "{c}");
        assert_ne!(t, c);
    }

    #[test]
    fn episode_persists_stop_and_failure_reason_in_context() {
        let out = output(
            AgentStatus::Failed,
            Some("llm hit max_tokens; response is truncated"),
            Some("max_tokens"),
        );
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "Will GOOG hit 450?", &out);

        assert_eq!(
            ep.context.get("stop_reason").and_then(|v| v.as_str()),
            Some("max_tokens")
        );
        assert_eq!(
            ep.context.get("failure_reason").and_then(|v| v.as_str()),
            Some("llm hit max_tokens; response is truncated")
        );
        assert!(ep.tags.contains(&"stop:max_tokens".to_string()));
        assert!(ep.error_details.is_some());
    }

    #[test]
    fn success_with_a_failure_reason_is_tagged_degraded_but_not_errored() {
        // The tool loop capped out and the answer came from the flush turn:
        // a real answer, but not a clean one. Previously invisible.
        let out = output(
            AgentStatus::Success,
            Some("tool loop hit iteration cap (5); answer produced from flush turn"),
            Some("end_turn"),
        );
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        assert!(ep.tags.contains(&"degraded:true".to_string()));
        assert!(
            ep.error_details.is_none(),
            "a degraded success is not a failure"
        );
        assert_eq!(
            ep.context.get("failure_reason").and_then(|v| v.as_str()),
            Some("tool loop hit iteration cap (5); answer produced from flush turn")
        );
    }

    // ── mig-199: the answer, not a reading of the answer ──────────────

    #[test]
    fn the_agents_own_output_survives_the_episode() {
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);

        assert_eq!(
            ep.response_text.as_deref(),
            Some("{\"verdict\": \"inconclusive\"}"),
            "episodes recorded every property of a run except what the agent \
             said; without this there is no corpus to induce an output type \
             from, and typing the corpus from its prompts is what produced \
             seven output_contracts naming schemas that do not exist"
        );
    }

    #[test]
    fn the_digest_is_not_a_substitute_for_the_record() {
        // `context` keeps `evidence` / `reasoning` / `sources_consulted`,
        // produced by `parse_evidence_text` — which is per-agent
        // (`tool_executor.rs` special-cases genome_profiler). So the digest
        // is one hand-written reading of the output, and it changes
        // retroactively whenever the parser does. These are different
        // things and must not collapse into each other.
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);

        assert!(
            ep.context.get("evidence").is_some(),
            "the digest is still written"
        );
        assert_ne!(
            ep.context.get("evidence").map(|v| v.to_string()),
            ep.response_text.clone(),
            "a digest that equals the record means one of them is not doing \
             its job"
        );
    }

    #[test]
    fn an_executor_with_no_single_document_records_absence_not_emptiness() {
        // The mock executor and the synthetic-episode paths genuinely have
        // no agent response. `None` must stay distinguishable from a run
        // that returned an empty string — the same reason `cost_basis` is
        // nullable rather than backfilled to a guess.
        let mut out = output(AgentStatus::Success, None, Some("end_turn"));
        out.raw_response = None;
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        assert!(ep.response_text.is_none());

        out.raw_response = Some(String::new());
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        assert_eq!(ep.response_text.as_deref(), Some(""));
    }

    /// A dropped judgement is countable, not just logged.
    ///
    /// The fixtures are the exact lines `weather_oracle` emitted and the
    /// platform threw away, copied out of `episodes.response_text` rather than
    /// invented — because a test written against the malformed line I imagined
    /// would not have caught either of the shapes it actually produces.
    ///
    /// They fail for two independent reasons and both must be caught:
    /// the first is below the declared multiplier floor of 0.1, and the second
    /// is *unordered* — a p50 of 0.035 sitting above its own p95 of 0.025, which
    /// is not a wide estimate but a broken one.
    #[test]
    fn a_rejected_multiplier_is_countable_on_the_episode() {
        let mut out = output(AgentStatus::Success, None, Some("end_turn"));
        out.raw_response = Some(
            "[MULTIPLIER] Suggested p50: 0.01 (p5: 0.005, p95: 0.02) — EGLC bucket\n\
             [MULTIPLIER] Suggested p50: 0.035 (p5: 0.003, p95: 0.025) — base-rate skew"
                .into(),
        );
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);

        // Nothing was recorded as an assertion — which is the correct call, and
        // exactly why the drop has to leave a trace. Without one this episode is
        // indistinguishable from an agent that quantified nothing at all.
        assert_eq!(
            ep.assertions
                .as_ref()
                .and_then(|a| a.as_array())
                .map(Vec::len),
            Some(0),
            "a malformed spread must be dropped rather than repaired"
        );
        assert!(
            ep.tags.iter().any(|t| t == "assertion:rejected"),
            "a silently dropped claim is the defect; tags were {:?}",
            ep.tags
        );
        // The count, not just the fact. One malformed line is a typo; two is a
        // format the agent has not understood, and the remedies differ.
        assert!(
            ep.tags.iter().any(|t| t == "assertion_rejected:2"),
            "both rejections should be counted; tags were {:?}",
            ep.tags
        );
    }

    /// The tag is absent when nothing was rejected.
    ///
    /// Without this the assertion above passes for an implementation that tags
    /// unconditionally, which would make the count useless the moment anyone
    /// queried it — the same false-green shape as a cross-check that compares
    /// nothing and reports zero mismatches.
    #[test]
    fn a_well_formed_multiplier_leaves_no_rejection_tag() {
        let mut out = output(AgentStatus::Success, None, Some("end_turn"));
        out.raw_response =
            Some("[MULTIPLIER] Suggested p50: 1.15 (p5: 1.05, p95: 1.28) — fine".into());
        let ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);

        assert_eq!(
            ep.assertions
                .as_ref()
                .and_then(|a| a.as_array())
                .map(Vec::len),
            Some(1),
            "an in-range ordered spread should be recovered"
        );
        assert!(
            !ep.tags.iter().any(
                |t| t.starts_with("assertion:rejected") || t.starts_with("assertion_rejected:")
            ),
            "tagged a rejection that did not happen; tags were {:?}",
            ep.tags
        );
    }
}
