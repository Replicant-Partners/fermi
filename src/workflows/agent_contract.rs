//! The ABW agent contract — one definition, both authoring paths.
//!
//! # Why this module exists
//!
//! An agent can reach the public catalogue two ways, and until now only
//! one of them was validated:
//!
//! 1. **On disk** — `agents/curated/<name>/agent_card.json`, loaded by
//!    `seed_agents_to_database` and published unconditionally. Guarded by
//!    the conformance regression tests in
//!    [`crate::agent_backend::agent_card`], which assert description,
//!    tags, sample_queries, valence and wallet.
//! 2. **Over the API** — `POST /api/agents` then `POST /api/agents/:id/publish`.
//!    `create_agent_handler` validates only the slug; every other field is
//!    `#[serde(default)]`. The publish gate then checked four things:
//!    name, description, system_prompt, and at least one tag.
//!
//! The consequence is visible in production: community agents published
//! with `accepts: []`, `produces: []`, `sample_queries: []`, no valence
//! and no taxonomy. They are unroutable by composition planning, invisible
//! to valence-diversity checks, and undiscoverable by example — yet they
//! sit in the public catalogue next to fully-formed curated specimens.
//!
//! The root cause was not a missing check in one place. It was that "what
//! a well-formed agent is" had **two** definitions that could drift. This
//! module is the single definition. Both paths call it.
//!
//! # What is deliberately NOT here
//!
//! - **Wallet.** A card-only concept; DB agents fund through `agent_wallets`.
//!   The curated-card test keeps asserting it separately.
//! - **Taxonomy.** Derived at birth by `fermi::taxonomy::derive` for every
//!   API-created agent (SPEC_30 / mig-186), so requiring it would fail
//!   nothing and catch nothing. The *editorial* ranks that do need a human
//!   are intentionally optional.
//! - **Anything advisory.** Violations here block a publish. Softer
//!   signals stay as `CheckSeverity::Warning` in
//!   [`crate::workflows::publish_pipeline::run_publish_checks`].
//! - **A system prompt for non-LLM executors.** `coherence_evaluator` is
//!   a deterministic MCP constraint-satisfaction engine; a persona would
//!   be meaningless. See [`ContractView::requires_persona`].

use super::types::{CheckSeverity, PublishCheck};

/// A read-only projection of the fields the contract cares about.
///
/// Exists so `Agent` (the DB row) and `AgentCard` (the on-disk card) can
/// be judged by identical code without either type depending on the
/// other, and without this module depending on both.
#[derive(Debug, Clone, Default)]
pub struct ContractView<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub system_prompt: Option<&'a str>,
    pub tags: &'a [String],
    pub sample_queries: &'a [String],
    pub accepts: &'a [String],
    pub produces: &'a [String],
    /// Whether an affective signature is present. A bool rather than the
    /// value because the two sources type it differently (`AgentValence`
    /// on the card, untyped `serde_json::Value` on the row) and presence
    /// is all the contract asserts.
    pub has_valence: bool,
    /// True for LLM-backed executors, which are steered by a prompt and
    /// are incoherent without one.
    ///
    /// False for `mcp` / `manual` / `skill`. A deterministic agent
    /// — `coherence_evaluator` runs Thagard TEC constraint satisfaction
    /// — has no persona to express and no decision policy a prompt could
    /// influence. Demanding one would force authors to write filler text
    /// that never reaches a model, which teaches them the checks are
    /// theatre.
    pub requires_persona: bool,
}

/// One unmet requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Stable machine name. Also the `PublishCheck::name`, so UI copy and
    /// the publish-checks endpoint can key off it.
    pub check: &'static str,
    /// Operator-facing explanation of what to add and why it matters.
    pub message: &'static str,
}

/// Every requirement, in the order an author would naturally fill them.
///
/// Each entry is `(check name, predicate, failure message)`. Keeping them
/// in one table means [`contract_violations`] and [`contract_checks`]
/// cannot disagree about the requirement set.
type Requirement = (
    &'static str,
    fn(&ContractView) -> bool,
    &'static str,
    &'static str,
);

fn requirements() -> &'static [Requirement] {
    &[
        (
            "name_set",
            |v| !v.name.trim().is_empty(),
            "Agent must have a name",
            "Name present",
        ),
        (
            "description_present",
            |v| v.description.is_some_and(|d| !d.trim().is_empty()),
            "Description is required — it is the only thing most people read before hiring",
            "Description present",
        ),
        (
            "system_prompt_present",
            |v| {
                !v.requires_persona || v.system_prompt.is_some_and(|p| !p.trim().is_empty())
            },
            "System prompt is required for LLM-backed agents — without one the agent has no persona or decision policy",
            "System prompt present (or not applicable to this executor)",
        ),
        (
            "has_tags",
            |v| !v.tags.is_empty(),
            "At least one tag is required for catalogue discovery",
            "Tags present",
        ),
        (
            "has_sample_queries",
            |v| !v.sample_queries.is_empty(),
            "At least one sample query is required — without one, nobody can tell what to ask this agent",
            "Sample queries present",
        ),
        (
            "declares_accepts",
            |v| !v.accepts.is_empty(),
            "`accepts` must declare at least one input type — composition planning cannot route work to an agent with no declared inputs",
            "Input contract declared",
        ),
        (
            "declares_produces",
            |v| !v.produces.is_empty(),
            "`produces` must declare at least one output type — downstream agents match against it to build pipelines",
            "Output contract declared",
        ),
        (
            "has_valence",
            |v| v.has_valence,
            "`valence` is required — the affective signature drives valence-diversity checks that stop a composition becoming an echo chamber",
            "Valence present",
        ),
    ]
}

/// Judge a view against the contract. Empty result means conforming.
pub fn contract_violations(view: &ContractView) -> Vec<Violation> {
    requirements()
        .iter()
        .filter(|(_, passes, _, _)| !passes(view))
        .map(|(check, _, message, _)| Violation { check, message })
        .collect()
}

/// The same requirements rendered as [`PublishCheck`]s — pass and fail
/// alike, because the publish-checks endpoint shows the full list as a
/// to-do rather than only what is broken.
pub fn contract_checks(view: &ContractView) -> Vec<PublishCheck> {
    requirements()
        .iter()
        .map(|(check, passes, fail_msg, ok_msg)| {
            let passed = passes(view);
            PublishCheck {
                name: (*check).to_string(),
                passed,
                severity: CheckSeverity::Error,
                message: if passed { *ok_msg } else { *fail_msg }.to_string(),
            }
        })
        .collect()
}

/// True when the view satisfies every requirement.
pub fn conforms(view: &ContractView) -> bool {
    contract_violations(view).is_empty()
}

// ─── Source adapters ───────────────────────────────────────────────

impl<'a> From<&'a agent_bestiary_memory::types::Agent> for ContractView<'a> {
    fn from(a: &'a agent_bestiary_memory::types::Agent) -> Self {
        ContractView {
            name: &a.agent_name,
            description: a.description.as_deref(),
            system_prompt: a.system_prompt.as_deref(),
            tags: &a.tags,
            sample_queries: &a.sample_queries,
            accepts: &a.accepts,
            produces: &a.produces,
            has_valence: a.valence.is_some(),
            // `executor_type` is a free-text column; anything that is not
            // explicitly a non-LLM executor is treated as LLM-backed, so a
            // typo cannot silently exempt an agent from needing a prompt.
            requires_persona: !matches!(
                a.executor_type.to_ascii_lowercase().as_str(),
                "mcp" | "manual" | "skill"
            ),
        }
    }
}

impl<'a> From<&'a crate::agent_backend::agent_card::AgentCard> for ContractView<'a> {
    fn from(c: &'a crate::agent_backend::agent_card::AgentCard) -> Self {
        use crate::ast::ExecutorType;
        ContractView {
            name: &c.agent_id,
            description: Some(&c.metadata.description),
            system_prompt: c.system_prompt.as_deref(),
            tags: &c.metadata.tags,
            sample_queries: &c.metadata.sample_queries,
            accepts: &c.accepts,
            produces: &c.produces,
            has_valence: c.metadata.valence.is_some(),
            requires_persona: matches!(c.capabilities.executor, ExecutorType::LLM),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// A view that satisfies every requirement, for tests to break one
    /// field at a time.
    fn conforming() -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
        (
            strings(&["research"]),
            strings(&["What is the base rate for X?"]),
            strings(&["forecast-question"]),
            strings(&["evidence"]),
        )
    }

    fn view<'a>(
        tags: &'a [String],
        samples: &'a [String],
        accepts: &'a [String],
        produces: &'a [String],
    ) -> ContractView<'a> {
        ContractView {
            name: "probe",
            description: Some("Does a thing, carefully."),
            system_prompt: Some("You are a probe."),
            tags,
            sample_queries: samples,
            accepts,
            produces,
            has_valence: true,
            requires_persona: true,
        }
    }

    #[test]
    fn fully_formed_agent_conforms() {
        let (t, s, a, p) = conforming();
        let v = view(&t, &s, &a, &p);
        assert!(conforms(&v), "violations: {:?}", contract_violations(&v));
    }

    /// The shape actually observed in production: description, prompt and
    /// a tag, but nothing that makes the agent usable or composable. This
    /// passed the old four-item gate.
    #[test]
    fn efra_shaped_agent_is_rejected() {
        let tags = strings(&["pipeline"]);
        let empty: Vec<String> = vec![];
        let v = ContractView {
            name: "efra_scout",
            description: Some("SCOUT is the first and cheapest filter in the pipeline."),
            system_prompt: Some("You are SCOUT."),
            tags: &tags,
            sample_queries: &empty,
            accepts: &empty,
            produces: &empty,
            has_valence: false,
            requires_persona: true,
        };
        let names: Vec<_> = contract_violations(&v).iter().map(|x| x.check).collect();
        assert_eq!(
            names,
            vec![
                "has_sample_queries",
                "declares_accepts",
                "declares_produces",
                "has_valence"
            ]
        );
    }

    #[test]
    fn each_requirement_is_independently_enforced() {
        let (t, s, a, p) = conforming();
        let empty: Vec<String> = vec![];

        let mut missing_tags = view(&t, &s, &a, &p);
        missing_tags.tags = &empty;
        assert!(!conforms(&missing_tags));

        let mut missing_samples = view(&t, &s, &a, &p);
        missing_samples.sample_queries = &empty;
        assert!(!conforms(&missing_samples));

        let mut missing_accepts = view(&t, &s, &a, &p);
        missing_accepts.accepts = &empty;
        assert!(!conforms(&missing_accepts));

        let mut missing_produces = view(&t, &s, &a, &p);
        missing_produces.produces = &empty;
        assert!(!conforms(&missing_produces));

        let mut missing_valence = view(&t, &s, &a, &p);
        missing_valence.has_valence = false;
        assert!(!conforms(&missing_valence));

        let mut blank_description = view(&t, &s, &a, &p);
        blank_description.description = Some("   ");
        assert!(!conforms(&blank_description));

        let mut no_prompt = view(&t, &s, &a, &p);
        no_prompt.system_prompt = None;
        assert!(!conforms(&no_prompt));

        let mut blank_name = view(&t, &s, &a, &p);
        blank_name.name = "  ";
        assert!(!conforms(&blank_name));
    }

    /// A deterministic executor has no persona to express, so an empty
    /// prompt must not block it. `coherence_evaluator` is the live case:
    /// executor `mcp`, model `deterministic`, Thagard TEC constraint
    /// satisfaction, `system_prompt: ""`.
    #[test]
    fn non_llm_executor_is_exempt_from_system_prompt() {
        let (t, s, a, p) = conforming();

        let mut deterministic = view(&t, &s, &a, &p);
        deterministic.system_prompt = Some("");
        deterministic.requires_persona = false;
        assert!(
            conforms(&deterministic),
            "violations: {:?}",
            contract_violations(&deterministic)
        );

        // The exemption is narrow: it covers the prompt and nothing else.
        let empty: Vec<String> = vec![];
        let mut deterministic_no_io = deterministic.clone();
        deterministic_no_io.accepts = &empty;
        assert!(!conforms(&deterministic_no_io));

        // And an LLM agent in the same state is still rejected.
        let mut llm = view(&t, &s, &a, &p);
        llm.system_prompt = Some("");
        llm.requires_persona = true;
        assert!(!conforms(&llm));
    }

    /// A free-text `executor_type` that nobody recognises must default to
    /// "needs a prompt", so a typo cannot buy an exemption.
    #[test]
    fn unrecognised_executor_still_requires_a_prompt() {
        use agent_bestiary_memory::types::Agent;

        let mut agent: Agent = serde_json::from_value(serde_json::json!({
            "agent_id": "00000000-0000-0000-0000-000000000000",
            "agent_name": "probe",
            "agent_type": "research",
            "version": "1.0.0",
            "tier": "community",
            "executor_type": "llmm",
            "model": "claude",
            "temperature": 0.3,
            "author": "tester",
            "visibility": "private",
            "tags": [],
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
            "sample_queries": [],
            "status": "draft",
            "fork_count": 0,
            "accepts": [],
            "produces": [],
            "model_ladder": [],
            "min_tier": "free",
            "capability_gates": {},
            "persona_version": 1,
            "model_params": {}
        }))
        .expect("Agent fixture should deserialize");
        agent.system_prompt = None;

        assert!(ContractView::from(&agent).requires_persona);

        agent.executor_type = "MCP".into();
        assert!(
            !ContractView::from(&agent).requires_persona,
            "executor match should be case-insensitive"
        );
    }

    /// `contract_checks` reports the whole list, not just failures — the
    /// publish-checks endpoint renders it as a to-do.
    #[test]
    fn checks_report_every_requirement_at_error_severity() {
        let (t, s, a, p) = conforming();
        let checks = contract_checks(&view(&t, &s, &a, &p));
        assert_eq!(checks.len(), requirements().len());
        assert!(checks.iter().all(|c| c.passed));
        assert!(checks.iter().all(|c| c.severity == CheckSeverity::Error));
    }

    /// The two functions must never disagree about what failed.
    #[test]
    fn checks_and_violations_agree() {
        let empty: Vec<String> = vec![];
        let tags = strings(&["x"]);
        let v = ContractView {
            name: "probe",
            description: Some("d"),
            system_prompt: Some("p"),
            tags: &tags,
            sample_queries: &empty,
            accepts: &empty,
            produces: &empty,
            has_valence: false,
            requires_persona: true,
        };
        let failed: Vec<_> = contract_checks(&v)
            .into_iter()
            .filter(|c| !c.passed)
            .map(|c| c.name)
            .collect();
        let violated: Vec<_> = contract_violations(&v)
            .into_iter()
            .map(|x| x.check.to_string())
            .collect();
        assert_eq!(failed, violated);
    }
}
