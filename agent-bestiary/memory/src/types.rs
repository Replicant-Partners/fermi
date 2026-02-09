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
