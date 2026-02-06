# Fermi Active Dreaming Memory (ADM)

**Status:** Phase 0 Complete ✅  
**Next:** Phase 1 - Vector Search & Clustering

---

## Quick Start

### Test the system
```bash
cd /home/ilabra/fermi
cargo test --package fermi-memory
```

### Check database
```bash
export DATABASE_URL="postgresql://neondb_owner:npg_wAY2hyU3eHbK@ep-plain-term-ahgv8fhm-pooler.c-3.us-east-1.aws.neon.tech/neondb?sslmode=require"
psql $DATABASE_URL -c "\dt"
```

---

## Architecture

```
┌─────────────────────────────────────┐
│     Active Dreaming Memory          │
├─────────────────────────────────────┤
│                                     │
│  Wake Phase:   Agent executes       │
│                ↓                    │
│                Episodes (PostgreSQL)│
│                                     │
│  Sleep Phase:  Consolidation        │
│                ↓                    │
│                Rules + Knowledge    │
│                                     │
│  Retrieval:    Multi-modal search   │
│                                     │
└─────────────────────────────────────┘
```

---

## Key Documents

- **Architecture:** `docs/ARCHITECTURE_ADM.md`
- **Roadmap:** `docs/ROADMAP_ADM_IMPLEMENTATION.md`
- **Database Schema:** `docs/MEMORY_SCHEMA.sql`
- **Quick Start:** `docs/QUICK_START.md`
- **Phase 0 Summary:** `docs/SESSION_COMPLETE_ADM_PHASE_0.md`

---

## Current Status

### ✅ Phase 0 Complete (Feb 6, 2026)
- Neon PostgreSQL database created
- fermi-memory crate operational
- Core types implemented
- Episode storage working
- Tests passing

### 🔜 Phase 1 Next (Week 1)
- Vector embeddings
- Similarity search
- DBSCAN clustering
- Distributed locking

---

## Crate Structure

```
fermi-memory/
├── src/
│   ├── lib.rs       # Exports
│   ├── types.rs     # Episode, Agent, Entity, Fact
│   ├── store.rs     # MemoryStore (database ops)
│   └── error.rs     # Error types
└── Cargo.toml
```

---

## Usage Example

```rust
use fermi_memory::{MemoryStore, Episode, ExecutionStatus};
use uuid::Uuid;
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to database
    let store = MemoryStore::new(&database_url).await?;
    
    // Create agent
    let agent = Agent { /* ... */ };
    let agent_id = store.upsert_agent(agent).await?;
    
    // Store episode
    let episode = Episode {
        episode_id: Uuid::new_v4(),
        agent_id,
        timestamp_ref: Utc::now(),
        query: "What is AMD market share?".to_string(),
        execution_status: ExecutionStatus::Success,
        // ...
    };
    
    store.store_episode(episode).await?;
    
    // Retrieve unconsolidated episodes
    let episodes = store.get_unconsolidated_episodes(agent_id).await?;
    
    Ok(())
}
```

---

## Database Tables

```sql
agents                   -- Agent metadata
episodes                 -- Episodic memory (wake phase)
semantic_rules           -- Consolidated rules (sleep phase)
entities                 -- Knowledge graph nodes
facts                    -- Knowledge graph edges
communities              -- Entity clusters
ontology_snapshots       -- Mermaid ER snapshots
consolidation_jobs       -- Sleep phase tracking
verification_tests       -- Rule verification
consolidation_locks      -- Race prevention
```

---

## Environment Variables

Copy `.env.example` to `.env` and configure:

```bash
DATABASE_URL=postgresql://...
ANTHROPIC_API_KEY=sk-ant-...
REPO_PATH=/home/ilabra/fermi
```

---

## Next Session

**Start Phase 1:**
1. Add embedding generation (Anthropic/OpenAI API)
2. Implement vector similarity search
3. Add DBSCAN clustering
4. Implement distributed locking

**Reference:** `docs/ROADMAP_ADM_IMPLEMENTATION.md` - Phase 1 section

---

## Help

**Questions?** Review:
- Architecture doc for design decisions
- Roadmap for implementation plan
- Quick Start for getting started

**Issues?** Check:
- Database connection: `psql $DATABASE_URL`
- Tests: `cargo test --package fermi-memory`
- Build: `cargo check --workspace`

---

**Built with:** Rust 🦀 | PostgreSQL 🐘 | pgvector 🔍
