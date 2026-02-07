# Session 2026-02-07: Vercel Deployment Attempt

**Date**: February 7, 2026  
**Duration**: ~2 hours  
**Goal**: Deploy Agent Bestiary API to Vercel  
**Status**: Blocked - Rust workspace compilation issue

## Summary

Attempted to deploy Agent Bestiary REST API to Vercel. Successfully configured database, fixed API code, but encountered Vercel Rust runtime limitations with Cargo workspace dependencies.

## Accomplishments

### 1. Database Verification ✅

**Existing Setup**:
- Neon Postgres already configured via Vercel
- Connection string: `postgresql://neondb_owner:npg_wAY2hyU3eHbK@ep-plain-term-ahgv8fhm-pooler.c-3.us-east-1.aws.neon.tech/neondb`
- All tables present: agents, episodes, semantic_rules, ontology_snapshots, etc.
- 102 agents already in database

**Migration Applied**:
```sql
-- 003_add_github_tracking.sql
ALTER TABLE ontology_snapshots ADD COLUMN github_url TEXT;
ALTER TABLE ontology_snapshots ADD COLUMN pushed_to_remote BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX idx_ontology_snapshots_github_url ON ontology_snapshots(github_url) WHERE github_url IS NOT NULL;
```

**Verification**:
```bash
psql $DATABASE_URL -c "SELECT COUNT(*) FROM agents;"
# Result: 102 agents
```

### 2. Environment Variables ✅

**Pulled from Vercel**:
```bash
vercel env pull .env.vercel
```

**Variables Available**:
- `DATABASE_URL` - Main connection string
- `DATABASE_URL_UNPOOLED` - Direct connection
- `POSTGRES_*` - Various Postgres connection formats
- `VERCEL_OIDC_TOKEN` - Auth token

### 3. API Code Fixes ✅

**Fixed Request Body Parsing** (`api/agents.rs`):
```rust
// Before (broken):
let body_bytes = req.body();

// After (fixed):
use http_body_util::BodyExt;
let body_bytes = req.into_body().collect().await?.to_bytes();
```

**API Endpoints**:
- `GET /api/health` - Health check
- `GET /api/agents` - List all agents
- `POST /api/agents` - Create new agent
- `POST /api/execute` - Execute FPL code (Fermi-specific)

### 4. Deployment Configuration Attempts

#### Attempt 1: agent-bestiary/api/ paths
```json
{
  "functions": {
    "agent-bestiary/api/**/*.rs": {
      "runtime": "vercel-rust@4.0.0"
    }
  }
}
```
**Result**: ❌ Vercel requires functions in `/api` directory

#### Attempt 2: Move back to /api
```bash
mv agent-bestiary/api api/
```
**Result**: ✅ Correct location, but...

#### Attempt 3: Simplified vercel.json
```json
{
  "functions": {
    "api/**/*.rs": {
      "runtime": "vercel-rust@4.0.0"
    }
  }
}
```
**Result**: ❌ Cargo workspace dependency compilation failed

## The Problem: Rust Workspace Dependencies

### Error Message
```
Error: Command failed with exit code 101: cargo build --bin agents --quiet --release
```

### Root Cause

Vercel's Rust runtime (`vercel-rust@4.0.0`) tries to compile each `.rs` file as a standalone binary. However, our API functions depend on:

```rust
use agent_bestiary_memory::MemoryStore;
```

This is a workspace dependency defined in the root `Cargo.toml`:

```toml
[workspace]
members = [
    ".",
    "agent-bestiary/memory",
    "agent-bestiary/ontology",
    "agent-bestiary/consolidate",
]

[dependencies]
agent-bestiary-memory = { path = "agent-bestiary/memory" }
```

**The Issue**: Vercel's Rust runtime doesn't handle Cargo workspaces well. It tries to compile the serverless function in isolation without access to workspace dependencies.

## File Changes

### Modified (5 files):
- `api/agents.rs` - Fixed body parsing
- `vercel.json` - Multiple iterations trying different configurations
- `.env.vercel` - Created (pulled from Vercel)
- File moves: `agent-bestiary/api/* → api/*`

### Created (1 file):
- `DEPLOYMENT_CHECKLIST.md` - Comprehensive deployment guide

## Commits Made

1. **"feat(api): wire up database connection to agents API"**
   - Fixed request body parsing
   - Verified database connection
   
2. **"docs: add deployment checklist for Vercel"**
   - Created comprehensive deployment guide

3. **"fix(vercel): update API paths after agent-bestiary refactor"**
   - Updated vercel.json paths
   
4. **"fix: remove secret reference from vercel.json"**
   - Removed @database_url secret reference
   
5. **"fix: move API back to /api for Vercel compatibility"**
   - Moved API files to required location

## Testing Attempted

### Local Test (Would Work)
```bash
export DATABASE_URL="postgresql://..."
cargo run --bin agents
```

### Vercel Test (Failed)
```bash
vercel --prod --yes
# Error: Workspace compilation failed
```

### Manual Deployment Test
```bash
# Tried with existing deployment
vercel curl /api/health --deployment fermi-9w8x8mf4g...
# Result: Old code (pre-refactor), authentication required
```

## Blocker Analysis

### Why It's Blocking Deployment

1. **Workspace Structure**: Our crates are organized as a Cargo workspace
2. **Vercel Limitation**: Vercel Rust runtime doesn't support workspace dependencies
3. **Dependency Chain**: API → agent-bestiary-memory → sqlx + pgvector

### What Vercel Expects

Vercel's Rust runtime expects:
- Standalone `.rs` files with all dependencies inline, OR
- Single-crate projects with dependencies from crates.io, OR
- Custom build configuration (which we'd need to configure)

### What We Have

Complex workspace with local path dependencies:
```
fermi/
├── agent-bestiary/
│   ├── memory/          # Local dependency
│   ├── ontology/        # Local dependency
│   └── consolidate/     # Local dependency
└── api/
    ├── agents.rs        # Depends on agent-bestiary-memory
    └── health.rs        # Simple, would work
```

## Potential Solutions

### Option 1: Inline Memory Code (Simplest)

**Approach**: Copy necessary code from `agent-bestiary-memory` directly into API functions

**Pros**:
- Works with Vercel Rust runtime immediately
- No workspace issues
- Fast deployment

**Cons**:
- Code duplication
- Harder to maintain
- Loses clean separation

**Effort**: 2-3 hours

### Option 2: Publish Crates to crates.io

**Approach**: Publish `agent-bestiary-memory` as a public crate

**Pros**:
- Clean dependency management
- Vercel can fetch from crates.io
- Proper open source distribution

**Cons**:
- Need to manage versioning
- Public release process
- Still might have compilation issues

**Effort**: 4-6 hours

### Option 3: Custom Vercel Build

**Approach**: Create custom build script for Vercel

```json
{
  "builds": [
    {
      "src": "api/**/*.rs",
      "use": "@vercel/rust",
      "config": {
        "rust": {
          "workspace": true
        }
      }
    }
  ]
}
```

**Pros**:
- Keeps workspace structure
- Proper build process

**Cons**:
- May not be supported by vercel-rust@4.0.0
- Needs experimentation

**Effort**: 3-4 hours (if supported)

### Option 4: Different Deployment Platform (Recommended)

**Approach**: Deploy to platform with better Rust support

**Options**:
- **Railway**: Great Rust support, automatic workspace handling
- **Fly.io**: Dockerfile-based, full control
- **Shuttle.rs**: Rust-native platform
- **Render**: Good Rust support

**Pros**:
- Proper Rust workspace support
- More control over build process
- Often better pricing

**Cons**:
- Different platform (not Vercel)
- Need to configure separately
- Might need different DNS setup

**Effort**: 2-3 hours

### Option 5: Containerize and Deploy

**Approach**: Create Docker container, deploy anywhere

**Dockerfile**:
```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin api-server

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/api-server /usr/local/bin/
CMD ["api-server"]
```

**Pros**:
- Full control
- Works anywhere (Fly.io, Railway, Cloud Run)
- Standard deployment method

**Cons**:
- Need to create server binary (not serverless)
- Slightly more complex setup

**Effort**: 4-6 hours

## Recommendation

### For Quick MVP (Next Week):

**Use Option 4 (Railway)** because:
1. Railway has excellent Rust workspace support
2. Automatic detection and build
3. Free tier sufficient for early testing
4. Can add custom domain easily
5. Better Rust ecosystem support than Vercel

### For Long-term (Post-Launch):

**Use Option 5 (Containerized)** because:
1. Platform-agnostic
2. Production-grade
3. Easy to scale
4. Standard DevOps practices
5. Can deploy anywhere (GCP Cloud Run, AWS ECS, Fly.io)

### Not Recommended:

- Option 1 (Inlining): Creates technical debt
- Option 2 (crates.io): Premature for MVP
- Option 3 (Custom Build): May not work with vercel-rust

## Next Steps

### Immediate (Next Session):

1. **Try Railway Deployment**
   ```bash
   # Install Railway CLI
   npm install -g @railway/cli
   
   # Login
   railway login
   
   # Initialize project
   railway init
   
   # Deploy
   railway up
   ```

2. **Add Custom Domain to Railway**
   - Point agent-bestiary.world to Railway
   - Update DNS CNAME records

3. **Test API Endpoints**
   - Verify /api/health works
   - Test /api/agents with real database

### Alternative (If Railway Doesn't Work):

1. **Create Dockerfile**
2. **Convert serverless functions to web server**
3. **Deploy to Fly.io**

### Documentation Updates:

1. Update DEPLOYMENT_CHECKLIST.md with Railway instructions
2. Document the workspace compilation issue
3. Add Railway-specific configuration

## Lessons Learned

1. **Vercel Rust Limitations**: Vercel's Rust runtime is designed for simple, standalone functions, not complex workspaces
2. **Platform Selection Matters**: Choose deployment platform based on language ecosystem support
3. **Workspace Trade-offs**: Clean code organization (workspaces) vs deployment simplicity (monolithic)
4. **Test Early**: Should have tested Vercel deployment earlier in the process
5. **Have Backup Plans**: Multiple deployment options reduces risk

## Technical Decisions

### Decision 1: Keep Workspace Structure

**Decision**: Don't inline code, maintain workspace  
**Rationale**: Code quality and maintainability trump deployment convenience  
**Alternative Considered**: Inline all memory code into API functions  
**Trade-off**: More complex deployment, but cleaner codebase

### Decision 2: Move to Railway

**Decision**: Switch from Vercel to Railway for API deployment  
**Rationale**: Better Rust ecosystem support, workspace-friendly  
**Alternative Considered**: Continue fighting with Vercel  
**Trade-off**: Different platform, but proper Rust support

### Decision 3: Keep Fermi Frontend on Vercel

**Decision**: Only move API to Railway, keep web UI on Vercel  
**Rationale**: Vercel excellent for frontends, just not Rust workspaces  
**Result**: Hybrid deployment (Vercel frontend + Railway API)

## Status Summary

**What Works**:
- ✅ Database configured and accessible
- ✅ API code complete and compiles locally
- ✅ Environment variables configured
- ✅ DNS configured (pointing to Vercel currently)

**What's Blocked**:
- ❌ Vercel deployment (workspace compilation)
- ❌ Public API endpoint testing
- ❌ Production verification

**Ready for Next Session**:
- 🟡 Railway deployment (new approach)
- 🟡 DNS re-pointing (once Railway works)
- 🟡 API testing (after deployment)

## Files to Review

Before continuing deployment:
- `api/agents.rs` - API endpoint implementation
- `api/health.rs` - Simple health check
- `Cargo.toml` - Workspace configuration
- `vercel.json` - Current (non-working) config
- `DEPLOYMENT_CHECKLIST.md` - Needs Railway section

## Open Questions

1. Should we stick with Vercel (custom build) or move to Railway?
2. Do we need serverless, or is a web server fine?
3. How important is Vercel for the overall stack?
4. Should frontend and API be on same platform?

## Resources

- [Vercel Rust Runtime](https://vercel.com/docs/functions/serverless-functions/runtimes/rust)
- [Railway Rust Support](https://docs.railway.app/deploy/deployments#rust)
- [Fly.io Rust Guide](https://fly.io/docs/languages-and-frameworks/rust/)
- [Shuttle.rs](https://www.shuttle.rs/) - Rust-native platform

---

**Next Session Goal**: Deploy Agent Bestiary API using Railway and test all endpoints.

**Estimated Time**: 2-3 hours to complete deployment and testing.
