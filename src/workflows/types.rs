//! Workflow types — agent lifecycle states, fork pricing, publish checks.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent lifecycle status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentLifecycleStatus {
    Draft,
    Published,
    Archived,
}

impl AgentLifecycleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            other => Err(format!("Invalid agent status: '{}'", other)),
        }
    }
}

/// Fork pricing set by agent author
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkPricing {
    pub base_price: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_price: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_price: Option<i32>,
}

impl Default for ForkPricing {
    fn default() -> Self {
        Self {
            base_price: 0,
            ontology_price: None,
            embedding_price: None,
        }
    }
}

/// Publish check severity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckSeverity {
    Error,
    Warning,
    Info,
}

/// A single publish readiness check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishCheck {
    pub name: String,
    pub passed: bool,
    pub severity: CheckSeverity,
    pub message: String,
}

/// Fork request from a user
#[derive(Debug, Clone, Deserialize)]
pub struct ForkRequest {
    #[serde(default)]
    pub include_ontology: bool,
    #[serde(default)]
    pub include_embeddings: bool,
}

/// Result of a status transition
#[derive(Debug, Clone, Serialize)]
pub struct TransitionResult {
    pub agent_id: Uuid,
    pub from: String,
    pub to: String,
}
