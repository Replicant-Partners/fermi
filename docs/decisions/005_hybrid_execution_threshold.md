# ADR-005: Hybrid Execution Model with 100K Iteration Threshold

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** ilabra, Claude  
**Related:** ADR-001 (Architecture Option C), Module 1 Q1.5

## Context

FPL forecasts use Monte Carlo simulation, which can range from 10K iterations (quick local execution) to 10M+ iterations (requiring backend resources). We need to decide where forecast execution should happen: always local, always remote, or hybrid with intelligent routing.

**Key Constraints:**
- Local execution: Fast startup (<100ms), no network latency, but limited to CPU/memory
- Backend execution: Scalable to massive iterations, can coordinate multiple agents, but has network overhead
- User experience: Users shouldn't need to think about where execution happens

**Performance Benchmarks (estimated):**
```
10K iterations:   ~50ms local   vs  ~300ms backend (network overhead dominates)
100K iterations:  ~500ms local  vs  ~800ms backend (comparable)
1M iterations:    ~5s local     vs  ~2s backend (backend parallelization wins)
10M iterations:   ~50s local    vs  ~5s backend (backend significantly faster)
```

**Current Architecture:** Option C provides clean separation between LSP (local) and Backend (remote), making hybrid routing straightforward.

## Decision

We will implement a **hybrid execution model with a 100K iteration threshold**:

- **Forecasts with <100K iterations:** Execute locally in LSP
- **Forecasts with ≥100K iterations:** Execute on backend
- **Agent-involved forecasts:** Always execute on backend (regardless of iteration count)
- **User override:** Settings allow forcing local or remote execution

**Implementation:**

```rust
// In FPL Language Server
pub enum ExecutionStrategy {
    Local,
    Backend,
    Hybrid { threshold: usize },
}

pub fn determine_execution_location(
    forecast: &Forecast,
    config: &ExecutionConfig,
) -> ExecutionLocation {
    // User override takes precedence
    if let Some(override_location) = config.force_execution_location {
        return override_location;
    }
    
    // Agent-involved forecasts must run on backend
    if forecast.uses_agents() {
        return ExecutionLocation::Backend;
    }
    
    // Hybrid routing based on iteration count
    match config.strategy {
        ExecutionStrategy::Hybrid { threshold } => {
            if forecast.iterations < threshold {
                ExecutionLocation::Local
            } else {
                ExecutionLocation::Backend
            }
        }
        ExecutionStrategy::Local => ExecutionLocation::Local,
        ExecutionStrategy::Backend => ExecutionLocation::Backend,
    }
}
```

**Default Configuration:**
```toml
[execution]
strategy = "hybrid"
threshold = 100000  # 100K iterations
force_location = null  # User can override with "local" or "backend"
local_max_duration = 30000  # Kill local execution after 30s
backend_timeout = 300000  # Backend can run up to 5 minutes
```

## Consequences

### Positive

1. **Optimal Performance:** Fast startup for common cases (most forecasts use 10-50K iterations), scalability for heavy computation
2. **Transparent:** Users don't need to think about execution location - it "just works"
3. **Resource Efficient:** Don't waste backend resources on trivial forecasts, don't overload local CPU on heavy ones
4. **Agent Coordination:** All agent-based forecasts run on backend where agent orchestration happens
5. **Future-Proof:** As backend improves, we can lower threshold or add smarter heuristics

### Negative

1. **Complexity:** Need to maintain execution logic in both LSP (local) and backend (remote)
2. **Testing Burden:** Must test both execution paths for every FPL feature
3. **Debugging Confusion:** Users might not know where their forecast is running when debugging issues
4. **Network Dependency:** Backend execution fails if network is down (but graceful fallback to local is possible)

### Neutral

1. **Configuration Surface:** Users can override if they want explicit control
2. **Monitoring Need:** Should track which forecasts run where for optimization

## Alternatives Considered

### A. Always Local Execution
**Pros:** Simple, no network dependency, predictable latency  
**Cons:** Can't scale to large simulations, can't coordinate agents, blocks editor during long runs  
**Rejected Because:** Eliminates key advantage of having a backend - scalability and agent coordination

### B. Always Backend Execution
**Pros:** Consistent behavior, all features available, easier to monitor  
**Cons:** Network latency for simple forecasts, requires internet, higher cost  
**Rejected Because:** Poor UX for simple forecasts - waiting 300ms when local could do it in 50ms

### D. User Configurable (No Smart Default)
**Pros:** Maximum control, explicit behavior  
**Cons:** Requires users to understand performance trade-offs, decision fatigue  
**Rejected Because:** We want forecasting IDE, not systems programming IDE - hide complexity

## Implementation Notes

### Phase 1 (MVP)
1. Implement local execution in LSP for basic forecasts (no agents)
2. Implement backend execution API
3. Add simple threshold-based routing (100K iterations)
4. Add status indicator showing where forecast is running

### Phase 2 (Optimization)
1. Add heuristics beyond iteration count:
   - Forecast complexity (nested loops, many drivers)
   - Historical execution time for this forecast
   - Current system load (CPU/memory)
2. Implement graceful fallback (backend failure → retry locally if possible)
3. Add execution analytics (track performance, adjust threshold)

### Phase 3 (Advanced)
1. Predictive routing using ML (predict execution time based on forecast AST)
2. Hybrid execution (start local, migrate to backend if taking too long)
3. Result caching (identical forecasts don't re-execute)

### Configuration Files

**LSP Configuration:**
```json
// .fermi/lsp-config.json
{
  "execution": {
    "strategy": "hybrid",
    "threshold": 100000,
    "local_timeout_ms": 30000,
    "cache_results": true
  }
}
```

**User Settings (Zed):**
```json
// settings.json
{
  "fermi": {
    "execution": {
      "force_location": null,  // null | "local" | "backend"
      "show_execution_location": true  // Show indicator in status bar
    }
  }
}
```

## References

- Module 1 Q1.5: Execution Model
- ADR-001: Architecture Option C (enables clean local/remote separation)
- ADR-002: Rust Backend Rebuild (backend must support execution API)
- Performance benchmarks: https://github.com/ilabra/fermi/docs/benchmarks/execution_latency.md (TODO)

## Metrics to Track

- **Routing Accuracy:** % of forecasts that chose optimal location (based on actual execution time)
- **Local Execution Time:** p50, p95, p99 for local forecasts
- **Backend Execution Time:** p50, p95, p99 for backend forecasts (including network)
- **Backend Fallback Rate:** How often do we fall back to local when backend is unavailable?
- **User Overrides:** How often do users manually force location? (Indicates routing issues)

## Future Considerations

- **Edge Cases:** What if forecast.iterations is dynamic (depends on data)? Currently assumes static iteration count.
- **Bandwidth Optimization:** For backend execution, compress results before sending to client
- **Partial Results:** Stream intermediate results during long backend runs (progress updates)
- **Cost Tracking:** Track backend execution costs, warn users for expensive forecasts
