use crate::types::ProjectionResult;
use dashmap::DashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct CacheKey {
    pub agent_id: Option<Uuid>,
    pub method: String,
    pub dimensions: u8,
}

struct CacheEntry {
    result: ProjectionResult,
    inserted_at: Instant,
}

pub struct ProjectionCache {
    entries: DashMap<CacheKey, CacheEntry>,
    ttl: Duration,
}

impl ProjectionCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            entries: DashMap::new(),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<ProjectionResult> {
        if let Some(entry) = self.entries.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Some(entry.result.clone());
            }
            // Expired — remove it
            drop(entry);
            self.entries.remove(key);
        }
        None
    }

    pub fn insert(&self, key: CacheKey, result: ProjectionResult) {
        self.entries.insert(
            key,
            CacheEntry {
                result,
                inserted_at: Instant::now(),
            },
        );
    }

    pub fn invalidate_agent(&self, agent_id: Uuid) {
        self.entries.retain(|k, _| k.agent_id != Some(agent_id));
    }
}
