# Active Dreaming Memory Implementation Roadmap

**Status:** Ready to Implement  
**Date:** 2026-02-06  
**Version:** 1.0

---

## Executive Summary

This roadmap outlines the complete implementation of Active Dreaming Memory (ADM) for Fermi forecasting agents, with PostgreSQL storage, Mermaid ER ontologies, and git-based version control.

**Timeline:** 7 weeks to production  
**Architecture:** Lightweight, modular, Rust end-to-end  
**Deployment:** Vercel (API) + External Worker (consolidation)

---

## Design Documents Reference

- **Architecture:** `/home/ilabra/fermi/docs/ARCHITECTURE_ADM.md`
- **Database Schema:** `/home/ilabra/fermi/docs/MEMORY_SCHEMA.sql`
- **Agent Bestiary Design:** `/home/ilabra/fermi/docs/AGENT_BESTIARY_DESIGN.md`

---

## Core Design Principles

### 1. ADM vs AKP Separation

**ADM (Active Dreaming Memory)** - Individual Agent Learning
- Agent learns from its own experiences
- Episodic → Semantic consolidation
- Builds personal ontology/worldview
- **Scope:** Phases 1-8 (this roadmap)

**AKP (Agent Knowledge Protocol)** - Inter-Agent Learning
- Agents learn from each other
- Ontology alignment across agents
- Shared meaning construction
- **Scope:** Phase 9+ (future, loosely coupled)

**Relationship:** ADM provides foundation, AKP builds on top

### 2. Architecture Decisions

✅ **Git as immutable event log** - Every consolidation = commit  
✅ **PostgreSQL as source of truth** - Dynamic data in DB, snapshots in git  
✅ **Bidirectional linking** - Agent card ↔ Git commit + DB snapshot  
✅ **External Rust worker** - No Vercel timeout constraints  
✅ **Fresh start for agents** - Clean migration, no historical backfill  
✅ **Combined verification** - Contradiction + Historical + Counterfactual

### 3. Race Condition Prevention

✅ **Distributed locks** via PostgreSQL functions  
✅ **Lock acquisition** with timeout and expiry  
✅ **Two-phase commit** for git + database consistency  
✅ **Transaction isolation** for atomic writes

### 4. Standardization for Future AKP

✅ **Single embedding model** across all agents  
✅ **Documented in agent card** (model, dimension, provider)  
✅ **Ensures cross-agent compatibility** for future ontology alignment

---

## Implementation Phases

### Phase 0: Environment Setup (Days 1-2)

**Objective:** Set up development environment and database

**Tasks:**
1. Create Vercel Postgres database
2. Run `MEMORY_SCHEMA.sql` to initialize schema
3. Set up environment variables
4. Create GitHub repository access (for git commits)
5. Install dependencies (sqlx-cli, etc.)

**Deliverables:**
- ✅ Database URL configured
- ✅ Schema deployed
- ✅ Environment ready for development

**Commands:**
```bash
# Install sqlx CLI
cargo install sqlx-cli --features postgres

# Set up database URL
export DATABASE_URL="postgres://user:pass@host/db"

# Run migrations
sqlx database create
psql $DATABASE_URL < docs/MEMORY_SCHEMA.sql

# Verify schema
psql $DATABASE_URL -c "\dt"
```

---

### Phase 1: Foundation - fermi-memory Crate (Week 1)

**Objective:** Create core memory abstraction layer

**Tasks:**

**Day 1-2: Crate Setup**
- [ ] Create `fermi-memory/` crate
- [ ] Add dependencies (sqlx, pgvector, tokio)
- [ ] Define core types (Episode, SemanticRule, Entity, Fact)
- [ ] Create error types

**Day 3-4: Database Connection**
- [ ] Implement `MemoryStore` struct
- [ ] Add connection pooling
- [ ] Write basic CRUD for episodes
- [ ] Add integration tests

**Day 5-6: Episode Storage**
- [ ] Implement `store_episode()`
- [ ] Add embedding generation (mock for now)
- [ ] Implement `get_unconsolidated_episodes()`
- [ ] Test episode queries

**Day 7: Testing & Documentation**
- [ ] Write comprehensive tests
- [ ] Add docstrings
- [ ] Create usage examples
- [ ] Integration test with real database

**Deliverables:**
```rust
// fermi-memory can store and retrieve episodes
let memory_store = MemoryStore::new(&database_url).await?;

let episode = Episode {
    agent_id,
    query: "What is AMD market share?".to_string(),
    execution_status: ExecutionStatus::Success,
    // ...
};

let episode_id = memory_store.store_episode(episode).await?;
let retrieved = memory_store.get_episode(episode_id).await?;
```

**Success Criteria:**
- ✅ Episodes written to PostgreSQL
- ✅ Episodes retrieved with filters
- ✅ All tests passing
- ✅ Documentation complete

---

### Phase 2: Episodic Memory - Clustering & Search (Week 2)

**Objective:** Implement vector search and episode clustering

**Tasks:**

**Day 1-2: Vector Embeddings**
- [ ] Integrate embedding API (OpenAI or Anthropic)
- [ ] Generate embeddings for episodes
- [ ] Store embeddings in pgvector column
- [ ] Test vector storage

**Day 3-4: Vector Search**
- [ ] Implement `search_similar_episodes()`
- [ ] Use pgvector cosine similarity
- [ ] Add hybrid search (vector + full-text)
- [ ] Test search accuracy

**Day 5-6: DBSCAN Clustering**
- [ ] Implement `cluster_episodes_dbscan()`
- [ ] Use pgvector for distance computation
- [ ] Test clustering on sample data
- [ ] Tune epsilon and min_samples

**Day 7: Distributed Locking**
- [ ] Implement `ConsolidationLock`
- [ ] Test lock acquisition/release
- [ ] Test lock expiry cleanup
- [ ] Test concurrent lock attempts

**Deliverables:**
```rust
// Vector search works
let results = memory_store
    .search_similar_episodes(agent_id, &query_embedding, 10)
    .await?;

// Clustering identifies patterns
let clusters = memory_store
    .cluster_episodes_dbscan(agent_id, 0.3, 3)
    .await?;

// Locking prevents races
let lock = ConsolidationLock::new(memory_store, "worker-1");
if lock.acquire(agent_id, 60).await? {
    // Consolidate
}
```

**Success Criteria:**
- ✅ Embeddings generated and stored
- ✅ Vector search returns relevant results
- ✅ Clustering groups similar episodes
- ✅ Locks prevent concurrent consolidation

---

### Phase 3: Semantic Memory - Rules & Knowledge Graph (Week 3)

**Objective:** Implement semantic rule storage and knowledge graph

**Tasks:**

**Day 1-2: Semantic Rules**
- [ ] Implement `store_semantic_rule()`
- [ ] Add verification status tracking
- [ ] Implement rule retrieval
- [ ] Test rule storage

**Day 3-4: Entity Storage**
- [ ] Implement `create_entity()`
- [ ] Add bi-temporal tracking
- [ ] Implement entity updates (versioning)
- [ ] Test temporal queries

**Day 5-6: Fact Storage**
- [ ] Implement `create_fact()`
- [ ] Add relationship cardinality
- [ ] Implement fact invalidation
- [ ] Test bi-temporal consistency

**Day 7: Knowledge Graph Queries**
- [ ] Implement BFS graph traversal
- [ ] Add entity resolution queries
- [ ] Create view for current knowledge graph
- [ ] Test graph queries

**Deliverables:**
```rust
// Rules stored and retrieved
let rule = SemanticRule {
    rule_content: "AMD forecasts need sentiment data".to_string(),
    confidence_score: 0.85,
    verification_status: VerificationStatus::Verified,
    // ...
};
memory_store.store_semantic_rule(rule).await?;

// Knowledge graph built
let entity = Entity {
    entity_name: "AMD".to_string(),
    entity_type: "company".to_string(),
    // ...
};
let entity_id = memory_store.create_entity(entity).await?;

let fact = Fact {
    source_entity_id: amd_id,
    target_entity_id: gpu_market_id,
    relation_type: "COMPETES_IN".to_string(),
    relation_cardinality: Cardinality::ManyToOne,
    // ...
};
memory_store.create_fact(fact).await?;
```

**Success Criteria:**
- ✅ Rules stored with verification status
- ✅ Entities created with temporal tracking
- ✅ Facts link entities correctly
- ✅ Temporal queries return correct versions

---

### Phase 4: fermi-ontology Crate (Week 4)

**Objective:** Implement Mermaid generation and git integration

**Tasks:**

**Day 1-2: Crate Setup**
- [ ] Create `fermi-ontology/` crate
- [ ] Add dependencies
- [ ] Define core types (GitCommit, MermaidDiagram)
- [ ] Create error types

**Day 3-4: Mermaid Generation**
- [ ] Implement `MermaidGenerator`
- [ ] Generate ER diagram from entities/facts
- [ ] Add cardinality symbols
- [ ] Test Mermaid output validity

**Day 5-6: Git Integration**
- [ ] Implement `GitManager`
- [ ] Write ontology files
- [ ] Create detailed commit messages
- [ ] Test git operations

**Day 7: Ontology Snapshots**
- [ ] Implement `create_ontology_snapshot()`
- [ ] Store Mermaid content in database
- [ ] Link to git commit SHA
- [ ] Test bidirectional linking

**Deliverables:**
```rust
// Mermaid generation works
let mermaid_gen = MermaidGenerator::new(memory_store);
let mermaid = mermaid_gen.generate(agent_id).await?;

// Git commits automated
let git_manager = GitManager::new(repo_path);
let commit = git_manager.commit_ontology(
    "market_research",
    &mermaid,
    &stats,
).await?;

// Snapshot stored
let snapshot_id = memory_store.create_ontology_snapshot(
    agent_id,
    &commit.sha,
    &mermaid,
    job_id,
).await?;
```

**Success Criteria:**
- ✅ Valid Mermaid ER diagrams generated
- ✅ Git commits created with detailed messages
- ✅ Snapshots stored in database
- ✅ Bidirectional links established

---

### Phase 5: Consolidation Worker - Basic Flow (Week 5)

**Objective:** Implement consolidation orchestration

**Tasks:**

**Day 1-2: Binary Setup**
- [ ] Create `fermi-consolidate/` binary crate
- [ ] Add dependencies
- [ ] Implement config loading
- [ ] Create main loop with scheduling

**Day 3-4: Consolidation Job**
- [ ] Implement `Consolidator` struct
- [ ] Add job creation/tracking
- [ ] Implement episode fetching
- [ ] Test job lifecycle

**Day 5-6: Rule Extraction (Mock)**
- [ ] Implement `extract_rules_from_cluster()` with mock LLM
- [ ] Add rule validation logic
- [ ] Store extracted rules
- [ ] Test rule extraction flow

**Day 7: End-to-End Test**
- [ ] Run full consolidation pipeline
- [ ] Verify episodes → clusters → rules
- [ ] Check database state
- [ ] Test lock handling

**Deliverables:**
```rust
// Consolidation worker runs
let consolidator = Consolidator::new(memory_store, mermaid_gen, git_manager);
let result = consolidator.consolidate_agent(agent_id).await?;

// Result shows progress
println!("Episodes processed: {}", result.episodes_processed);
println!("Rules extracted: {}", result.rules_extracted);
println!("Git commit: {}", result.git_commit_sha);
```

**Success Criteria:**
- ✅ Worker acquires lock successfully
- ✅ Episodes clustered
- ✅ Rules extracted (mock)
- ✅ Job tracked in database

---

### Phase 6: LLM Integration & Verification (Week 6)

**Objective:** Add real LLM integration and verification

**Tasks:**

**Day 1-2: LLM Client**
- [ ] Implement Anthropic API client
- [ ] Add retry logic and error handling
- [ ] Test API connectivity
- [ ] Handle rate limiting

**Day 3-4: Rule Extraction**
- [ ] Replace mock with real LLM extraction
- [ ] Design prompts for pattern detection
- [ ] Parse LLM responses
- [ ] Test on real episode clusters

**Day 5-6: Combined Verification**
- [ ] Implement contradiction checker
- [ ] Add historical validation
- [ ] Add counterfactual scenarios
- [ ] Test verification pipeline

**Day 7: Entity Extraction**
- [ ] Implement entity extraction from episodes
- [ ] Add entity resolution (LLM-based)
- [ ] Extract facts/relationships
- [ ] Test knowledge graph building

**Deliverables:**
```rust
// Real LLM extraction
let rules = extractor.extract_rules_from_cluster(&cluster).await?;

// Verification works
let result = verifier.verify_rule(&rule, agent_id).await?;
match result {
    VerificationResult::Verified => {
        memory_store.store_rule(rule).await?;
    }
    VerificationResult::Rejected(reason) => {
        warn!("Rule rejected: {}", reason);
    }
}

// Knowledge graph extracted
let (entities, facts) = extractor.extract_knowledge_graph(agent_id, &episodes).await?;
```

**Success Criteria:**
- ✅ LLM generates meaningful rules
- ✅ Verification filters invalid rules
- ✅ Entities extracted accurately
- ✅ Knowledge graph built correctly

---

### Phase 7: Agent Migration (Week 7)

**Objective:** Migrate existing agents to ADM system

**Tasks:**

**Day 1-2: Agent Card Migration**
- [ ] Migrate `market_research` agent card to database
- [ ] Migrate `sentiment_analyzer` agent card
- [ ] Add embedding model metadata
- [ ] Test agent retrieval

**Day 3-4: Integration with Agent Execution**
- [ ] Modify agent executor to write episodes
- [ ] Add episode generation after each execution
- [ ] Test episode storage during execution
- [ ] Verify embeddings generated

**Day 5: First Consolidation**
- [ ] Run consolidation manually for both agents
- [ ] Verify rules extracted
- [ ] Check knowledge graph built
- [ ] Verify Mermaid files generated
- [ ] Check git commits

**Day 6: Automated Scheduling**
- [ ] Set up cron/systemd timer for daily consolidation
- [ ] Test scheduled execution
- [ ] Monitor logs
- [ ] Verify no errors

**Day 7: Validation & Cleanup**
- [ ] Review generated ontologies
- [ ] Validate temporal queries
- [ ] Test retrieval in agent execution
- [ ] Document operational procedures

**Deliverables:**
```bash
# Agents in database
psql $DATABASE_URL -c "SELECT agent_name, agent_type FROM agents"
# Returns: market_research, sentiment_analyzer

# Episodes accumulated
psql $DATABASE_URL -c "SELECT agent_id, COUNT(*) FROM episodes GROUP BY agent_id"

# First consolidation successful
ls agents/curated/market_research/ontology.mermaid
git log --oneline | grep "agent(market_research)"

# Scheduled worker running
systemctl status fermi-consolidate
```

**Success Criteria:**
- ✅ Both agents migrated successfully
- ✅ Episodes written during execution
- ✅ First consolidation generates ontologies
- ✅ Git history shows consolidation commits
- ✅ Automated scheduling works

---

### Phase 8: Vercel Integration (Week 8)

**Objective:** Deploy API to Vercel, connect to PostgreSQL

**Tasks:**

**Day 1-2: API Crate Setup**
- [ ] Create `fermi-api/` crate
- [ ] Add Vercel Rust runtime dependencies
- [ ] Implement API endpoints (list_agents, get_agent, execute_agent)
- [ ] Test locally

**Day 3-4: Database Connection**
- [ ] Connect to Vercel Postgres from API
- [ ] Implement agent state queries
- [ ] Add knowledge graph retrieval endpoints
- [ ] Test database connectivity

**Day 5: Vercel Deployment**
- [ ] Configure `vercel.json`
- [ ] Deploy API functions
- [ ] Test endpoints from Vercel
- [ ] Verify PostgreSQL connection

**Day 6: MCP Server Update**
- [ ] Update MCP server to use PostgreSQL backend
- [ ] Test MCP tools in Zed
- [ ] Verify retrieval works
- [ ] Test agent execution

**Day 7: Documentation**
- [ ] Write deployment guide
- [ ] Document API endpoints
- [ ] Create operational runbook
- [ ] Update README

**Deliverables:**
```bash
# API deployed to Vercel
curl https://fermi.vercel.app/api/agents
# Returns: list of agents

curl https://fermi.vercel.app/api/agents/market_research
# Returns: agent details with current ontology

# MCP server uses PostgreSQL
# In Zed:
"List available agents" → Returns agents from database
"Execute market_research with query X" → Writes episode to database
```

**Success Criteria:**
- ✅ API deployed and accessible
- ✅ Database queries work from Vercel
- ✅ MCP server uses PostgreSQL backend
- ✅ End-to-end flow functional

---

## Technical Stack Summary

### Core Technologies
- **Language:** Rust (2021 edition)
- **Database:** PostgreSQL (Vercel Postgres)
- **Vector Search:** pgvector extension
- **Async Runtime:** tokio
- **Database Client:** sqlx
- **Git Integration:** git CLI via Command
- **API Runtime:** Vercel Rust runtime

### Key Crates
```toml
# Workspace Cargo.toml
[workspace]
members = [
    "fermi-core",
    "fermi-memory",
    "fermi-ontology",
    "fermi-consolidate",
    "fermi-api",
    "fermi-mcp",
    "fermi-agent-backend",
    "fermi-lsp",
]
```

### Dependencies
- `sqlx` - PostgreSQL async client
- `pgvector` - Vector operations
- `tokio` - Async runtime
- `uuid` - Unique identifiers
- `chrono` - Timestamps
- `serde` / `serde_json` - Serialization
- `anyhow` / `thiserror` - Error handling
- `tracing` - Logging

---

## Environment Variables

### Development
```bash
# Database
DATABASE_URL="postgres://user:pass@localhost:5432/fermi_dev"

# API Keys
ANTHROPIC_API_KEY="sk-ant-..."

# Git
REPO_PATH="/home/ilabra/fermi"
GIT_USER_NAME="Fermi Bot"
GIT_USER_EMAIL="bot@fermi.dev"

# Worker
WORKER_ID="dev-worker-1"
CONSOLIDATION_TIME="02:00"
```

### Production
```bash
# Database (Vercel Postgres)
DATABASE_URL="postgres://user:pass@vercel.postgres.com/fermi_prod"

# API Keys
ANTHROPIC_API_KEY="sk-ant-..."

# Git (GitHub)
REPO_PATH="/app/fermi"
GITHUB_TOKEN="ghp_..."

# Worker (External)
WORKER_ID="prod-worker-1"
CONSOLIDATION_TIME="02:00"
AUTO_PUSH="true"
```

---

## Success Metrics

### Phase 1-3 (Foundation)
- [ ] Episodes stored in PostgreSQL
- [ ] Vector search returns relevant results
- [ ] Knowledge graph queries work
- [ ] All tests passing (>90% coverage)

### Phase 4-6 (Consolidation)
- [ ] Mermaid diagrams generated
- [ ] Git commits automated
- [ ] Rules extracted by LLM
- [ ] Verification filters invalid rules

### Phase 7 (Migration)
- [ ] Both agents migrated
- [ ] First consolidation successful
- [ ] Ontologies evolve over time
- [ ] No data loss

### Phase 8 (Production)
- [ ] API deployed to Vercel
- [ ] Daily consolidation runs successfully
- [ ] MCP server functional
- [ ] No race conditions observed

---

## Risk Mitigation

### Risk 1: LLM API Costs
**Mitigation:**
- Use cheaper models for verification (Haiku)
- Cache rule extractions
- Batch consolidations
- Set cost alerts

### Risk 2: Database Performance
**Mitigation:**
- Index all foreign keys
- Use connection pooling
- Monitor query performance
- Optimize slow queries

### Risk 3: Git Conflicts
**Mitigation:**
- Separate files per agent
- Lock-based serialization
- Retry logic for conflicts
- Manual resolution procedures

### Risk 4: Lock Timeouts
**Mitigation:**
- Configurable timeout (60 min default)
- Automatic lock expiry cleanup
- Monitor lock status
- Alert on stuck locks

### Risk 5: Consolidation Failures
**Mitigation:**
- Transaction rollback on errors
- Retry logic for transient failures
- Job status tracking
- Alert on repeated failures

---

## Monitoring & Operations

### Key Metrics to Track
- Episodes accumulated (per agent)
- Consolidation success rate
- Rule verification rate
- Entity/fact growth
- Ontology snapshot size
- Git commit frequency
- API response times
- Database query latency

### Alerting
- Consolidation failures (3+ consecutive)
- Lock timeouts (>1 hour)
- Database connection failures
- API error rate >5%
- Disk space <10%

### Logs to Collect
- Consolidation job logs
- LLM API request/response
- Git operation logs
- Database query logs
- API access logs

---

## Post-Implementation (Phase 9+)

### AKP (Agent Knowledge Protocol) Extension

**When:** After Phase 8 is stable and operational

**Scope:**
- Cross-agent ontology alignment
- Entity equivalence detection
- Knowledge transfer between agents
- Consensus building
- Agent-to-agent communication

**New Components:**
- `fermi-akp/` crate
- Cross-agent alignment tables
- Inter-agent communication protocol
- Shared knowledge consensus layer

**Benefits:**
- Faster agent bootstrapping
- Ensemble forecasting
- Collective intelligence
- Gap detection and targeted learning

---

## Getting Started Checklist

### Day 1 - Environment Setup
- [ ] Create Vercel Postgres database
- [ ] Note database connection URL
- [ ] Run `MEMORY_SCHEMA.sql`
- [ ] Verify schema with `\dt`
- [ ] Set up environment variables
- [ ] Install Rust toolchain (if needed)
- [ ] Install sqlx-cli
- [ ] Clone/pull latest code

### Day 2 - First Code
- [ ] Create `fermi-memory/` directory
- [ ] Initialize `Cargo.toml`
- [ ] Add dependencies
- [ ] Create `src/lib.rs` with core types
- [ ] Write first test
- [ ] Verify database connection

### Week 1 Goal
- [ ] Episode storage working
- [ ] Basic queries functional
- [ ] Tests passing
- [ ] Ready for Phase 2

---

## Contact & Support

**Questions?** Refer to:
- `/home/ilabra/fermi/docs/ARCHITECTURE_ADM.md` - Detailed architecture
- `/home/ilabra/fermi/docs/MEMORY_SCHEMA.sql` - Database schema
- `/home/ilabra/fermi/docs/AGENT_BESTIARY_DESIGN.md` - Original vision

**Need Help?** Review:
- Phase objectives
- Deliverables
- Success criteria

---

## Conclusion

This roadmap provides a clear path from design to production for Active Dreaming Memory in Fermi. Follow the phases sequentially, validate deliverables at each step, and maintain the lightweight, modular architecture throughout.

**Ready to build!** 🚀

---

**Document Version:** 1.0  
**Last Updated:** 2026-02-06  
**Status:** Ready for Implementation
