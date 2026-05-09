use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Episode (episodic memory entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub episode_id: Uuid,
    pub agent_id: Uuid,
    pub timestamp_ref: DateTime<Utc>,
    pub query: String,
    pub context: serde_json::Value,
    pub execution_status: ExecutionStatus,
    pub error_details: Option<String>,
    pub execution_time_ms: i64,
    pub tokens_used: Option<i32>,
    pub cost_usd: Option<Decimal>,
    pub embedding: Option<Vec<f32>>,
    pub consolidated: bool,
    #[serde(default)]
    pub tags: Vec<String>,

    // ─── Phase 0 observability foundations (migration 103) ───
    /// Source of this episode — `auto_pass` (default), HITL outcomes,
    /// or `synthetic_correction` (HumanAuthority-weighted re-write).
    #[serde(default)]
    pub provenance: Provenance,
    /// 1.0 = HumanAuthority (max), <1.0 = lower-confidence sources.
    /// Default 0.5 = "automated default".
    #[serde(default = "default_authority_weight")]
    pub authority_weight: f64,
    /// Deterministic id of (agent, human) dyad. Wiring deferred per Q4
    /// — populated only by callers that explicitly know the human
    /// participant in the interaction.
    #[serde(default)]
    pub dyad_id: Option<String>,
    /// `agents.persona_version` at the time this episode was written.
    /// Drift monitor uses this to compare embeddings across versions.
    #[serde(default)]
    pub persona_version_at_write: Option<i32>,
}

fn default_authority_weight() -> f64 {
    0.5
}

/// Provenance of an episode — who/what produced it and at what authority.
///
/// See `migrations/103_observability_foundations.sql` for the matching DB
/// constraint. Default is `AutoPass` so existing automated runs keep their
/// current semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Evaluator registry passed the episode (no human review needed).
    AutoPass,
    /// Evaluator registry failed the episode (no human has seen it yet).
    AutoFail,
    /// HITL reviewer confirmed the evaluator verdict.
    HumanApproved,
    /// HITL reviewer corrected dimension scores only (no behavioural change).
    HumanRelabeled,
    /// HITL reviewer ran a full intervention — the *original* episode is
    /// flagged, the synthetic corrected episode is `SyntheticCorrection`.
    HumanCorrected,
    /// The second write of the intervention flow — synthetic corrected
    /// episode at HumanAuthority weight (1.0).
    SyntheticCorrection,
}

impl Default for Provenance {
    fn default() -> Self {
        Provenance::AutoPass
    }
}

impl std::fmt::Display for Provenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provenance::AutoPass => write!(f, "auto_pass"),
            Provenance::AutoFail => write!(f, "auto_fail"),
            Provenance::HumanApproved => write!(f, "human_approved"),
            Provenance::HumanRelabeled => write!(f, "human_relabeled"),
            Provenance::HumanCorrected => write!(f, "human_corrected"),
            Provenance::SyntheticCorrection => write!(f, "synthetic_correction"),
        }
    }
}

impl std::str::FromStr for Provenance {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto_pass" => Ok(Provenance::AutoPass),
            "auto_fail" => Ok(Provenance::AutoFail),
            "human_approved" => Ok(Provenance::HumanApproved),
            "human_relabeled" => Ok(Provenance::HumanRelabeled),
            "human_corrected" => Ok(Provenance::HumanCorrected),
            "synthetic_correction" => Ok(Provenance::SyntheticCorrection),
            _ => Err(format!("Invalid provenance: {}", s)),
        }
    }
}

impl Provenance {
    /// Whether this provenance represents a write originating from a
    /// human reviewer (used by the timeline filters in Phase 3+).
    pub fn is_human_originated(&self) -> bool {
        matches!(
            self,
            Provenance::HumanApproved
                | Provenance::HumanRelabeled
                | Provenance::HumanCorrected
                | Provenance::SyntheticCorrection
        )
    }

    /// HumanAuthority-weighted writes that should never be averaged
    /// down by lower-confidence subsequent observations.
    pub fn is_human_authority(&self) -> bool {
        matches!(
            self,
            Provenance::HumanCorrected | Provenance::SyntheticCorrection
        )
    }
}

/// HITL correction record — see `episode_corrections` table.
///
/// Append-only; mutating an existing row is rejected by a DB-level
/// trigger (see migration 103). The optional `synthetic_episode_id`
/// pointer is filled in by Phase 5 when the second write (synthetic
/// corrected episode) is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeCorrection {
    pub correction_id: Uuid,
    pub episode_id: Uuid,
    pub agent_id: Uuid,

    pub reviewer_id: String,
    pub reviewer_action: ReviewerAction,

    pub scope: CorrectionScope,
    pub classification: Option<CorrectionClassification>,

    pub dimension: Option<String>,
    pub correction_text: Option<String>,
    #[serde(default)]
    pub score_overrides: serde_json::Value,

    pub coherence_check: Option<serde_json::Value>,
    pub minimum_update_set: Option<serde_json::Value>,
    pub tensions_flagged: Option<serde_json::Value>,

    pub synthetic_episode_id: Option<Uuid>,

    pub authority_weight: f64,
    pub persona_version_bump: Option<i32>,

    pub justification: Option<String>,

    pub created_at: DateTime<Utc>,
}

/// Action a reviewer took in the HITL queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerAction {
    /// Confirm the evaluator verdict; no change to scores or behaviour.
    Approve,
    /// Correct dimension scores only.
    Relabel,
    /// Substantive correction — triggers the full intervention flow
    /// (coherence gate + two-write memory pattern).
    Intervene,
}

impl std::fmt::Display for ReviewerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewerAction::Approve => write!(f, "approve"),
            ReviewerAction::Relabel => write!(f, "relabel"),
            ReviewerAction::Intervene => write!(f, "intervene"),
        }
    }
}

impl std::str::FromStr for ReviewerAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "approve" => Ok(ReviewerAction::Approve),
            "relabel" => Ok(ReviewerAction::Relabel),
            "intervene" => Ok(ReviewerAction::Intervene),
            _ => Err(format!("Invalid reviewer action: {}", s)),
        }
    }
}

/// Scope of a correction — how broadly the corrective signal applies.
///
/// Per architecture doc step 2: `Episode` annotates the single record;
/// `Dyad` updates relationship state for one human–agent pair;
/// `AgentWide` updates persona baseline globally and requires two-reviewer
/// consensus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionScope {
    Episode,
    Dyad,
    AgentWide,
}

impl std::fmt::Display for CorrectionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorrectionScope::Episode => write!(f, "episode"),
            CorrectionScope::Dyad => write!(f, "dyad"),
            CorrectionScope::AgentWide => write!(f, "agent_wide"),
        }
    }
}

impl std::str::FromStr for CorrectionScope {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "episode" => Ok(CorrectionScope::Episode),
            "dyad" => Ok(CorrectionScope::Dyad),
            "agent_wide" => Ok(CorrectionScope::AgentWide),
            _ => Err(format!("Invalid correction scope: {}", s)),
        }
    }
}

/// Belief vs behavioural classification (architecture doc step 3).
///
/// `Belief` — agent held an incorrect belief; target = world model /
/// relationship beliefs / factual grounding.
/// `Behaviour` — agent acted wrongly despite correct beliefs; target =
/// action tendencies / response style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionClassification {
    Belief,
    Behaviour,
}

impl std::fmt::Display for CorrectionClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorrectionClassification::Belief => write!(f, "belief"),
            CorrectionClassification::Behaviour => write!(f, "behaviour"),
        }
    }
}

impl std::str::FromStr for CorrectionClassification {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "belief" => Ok(CorrectionClassification::Belief),
            "behaviour" => Ok(CorrectionClassification::Behaviour),
            _ => Err(format!("Invalid correction classification: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Success,
    Failure,
    Partial,
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionStatus::Success => write!(f, "success"),
            ExecutionStatus::Failure => write!(f, "failure"),
            ExecutionStatus::Partial => write!(f, "partial"),
        }
    }
}

impl std::str::FromStr for ExecutionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "success" => Ok(ExecutionStatus::Success),
            "failure" => Ok(ExecutionStatus::Failure),
            "partial" => Ok(ExecutionStatus::Partial),
            _ => Err(format!("Invalid execution status: {}", s)),
        }
    }
}

/// Semantic rule (consolidated knowledge)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticRule {
    pub rule_id: Uuid,
    pub agent_id: Uuid,
    pub rule_content: String,
    pub rule_description: Option<String>,
    pub confidence_score: f64,
    pub verification_status: VerificationStatus,
    pub verification_method: Option<String>,
    pub source_episode_cluster: Vec<Uuid>,
    pub episode_count: i32,
    pub embedding: Option<Vec<f32>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Shopping preference profile (consumer side of embedding marketplace)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShoppingProfile {
    pub profile_id: Uuid,
    pub user_id: String,
    pub agent_id: Uuid,
    pub profile_name: String,
    pub composite_embedding: Option<Vec<f32>>,
    pub embedding_version: i32,
    pub episode_count: i32,
    #[serde(default)]
    pub category_tags: Vec<String>,
    pub price_sensitivity: Option<f64>,
    pub quality_bias: Option<f64>,
    pub brand_affinities: serde_json::Value,
    pub metadata: serde_json::Value,
    pub is_listed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Marketplace listing (profile listed for advertiser queries)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceListing {
    pub listing_id: Uuid,
    pub profile_id: Uuid,
    pub seller_id: String,
    pub price_credits: i32,
    pub max_queries_per_buyer: Option<i32>,
    pub total_queries: i32,
    pub total_earned: i32,
    pub status: String,
    #[serde(default)]
    pub category_tags: Vec<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Marketplace transaction (record of a match query)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceTransaction {
    pub tx_id: Uuid,
    pub listing_id: Uuid,
    pub buyer_id: String,
    pub seller_id: String,
    pub similarity_score: f64,
    pub product_embedding_hash: Option<String>,
    pub credits_charged: i32,
    pub credits_to_seller: i32,
    pub platform_fee: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatus {
    Pending,
    Verified,
    Rejected,
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationStatus::Pending => write!(f, "pending"),
            VerificationStatus::Verified => write!(f, "verified"),
            VerificationStatus::Rejected => write!(f, "rejected"),
        }
    }
}

impl std::str::FromStr for VerificationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(VerificationStatus::Pending),
            "verified" => Ok(VerificationStatus::Verified),
            "rejected" => Ok(VerificationStatus::Rejected),
            _ => Err(format!("Invalid verification status: {}", s)),
        }
    }
}

/// Entity (knowledge graph node)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_id: Uuid,
    pub agent_id: Uuid,
    pub entity_name: String,
    pub entity_type: String,
    pub summary: Option<String>,
    pub t_valid: DateTime<Utc>,
    pub t_invalid: Option<DateTime<Utc>>,
    pub source_episodes: Vec<Uuid>,
    pub extraction_confidence: f64,
    pub embedding: Option<Vec<f32>>,
    /// Structured attributes. CEP seed entities (entity_type = "cep_*") use this
    /// to store numeric reference data: {n, source, year, area, ...}.
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
}

/// Fact (knowledge graph edge)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub fact_id: Uuid,
    pub agent_id: Uuid,
    pub source_entity_id: Uuid,
    pub target_entity_id: Uuid,
    pub relation_type: String,
    pub relation_cardinality: Cardinality,
    pub confidence: f64,
    pub reasoning: Option<String>,
    pub t_valid: DateTime<Utc>,
    pub t_invalid: Option<DateTime<Utc>>,
    pub source_episodes: Vec<Uuid>,
    /// Arbitrary payload for richer fact metadata (CEP, provenance, etc.).
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Cardinality {
    OneToOne,   // ||--||
    OneToMany,  // ||--o{
    ManyToOne,  // }o--||
    ManyToMany, // }o--o{
}

impl Cardinality {
    pub fn to_mermaid(&self) -> &'static str {
        match self {
            Cardinality::OneToOne => "||--||",
            Cardinality::OneToMany => "||--o{",
            Cardinality::ManyToOne => "}o--||",
            Cardinality::ManyToMany => "}o--o{",
        }
    }
}

impl std::fmt::Display for Cardinality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_mermaid())
    }
}

impl std::str::FromStr for Cardinality {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "||--||" => Ok(Cardinality::OneToOne),
            "||--o{" => Ok(Cardinality::OneToMany),
            "}o--||" => Ok(Cardinality::ManyToOne),
            "}o--o{" => Ok(Cardinality::ManyToMany),
            _ => Err(format!("Invalid cardinality: {}", s)),
        }
    }
}

/// Agent metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub agent_type: String,
    pub version: String,
    pub tier: String,
    pub executor_type: String,
    pub model: String,
    pub temperature: f64,
    pub mcp_servers: Option<serde_json::Value>, // Array of MCP server configs
    pub description: Option<String>,
    pub author: String,
    pub system_prompt: Option<String>,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    pub owner_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub current_ontology_commit: Option<String>,
    pub current_ontology_snapshot_id: Option<Uuid>,
    pub last_consolidated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub total_executions: i32,
    #[serde(default)]
    pub successful_executions: i32,
    #[serde(default)]
    pub failed_executions: i32,
    pub total_cost_usd: Option<rust_decimal::Decimal>,
    #[serde(default)]
    pub avg_execution_time_ms: i64,
    #[serde(default)]
    pub dreaming_budget_credits: i32,
    #[serde(default)]
    pub dreaming_credits_used: i32,
    pub dreaming_budget_reset_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub education_budget_credits: i32,
    #[serde(default)]
    pub education_credits_used: i32,
    #[serde(default)]
    pub auto_collect_pct: i32,
    pub display_alias: Option<String>,
    #[serde(default = "default_llm_provider")]
    pub llm_provider: String,
    #[serde(default = "default_embedding_provider")]
    pub embedding_provider: String,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_embedding_dimension")]
    pub embedding_dimension: i32,
    #[serde(default)]
    pub sample_queries: Vec<String>,
    #[serde(default = "default_status")]
    pub status: String,
    pub fork_pricing: Option<serde_json::Value>,
    pub forked_from: Option<Uuid>,
    #[serde(default)]
    pub fork_count: i32,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default)]
    pub produces: Vec<String>,
    pub workflow_template: Option<serde_json::Value>,
    pub prompt_template: Option<String>,
    pub requires_secrets: Option<serde_json::Value>,
    // ADR-011: cognition economy
    #[serde(default = "default_json_array")]
    pub model_ladder: serde_json::Value,
    #[serde(default = "default_free_tier")]
    pub min_tier: String,
    #[serde(default = "default_json_object")]
    pub capability_gates: serde_json::Value,
    // Phase 0 observability foundations (migration 103) —
    // monotonic counter bumped by AgentVersion writes (DB trigger) and
    // by agent-wide HITL interventions. Drift baseline.
    #[serde(default = "default_persona_version")]
    pub persona_version: i32,
    // CEP: structured probabilistic reasoning contract (migration 105)
    #[serde(default)]
    pub fermi_contract: Option<serde_json::Value>,
    // ADR-011 Phase 4: provider-agnostic sampling configuration (migration 106)
    #[serde(default = "default_json_object")]
    pub model_params: serde_json::Value,
}

fn default_persona_version() -> i32 {
    1
}

fn default_json_array() -> serde_json::Value {
    serde_json::Value::Array(vec![])
}
fn default_json_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}
fn default_free_tier() -> String {
    "free".to_string()
}

fn default_status() -> String {
    "draft".to_string()
}

fn default_llm_provider() -> String {
    "anthropic".to_string()
}
fn default_embedding_provider() -> String {
    "anthropic".to_string()
}
fn default_embedding_model() -> String {
    "voyage-2".to_string()
}
fn default_embedding_dimension() -> i32 {
    1024
}

fn default_visibility() -> String {
    "public".to_string()
}

/// Partial update for agents (all fields optional)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentUpdate {
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub visibility: Option<String>,
    pub tags: Option<Vec<String>>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub education_budget_credits: Option<i32>,
    pub display_alias: Option<String>,
    pub status: Option<String>,
    pub fork_pricing: Option<serde_json::Value>,
    pub accepts: Option<Vec<String>>,
    pub produces: Option<Vec<String>>,
    pub workflow_template: Option<serde_json::Value>,
    pub prompt_template: Option<String>,
    pub requires_secrets: Option<serde_json::Value>,
    pub llm_provider: Option<String>,
    // ADR-011: cognition economy
    pub model_ladder: Option<serde_json::Value>,
    pub min_tier: Option<String>,
    pub capability_gates: Option<serde_json::Value>,
    // ADR-011 Phase 4: provider-agnostic sampling config
    pub model_params: Option<serde_json::Value>,
}

/// Snapshot of mutable agent fields at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentVersion {
    pub version_id: Uuid,
    pub agent_id: Uuid,
    pub version_number: i32,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub tags: Vec<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub visibility: Option<String>,
    pub display_alias: Option<String>,
    pub changed_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Community (clustered group of entities)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub community_id: Uuid,
    pub agent_id: Uuid,
    pub community_name: Option<String>,
    pub summary: Option<String>,
    pub member_entity_ids: Vec<Uuid>,
    pub member_count: i32,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
}

/// Consolidation job record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationJob {
    pub job_id: Uuid,
    pub agent_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
    pub episode_range_start: Uuid,
    pub episode_range_end: Uuid,
    pub episodes_processed: i32,
    pub clusters_identified: i32,
    pub rules_extracted: i32,
    pub rules_verified: i32,
    pub rules_rejected: i32,
    pub entities_created: i32,
    pub facts_created: i32,
}

/// Coherence evaluation result for a workspace conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceEvaluation {
    pub eval_id: Uuid,
    pub workspace_id: Uuid,
    pub global_score: f64,
    pub quality_label: String,
    pub principle_scores: serde_json::Value,
    pub health_indicators: serde_json::Value,
    pub utterance_count: i32,
    pub message_window: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Eval test case for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTestCase {
    pub test_case_id: Uuid,
    pub agent_id: Uuid,
    pub query: String,
    pub expected_output: Option<String>,
    pub rubric: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Eval run (batch execution of test cases for an agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRun {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub triggered_by: String,
    pub status: String,
    pub judge_enabled: bool,
    pub total_cases: i32,
    pub passed: i32,
    pub failed: i32,
    pub avg_latency_ms: Option<i64>,
    pub avg_tokens: Option<i32>,
    pub avg_judge_score: Option<f64>,
    pub total_cost_credits: i32,
    pub case_results: serde_json::Value,
    pub regression_detected: bool,
    pub regression_details: Option<serde_json::Value>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    // Phase 2 — evaluator-registry aggregated outputs (migration 104)
    /// Serialized `AggregatedSignal` from the evaluator registry.
    /// Renders the per-dimension breakdown without re-aggregating
    /// from `eval_signals`. `None` for runs that pre-date Phase 2.
    #[serde(default)]
    pub aggregated_signal: Option<serde_json::Value>,
    /// Conflict flags from the registry's aggregator. Always an
    /// array (possibly empty). One entry per dimension where
    /// evaluators disagreed beyond the conflict threshold.
    #[serde(default = "default_json_array")]
    pub conflict_flags: serde_json::Value,
    /// True when the registry pre-filter short-circuited dimensional
    /// evaluators on this run (e.g. safety filter fired).
    #[serde(default)]
    pub prefilter_blocked: bool,
}

/// One per-evaluator, per-dimension scoring signal — see
/// migration 104 (`eval_signals` table). Phase 3 trend analyser
/// reads from here; Phase 4 HITL surfaces the dimension breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSignal {
    pub signal_id: Uuid,
    /// `None` when the registry was invoked outside the eval pipeline
    /// (Phase 3 longitudinal scoring path).
    pub run_id: Option<Uuid>,
    /// `None` when the underlying execution didn't store an episode.
    pub episode_id: Option<Uuid>,
    pub agent_id: Uuid,

    pub evaluator_name: String,
    pub evaluator_version: String,
    pub evaluator_tier: String, // 'pre_filter' | 'dimensional'

    pub dimension: String,
    pub score: f64,
    pub confidence: f64,

    pub flags: serde_json::Value,
    pub bundle_provenance: String,
    pub persona_version: Option<i32>,

    pub model_used: Option<String>,
    pub cost_credits: i32,
    pub latency_ms: i64,

    pub rationale: Option<String>,

    pub created_at: DateTime<Utc>,
}

// ─── Phase 3 — longitudinal observability (migration 105) ────────────

/// One row per scored episode in `agent_timeline_entries` — the
/// per-agent timeline that powers the observatory dashboard charts.
///
/// Mostly a denormalized projection of `(Episode, AggregatedSignal,
/// persona_version, dyad_id)` for fast chart reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub entry_id: Uuid,
    pub agent_id: Uuid,
    pub episode_id: Option<Uuid>,
    pub run_id: Option<Uuid>,

    pub persona_version: i32,
    pub dyad_id: Option<String>,
    pub session_id: Option<String>,

    pub provenance: String,

    /// Per-dimension means as `{ dim_name: f64 }`.
    pub dim_scores: serde_json::Value,

    /// Drift vector magnitude vs. the previous persona_version baseline.
    /// `None` when no prior baseline exists yet.
    pub drift_norm: Option<f64>,
    /// Cosine similarity vs. the rolling-mean embedding of the same
    /// persona_version (within-version cohesion).
    pub within_version_cosine: Option<f64>,

    pub anomaly_flags: serde_json::Value,

    pub created_at: DateTime<Utc>,
}

/// Per-(agent, human) running rapport / trust / reciprocity.
///
/// Phase 3 ships the schema and the running update math; values
/// stay scaffolding-quality until multi-turn workspace data flows in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DyadState {
    pub dyad_id: String,
    pub agent_id: Uuid,
    pub human_id: String,
    pub rapport: f64,
    pub trust: f64,
    pub reciprocity: f64,
    pub episode_count: i32,
    /// Bounded JSON array of recent rapport scores for rupture detection.
    pub recent_rapport: serde_json::Value,
    pub last_updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Anomaly event — drift / rolling_conflict / rupture / safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyEvent {
    pub event_id: Uuid,
    pub agent_id: Uuid,
    pub episode_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub dyad_id: Option<String>,
    pub kind: String,
    pub severity: String,
    pub payload: serde_json::Value,
    pub requires_review: bool,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// HITL action record — see `hitl_actions` table (Phase 4).
///
/// Append-only audit trail of reviewer decisions on anomaly events.
/// One row per reviewer-action; an anomaly may have multiple rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlAction {
    pub action_id: Uuid,
    pub anomaly_event_id: Uuid,
    pub agent_id: Uuid,
    pub reviewer_id: String,
    pub action: ReviewerAction,
    pub notes: Option<String>,
    pub score_overrides: serde_json::Value,
    pub correction_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Per-agent observability worker checkpoint — drives the Phase 3
/// hybrid scheduling model (timeline written inline, drift + anomaly
/// scanned in the background).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentObservabilityState {
    pub agent_id: Uuid,
    pub last_scanned_entry_id: Option<Uuid>,
    pub last_scan_started_at: Option<DateTime<Utc>>,
    pub last_scan_completed_at: Option<DateTime<Utc>>,
    pub last_scan_duration_ms: Option<i64>,
    pub timeline_entry_count: i32,
    pub anomaly_event_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Phase 5 — two-reviewer consensus (migration 108) ────────────────

/// Pending two-reviewer consensus request for `agent_wide` interventions.
///
/// Created by the first reviewer when they submit an `intervene` action
/// on an `agent_wide`-scope anomaly. The second reviewer must confirm
/// (via the same endpoint passing the `request_id`) before the coherence
/// gate + two-write memory pattern executes.
///
/// See `migrations/108_intervention_feedback_loop.sql`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwoReviewerRequest {
    pub request_id: Uuid,
    pub anomaly_event_id: Uuid,
    pub agent_id: Uuid,

    /// Full serialized `EncodedIntervention` so the second reviewer sees
    /// exactly what the first reviewer submitted.
    pub encoded_intervention: serde_json::Value,

    pub first_reviewer_id: String,
    pub first_reviewed_at: DateTime<Utc>,

    pub second_reviewer_id: Option<String>,
    pub second_reviewed_at: Option<DateTime<Utc>>,
    /// `None` = awaiting, `true` = approved, `false` = rejected.
    pub second_approved: Option<bool>,

    /// `pending` | `approved` | `rejected` | `expired`
    pub status: String,

    /// Populated after the two-write pattern executes.
    pub correction_id: Option<Uuid>,
    pub synthetic_episode_id: Option<Uuid>,

    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Workspace chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMessage {
    pub message_id: Uuid,
    pub workspace_id: Uuid,
    pub sender_type: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub content: String,
    pub message_type: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
