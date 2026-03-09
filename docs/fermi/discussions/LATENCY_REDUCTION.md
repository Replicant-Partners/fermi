# Latency Reduction — Making Fermi Feel Tight

**Date:** 2026-03-09
**Status:** Design proposal
**Goal:** Reduce perceived and actual latency so the forecasting workflow feels responsive

---

## Current Latency Profile

| Step | Actual | Perceived | Bottleneck |
|------|--------|-----------|------------|
| Fermi decomposition (Ctrl+Enter) | 25–35s | Painful | Sonnet + 4096 tokens + ABW round-trip |
| Research agent execution | 15–30s each | Tolerable if async | Sonnet + tool loop (1–3 iterations) |
| Multiple agents (3–5 drivers) | 60–150s total | Unacceptable sequential | Agents fire one-at-a-time per driver |
| Local simulation (Ctrl+R) | <100ms | Instant ✅ | — |
| Save + git commit (Ctrl+S) | <500ms | Instant ✅ | — |
| Load forecast from disk | <200ms | Instant ✅ | — |

**The critical path:** Question → Fermi decomposition (30s) → assign agents → research (30s each) → simulate → insight. Total: **2–5 minutes** before the user has a complete picture.

**The 30-credit problem:** The initial decomposition costs ~30 credits ($2–3) and takes 30 seconds. If the result is vague or generic, the user feels robbed. The first response MUST deliver visible, specific value.

---

## Strategy 1: Faster Decomposition Model

### Problem
Fermi uses `claude-sonnet-4-5-20250929` for decomposition. Sonnet is accurate but slow (25–35s for structured JSON).

### Solution
Use Haiku for the initial scaffolding, Sonnet for research depth.

**Two-phase decomposition:**
1. **Phase 1 (Haiku, 3–5s):** Generate base rate + driver names + rationale. This is a classification/structuring task that doesn't need deep reasoning. Populate the UI immediately.
2. **Phase 2 (Sonnet, 20–30s):** Fire Sonnet in the background to refine: adjust driver parameters (p5/p50/p95), provide initial evidence, suggest agents. Update the UI when it completes.

**User experience:**
- 0s: "Fermi is decomposing…" banner
- 3–5s: Base rate appears, 4–5 driver cards populate (skeleton data)
- 5s: User can already start editing drivers, assigning agents
- 20–30s: Sonnet results arrive, driver parameters update with evidence

**Implementation:**
```rust
// Phase 1: Haiku — fast scaffolding
fire_agent_with_model("fermi", &scaffold_query, "claude-3-haiku-20240307", cx);

// Phase 2: Sonnet — deep decomposition (parallel, updates in place)
fire_agent_with_model("fermi", &refine_query, "claude-sonnet-4-5-20250929", cx);
```

**Cost:** Haiku call ~2 credits + Sonnet call ~15 credits = ~17 credits (vs 30 for Sonnet-only). Faster AND cheaper.

**ABW changes needed:** Add optional `model` field to execute request, or create a separate `fermi_scaffold` agent on Haiku.

### Impact: Perceived latency from 30s → 5s for initial population

---

## Strategy 2: Streaming Agent Results (SSE)

### Problem
ABW returns agent results as a single JSON blob after the entire execution completes. The user stares at a loading spinner for 25–30 seconds with no feedback.

### Solution
Stream partial results from ABW via Server-Sent Events (SSE).

**What to stream:**
1. **Token-by-token reasoning** — the LLM's text appears live, like ChatGPT
2. **Tool call progress** — "Searching knowledge base…", "Analyzing data…"
3. **Partial evidence** — each key finding appears as it's generated
4. **Confidence updates** — probability estimate refines as reasoning develops

**ABW endpoint:**
```
POST /api/agents/:id/execute/stream
Accept: text/event-stream

data: {"type": "reasoning_delta", "text": "The base rate for"}
data: {"type": "tool_call", "tool": "search_knowledge", "status": "running"}
data: {"type": "tool_result", "tool": "search_knowledge", "summary": "Found 3 relevant entries"}
data: {"type": "evidence", "finding": "NBA home teams win 58% of games"}
data: {"type": "complete", "result": { ... full result JSON ... }}
```

**Console integration:**
```rust
// Use eventsource-client (already in Cargo.toml)
let stream = api.execute_agent_stream(&agent_id, &query).await;
while let Some(event) = stream.next().await {
    this.update(cx, |state, cx| {
        match event.event_type {
            "reasoning_delta" => state.update_agent_reasoning(&agent_id, &event.text),
            "evidence" => state.add_incremental_evidence(&agent_id, &event),
            "complete" => state.process_agent_result(&agent_id, &event.result),
        }
        cx.notify(); // UI updates live
    });
}
```

**User experience:**
- 0s: Agent badge shows "researching…"
- 1–2s: First words of reasoning appear in a live text area
- 5–10s: First evidence finding pops in
- 15–25s: Complete result finalizes

### Impact: Perceived latency from "30s black hole" → "watching the agent think"

---

## Strategy 3: Parallel Agent Execution

### Problem
When the user assigns agents to multiple drivers, each agent fires and completes before the next one's results are visible. With 5 drivers, that's 5 × 25s = 2+ minutes of sequential waiting.

### Current state
Agents already fire in parallel via `cx.spawn()` + `tokio::spawn`. But the UI updates are per-agent — each agent's results appear as a batch when it completes. The problem is **perceived** sequentiality because the UI doesn't show partial progress well.

### Solution
- **Stagger visual updates:** Show each agent's speech bubble and evidence count as they complete, with a satisfying "pop" animation.
- **Pre-assign agents automatically:** After Fermi decomposes, auto-assign the recommended agent to each driver immediately (don't wait for user to click "+ Assign Agent"). User can override later.
- **Batch execute:** "Research All" button that fires all assigned agents simultaneously.

**Auto-assign flow:**
```
Fermi decomposes → 5 drivers appear
  → For each driver, data-driven matcher picks best agent
  → All 5 agents fire simultaneously (parallel ABW calls)
  → Results stream back as each completes (fastest first)
  → User sees evidence appearing on different drivers in real-time
```

**Implementation:**
```rust
fn auto_assign_and_fire_all(&mut self, cx: &mut Context<Self>) {
    let orchestra = self.discover_research_agents();
    for driver in self.program.drivers() {
        let best_agent = self.score_agent_match(&driver, &orchestra);
        self.assign_agent_to_driver(&driver.name, &best_agent, Schedule::Once, cx);
        // assign_agent_to_driver already fires the agent
    }
}
```

### Impact: Total research time from 5×30s=150s → max(30s)=30s (all parallel)

---

## Strategy 4: Aggressive Caching

### Problem
Same questions or similar decompositions hit the LLM every time. "Will the Lakers win their next game?" produces nearly identical base rates and driver structures each time.

### Solution
Cache decomposition results by question similarity.

**Cache levels:**
1. **Exact match cache:** Same question text → return cached decomposition instantly
2. **Embedding similarity cache:** Question embedding within cosine distance 0.95 → return cached decomposition with freshness flag
3. **Driver template cache:** "NBA game prediction" template → pre-built driver structure, skip LLM entirely

**Implementation (local SQLite):**
```sql
CREATE TABLE decomposition_cache (
    question_hash TEXT PRIMARY KEY,
    question_text TEXT,
    embedding BLOB,
    decomposition_json TEXT,
    created_at TIMESTAMP,
    hit_count INTEGER DEFAULT 0
);
```

**Freshness:** Cached decompositions older than 24 hours get a "stale" badge. User can force refresh. Evidence is never cached — always fresh from agents.

### Impact: Repeat questions go from 30s → <1s

---

## Strategy 5: Optimistic UI

### Problem
The UI waits for the full Fermi response before showing anything. Even with streaming, there's a gap between "question submitted" and "first content".

### Solution
Generate an **instant local scaffold** before the LLM responds.

**On Ctrl+Enter, immediately:**
1. Parse question for domain keywords → generate template drivers
2. Look up cached base rates for common domains (NBA: 58%, FDA approval: 15%, etc.)
3. Show skeleton driver cards with placeholder values
4. Mark everything as "⟳ refining…"

**When Fermi responds:**
- Replace skeleton data with real data
- Animate the transition (values slide to new positions)
- Flash changed values briefly

**Local domain templates:**
```rust
fn instant_scaffold(question: &str) -> (f64, Vec<DriverTemplate>) {
    let domain = detect_domain(question);
    match domain.as_str() {
        "sports_nba" => (0.58, vec![
            template("home_court", 0.9, 1.0, 1.1),
            template("opponent_strength", 0.7, 1.0, 1.3),
            template("recent_form", 0.8, 1.0, 1.2),
            template("injury_impact", 0.8, 1.0, 1.1),
        ]),
        "finance" => (0.50, vec![
            template("fundamentals", 0.7, 1.0, 1.4),
            template("market_conditions", 0.6, 1.0, 1.5),
            template("momentum", 0.8, 1.0, 1.3),
        ]),
        "biotech_fda" => (0.15, vec![
            template("clinical_data", 0.5, 1.0, 1.5),
            template("regulatory_path", 0.7, 1.0, 1.2),
            template("competitive_landscape", 0.8, 1.0, 1.3),
        ]),
        _ => (0.50, vec![
            template("strength_factor", 0.7, 1.0, 1.3),
            template("conditions", 0.8, 1.0, 1.2),
            template("risk_factor", 0.8, 1.0, 1.1),
        ]),
    }
}
```

This is essentially what already exists in `generate_decomposition()` — it just needs to be treated as the immediate UI state while the LLM works.

### Impact: Time to first visual content from 30s → <1s

---

## Strategy 6: Model Selection Per Agent Role

### Problem
All agents use Sonnet, but not all tasks need Sonnet-level reasoning.

### Solution
Match model to task complexity.

| Agent/Task | Current Model | Recommended | Reasoning |
|------------|--------------|-------------|-----------|
| Fermi scaffold | Sonnet (30s) | **Haiku (3–5s)** | Structuring, not reasoning |
| Fermi refinement | Sonnet (30s) | Sonnet (30s) | Needs deep reasoning |
| macro_forecaster | Sonnet (25s) | Sonnet (25s) | Complex economic analysis |
| market_research | Sonnet (25s) | **Haiku (8–10s)** | Data retrieval, not reasoning |
| sentiment_analyzer | Sonnet (25s) | **Haiku (8–10s)** | Classification task |
| entity_investigator | Sonnet (25s) | Sonnet (25s) | Complex OSINT reasoning |
| nba_analyst | Sonnet (25s) | Sonnet (25s) | Deep domain reasoning |
| biotech_analyst | Sonnet (25s) | Sonnet (25s) | Domain reasoning |

**Savings:** 2 out of 5 common agents move to Haiku → 40% faster research phase, 60% cheaper.

### Impact: Research agent average from 25s → 15s

---

## Implementation Roadmap

### Phase A: Quick Wins (This Sprint)

- [ ] **Optimistic UI scaffold** — instant local template drivers on Ctrl+Enter (Strategy 5)
- [ ] **Auto-assign agents** — skip manual "+ Assign Agent" clicks (Strategy 3)
- [ ] **Batch "Research All" button** — fire all assigned agents at once (Strategy 3)
- [ ] **Model per agent** — Haiku for market_research and sentiment_analyzer (Strategy 6)

**Expected improvement:** Perceived latency for initial population from 30s → <1s. Total research from 150s → 30s.

### Phase B: Two-Phase Decomposition (Next Sprint)

- [ ] **Create `fermi_scaffold` agent** on Haiku with minimal prompt
- [ ] **Two-phase fire:** Haiku scaffold → Sonnet refinement
- [ ] **UI transition:** skeleton → refined values with animation
- [ ] **Decomposition cache** in local SQLite (Strategy 4)

**Expected improvement:** Actual decomposition latency from 30s → 5s (Haiku) + 25s background (Sonnet).

### Phase C: Streaming (Sprint After)

- [ ] **ABW SSE endpoint** — `/api/agents/:id/execute/stream`
- [ ] **Token streaming** from Anthropic API → SSE → console
- [ ] **Incremental evidence** — findings appear as generated
- [ ] **Progress indicators** — tool calls shown in real-time
- [ ] **Console SSE client** using `eventsource-client` crate

**Expected improvement:** Perceived latency from "30s black hole" → "watching the agent think live".

### Phase D: Infrastructure (Longer Term)

- [ ] **Redis cache** on ABW for decomposition results
- [ ] **Edge deployment** — Railway regions closer to Anthropic API
- [ ] **Connection pooling** — reuse HTTP connections to Anthropic
- [ ] **Prompt compression** — shorter system prompts = fewer input tokens = faster response
- [ ] **Batch API** — Anthropic's batch endpoint for non-urgent research (50% cheaper, minutes latency)

---

## Latency Budget

Target for the core workflow:

| Step | Current | Target | Strategy |
|------|---------|--------|----------|
| Question → first visual | 30s | **<1s** | Optimistic UI (A) |
| Question → base rate + drivers | 30s | **5s** | Haiku scaffold (B) |
| Question → refined decomposition | 30s | **30s** (background) | Two-phase (B) |
| Assign agents → first evidence | 30s | **10–15s** (streaming) | SSE (C) |
| All agents complete | 150s | **30s** | Parallel + auto-assign (A) |
| Total: question → complete forecast | **3–5 min** | **<1 min** | All strategies |

---

## Cost Budget

| Scenario | Current | After Optimization |
|----------|---------|-------------------|
| Initial decomposition | ~30 credits (Sonnet) | ~5 credits (Haiku scaffold) + ~15 credits (Sonnet refine) = ~20 credits |
| 5 research agents | ~75 credits (5 × Sonnet) | ~45 credits (2 Haiku + 3 Sonnet) |
| Total per forecast | **~105 credits** | **~65 credits** (38% reduction) |

---

## Metrics to Track

- **Time to First Content (TTFC):** Question submit → first driver appears
- **Time to Base Rate (TTBR):** Question submit → base rate displayed
- **Time to Simulation Ready (TTSR):** Question submit → Ctrl+R is meaningful
- **Agent Execution P50/P95:** Median and 95th percentile agent call duration
- **Credits per Forecast:** Total credits consumed for a complete forecast
- **Cache Hit Rate:** % of decompositions served from cache
- **Streaming First Token:** Time from request to first SSE event

---

## Key Design Principles

1. **Show something immediately.** Even if it's a template, it's better than a spinner.
2. **Progressive refinement.** Start rough, get precise. Never block the user.
3. **Parallel everything.** No sequential agent calls. Fire and forget, update on arrive.
4. **Match model to task.** Haiku for structure, Sonnet for reasoning.
5. **Cache aggressively.** Same question = instant result. Similar question = warm start.
6. **Stream, don't batch.** Findings appear one by one, not all at once after 30 seconds.

---

## References

- Anthropic streaming API: `stream: true` on Messages API
- `eventsource-client` crate: already in Cargo.toml
- ABW SSE precedent: `rabble_moved` events already use SSE for real-time map updates
- Local database: `rusqlite` already in Cargo.toml for decomposition cache