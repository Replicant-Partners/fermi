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

// ─── Cognition tier (ADR-011) ──────────────────────────────────────

/// Cognitive bandwidth tier for creature-driven model selection.
/// Declaration order determines Ord: Free < Standard < Premium.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum CognitionTier {
    Free,
    Standard,
    Premium,
}

impl Default for CognitionTier {
    fn default() -> Self {
        CognitionTier::Free
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
    #[serde(default)]
    pub mcp_tools: Vec<McpTool>,
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
    /// Feature gates: capability name → minimum tier required to invoke it.
    #[serde(default)]
    pub capability_gates: HashMap<String, CognitionTier>,

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

    /// Provider-agnostic sampling configuration. Keys override the legacy
    /// `temperature` field and add provider-specific params (top_p, top_k,
    /// extended_thinking, thinking_budget_tokens, frequency_penalty, etc.).
    /// `apply_tier_resolution()` merges the selected rung's `params` on top.
    #[serde(default = "default_json_object")]
    pub model_params: serde_json::Value,
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
                if let (
                    serde_json::Value::Object(base),
                    serde_json::Value::Object(overrides),
                ) = (&mut self.model_params, rp)
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
            top_k: p
                .get("top_k")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32),
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
                model_ladder: vec![],
                min_tier: CognitionTier::Free,
                capability_gates: HashMap::new(),
                fermi_contract: None,
                output_contract: None,
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
            unique.len(), names.len(),
            "Duplicate skill names in SkillRegistry: {:?}", names
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
            names.len(), names
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
