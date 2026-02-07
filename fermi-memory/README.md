# Fermi Memory - Active Dreaming Memory (ADM)

Core memory infrastructure for Fermi forecasting agents.

## Phase 1 Progress: Foundation (Days 1-2) ✅

**Status:** Core types, structure, and database connected  
**Completion:** ~90% of Phase 1 Day 1-2 objectives

**Database:** Connected to Neon PostgreSQL (shared with Agent Bestiary)  
**Schema:** All ADM tables deployed and verified

### Completed
- ✅ Crate structure created (`fermi-memory/`)
- ✅ Dependencies configured (sqlx, tokio, uuid, chrono, serde)
- ✅ Core types defined:
  - `Episode` - Episodic memory (individual executions)
  - `SemanticRule` - Consolidated knowledge rules
  - `Entity` - Knowledge graph entities
  - `Relationship` - Knowledge graph relationships  
  - `Fact` - Atomic knowledge pieces
- ✅ `MemoryStore` abstraction with connection pooling
- ✅ Episode storage and retrieval methods
- ✅ Semantic rule storage and retrieval methods
- ✅ Error types and result handling
- ✅ Comprehensive documentation

### Next Steps (Phase 1 Day 3-4)

**Database Setup Required:**
```bash
# 1. Set up PostgreSQL database
export DATABASE_URL="postgresql://localhost/fermi"

# 2. Run schema migration
psql $DATABASE_URL < ../docs/agent-bestiary/MEMORY_SCHEMA.sql

# 3. Prepare sqlx offline data
cargo sqlx prepare

# 4. Build successfully
cargo build
```

**Then continue with:**
- [ ] Integration tests with real database
- [ ] Add embedding generation (mock for now)
- [ ] Complete health check implementation
- [ ] Entity and Relationship CRUD operations

## Architecture

```
┌─────────────────────────────────────────────────┐
│  Wake Phase (Episodic Memory)                   │
│  - Store individual agent executions            │
│  - Track metrics, context, results              │
│  - Episodes remain unconsolidated               │
└─────────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────────┐
│  Sleep Phase (Consolidation)                    │
│  - Cluster similar episodes                     │
│  - Extract semantic rules                       │
│  - Update knowledge graph                       │
│  - Verify and commit to git                     │
└─────────────────────────────────────────────────┘
```

## Usage Example

```rust
use fermi_memory::{MemoryStore, Episode, ExecutionStatus};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to database
    let store = MemoryStore::new("postgresql://localhost/fermi").await?;

    // Store an episode
    let agent_id = Uuid::new_v4();
    let episode = Episode::new(
        agent_id,
        "What is AMD's market share?".to_string(),
        serde_json::json!({"result": "15%"}),
        ExecutionStatus::Success,
    );

    let episode_id = store.store_episode(episode).await?;
    
    // Retrieve unconsolidated episodes
    let episodes = store.get_unconsolidated_episodes(agent_id, 100).await?;
    
    println!("Stored {} unconsolidated episodes", episodes.len());
    
    Ok(())
}
```

## Database Schema

See `/docs/agent-bestiary/MEMORY_SCHEMA.sql` for complete PostgreSQL schema including:
- `episodes` - Episodic memory with bi-temporal tracking
- `semantic_rules` - Consolidated knowledge rules
- `entities` - Knowledge graph entities
- `relationships` - Entity relationships
- `facts` - Atomic knowledge pieces
- `consolidation_jobs` - Async consolidation tracking

## Roadmap

- **Phase 1** (Week 1): Foundation - `fermi-memory` crate ✅ (80%)
- **Phase 2** (Week 2): Embedding & search
- **Phase 3** (Week 3): Consolidation engine
- **Phase 4** (Week 4): Git integration
- **Phase 5** (Week 5): Mermaid ontology visualization
- **Phase 6** (Week 6): Verification module
- **Phase 7** (Week 7): API & integration

## Notes

**Rust 1.85 Compatibility:**
- Uses workspace-level patches for `time` and `home` dependencies
- Compatible with Railway deployment environment

**SQLx Compile-Time Checking (Known Issue):**
- `sqlx::query!()` macros require compile-time database verification
- Currently blocked by `sqlx-cli` requiring Rust 1.88+ (we use 1.85 for Railway)
- **Workaround:** Tests use runtime queries via `sqlx::query()` instead
- **Status:** Functional for development, will resolve post-Railway migration to newer Rust

**Alternative Approach (if needed):**
```rust
// Instead of compile-time checked:
let result = sqlx::query!("SELECT * FROM episodes").fetch_all(&pool).await?;

// Use runtime queries:
let result = sqlx::query("SELECT * FROM episodes")
    .fetch_all(&pool)
    .await?;
```

**Testing:**
- Integration tests marked with `#[ignore]` until database available
- Run with `cargo test -- --ignored` once database configured
