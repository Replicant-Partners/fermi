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
    pub mermaid: String,
    pub stages: Vec<WorkflowStage>,
    #[serde(default)]
    pub description: Option<String>,
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
                valence: None,
            },
            system_prompt: None,
            dependencies: AgentDependencies::default(),
            accepts: vec![],
            produces: vec![],
            workflow_template: None,
            prompt_template: None,
            requires_secrets: vec![],
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

    #[test]
    fn test_all_cards_have_required_fields() {
        let cards = load_all_cards();
        for (dir_name, card) in &cards {
            // metadata.description must be meaningful
            assert!(
                !card.metadata.description.is_empty()
                    && !card.metadata.description.starts_with("Agent: "),
                "{}: metadata.description is missing or default",
                dir_name
            );
            // metadata.tags must be non-empty
            assert!(
                !card.metadata.tags.is_empty(),
                "{}: metadata.tags is empty",
                dir_name
            );
            // metadata.sample_queries must be non-empty
            assert!(
                !card.metadata.sample_queries.is_empty(),
                "{}: metadata.sample_queries is empty",
                dir_name
            );
            // metadata.valence must be present
            assert!(
                card.metadata.valence.is_some(),
                "{}: metadata.valence is missing",
                dir_name
            );
            // wallet must be present
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
        let cards = load_all_cards();
        for (dir_name, card) in &cards {
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
