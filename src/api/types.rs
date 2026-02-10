/// API Request and Response Types
use crate::agent_backend::AgentOutput;
use serde::{Deserialize, Serialize};

/// Request to execute an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteAgentRequest {
    pub query: String,
    pub agent_type: Option<String>,
    pub driver_refs: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub confidence_threshold: Option<f64>,
}

/// Response from agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteAgentResponse {
    pub agent_name: String,
    pub status: String,
    pub confidence: f64,
    pub execution_time_ms: u64,
    pub tokens_used: Option<u32>,
    pub evidence: Vec<EvidenceResponse>,
}

/// Evidence in API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceResponse {
    pub id: String,
    pub source: String,
    pub summary: Option<String>,
    pub key_findings: Vec<String>,
    pub relevance: Option<f64>,
}

impl From<&AgentOutput> for ExecuteAgentResponse {
    fn from(output: &AgentOutput) -> Self {
        ExecuteAgentResponse {
            agent_name: output.agent_name.clone(),
            status: format!("{:?}", output.status),
            confidence: output.confidence,
            execution_time_ms: output.execution_time_ms,
            tokens_used: output.tokens_used,
            evidence: output
                .evidence
                .iter()
                .map(|e| EvidenceResponse {
                    id: e.id.clone(),
                    source: e.source.clone(),
                    summary: e.summary.clone(),
                    key_findings: e.key_findings.clone(),
                    relevance: e.relevance,
                })
                .collect(),
        }
    }
}

/// List agents response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsResponse {
    pub agents: Vec<String>,
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
