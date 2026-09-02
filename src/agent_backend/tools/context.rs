use crate::agent_backend::registry::AgentRegistry;
use agent_bestiary_memory::embeddings::EmbeddingGenerator;
use agent_bestiary_memory::store::MemoryStore;
use agent_bestiary_ontology::WorkspaceGitManager;
use std::sync::Arc;
use uuid::Uuid;

/// Context available to tools during execution
pub struct ToolContext {
    pub memory_store: Arc<MemoryStore>,
    pub embedder: Arc<dyn EmbeddingGenerator>,
    pub registry: Arc<AgentRegistry>,
    pub current_agent_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub workspace_slug: Option<String>,
    pub workspace_git: Option<Arc<WorkspaceGitManager>>,
    pub db: Option<sqlx::PgPool>,
    pub gas_fees: Option<crate::gas::GasFees>,
    pub user_id: Option<String>,
    /// Third-party / MCP tool credentials for the agent owner. NOT LLM
    /// provider keys — those live on `ExecutionContext.credentials`
    /// (SPEC_28). Renaming this to `tool_secrets` is P5.3.
    pub user_secrets: Option<std::collections::HashMap<String, String>>,
    /// LLM provider credentials for the *current* execution, carried here
    /// only so the delegation tools (`execute_agent`, `delegate_to_agent`)
    /// can propagate them when they build a child `ExecutionContext`.
    ///
    /// Executors read credentials from `ExecutionContext`, never from
    /// here. Today a delegated child runs on the parent's credentials,
    /// matching the pre-existing `user_secrets` propagation above;
    /// funding a child by *its own* owner is a SPEC_28 P5.2 follow-up.
    pub credentials: std::sync::Arc<crate::agent_backend::credentials::ResolvedCredentials>,
    /// Episode id of the execution currently running (mig-198).
    ///
    /// Set by whoever mints the episode id, BEFORE execution starts, so the
    /// delegation tools can stamp it as `parent_episode_id` on the child
    /// episodes they write. That is what makes a compound execution's true
    /// cost recoverable: the caller records only its own tokens, and the tree
    /// is reassembled from these links.
    ///
    /// `None` for paths that don't persist an episode; their delegated
    /// children are still recorded, just as roots.
    pub parent_episode_id: Option<Uuid>,
    /// Optional eval-trigger bridge. The library can't reach AppState
    /// (it lives in the bin), so handlers that have AppState build an
    /// EvalTriggerImpl and stash it here. The MCP tool
    /// `run_evaluator_registry` calls into this. Sites that pass `None`
    /// get a graceful tool error instead of a trigger.
    pub eval_trigger: Option<Arc<dyn EvalTrigger>>,
    /// Remote MCP tools this agent may call, discovered from the
    /// `mcp_servers` block on its own card.
    ///
    /// Deliberately carried on the context rather than resolved from a
    /// global registry: this is an authorization boundary. Builtin tools
    /// are global (every agent gets all of them and `execute` performs no
    /// per-agent check) — remote tools must not inherit that, or one
    /// agent's third-party credential becomes every agent's.
    ///
    /// `None` means the caller did not resolve remote tools; the agent
    /// simply has none. Never a silent anonymous fallback.
    pub remote_mcp: Option<Arc<crate::agent_backend::mcp_client::RemoteMcpCatalogue>>,
}

/// Bridge for triggering an eval run from inside a tool handler.
///
/// Implemented in `src/handlers/eval.rs` (where AppState is in scope).
/// The library-side tools.rs can't see AppState directly, so we abstract
/// the trigger behind this trait. ToolContexts that have access to
/// AppState (workspace chat, /api/agents/:id/execute) populate it.
#[async_trait::async_trait]
pub trait EvalTrigger: Send + Sync {
    /// Trigger an eval run for the given agent. Returns the new run_id.
    /// `user_id` is required to charge the wallet; `judge` toggles the
    /// LlmJudgeEvaluator inside the registry; `tags` filters test cases.
    async fn trigger_eval(
        &self,
        agent_id: Uuid,
        user_id: String,
        judge: bool,
        tags: Vec<String>,
    ) -> Result<Uuid, String>;
}
