# ADR-001: Architecture Option C - Loose Coupling

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** Project team  
**Related:** [MODULE_ARCHITECTURE.md](../roadmap/MODULE_ARCHITECTURE.md)

---

## Context

We need to decide on the overall architecture for the Fermi Forecasting IDE. The system has three major components:

1. **FPL Language Server** - Core language intelligence (diagnostics, execution)
2. **Fermi Backend** - Agent coordination, storage, collaboration
3. **Zed Extensions** - UI layer for Zed editor

We need to determine how these components interact and how tightly coupled they should be.

**Constraints:**
- Must support independent development of each component
- Need to enable easy testing and deployment
- Want to avoid monolithic complexity
- Should allow swapping implementations (e.g., different backends)

---

## Decision

We will use **Architecture Option C: Clean Separation of Concerns** with loose coupling between components.

**Architecture:**
```
Zed Extensions (UI only)
    ↕ LSP Protocol (JSON-RPC)
FPL Language Server (standalone process)
    ↕ REST/WebSocket API
Fermi Backend (separate service)
    ↕ External APIs
LLM Services, MCP Servers, etc.
```

**Key Principles:**
1. **FPL Language Server** is a standalone process communicating only via LSP protocol
2. **Fermi Backend** is a separate service with REST/WebSocket API
3. **Zed Extensions** contain only UI logic, no business logic
4. Each component can be developed, tested, and deployed independently

---

## Consequences

### Positive Consequences

✅ **Independent Development**
- Can work on LSP without touching backend
- Can swap backend implementation without changing LSP
- Can test each component in isolation

✅ **Technology Flexibility**
- Can use different languages for different components (all Rust here, but could mix)
- Can deploy components separately (scale backend independently)
- Can version components independently

✅ **Testing & Debugging**
- Mock LSP client for testing language server
- Mock backend API for testing extensions
- Clear boundaries make bugs easier to isolate

✅ **Modularity**
- Prevents "monolith creep"
- Forces good API design
- Enables reuse (e.g., LSP could work with VS Code too)

### Negative Consequences

❌ **Network Latency**
- Extra hop between extension → LSP → backend adds latency
- Not an issue for most operations, but could affect real-time features

❌ **Complexity**
- Three separate processes to coordinate
- More moving parts to deploy and monitor
- Need to handle inter-process communication failures

❌ **Boilerplate**
- Need to define APIs between components
- More serialization/deserialization code
- More error handling for network issues

### Neutral Consequences

⚖️ **Development Workflow**
- Need to run multiple processes during development
- Need to coordinate versions across components
- Better separation of concerns, but requires discipline

⚖️ **Documentation**
- More API documentation needed
- Clearer module boundaries to document
- Each component needs its own docs

---

## Alternatives Considered

### Alternative A: Monolithic (All-in-one)

```
Single Zed Extension containing:
- Language server logic
- Backend logic  
- UI logic
All in one process
```

**Pros:**
- Simplest to start
- No network latency
- Easy debugging (single process)

**Cons:**
- Becomes monolithic and hard to maintain
- Can't scale components independently
- Testing is harder (tight coupling)
- Hard to swap implementations

**Why not:** Would lead to unmaintainable complexity as project grows. Goes against "modular complexity management" principle.

---

### Alternative B: LSP + Backend Merged

```
Zed Extension (UI only)
    ↕ LSP Protocol
Combined FPL Server (LSP + Backend in one process)
    ↕ External APIs
LLM Services, MCP Servers
```

**Pros:**
- One less process to manage
- Faster communication between LSP and backend logic
- Simpler deployment

**Cons:**
- LSP becomes heavyweight (agents, database, etc.)
- Can't scale LSP and backend independently
- Harder to test LSP in isolation
- Violates single responsibility principle

**Why not:** Makes the LSP do too much. Language servers should be focused on language features, not agents/tournaments/storage.

---

### Alternative D: Micro-services (10+ services)

```
Zed Extension
    ↕
API Gateway
    ↕
LSP Service | Agent Service | Storage Service | 
Tournament Service | Auth Service | ...
(Each module is a separate service)
```

**Pros:**
- Maximum flexibility
- Can scale each service independently
- Clear boundaries

**Cons:**
- Over-engineered for current scale
- Operations complexity (10+ services to deploy)
- Network overhead between services
- Distributed system problems (latency, failures)

**Why not:** Premature optimization. We can refactor to this later if needed, but starting here would slow development significantly.

---

## Implementation Notes

### Component Boundaries

**FPL Language Server:**
- Owns: Lexing, parsing, semantic analysis, local execution
- Exposes: LSP protocol (diagnostics, hover, completion, etc.)
- Calls: Nothing (standalone)
- Optional: Can call backend for heavy execution, but not required

**Fermi Backend:**
- Owns: Agent registry, coordination, database, tournaments, auth
- Exposes: REST API, WebSocket for real-time updates
- Calls: LLM APIs, MCP servers, agent ontology store

**Zed Extensions:**
- Owns: UI rendering, user interaction, panel management
- Exposes: Nothing (UI only)
- Calls: LSP (for language features), Backend API (for agents/collab)

### Communication Protocols

**Extension ↔ LSP:**
- Protocol: LSP (JSON-RPC over stdio)
- Synchronous for most requests
- Async notifications for diagnostics

**Extension ↔ Backend:**
- Protocol: REST (HTTPS) for mutations
- WebSocket for real-time updates (agent callbacks, leaderboards)
- JWT for authentication

**LSP ↔ Backend (optional):**
- Protocol: REST (HTTPS)
- Only for heavy execution (>100K iterations)
- LSP can work entirely standalone if backend unavailable

### Testing Strategy

**LSP Testing:**
- Unit tests for lexer, parser, semantic, executor
- LSP protocol tests with mock client
- No backend dependency

**Backend Testing:**
- Unit tests for business logic
- Integration tests with test database
- Mock LLM APIs for deterministic tests

**Extension Testing:**
- UI tests with mock LSP
- Mock backend API responses
- E2E tests with real LSP + backend (later)

### Deployment

**Development:**
- Run all three locally
- Use localhost for API calls
- Hot reload for rapid iteration

**Production:**
- LSP bundled with Zed extension
- Backend deployed separately (cloud service)
- Extensions download updates from marketplace

---

## References

- [MODULE_ARCHITECTURE.md](../roadmap/MODULE_ARCHITECTURE.md) - Detailed architecture
- [ROADMAP.md](../ROADMAP.md) - Implementation phases
- Session discussion: [SESSION_2026-02-04.md](../sessions/SESSION_2026-02-04.md)

---

## Revision History

- **2026-02-04:** Initial version - Status: Accepted
