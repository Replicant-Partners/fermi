/// Agent Card
///
/// Complete metadata and performance tracking for an agent.
/// Based on Agent Bestiary Design Document.
use crate::ast::ExecutorType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent card containing all metadata and performance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub agent_id: String,
    pub agent_type: String,
    pub version: String,
    pub tier: AgentTier,
    pub capabilities: AgentCapabilities,
    #[serde(default)]
    pub performance: AgentPerformance,
    #[serde(default)]
    pub usage: AgentUsage,
    pub wallet: Option<AgentWallet>,
    #[serde(default)]
    pub ontology_stats: OntologyStats,
    pub metadata: AgentMetadata,
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// SHA-256 of `system_prompt` as DECLARED, captured before anything augments
    /// it. Hex, lowercase. Never serialised to a card file — it is derived, and a
    /// stored copy could disagree with the text beside it.
    ///
    /// ── Why this cannot be computed at execution time ──────────────────
    ///
    /// By the time an executor runs, `system_prompt` is no longer the card's.
    /// `kg_context::enrich` appends a retrieved-knowledge block built from
    /// similarity scores against the query embedding, so the effective prompt is
    /// per-RUN, not per-card. `orchestras.rs` appends a block too.
    ///
    /// Measured, which is how this was found: five `weather_oracle` runs of one
    /// unchanged card produced FOUR distinct hashes, while the card on disk and
    /// the row in `agents` agreed exactly. Runs four and five matched each other
    /// — same retrieved set, same hash.
    ///
    /// The failure was worse than noise. Agents with empty retrieval hashed to
    /// their card and joined their cohort; agents that had actually learned
    /// something never matched. So a cohort meant to isolate "runs under the
    /// current card" was silently excluding precisely the runs most worth
    /// checking, and reporting INERT — a legitimate-looking state — while doing
    /// it.
    ///
    /// Captured in `resolve_agent_card`, the one place a card's prompt is
    /// bridged from the database, so augmentation downstream cannot destroy it.
    #[serde(default, skip_serializing)]
    pub declared_prompt_sha256: Option<String>,
    #[serde(default)]
    pub dependencies: AgentDependencies,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default)]
    pub produces: Vec<String>,
    #[serde(default)]
    pub workflow_template: Option<WorkflowTemplate>,
    #[serde(default)]
    pub prompt_template: Option<String>,
    #[serde(default)]
    pub requires_secrets: Vec<SecretRequirement>,
}

/// Workflow template for compound agents — static mermaid diagram + stage definitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    // Legacy fields — still required on cards authored before the typed graph.
    #[serde(default)]
    pub mermaid: Option<String>,
    #[serde(default)]
    pub stages: Vec<WorkflowStage>,
    #[serde(default)]
    pub description: Option<String>,

    // ── Phase 2: typed coordination graph (additive, backward compatible) ──────────
    //
    // `nodes` and `edges` are the typed graph. When present, the platform executor
    // (`execute_coordination_graph`) traverses the graph instead of the LLM narrating
    // it. `stages` remains for cards that predate the typed graph; the executor
    // falls back to `stages` if `nodes` is empty.
    //
    // `synthesis` and `selection` belong HERE, not on specialist agent cards.
    // A specialist does not know how a coordinator combines its outputs.
    /// Synthesis protocol for combining member outputs.
    /// `pipeline` | `aggregation` | `selection` | `max_risk` | `cep_weighted`
    #[serde(default)]
    pub synthesis: Option<String>,

    /// Selection criteria for open slots. Passed to `select_agent` at runtime.
    #[serde(default)]
    pub selection: Option<CoordinationSelection>,

    /// Typed graph nodes. Each declares a schema-typed slot.
    /// A node with `agent: None` is an open slot filled by `select_agent`.
    #[serde(default)]
    pub nodes: Vec<CoordinationNode>,

    /// Typed edges — seams between nodes, each carrying a schema ID.
    #[serde(default)]
    pub edges: Vec<CoordinationEdge>,
}

/// Selection criteria for open coordination graph slots.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoordinationSelection {
    /// Candidate pool scope: `"workspace"` (default) | `"fleet"` | `"marketplace"`
    #[serde(default = "default_coordination_scope")]
    pub scope: String,
    /// Fleet name when `scope == "fleet"` (e.g. `"fermi"`, `"simops"`).
    #[serde(default)]
    pub fleet_id: Option<String>,
    /// Scoring weights. Must sum to 1.0. Defaults per `select_agent` tool.
    #[serde(default)]
    pub criteria: Option<SelectionCriteria>,
}

fn default_coordination_scope() -> String {
    "workspace".to_string()
}

/// Scoring weights for `select_agent`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectionCriteria {
    pub brier: Option<f64>,
    pub cost: Option<f64>,
    pub valence_fit: Option<f64>,
    pub fidelity: Option<f64>,
}

/// A schema-typed slot in a coordination graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationNode {
    /// Unique ID within this graph (referenced by `CoordinationEdge`).
    pub id: String,
    /// Bound agent name. `None` = open slot, filled by `select_agent` at runtime.
    #[serde(default)]
    pub agent: Option<String>,
    /// If `true`, the bound agent is never replaced by `select_agent` even if a
    /// higher-scoring candidate exists.
    #[serde(default)]
    pub pinned: bool,
    /// Schema ID this node expects as input.
    #[serde(default)]
    pub input_schema: Option<String>,
    /// Schema ID this node produces as output.
    #[serde(default)]
    pub output_schema: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// A typed seam between two nodes in a coordination graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationEdge {
    pub from: String,
    pub to: String,
    /// Schema ID carried on this edge. Validated against the upstream node’s
    /// `output_schema` and the downstream node’s `input_schema` at seam-check time.
    #[serde(default)]
    pub schema: Option<String>,
}

/// A single stage in a compound agent's workflow pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStage {
    pub name: String,
    /// Agent that fills this slot, or None for an open/user slot
    pub agent: Option<String>,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default)]
    pub produces: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// A credential that an agent needs to function (e.g. API tokens for publishing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRequirement {
    pub name: String,
    pub label: String,
    pub description: String,
    #[serde(default = "default_true")]
    pub is_required: bool,
}

fn default_true() -> bool {
    true
}

/// Dependencies that a compound agent requires or optionally uses
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDependencies {
    /// Agents that must be present for the compound agent to function
    #[serde(default)]
    pub required: Vec<String>,
    /// Agents that enhance functionality but aren't strictly needed
    #[serde(default)]
    pub optional: Vec<String>,
}

/// Agent tier (curated, community, or system)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentTier {
    Curated,
    Community,
    System,
}

impl std::fmt::Display for AgentTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentTier::Curated => write!(f, "curated"),
            AgentTier::Community => write!(f, "community"),
            AgentTier::System => write!(f, "system"),
        }
    }
}

/// MCP tool descriptor.
///
/// **This describes a tool ABW itself implements** — the name is
/// resolved against the compile-time dispatch table in
/// `tools_legacy::ToolRegistry::execute`, and the entry doubles as the
/// allowlist for exposing that tool over `/mcp/agents/:id` (Fermi acting
/// as an MCP *server*).
///
/// It carries no endpoint. To let an agent *consume* tools from a remote
/// MCP server, use [`AgentCapabilities::mcp_servers`] instead. Declaring
/// a name here with no corresponding dispatch arm produces a phantom
/// tool: the model is told it exists, calls it, and gets
/// `Unknown tool: X`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

// ─── Cognition tier (ADR-011) ──────────────────────────────────────

/// Cognitive bandwidth tier for creature-driven model selection.
///
/// **Declaration order determines `Ord`: `Local < Free < Standard < Premium`.**
///
/// `Local` is the topology Phase-0 addition (see
/// `docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md` §10.4.0). It is the
/// substrate-flexibility floor — a model_ladder rung tagged `tier: "local"`
/// is reachable only when a request opts into local execution explicitly
/// (e.g. via a cognition_tier override or an Ollama-hosted agent card).
/// Routing logic in `apply_tier_resolution()` walks the ladder by
/// `rung.tier <= request_tier`, so placing `Local` *below* `Free` means
/// free-tier users do not accidentally land on local models without explicit
/// opt-in via a per-agent ladder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum CognitionTier {
    Local,
    Free,
    Standard,
    Premium,
}

impl Default for CognitionTier {
    fn default() -> Self {
        CognitionTier::Free
    }
}

/// Orthogonal substrate-class signal: declares which class of provider an
/// agent (or capability) is willing to execute on.
///
/// This is intentionally separate from `CognitionTier` because they answer
/// different questions:
///   - `CognitionTier` — "how much cognitive bandwidth does the request budget?"
///   - `MinProviderClass` — "what *quality of substrate* does the agent require?"
///
/// Examples:
///   - A coherence evaluator that needs frontier reasoning declares
///     `min_provider_class: cloud_frontier`. Refusing to run on Local prevents
///     a low-quality output from being mistaken for a sound coherence verdict.
///   - A SimOps cascade agent that does deterministic arithmetic declares
///     `min_provider_class: local`. It will run anywhere.
///
/// Defaults to `CloudStandard` — the conservative middle. Authors who
/// genuinely don't care can leave the field off; authors who need frontier
/// must say so explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MinProviderClass {
    Local,
    CloudStandard,
    CloudFrontier,
}

impl Default for MinProviderClass {
    fn default() -> Self {
        MinProviderClass::CloudStandard
    }
}

/// One rung in an agent's model ladder — maps a tier to a specific model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRung {
    pub tier: CognitionTier,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub eval_score: Option<f64>,
    #[serde(default)]
    pub benchmarked_at: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Per-rung sampling overrides merged on top of agent-level model_params
    /// when this rung is selected by apply_tier_resolution().
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

fn default_min_tier() -> CognitionTier {
    CognitionTier::Free
}

// ─── Agent capabilities ────────────────────────────────────────────

/// Agent capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub executor: ExecutorType,
    /// Platform tools this agent declares. Resolved against the
    /// compile-time dispatch table; see [`McpTool`].
    #[serde(default)]
    pub mcp_tools: Vec<McpTool>,
    /// Remote MCP servers this agent is authorised to call.
    ///
    /// This is the *client* direction, and it is a general ABW
    /// capability: any card may declare any number of servers, their
    /// tools are discovered at runtime via `tools/list`, namespaced
    /// `server__tool`, and dispatched via `tools/call`. Adding a new
    /// third-party MCP endpoint to the platform is a card edit — no Rust
    /// changes and no new dispatch arm.
    ///
    /// Unlike builtin tools (which every agent currently receives), this
    /// list is a real per-agent capability boundary: an agent can reach a
    /// remote server only if its own card names it.
    ///
    /// Accepts both the ecosystem-standard map form (`{"name": {...}}`,
    /// as used by Claude Desktop/Cursor and by cards already in this
    /// repo) and a sequence. See
    /// `mcp_client::deserialize_mcp_servers`.
    #[serde(
        default,
        deserialize_with = "crate::agent_backend::mcp_client::deserialize_mcp_servers"
    )]
    pub mcp_servers: Vec<crate::agent_backend::mcp_client::RemoteMcpServer>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub model: String,
    pub temperature: f64,
    #[serde(default = "default_provider")]
    pub provider: String,

    // ── ADR-011: Cognition economy ──────────────────────────────────
    /// Ordered list of (tier → model) mappings. `model`/`provider` above
    /// remain the effective runtime fields; the ladder is used when a
    /// creature's cognition_tier is known.
    #[serde(default)]
    pub model_ladder: Vec<ModelRung>,
    /// The lowest tier this agent will accept — requests below this fail gracefully.
    #[serde(default = "default_min_tier")]
    pub min_tier: CognitionTier,
    /// Feature gates: capability name → minimum cognition tier required.
    /// Used by the platform to gate access to expensive capabilities.
    ///
    /// Note: the well-known key `min_provider_class` was historically
    /// authored here as a stringly-typed alias. `AgentCard::from_json`
    /// hoists that legacy key into the typed `min_provider_class` field
    /// below before deserialisation, so this map only ever contains
    /// cognition-tier gates by the time it reaches user code.
    #[serde(default)]
    pub capability_gates: HashMap<String, CognitionTier>,
    /// Substrate-class floor — `local`, `cloud_standard` (default), or
    /// `cloud_frontier`. Orthogonal to `min_tier`. See `MinProviderClass`
    /// doc.
    ///
    /// Cards may also declare this under
    /// `capability_gates["min_provider_class"]` (the legacy authoring
    /// pattern from the topology Phase-0 draft). `AgentCard::from_json`
    /// normalises that into this field automatically.
    #[serde(default)]
    pub min_provider_class: MinProviderClass,

    // ── CEP: Calibrated Evidence Protocol ──────────────────────────
    /// Structured probabilistic reasoning contract for fermi-orchestra agents.
    #[serde(default)]
    pub fermi_contract: Option<FermiContract>,

    /// Domain output contract — the typed schema every member of a
    /// domain-constrained MoE must produce.
    ///
    /// This generalises `fermi_contract` to arbitrary domains. Where
    /// `fermi_contract` is forecasting-specific (finding_labels, multiplier_range,
    /// p50/p5/p95), `output_contract` is domain-agnostic. The orchestrator agent
    /// declares what it expects from members; member agents declare what they
    /// produce against this contract.
    ///
    /// Shape:
    /// ```json
    /// {
    ///   "domain": "process_optimisation",      // human-readable domain name
    ///   "produces": ["risk-assessment"],        // semantic labels (mirrors agent.produces)
    ///   "schema": { ... },                      // JSON Schema for the output document
    ///   "calibration": {                        // how to evaluate correctness over time
    ///     "signal": "sosa_observation" | "hitl_review" | "brier_forecast" | "user_rating",
    ///     "observable_property": "...",         // for sosa_observation
    ///     "resolution_delay_hours": 72,         // how long before ground truth arrives
    ///     "comparison": "continuous_mse" | "binary_accuracy" | "brier_score" | "max_risk"
    ///   },
    ///   "synthesis": "aggregation" | "pipeline" | "selection" | "max_risk" | "cep_weighted"
    /// }
    /// ```
    ///
    /// For Fermi: domain="forecasting", calibration.signal="brier_forecast",
    ///            synthesis="cep_weighted". fermi_contract holds the finding_labels
    ///            and multiplier details; output_contract holds the calibration spec.
    #[serde(default)]
    pub output_contract: Option<serde_json::Value>,

    /// Input contract — what callers must send to invoke this agent.
    ///
    /// Symmetric to `output_contract`. Compiled from `input_contract.sketch.json`
    /// by the `input-contract-sketch` binary. Shape:
    ///
    /// ```json
    /// {
    ///   "accepts_schema": "scro/bom-query/1",
    ///   "title": "BOM pricing request",
    ///   "required": ["task", "bom_items"],
    ///   "schema": {
    ///     "$id": "scro/bom-query/1",
    ///     "type": "object",
    ///     "required": ["task", "bom_items"],
    ///     "properties": { ... }
    ///   }
    /// }
    /// ```
    ///
    /// Unlike `output_contract`, no grounding map is needed — there is no
    /// provenance claim to make about where the *caller* sourced their data.
    ///
    /// Enforcement: a symmetric `envelope::validate_input` call (Phase C) checks
    /// the caller's query against this schema before dispatch. Absence means no
    /// input validation — a soft warning, not a hard block — preserving backward
    /// compatibility with untyped callers.
    ///
    /// Discovery: `list_workspace_agents` returns `input_schema_id` (the
    /// `accepts_schema` value) so strategist agents can route by schema ID
    /// rather than description heuristics.
    #[serde(default)]
    pub input_contract: Option<serde_json::Value>,

    /// Competition declaration — how this agent participates in open-slot
    /// selection within a coordination graph.
    ///
    /// A specialist that declares an `input_contract` and wants to be a
    /// candidate for `select_agent` also fills this block. Authors declare
    /// `domains`, `price_credits_per_call`, and `support_tier`. The platform
    /// computes `fidelity` (Gate::OutputSchema history) and `selection_rate`
    /// (select_agent decisions) — those are never self-declared.
    ///
    /// `None` means the agent is still callable by name but does not appear
    /// in `select_agent` results unless its `input_schema_id` matches.
    #[serde(default)]
    pub competition: Option<CompetitionDeclaration>,

    /// Provider-agnostic sampling configuration. Keys override the legacy
    /// `temperature` field and add provider-specific params (top_p, top_k,
    /// extended_thinking, thinking_budget_tokens, frequency_penalty, etc.).
    /// `apply_tier_resolution()` merges the selected rung's `params` on top.
    #[serde(default = "default_json_object")]
    pub model_params: serde_json::Value,
}

/// How an agent participates in open-slot selection within a coordination graph.
///
/// Authors fill this on their agent card. The platform reads it in `select_agent`
/// alongside platform-computed signals (fidelity, Brier, selection rate) that
/// cannot be self-declared.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetitionDeclaration {
    /// Domain tags this agent competes in.
    /// e.g. ["supply-chain", "bom-pricing"]. Used for domain-match scoring.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Platform credits charged per successful `execute_agent` call.
    /// `None` = free (the default for curated platform agents).
    #[serde(default)]
    pub price_credits_per_call: Option<u32>,
    /// Owner's support commitment. Self-declared; unverified initially.
    /// Values: "community" (default) | "standard" | "enterprise".
    #[serde(default = "default_competition_support_tier")]
    pub support_tier: String,
}

fn default_competition_support_tier() -> String {
    "community".to_string()
}

/// CEP finding labels an orchestra agent is expected to emit.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FermiContract {
    /// Labels this agent uses in key_findings (e.g. ["BASE RATE", "TRIAL DATA", "MULTIPLIER"]).
    #[serde(default)]
    pub finding_labels: Vec<String>,
    /// Valid range for multiplier suggestions [min, max].
    pub multiplier_range: Option<[f64; 2]>,
    /// KG fact categories this agent maintains (e.g. ["base_rate", "designation_multiplier"]).
    #[serde(default)]
    pub kg_fact_categories: Vec<String>,
    /// Seed facts to populate the KG on first run.
    #[serde(default)]
    pub seed_facts: Vec<CepSeedFact>,
}

/// A single seed fact for CEP KG initialisation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CepSeedFact {
    pub entity_type: String,
    pub name: String,
    pub description: String,
    pub properties: serde_json::Value,
    pub confidence: f64,
}

fn default_provider() -> String {
    "anthropic".to_string()
}

fn default_json_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// Resolved sampling parameters — single source of truth for all 5 request
/// construction sites in the executor pipeline (llm_executor, multi_model_executor,
/// tool_executor ×2). Produced by `AgentCapabilities::resolve_sampling_params()`.
#[derive(Debug, Clone)]
pub struct SamplingParams {
    pub temperature: Option<f64>,
    pub max_tokens: u32,
    pub top_p: Option<f64>,
    pub top_k: Option<i32>,
    pub extended_thinking: bool,
    pub thinking_budget_tokens: Option<u32>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub repetition_penalty: Option<f64>,
    pub random_seed: Option<u32>,
}

impl AgentCapabilities {
    /// Resolve the best (provider, model) for the given tier and patch self in place.
    ///
    /// Algorithm (from ADR-011):
    ///   1. Find the highest rung whose tier ≤ requested tier
    ///   2. If found, overwrite model + provider and merge rung params into model_params
    ///   3. If no matching rung exists, leave defaults unchanged
    pub fn apply_tier_resolution(&mut self, tier: &CognitionTier) {
        if self.model_ladder.is_empty() {
            return;
        }
        // Extract needed data before taking a mutable borrow on self
        let best = self
            .model_ladder
            .iter()
            .filter(|r| &r.tier <= tier)
            .max_by(|a, b| a.tier.cmp(&b.tier))
            .map(|r| (r.model.clone(), r.provider.clone(), r.params.clone()));

        if let Some((model, provider, rung_params)) = best {
            self.model = model;
            self.provider = provider;
            // Merge rung-level params on top of agent-level model_params
            if let Some(rp) = rung_params {
                if let (serde_json::Value::Object(base), serde_json::Value::Object(overrides)) =
                    (&mut self.model_params, rp)
                {
                    for (k, v) in overrides {
                        base.insert(k, v);
                    }
                }
            }
        }
    }

    /// Produce resolved sampling parameters for one LLM request.
    ///
    /// Priority: model_params JSONB keys > legacy `temperature` f64 field.
    /// Extended thinking forces temperature = 1.0 (Anthropic requirement).
    pub fn resolve_sampling_params(&self, default_max_tokens: u32) -> SamplingParams {
        let p = &self.model_params;

        let extended_thinking = p
            .get("extended_thinking")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let temperature = if extended_thinking {
            Some(1.0)
        } else {
            p.get("temperature")
                .and_then(|v| v.as_f64())
                .or(Some(self.temperature))
        };

        SamplingParams {
            temperature,
            max_tokens: p
                .get("max_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(default_max_tokens),
            top_p: p.get("top_p").and_then(|v| v.as_f64()),
            top_k: p.get("top_k").and_then(|v| v.as_i64()).map(|v| v as i32),
            extended_thinking,
            thinking_budget_tokens: p
                .get("thinking_budget_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            frequency_penalty: p.get("frequency_penalty").and_then(|v| v.as_f64()),
            presence_penalty: p.get("presence_penalty").and_then(|v| v.as_f64()),
            repetition_penalty: p.get("repetition_penalty").and_then(|v| v.as_f64()),
            random_seed: p
                .get("random_seed")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        }
    }
}

/// Agent performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentPerformance {
    #[serde(default)]
    pub forecasts_contributed: u32,
    #[serde(default)]
    pub avg_brier_impact: f64,
    #[serde(default)]
    pub avg_confidence: f64,
    #[serde(default)]
    pub accuracy_rate: f64,
    #[serde(default)]
    pub total_queries: u32,
}

/// Agent usage and cost tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentUsage {
    pub total_executions: u32,
    pub successful_executions: u32,
    pub failed_executions: u32,
    pub total_tokens_used: u64,
    pub total_cost_usd: f64,
    pub avg_execution_time_ms: u64,
    pub last_30_days: UsageWindow,
}

/// Rolling window of usage stats
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageWindow {
    pub executions: u32,
    pub tokens: u64,
    pub cost_usd: f64,
}

/// Agent wallet — flexible structure for future revenue model
pub type AgentWallet = serde_json::Value;

/// Ontology statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyStats {
    #[serde(default)]
    pub entities: u32,
    #[serde(default)]
    pub relationships: u32,
    #[serde(default = "default_datetime")]
    pub last_updated: DateTime<Utc>,
    #[serde(default)]
    pub evolution_commits: u32,
}

fn default_datetime() -> DateTime<Utc> {
    chrono::DateTime::UNIX_EPOCH
}

impl Default for OntologyStats {
    fn default() -> Self {
        Self {
            entities: 0,
            relationships: 0,
            last_updated: default_datetime(),
            evolution_commits: 0,
        }
    }
}

/// Agent valence — affective signature for personality and interaction style
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentValence {
    pub primary_affect: String,
    pub arousal: f64,
    pub valence: f64,
    pub personality_traits: Vec<String>,
}

/// Agent metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub created: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub sample_queries: Vec<String>,
    #[serde(default)]
    pub valence: Option<AgentValence>,
    /// Seven-rank classification (SPEC_30). Kept as a raw JSON map rather
    /// than a struct because the rank set is a modelling decision that has
    /// already changed once — SPEC_30 reformed four of the seven — and a
    /// typed struct would turn the next reform into a breaking change across
    /// every card. Read by the seeder so a card's EDITORIAL ranks
    /// (kingdom/family/genus, which need a human) reach `agents.taxonomy`;
    /// derived ranks are always recomputed rather than trusted.
    #[serde(default)]
    pub taxonomy: Option<serde_json::Value>,
}

impl AgentCard {
    /// Record the hash of the prompt as declared, before anything appends to it.
    ///
    /// Call this at the moment a card's `system_prompt` is settled from its
    /// source of truth and BEFORE any enrichment. Calling it twice is safe;
    /// calling it after `kg_context::enrich` records the wrong thing, which is
    /// the bug this exists to fix rather than a hazard it introduces.
    ///
    /// Idempotent in the sense that matters: it overwrites, so a card resolved
    /// twice ends up with the hash of whatever prompt it currently declares.
    pub fn stamp_declared_prompt(&mut self) {
        use sha2::{Digest, Sha256};
        self.declared_prompt_sha256 = self.system_prompt.as_deref().map(|p| {
            let mut h = Sha256::new();
            h.update(p.as_bytes());
            format!("{:x}", h.finalize())
        });
    }

    /// Create a new agent card with default values
    pub fn new(agent_id: String, agent_type: String) -> Self {
        AgentCard {
            agent_id,
            agent_type,
            version: "1.0.0".to_string(),
            tier: AgentTier::Curated,
            capabilities: AgentCapabilities {
                executor: ExecutorType::LLM,
                mcp_tools: vec![],
                mcp_servers: vec![],
                skills: vec![],
                model: "claude-haiku-4-5-20251001".to_string(),
                temperature: 0.3,
                provider: "anthropic".to_string(),
                model_ladder: vec![],
                min_tier: CognitionTier::Free,
                capability_gates: HashMap::new(),
                min_provider_class: MinProviderClass::default(),
                fermi_contract: None,
                output_contract: None,
                input_contract: None,
                competition: None,
                model_params: serde_json::Value::Object(serde_json::Map::new()),
            },
            performance: AgentPerformance {
                forecasts_contributed: 0,
                avg_brier_impact: 0.0,
                avg_confidence: 0.0,
                accuracy_rate: 0.0,
                total_queries: 0,
            },
            usage: AgentUsage {
                total_executions: 0,
                successful_executions: 0,
                failed_executions: 0,
                total_tokens_used: 0,
                total_cost_usd: 0.0,
                avg_execution_time_ms: 0,
                last_30_days: UsageWindow {
                    executions: 0,
                    tokens: 0,
                    cost_usd: 0.0,
                },
            },
            wallet: None,
            ontology_stats: OntologyStats {
                entities: 0,
                relationships: 0,
                last_updated: Utc::now(),
                evolution_commits: 0,
            },
            metadata: AgentMetadata {
                created: Utc::now().to_rfc3339(),
                author: "Fermi Team".to_string(),
                description: "Agent description".to_string(),
                tags: vec![],
                sample_queries: vec![],
                valence: None,
                taxonomy: None,
            },
            system_prompt: None,
            declared_prompt_sha256: None,
            dependencies: AgentDependencies::default(),
            accepts: vec![],
            produces: vec![],
            workflow_template: None,
            prompt_template: None,
            requires_secrets: vec![],
        }
    }

    /// Load agent card from JSON string.
    ///
    /// Performs an in-place legacy-key normalisation before deserialisation
    /// so cards authored against earlier draft schemas continue to load:
    ///
    /// * `capabilities.capability_gates.min_provider_class` (a stringly-typed
    ///   gate value) is hoisted to the typed `capabilities.min_provider_class`
    ///   field and removed from the gates map. This is the topology Phase-0
    ///   draft pattern — kept compatible so existing cards
    ///   (`simops_companion`, `simops_narrator_local`) keep loading without
    ///   an out-of-tree migration.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        // Parse as a generic Value first so we can rewrite legacy keys.
        let mut raw: serde_json::Value = serde_json::from_str(json)?;
        normalise_legacy_capability_fields(&mut raw);
        let mut card: Self = serde_json::from_value(raw)?;
        // Stamp on load, so the hash does not depend on which handler resolved
        // the card.
        //
        // `resolve_agent_card` also stamps, after overriding the prompt from the
        // database, and that is the authoritative one for HTTP execution. But two
        // paths build an `ExecutionContext` without ever calling it —
        // `tools_legacy.rs` (the `execute_agent` / `delegate_to_agent` tools) and
        // `consolidation.rs` — and a delegated child agent is exactly the run
        // whose provenance matters most. Stamping at deserialisation means those
        // paths get a correct hash rather than `None`, and `None` would have been
        // invisible: it renders as INERT, which looks like an agent that has not
        // run yet.
        card.stamp_declared_prompt();
        Ok(card)
    }

    /// Save agent card to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

// ─── Legacy-shape normalisation ──────────────────────────────────────────────
//
// Cards authored against the topology Phase-0 draft put a stringly-typed
// `min_provider_class` inside `capability_gates`. The typed schema now lifts
// that to a sibling field with its own enum. This function rewrites the
// older shape into the new one before deserialisation, so the on-disk cards
// keep loading verbatim.
//
// Pure JSON-Value manipulation — no serde derives in the loop. Defensive
// against any of the nested keys being absent.

fn normalise_legacy_capability_fields(raw: &mut serde_json::Value) {
    let caps = match raw.get_mut("capabilities").and_then(|c| c.as_object_mut()) {
        Some(c) => c,
        None => return, // No capabilities block — nothing to normalise
    };

    // Skip if the typed field is already present at the top level.
    let typed_present = caps.contains_key("min_provider_class");

    let legacy_value: Option<serde_json::Value> = {
        let gates = caps
            .get_mut("capability_gates")
            .and_then(|g| g.as_object_mut());
        match gates {
            Some(g) => g.remove("min_provider_class"),
            None => None,
        }
    };

    if let Some(value) = legacy_value {
        if !typed_present {
            caps.insert("min_provider_class".to_string(), value);
        }
        // If both forms were present (typed wins), we still removed the
        // legacy key from the gates map above — that's the intended cleanup.
    }
}

#[cfg(test)]
mod declared_prompt_tests {
    use super::*;

    fn card_with(prompt: Option<&str>) -> AgentCard {
        let mut c = AgentCard::new("weather_oracle".into(), "research".into());
        c.system_prompt = prompt.map(str::to_string);
        c.stamp_declared_prompt();
        c
    }

    /// The regression. Enrichment must not move the declared hash.
    ///
    /// `kg_context::enrich` appends a retrieved-knowledge block to
    /// `card.system_prompt` before execution — literally
    /// `format!("{}{}", system_prompt, block)`, reproduced here because that
    /// function is private. The first version of the prompt hash was computed
    /// downstream of this, so it hashed card-plus-knowledge and changed on every
    /// run whose retrieval differed.
    ///
    /// Two different blocks, because a test with one block would pass for an
    /// implementation that merely ignored a CONSTANT suffix.
    #[test]
    fn a_retrieved_knowledge_block_does_not_change_the_declared_hash() {
        let base = "You are the Weather Oracle.";
        let mut card = card_with(Some(base));
        let declared = card.declared_prompt_sha256.clone().expect("stamped");

        for block in [
            "\n\n## Recalled\n- EGLC lead-1 RMSE is 0.909C\n",
            "\n\n## Recalled\n- KLGA lead-1 RMSE is 1.49C\n- buckets are integer sets\n",
        ] {
            card.system_prompt = Some(format!("{}{}", card.system_prompt.take().unwrap(), block));
            assert_eq!(
                card.declared_prompt_sha256.as_deref(),
                Some(declared.as_str()),
                "appending retrieved knowledge must not move the declared hash — \
                 that is what made five runs of one card produce four hashes"
            );
        }

        // ...and the effective prompt really did change, so the two hashes are
        // measuring different things rather than the test proving nothing.
        let mut effective = card.clone();
        effective.stamp_declared_prompt();
        assert_ne!(
            effective.declared_prompt_sha256.as_deref(),
            Some(declared.as_str()),
            "re-stamping an enriched prompt should differ; if it does not, this \
             test cannot detect the bug it exists for"
        );
    }

    /// Same text, same hash, regardless of which card carries it — the property
    /// the SQL side depends on when it compares against `agents.system_prompt`.
    #[test]
    fn the_hash_is_of_the_text_and_nothing_else() {
        let a = card_with(Some("identical prompt"));
        let mut b = AgentCard::new("other_agent".into(), "creative".into());
        b.system_prompt = Some("identical prompt".into());
        b.stamp_declared_prompt();
        assert_eq!(a.declared_prompt_sha256, b.declared_prompt_sha256);

        // Known-answer, so a change of algorithm is caught rather than merely
        // producing a different-but-self-consistent value. This is the digest
        // Postgres returns for `encode(sha256(convert_to('abc','UTF8')),'hex')`.
        let abc = card_with(Some("abc"));
        assert_eq!(
            abc.declared_prompt_sha256.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
            "must stay SHA-256 hex: the cohort predicate compares against \
             Postgres sha256() and a silent algorithm change matches nothing"
        );
    }

    /// No prompt means no hash, not a hash of the empty string.
    ///
    /// An empty-string digest is a real, matchable value, so it would let every
    /// promptless card share one cohort.
    #[test]
    fn a_card_with_no_prompt_has_no_declared_hash() {
        assert!(card_with(None).declared_prompt_sha256.is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;

    /// Resolve the agents/curated directory relative to the workspace root.
    /// `cargo test` runs with cwd = package root, but we need the workspace root.
    fn curated_dir() -> std::path::PathBuf {
        // Try workspace root first (when run from repo root)
        let candidates = [
            Path::new("agents/curated"),
            Path::new("../../agents/curated"), // from nested crate
        ];
        for c in &candidates {
            if c.exists() {
                return c.to_path_buf();
            }
        }
        panic!(
            "Cannot find agents/curated directory. Run tests from the workspace root: \
             cargo test --lib -p fermi agent_card::tests"
        );
    }

    /// Load all agent cards from agents/curated/*/agent_card.json
    fn load_all_cards() -> Vec<(String, AgentCard)> {
        let dir = curated_dir();
        let mut cards = Vec::new();
        for entry in fs::read_dir(&dir).expect("Failed to read curated dir") {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                let card_path = path.join("agent_card.json");
                let dir_name = path.file_name().unwrap().to_string_lossy().to_string();
                if card_path.exists() {
                    let json = fs::read_to_string(&card_path).unwrap_or_else(|e| {
                        panic!("Failed to read {}: {}", card_path.display(), e)
                    });
                    let card: AgentCard = AgentCard::from_json(&json).unwrap_or_else(|e| {
                        panic!("Failed to deserialize {}: {}", card_path.display(), e)
                    });
                    cards.push((dir_name, card));
                } else {
                    panic!("Agent directory '{}' has no agent_card.json", dir_name);
                }
            }
        }
        assert!(!cards.is_empty(), "No agent cards found");
        cards
    }

    #[test]
    fn test_agent_card_creation() {
        let card = AgentCard::new("test_agent".to_string(), "research".to_string());
        assert_eq!(card.agent_id, "test_agent");
        assert_eq!(card.agent_type, "research");
        assert_eq!(card.tier, AgentTier::Curated);
    }

    #[test]
    fn test_agent_card_serialization() {
        let card = AgentCard::new("test_agent".to_string(), "research".to_string());
        let json = card.to_json().unwrap();
        let deserialized = AgentCard::from_json(&json).unwrap();
        assert_eq!(card.agent_id, deserialized.agent_id);
    }

    // --- Conformance regression tests ---

    #[test]
    fn test_all_curated_agents_have_valid_cards() {
        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            assert_eq!(
                &card.agent_id, dir_name,
                "agent_id '{}' does not match directory name '{}'",
                card.agent_id, dir_name
            );
            assert!(!card.agent_id.is_empty(), "Empty agent_id in {}", dir_name);
            assert!(
                !card.agent_type.is_empty(),
                "Empty agent_type in {}",
                dir_name
            );
        }
        println!("Validated {} agent cards", cards.len());
    }

    /// Every curated card must satisfy the shared ABW agent contract —
    /// the same requirement set the publish gate applies to
    /// API-authored agents (`workflows::agent_contract`).
    ///
    /// Sharing the definition is the point. This test and
    /// `run_publish_checks` used to encode "well-formed" separately, so
    /// the on-disk path enforced sample_queries and valence while the API
    /// path did not — which is how community agents reached the public
    /// catalogue with neither.
    #[test]
    fn test_all_cards_satisfy_agent_contract() {
        use crate::workflows::agent_contract::{contract_violations, ContractView};

        let cards = load_all_cards();
        let mut failures: Vec<String> = Vec::new();

        for (dir_name, card) in &cards {
            for v in contract_violations(&ContractView::from(card)) {
                failures.push(format!("{dir_name}: {} — {}", v.check, v.message));
            }
        }

        assert!(
            failures.is_empty(),
            "{} curated card(s) violate the agent contract:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    /// Card-only requirements — fields that exist on `AgentCard` but have
    /// no counterpart on the `agents` row, so they cannot live in the
    /// shared contract.
    #[test]
    fn test_all_cards_have_card_specific_fields() {
        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            // A card whose description is the generated placeholder is
            // worse than one with no description: it looks filled in.
            assert!(
                !card.metadata.description.starts_with("Agent: "),
                "{}: metadata.description is still the default placeholder",
                dir_name
            );
            // Wallet is a card concept; DB agents fund via agent_wallets.
            assert!(card.wallet.is_some(), "{}: wallet is missing", dir_name);
        }
    }

    #[test]
    fn test_all_cards_have_tools_as_objects() {
        // Deserialization into Vec<McpTool> enforces object format.
        // If any card had flat strings, load_all_cards() would panic.
        // This test explicitly confirms all cards load successfully.
        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            for tool in &card.capabilities.mcp_tools {
                assert!(
                    !tool.name.is_empty(),
                    "{}: mcp_tool has empty name",
                    dir_name
                );
            }
        }
    }

    #[test]
    fn test_all_cards_have_dependencies() {
        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            // dependencies field exists (deserialized with Default)
            // Just verify it's structurally sound
            let _ = &card.dependencies.required;
            let _ = &card.dependencies.optional;
            // Compound agents with deps should not have empty required+optional
            // (but single agents can have both empty — that's fine)
            let _ = dir_name; // used in assertion context
        }
    }

    #[test]
    fn test_compound_agents_have_execute_agent_tool() {
        // Cards that have dependencies declared but are missing execute_agent/delegate_to_agent.
        // These are pre-existing gaps in domain research agent cards — tracked for fix in a
        // follow-up card-authoring pass. New cards must NOT be added to this list.
        let pre_existing_gaps: HashSet<&str> = [
            "adc_pk_oracle",
            "biotech_analyst",
            "enemy_sensor",
            "entity_investigator",
            "equity_analyst",
            "fermi",
            "football_analyst",
            "genome_profiler",
            "macro_forecaster",
            "market_research",
            "nba_analyst",
            "sentiment_analyzer",
            "simops_advisor",
            "simops_cascade",
            "simops_narrator",
            "simops_optimizer",
            "simops_predictor",
            "supply_chain_oracle",
        ]
        .into_iter()
        .collect();

        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            if pre_existing_gaps.contains(dir_name.as_str()) {
                continue; // pre-existing gap — tracked separately
            }
            let has_deps =
                !card.dependencies.required.is_empty() || !card.dependencies.optional.is_empty();
            if has_deps {
                let has_execute = card
                    .capabilities
                    .mcp_tools
                    .iter()
                    .any(|t| t.name == "execute_agent" || t.name == "delegate_to_agent");
                assert!(
                    has_execute,
                    "{}: compound agent (has dependencies) but no execute_agent or delegate_to_agent tool",
                    dir_name
                );
            }
        }
    }

    #[test]
    fn test_all_agents_registered_with_xaman_ek() {
        let dir = curated_dir();
        let xaman_path = dir.join("xaman_ek/agent_card.json");
        let json = fs::read_to_string(&xaman_path).expect("Failed to read xaman_ek card");
        let xaman: AgentCard = AgentCard::from_json(&json).expect("Failed to parse xaman_ek card");
        let prompt = xaman.system_prompt.expect("xaman_ek has no system_prompt");

        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            if card.agent_id == "xaman_ek" {
                continue; // Xaman Ek doesn't need to list itself
            }
            assert!(
                prompt.contains(&format!("**{}**", card.agent_id)),
                "{}: agent is not registered in Xaman Ek's system prompt \
                 (expected '**{}**' to appear)",
                dir_name,
                card.agent_id
            );
        }
    }

    #[test]
    fn test_skill_registry_completeness() {
        // All skill names in the SkillRegistry must be non-empty and unique.
        let names = crate::agent_backend::tools::SkillRegistry::names();
        assert!(!names.is_empty(), "SkillRegistry is empty");
        let unique: HashSet<&&str> = names.iter().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "Duplicate skill names in SkillRegistry: {:?}",
            names
        );
        println!("SkillRegistry has {} skills: {:?}", names.len(), names);
    }

    #[test]
    fn test_skill_registry_covers_executable_skills() {
        // The SkillRegistry covers the EXECUTABLE deterministic skills —
        // pure-function capabilities the platform dispatches at runtime.
        //
        // Agent cards also use `capabilities.skills` as a TAXONOMY field:
        // free-text domain labels like "market-analysis", "coherence-analysis",
        // "sentiment-detection" that describe what the agent does for
        // discovery (xamanEK reads them) but are not dispatched as functions.
        //
        // This test verifies that every name in SkillRegistry::names() is
        // unique and that the registry is non-empty — it does NOT enforce
        // that all card skill labels must be in the registry, because the
        // taxonomy labels and executable skills serve different purposes.
        //
        // See: docs/AGENT_MODEL.md §1.2, docs/STATE_OF_PROJECT.md §3

        let names = crate::agent_backend::tools::SkillRegistry::names();
        assert!(!names.is_empty(), "SkillRegistry must not be empty");

        // Every executable skill in the registry must be findable by name
        for name in &names {
            assert!(
                crate::agent_backend::tools::SkillRegistry::find(name).is_some(),
                "SkillRegistry::find('{}') returned None — find() is broken",
                name
            );
        }

        println!(
            "SkillRegistry: {} executable skills registered: {:?}",
            names.len(),
            names
        );
    }

    #[test]
    fn test_no_duplicate_agent_ids() {
        let cards = load_all_cards();
        let mut seen = HashSet::new();
        for (dir_name, card) in &cards {
            assert!(
                seen.insert(card.agent_id.clone()),
                "Duplicate agent_id '{}' found in directory '{}'",
                card.agent_id,
                dir_name
            );
        }
    }

    #[test]
    fn test_all_migrations_registered() {
        // Every .sql file in migrations/ must be listed in run_migrations() in api_server.rs
        // This prevents the exact bug where migration files exist but aren't run on startup,
        // causing 500 errors when handlers reference columns that don't exist yet.

        // Intentionally unregistered migrations (deferred features not yet wired up)
        let allowlist: HashSet<&str> = [
            "048_fermi_notebooks.sql", // Deferred: notebook system
            "049_akp_foundation.sql",  // Deferred: AKP protocol
            // 126 is in-flight parallel-session work (agent_version_full_config);
            // not registered yet because its handler/loader counterparts haven't
            // landed. Allowlisted to keep this test green until the feature ships.
            "126_agent_version_full_config.sql",
        ]
        .into_iter()
        .collect();

        // Resolve migrations directory
        let candidates = [Path::new("migrations"), Path::new("../../migrations")];
        let migrations_dir = candidates
            .iter()
            .find(|c| c.exists())
            .expect("Cannot find migrations/ directory");

        // Resolve api_server.rs
        let server_candidates = [
            Path::new("src/api_server.rs"),
            Path::new("../../src/api_server.rs"),
        ];
        let server_path = server_candidates
            .iter()
            .find(|c| c.exists())
            .expect("Cannot find src/api_server.rs");

        let server_source = fs::read_to_string(server_path).expect("Failed to read api_server.rs");

        // Collect all .sql files (excluding rollbacks)
        let mut missing = Vec::new();
        for entry in fs::read_dir(migrations_dir).expect("Failed to read migrations dir") {
            let entry = entry.unwrap();
            let filename = entry.file_name().to_string_lossy().to_string();
            if !filename.ends_with(".sql") {
                continue;
            }
            if filename.starts_with("rollback") {
                continue;
            }
            if allowlist.contains(filename.as_str()) {
                continue;
            }
            let expected = format!("migrations/{}", filename);
            if !server_source.contains(&expected) {
                missing.push(filename);
            }
        }

        missing.sort();
        assert!(
            missing.is_empty(),
            "Migration files exist on disk but are NOT registered in run_migrations():\n  {}\n\
             Either add them to run_migrations() in api_server.rs, or add to the allowlist \
             in this test if intentionally deferred.",
            missing.join("\n  ")
        );
        println!(
            "All {} migration files are registered (+ {} in allowlist)",
            fs::read_dir(migrations_dir)
                .unwrap()
                .filter(|e| {
                    let f = e
                        .as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .to_string();
                    f.ends_with(".sql") && !f.starts_with("rollback")
                })
                .count()
                - allowlist.len(),
            allowlist.len()
        );
    }

    #[test]
    fn test_system_agents_have_system_tier() {
        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            if card.metadata.tags.contains(&"system".to_string()) {
                assert_eq!(
                    card.tier,
                    AgentTier::System,
                    "{}: tagged 'system' but tier is {:?}",
                    dir_name,
                    card.tier
                );
            }
        }
    }
}
