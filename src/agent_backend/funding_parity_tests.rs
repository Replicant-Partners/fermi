//! Acceptance tests for SPEC_28 — funding must not depend on output shape.
//!
//! These encode the invariant the platform owner stated as a release
//! condition: *"all agents need to be able to use the secrets paths the
//! same way"*. Before SPEC_28, an agent's funding depended on whether its
//! system prompt tripped `prompt_demands_structured_output` — because that
//! predicate routes execution away from the tool loop and into the
//! process-wide startup executor, which held only env-sourced keys.
//!
//! Measured blast radius at the time: 17 of 96 curated cards took the
//! bypass, 16 of them declaring `anthropic` — so they *succeeded*, on the
//! platform's key, for agents owned by other people. A silent
//! cross-tenant billing leak.
//!
//! # Why these tests can assert without a network
//!
//! Credential resolution happens **before** any HTTP request on every
//! path. So an unfunded agent fails with `ExecutionError::Unfunded`
//! rather than a transport error, and a *funded* agent gets past
//! resolution and fails later (at the network) — which is exactly the
//! signal we need to distinguish "used the owner's key" from "refused for
//! lack of one".

#![cfg(test)]

use crate::agent_backend::credentials::{CredentialSource, ResolvedCredentials};
use crate::agent_backend::executor::{AgentExecutor, ExecutionContext, ExecutionError};
use crate::agent_backend::llm_executor::LLMExecutor;
use crate::agent_backend::multi_model_executor::MultiModelExecutor;
use crate::agent_backend::tool_executor::prompt_demands_structured_output;
use crate::agent_backend::AgentCard;
use crate::ast::{AgentStmt, Program};

/// A prompt that trips the structured-output heuristic → tool-loop bypass.
const STRUCTURED_PROMPT: &str = "Return a valid JSON object. Output ONLY JSON.";
/// A prompt that does not → normal tool-loop path.
const NARRATIVE_PROMPT: &str = "You are a helpful research analyst. Explain your reasoning.";

fn card(agent_id: &str, provider: &str, system_prompt: &str) -> AgentCard {
    let mut c = AgentCard::new(agent_id.to_string(), "research".to_string());
    c.capabilities.provider = provider.to_string();
    c.system_prompt = Some(system_prompt.to_string());
    c
}

fn stmt(agent_id: &str) -> AgentStmt {
    AgentStmt {
        name: agent_id.to_string(),
        agent_type: Some("research".to_string()),
        query: "What is the outlook?".to_string(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    }
}

fn ctx(c: AgentCard, creds: std::sync::Arc<ResolvedCredentials>) -> ExecutionContext {
    ExecutionContext::for_agent(Program { statements: vec![] }, c).with_credentials(creds)
}

/// Sanity-check the fixtures actually straddle the branch under test. If
/// the heuristic is ever narrowed (SPEC_28 §8), this fails loudly instead
/// of the parity tests below silently testing one path twice.
#[test]
fn fixtures_straddle_the_bypass_branch() {
    assert!(
        prompt_demands_structured_output(STRUCTURED_PROMPT),
        "STRUCTURED_PROMPT must trip the bypass"
    );
    assert!(
        !prompt_demands_structured_output(NARRATIVE_PROMPT),
        "NARRATIVE_PROMPT must not trip the bypass"
    );
}

/// **The headline invariant.** Two agents identical but for output shape
/// resolve credentials identically — same key, same source, same funding
/// principal. Asserting on `CredentialSource`, not merely on success:
/// the old leak *succeeded*, so a success-only assertion would pass on
/// the buggy code.
#[test]
fn funding_is_identical_regardless_of_output_shape() {
    let creds = ResolvedCredentials::builder()
        .funding_principal("mario")
        .key("deepseek", "sk-mario-owned", CredentialSource::AgentScoped)
        .build_arc();

    let structured = ctx(
        card("valuation_agent", "deepseek", STRUCTURED_PROMPT),
        creds.clone(),
    );
    let narrative = ctx(
        card("valuation_agent", "deepseek", NARRATIVE_PROMPT),
        creds.clone(),
    );

    assert_eq!(
        structured.key_for("deepseek").unwrap(),
        narrative.key_for("deepseek").unwrap(),
        "same agent must resolve the same key on both paths"
    );
    assert_eq!(
        structured.credentials.source_for("deepseek"),
        narrative.credentials.source_for("deepseek"),
    );
    assert_eq!(
        structured.credentials.funding_principal(),
        narrative.credentials.funding_principal(),
        "the account that pays must not change with output shape"
    );
    assert_eq!(structured.key_for("deepseek").unwrap(), "sk-mario-owned");
}

/// The bypass path for `anthropic` — the silent leak. `LLMExecutor` is
/// what `self.inner` resolves to for Claude cards, and it used to hold a
/// boot-captured env key. An unfunded agent must now refuse, not succeed
/// on the platform's account.
#[tokio::test]
async fn anthropic_bypass_path_refuses_when_unfunded() {
    let exec = LLMExecutor::new();
    let context = ctx(
        card("efra_forensic", "anthropic", STRUCTURED_PROMPT),
        ResolvedCredentials::unfunded_arc(),
    );

    match exec.execute(&stmt("efra_forensic"), &context).await {
        Err(ExecutionError::Unfunded { agent_id, provider }) => {
            assert_eq!(agent_id, "efra_forensic");
            assert_eq!(provider, "anthropic");
        }
        Err(other) => panic!("expected Unfunded, got: {other}"),
        Ok(_) => panic!(
            "SILENT BILLING LEAK: an unfunded agent executed successfully. \
             It must have drawn on a key it was not entitled to."
        ),
    }
}

/// Same for the multi-provider bypass. Also pins the *message*: the old
/// error told the agent's owner to "Set DEEPSEEK_API_KEY env var" — a
/// server-side action they cannot perform.
#[tokio::test]
async fn non_anthropic_bypass_path_refuses_when_unfunded_with_owner_facing_message() {
    let exec = MultiModelExecutor::from_env().expect("executor construction reads no credentials");
    let context = ctx(
        card("valuation_agent", "deepseek", STRUCTURED_PROMPT),
        ResolvedCredentials::unfunded_arc(),
    );

    let err = exec
        .execute(&stmt("valuation_agent"), &context)
        .await
        .expect_err("unfunded agent must not execute");

    assert!(
        matches!(err, ExecutionError::Unfunded { .. }),
        "expected structured Unfunded, got: {err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("valuation_agent"), "names the agent: {msg}");
    assert!(msg.contains("DEEPSEEK_API_KEY"), "names the key: {msg}");
    assert!(
        !msg.contains("env var"),
        "must not ask the owner to set a server env var: {msg}"
    );
}

/// The positive case: a funded owner key is actually *reached* on the
/// bypass path. We can't complete the call offline, but we can prove
/// resolution succeeded — the failure must be a transport/API error, not
/// `Unfunded`. This is what confirms the owner's key is used rather than
/// ignored in favour of the platform's.
#[tokio::test]
async fn funded_owner_key_is_used_on_the_bypass_path() {
    let exec = MultiModelExecutor::from_env().unwrap();
    let creds = ResolvedCredentials::builder()
        .funding_principal("mario")
        .key(
            "deepseek",
            "sk-not-a-real-key",
            CredentialSource::PrincipalDefault,
        )
        .build_arc();
    let context = ctx(
        card("valuation_agent", "deepseek", STRUCTURED_PROMPT),
        creds,
    );

    match exec.execute(&stmt("valuation_agent"), &context).await {
        Err(ExecutionError::Unfunded { .. }) => panic!(
            "the owner's stored key was not consulted — this is the \
             original defect: a funded agent reported as unfunded"
        ),
        // Anything else means we got past credential resolution and
        // attempted the call with the owner's key. That's the assertion.
        _ => {}
    }
}

/// Provider parity: every provider an owner can fund on their profile
/// page resolves through the same path. Guards against a new provider
/// being wired up with a bespoke env branch.
#[test]
fn every_supported_provider_resolves_from_the_store() {
    for provider in [
        "anthropic",
        "deepseek",
        "glm",
        "kimi",
        "mistral",
        "qwen",
        "openrouter",
    ] {
        let creds = ResolvedCredentials::builder()
            .key(
                provider,
                format!("sk-{provider}"),
                CredentialSource::PrincipalDefault,
            )
            .build_arc();
        let context = ctx(card("a", provider, STRUCTURED_PROMPT), creds);
        assert_eq!(
            context.key_for(provider).unwrap(),
            format!("sk-{provider}"),
            "provider {provider} must resolve from the credential store"
        );
    }
}

/// Ollama authenticates by network locality, so it must remain usable
/// with no credential — without that exception becoming a general
/// "empty key is fine" hole for paid providers.
#[test]
fn ollama_needs_no_credential_but_paid_providers_still_do() {
    let context = ctx(
        card("local_agent", "ollama", NARRATIVE_PROMPT),
        ResolvedCredentials::unfunded_arc(),
    );
    assert_eq!(context.key_for("ollama").unwrap(), "");
    assert!(
        context.key_for("deepseek").is_err(),
        "the ollama exception must not generalise to paid providers"
    );
}
