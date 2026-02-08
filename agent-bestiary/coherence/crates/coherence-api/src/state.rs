//! Shared application state for the API server.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use coherence_core::{types::ConversationId, CoherenceSystem};
use coherence_engine::SettlingConfig;
use coherence_observer::ConversationObserver;

/// A single evaluation session — ties together a system, its observer,
/// and the engine configuration.
pub struct Session {
    pub system: CoherenceSystem,
    pub observer: ConversationObserver,
    pub title: Option<String>,
}

/// Shared application state, wrapped in `Arc<RwLock<_>>` for concurrent access.
#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<RwLock<HashMap<ConversationId, Session>>>,
    pub settling_config: SettlingConfig,
}

impl AppState {
    /// Create a new empty app state with default settling config.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            settling_config: SettlingConfig::default(),
        }
    }

    /// Create a new app state with custom settling config.
    pub fn with_config(config: SettlingConfig) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            settling_config: config,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
