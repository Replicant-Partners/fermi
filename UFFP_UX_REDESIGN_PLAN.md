# UFFP Mobile - UX Redesign & Refactoring Plan
**Version:** 1.0  
**Date:** 2026-02-04  
**Status:** Ready for Discussion

---

## Executive Summary

This document outlines a comprehensive plan to redesign and refactor the UFFP Mobile application based on a thorough code audit. The primary goal is to transform the application from its current state (with an 8,975-line monolithic component) into a maintainable, testable, and scalable CLI-driven forecasting tool that provides intelligent adaptive coaching.

### Critical Findings

1. **ForecastWorkspaceScreen.tsx is 8,975 lines** - represents 87% of total screen complexity
2. **No global state management** - 25+ useState hooks in a single component
3. **Duplicate logic patterns** - Same code repeated 27+ times
4. **Missing abstractions** - No custom hooks, no compound components
5. **Well-designed foundation** - Excellent service layer and design system to build upon

### Strategic Priorities

1. **Emergency**: Break down ForecastWorkspaceScreen (200 hours, HIGH RISK)
2. **High**: Implement proper state management with Zustand (80 hours)
3. **Medium**: Enhance service layer and extract business logic (60 hours)
4. **Medium**: Refactor large components (100 hours)
5. **Low**: Optimize data flow and improve type safety (100 hours)

**Total Estimated Effort:** 540 hours (3-4 months with 2-3 developers)

---

## 1. System Overview

### Current Architecture

```
UFFP Mobile (React Native + Expo)
├── Frontend Layer
│   ├── Screens (10 files, 1 at 8,975 lines)
│   ├── Components (7 reusable components)
│   └── Tufte Design System (excellent)
├── Service Layer (well-designed)
│   ├── forecastService.ts (Monte Carlo)
│   ├── researchService.ts (AI agents)
│   ├── authService.ts (auth state)
│   ├── fermiCommands.ts (CLI commands)
│   └── ontology/ (knowledge graph)
├── Data Layer
│   ├── Local: AsyncStorage (React Native) / localStorage (Web)
│   ├── Backend: PostgreSQL + Redis
│   └── Sync: backendSync.ts (dual-storage strategy)
└── Constants & Config
    ├── Research prompts (10 templates)
    ├── Agent configs (6 specialized agents)
    └── Fermi hints (100+ drivers)
```

### Design Philosophy

The app follows **Tufte-Tschichold** principles:
- High data-ink ratio (minimize decoration)
- Clear typographic hierarchy
- Functional minimalism
- Academic rigor over consumer app polish

---

## 2. Business Rules & Data Model Analysis

### Core Domain Concepts

#### Forecast Lifecycle

```
1. Question → 2. Drivers → 3. Evidence → 4. Simulation → 5. Resolution → 6. Scoring

Question: "Will X happen by date Y?"
  ↓
Drivers: Decompose into 3-7 multiplicative factors
  ↓
Evidence: Research, data, expert opinion
  ↓  
Simulation: Monte Carlo (10,000 iterations)
  ↓
Resolution: Outcome determined (true/false)
  ↓
Scoring: Brier score = (probability - outcome)²
```

#### Data Model Issues Identified

**1. Inconsistent Driver Structure**
```typescript
// Current: Mixed concerns
interface ForecastDriver {
  name: string;
  description: string;
  distributionType: string;
  parameters: { /* varies by type */ };
  unit: string;
  rationale?: string;
  evidence?: Evidence[];
  distributionRationale?: string;
}

// Problem: evidence is optional but critical for Tetlock methodology
// Problem: rationale fields are vague (what kind of rationale?)
```

**Proposed Fix:**
```typescript
interface ForecastDriver {
  id: string; // Add stable ID
  name: string;
  description: string;
  
  // Distribution configuration
  distribution: {
    type: "triangular" | "normal" | "beta" | "uniform";
    parameters: DistributionParams; // Discriminated union by type
    rationale: string; // Why this distribution type?
    confidenceLevel: "high" | "medium" | "low";
  };
  
  // Evidence (required for quality forecasts)
  evidence: {
    items: Evidence[];
    synthesisNote?: string; // How evidence informed parameters
    lastUpdated: string;
  };
  
  // Metadata
  unit: string;
  createdAt: string;
  updatedAt: string;
  version: { major: number; minor: number };
}

// Discriminated union for type-safe parameters
type DistributionParams = 
  | { type: "triangular"; low: number; mode: number; high: number }
  | { type: "normal"; mean: number; stdDev: number }
  | { type: "beta"; alpha: number; beta: number; min: number; max: number }
  | { type: "uniform"; min: number; max: number };
```

**2. SavedForecast Structure Ambiguity**

Current structure mixes concerns:
- UI state (`fermiConversation`)
- Domain data (`drivers`, `probability`)
- Metadata (`resolved`, `brierScore`)
- Computed values (`simulations[]`)

**Proposed Fix:** Separate into layers:
```typescript
// Domain entity
interface Forecast {
  id: string;
  question: string;
  domain?: string;
  timeframe: { start: string; end: string; };
  
  // External view (Tetlock base rate)
  externalView: {
    referenceClass: string;
    baseRate: number;
    source: string;
    confidence: "high" | "medium" | "low";
    reasoning: string;
  };
  
  // Premortem analysis
  premortem: {
    completed: boolean;
    failureScenarios: FailureScenario[];
  };
  
  // Drivers (the model)
  drivers: ForecastDriver[];
  
  // Lifecycle
  status: "draft" | "active" | "expired" | "resolved";
  createdAt: string;
  updatedAt: string;
}

// Simulation results (separate entity)
interface Simulation {
  id: string;
  forecastId: string;
  runAt: string;
  runBy: "user" | "auto-update";
  reason?: string;
  
  driverSnapshot: ForecastDriver[]; // State at time of simulation
  iterations: number;
  
  result: {
    probability: number; // P(above target)
    distribution: {
      p10: number; p25: number; p50: number; p75: number; p90: number;
    };
    histogram: { bin: number; count: number }[];
    metadata: {
      runtime: number;
      cost?: number; // If using paid compute
    };
  };
}

// Resolution (separate entity)
interface Resolution {
  id: string;
  forecastId: string;
  resolvedAt: string;
  resolvedBy: string; // User ID
  actualOutcome: boolean;
  predictedProbability: number; // From last simulation
  brierScore: number;
  notes?: string;
}

// UI-specific state (ephemeral, not persisted)
interface ForecastWorkspaceState {
  activeForecastId: string | null;
  fermiChatHistory: ChatMessage[];
  driverInConfig: string | null; // Driver ID
  agentInConfig: AgentConfig | null;
  pendingChanges: boolean;
}
```

**Benefits:**
- Clearer separation of concerns
- Easier to test
- Simpler persistence logic
- Type-safe status transitions

**3. Evidence Model Enhancement**

Current evidence is too generic:

```typescript
// Current: Everything is "Evidence"
interface Evidence {
  id: string;
  type: "research" | "web_article" | "competitor_data" | ...;
  // ... generic fields
}

// Problem: Different evidence types need different fields
// Problem: No quality assessment
// Problem: No update tracking
```

**Proposed Fix:** Discriminated union by type:
```typescript
type Evidence = 
  | ResearchEvidence
  | WebArticleEvidence
  | ExpertOpinionEvidence
  | InternalDataEvidence;

interface BaseEvidence {
  id: string;
  driverId: string;
  addedAt: string;
  addedBy: "user" | "agent";
  quality: "high" | "medium" | "low" | "unverified";
  relevance: number; // 0-10 score
}

interface ResearchEvidence extends BaseEvidence {
  type: "research";
  source: {
    agentId: string;
    agentRun: string;
    query: string;
  };
  findings: {
    summary: string;
    keyPoints: string[];
    dataPoints: { metric: string; value: number; }[];
  };
  confidence: number; // 0-1 from agent
}

interface WebArticleEvidence extends BaseEvidence {
  type: "web_article";
  url: string;
  title: string;
  author?: string;
  publishedAt?: string;
  excerpt: string;
  linkPreview: LinkPreview;
}

interface ExpertOpinionEvidence extends BaseEvidence {
  type: "expert_opinion";
  expert: {
    name: string;
    credentials: string;
    bias?: string; // Potential conflicts of interest
  };
  opinion: string;
  reasoning: string;
}
```

---

## 3. CLI Interface Design - Intelligent Adaptive Coaching

### Current State

The CLI interface exists but lacks intelligence:
- Commands work but don't guide users
- No contextual awareness beyond basic context types
- No learning or adaptation
- No progressive disclosure

### Proposed: Adaptive Coaching System

#### Coaching Principles (Tetlock-Inspired)

1. **Progressive Disclosure**: Show complexity gradually
2. **Just-In-Time Guidance**: Help when needed, not before
3. **Exemplar-Based Learning**: Show examples, not abstractions
4. **Feedback Loops**: Immediate, specific, actionable

#### Coaching States

```typescript
type CoachingState = 
  | "new_user" // First forecast ever
  | "learning" // Forecasts 1-5
  | "intermediate" // Forecasts 6-20
  | "advanced" // 21+ forecasts
  | "superforecaster"; // Brier score < 0.15

interface CoachingContext {
  userLevel: CoachingState;
  currentStep: ForecastStep;
  mistakesObserved: string[]; // Track common errors
  strengthsObserved: string[]; // Track good practices
  lastHintShown?: string;
  hintsDeclined: number; // User preferring autonomy
}
```

#### Adaptive Command Suggestions

**Current:** Static command list  
**Proposed:** Context + history-aware suggestions

```typescript
interface SmartSuggestion {
  command: string;
  reason: string; // Why this suggestion now
  priority: "high" | "medium" | "low";
  learnMore?: string; // Link to documentation
  example?: string; // Concrete example
}

// Example: User has 2 drivers, no evidence
getSuggestions(context) => [
  {
    command: "/agent research Market size for X",
    reason: "Your 'Market Size' driver lacks evidence. Let me research it.",
    priority: "high",
    example: "I'll search for TAM estimates, growth rates, and market reports."
  },
  {
    command: "/bias",
    reason: "Consider cognitive biases before finalizing parameters.",
    priority: "medium",
    learnMore: "/help bias"
  },
  {
    command: "/simulate",
    reason: "Ready to see your forecast distribution.",
    priority: "low"
  }
]
```

#### Coaching Interventions

**1. Missing Drivers Detection**
```
User: /simulate

Fermi: "⚠️ I notice you only have 2 drivers. Research shows forecasts 
       with 4-6 drivers are 23% more accurate.
       
       Missing drivers I detected:
       • Competition intensity
       • Regulatory environment
       • Technology adoption curve
       
       Would you like to:
       /add Competition intensity
       /proceed anyway"
```

**2. Range Checking (Overconfidence Bias)**
```
User: /range 100 105 110  (very narrow range)

Fermi: "🤔 That's a very narrow range (±5%). For comparison:
       
       • Expert forecasters typically use ±30% ranges
       • Your last 3 forecasts averaged ±18%
       • For this driver type (revenue), typical range is ±40%
       
       Consider these options:
       /range 70 105 140  (conservative, ±33%)
       /range 85 105 125  (moderate, ±20%)
       /keep-mine  (stick with ±5%)"
```

**3. Evidence Quality Check**
```
User: /simulate

Fermi: "📊 Evidence quality check:
       
       Driver: Market Size
       • ⚠️ Only 1 source (weak)
       • 🔗 Source is 18 months old
       • ✅ Good: From reputable analyst (Gartner)
       
       Suggestion: Add recent data
       /agent research Market size for [X] in 2026
       
       Or proceed: /simulate --acknowledge-limitations"
```

**4. Base Rate Coaching**
```
User: /question Will startup X reach $100M ARR by 2027?

Fermi: "Great question! Let's start with the outside view.
       
       I found the reference class: 'B2B SaaS startups reaching $100M ARR'
       Base rate: 3.2% (from Bessemer data, n=500 companies)
       
       This means historically, about 3 in 100 similar companies achieve this.
       Your forecast should start here and adjust based on specific factors.
       
       Next step: What makes X different?
       /driver Product-market fit
       /driver Team strength
       /driver Market timing"
```

**5. Premortem Guidance**
```
User: [Created forecast, added drivers]

Fermi: "🎯 Before we simulate, let's do a premortem.
       
       Imagine it's 2027 and X did NOT reach $100M ARR.
       What went wrong?
       
       Common failure modes I've seen:
       • Market smaller than expected (40% of failures)
       • Competition intensified (30%)
       • Execution issues (20%)
       • Regulatory blockers (10%)
       
       /premortem [your failure scenario]
       
       This typically improves forecast accuracy by 15-20%."
```

#### Progressive Complexity

**New User Flow** (Highly Guided):
```
Step 1: /question <text>
  → Fermi parses, suggests drivers
  
Step 2: Fermi: "Let's add your first driver: Market Size"
        Fermi: "What's the LOW estimate? (Think: pessimistic case)"
        User: 500
        Fermi: "And the HIGH estimate? (Optimistic case)"
        User: 2000
        Fermi: "Most likely value? (Your best guess)"
        User: 1000
        Fermi: "✅ Added triangular distribution (500, 1000, 2000)"
        
Step 3: Fermi: "Let's find evidence. I'll research market size for you."
        [Agent runs automatically]
        
Step 4: [Repeat for 3-5 drivers]

Step 5: Fermi: "Great work! Ready to simulate?"
        /simulate
```

**Advanced User Flow** (Minimal Guidance):
```
/q Will X reach Y by Z?
/driver Market:low=500,mode=1000,high=2000
/driver Growth:normal(mean=0.3,sd=0.1)
/agent research Market size --schedule weekly
/base-rate B2B SaaS $100M ARR
/premortem Market saturation, Competition from incumbents
/simulate
/resolve true
```

#### Conversational Context Tracking

**Current:** Commands are stateless  
**Proposed:** Multi-turn conversations

```typescript
interface Conversation {
  turns: Turn[];
  intent?: string; // "configure_driver" | "add_evidence" | etc
  entities: { [key: string]: any }; // Extracted info
  waitingFor?: string; // Next expected input
}

// Example:
Turn 1:
  User: "The market is probably around 1000"
  Parse: { intent: "set_parameter", driver: "current", param: "mode", value: 1000 }
  Fermi: "Got it, mode = 1000. What's the LOW end?"
  
Turn 2:
  User: "Maybe 500?"
  Parse: { intent: "set_parameter", driver: "current", param: "low", value: 500 }
  Fermi: "And the HIGH end?"
  
Turn 3:
  User: "Could go up to 2000"
  Parse: { intent: "set_parameter", driver: "current", param: "high", value: 2000 }
  Fermi: "Perfect! Added Market Size driver: triangular(500, 1000, 2000)"
```

#### Command Auto-Completion Enhancement

**Current:** Simple prefix matching  
**Proposed:** Intent-based suggestions

```typescript
// User types: "add evid"
// Current: Shows "/add-evidence"
// Proposed: Understands incomplete intent

suggestions = [
  {
    primary: "/evidence <title> <url>",
    description: "Add evidence to current driver",
    confidence: 0.9
  },
  {
    primary: "Let me help you find evidence",
    description: "I can research this for you",
    action: () => showAgentPrompt(),
    confidence: 0.7
  }
]
```

### Fermi Chat Interface Improvements

#### Current Issues

1. Chat history not persistent across sessions
2. No conversation threading
3. No rich media in responses
4. No actions from chat (must use commands)

#### Proposed Improvements

**1. Persistent Conversations**
```typescript
interface FermiConversation {
  id: string;
  forecastId: string;
  threads: ConversationThread[];
  createdAt: string;
}

interface ConversationThread {
  id: string;
  topic: string; // "driver_configuration" | "evidence_review" | etc
  messages: Message[];
  status: "active" | "completed";
}

interface Message {
  id: string;
  role: "user" | "fermi";
  content: string | RichContent;
  timestamp: string;
  actions?: Action[]; // Executable actions from message
}

interface RichContent {
  text: string;
  attachments?: {
    type: "chart" | "table" | "link_preview";
    data: any;
  }[];
}
```

**2. Actionable Messages**
```
Fermi: "I found 5 market size estimates:

        Source          | Value | Date    | Quality
        ----------------|-------|---------|--------
        Gartner         | $1.2B | 2025-09 | ⭐⭐⭐
        Forrester       | $980M | 2025-06 | ⭐⭐⭐
        Company blog    | $2.1B | 2024-12 | ⭐
        
        Median: $1.2B
        Range: $980M - $2.1B
        
        [Use median for mode ✓] [Use range for low/high ✓] [Ignore ✗]"
        
// Clicking button executes: /range 980 1200 2100
```

**3. Coaching Checkpoints**
```
Fermi: "🎯 Checkpoint: Driver Quality Review
       
       Driver          | Evidence | Range   | Quality
       ----------------|----------|---------|--------
       Market Size     | 3 items  | ±40%   | ✅ Good
       Growth Rate     | 1 item   | ±10%   | ⚠️  Too narrow
       Churn Rate      | 0 items  | ±50%   | ❌ No evidence
       
       Action needed:
       1. Widen Growth Rate range or add evidence
       2. Research Churn Rate
       
       [Fix automatically 🤖] [I'll do it manually]"
```

**4. Learning Mode**
```
Fermi: "💡 Teaching moment: Reference Class Forecasting
       
       You set Market Size to $2B based on one analyst report.
       Let's use the 'outside view' instead:
       
       Similar markets (SaaS dev tools):
       • GitHub Copilot: $1.8B (Year 2)
       • Replit: $600M (Year 2) 
       • CodeSandbox: $200M (Year 2)
       
       Median: $600M (67% lower than your estimate)
       
       This suggests anchoring bias - you may be too optimistic.
       
       [Adjust to $600M] [Keep $2B] [Tell me more about anchoring]"
```

---

## 4. Refactoring Plan

### Phase 1: Emergency Triage (ForecastWorkspaceScreen)

**Priority:** CRITICAL  
**Effort:** 200 hours  
**Risk:** HIGH

#### 1.1 Create Custom Hooks (Week 1-2)

Extract state management to focused hooks:

**`src/hooks/useForecastState.ts`** (Manage forecast CRUD)
```typescript
export function useForecastState() {
  const [forecasts, setForecasts] = useState<SavedForecast[]>([]);
  const [active, setActive] = useState<string | null>(null);
  
  const loadForecasts = async () => { /* ... */ };
  const createForecast = async (question: string) => { /* ... */ };
  const updateForecast = async (id: string, updates: Partial<SavedForecast>) => { /* ... */ };
  const deleteForecast = async (id: string) => { /* ... */ };
  const setActiveForecast = (id: string) => { /* ... */ };
  
  return {
    forecasts,
    activeForecast: forecasts.find(f => f.id === active),
    loadForecasts,
    createForecast,
    updateForecast,
    deleteForecast,
    setActiveForecast
  };
}
```

**`src/hooks/useDriverConfiguration.ts`** (Driver config workflow)
```typescript
interface DriverConfigState {
  driver: ForecastDriver | null;
  step: "select" | "configure" | "evidence" | "confirm";
  pendingChanges: boolean;
}

export function useDriverConfiguration(forecastId: string) {
  const [state, setState] = useState<DriverConfigState>({ /* ... */ });
  
  const startConfig = (driver: ForecastDriver) => { /* ... */ };
  const updateParameter = (param: string, value: any) => { /* ... */ };
  const addEvidence = (evidence: Evidence) => { /* ... */ };
  const saveDriver = async () => { /* ... */ };
  const cancelConfig = () => { /* ... */ };
  
  return { state, startConfig, updateParameter, addEvidence, saveDriver, cancelConfig };
}
```

**`src/hooks/useAgentConfiguration.ts`** (Agent config workflow)
**`src/hooks/useFermiChat.ts`** (Chat interface state)
**`src/hooks/useCommandExecution.ts`** (Command parsing)
**`src/hooks/useVersionControl.ts`** (Version tracking)
**`src/hooks/useBackendSync.ts`** (Sync orchestration)
**`src/hooks/useToast.ts`** (Toast notifications)
**`src/hooks/useCoaching.ts`** (Adaptive coaching)

#### 1.2 Extract Sub-Components (Week 3-4)

Create focused UI components in `src/screens/ForecastWorkspace/components/`:

```
ForecastWorkspace/
  components/
    QuestionHeader.tsx          (50 lines)
    ExternalViewCard.tsx        (120 lines)
    SimulationChart.tsx         (150 lines)
    DriverConfigPanel.tsx       (200 lines)
    AgentConfigPanel.tsx        (150 lines)
    EvidenceSection.tsx         (180 lines)
    DriverList.tsx              (250 lines)
    ForecastListPanel.tsx       (200 lines)
    LeaderboardPanel.tsx        (150 lines)
    FermiChatInterface.tsx      (300 lines)
    CommandAutocomplete.tsx     (100 lines)
    CoachingPrompts.tsx         (150 lines)
    ToastNotification.tsx       (50 lines)
  ForecastWorkspaceScreen.tsx   (300 lines - orchestration only)
  index.ts
```

Each component should:
- Be under 300 lines
- Have a single responsibility
- Accept props, not manage global state
- Be testable in isolation

#### 1.3 Extract Services (Week 5-6)

Create new services to handle business logic:

**`src/services/ForecastStateService.ts`**
- Centralized forecast state management
- CRUD operations
- State validation
- Change tracking

**`src/services/DriverService.ts`**
- Driver operations (add, update, remove)
- Driver validation
- Evidence management for drivers
- Hints and suggestions

**`src/services/AgentService.ts`**
- Agent configuration
- Research execution
- Scheduled research
- Result processing

**`src/services/VersionControlService.ts`**
- Version tracking
- Change detection (major vs minor)
- History management
- Rollback support

**`src/services/CoachingService.ts`** (NEW)
- User level detection
- Context-aware suggestions
- Mistake pattern detection
- Intervention triggering

#### 1.4 Reorganize Utilities (Week 7)

**`src/utils/validation/`**
```
driverValidator.ts
forecastValidator.ts
evidenceValidator.ts
simulationValidator.ts
```

**`src/utils/commandParser.ts`** - Extract command parsing from workspace  
**`src/utils/migrations.ts`** - Data migration logic  
**`src/utils/formatting.ts`** - Display formatting utilities

**Target Result:**
- ForecastWorkspaceScreen: 8,975 → ~300 lines
- 13 new sub-components (average 150 lines each)
- 8 new custom hooks (average 100 lines each)
- 5 new services (average 200 lines each)
- All code testable in isolation

### Phase 2: State Management Migration

**Priority:** HIGH  
**Effort:** 80 hours  
**Risk:** MEDIUM

#### 2.1 Implement Zustand Stores (Week 8-9)

**Why Zustand?**
- Minimal boilerplate (vs Redux)
- TypeScript-first
- No Provider hell (vs Context)
- Built-in persistence middleware
- Devtools support

**Store Structure:**

**`src/stores/forecastStore.ts`**
```typescript
import create from 'zustand';
import { persist } from 'zustand/middleware';

interface ForecastStore {
  // State
  forecasts: Map<string, Forecast>;
  activeForecastId: string | null;
  
  // Computed
  get activeForecast(): Forecast | null;
  get activeForecasts(): Forecast[];
  get expiredForecasts(): Forecast[];
  
  // Actions
  loadForecasts: () => Promise<void>;
  createForecast: (question: string) => Promise<string>;
  updateForecast: (id: string, updates: Partial<Forecast>) => Promise<void>;
  deleteForecast: (id: string) => Promise<void>;
  setActiveForecast: (id: string) => void;
}

export const useForecastStore = create<ForecastStore>()(
  persist(
    (set, get) => ({
      forecasts: new Map(),
      activeForecastId: null,
      
      get activeForecast() {
        const id = get().activeForecastId;
        return id ? get().forecasts.get(id) || null : null;
      },
      
      loadForecasts: async () => {
        const forecasts = await loadForecastsWithSync();
        set({ forecasts: new Map(forecasts.map(f => [f.id, f])) });
      },
      
      // ... other actions
    }),
    {
      name: 'forecast-storage',
      storage: createAsyncStorageAdapter(), // Custom adapter
    }
  )
);
```

**`src/stores/driverStore.ts`** - Driver state management  
**`src/stores/agentStore.ts`** - Agent configurations  
**`src/stores/uiStore.ts`** - UI state (modals, toasts, etc)  
**`src/stores/coachingStore.ts`** - Coaching state and history

#### 2.2 Sync Middleware (Week 10)

Create Zustand middleware that integrates with `backendSync.ts`:

**`src/stores/middleware/syncMiddleware.ts`**
```typescript
import { StateCreator, StoreMutatorIdentifier } from 'zustand';

type SyncMiddleware = <
  T extends object,
  Mps extends [StoreMutatorIdentifier, unknown][] = [],
  Mcs extends [StoreMutatorIdentifier, unknown][] = []
>(
  f: StateCreator<T, Mps, Mcs>,
  syncConfig: SyncConfig
) => StateCreator<T, Mps, Mcs>;

interface SyncConfig {
  mode: 'local-only' | 'backend-primary' | 'backend-only';
  syncActions: string[]; // Actions that trigger sync
  transformer: {
    toBackend: (state: any) => any;
    fromBackend: (data: any) => any;
  };
}

export const syncMiddleware: SyncMiddleware = (f, config) => (set, get, store) => {
  // Wrap set() to detect changes and sync
  const syncedSet = (partial, replace) => {
    const prevState = get();
    set(partial, replace);
    const nextState = get();
    
    // Detect which actions changed
    const changedActions = detectChanges(prevState, nextState);
    
    // Sync if needed
    if (changedActions.some(a => config.syncActions.includes(a))) {
      syncToBackend(nextState, config);
    }
  };
  
  return f(syncedSet, get, store);
};
```

### Phase 3: CLI Intelligence Layer

**Priority:** HIGH  
**Effort:** 100 hours  
**Risk:** MEDIUM

#### 3.1 Coaching Service (Week 11-12)

**`src/services/CoachingService.ts`**
```typescript
export class CoachingService {
  private userProfile: UserProfile;
  private conversationHistory: ConversationTurn[];
  
  // Analyze user's forecasting level
  async detectUserLevel(): Promise<CoachingState> {
    const forecasts = await this.getUserForecasts();
    const brierScores = forecasts.filter(f => f.resolved).map(f => f.brierScore);
    
    if (forecasts.length === 0) return "new_user";
    if (forecasts.length <= 5) return "learning";
    if (forecasts.length <= 20) return "intermediate";
    
    const avgBrier = brierScores.reduce((a, b) => a + b, 0) / brierScores.length;
    if (avgBrier < 0.15) return "superforecaster";
    
    return "advanced";
  }
  
  // Get context-aware suggestions
  async getSuggestions(context: ForecastContext): Promise<SmartSuggestion[]> {
    const level = await this.detectUserLevel();
    const mistakes = await this.detectMistakes(context);
    const strengths = await this.detectStrengths(context);
    
    return this.generateSuggestions(level, context, mistakes, strengths);
  }
  
  // Detect common mistakes
  private async detectMistakes(context: ForecastContext): Promise<string[]> {
    const mistakes: string[] = [];
    
    // Too few drivers
    if (context.drivers.length < 3) {
      mistakes.push("too_few_drivers");
    }
    
    // No evidence
    const driversWithoutEvidence = context.drivers.filter(d => !d.evidence || d.evidence.length === 0);
    if (driversWithoutEvidence.length > 0) {
      mistakes.push("missing_evidence");
    }
    
    // Overconfident ranges
    const narrowRanges = context.drivers.filter(d => {
      if (d.distributionType === "triangular") {
        const range = (d.parameters.high - d.parameters.low) / d.parameters.mode;
        return range < 0.3; // Less than ±15%
      }
      return false;
    });
    if (narrowRanges.length > 0) {
      mistakes.push("overconfident_ranges");
    }
    
    // No base rate
    if (!context.externalView || !context.externalView.baseRate) {
      mistakes.push("missing_base_rate");
    }
    
    // No premortem
    if (!context.premortem || context.premortem.failureScenarios.length === 0) {
      mistakes.push("missing_premortem");
    }
    
    return mistakes;
  }
  
  // Generate coaching interventions
  private generateSuggestions(
    level: CoachingState,
    context: ForecastContext,
    mistakes: string[],
    strengths: string[]
  ): SmartSuggestion[] {
    const suggestions: SmartSuggestion[] = [];
    
    // Prioritize by user level
    if (level === "new_user" || level === "learning") {
      // High guidance for new users
      suggestions.push(...this.getBeginnerSuggestions(context, mistakes));
    } else {
      // Lower guidance for experienced users
      suggestions.push(...this.getAdvancedSuggestions(context, mistakes));
    }
    
    return suggestions.sort((a, b) => {
      const priorityOrder = { high: 3, medium: 2, low: 1 };
      return priorityOrder[b.priority] - priorityOrder[a.priority];
    });
  }
}

export const coachingService = new CoachingService();
```

#### 3.2 Conversational Context Tracking (Week 13)

**`src/services/ConversationService.ts`**
```typescript
interface ConversationContext {
  turns: ConversationTurn[];
  currentIntent?: string;
  entities: Map<string, any>;
  waitingFor?: string;
}

export class ConversationService {
  private context: ConversationContext;
  
  // Parse natural language input
  async parseInput(input: string): Promise<ParsedInput> {
    // Extract intent
    const intent = await this.detectIntent(input);
    
    // Extract entities
    const entities = this.extractEntities(input);
    
    // Check if completing previous intent
    if (this.context.waitingFor) {
      return this.completeIntent(this.context.waitingFor, entities);
    }
    
    return { intent, entities };
  }
  
  // Detect user intent from text
  private async detectIntent(input: string): Promise<string> {
    // Simple rule-based for now, could use LLM later
    const patterns = {
      'add_driver': /add|create.*driver/i,
      'set_parameter': /\d+|low|high|mode|mean/i,
      'add_evidence': /evidence|source|link/i,
      'run_simulation': /simulate|run|calculate/i,
    };
    
    for (const [intent, pattern] of Object.entries(patterns)) {
      if (pattern.test(input)) {
        return intent;
      }
    }
    
    return 'unknown';
  }
  
  // Extract entities (numbers, names, etc)
  private extractEntities(input: string): Map<string, any> {
    const entities = new Map();
    
    // Extract numbers
    const numbers = input.match(/\d+(\.\d+)?/g);
    if (numbers) {
      entities.set('numbers', numbers.map(parseFloat));
    }
    
    // Extract URLs
    const urls = input.match(/https?:\/\/[^\s]+/g);
    if (urls) {
      entities.set('urls', urls);
    }
    
    return entities;
  }
  
  // Multi-turn conversation handling
  async handleMultiTurn(input: string): Promise<ConversationResponse> {
    const parsed = await this.parseInput(input);
    
    // Add to conversation history
    this.context.turns.push({
      role: 'user',
      input,
      parsed
    });
    
    // Generate response
    const response = await this.generateResponse(parsed);
    
    this.context.turns.push({
      role: 'fermi',
      message: response.message
    });
    
    return response;
  }
}
```

#### 3.3 Enhanced Command System (Week 14)

Extend `fermiCommands.ts` with intelligence:

**`src/services/IntelligentCommandSystem.ts`**
```typescript
export class IntelligentCommandSystem {
  // Fuzzy command matching
  findCommand(input: string): Command | null {
    // Try exact match first
    let cmd = COMMANDS[input];
    if (cmd) return cmd;
    
    // Try fuzzy match
    const candidates = Object.keys(COMMANDS);
    const matches = candidates.map(name => ({
      name,
      distance: levenshtein(input, name)
    })).sort((a, b) => a.distance - b.distance);
    
    // Accept if within threshold
    if (matches[0].distance <= 2) {
      return COMMANDS[matches[0].name];
    }
    
    return null;
  }
  
  // Auto-complete with context
  getAutocompleteSuggestions(
    partial: string,
    context: CommandContext
  ): CommandSuggestion[] {
    // Filter by context
    const availableCommands = Object.values(COMMANDS)
      .filter(cmd => cmd.contexts.includes(context) || cmd.contexts.includes("any"));
    
    // Match partial input
    const matches = availableCommands
      .filter(cmd => cmd.name.startsWith(partial) || cmd.syntax.includes(partial))
      .slice(0, 5);
    
    // Add context-aware suggestions even if not matching
    const contextSuggestions = this.getContextSuggestions(context);
    
    return [...matches, ...contextSuggestions].slice(0, 5);
  }
  
  // Suggest commands based on context
  private getContextSuggestions(context: CommandContext): CommandSuggestion[] {
    const suggestions: CommandSuggestion[] = [];
    
    if (context === "forecast_active") {
      const drivers = this.getCurrentDrivers();
      if (drivers.length === 0) {
        suggestions.push({
          command: "/driver",
          description: "Add your first driver",
          priority: "high"
        });
      } else if (drivers.some(d => !d.evidence || d.evidence.length === 0)) {
        suggestions.push({
          command: "/agent research",
          description: "Research evidence for drivers",
          priority: "high"
        });
      } else {
        suggestions.push({
          command: "/simulate",
          description: "Run simulation",
          priority: "medium"
        });
      }
    }
    
    return suggestions;
  }
}
```

### Phase 4: Component Refactoring

**Priority:** MEDIUM  
**Effort:** 100 hours  
**Risk:** MEDIUM

#### 4.1 Create Shared UI Components (Week 15-16)

**`src/components/ui/`**
```
Button.tsx           - Standardized button component
Input.tsx            - Text input with validation
Select.tsx           - Dropdown select
Card.tsx             - Card container
Modal.tsx            - Modal dialog
LoadingSpinner.tsx   - Loading indicator
ErrorMessage.tsx     - Error display
Toast.tsx            - Toast notification
FormField.tsx        - Form field wrapper
Tooltip.tsx          - Tooltip component
Badge.tsx            - Badge/tag component
Tabs.tsx             - Tab navigation
```

Each component follows Tufte design system.

#### 4.2 Refactor Large Screens (Week 17-18)

**CreateForecastScreen.tsx** (725 → 200 lines)
- Extract form logic to `useCreateForecastForm` hook
- Extract parsing logic to `ForecastParser` service
- Create `ForecastForm` sub-component

**ForecastInputScreen.tsx** (692 → 180 lines)
- Extract to `useQuestionInput` hook
- Create `QuestionParser` component
- Create `DriverSuggestions` component

**PromptBuilder.tsx** (630 → 200 lines)
- Extract to `usePromptBuilder` hook
- Create `PromptTemplate` component
- Create `PromptVariables` component

#### 4.3 Create Compound Components (Week 19)

**`src/components/DriverConfiguration/`**
```typescript
// Compound component pattern
export function DriverConfiguration({ driver, onSave, onCancel }) {
  return (
    <DriverConfiguration.Container>
      <DriverConfiguration.Header driver={driver} />
      <DriverConfiguration.Form driver={driver} />
      <DriverConfiguration.Evidence driver={driver} />
      <DriverConfiguration.Actions onSave={onSave} onCancel={onCancel} />
    </DriverConfiguration.Container>
  );
}

DriverConfiguration.Container = DriverContainer;
DriverConfiguration.Header = DriverHeader;
DriverConfiguration.Form = DriverForm;
DriverConfiguration.Evidence = EvidenceSection;
DriverConfiguration.Actions = ActionButtons;
```

### Phase 5: Data Flow Optimization

**Priority:** LOW  
**Effort:** 40 hours  
**Risk:** LOW

#### 5.1 Implement Optimistic Updates (Week 20)

**`src/stores/middleware/optimisticMiddleware.ts`**
```typescript
export const optimisticMiddleware = (config) => (set, get, store) => {
  return (action) => {
    // Apply optimistically
    set(action);
    
    // Sync to backend
    syncToBackend(action)
      .catch(error => {
        // Rollback on failure
        set(config.rollback(action));
        showToast("Failed to sync: " + error.message);
      });
  };
};
```

#### 5.2 Add Request Deduplication (Week 20)

**`src/api/queryCache.ts`**
```typescript
export class QueryCache {
  private cache = new Map<string, CacheEntry>();
  private pending = new Map<string, Promise<any>>();
  
  async query<T>(key: string, fn: () => Promise<T>, ttl = 60000): Promise<T> {
    // Check cache
    const cached = this.cache.get(key);
    if (cached && Date.now() - cached.timestamp < ttl) {
      return cached.data;
    }
    
    // Check if already pending
    if (this.pending.has(key)) {
      return this.pending.get(key);
    }
    
    // Execute query
    const promise = fn().then(data => {
      this.cache.set(key, { data, timestamp: Date.now() });
      this.pending.delete(key);
      return data;
    });
    
    this.pending.set(key, promise);
    return promise;
  }
}
```

### Phase 6: Type Safety Improvements

**Priority:** LOW  
**Effort:** 60 hours  
**Risk:** LOW

#### 6.1 Remove `any` Types (Week 21-22)

Search and replace all `any` with proper types:
```bash
# Find all instances
grep -r "any" src/ --include="*.ts" --include="*.tsx"

# Create proper interfaces for each
```

#### 6.2 Add Discriminated Unions (Week 22)

Example:
```typescript
// Before
interface Command {
  type: string;
  payload: any;
}

// After
type Command = 
  | { type: "ADD_DRIVER"; payload: ForecastDriver }
  | { type: "UPDATE_DRIVER"; payload: { id: string; updates: Partial<ForecastDriver> } }
  | { type: "REMOVE_DRIVER"; payload: { id: string } };
```

---

## 5. API & Backend Changes

### Current Issues

1. **Inconsistent response formats** - Some endpoints return `{ success, data }`, others just data
2. **No API versioning** - Breaking changes will affect all clients
3. **Limited error information** - Just error messages, no codes or retry hints
4. **No pagination** - Could cause performance issues with many forecasts
5. **Tight coupling** - Frontend knows too much about backend data structure

### Proposed API Improvements

#### 5.1 Standardized Response Format

```typescript
// All endpoints return this structure
interface APIResponse<T> {
  success: boolean;
  data?: T;
  error?: {
    code: string;
    message: string;
    details?: any;
    retryable: boolean;
  };
  meta?: {
    version: string;
    timestamp: string;
    requestId: string;
  };
}
```

#### 5.2 API Versioning

```
Current: /api/forecasts
Proposed: /api/v1/forecasts

// Maintain backwards compatibility
/api/forecasts → redirects to latest stable version
```

#### 5.3 Enhanced Forecast Endpoints

**GET /api/v1/forecasts**
```typescript
// Request
GET /api/v1/forecasts?status=active&limit=20&offset=0&sort=-updatedAt

// Response
{
  success: true,
  data: {
    forecasts: Forecast[],
    pagination: {
      total: 150,
      limit: 20,
      offset: 0,
      hasMore: true
    }
  }
}
```

**POST /api/v1/forecasts**
```typescript
// Request
POST /api/v1/forecasts
{
  question: string,
  domain?: string,
  timeframe: { start: string, end: string }
}

// Response
{
  success: true,
  data: {
    forecast: Forecast,
    suggestions: {
      drivers: string[],  // Suggested drivers
      referenceClass: string,  // For base rate
      similarForecasts: string[]  // IDs of similar forecasts
    }
  }
}
```

#### 5.4 Driver Management Endpoints

**POST /api/v1/forecasts/:id/drivers**
```typescript
// Request
POST /api/v1/forecasts/abc123/drivers
{
  driver: ForecastDriver,
  autoResearch?: boolean  // Trigger research automatically
}

// Response
{
  success: true,
  data: {
    driver: ForecastDriver,
    hints: DriverHint[],  // From fermiHints.ts
    researchTask?: string  // ID of triggered research task
  }
}
```

#### 5.5 Simulation Endpoints

**POST /api/v1/forecasts/:id/simulate**
```typescript
// Request
POST /api/v1/forecasts/abc123/simulate
{
  iterations?: number,  // Default 10000
  reason?: string  // Why running simulation
}

// Response
{
  success: true,
  data: {
    simulation: Simulation,
    warnings: Warning[],  // Quality warnings
    suggestions: string[]  // Improvement suggestions
  }
}
```

#### 5.6 Coaching Endpoints (NEW)

**GET /api/v1/coaching/profile**
```typescript
// Response
{
  success: true,
  data: {
    userId: string,
    level: CoachingState,
    stats: {
      totalForecasts: number,
      resolvedForecasts: number,
      avgBrierScore: number,
      calibration: number
    },
    strengths: string[],
    improvementAreas: string[]
  }
}
```

**POST /api/v1/coaching/suggestions**
```typescript
// Request
POST /api/v1/coaching/suggestions
{
  forecastId: string,
  context: ForecastContext
}

// Response
{
  success: true,
  data: {
    suggestions: SmartSuggestion[],
    interventions: CoachingIntervention[]
  }
}
```

### Database Schema Changes

#### New Tables

**coaching_profiles**
```sql
CREATE TABLE coaching_profiles (
  user_id TEXT PRIMARY KEY,
  level TEXT NOT NULL, -- new_user | learning | intermediate | advanced | superforecaster
  total_forecasts INTEGER DEFAULT 0,
  resolved_forecasts INTEGER DEFAULT 0,
  avg_brier_score REAL,
  calibration_score REAL,
  strengths TEXT[], -- Array of strength IDs
  improvement_areas TEXT[], -- Array of improvement area IDs
  hints_shown TEXT[], -- Array of hint IDs shown
  hints_dismissed TEXT[], -- Array of hint IDs dismissed
  last_active_at TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW()
);
```

**conversation_history**
```sql
CREATE TABLE conversation_history (
  id TEXT PRIMARY KEY,
  forecast_id TEXT REFERENCES forecasts(id),
  user_id TEXT NOT NULL,
  thread_id TEXT,
  role TEXT NOT NULL, -- user | fermi
  message TEXT NOT NULL,
  rich_content JSONB, -- Structured content
  actions JSONB[], -- Executable actions
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_conversation_forecast ON conversation_history(forecast_id);
CREATE INDEX idx_conversation_thread ON conversation_history(thread_id);
```

**coaching_interventions**
```sql
CREATE TABLE coaching_interventions (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  forecast_id TEXT REFERENCES forecasts(id),
  intervention_type TEXT NOT NULL,
  trigger TEXT NOT NULL, -- What triggered this
  message TEXT NOT NULL,
  actions JSONB[], -- Suggested actions
  user_response TEXT, -- accepted | dismissed | modified
  created_at TIMESTAMP DEFAULT NOW(),
  responded_at TIMESTAMP
);

CREATE INDEX idx_interventions_user ON coaching_interventions(user_id);
CREATE INDEX idx_interventions_forecast ON coaching_interventions(forecast_id);
```

#### Modified Tables

**forecasts** - Add fields:
```sql
ALTER TABLE forecasts ADD COLUMN domain TEXT;
ALTER TABLE forecasts ADD COLUMN timeframe_start TIMESTAMP;
ALTER TABLE forecasts ADD COLUMN timeframe_end TIMESTAMP;
ALTER TABLE forecasts ADD COLUMN quality_score REAL; -- Computed quality metric
ALTER TABLE forecasts ADD COLUMN coaching_notes JSONB; -- Coaching feedback
```

**drivers** - New table (currently embedded in forecast):
```sql
CREATE TABLE drivers (
  id TEXT PRIMARY KEY,
  forecast_id TEXT REFERENCES forecasts(id),
  name TEXT NOT NULL,
  description TEXT,
  distribution_type TEXT NOT NULL,
  distribution_params JSONB NOT NULL,
  distribution_rationale TEXT,
  confidence_level TEXT,
  unit TEXT,
  version_major INTEGER DEFAULT 1,
  version_minor INTEGER DEFAULT 0,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_drivers_forecast ON drivers(forecast_id);
```

**evidence** - New table (currently embedded in driver):
```sql
CREATE TABLE evidence (
  id TEXT PRIMARY KEY,
  driver_id TEXT REFERENCES drivers(id),
  type TEXT NOT NULL,
  title TEXT NOT NULL,
  source TEXT,
  url TEXT,
  summary TEXT,
  key_finding TEXT,
  relevance TEXT,
  quality TEXT,
  added_by TEXT, -- user | agent
  agent_id TEXT, -- If added by agent
  link_preview JSONB,
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_evidence_driver ON evidence(driver_id);
CREATE INDEX idx_evidence_agent ON evidence(agent_id);
```

**simulations** - New table (currently embedded in forecast):
```sql
CREATE TABLE simulations (
  id TEXT PRIMARY KEY,
  forecast_id TEXT REFERENCES forecasts(id),
  run_by TEXT, -- user | auto
  reason TEXT,
  iterations INTEGER NOT NULL,
  probability REAL NOT NULL,
  distribution JSONB NOT NULL, -- Percentiles and histogram
  driver_snapshot JSONB NOT NULL, -- State at time of run
  runtime_ms INTEGER,
  cost REAL,
  warnings JSONB[],
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_simulations_forecast ON simulations(forecast_id);
CREATE INDEX idx_simulations_created ON simulations(created_at DESC);
```

### Migration Strategy

1. **Phase 1**: Add new tables alongside existing structure
2. **Phase 2**: Migrate data from embedded format to relational
3. **Phase 3**: Update application to use new structure
4. **Phase 4**: Remove old embedded fields (after verification)

---

## 6. Testing Strategy

### Current State

- No tests mentioned in codebase audit
- Manual testing only
- High risk for regressions

### Proposed Testing Approach

#### 6.1 Unit Tests

**Test all services, hooks, and utilities:**
```
src/
  hooks/
    __tests__/
      useForecastState.test.ts
      useDriverConfiguration.test.ts
      ...
  services/
    __tests__/
      forecastService.test.ts
      coachingService.test.ts
      ...
  utils/
    __tests__/
      commandParser.test.ts
      validation.test.ts
      ...
```

**Tools:**
- Jest for test runner
- React Testing Library for hooks
- MSW for API mocking

#### 6.2 Integration Tests

Test key workflows:
```typescript
// tests/integration/forecastCreation.test.ts
describe("Forecast Creation Flow", () => {
  it("should create forecast with guided flow", async () => {
    // Start with question
    const { result } = renderHook(() => useForecastState());
    await act(async () => {
      await result.current.createForecast("Will X reach Y?");
    });
    
    // Add drivers
    await act(async () => {
      await result.current.addDriver({ /* ... */ });
    });
    
    // Run simulation
    const simulation = await result.current.runSimulation();
    expect(simulation.result.probability).toBeGreaterThan(0);
  });
});
```

#### 6.3 E2E Tests

Test full user journeys:
```typescript
// e2e/newUserJourney.spec.ts
describe("New User Journey", () => {
  it("should guide new user through first forecast", async () => {
    // Open app
    await page.goto("/");
    
    // See welcome message
    await expect(page.locator("text=Welcome")).toBeVisible();
    
    // Type question
    await page.fill("[data-testid=command-input]", "/question Will AMD reach $200?");
    await page.press("[data-testid=command-input]", "Enter");
    
    // See coaching prompt
    await expect(page.locator("text=Let's break this down")).toBeVisible();
    
    // Add driver
    await page.fill("[data-testid=command-input]", "/driver Market Size");
    // ... etc
  });
});
```

**Tools:**
- Playwright for E2E testing
- Test on iOS, Android, Web

#### 6.4 Visual Regression Tests

Ensure UI changes don't break design:
```typescript
// tests/visual/components.spec.ts
describe("Component Visual Regression", () => {
  it("should match ForecastCard snapshot", async () => {
    await page.goto("/storybook?path=/story/forecastcard--default");
    expect(await page.screenshot()).toMatchSnapshot("forecastcard-default.png");
  });
});
```

**Tools:**
- Storybook for component isolation
- Chromatic or Percy for visual diffing

---

## 7. Implementation Timeline

### Team Structure

**Recommended Team:**
- 1 Senior Frontend Engineer (React Native + Zustand expert)
- 1 Mid-level Frontend Engineer (TypeScript + UI components)
- 1 Backend Engineer (API + Database)
- 1 QA Engineer (Testing strategy)
- 1 UX Designer (Coaching flows)

### Milestones

**Month 1: Emergency Triage**
- Week 1-2: Extract custom hooks from ForecastWorkspaceScreen
- Week 3-4: Extract sub-components
- Week 5-6: Extract services
- Week 7: Reorganize utilities
- **Deliverable:** ForecastWorkspaceScreen reduced to <500 lines

**Month 2: State Management & CLI Intelligence**
- Week 8-9: Implement Zustand stores
- Week 10: Create sync middleware
- Week 11-12: Build CoachingService
- Week 13: Implement ConversationService
- Week 14: Enhance command system
- **Deliverable:** Global state management + Intelligent coaching

**Month 3: Component Refactoring & Testing**
- Week 15-16: Create shared UI components
- Week 17-18: Refactor large screens
- Week 19: Build compound components
- Week 20: Implement optimistic updates
- Week 21-22: Remove `any` types, add tests
- **Deliverable:** Polished components + Test coverage

**Month 4: Backend Integration & Launch**
- Week 23-24: Update API endpoints
- Week 25: Migrate database schema
- Week 26: E2E testing
- Week 27: Beta testing
- Week 28: Production launch
- **Deliverable:** Fully refactored app in production

### Risk Mitigation

**High-Risk Areas:**
1. **ForecastWorkspaceScreen refactor** - Most complex, highest chance of bugs
   - Mitigation: Incremental refactor, feature flags, extensive testing
   
2. **State management migration** - Could break existing functionality
   - Mitigation: Parallel implementation, gradual migration per screen
   
3. **Backend API changes** - Could break mobile app
   - Mitigation: API versioning, backwards compatibility, staged rollout

---

## 8. Success Metrics

### Code Quality Metrics

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| Largest file | 8,975 lines | <500 lines | 🔴 Critical |
| Files >500 lines | 10 files | 0 files | 🔴 High |
| useState per component | 25+ | <5 | 🔴 High |
| Test coverage | 0% | 80%+ | 🔴 Critical |
| TypeScript `any` types | 50+ | 0 | 🟡 Medium |
| Duplicate code | 27+ patterns | <5 | 🟡 Medium |
| Build time | Unknown | <30s | 🟢 OK |

### User Experience Metrics

| Metric | Current | Target |
|--------|---------|--------|
| New user completion rate | Unknown | 60%+ |
| Average drivers per forecast | Unknown | 5-7 |
| Evidence items per driver | Unknown | 2-3 |
| Simulations per forecast | Unknown | 3-5 |
| Time to first forecast | Unknown | <10 min |
| Coaching acceptance rate | N/A | 70%+ |
| Brier score (all users) | Unknown | <0.25 |
| Brier score (active users) | Unknown | <0.20 |

### Performance Metrics

| Metric | Target |
|--------|--------|
| App load time | <2s |
| Command execution | <100ms |
| Simulation (10k iterations) | <3s |
| Backend sync latency | <500ms |
| Offline functionality | 100% |

---

## 9. Post-Launch Improvements

### Phase 7: Advanced Features (Month 5+)

**1. Collaborative Forecasting**
- Share forecasts with team
- Comment and suggest edits
- Track team calibration

**2. Forecast Templates**
- Save forecast as template
- Template marketplace
- Company-specific templates

**3. Advanced Analytics**
- Decomposition analysis (which drivers matter most?)
- Sensitivity analysis
- What-if scenarios
- Historical accuracy tracking

**4. AI Pair Forecasting**
- AI suggests drivers based on question
- AI researches all drivers automatically
- AI suggests parameter ranges from evidence
- AI writes premortem scenarios
- Human reviews and adjusts

**5. Integration with External Data**
- Stock market APIs
- News APIs
- Company financials (SEC filings)
- Social media sentiment
- Auto-update forecasts with new data

---

## 10. Open Questions & Decisions Needed

### Technical Decisions

1. **State Management:** Zustand vs Redux Toolkit vs Jotai?
   - Recommendation: Zustand (simpler, less boilerplate)
   
2. **Testing Framework:** Jest + RTL vs Vitest?
   - Recommendation: Jest (more mature, better RN support)
   
3. **E2E Testing:** Playwright vs Detox?
   - Recommendation: Detox (better RN support, though slower)
   
4. **Backend Hosting:** Keep Vercel or move to dedicated?
   - Recommendation: Keep Vercel (works well, easy deployment)

### UX Decisions

1. **Coaching Intensity:** How aggressive should coaching be?
   - Need user testing to find balance
   
2. **Natural Language:** Should we support full NL or keep CLI syntax?
   - Recommendation: Hybrid (NL for new users, CLI for power users)
   
3. **Mobile vs Desktop:** Optimize for which primarily?
   - Recommendation: Desktop first (complex workflows), mobile second
   
4. **Onboarding:** Required tutorial or optional?
   - Recommendation: Optional but strongly encouraged

### Business Decisions

1. **Open Source:** Is this project open source?
   - Affects: Code quality, documentation needs
   
2. **Pricing:** Freemium or paid only?
   - Affects: Feature gating, API limits
   
3. **Target Audience:** Individuals or organizations?
   - Affects: Collaboration features priority

---

## 11. Clarifying Questions

### About the Product

1. **User Base:** Who are the primary users? (Researchers? Analysts? Teams?)
2. **Use Cases:** Most common forecasting scenarios?
3. **Success Definition:** What does a successful forecast look like?
4. **Monetization:** How will this be monetized?

### About the Architecture

1. **Backend Access:** Do we have full access to backend code?
2. **Breaking Changes:** Can we make breaking changes to API?
3. **Database Migrations:** Can we run migrations on production DB?
4. **Feature Flags:** Do we have a feature flag system?

### About the Team

1. **Development Team:** How many developers available?
2. **Design Resources:** Do we have dedicated UX designer?
3. **Testing Resources:** Do we have QA team?
4. **Timeline Constraints:** Hard deadline or flexible?

### About Users

1. **Current Users:** How many active users?
2. **Beta Testers:** Can we recruit beta testers for new features?
3. **Feedback:** Do we have user feedback on current version?
4. **Usage Data:** Do we have analytics on current usage patterns?

---

## 12. Next Steps

### Immediate Actions (This Week)

1. **Review this document** with team and stakeholders
2. **Answer open questions** in Section 11
3. **Prioritize phases** based on business goals
4. **Set up development environment** for refactoring
5. **Create feature flags** for incremental rollout
6. **Set up tracking** for success metrics

### Week 1 Tasks

1. **Extract `useToast` hook** (Quick Win #1)
2. **Extract command parser** (Quick Win #2)
3. **Create driver update helper** (Quick Win #3)
4. **Set up Jest + RTL** for testing
5. **Create first unit tests** for services

### Ongoing

1. **Daily standups** to track progress
2. **Weekly demos** of refactored components
3. **Bi-weekly retros** to adjust plan
4. **User testing** of coaching features
5. **Documentation** as we build

---

## Appendix A: Key Files Reference

### Files to Refactor (Priority Order)

1. **src/screens/ForecastWorkspaceScreen.tsx** (8,975 lines) - CRITICAL
2. **src/screens/CreateForecastScreen.tsx** (725 lines)
3. **src/screens/ForecastInputScreen.tsx** (692 lines)
4. **src/components/PromptBuilder.tsx** (630 lines)
5. **src/screens/CompareScreen.tsx** (607 lines)
6. **src/screens/CalibrationScreen.tsx** (579 lines)
7. **src/components/EvidenceManager.tsx** (519 lines)

### Files to Use as Reference (Well-Designed)

1. **src/services/forecastService.ts** (209 lines) - Clean service pattern
2. **src/services/authService.ts** (275 lines) - Good state management
3. **src/styles/tufte.ts** - Excellent design system
4. **src/utils/probability.ts** - Good utility functions

### New Files to Create

#### Hooks (src/hooks/)
- useForecastState.ts
- useDriverConfiguration.ts
- useAgentConfiguration.ts
- useFermiChat.ts
- useCommandExecution.ts
- useVersionControl.ts
- useBackendSync.ts
- useToast.ts
- useCoaching.ts

#### Services (src/services/)
- ForecastStateService.ts
- DriverService.ts
- AgentService.ts
- VersionControlService.ts
- CoachingService.ts
- ConversationService.ts
- IntelligentCommandSystem.ts

#### Components (src/screens/ForecastWorkspace/components/)
- QuestionHeader.tsx
- ExternalViewCard.tsx
- SimulationChart.tsx
- DriverConfigPanel.tsx
- AgentConfigPanel.tsx
- EvidenceSection.tsx
- DriverList.tsx
- ForecastListPanel.tsx
- LeaderboardPanel.tsx
- FermiChatInterface.tsx
- CommandAutocomplete.tsx
- CoachingPrompts.tsx
- ToastNotification.tsx

#### Stores (src/stores/)
- forecastStore.ts
- driverStore.ts
- agentStore.ts
- uiStore.ts
- coachingStore.ts

---

## Appendix B: Coaching Examples

### Example 1: New User - First Forecast

```
User: /question Will Tesla stock reach $500 by end of 2026?

Fermi: "Welcome! 🎉 Let's create your first forecast together.

First, let's start with the 'outside view' - what happened historically?

Reference class: 'Major tech stocks doubling in 2 years'
Base rate: 12% (based on S&P 500 data, n=500 companies)

This means historically, about 1 in 8 major tech stocks double in 2 years.

Now let's think about what makes Tesla different.
What key factors will influence whether Tesla reaches $500?

Common drivers for stock forecasts:
• Company revenue growth
• Market conditions
• Competition
• Regulations

Type: /driver Revenue Growth
(I'll guide you through setting this up)"
```

### Example 2: Intermediate User - Overconfidence

```
User: /range 180 200 220  (for Revenue driver)

Fermi: "⚠️ That's a very narrow range (±10%).

Your forecast history shows:
• You tend to use ranges of ±8-12%
• Your ranges were too narrow in 7 out of 10 forecasts
• When you widened ranges, accuracy improved by 18%

Typical revenue ranges by industry:
• Enterprise SaaS: ±40%
• Consumer SaaS: ±60%
• Biotech: ±80%

For this driver (SaaS revenue), I recommend ±35-50%

Suggested ranges:
/range 100 200 300  (±50%, conservative)
/range 130 200 270  (±35%, moderate)
/keep 180 200 220  (±10%, your choice)

Your call - what fits your evidence?"
```

### Example 3: Advanced User - Quality Check

```
User: /simulate

Fermi: "Running quick quality check before simulation...

✅ Drivers: 6 drivers (excellent coverage)
✅ Evidence: 14 total items (good)
⚠️  Base rate: Not set (missing outside view)
✅ Premortem: 3 failure scenarios
⚠️  Driver: 'Market Size' - only 1 evidence item from 2024

Recommendations:
1. Add base rate: /base-rate [reference class]
2. Update Market Size evidence: /agent research Market size --filter recent

Proceed anyway? /simulate --skip-checks
Or fix issues first?"
```

---

**END OF DOCUMENT**

---

This plan is ready for team discussion. Please review and provide feedback on:
1. Priorities and timeline
2. Technical decisions
3. Open questions
4. Resource allocation
5. Any concerns or alternative approaches

Let's build something great! 🚀
