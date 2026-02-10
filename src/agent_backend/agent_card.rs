/// Agent Card
///
/// Complete metadata and performance tracking for an agent.
/// Based on Agent Bestiary Design Document.
use crate::ast::ExecutorType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub dependencies: AgentDependencies,
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

/// MCP tool descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

/// Agent capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub executor: ExecutorType,
    #[serde(default)]
    pub mcp_tools: Vec<McpTool>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub model: String,
    pub temperature: f64,
    #[serde(default = "default_provider")]
    pub provider: String,
}

fn default_provider() -> String {
    "anthropic".to_string()
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

/// Agent metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub created: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub sample_queries: Vec<String>,
}

impl AgentCard {
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
                skills: vec![],
                model: "claude-3-haiku-20240307".to_string(),
                temperature: 0.3,
                provider: "anthropic".to_string(),
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
            },
            system_prompt: None,
            dependencies: AgentDependencies::default(),
        }
    }

    /// Load agent card from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Save agent card to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
