use crate::error::ProjectionError;
use crate::executor::ModelExecutor;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of all available model executors.
/// Executors register themselves by kind string.
/// The projection engine resolves the executor by the `kind` field
/// in the incoming [`ModelConfig`].
pub struct ExecutorRegistry {
    executors: HashMap<String, Arc<dyn ModelExecutor>>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
        }
    }

    /// Register an executor. Overwrites any existing registration for the
    /// same kind string.
    pub fn register(&mut self, executor: Arc<dyn ModelExecutor>) {
        self.executors.insert(executor.kind().to_string(), executor);
    }

    /// Retrieve an executor by kind string.
    pub fn get(&self, kind: &str) -> Result<Arc<dyn ModelExecutor>, ProjectionError> {
        self.executors
            .get(kind)
            .cloned()
            .ok_or_else(|| ProjectionError::UnknownExecutor(kind.to_string()))
    }

    /// List all registered executor kind strings.
    pub fn kinds(&self) -> Vec<&str> {
        self.executors.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        let mut registry = Self::new();

        // Register built-in executors
        #[cfg(feature = "simops-executor")]
        {
            use crate::simops_executor::SimOpsCascadeExecutor;
            registry.register(Arc::new(SimOpsCascadeExecutor));
        }

        registry
    }
}
