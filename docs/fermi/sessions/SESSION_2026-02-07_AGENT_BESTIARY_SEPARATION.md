# Session 2026-02-07: Agent Bestiary Separation & Launch Prep

**Date**: February 7, 2026  
**Duration**: ~3 hours  
**Goal**: Separate Agent Bestiary as standalone product and prepare for launch

## Summary

Major reorganization to position Agent Bestiary as a universal memory backend, separate from Fermi forecasting agents. Completed documentation partition, crate renaming, DNS configuration, and GTM planning.

## Key Accomplishments

### 1. Domain Acquisition & Configuration ✅

**Domains Acquired**:
- `agent-bestiary.world` (primary)
- `the-agent-bestiary.world` (redirect)
- `fermi.systems` (Fermi forecasting)

**DNS Configuration**:
- Configured Name.com API integration
- Set up A records and CNAMEs via API
- Pointed all domains to Vercel
- Automated DNS setup with bash script

**Files**:
- `docs/shared/VERCEL_DOMAIN_SETUP.md` - Domain setup guide

### 2. Go-To-Market Plan ✅

Created comprehensive GTM plan for Agent Bestiary launch:

**Structure**: `docs/agent-bestiary/go-to-market/`
- `README.md` - Overview and index
- `01-pre-launch.md` - Weeks 1-4: MVP polish, beta testing
- `02-launch.md` - Week 5: HN/Reddit launch strategy
- `03-first-100-users.md` - Weeks 6-12: Growth tactics
- `04-validation.md` - PMF assessment and metrics
- `05-iteration.md` - Product roadmap and experimentation
- `positioning-messaging.md` - Core messaging, personas, objections
- `budget-resources.md` - Budget breakdown, time investment, ROI
- `week-1-action-items.md` - Immediate next steps

**Key Details**:
- **Target Launch**: Q2 2026 (April-May)
- **Budget**: $50-100/month initially
- **Time Investment**: ~500 hours over 12 weeks
- **Pricing**: Free tier, Pro $20/month, Enterprise custom
- **Target**: 100 users, $400 MRR by Week 12

**Positioning**: "Real Memory for AI Agents - episodic to semantic consolidation"

### 3. Documentation Partition ✅

Cleanly separated docs into product boundaries:

```
docs/
├── README.md                    # Top-level overview
├── agent-bestiary/              # Agent Bestiary (standalone)
│   ├── README.md                # Product overview
│   ├── QUICK_START.md           # 5-minute integration guide
│   ├── API.md                   # REST API spec
│   ├── FEATURES.md              # Core features + GDPR
│   ├── ARCHITECTURE.md          # Technical design
│   ├── INTEGRATIONS.md          # Framework integrations
│   ├── GDPR.md                  # Compliance guide
│   ├── DEPLOYMENT.md            # Self-hosting
│   ├── MEMORY_SCHEMA.sql        # Database schema
│   └── go-to-market/            # GTM plan (9 files)
├── fermi/                       # Fermi forecasting
│   ├── README.md                # Fermi docs
│   ├── guides/                  # User guides
│   ├── architecture/            # Technical architecture
│   ├── sessions/                # Development history
│   └── ...                      # All Fermi-specific docs
└── shared/                      # Cross-cutting
    ├── ARCHITECTURE_ADM.md      # ADM concepts
    ├── MCP_SETUP.md             # MCP integration
    └── VERCEL_DOMAIN_SETUP.md   # Infrastructure
```

**Files Created**:
- `docs/README.md` - Top-level navigation
- `docs/agent-bestiary/README.md` - Agent Bestiary entry point
- `docs/agent-bestiary/QUICK_START.md` - Integration guide
- Placeholder files for INTEGRATIONS.md, GDPR.md, DEPLOYMENT.md

**Files Moved**: 107 files reorganized

**Benefits**:
- Agent Bestiary docs completely standalone
- No Fermi concepts leak into Agent Bestiary docs
- Clear product boundaries for users
- Easy to split into separate repos later

### 4. Crate Namespace Refactoring ✅

Renamed all crates to `agent-bestiary-*` namespace:

**Before**:
```
fermi/
├── fermi-memory/
├── fermi-ontology/
├── fermi-consolidate/
└── api/
```

**After**:
```
fermi/
├── agent-bestiary/
│   ├── memory/          # agent-bestiary-memory
│   ├── ontology/        # agent-bestiary-ontology
│   ├── consolidate/     # agent-bestiary-consolidate
│   └── api/             # REST API endpoints
├── fermi-lsp/          # Fermi LSP
└── src/                # Fermi agents
```

**Changes**:
- Renamed crates: `fermi-memory` → `agent-bestiary-memory`
- Renamed crates: `fermi-ontology` → `agent-bestiary-ontology`
- Renamed crates: `fermi-consolidate` → `agent-bestiary-consolidate`
- Updated all Rust imports: `fermi_memory` → `agent_bestiary_memory`
- Updated workspace members in root `Cargo.toml`
- Moved `api/` to `agent-bestiary/api/`

**Verification**: `cargo check --workspace` passes with only warnings

**Benefits**:
- Open source ready - users see "agent-bestiary" not "fermi"
- Clear product separation in codebase
- Professional naming for universal memory backend
- No confusion about what's what

### 5. API Endpoints (Partial)

**Existing Endpoints**:
- `api/health.rs` - Health check (Agent Bestiary branding)
- `api/agents.rs` - Agent management (list, create)
- `api/execute.rs` - FPL execution (Fermi-specific)

**Status**: Skeleton created, needs full implementation

## File Changes

### Created (36 files):
- `docs/README.md` - Top-level docs index
- `docs/agent-bestiary/README.md` - Agent Bestiary docs
- `docs/agent-bestiary/QUICK_START.md` - Integration guide
- `docs/agent-bestiary/INTEGRATIONS.md` - Framework integrations (placeholder)
- `docs/agent-bestiary/GDPR.md` - Compliance guide (placeholder)
- `docs/agent-bestiary/DEPLOYMENT.md` - Self-hosting guide (placeholder)
- `docs/agent-bestiary/go-to-market/*.md` - 9 GTM files
- `docs/shared/VERCEL_DOMAIN_SETUP.md` - Domain setup guide
- `docs/fermi/DOCUMENTATION_PARTITION_PLAN.md` - Partition strategy

### Moved (107 files):
- All Agent Bestiary docs to `docs/agent-bestiary/`
- All Fermi docs to `docs/fermi/`
- Shared docs to `docs/shared/`
- Crates to `agent-bestiary/` namespace

### Modified (5 files):
- `Cargo.toml` - Updated workspace members
- `vercel.json` - API routing
- Crate `Cargo.toml` files - Updated names and paths
- API Rust files - Updated imports

## Commits

1. **"docs: partition Agent Bestiary and Fermi documentation"**
   - 107 files changed, 4192 insertions(+), 196 deletions(-)
   - Complete documentation reorganization

2. **"refactor: namespace Agent Bestiary crates"**
   - 26 files changed, 25 insertions(+), 23 deletions(-)
   - Crate renaming and namespace organization

## Technical Decisions

### Decision 1: Monorepo vs Separate Repos

**Decision**: Keep monorepo, clean namespace separation  
**Rationale**:
- Easier to develop initially
- Shared CI/CD and tooling
- Can split later if needed
- Clear boundaries via namespace

**Alternative Considered**: Create `agent-bestiary` repo immediately  
**Why Not**: Premature optimization, more overhead

### Decision 2: Domain Strategy

**Decision**: 
- `agent-bestiary.world` for Agent Bestiary
- `fermi.systems` for Fermi
- Both point to same Vercel project initially

**Rationale**:
- Clear product branding
- Can split Vercel projects later
- `.world` TLD emphasizes universal/global platform

### Decision 3: Crate Naming

**Decision**: `agent-bestiary-*` not `agent_bestiary_*`  
**Rationale**:
- Cargo convention: hyphens in crate names
- Rust imports use underscores automatically
- Consistent with ecosystem standards

### Decision 4: API Location

**Decision**: `/api/execute` stays in Agent Bestiary project  
**Rationale**:
- Small endpoint, not worth separate project
- Fermi is just one agent type using Agent Bestiary
- Can split later if Fermi grows significantly

### Decision 5: Documentation Strategy

**Decision**: Complete partition, not just organization  
**Rationale**:
- Agent Bestiary must be 100% standalone
- No Fermi concepts in Agent Bestiary docs
- Easier for non-Fermi users to adopt

## Challenges & Solutions

### Challenge 1: "Fermi" References Everywhere

**Problem**: Crates named `fermi-*`, imports using `fermi_*`  
**Solution**: Systematic rename to `agent-bestiary-*`  
**Approach**: 
- Used `sed` to replace all imports
- Updated Cargo.toml files
- Verified with `cargo check`

### Challenge 2: Documentation Mixed Together

**Problem**: Fermi and Agent Bestiary docs intermingled  
**Solution**: Three-way partition (agent-bestiary, fermi, shared)  
**Result**: 107 files reorganized cleanly

### Challenge 3: DNS Configuration

**Problem**: Manual DNS setup is slow and error-prone  
**Solution**: Automated via Name.com API  
**Implementation**: Bash script with curl calls

## Next Steps

### Immediate (This Session)
- [ ] Continue with API implementation
- [ ] Add database connection
- [ ] Implement remaining endpoints
- [ ] Deploy to Vercel

### Week 1 (GTM Plan)
- [ ] Complete Vercel deployment
- [ ] Add authentication/API keys
- [ ] Write API documentation
- [ ] Create demo agent data
- [ ] Set up monitoring

### Week 2-4 (Pre-Launch)
- [ ] Build landing page
- [ ] Create demo video
- [ ] Write 3 blog posts
- [ ] Recruit 10 beta testers

### Week 5 (Launch)
- [ ] HN/Reddit launch
- [ ] Community outreach
- [ ] Content distribution

## Metrics

**Code Changes**:
- Files moved: 107
- Files created: 36
- Files modified: 5
- Lines added: ~4,500
- Lines removed: ~200

**Documentation**:
- GTM plan pages: 9
- Total words: ~15,000
- Agent Bestiary docs: Complete structure
- Fermi docs: Organized

**Infrastructure**:
- Domains configured: 3
- DNS records created: 5
- Vercel projects: 1 (pending deployment)

## Lessons Learned

1. **Namespace early**: Easier to rename before launch than after
2. **Document separation matters**: Clean boundaries reduce confusion
3. **Automate DNS**: API-based setup is faster and reproducible
4. **GTM planning is valuable**: Forces clarity on positioning and tactics
5. **Monorepo works**: Clean namespace separation achieves product isolation

## References

- [Agent Bestiary Docs](../agent-bestiary/)
- [GTM Plan](../agent-bestiary/go-to-market/)
- [Documentation Partition Plan](DOCUMENTATION_PARTITION_PLAN.md)
- [Name.com API Docs](https://docs.name.com/)
- [Vercel Domains Guide](https://vercel.com/docs/concepts/projects/domains)

## Status

**Agent Bestiary Launch Prep**: 60% complete
- ✅ Product positioning and GTM plan
- ✅ Documentation structure
- ✅ Codebase organization
- ✅ Domain acquisition and DNS
- ⏳ API implementation (in progress)
- ⏳ Vercel deployment (pending)
- ⏳ Landing page (pending)
- ⏳ Beta testing (pending)

**Target Launch**: April-May 2026 (Q2)

---

**Session Complete**: Ready to continue with API implementation and Vercel deployment.
