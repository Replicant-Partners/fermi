use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

/// Execution status for an episode
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "snake_case")]
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

/// Episodic memory - a single agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub episode_id: Option<Uuid>,
    pub agent_id: Uuid,

    // Temporal tracking
    pub timestamp_ref: DateTime<Utc>,
    pub timestamp_created: Option<DateTime<Utc>>,

    // Execution context
    pub query: String,
    pub context: serde_json::Value,
    pub execution_status: ExecutionStatus,
    pub error_details: Option<String>,

    // Execution metrics
    pub execution_time_ms: i64,
    pub tokens_used: Option<i32>,
    pub cost_usd: Option<rust_decimal::Decimal>,

    // Consolidation tracking
    pub consolidated: bool,
    pub consolidation_job_id: Option<Uuid>,
    pub cluster_id: Option<Uuid>,

    pub created_at: Option<DateTime<Utc>>,
}

impl Episode {
    pub fn new(
        agent_id: Uuid,
        query: String,
        context: serde_json::Value,
        execution_status: ExecutionStatus,
    ) -> Self {
        Self {
            episode_id: None,
            agent_id,
            timestamp_ref: Utc::now(),
            timestamp_created: None,
            query,
            context,
            execution_status,
            error_details: None,
            execution_time_ms: 0,
            tokens_used: None,
            cost_usd: None,
            consolidated: false,
            consolidation_job_id: None,
            cluster_id: None,
            created_at: None,
        }
    }
}

/// Verification status for semantic rules
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "snake_case")]
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

/// Semantic rule - consolidated knowledge from episodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticRule {
    pub rule_id: Option<Uuid>,
    pub agent_id: Uuid,

    // Rule content
    pub rule_content: String,
    pub rule_description: Option<String>,
    pub confidence_score: f32,

    // Verification tracking
    pub verification_status: VerificationStatus,
    pub verification_method: Option<String>,
    pub verification_details: Option<serde_json::Value>,

    // Source episodes
    pub source_episode_cluster: Vec<Uuid>,
    pub episode_count: i32,

    // Usage tracking
    pub created_at: Option<DateTime<Utc>>,
    pub last_validated_at: Option<DateTime<Utc>>,
    pub application_count: i32,
    pub successful_applications: i32,
    pub failed_applications: i32,

    // Invalidation
    pub is_active: bool,
    pub invalidated_at: Option<DateTime<Utc>>,
    pub invalidation_reason: Option<String>,
}

impl SemanticRule {
    pub fn new(
        agent_id: Uuid,
        rule_content: String,
        confidence_score: f32,
        source_episodes: Vec<Uuid>,
    ) -> Self {
        Self {
            rule_id: None,
            agent_id,
            rule_content,
            rule_description: None,
            confidence_score,
            verification_status: VerificationStatus::Pending,
            verification_method: None,
            verification_details: None,
            source_episode_cluster: source_episodes.clone(),
            episode_count: source_episodes.len() as i32,
            created_at: None,
            last_validated_at: None,
            application_count: 0,
            successful_applications: 0,
            failed_applications: 0,
            is_active: true,
            invalidated_at: None,
            invalidation_reason: None,
        }
    }
}

/// Knowledge graph entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_id: Option<Uuid>,
    pub agent_id: Uuid,

    // Entity identification
    pub entity_name: String,
    pub entity_type: String,
    pub summary: Option<String>,

    // Bi-temporal tracking
    pub t_valid: DateTime<Utc>,
    pub t_invalid: Option<DateTime<Utc>>,
    pub t_created: Option<DateTime<Utc>>,
    pub t_expired: Option<DateTime<Utc>>,

    // Attributes
    pub attributes: Option<serde_json::Value>,

    // Source tracking
    pub source_episode_ids: Vec<Uuid>,
    pub source_rule_ids: Vec<Uuid>,
}

impl Entity {
    pub fn new(agent_id: Uuid, entity_name: String, entity_type: String) -> Self {
        Self {
            entity_id: None,
            agent_id,
            entity_name,
            entity_type,
            summary: None,
            t_valid: Utc::now(),
            t_invalid: None,
            t_created: None,
            t_expired: None,
            attributes: None,
            source_episode_ids: Vec::new(),
            source_rule_ids: Vec::new(),
        }
    }
}

/// Knowledge graph relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub relationship_id: Option<Uuid>,
    pub agent_id: Uuid,

    // Relationship
    pub from_entity_id: Uuid,
    pub to_entity_id: Uuid,
    pub relationship_type: String,
    pub properties: Option<serde_json::Value>,

    // Bi-temporal tracking
    pub t_valid: DateTime<Utc>,
    pub t_invalid: Option<DateTime<Utc>>,
    pub t_created: Option<DateTime<Utc>>,
    pub t_expired: Option<DateTime<Utc>>,

    // Source tracking
    pub source_episode_ids: Vec<Uuid>,
    pub source_rule_ids: Vec<Uuid>,
}

impl Relationship {
    pub fn new(
        agent_id: Uuid,
        from_entity_id: Uuid,
        to_entity_id: Uuid,
        relationship_type: String,
    ) -> Self {
        Self {
            relationship_id: None,
            agent_id,
            from_entity_id,
            to_entity_id,
            relationship_type,
            properties: None,
            t_valid: Utc::now(),
            t_invalid: None,
            t_created: None,
            t_expired: None,
            source_episode_ids: Vec::new(),
            source_rule_ids: Vec::new(),
        }
    }
}

/// Fact - atomic piece of knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub fact_id: Option<Uuid>,
    pub agent_id: Uuid,

    // Fact content
    pub fact_content: String,
    pub fact_type: String,
    pub confidence: f32,

    // Related entities
    pub related_entity_ids: Vec<Uuid>,

    // Bi-temporal tracking
    pub t_valid: DateTime<Utc>,
    pub t_invalid: Option<DateTime<Utc>>,
    pub t_created: Option<DateTime<Utc>>,
    pub t_expired: Option<DateTime<Utc>>,

    // Source tracking
    pub source_episode_ids: Vec<Uuid>,
    pub source_rule_ids: Vec<Uuid>,
}

impl Fact {
    pub fn new(agent_id: Uuid, fact_content: String, fact_type: String, confidence: f32) -> Self {
        Self {
            fact_id: None,
            agent_id,
            fact_content,
            fact_type,
            confidence,
            related_entity_ids: Vec::new(),
            t_valid: Utc::now(),
            t_invalid: None,
            t_created: None,
            t_expired: None,
            source_episode_ids: Vec::new(),
            source_rule_ids: Vec::new(),
        }
    }
}
