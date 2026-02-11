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
