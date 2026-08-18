/// Agent Executors
///
/// Pluggable execution engines for agents.
/// Currently implements Mock executor for testing.
use crate::agent_backend::agent_card::{AgentCard, CognitionTier};
use crate::agent_backend::credentials::ResolvedCredentials;
use crate::ast::{AgentStmt, EvidenceStmt, Program};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Execution context passed to executors
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub program: Program,
    pub agent_card: AgentCard,
    /// Creature ID executing this agent (None for non-creature executions)
    pub creature_id: Option<Uuid>,
    /// Creature's cognition tier — drives model ladder resolution (None = use card defaults)
    pub cognition_tier: Option<CognitionTier>,
    /// Provider credentials for THIS execution, resolved from the
    /// `agent_credentials` store (SPEC_28). Carried here — rather than
    /// captured in an executor at construction — so funding is per-agent
    /// and identical on every execution path, including the tool-loop
    /// bypasses that structured-output agents take.
    ///
    /// Defaults to `unfunded`, which fails loudly. A new execution path
    /// that forgets to resolve credentials must not quietly spend the
    /// platform's money.
    pub credentials: Arc<ResolvedCredentials>,
}

impl ExecutionContext {
    /// Single-agent context with no credentials. For tests, mock
    /// executions, and any path that must not be able to spend.
    /// Attach real credentials with `with_credentials`.
    pub fn for_agent(program: Program, agent_card: AgentCard) -> Self {
        Self {
            program,
            agent_card,
            creature_id: None,
            cognition_tier: None,
            credentials: ResolvedCredentials::unfunded_arc(),
        }
    }

    pub fn with_credentials(mut self, credentials: Arc<ResolvedCredentials>) -> Self {
        self.credentials = credentials;
        self
    }

    pub fn with_creature(mut self, id: Uuid, tier: Option<CognitionTier>) -> Self {
        self.creature_id = Some(id);
        self.cognition_tier = tier;
        self
    }

    /// Resolve the API key for `provider` for this execution.
    /// Convenience so executors don't repeat the agent-id plumbing.
    pub fn key_for(&self, provider: &str) -> Result<&str, ExecutionError> {
        self.credentials
            .key_for(provider, self.agent_card.agent_id.as_str())
    }

    /// Funding provenance for `provider`, for stamping onto `AgentMetadata`
    /// so every run records which account paid. Returns
    /// `(funding_principal, credential_source)`.
    pub fn funding_provenance(&self, provider: &str) -> (Option<String>, Option<String>) {
        (
            self.credentials.funding_principal().map(str::to_string),
            Some(self.credentials.source_for(provider).as_str().to_string()),
        )
    }

    /// `md5` of the card's declared system prompt, for stamping onto
    /// `AgentMetadata` so a cross-check can tell which prompt produced a row.
    ///
    /// Hashes `agent_card.system_prompt` — the card's own value as RESOLVED
    /// server-side for this run — and deliberately not the fully assembled
    /// system message. `LlmExecutor::build_system_prompt` appends to the card's
    /// text, so hashing the assembled string would produce a value that could
    /// never equal `md5(agents.system_prompt)` and the cohort comparison would
    /// match nothing while looking like it worked.
    ///
    /// Resolved rather than caller-supplied is the load-bearing part: it is the
    /// card the executor actually ran, not a claim about it. That distinction is
    /// exactly what `stamp_invocation` got wrong when it filed the caller's
    /// assertion about input binding as fact.
    ///
    /// `None` when the card declares no prompt, which is honest — such a run is
    /// steered by an executor default, so there is no card text to attribute it
    /// to, and it must not join any card's cohort.
    ///
    /// SHA-256, hex, lowercase. `sha2` is already a direct dependency and
    /// Postgres has `sha256()` natively, so the comparison needs no new crate on
    /// either side. The SQL counterpart is exactly:
    ///
    /// ```sql
    /// encode(sha256(convert_to(a.system_prompt, 'UTF8')), 'hex')
    /// ```
    ///
    /// `convert_to(..., 'UTF8')` matters: `sha256` takes `bytea`, and letting
    /// the server pick an encoding would make the hash depend on database
    /// settings rather than on the prompt.
    pub fn card_prompt_hash(&self) -> Option<String> {
        use sha2::{Digest, Sha256};
        self.agent_card.system_prompt.as_deref().map(|p| {
            let mut h = Sha256::new();
            h.update(p.as_bytes());
            format!("{:x}", h.finalize())
        })
    }
}

/// A record of a single tool invocation during agentic execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub output: String,
    pub duration_ms: u64,
    pub iteration: u32,
}

/// Agent execution output
#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub agent_name: String,
    pub agent_type: String,
    pub timestamp: DateTime<Utc>,
    pub status: AgentStatus,
    pub evidence: Vec<EvidenceStmt>,
    pub confidence: f64,
    pub sources_consulted: Vec<String>,
    pub execution_time_ms: u64,
    /// Total tokens (input + output). Retained as the headline figure
    /// every existing reader expects; prefer `input_tokens` /
    /// `output_tokens` for anything that prices a run.
    pub tokens_used: Option<u32>,
    /// Input (prompt) tokens, when the provider reported the split.
    ///
    /// Providers charge 3–5× more for output than input, so a total alone
    /// cannot price a run better than ±2× — named in
    /// `PLATFORM_ECONOMICS.md` §4.1 as the largest remaining source of
    /// cost error. The Anthropic paths always tracked the split and threw
    /// it away when summing; these two fields stop that.
    pub input_tokens: Option<u32>,
    /// Output (completion) tokens, when the provider reported the split.
    pub output_tokens: Option<u32>,
    pub metadata: AgentMetadata,
    /// Tool invocations performed during agentic loop (empty for single-turn)
    pub tool_invocations: Vec<ToolInvocation>,
    /// Number of LLM round-trips (1 for single-turn, >1 for tool-using agents)
    pub loop_iterations: u32,
    /// The model's final text, verbatim, before `parse_evidence_text`
    /// digests it into `evidence` / `confidence` / `metadata.reasoning`.
    ///
    /// That digest is **per-agent** — `tool_executor.rs` special-cases
    /// `genome_profiler` to reach into a nested `conservation` object — so
    /// what used to survive a run was one hand-written reading of the
    /// output rather than the output. Changing the parser silently changed
    /// the historical record.
    ///
    /// Persisted to `episodes.response_text` (migration 199). It is the
    /// evidence base for inducing an agent's output type from what it has
    /// actually produced, instead of from what its card claims — the
    /// distinction that separates a real contract from the seven
    /// `output_contract`s that name schemas which do not exist.
    ///
    /// `None` where the executor never had a single text document to point
    /// at (multi-model fan-out) rather than as a shorthand for empty.
    pub raw_response: Option<String>,
}

impl AgentOutput {
    /// Price this run against the rate card.
    ///
    /// The **only** place an execution is converted to money, so the
    /// persistence path and the in-memory usage rollup cannot drift onto
    /// different cost bases — which is exactly how a DeepSeek agent came
    /// to be recorded at Anthropic Sonnet's rate.
    ///
    /// Uses the real input/output split when the provider reported it and
    /// falls back to an assumed split otherwise, marking the result
    /// accordingly so a consumer can tell the two apart. `None` only when
    /// no token count was reported at all.
    pub fn cost(&self) -> Option<crate::agent_backend::rate_card::CostEstimate> {
        use crate::agent_backend::rate_card;
        // Prefer the model/provider that actually served the run over
        // whatever the card declared — they diverge on any ladder
        // resolution or provider fallback.
        let provider = self.metadata.provider.as_deref().unwrap_or_default();
        let model = self.metadata.model_used.as_deref().unwrap_or_default();
        match (self.input_tokens, self.output_tokens) {
            (Some(i), Some(o)) => Some(rate_card::cost_of_split(provider, model, i, o)),
            _ => self
                .tokens_used
                .map(|t| rate_card::cost_of_total(provider, model, t)),
        }
    }
}

/// Agent execution status
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Success,
    Failed,
    Timeout,
    BelowConfidenceThreshold,
}

/// Agent execution metadata
#[derive(Debug, Clone, Default)]
pub struct AgentMetadata {
    pub model_used: Option<String>,
    pub temperature: Option<f64>,
    pub reasoning: Option<String>,
    /// Provider that handled the request (anthropic, openai, openrouter, …) —
    /// resolved at execution time, not just whatever the card declared.
    pub provider: Option<String>,
    /// SPEC_28 — funding provenance: whose budget bore the raw LLM cost.
    /// `abw-system` for platform-service agents, else the owner's user_id.
    ///
    /// Recorded per run so the economics are auditable after the fact
    /// rather than inferred. Without this, "which key paid for this?" was
    /// unanswerable — which is how a cross-tenant billing leak survived a
    /// release unnoticed.
    pub funding_principal: Option<String>,
    /// How that credential was resolved: `agent_scoped`,
    /// `principal_default`, `legacy_user_secrets`, or `unfunded`.
    pub credential_source: Option<String>,
    /// Final stop / finish reason reported by the LLM
    /// (anthropic: stop / tool_use / max_tokens / end_turn / pause_turn / refusal;
    ///  openai-compatible: stop / tool_calls / length / content_filter / function_call).
    /// Critical for diagnosing tool-loop exits and silent truncations
    /// (see issue #3 / docs/specs/10_RESEARCH_AGENTS_EMPTY_LLM_OUTPUT.md).
    pub stop_reason: Option<String>,
    /// If the executor decided this run did not actually succeed
    /// (e.g. empty content after consuming tokens, tool-loop cap-out without
    /// final text), a short machine-readable reason string. None = no failure.
    pub failure_reason: Option<String>,
    /// Doc 12 § Capability 2 — agent version at execution time. Populated
    /// by the caller (workspace message handler, execution endpoint) after
    /// the executor returns, by resolving `MAX(version_number)` from
    /// `agent_versions` for this agent. Executors themselves leave these
    /// as `None`.
    pub agent_version_id: Option<uuid::Uuid>,
    pub agent_version_number: Option<i32>,
    /// `md5` of the card's declared system prompt, as resolved for this run.
    ///
    /// ── Why a content hash and not `agent_version_id` ──────────────────
    ///
    /// `agent_versions` already exists, holds `system_prompt`, and has 46
    /// rows, and the two fields above were added to carry its id here. Both
    /// paths are dead: **3,391 episodes, 0 carrying either field**, and
    /// `weather_oracle` — whose prompt has been edited repeatedly — has **0
    /// version rows**. `calibration.rs` already notes that per-version Brier
    /// needs `agent_version_id` and cannot have it. So versioning would have to
    /// be revived at both ends *and* given a policy about who cuts a version
    /// when a card file changes, before it could answer one question.
    ///
    /// A hash needs no policy and cannot drift, because it is derived from the
    /// content rather than maintained alongside it. Nobody has to remember to
    /// bump it.
    ///
    /// ── Why the executor sets it ───────────────────────────────────────
    ///
    /// This is the hash of the prompt that was **sent**, taken where it is
    /// sent. Hashing the card at the handler would record what a caller
    /// believed the card said — the same defect `stamp_invocation` had when it
    /// filed the caller's claim about input binding as fact.
    ///
    /// It exists so a cross-check can ask "was this row produced by the prompt
    /// this agent has now?", which is the difference between a suite that detects
    /// a defect and one that can also confirm a fix.
    pub card_prompt_hash: Option<String>,
}

/// Execution error
#[derive(Debug, Clone)]
pub enum ExecutionError {
    ExecutorNotFound(String),
    ExecutionFailed(String),
    InvalidContext(String),
    Timeout,
    /// No credential is available to pay for `provider` on behalf of
    /// `agent_id` (SPEC_28). Structured rather than a formatted string so
    /// callers can distinguish "unfunded" from "provider returned 5xx" —
    /// the workspace UI could previously only echo
    /// `"Execution failed: Execution failed: DEEPSEEK_API_KEY not set"`.
    Unfunded {
        agent_id: String,
        provider: String,
    },
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ExecutionError::ExecutorNotFound(s) => write!(f, "Executor not found: {}", s),
            ExecutionError::ExecutionFailed(s) => write!(f, "Execution failed: {}", s),
            ExecutionError::InvalidContext(s) => write!(f, "Invalid context: {}", s),
            ExecutionError::Timeout => write!(f, "Execution timeout"),
            // Owner-facing remediation. Deliberately never mentions env
            // vars: the person who can fix this is the agent's owner, on
            // their profile page, not an operator with shell access.
            ExecutionError::Unfunded { agent_id, provider } => write!(
                f,
                "Agent '{}' is not funded for provider '{}'. Its owner needs to set \
                 {}_API_KEY on their profile (Profile → LLM Provider Keys) at {}/profile.",
                agent_id,
                provider,
                provider.to_uppercase(),
                crate::agent_backend::credentials::abw_base_url(),
            ),
        }
    }
}

impl std::error::Error for ExecutionError {}

/// Agent executor trait
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Execute the agent with given context
    async fn execute(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError>;

    /// Name of this executor
    fn name(&self) -> &str;
}

/// Mock executor for testing
pub struct MockExecutor;

impl MockExecutor {
    pub fn new() -> Self {
        MockExecutor
    }
}

#[async_trait::async_trait]
impl AgentExecutor for MockExecutor {
    async fn execute(
        &self,
        agent: &AgentStmt,
        _context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError> {
        use std::time::Instant;
        let start = Instant::now();

        // Generate mock evidence
        let evidence = EvidenceStmt {
            id: format!("{}_evidence_mock", agent.name),
            source: "Mock Executor".to_string(),
            summary: Some("Generated by mock executor for testing purposes".to_string()),
            url: None,
            relevance: Some(0.75),
            date: Some(Utc::now().format("%Y-%m-%d").to_string()),
            strength: Some(0.75),
            key_findings: vec![
                format!("Mock finding 1 for agent '{}'", agent.name),
                format!("Mock finding 2 based on query: {}", agent.query),
                "This is simulated evidence for testing".to_string(),
            ],
        };

        let elapsed = start.elapsed();

        Ok(AgentOutput {
            raw_response: None,
            agent_name: agent.name.clone(),
            agent_type: agent.agent_type.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            status: AgentStatus::Success,
            evidence: vec![evidence],
            confidence: 0.75,
            sources_consulted: vec!["mock://test".to_string()],
            execution_time_ms: elapsed.as_millis() as u64,
            tokens_used: Some(100), // Mock token count
            // Mock runs spend nothing; the split is stated rather than left
            // absent so the mock exercises the measured-split code path.
            input_tokens: Some(80),
            output_tokens: Some(20),
            metadata: AgentMetadata {
                model_used: Some("mock-model".to_string()),
                temperature: Some(0.0),
                reasoning: Some("Mock execution for testing".to_string()),
                ..Default::default()
            },
            tool_invocations: vec![],
            loop_iterations: 1,
        })
    }

    fn name(&self) -> &str {
        "mock"
    }
}

impl Default for MockExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AgentStmt, Schedule, TimeUnit};

    #[tokio::test]
    async fn test_mock_executor() {
        let agent = AgentStmt {
            name: "test_agent".to_string(),
            agent_type: Some("research".to_string()),
            query: "Test query".to_string(),
            executor: None,
            schedule: Some(Schedule::Every {
                interval: 1,
                unit: TimeUnit::Day,
            }),
            driver_refs: vec![],
            depends_on: vec![],
            confidence_threshold: None,
        };

        let program = Program { statements: vec![] };
        let card =
            crate::agent_backend::AgentCard::new("test_agent".to_string(), "research".to_string());

        let context = ExecutionContext::for_agent(program, card);

        let executor = MockExecutor::new();
        let result = executor.execute(&agent, &context).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.agent_name, "test_agent");
        assert_eq!(output.status, AgentStatus::Success);
        assert_eq!(output.evidence.len(), 1);
        assert_eq!(output.confidence, 0.75);
    }

    /// Pins the optional fields (issue #3 + issue #5) so older constructors
    /// that don't set them keep compiling via `..Default::default()`.
    #[test]
    fn agent_metadata_default_has_none_observability_fields() {
        let m = AgentMetadata::default();
        assert!(m.model_used.is_none());
        assert!(m.temperature.is_none());
        assert!(m.reasoning.is_none());
        assert!(m.provider.is_none());
        assert!(m.stop_reason.is_none());
        assert!(m.failure_reason.is_none());
        // Doc 12 § Capability 2 — version stamp fields default to None.
        assert!(m.agent_version_id.is_none());
        assert!(m.agent_version_number.is_none());
    }
}
