# Open Questions by Module

**Last Updated:** 2026-02-04  
**Status:** Planning Phase - Need answers before implementation

---

## Module 1: FPL Language Server

**Status:** Foundation exists, needs LSP layer  
**Priority:** HIGH (Current Sprint)

### Q1.1: Incremental Parsing Strategy
**Options:**
- A) **salsa** (rust-analyzer's incremental computation framework)
- B) **rowan** (lossless syntax tree, easier but less optimized)
- C) **Custom incremental parser**

**Context:** Need sub-100ms re-parse latency for real-time coaching

**Trade-offs:**
- salsa: More complex, battle-tested, very fast
- rowan: Simpler API, lossless tree, good enough for most cases
- Custom: Full control, high effort

**Action Required:** Prototype both salsa and rowan, benchmark performance

---

### Q1.2: LSP Features Priority
What order should we implement LSP features?

**Proposed Order:**
1. Diagnostics (errors/warnings inline)
2. Autocompletion (driver names, functions)
3. Hover info (show distribution details)
4. Code actions ("Add evidence", "Run forecast")

**Question:** Is this the right priority, or should we start with something else?

This is correct
---

### Q1.3: Fermi Coaching Integration
How should Fermi's coaching appear in the LSP?

**Options:**
- A) Part of **LSP diagnostics** (special warning/info type)
- B) **Separate LSP extension** (custom protocol messages)
- C) **Hybrid** (diagnostics for errors, separate for suggestions)

**Trade-offs:**
- Diagnostics: Standard protocol, works in any LSP client
- Separate: More flexible, can be richer
- Hybrid: Best of both, more complexity

**Question:** Which approach fits Zed best? Need to check Zed's capabilities.

Hybrid is the way to go - 
---

### Q1.4: Coaching Verbosity
How often should Fermi provide suggestions?

**Options:**
- A) **Every line** (aggressive coaching, like Copilot)
- B) **On significant issues** (only when mistakes detected)
- C) **On request** (user asks for help explicitly)
- D) **Adaptive** (learns user preference over time)

**Question:** What feels helpful vs. annoying?

this should be somethingthat changes over time - start agreessive as part of onbording

---

### Q1.5: Execution Model
Where should forecast execution happen?

**Options:**
- A) **Always local** (fast, but limited to 10K-100K iterations)
- B) **Always backend** (scalable, but network latency)
- C) **Hybrid with threshold** (local <100K, backend >100K)
- D) **User configurable** (let user choose)

**Current thinking:** Option C (hybrid) makes most sense

**Question:** What's the threshold? 50K? 100K? 1M?

what would you suggest im thinking we start with 100k

## Module 2: Zed Extensions - Core

**Status:** Not started  
**Priority:** HIGH (Current Sprint)

### Q2.1: Tree-sitter Grammar Creation
How should we create the tree-sitter grammar for FPL?

**Options:**
- A) **Generate from existing parser** (automated, may need tweaks)
- B) **Hand-write** (full control, more effort)
- C) **Start with minimal, iterate** (ship fast, improve later)

**Question:** Is there tooling to convert our Rust parser to tree-sitter?

https://github.com/hydro-project/rust-sitter 
---

### Q2.2: Inline Sparklines Implementation
How to render Tufte-style sparklines inline?

**Options:**
- A) **Zed inlay hints** (standard, might support unicode/emoji)
- B) **Custom decorations** (more flexible, harder to implement)
- C) **Both** (inlay hints for simple, decorations for rich)

**Context:** Want to show distribution shape like `▁▃▅▇▅▃▁ [1200±800]`

**Question:** What does Zed's inlay hint API actually support?

https://zed.dev/docs/configuring-languages?highlight=inlay#inlay-hints

https://zed.dev/docs/languages/rust?highlight=inlay#inlay-hints
---

### Q2.3: Sparkline Content
What should sparklines show?

**Options:**
- A) **Current distribution** (shape of triangular(500, 1200, 2500))
- B) **Historical trend** (how estimate changed over time)
- C) **Confidence interval** (shaded p10-p90 band)
- D) **All of the above** (different sparklines for different contexts)

**Question:** What's most useful for forecasters?

all of the above
---

### Q2.4: Execute Command UX
How should users trigger forecast execution?

**Options:**
- A) **Keyboard shortcut** (e.g., Cmd+R)
- B) **Command palette** ("Fermi: Run Forecast")
- C) **Auto-execute on save** (like formatters)
- D) **All of the above**

**Question:** What's the most natural workflow?

All of the above
---

### Q2.5: Results Panel Location
Where should forecast results appear?

**Options:**
- A) **Bottom panel** (like terminal, familiar)
- B) **Right sidebar** (keeps editor visible)
- C) **Floating window** (movable, dismissible)
- D) **Inline expansion** (results appear in editor itself)

**Question:** What feels right for forecasting workflow?

lets start with right side bar
---

### Q2.6: Status Indicator During Execution
How to show that forecast is running?

**Options:**
- A) **Progress bar** (in results panel)
- B) **Status bar text** (bottom of Zed)
- C) **Spinner icon** (near execute button)
- D) **All of the above**

**Question:** What's the Zed-native way?

D
---

## Module 3: Agent Bestiary UI

**Status:** Not started  
**Priority:** MEDIUM (Sprint 2)

### Q3.1: Yokai Avatar System
How should agent avatars work?

**Options:**
- A) **Pre-designed set** (10-20 avatars, assign per agent type)
- B) **AI-generated** (DALL-E/Midjourney, custom per agent)
- C) **User-uploadable** (custom images)
- D) **Symbolic only** (emoji/icons, no artwork)

**Question:** What's the right balance of aesthetic vs. effort?

---

### Q3.2: Agent Card Design
What information should agent cards show?

**Critical (always visible):**
- Name
- Type (a2a, MCP, custom)
- Status (active/inactive)
- ?

**Secondary (click to expand):**
- Usage stats (calls, tokens, cost)
- Ontology version
- Recent activity
- ?

**Question:** What's essential vs. nice-to-have?

---

### Q3.3: Handle System Mechanics
How do users add agents to their forecast?

**Options:**
- A) **Drag-and-drop** (agent card → editor)
- B) **Click-to-insert** (inserts `agent` statement at cursor)
- C) **Preview-on-hover** (show card when hovering over agent name in code)
- D) **All of the above**

**Question:** What's most intuitive?

---

### Q3.4: Agent Configuration UI
Where should users configure agents (API keys, parameters)?

**Options:**
- A) **In-panel form** (traditional settings UI)
- B) **In-code configuration** (all settings in .fpl file)
- C) **Hybrid** (sensitive data in panel, logic in code)

**Example of in-code:**
```fpl
agent market_research {
    query: "Current GPU market size"
    model: "claude-opus-4"
    temperature: 0.3
    review: manual
}
```

**Question:** What's more secure and user-friendly?

---

### Q3.5: Bestiary Organization
How should users navigate many agents?

**Options:**
- A) **Categories** (Research, Data, Analysis)
- B) **Search/filter** (by type, capability, tag)
- C) **Favorites/recent** (pin frequently used)
- D) **Custom collections** (user-defined groupings)
- E) **All of the above**

**Question:** What's the minimum viable organization?

---

### Q3.6: Initial Agent Count
How many agents should we launch with?

**Options:**
- A) **3-5 agents** (minimum viable, focused)
- B) **10 agents** (good variety)
- C) **20+ agents** (comprehensive bestiary)

**Question:** Quality vs. quantity trade-off?

---

## Module 4: Visualization & Charts

**Status:** Not started  
**Priority:** MEDIUM (Sprint 3)

### Q4.1: Charting Library Choice
What library should we use for charts?

**Options:**
- A) **Plotly** (web-based via WebView, rich features)
- B) **plotters** (native Rust, fast, simpler)
- C) **egui** (native Rust, immediate mode GUI)
- D) **ASCII/Unicode** (terminal-style, ultra-fast)

**Trade-offs:**
| Library | Beauty | Speed | Complexity | Bundle Size |
|---------|--------|-------|------------|-------------|
| Plotly  | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | Large |
| plotters | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ | Small |
| egui    | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | Small |
| ASCII   | ⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐ | Tiny |

**Question:** What's the right balance for forecasting?

---

### Q4.2: Chart Types Priority
What order should we implement chart types?

**Proposed Priority:**
1. Histogram (distribution shape)
2. Statistics table (mean, median, percentiles)
3. Confidence bands (uncertainty visualization)
4. Line chart (forecast evolution over time)
5. Tornado chart (sensitivity analysis)
6. Calibration plot (Brier score breakdown)

**Question:** Is this the right order?

---

### Q4.3: Interactive Features
What interactivity should charts have?

**Options:**
- A) **Click sparkline → open full chart**
- B) **Hover → show detailed stats**
- C) **Zoom/pan** on distributions
- D) **Export** (PNG, SVG, data CSV)
- E) **All of the above**

**Question:** What's essential vs. nice-to-have?

---

## Module 5: Fermi Backend

**Status:** Needs rebuild from uffp-backend  
**Priority:** MEDIUM (Sprint 2)

### Q5.1: Web Framework Choice
What Rust web framework should we use?

**Options:**
- A) **axum** (modern, fast, Tower-based, active development)
- B) **actix-web** (mature, battle-tested, slightly faster)
- C) **rocket** (simpler, more opinionated, less flexible)

**Question:** What's your preference based on team experience?

---

### Q5.2: Zed ACP Integration
How does Zed's Agent Communication Protocol work?

**Research Needed:**
- What's the registry structure?
- How are agents discovered?
- What's the authentication model?
- Can we extend/wrap it?

**Question:** Need to investigate Zed's ACP documentation - where is it?

---

### Q5.3: Agent Coordinator Execution
How should the agent coordinator work?

**Design Questions:**
- **Queue system:** Redis, PostgreSQL, in-memory?
- **Max concurrent agents:** Limit to avoid rate limits/costs?
- **Timeout handling:** What if agent takes too long?
- **Retry logic:** How to handle transient failures?

**Question:** What's the right architecture for reliability?

---

### Q5.4: Agent Callbacks (Async Execution)
How does Zed get notified when an agent completes?

**Options:**
- A) **WebSocket** (real-time push, bi-directional)
- B) **Polling** (check status periodically, simpler)
- C) **Webhook** (agent posts back to backend)
- D) **Hybrid** (WebSocket for UI, webhook for agents)

**Question:** What's most reliable and performant?

---

### Q5.5: Manual Review Workflow
How should manual review of agent results work?

**Design Questions:**
- Where are pending results stored? (DB, cache, both?)
- What's the approval UI? (in Zed panel, web interface?)
- Per-result or batch approval?
- Can users edit agent results before accepting?

**Question:** What workflow is most efficient for forecasters?

---

### Q5.6: Database Schema
What tables do we need in PostgreSQL?

**Proposed Schema:**
```sql
users (id, email, password_hash, created_at)
forecasts (id, user_id, fpl_code, version, created_at)
executions (id, forecast_id, results_json, timestamp)
agents (id, name, type, config_json, ontology_version)
agent_calls (id, agent_id, forecast_id, query, response, status, created_at)
tournaments (id, name, question, deadline, scoring_method)
submissions (id, tournament_id, user_id, forecast_id, score, submitted_at)
evidence (id, forecast_id, content, source, relevance, created_at)
```

**Question:** What's missing? What's unnecessary?

---

### Q5.7: Real-time Sync Strategy
How should multi-user collaboration work?

**Options:**
- A) **WebSocket for everything** (real-time, complex)
- B) **REST for writes, WebSocket for reads** (hybrid)
- C) **Polling** (simpler, less efficient, more reliable)
- D) **Operational Transform** (like Google Docs, very complex)

**Question:** What's the right level of real-time for forecasting?

---

### Q5.8: Conflict Resolution
What happens when multiple users edit the same forecast?

**Options:**
- A) **Last write wins** (simple, data loss possible)
- B) **Optimistic locking** (detect conflicts, user resolves)
- C) **CRDTs** (conflict-free, complex)
- D) **Lock on edit** (no conflicts, poor UX)

**Question:** What's acceptable for forecasting use case?

---

## Module 6: Mermaid ER Viewer

**Status:** Not started  
**Priority:** LOW (Sprint 3)

### Q6.1: Mermaid Rendering Approach
How to render Mermaid diagrams in Zed?

**Options:**
- A) **Embedded WebView** (use mermaid.js, full features)
- B) **Native Rust parser** (parse mermaid → render SVG)
- C) **External tool** (generate SVG server-side)

**Question:** What does Zed support? WebViews available?

---

### Q6.2: Agent Ontology Evolution
How do agent ontologies evolve over time?

**Options:**
- A) **Auto-generated from interactions** (agent learns)
- B) **Manually curated** (agent creator defines)
- C) **Hybrid** (base version manual, extensions auto)

**Question:** Who's responsible for ontology updates?

---

### Q6.3: Ontology Representation
What should agent ontology ER diagrams show?

**Proposed Entities:**
- AGENT (name, type, version)
- TOOL (name, description, API)
- KNOWLEDGE_DOMAIN (area, confidence)
- DATA_SOURCE (type, access_method)

**Question:** What relationships matter most?

---

### Q6.4: Time Travel for Ontologies
Should we show ontology history?

**Options:**
- A) **Yes** - Full version history with diff view
- B) **Partial** - Show current + previous version only
- C) **No** - Only current version (simpler)

**Question:** Is this valuable enough to build?

---

## Module 7: Collaboration & Tournaments

**Status:** Not started  
**Priority:** MEDIUM (Sprint 5)

### Q7.1: Tournament Lifecycle
How should tournaments work?

**Design Questions:**
- **Creation:** Who can create? Public/private?
- **Submission:** Can users update forecasts? Until deadline?
- **Resolution:** Who resolves? Manual/automated?
- **Scoring:** Brier score only, or other metrics?

**Question:** What's the MVP tournament flow?

---

### Q7.2: Leaderboard Display
What should leaderboards show?

**Options:**
- A) **Overall ranking** (all tournaments combined)
- B) **Per-tournament** (specific competition)
- C) **Calibration curve** (visual accuracy)
- D) **Historical performance** (trend over time)
- E) **All of the above**

**Question:** What's most motivating for forecasters?

---

### Q7.3: Collaboration Modes
How should multiple users work together?

**Options:**
- A) **Real-time co-editing** (Google Docs style)
- B) **Fork/merge** (GitHub style)
- C) **Comment threads** (discussion on forecasts)
- D) **Team forecasts** (aggregate team members)
- E) **Combination**

**Question:** What collaboration style fits forecasting?

---

### Q7.4: Sharing & Privacy
How should forecast visibility work?

**Options:**
- A) **Public** (anyone can view)
- B) **Private** (only creator)
- C) **Tournament-only** (visible to participants)
- D) **Granular permissions** (view/edit/execute roles)

**Question:** What's the default? How much control?

---

### Q7.5: Versioning Strategy
How should forecast history work?

**Options:**
- A) **Git-like** (commits, branches, merges)
- B) **Automatic snapshots** (save on every change)
- C) **Manual checkpoints** (user tags versions)
- D) **Time-based** (keep all changes for N days)

**Question:** What's intuitive for non-technical forecasters?

---

## Module 8: Settings & Configuration

**Status:** Not started  
**Priority:** LOW (Sprint 6)

### Q8.1: Settings Access Model
How should users change settings?

**Options:**
- A) **Traditional UI** (settings panel exists)
- B) **Agent-only** (ask Fermi to change settings)
- C) **Hybrid** (both available)

**Question:** How much do we commit to "agent-assisted everything"?

---

### Q8.2: Configuration Scope
What levels of configuration should exist?

**Options:**
- A) **Global** (all forecasts inherit)
- B) **Per-forecast** (override in .fpl file)
- C) **Per-workspace** (tournament-specific)
- D) **All levels** (with precedence rules)

**Question:** What's the right granularity?

---

### Q8.3: What Needs Configuration?
What settings are essential?

**Execution:**
- default_iterations
- execution_timeout
- cache_results

**Agents:**
- default_agent
- agent_timeout
- require_manual_review

**UI:**
- show_sparklines
- theme
- panel_layout

**Question:** What else is critical?

---

### Q8.4: Fermi Assistant Capabilities
What can Fermi do with settings?

**Options:**
- A) **Read only** (explain current config)
- B) **Read + modify** (change settings)
- C) **Read + modify + suggest** (proactive recommendations)

**Question:** Should changes require confirmation?

---

## Module 9: Navigation & Discovery

**Status:** Not started  
**Priority:** MEDIUM (Sprint 4)

### Q9.1: Primary Navigation Method
Without a file tree, how do users find forecasts?

**Options:**
- A) **Command palette** (fuzzy search by name)
- B) **Forecast library panel** (card gallery view)
- C) **Timeline view** (recent/starred/all)
- D) **Tag browser** (filter by labels)
- E) **Combination**

**Question:** What's the main way to navigate?

---

### Q9.2: Organization System
How should forecasts be organized?

**Options:**
- A) **Tags/labels** (replace folders)
- B) **Projects/workspaces** (tournaments as containers)
- C) **Collections** (user-defined groupings)
- D) **Smart folders** (auto-organize by criteria)

**Question:** What's intuitive for forecasters?

---

### Q9.3: Search Capabilities
What types of search should we support?

**Options:**
- A) **Full-text** (search within .fpl code)
- B) **Metadata** (by author, date, tags)
- C) **Semantic** (find forecasts about "market size")
- D) **Results-based** (forecasts with p50 > 100)
- E) **All of the above**

**Question:** What's the priority order?

---

### Q9.4: Forecast Library UI Layout
How should the forecast library look?

**Concept:**
```
┌─────────────────────────────────────────┐
│ [Search: "AMD stock"]                   │
├─────────────────────────────────────────┤
│ ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│ │ AMD Q4  │ │ GPU Mkt │ │ Nvda vs │   │
│ │ Revenue │ │ Share   │ │ AMD     │   │
│ │ p50:195 │ │ p50:32% │ │ p50:62% │   │
│ └─────────┘ └─────────┘ └─────────┘   │
└─────────────────────────────────────────┘
```

**Question:** Card view, list view, or both?

---

## Module 10: Mobile Client

**Status:** Deferred  
**Priority:** LOW (Future)

### Q10.1: Mobile Priorities
What should mobile support?

**Options:**
- A) **View-only** (review forecasts, see results)
- B) **Agent management** (trigger research, approve results)
- C) **Light editing** (adjust parameters, not write from scratch)
- D) **Notifications** (tournament deadlines, agent results ready)
- E) **Combination**

**Question:** What's the minimum viable mobile experience?

---

### Q10.2: Platform Choice
What mobile platform?

**Options:**
- A) **React Native** (cross-platform, existing uffp knowledge)
- B) **Flutter** (better performance, different language)
- C) **Native iOS first** (best UX, limited audience)
- D) **Progressive Web App** (web-based, works everywhere)

**Question:** What's the strategy for mobile?

---

### Q10.3: Unique Mobile Features
What should mobile do that desktop doesn't?

**Ideas:**
- Voice input ("Ask market research agent about X")
- Photo capture (photo of data → evidence)
- Push notifications (tournament reminders)
- Quick actions (swipe gestures)

**Question:** What makes mobile valuable vs. just "desktop lite"?

---

## Summary by Priority

### HIGH PRIORITY (Current Sprint)
- **Module 1:** 5 questions (incremental parsing, coaching, execution)
- **Module 2:** 6 questions (tree-sitter, sparklines, UX)

### MEDIUM PRIORITY (Next 2 Sprints)
- **Module 3:** 6 questions (avatars, cards, handles)
- **Module 4:** 3 questions (charting library, priority, interactivity)
- **Module 5:** 8 questions (framework, ACP, callbacks, database)
- **Module 7:** 5 questions (tournaments, leaderboards, collaboration)
- **Module 9:** 4 questions (navigation, organization, search)

### LOW PRIORITY (Future)
- **Module 6:** 4 questions (mermaid rendering, ontology evolution)
- **Module 8:** 4 questions (settings access, scope, configuration)
- **Module 10:** 3 questions (mobile priorities, platform, features)

---

## How to Use This Document

1. **Before starting a module:** Read all questions for that module
2. **During planning:** Answer questions, create ADRs for decisions
3. **Update this doc:** Mark questions as answered, add new questions
4. **Reference in commits:** Link to specific questions when relevant

**Next Action:** Answer Module 1 and Module 2 questions before starting Sprint 1 implementation.
