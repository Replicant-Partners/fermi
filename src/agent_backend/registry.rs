/// Agent Registry
///
/// Manages agent cards, discovery, and execution routing.
/// Currently uses in-memory storage.
use crate::agent_backend::agent_card::AgentCard;
use crate::agent_backend::executor::{
    AgentExecutor, AgentOutput, ExecutionContext, ExecutionError, MockExecutor,
};
use crate::ast::AgentStmt;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};

/// Agent registry with in-memory storage
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, AgentCard>>>,
    executor: Arc<dyn AgentExecutor>,
}

impl AgentRegistry {
    /// Create a new agent registry with mock executor
    pub fn new() -> Self {
        AgentRegistry {
            agents: Arc::new(RwLock::new(HashMap::new())),
            executor: Arc::new(MockExecutor::new()),
        }
    }

    /// Create a new agent registry with specified executor
    pub fn with_executor(executor: Arc<dyn AgentExecutor>) -> Self {
        AgentRegistry {
            agents: Arc::new(RwLock::new(HashMap::new())),
            executor,
        }
    }

    /// Load agent cards from a directory
    pub fn load_from_directory<P: AsRef<Path>>(&self, dir: P) -> Result<usize, RegistryError> {
        let dir_path = dir.as_ref();

        if !dir_path.exists() {
            return Err(RegistryError::SerializationError(format!(
                "Directory does not exist: {}",
                dir_path.display()
            )));
        }

        let mut loaded_count = 0;

        // Iterate through subdirectories (each agent has its own folder)
        for entry in
            fs::read_dir(dir_path).map_err(|e| RegistryError::SerializationError(e.to_string()))?
        {
            let entry = entry.map_err(|e| RegistryError::SerializationError(e.to_string()))?;
            let path = entry.path();

            if path.is_dir() {
                // Look for agent_card.json in this directory
                let card_path = path.join("agent_card.json");
                if card_path.exists() {
                    match self.load_agent_card(&card_path) {
                        Ok(card) => {
                            self.register(card)?;
                            loaded_count += 1;
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to load agent card from {}: {}",
                                card_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        Ok(loaded_count)
    }

    /// Load a single agent card from a JSON file
    fn load_agent_card<P: AsRef<Path>>(&self, path: P) -> Result<AgentCard, RegistryError> {
        let json = fs::read_to_string(path.as_ref())
            .map_err(|e| RegistryError::SerializationError(e.to_string()))?;

        AgentCard::from_json(&json).map_err(|e| RegistryError::SerializationError(e.to_string()))
    }

    /// Save an agent card to filesystem
    pub fn save_agent_card<P: AsRef<Path>>(
        &self,
        agent_id: &str,
        base_dir: P,
    ) -> Result<(), RegistryError> {
        let card = self.get(agent_id)?;

        // Create agent directory
        let agent_dir = base_dir.as_ref().join(agent_id);
        fs::create_dir_all(&agent_dir)
            .map_err(|e| RegistryError::SerializationError(e.to_string()))?;

        // Write agent_card.json
        let card_path = agent_dir.join("agent_card.json");
        let json = card
            .to_json()
            .map_err(|e| RegistryError::SerializationError(e.to_string()))?;

        fs::write(&card_path, json)
            .map_err(|e| RegistryError::SerializationError(e.to_string()))?;

        Ok(())
    }

    /// Save agent card and auto-commit to git
    pub fn save_and_commit<P: AsRef<Path>>(
        &self,
        agent_id: &str,
        base_dir: P,
    ) -> Result<(), RegistryError> {
        let card = self.get(agent_id)?;

        // Save the card
        self.save_agent_card(agent_id, &base_dir)?;

        // Build commit message
        let message = format!(
            "agent({}): updated usage stats\n\n\
             Total executions: {}\n\
             Successful: {}\n\
             Failed: {}\n\
             Total tokens: {}\n\
             Total cost: ${:.6}\n\
             Avg execution time: {}ms\n\
             Success rate: {:.1}%",
            agent_id,
            card.usage.total_executions,
            card.usage.successful_executions,
            card.usage.failed_executions,
            card.usage.total_tokens_used,
            card.usage.total_cost_usd,
            card.usage.avg_execution_time_ms,
            if card.usage.total_executions > 0 {
                (card.usage.successful_executions as f64 / card.usage.total_executions as f64)
                    * 100.0
            } else {
                0.0
            }
        );

        // Git commit
        let agent_dir = base_dir.as_ref().join(agent_id);
        let card_path = agent_dir.join("agent_card.json");

        self.git_commit(&card_path, &message)?;

        Ok(())
    }

    /// Commit changes to git
    fn git_commit<P: AsRef<Path>>(&self, file_path: P, message: &str) -> Result<(), RegistryError> {
        let path_str = file_path
            .as_ref()
            .to_str()
            .ok_or_else(|| RegistryError::SerializationError("Invalid file path".to_string()))?;

        // Git add
        let add_output = Command::new("git")
            .args(&["add", path_str])
            .output()
            .map_err(|e| RegistryError::SerializationError(format!("Git add failed: {}", e)))?;

        if !add_output.status.success() {
            let stderr = String::from_utf8_lossy(&add_output.stderr);
            return Err(RegistryError::SerializationError(format!(
                "Git add failed: {}",
                stderr
            )));
        }

        // Git commit
        let commit_output = Command::new("git")
            .args(&["commit", "-m", message])
            .output()
            .map_err(|e| RegistryError::SerializationError(format!("Git commit failed: {}", e)))?;

        if !commit_output.status.success() {
            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            // Ignore "nothing to commit" errors
            if !stderr.contains("nothing to commit") && !stderr.contains("no changes added") {
                return Err(RegistryError::SerializationError(format!(
                    "Git commit failed: {}",
                    stderr
                )));
            }
        }

        Ok(())
    }

    /// Register a new agent
    pub fn register(&self, card: AgentCard) -> Result<(), RegistryError> {
        let mut agents = self.agents.write().map_err(|_| RegistryError::LockError)?;

        if agents.contains_key(&card.agent_id) {
            return Err(RegistryError::AgentExists(card.agent_id.clone()));
        }

        agents.insert(card.agent_id.clone(), card);
        Ok(())
    }

    /// Get an agent card by ID
    pub fn get(&self, agent_id: &str) -> Result<AgentCard, RegistryError> {
        let agents = self.agents.read().map_err(|_| RegistryError::LockError)?;

        agents
            .get(agent_id)
            .cloned()
            .ok_or_else(|| RegistryError::AgentNotFound(agent_id.to_string()))
    }

    /// List all agent IDs
    pub fn list(&self) -> Result<Vec<String>, RegistryError> {
        let agents = self.agents.read().map_err(|_| RegistryError::LockError)?;
        Ok(agents.keys().cloned().collect())
    }

    /// List all agent cards
    pub fn list_cards(&self) -> Result<Vec<AgentCard>, RegistryError> {
        let agents = self.agents.read().map_err(|_| RegistryError::LockError)?;
        Ok(agents.values().cloned().collect())
    }

    /// Get a reference to the inner executor
    pub fn executor_arc(&self) -> Arc<dyn AgentExecutor> {
        Arc::clone(&self.executor)
    }

    /// Execute an agent
    pub async fn execute_agent(
        &self,
        agent: &AgentStmt,
        context: &ExecutionContext,
    ) -> Result<AgentOutput, ExecutionError> {
        self.executor.execute(agent, context).await
    }

    /// Update agent card after execution
    pub fn record_execution(
        &self,
        agent_id: &str,
        output: &AgentOutput,
    ) -> Result<(), RegistryError> {
        let mut agents = self.agents.write().map_err(|_| RegistryError::LockError)?;

        let card = agents
            .get_mut(agent_id)
            .ok_or_else(|| RegistryError::AgentNotFound(agent_id.to_string()))?;

        // Update usage stats
        card.usage.total_executions += 1;

        match output.status {
            crate::agent_backend::executor::AgentStatus::Success => {
                card.usage.successful_executions += 1;
            }
            _ => {
                card.usage.failed_executions += 1;
            }
        }

        // Update token usage and cost
        if let Some(tokens) = output.tokens_used {
            card.usage.total_tokens_used += tokens as u64;

            // Calculate cost based on model
            let cost = calculate_cost(&card.capabilities.model, tokens);
            card.usage.total_cost_usd += cost;
        }

        // Update average execution time
        let total_execs = card.usage.total_executions;
        card.usage.avg_execution_time_ms = (card.usage.avg_execution_time_ms
            * (total_execs - 1) as u64
            + output.execution_time_ms)
            / total_execs as u64;

        Ok(())
    }

    /// Update an agent card
    pub fn update(&self, card: AgentCard) -> Result<(), RegistryError> {
        let mut agents = self.agents.write().map_err(|_| RegistryError::LockError)?;

        if !agents.contains_key(&card.agent_id) {
            return Err(RegistryError::AgentNotFound(card.agent_id.clone()));
        }

        agents.insert(card.agent_id.clone(), card);
        Ok(())
    }

    /// Remove an agent
    pub fn remove(&self, agent_id: &str) -> Result<(), RegistryError> {
        let mut agents = self.agents.write().map_err(|_| RegistryError::LockError)?;

        agents
            .remove(agent_id)
            .ok_or_else(|| RegistryError::AgentNotFound(agent_id.to_string()))?;

        Ok(())
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry errors
#[derive(Debug, Clone)]
pub enum RegistryError {
    AgentNotFound(String),
    AgentExists(String),
    LockError,
    SerializationError(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RegistryError::AgentNotFound(id) => write!(f, "Agent not found: {}", id),
            RegistryError::AgentExists(id) => write!(f, "Agent already exists: {}", id),
            RegistryError::LockError => write!(f, "Failed to acquire lock"),
            RegistryError::SerializationError(e) => write!(f, "Serialization error: {}", e),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Calculate cost based on model and token count
fn calculate_cost(model: &str, tokens: u32) -> f64 {
    // Model-specific pricing (per 1M tokens)
    let rate_per_million = match model {
        "claude-3-5-sonnet-20241022" => 3.0,
        "claude-3-5-sonnet-20240620" => 3.0,
        "claude-3-opus-20240229" => 15.0,
        "claude-3-haiku-20240307" => 0.25,
        _ => 3.0, // Default
    };

    (tokens as f64 / 1_000_000.0) * rate_per_million
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_backend::AgentCard;

    #[test]
    fn test_registry_register_and_get() {
        let registry = AgentRegistry::new();
        let card = AgentCard::new("test_agent".to_string(), "research".to_string());

        registry.register(card.clone()).unwrap();
        let retrieved = registry.get("test_agent").unwrap();

        assert_eq!(retrieved.agent_id, "test_agent");
    }

    #[test]
    fn test_registry_list() {
        let registry = AgentRegistry::new();
        let card1 = AgentCard::new("agent1".to_string(), "research".to_string());
        let card2 = AgentCard::new("agent2".to_string(), "sentiment".to_string());

        registry.register(card1).unwrap();
        registry.register(card2).unwrap();

        let list = registry.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"agent1".to_string()));
        assert!(list.contains(&"agent2".to_string()));
    }

    #[test]
    fn test_registry_duplicate_register() {
        let registry = AgentRegistry::new();
        let card = AgentCard::new("test_agent".to_string(), "research".to_string());

        registry.register(card.clone()).unwrap();
        let result = registry.register(card);

        assert!(result.is_err());
    }

    #[test]
    fn test_registry_remove() {
        let registry = AgentRegistry::new();
        let card = AgentCard::new("test_agent".to_string(), "research".to_string());

        registry.register(card).unwrap();
        registry.remove("test_agent").unwrap();

        let result = registry.get("test_agent");
        assert!(result.is_err());
    }
}
