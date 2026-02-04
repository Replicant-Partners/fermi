# ADR-002: Rust Backend Rebuild

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** Project team  
**Related:** ADR-001 (Architecture), [MODULE_ARCHITECTURE.md](../roadmap/MODULE_ARCHITECTURE.md)

---

## Context

We have an existing `uffp-backend` written in Node.js/TypeScript with the following components:
- Agent coordination
- Database (PostgreSQL)
- User authentication
- Basic API endpoints

As we build the Fermi Forecasting IDE, we need to decide whether to:
1. **Extend** the existing Node.js backend
2. **Rebuild** in Rust from scratch
3. **Migrate** incrementally (dual systems during transition)

**Current uffp-backend status:**
- ~2,000 lines of TypeScript
- Working but exploratory prototype
- Some technical debt
- Not optimized for the new FPL-centric workflow

**Constraints:**
- Want single-language stack (Rust for LSP + executor already)
- Need high performance for agent coordination
- Want type safety and reliability
- Clean slate opportunity to fix design issues

---

## Decision

We will **rebuild the backend from scratch in Rust**.

The new Fermi Backend will:
- Use a Rust web framework (axum or actix-web)
- Clean PostgreSQL schema designed for FPL
- Type-safe API with Rust type system
- No code ported from Node.js (fresh design)

---

## Consequences

### Positive Consequences

✅ **Single Language Stack**
- All components in Rust (LSP, executor, backend, extensions)
- Share code between components (e.g., AST types, validation)
- One build system, one dependency manager
- Team only needs Rust expertise

✅ **Type Safety**
- Rust's type system prevents entire classes of bugs
- Compile-time guarantees for API contracts
- No runtime type errors (unlike TypeScript's `any`)
- Better refactoring confidence

✅ **Performance**
- Rust is significantly faster than Node.js (~10-100x for some workloads)
- Lower memory usage (important for agent coordination)
- Better CPU utilization for concurrent requests
- Native async/await without GC pauses

✅ **Clean Slate**
- Fix design issues from uffp-backend
- Schema designed for FPL from day one
- No legacy code or technical debt
- Modern API design patterns

✅ **Reliability**
- Memory safety without garbage collection
- No null/undefined errors
- Fearless concurrency
- Better error handling with Result types

### Negative Consequences

❌ **Development Time**
- Rebuild takes longer than extending existing code
- Estimated 2-3 weeks to reach feature parity
- No "quick wins" from reusing Node.js code

❌ **Learning Curve** (if team not fluent in Rust)
- Web frameworks in Rust are less mature than Express.js
- Async Rust can be complex
- Fewer Stack Overflow answers for Rust web dev

❌ **Lost Work**
- Existing uffp-backend becomes throwaway prototype
- Some design decisions need to be re-made
- Tests need to be rewritten

❌ **Ecosystem**
- Fewer Rust libraries for some tasks (compared to npm)
- Some LLM client libraries may only have Node.js versions
- Need to find Rust equivalents for tools

### Neutral Consequences

⚖️ **Database Migration**
- Opportunity to redesign schema
- Will need migration scripts for any existing data
- Can start fresh with better design

⚖️ **API Changes**
- Can redesign API to be FPL-centric
- Breaking changes from uffp-backend API
- Extensions will need to use new API (but they're new too)

---

## Alternatives Considered

### Alternative 1: Extend Node.js Backend

Keep `uffp-backend` in TypeScript, add new features.

**Pros:**
- Fastest short-term (no rebuild)
- Reuse existing code
- Team may already know Node.js
- Large npm ecosystem

**Cons:**
- Split language stack (Node.js + Rust)
- TypeScript's type safety is weaker than Rust
- Performance limitations for agent coordination
- Technical debt from prototype persists
- Can't share AST types with LSP easily

**Why not:** The backend is small enough (~2K lines) that rebuilding is feasible, and the benefits of Rust (type safety, performance, single stack) outweigh the short-term speed of extending Node.js.

---

### Alternative 2: Incremental Migration

Run both backends simultaneously, migrate feature-by-feature.

**Pros:**
- Lower risk (can fall back to Node.js)
- Gradual migration
- Keep working features during rebuild

**Cons:**
- Highest complexity (two backends running)
- Need to keep both in sync during migration
- Database schema conflicts
- Deployment complexity
- Extends timeline significantly

**Why not:** The uffp-backend is a prototype, not production. We're not serving users yet, so we don't need the safety net of dual systems. Clean break is simpler.

---

### Alternative 3: Keep Node.js, Use Rust Only for LSP

Accept a multi-language stack.

**Pros:**
- Can extend uffp-backend quickly
- Use best tool for each job
- Node.js is great for APIs
- Rust is great for language servers

**Cons:**
- Can't share types between backend and LSP
- Need expertise in both ecosystems
- More complex deployment
- Harder to share code

**Why not:** The backend isn't that large, and we're starting fresh. Single-language simplicity wins.

---

### Alternative 4: Use Different Language Entirely (Go, Python, etc.)

Consider other languages for backend.

**Pros (Go):**
- Great for web services
- Fast, simple
- Good concurrency

**Pros (Python):**
- Huge ML/AI ecosystem
- Easy to integrate LLM libraries
- Fast development

**Cons (Both):**
- Still multi-language stack
- Can't share AST types with Rust LSP
- Need different deployment tooling

**Why not:** If we're rebuilding anyway, Rust makes the most sense given we're already using it for LSP and executor.

---

## Implementation Notes

### Rust Web Framework Choice

We'll decide between:
- **axum** (modern, Tower-based, active development)
- **actix-web** (mature, battle-tested, slightly faster)

**Decision deferred to:** ADR-004 (or Module 5 planning)

### Database Schema

New schema will be designed for FPL:
```sql
forecasts (
    id, user_id, fpl_code, version, created_at, updated_at
)
executions (
    id, forecast_id, results_json, iterations, duration_ms, created_at
)
agents (
    id, name, type, config_json, ontology_version
)
agent_calls (
    id, agent_id, forecast_id, query, response, status, tokens, cost, created_at
)
tournaments (
    id, name, question, deadline, resolution_date, scoring_method
)
```

**See:** Module 5 design docs for full schema

### Migration Plan

**Phase 1:** Build backend skeleton (auth, database)  
**Phase 2:** Add agent registry (ACP integration)  
**Phase 3:** Add agent coordinator (async execution)  
**Phase 4:** Add collaboration features (tournaments, sharing)  
**Phase 5:** Production deployment

**Timeline:** Estimated 2-3 weeks for Phase 1-3

### Code Sharing Strategy

Share code between LSP and Backend:
- **AST types** - Both need to understand FPL structure
- **Validation rules** - Backend can re-validate before execution
- **Type system** - Consistent type checking

**Create:** `fermi-core` crate with shared types

### Testing Approach

**Unit tests:**
- API endpoint tests
- Database access layer tests
- Agent coordinator tests

**Integration tests:**
- Full API flow tests
- Mock LLM APIs
- Test database (separate from production)

**Load tests:**
- Tournament with 100+ participants
- Concurrent agent executions
- Database query performance

---

## References

- Existing uffp-backend: `../uffp-backend/` (for reference)
- Rust web frameworks comparison: [axum vs actix](https://www.lpalmieri.com/posts/2020-12-11-zero-to-production-6-persist-data/)
- ADR-001: Architecture Option C

---

## Revision History

- **2026-02-04:** Initial version - Status: Accepted
