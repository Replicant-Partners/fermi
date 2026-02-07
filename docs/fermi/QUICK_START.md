# Quick Start Guide - ADM Implementation

**Goal:** Get from design to first working code in under 30 minutes

---

## Prerequisites

- [ ] Rust toolchain installed (`rustc --version`)
- [ ] PostgreSQL client (`psql --version`)
- [ ] Git configured
- [ ] Text editor ready

---

## Step 1: Database Setup (5 minutes)

### Create Vercel Postgres Database

1. Go to https://vercel.com/dashboard
2. Click **Storage** tab
3. Click **Create Database**
4. Select **Postgres**
5. Name: `fermi-adm`
6. Region: Choose closest to you
7. Click **Create**

### Get Connection String

1. Click on your new database
2. Go to **Settings** tab
3. Copy the **Postgres URL** (the one that starts with `postgres://`)

### Initialize Schema

```bash
# Save connection URL
export DATABASE_URL="postgres://..."

# Run schema creation
psql $DATABASE_URL < docs/MEMORY_SCHEMA.sql

# Verify tables created
psql $DATABASE_URL -c "\dt"

# Should see: agents, episodes, semantic_rules, entities, facts, etc.
```

---

## Step 2: Environment Configuration (2 minutes)

```bash
# Copy example env file
cp .env.example .env

# Edit .env with your values
nano .env
# or
code .env
```

**Required values:**
- `DATABASE_URL` - From Step 1
- `ANTHROPIC_API_KEY` - Your existing API key
- `REPO_PATH` - Current directory (`pwd`)

**Optional (can leave defaults):**
- `WORKER_ID` - Unique worker identifier
- `CONSOLIDATION_TIME` - When to run daily consolidation

---

## Step 3: Install Dependencies (3 minutes)

```bash
# Install sqlx CLI (needed for compile-time query verification)
cargo install sqlx-cli --no-default-features --features postgres

# Prepare sqlx (generates query metadata)
cd /home/ilabra/fermi
export DATABASE_URL="postgres://..."  # Your connection string
sqlx database create  # Skip if database already exists
```

---

## Step 4: Create fermi-memory Crate (5 minutes)

```bash
# Create new crate
mkdir -p fermi-memory/src
cd fermi-memory

# Create Cargo.toml
cat > Cargo.toml << 'EOF'
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

[dev-dependencies]
tokio-test = "0.4"
EOF

# Create basic lib.rs
cat > src/lib.rs << 'EOF'
//! Fermi Active Dreaming Memory
//! 
//! Episodic and semantic memory storage for forecasting agents.

pub mod types;
pub mod store;
pub mod error;

pub use types::*;
pub use store::MemoryStore;
pub use error::{MemoryError, Result};
EOF

# Update workspace Cargo.toml
cd ..
```

Add to workspace `Cargo.toml`:
```toml
[workspace]
members = [
    "fermi-core",
    "fermi-memory",  # Add this line
    # ... other members
]
```

---

## Step 5: Implement Core Types (5 minutes)

```bash
cd fermi-memory

# Create types module
cat > src/types.rs << 'EOF'
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Episode (episodic memory entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Success,
    Failure,
    Partial,
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionStatus::Success => write!(f, "success"),
            ExecutionStatus::Failure => write!(f, "failure"),
            ExecutionStatus::Partial => write!(f, "partial"),
        }
    }
}
EOF

# Create error module
cat > src/error.rs << 'EOF'
use thiserror::Error;

pub type Result<T> = std::result::Result<T, MemoryError>;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
    
    #[error("Lock acquisition failed")]
    LockFailed,
}
EOF

# Create store module (basic skeleton)
cat > src/store.rs << 'EOF'
use sqlx::PgPool;
use crate::{Episode, Result, MemoryError};
use uuid::Uuid;

pub struct MemoryStore {
    pool: PgPool,
}

impl MemoryStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await?;
        Ok(Self { pool })
    }
    
    pub async fn store_episode(&self, episode: Episode) -> Result<Uuid> {
        // TODO: Implement in Phase 1
        todo!("Implement episode storage")
    }
    
    pub async fn get_episode(&self, episode_id: Uuid) -> Result<Episode> {
        // TODO: Implement in Phase 1
        todo!("Implement episode retrieval")
    }
}
EOF
```

---

## Step 6: Test Database Connection (5 minutes)

```bash
# Create test file
mkdir -p tests
cat > tests/connection_test.rs << 'EOF'
use fermi_memory::MemoryStore;

#[tokio::test]
async fn test_database_connection() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    let store = MemoryStore::new(&database_url)
        .await
        .expect("Failed to connect to database");
    
    println!("✅ Database connection successful!");
}
EOF

# Run test
export DATABASE_URL="postgres://..."  # Your connection string
cargo test --package fermi-memory test_database_connection

# Should see: "✅ Database connection successful!"
```

---

## Step 7: Verify Setup (5 minutes)

```bash
# Check workspace builds
cd /home/ilabra/fermi
cargo check --workspace

# Should see: "Finished dev [unoptimized + debuginfo]"

# Verify database has schema
psql $DATABASE_URL -c "SELECT COUNT(*) FROM agents"
# Should return: 0 (no agents yet)

# Check git status
git status
# Should show new files: fermi-memory/, .env.example, docs/QUICK_START.md
```

---

## Checkpoint: What We Have

✅ **Database:** Vercel Postgres with full schema  
✅ **Crate:** `fermi-memory` skeleton created  
✅ **Types:** Core types defined  
✅ **Connection:** Database connection tested  
✅ **Environment:** Configuration ready

---

## Next Steps: Phase 1 Implementation

Now you're ready to implement Phase 1 (Week 1):

1. **Day 1-2:** Implement episode storage
2. **Day 3-4:** Add episode retrieval
3. **Day 5-6:** Write comprehensive tests
4. **Day 7:** Documentation and integration tests

---

## Troubleshooting

### Database connection fails
```bash
# Test connection directly
psql $DATABASE_URL -c "SELECT 1"

# If fails, check:
# - DATABASE_URL is correct
# - Network allows connection
# - Database exists
```

### Cargo build errors
```bash
# Update Rust toolchain
rustup update

# Clean and rebuild
cargo clean
cargo build
```

### Schema creation fails
```bash
# Check if extensions available
psql $DATABASE_URL -c "CREATE EXTENSION IF NOT EXISTS vector"

# If fails, contact Vercel support for pgvector extension
```

---

## Success Criteria

Before moving to Phase 1 implementation, verify:

- [ ] Database created and accessible
- [ ] Schema fully deployed (all tables exist)
- [ ] `fermi-memory` crate compiles
- [ ] Database connection test passes
- [ ] Environment variables configured
- [ ] Git repository updated

**All checked?** You're ready to implement Phase 1! 🚀

---

## Quick Reference

**Database URL:** 
```bash
export DATABASE_URL="postgres://..."
```

**Test connection:**
```bash
psql $DATABASE_URL -c "\dt"
```

**Run tests:**
```bash
cargo test --package fermi-memory
```

**Check workspace:**
```bash
cargo check --workspace
```

---

**Need Help?** Review:
- `docs/ARCHITECTURE_ADM.md` - Full architecture
- `docs/ROADMAP_ADM_IMPLEMENTATION.md` - 8-week roadmap
- `docs/MEMORY_SCHEMA.sql` - Database schema
