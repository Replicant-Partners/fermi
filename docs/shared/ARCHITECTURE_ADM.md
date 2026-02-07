# Fermi Active Dreaming Memory Architecture

**Status:** Design Complete  
**Date:** 2026-02-06  
**Version:** 1.0

## Executive Summary

This document describes the complete Active Dreaming Memory (ADM) architecture for Fermi forecasting agents, implementing biologically-inspired episodic memory consolidation with PostgreSQL storage and Mermaid ER ontology representation.

---

## Core Design Decisions

### 1. **Git as Immutable Event Log (Option B)**
- Every consolidation creates a git commit
- Mermaid files are immutable snapshots
- Agent card references git commit SHA
- Full temporal history via git log

### 2. **Bidirectional Linking with Race Prevention (Option C)**
- Agent card has BOTH git commit SHA AND database snapshot ID
- Database locks prevent concurrent consolidation
- Atomic updates ensure consistency
- No orphaned references

### 3. **Combined Verification Approach**
1. **Contradiction Check** - Fast, prevents logical conflicts
2. **Historical Validation** - Data-driven, checks past episodes
3. **Counterfactual Scenarios** - Deep verification for low-confidence rules

### 4. **External Consolidation Worker**
- Separate Rust binary (not on Vercel)
- Runs on schedule (daily 2am)
- Connects to Vercel Postgres
- Commits to GitHub
- Simple architecture, no timeouts

### 5. **Fresh Start for Agents**
- Migrate market_research and sentiment_analyzer to new system
- No historical backfill (they're experimental)
- Start clean with ADM architecture
- Build ontologies from scratch

---

## System Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                    FERMI ADM ECOSYSTEM                        │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  WAKE PHASE (Online - Agent Execution)                  │ │
│  ├─────────────────────────────────────────────────────────┤ │
│  │                                                         │ │
│  │  Agent executes → Write episode to PostgreSQL          │ │
│  │                                                         │ │
│  │  Episode:                                               │ │
│  │    - Query, context, result                            │ │
│  │    - Success/failure status                            │ │
│  │    - Embedding (vector)                                │ │
│  │    - Metrics (time, cost, tokens)                      │ │
│  │                                                         │ │
│  └─────────────────────────────────────────────────────────┘ │
│                            ↓                                  │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  SLEEP PHASE (Offline - Consolidation Worker)          │ │
│  ├─────────────────────────────────────────────────────────┤ │
│  │                                                         │ │
│  │  1. Acquire lock (prevent races)                       │ │
│  │  2. Cluster unconsolidated episodes (DBSCAN)           │ │
│  │  3. Extract candidate rules from clusters              │ │
│  │  4. Verify rules (contradiction + historical + CF)     │ │
│  │  5. Store verified rules                               │ │
│  │  6. Extract entities and facts                         │ │
│  │  7. Generate Mermaid ER diagram                        │ │
│  │  8. Commit to git                                      │ │
│  │  9. Update agent card (bidirectional link)            │ │
│  │  10. Release lock                                      │ │
│  │                                                         │ │
│  └─────────────────────────────────────────────────────────┘ │
│                            ↓                                  │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  RETRIEVAL PHASE (Application)                         │ │
│  ├─────────────────────────────────────────────────────────┤ │
│  │                                                         │ │
│  │  Query arrives → Generate embedding                     │ │
│  │                                                         │ │
│  │  Multi-modal search:                                   │ │
│  │    φcos: Vector similarity on rules/entities           │ │
│  │    φbm25: Full-text search on content                  │ │
│  │    φbfs: Graph traversal on relationships              │ │
│  │                                                         │ │
│  │  Rerank: Reciprocal rank fusion                        │ │
│  │  Apply: Top-k rules to task                            │ │
│  │  Track: Application success/failure                    │ │
│  │                                                         │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

---

## Rust Crate Architecture

### Workspace Structure

```
fermi/
├── Cargo.toml                    # Workspace
├── fermi-core/                   # Existing (lexer, parser, AST)
├── fermi-memory/                 # NEW: ADM + PostgreSQL
├── fermi-ontology/               # NEW: Mermaid + Knowledge Graph
├── fermi-consolidate/            # NEW: Sleep phase worker
├── fermi-agent-backend/          # Existing (execution, registry)
├── fermi-mcp/                    # Existing (MCP server)
├── fermi-api/                    # NEW: Vercel functions
└── fermi-lsp/                    # Existing (language server)
```

---

## Crate 1: `fermi-memory`

**Purpose:** Episodic and semantic memory storage, PostgreSQL abstraction, vector search

### Key Modules

```rust
// fermi-memory/src/lib.rs

pub mod episodic;    // Episode storage and retrieval
pub mod semantic;    // Semantic rule storage
pub mod knowledge;   // Entity/fact knowledge graph
pub mod search;      // Multi-modal retrieval
pub mod lock;        // Distributed locking
pub mod db;          // Database connection pool

// Core traits
pub trait Memory: Send + Sync {
    async fn store_episode(&self, episode: Episode) -> Result<EpisodeId>;
    async fn retrieve_episodes(&self, query: EpisodeQuery) -> Result<Vec<Episode>>;
    async fn consolidate(&self, agent_id: AgentId) -> Result<ConsolidationResult>;
}

pub trait VectorSearch: Send + Sync {
    async fn search_similar(&self, embedding: Vec<f32>, limit: usize) -> Result<Vec<SearchMatch>>;
    async fn search_hybrid(&self, query: HybridQuery) -> Result<Vec<SearchMatch>>;
}
```

### Core Types

```rust
// Episode (Episodic Memory)
pub struct Episode {
    pub episode_id: Uuid,
    pub agent_id: Uuid,
    pub timestamp_ref: DateTime<Utc>,
    pub query: String,
    pub context: serde_json::Value,
    pub execution_status: ExecutionStatus,
    pub execution_time_ms: u64,
    pub tokens_used: Option<u32>,
    pub cost_usd: Option<f64>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Success,
    Failure,
    Partial,
}

// Semantic Rule
pub struct SemanticRule {
    pub rule_id: Uuid,
    pub agent_id: Uuid,
    pub rule_content: String,
    pub confidence_score: f64,
    pub verification_status: VerificationStatus,
    pub source_episodes: Vec<Uuid>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStatus {
    Pending,
    Verified,
    Rejected,
}

// Entity (Knowledge Graph Node)
pub struct Entity {
    pub entity_id: Uuid,
    pub agent_id: Uuid,
    pub entity_name: String,
    pub entity_type: String,
    pub summary: Option<String>,
    pub t_valid: DateTime<Utc>,
    pub t_invalid: Option<DateTime<Utc>>,
    pub embedding: Option<Vec<f32>>,
}

// Fact (Knowledge Graph Edge)
pub struct Fact {
    pub fact_id: Uuid,
    pub agent_id: Uuid,
    pub source_entity_id: Uuid,
    pub target_entity_id: Uuid,
    pub relation_type: String,
    pub relation_cardinality: Cardinality,
    pub confidence: f64,
    pub t_valid: DateTime<Utc>,
    pub t_invalid: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Cardinality {
    OneToOne,       // ||--||
    OneToMany,      // ||--o{
    ManyToOne,      // }o--||
    ManyToMany,     // }o--o{
}
```

### Database Integration

```rust
// fermi-memory/src/db.rs

use sqlx::{PgPool, postgres::PgPoolOptions};

pub struct MemoryStore {
    pool: PgPool,
}

impl MemoryStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect(database_url)
            .await?;
        
        Ok(Self { pool })
    }
    
    pub async fn store_episode(&self, episode: Episode) -> Result<Uuid> {
        let row = sqlx::query!(
            r#"
            INSERT INTO episodes (
                agent_id, timestamp_ref, query, context,
                execution_status, execution_time_ms, tokens_used,
                cost_usd, embedding
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING episode_id
            "#,
            episode.agent_id,
            episode.timestamp_ref,
            episode.query,
            episode.context,
            episode.execution_status.to_string(),
            episode.execution_time_ms as i64,
            episode.tokens_used.map(|t| t as i32),
            episode.cost_usd.map(|c| c as f64),
            episode.embedding.as_ref().map(|e| pgvector::Vector::from(e.clone()))
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(row.episode_id)
    }
    
    pub async fn cluster_episodes(&self, agent_id: Uuid, epsilon: f32) -> Result<Vec<EpisodeCluster>> {
        // DBSCAN clustering using vector similarity
        // Implementation uses pgvector distance operators
        todo!()
    }
}
```

### Vector Search

```rust
// fermi-memory/src/search.rs

pub struct HybridSearch {
    memory_store: Arc<MemoryStore>,
}

impl HybridSearch {
    pub async fn search(&self, query: &str, agent_id: Uuid) -> Result<Vec<SearchResult>> {
        // Generate query embedding
        let embedding = self.generate_embedding(query).await?;
        
        // φcos: Vector similarity search
        let vector_results = self.search_vector(&embedding, agent_id, 20).await?;
        
        // φbm25: Full-text search
        let text_results = self.search_fulltext(query, agent_id, 20).await?;
        
        // φbfs: Graph traversal (if entity detected)
        let graph_results = self.search_graph(query, agent_id, 20).await?;
        
        // Reciprocal rank fusion
        let fused = self.reciprocal_rank_fusion(vec![
            vector_results,
            text_results,
            graph_results,
        ])?;
        
        Ok(fused)
    }
    
    async fn search_vector(&self, embedding: &[f32], agent_id: Uuid, limit: usize) -> Result<Vec<SearchResult>> {
        let results = sqlx::query!(
            r#"
            SELECT rule_id, rule_content, confidence_score,
                   embedding <=> $1 AS distance
            FROM semantic_rules
            WHERE agent_id = $2
              AND verification_status = 'verified'
              AND is_active = true
            ORDER BY embedding <=> $1
            LIMIT $3
            "#,
            pgvector::Vector::from(embedding.to_vec()),
            agent_id,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;
        
        // Convert to SearchResult
        todo!()
    }
}
```

### Distributed Locking

```rust
// fermi-memory/src/lock.rs

pub struct ConsolidationLock {
    memory_store: Arc<MemoryStore>,
    worker_id: String,
}

impl ConsolidationLock {
    pub async fn acquire(&self, agent_id: Uuid, timeout_minutes: i32) -> Result<bool> {
        let acquired = sqlx::query_scalar!(
            "SELECT acquire_consolidation_lock($1, $2, $3)",
            agent_id,
            self.worker_id,
            timeout_minutes
        )
        .fetch_one(&self.memory_store.pool)
        .await?;
        
        Ok(acquired.unwrap_or(false))
    }
    
    pub async fn release(&self, agent_id: Uuid) -> Result<()> {
        sqlx::query_scalar!(
            "SELECT release_consolidation_lock($1, $2)",
            agent_id,
            self.worker_id
        )
        .fetch_one(&self.memory_store.pool)
        .await?;
        
        Ok(())
    }
}
```

### Dependencies

```toml
# fermi-memory/Cargo.toml

[package]
name = "fermi-memory"
version = "0.1.0"
edition = "2021"

[dependencies]
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-rustls", "uuid", "chrono", "json"] }
pgvector = { version = "0.3", features = ["sqlx"] }
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
```

---

## Crate 2: `fermi-ontology`

**Purpose:** Mermaid ER generation, git integration, knowledge graph operations

### Key Modules

```rust
// fermi-ontology/src/lib.rs

pub mod mermaid;     // Mermaid ER diagram generation/parsing
pub mod git;         // Git commit operations
pub mod graph;       // Knowledge graph algorithms
pub mod resolution;  // Entity resolution and merging

pub trait OntologyManager: Send + Sync {
    async fn generate_mermaid(&self, agent_id: Uuid) -> Result<String>;
    async fn commit_ontology(&self, agent_id: Uuid, mermaid: String) -> Result<GitCommit>;
    async fn resolve_entities(&self, entities: Vec<Entity>) -> Result<Vec<Entity>>;
}
```

### Mermaid Generation

```rust
// fermi-ontology/src/mermaid.rs

pub struct MermaidGenerator {
    memory_store: Arc<MemoryStore>,
}

impl MermaidGenerator {
    pub async fn generate(&self, agent_id: Uuid) -> Result<String> {
        // Fetch current entities and facts
        let entities = self.memory_store.get_current_entities(agent_id).await?;
        let facts = self.memory_store.get_current_facts(agent_id).await?;
        
        // Generate Mermaid ER diagram
        let mut mermaid = String::from("erDiagram\n");
        
        // Add relationships
        for fact in &facts {
            let source = entities.iter().find(|e| e.entity_id == fact.source_entity_id)?;
            let target = entities.iter().find(|e| e.entity_id == fact.target_entity_id)?;
            
            mermaid.push_str(&format!(
                "    {} {} {} : {}\n",
                Self::escape_name(&source.entity_name),
                fact.relation_cardinality.to_mermaid(),
                Self::escape_name(&target.entity_name),
                fact.relation_type
            ));
        }
        
        // Add entity definitions with attributes
        for entity in &entities {
            mermaid.push_str(&format!("\n    {} {{\n", Self::escape_name(&entity.entity_name)));
            mermaid.push_str(&format!("        string entity_id\n"));
            mermaid.push_str(&format!("        string entity_type\n"));
            
            if let Some(summary) = &entity.summary {
                let summary_safe = summary.replace('\n', ' ').chars().take(100).collect::<String>();
                mermaid.push_str(&format!("        string summary \"{}...\"\n", summary_safe));
            }
            
            mermaid.push_str(&format!("        timestamp t_valid\n"));
            if entity.t_invalid.is_some() {
                mermaid.push_str(&format!("        timestamp t_invalid\n"));
            }
            
            mermaid.push_str("    }\n");
        }
        
        Ok(mermaid)
    }
    
    fn escape_name(name: &str) -> String {
        name.replace(' ', "_")
            .replace('-', "_")
            .replace('/', "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect()
    }
}

impl Cardinality {
    pub fn to_mermaid(&self) -> &'static str {
        match self {
            Cardinality::OneToOne => "||--||",
            Cardinality::OneToMany => "||--o{",
            Cardinality::ManyToOne => "}o--||",
            Cardinality::ManyToMany => "}o--o{",
        }
    }
}
```

### Git Integration

```rust
// fermi-ontology/src/git.rs

use std::process::Command;

pub struct GitManager {
    repository_path: PathBuf,
}

impl GitManager {
    pub async fn commit_ontology(
        &self,
        agent_name: &str,
        mermaid_content: &str,
        stats: &OntologyStats,
    ) -> Result<GitCommit> {
        // Write mermaid file
        let ontology_path = self.repository_path
            .join("agents")
            .join("curated")
            .join(agent_name)
            .join("ontology.mermaid");
        
        tokio::fs::create_dir_all(ontology_path.parent().unwrap()).await?;
        tokio::fs::write(&ontology_path, mermaid_content).await?;
        
        // Git add
        let output = Command::new("git")
            .current_dir(&self.repository_path)
            .args(&["add", ontology_path.to_str().unwrap()])
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow!("Git add failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        
        // Create detailed commit message
        let message = format!(
            "agent({}): ontology consolidation\n\n\
             Entities: {}\n\
             Relationships: {}\n\
             Rules: {}\n\
             Episode range: {} episodes\n\n\
             Timestamp: {}",
            agent_name,
            stats.entity_count,
            stats.fact_count,
            stats.rule_count,
            stats.episode_count,
            Utc::now().to_rfc3339()
        );
        
        // Git commit
        let output = Command::new("git")
            .current_dir(&self.repository_path)
            .args(&["commit", "-m", &message])
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow!("Git commit failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        
        // Get commit SHA
        let output = Command::new("git")
            .current_dir(&self.repository_path)
            .args(&["rev-parse", "HEAD"])
            .output()?;
        
        let commit_sha = String::from_utf8(output.stdout)?.trim().to_string();
        
        Ok(GitCommit {
            sha: commit_sha,
            message,
            timestamp: Utc::now(),
        })
    }
    
    pub async fn push_to_remote(&self) -> Result<()> {
        let output = Command::new("git")
            .current_dir(&self.repository_path)
            .args(&["push", "origin", "main"])
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow!("Git push failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(())
    }
}

pub struct GitCommit {
    pub sha: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}
```

### Entity Resolution

```rust
// fermi-ontology/src/resolution.rs

pub struct EntityResolver {
    memory_store: Arc<MemoryStore>,
}

impl EntityResolver {
    pub async fn resolve(&self, new_entity: &Entity, agent_id: Uuid) -> Result<EntityResolution> {
        // 1. Vector similarity search
        let similar = self.memory_store
            .search_similar_entities(agent_id, &new_entity.embedding, 5)
            .await?;
        
        // 2. Name similarity (fuzzy matching)
        let name_matches = self.memory_store
            .search_entities_by_name(agent_id, &new_entity.entity_name, 0.8)
            .await?;
        
        // 3. Combine results
        let candidates: Vec<Entity> = similar
            .into_iter()
            .chain(name_matches)
            .collect();
        
        if candidates.is_empty() {
            return Ok(EntityResolution::NewEntity);
        }
        
        // 4. LLM-based resolution (if multiple candidates)
        if candidates.len() > 1 {
            let resolved = self.llm_resolve(new_entity, &candidates).await?;
            return Ok(resolved);
        }
        
        // 5. High confidence match
        let candidate = &candidates[0];
        if self.is_same_entity(new_entity, candidate) {
            Ok(EntityResolution::Merge(candidate.entity_id))
        } else {
            Ok(EntityResolution::NewEntity)
        }
    }
    
    async fn llm_resolve(&self, new: &Entity, candidates: &[Entity]) -> Result<EntityResolution> {
        // Use LLM to determine if entities should merge
        // Prompt: "Are these entities the same? Entity A: {...}, Entity B: {...}"
        todo!()
    }
}

pub enum EntityResolution {
    NewEntity,
    Merge(Uuid), // Merge into existing entity_id
    Update(Uuid), // Update existing entity
}
```

### Dependencies

```toml
# fermi-ontology/Cargo.toml

[package]
name = "fermi-ontology"
version = "0.1.0"
edition = "2021"

[dependencies]
fermi-memory = { path = "../fermi-memory" }
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
```

---

## Crate 3: `fermi-consolidate`

**Purpose:** Sleep phase worker binary - consolidation orchestration

### Main Binary

```rust
// fermi-consolidate/src/main.rs

use fermi_memory::{MemoryStore, ConsolidationLock};
use fermi_ontology::{MermaidGenerator, GitManager};
use tokio::time::{sleep, Duration};
use tracing::{info, error, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Load configuration
    let config = Config::from_env()?;
    
    // Initialize services
    let memory_store = Arc::new(MemoryStore::new(&config.database_url).await?);
    let mermaid_gen = Arc::new(MermaidGenerator::new(memory_store.clone()));
    let git_manager = Arc::new(GitManager::new(config.repository_path.clone()));
    let consolidator = Consolidator::new(
        memory_store.clone(),
        mermaid_gen,
        git_manager,
        config.clone(),
    );
    
    info!("Consolidation worker started");
    info!("Worker ID: {}", config.worker_id);
    info!("Schedule: Daily at {}", config.consolidation_time);
    
    // Main loop
    loop {
        // Wait until next scheduled time
        let next_run = calculate_next_run(&config.consolidation_time);
        let duration = next_run.signed_duration_since(Utc::now());
        
        if duration.num_seconds() > 0 {
            info!("Next consolidation in {} minutes", duration.num_minutes());
            sleep(Duration::from_secs(duration.num_seconds() as u64)).await;
        }
        
        // Run consolidation for all agents
        match consolidator.consolidate_all().await {
            Ok(results) => {
                info!("Consolidation completed successfully");
                for result in results {
                    info!(
                        "Agent {}: {} episodes, {} rules extracted, {} verified",
                        result.agent_name,
                        result.episodes_processed,
                        result.rules_extracted,
                        result.rules_verified
                    );
                }
            }
            Err(e) => {
                error!("Consolidation failed: {}", e);
            }
        }
    }
}

struct Config {
    database_url: String,
    repository_path: PathBuf,
    worker_id: String,
    consolidation_time: String, // "02:00" format
    anthropic_api_key: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")?,
            repository_path: PathBuf::from(std::env::var("REPO_PATH")?),
            worker_id: std::env::var("WORKER_ID")
                .unwrap_or_else(|_| format!("worker-{}", uuid::Uuid::new_v4())),
            consolidation_time: std::env::var("CONSOLIDATION_TIME")
                .unwrap_or_else(|_| "02:00".to_string()),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY")?,
        })
    }
}

fn calculate_next_run(time_str: &str) -> DateTime<Utc> {
    // Parse "HH:MM" and calculate next occurrence
    let parts: Vec<&str> = time_str.split(':').collect();
    let hour: u32 = parts[0].parse().unwrap_or(2);
    let minute: u32 = parts[1].parse().unwrap_or(0);
    
    let now = Utc::now();
    let mut next = now
        .date_naive()
        .and_hms_opt(hour, minute, 0)
        .unwrap()
        .and_utc();
    
    if next <= now {
        next = next + chrono::Duration::days(1);
    }
    
    next
}
```

### Consolidator

```rust
// fermi-consolidate/src/consolidator.rs

pub struct Consolidator {
    memory_store: Arc<MemoryStore>,
    mermaid_gen: Arc<MermaidGenerator>,
    git_manager: Arc<GitManager>,
    verifier: Arc<RuleVerifier>,
    entity_extractor: Arc<EntityExtractor>,
    config: Config,
}

impl Consolidator {
    pub async fn consolidate_all(&self) -> Result<Vec<ConsolidationResult>> {
        let agents = self.memory_store.list_agents().await?;
        let mut results = Vec::new();
        
        for agent in agents {
            match self.consolidate_agent(agent.agent_id).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Failed to consolidate agent {}: {}", agent.agent_name, e);
                }
            }
        }
        
        Ok(results)
    }
    
    pub async fn consolidate_agent(&self, agent_id: Uuid) -> Result<ConsolidationResult> {
        info!("Starting consolidation for agent {}", agent_id);
        
        // 1. Acquire lock
        let lock = ConsolidationLock::new(self.memory_store.clone(), self.config.worker_id.clone());
        if !lock.acquire(agent_id, 60).await? {
            warn!("Could not acquire lock for agent {}, skipping", agent_id);
            return Err(anyhow!("Lock acquisition failed"));
        }
        
        // Ensure lock is released on drop
        let _lock_guard = scopeguard::guard(lock, |l| {
            tokio::spawn(async move {
                let _ = l.release(agent_id).await;
            });
        });
        
        // 2. Create consolidation job
        let job_id = self.memory_store.create_consolidation_job(agent_id).await?;
        
        // 3. Fetch unconsolidated episodes
        let episodes = self.memory_store
            .get_unconsolidated_episodes(agent_id)
            .await?;
        
        if episodes.is_empty() {
            info!("No unconsolidated episodes for agent {}", agent_id);
            return Ok(ConsolidationResult::default());
        }
        
        info!("Found {} unconsolidated episodes", episodes.len());
        
        // 4. Cluster episodes (DBSCAN)
        let clusters = self.cluster_episodes(&episodes).await?;
        info!("Identified {} clusters", clusters.len());
        
        // 5. Extract rules from clusters
        let mut rules_extracted = 0;
        let mut rules_verified = 0;
        
        for cluster in &clusters {
            if let Ok(rules) = self.extract_rules_from_cluster(cluster).await {
                rules_extracted += rules.len();
                
                // 6. Verify each rule
                for rule in rules {
                    match self.verify_rule(&rule, agent_id).await {
                        Ok(VerificationResult::Verified) => {
                            self.memory_store.store_rule(rule).await?;
                            rules_verified += 1;
                        }
                        Ok(VerificationResult::Rejected(reason)) => {
                            warn!("Rule rejected: {}", reason);
                        }
                        Err(e) => {
                            error!("Verification failed: {}", e);
                        }
                    }
                }
            }
        }
        
        // 7. Extract entities and facts
        let (entities, facts) = self.extract_knowledge_graph(agent_id, &episodes).await?;
        info!("Extracted {} entities, {} facts", entities.len(), facts.len());
        
        // 8. Generate Mermaid diagram
        let mermaid = self.mermaid_gen.generate(agent_id).await?;
        
        // 9. Commit to git
        let git_commit = self.git_manager.commit_ontology(
            &agent.agent_name,
            &mermaid,
            &OntologyStats {
                entity_count: entities.len(),
                fact_count: facts.len(),
                rule_count: rules_verified,
                episode_count: episodes.len(),
            },
        ).await?;
        
        info!("Committed ontology to git: {}", git_commit.sha);
        
        // 10. Create ontology snapshot in database
        let snapshot_id = self.memory_store.create_ontology_snapshot(
            agent_id,
            &git_commit.sha,
            &mermaid,
            job_id,
        ).await?;
        
        // 11. Update agent card (bidirectional linking)
        self.memory_store.update_agent_ontology_refs(
            agent_id,
            &git_commit.sha,
            snapshot_id,
        ).await?;
        
        // 12. Mark episodes as consolidated
        self.memory_store.mark_episodes_consolidated(
            &episodes.iter().map(|e| e.episode_id).collect::<Vec<_>>(),
            job_id,
        ).await?;
        
        // 13. Complete job
        self.memory_store.complete_consolidation_job(
            job_id,
            clusters.len(),
            rules_extracted,
            rules_verified,
            entities.len(),
            facts.len(),
        ).await?;
        
        // 14. Push to remote (optional, can be manual)
        if self.config.auto_push {
            self.git_manager.push_to_remote().await?;
        }
        
        info!("Consolidation completed for agent {}", agent_id);
        
        Ok(ConsolidationResult {
            agent_name: agent.agent_name,
            episodes_processed: episodes.len(),
            clusters_identified: clusters.len(),
            rules_extracted,
            rules_verified,
            entities_created: entities.len(),
            facts_created: facts.len(),
            git_commit_sha: git_commit.sha,
        })
    }
    
    async fn cluster_episodes(&self, episodes: &[Episode]) -> Result<Vec<EpisodeCluster>> {
        // DBSCAN clustering on episode embeddings
        // Use pgvector for similarity computation
        self.memory_store.cluster_episodes_dbscan(
            episodes,
            0.3, // epsilon (distance threshold)
            3,   // min_samples
        ).await
    }
    
    async fn extract_rules_from_cluster(&self, cluster: &EpisodeCluster) -> Result<Vec<SemanticRule>> {
        // Use LLM to extract patterns from clustered failures
        // Prompt: "These forecasts failed for similar reasons. What pattern do you see?"
        todo!()
    }
    
    async fn verify_rule(&self, rule: &SemanticRule, agent_id: Uuid) -> Result<VerificationResult> {
        // Combined verification approach
        
        // 1. Contradiction check (fast)
        if self.verifier.check_contradiction(rule, agent_id).await? {
            return Ok(VerificationResult::Rejected("Contradicts existing rules".to_string()));
        }
        
        // 2. Historical validation
        let historical_score = self.verifier.validate_historical(rule, agent_id).await?;
        
        // 3. Counterfactual scenarios (if confidence < threshold)
        if historical_score < 0.8 {
            let cf_score = self.verifier.validate_counterfactual(rule).await?;
            
            if cf_score < 0.8 {
                return Ok(VerificationResult::Rejected(
                    format!("Low confidence: historical={}, counterfactual={}", historical_score, cf_score)
                ));
            }
        }
        
        Ok(VerificationResult::Verified)
    }
    
    async fn extract_knowledge_graph(&self, agent_id: Uuid, episodes: &[Episode]) -> Result<(Vec<Entity>, Vec<Fact>)> {
        // Extract entities and relationships from episodes
        self.entity_extractor.extract(agent_id, episodes).await
    }
}

pub enum VerificationResult {
    Verified,
    Rejected(String),
}

pub struct ConsolidationResult {
    pub agent_name: String,
    pub episodes_processed: usize,
    pub clusters_identified: usize,
    pub rules_extracted: usize,
    pub rules_verified: usize,
    pub entities_created: usize,
    pub facts_created: usize,
    pub git_commit_sha: String,
}
```

### Dependencies

```toml
# fermi-consolidate/Cargo.toml

[package]
name = "fermi-consolidate"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "fermi-consolidate"
path = "src/main.rs"

[dependencies]
fermi-memory = { path = "../fermi-memory" }
fermi-ontology = { path = "../fermi-ontology" }
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
scopeguard = "1"
```

---

## Bidirectional Linking Strategy

### Agent Card Structure

```json
{
  "agent_id": "uuid",
  "agent_name": "market_research",
  "current_ontology_commit": "abc123def456",
  "current_ontology_snapshot_id": "uuid-snapshot",
  "last_consolidated_at": "2026-02-06T02:00:00Z"
}
```

### Ontology Snapshot Structure

```sql
SELECT * FROM ontology_snapshots WHERE snapshot_id = 'uuid-snapshot';

-- Returns:
-- snapshot_id: uuid-snapshot
-- agent_id: uuid (links back to agent)
-- git_commit_sha: abc123def456
-- git_path: agents/curated/market_research/ontology.mermaid
-- mermaid_content: "erDiagram..."
```

### Query Patterns

**Get agent's current ontology:**
```rust
let agent = memory_store.get_agent(agent_id).await?;
let snapshot = memory_store.get_snapshot(agent.current_ontology_snapshot_id).await?;
let mermaid = snapshot.mermaid_content;
```

**Get ontology at specific time (temporal query):**
```rust
let snapshot = memory_store.get_snapshot_at_time(agent_id, timestamp).await?;
```

**Get ontology from git commit:**
```rust
let snapshot = memory_store.get_snapshot_by_commit(agent_id, "abc123").await?;
```

### Race Condition Prevention

**Scenario 1: Concurrent Consolidation Attempts**
- Worker A tries to consolidate agent X
- Worker B tries to consolidate agent X simultaneously
- **Solution:** Lock acquisition - only first worker proceeds

```rust
// Worker A
let lock = ConsolidationLock::new(memory_store, "worker-a");
if lock.acquire(agent_id, 60).await? {
    // Consolidate
} else {
    // Skip, another worker is consolidating
}

// Worker B (concurrent)
let lock = ConsolidationLock::new(memory_store, "worker-b");
if lock.acquire(agent_id, 60).await? {
    // This returns false, Worker B skips
}
```

**Scenario 2: Agent Execution During Consolidation**
- Consolidation is writing entities/facts
- Agent execution tries to query knowledge graph
- **Solution:** Read operations don't block, write atomicity via transactions

```rust
// Consolidation (write)
let mut tx = pool.begin().await?;
// All inserts within transaction
sqlx::query!("INSERT INTO entities ...").execute(&mut tx).await?;
sqlx::query!("INSERT INTO facts ...").execute(&mut tx).await?;
tx.commit().await?; // Atomic

// Agent execution (read) - sees consistent state
let entities = memory_store.get_current_entities(agent_id).await?;
```

**Scenario 3: Git Commit Conflicts**
- Worker A commits ontology.mermaid
- Worker B commits ontology.mermaid (different agent, same time)
- **Solution:** Separate files per agent, no conflicts

```
agents/curated/market_research/ontology.mermaid  (Worker A)
agents/curated/sentiment_analyzer/ontology.mermaid  (Worker B)
```

**Scenario 4: Snapshot ID vs Git SHA Mismatch**
- Database transaction commits
- Git push fails
- **Solution:** Two-phase commit pattern

```rust
// Phase 1: Database transaction
let snapshot_id = memory_store.create_snapshot(...).await?;

// Phase 2: Git commit
match git_manager.commit_ontology(...).await {
    Ok(git_commit) => {
        // Update with git SHA
        memory_store.update_snapshot_git_ref(snapshot_id, &git_commit.sha).await?;
    }
    Err(e) => {
        // Rollback: mark snapshot as failed
        memory_store.mark_snapshot_failed(snapshot_id).await?;
        return Err(e);
    }
}
```

---

## Implementation Roadmap

### Phase 1: Foundation (Week 1)
- [x] Design PostgreSQL schema
- [ ] Create `fermi-memory` crate skeleton
- [ ] Implement basic database connection
- [ ] Implement episode storage
- [ ] Write integration tests

### Phase 2: Episodic Memory (Week 2)
- [ ] Implement episode clustering (DBSCAN)
- [ ] Add vector search (pgvector)
- [ ] Add full-text search integration
- [ ] Implement distributed locking
- [ ] Test race condition scenarios

### Phase 3: Semantic Memory (Week 3)
- [ ] Create `fermi-ontology` crate
- [ ] Implement rule storage
- [ ] Implement entity/fact storage
- [ ] Add bi-temporal queries
- [ ] Test temporal consistency

### Phase 4: Consolidation Worker (Week 4)
- [ ] Create `fermi-consolidate` binary
- [ ] Implement clustering logic
- [ ] Add rule extraction (LLM integration)
- [ ] Add verification (3-stage)
- [ ] Test end-to-end consolidation

### Phase 5: Mermaid & Git (Week 5)
- [ ] Implement Mermaid generation
- [ ] Add git commit automation
- [ ] Test bidirectional linking
- [ ] Add snapshot versioning
- [ ] Create ontology evolution queries

### Phase 6: Migration (Week 6)
- [ ] Migrate market_research agent
- [ ] Migrate sentiment_analyzer agent
- [ ] Run first consolidation
- [ ] Verify ontology generation
- [ ] Test retrieval

### Phase 7: Vercel Integration (Week 7)
- [ ] Create API endpoints (fermi-api)
- [ ] Deploy to Vercel
- [ ] Connect to Vercel Postgres
- [ ] Test from Vercel functions
- [ ] Deploy consolidation worker externally

---

## Success Criteria

### Phase 1-3 (Foundation)
- ✅ Agents write episodes to PostgreSQL
- ✅ Episodes have embeddings
- ✅ Distributed locks prevent races
- ✅ Bi-temporal queries work correctly

### Phase 4-5 (Consolidation)
- ✅ Clustering identifies failure patterns
- ✅ Rules extracted and verified
- ✅ Entities and facts extracted
- ✅ Mermaid diagrams generated
- ✅ Git commits automated
- ✅ Bidirectional linking works

### Phase 6-7 (Production)
- ✅ Agents migrated and operational
- ✅ Daily consolidation runs successfully
- ✅ Ontologies evolve over time
- ✅ API endpoints return current knowledge
- ✅ Vercel deployment stable
- ✅ No race conditions in production

---

## Next Steps

1. **Create Vercel Postgres database**
2. **Run schema creation script**
3. **Start implementing fermi-memory crate**
4. **Set up development environment**

Ready to proceed?
