# Fermi Agent - Broca Brain Architecture
**The Forecasting Programming Language (FPL) Processing Engine**

---

## Overview

Fermi's "Broca brain" is the language processing center that parses, validates, and executes the Forecasting Programming Language (FPL). This is the core reasoning engine that transforms natural language and structured commands into executable forecasting models.

---

## Complete System Architecture

```mermaid
graph TB
    subgraph "User Interface Layer"
        CLI[CLI Interface]
        NLU[Natural Language Input]
        REPL[REPL Environment]
    end
    
    subgraph "Broca Brain - Language Processing Core"
        subgraph "Lexer Stage"
            LEX[Lexer/Tokenizer]
            TOK[Token Stream]
        end
        
        subgraph "Parser Stage"
            PAR[FPL Parser]
            AST[Abstract Syntax Tree]
        end
        
        subgraph "Semantic Analysis"
            SEM[Semantic Analyzer]
            TYPE[Type Checker]
            VAL[Validator]
            SYMT[Symbol Table]
        end
        
        subgraph "Compilation"
            IR[Intermediate Representation]
            OPT[Optimizer]
        end
    end
    
    subgraph "Execution Engine"
        subgraph "Interpreter"
            EXEC[Executor]
            ENV[Environment/Context]
        end
        
        subgraph "Runtime Services"
            DIST[Distribution Engine]
            MONTE[Monte Carlo Simulator]
            AGENT[Agent Orchestrator]
            STORE[State Manager]
        end
    end
    
    subgraph "Fermi Coach - Intelligence Layer"
        COACH[Coaching Engine]
        HINT[Hint Generator]
        ERR[Error Explainer]
        SUGGEST[Suggestion Engine]
    end
    
    subgraph "Knowledge Base"
        DOMAIN[Domain Knowledge]
        HIST[Historical Data]
        PATTERN[Pattern Library]
        TEMPLATE[Forecast Templates]
    end
    
    subgraph "External Integrations"
        LLM[LLM APIs - Claude/GPT]
        DATA[Data Sources]
        DB[Database]
    end

    %% User Input Flow
    CLI --> LEX
    NLU --> LEX
    REPL --> LEX
    
    %% Lexical Analysis
    LEX --> TOK
    TOK --> PAR
    
    %% Parsing
    PAR --> AST
    AST --> SEM
    
    %% Semantic Analysis
    SEM --> TYPE
    TYPE --> VAL
    VAL --> SYMT
    SYMT --> IR
    
    %% Compilation
    IR --> OPT
    OPT --> EXEC
    
    %% Execution
    EXEC --> ENV
    ENV --> DIST
    ENV --> MONTE
    ENV --> AGENT
    ENV --> STORE
    
    %% Coach Integration
    SEM -.coaching.-> COACH
    VAL -.coaching.-> COACH
    EXEC -.coaching.-> COACH
    COACH --> HINT
    COACH --> ERR
    COACH --> SUGGEST
    
    %% Knowledge Access
    COACH --> DOMAIN
    COACH --> HIST
    COACH --> PATTERN
    EXEC --> TEMPLATE
    
    %% External Services
    AGENT --> LLM
    DIST --> DATA
    STORE --> DB
    
    %% Feedback Loop
    SUGGEST -.suggestions.-> CLI
    ERR -.errors.-> CLI
    HINT -.hints.-> CLI

    style LEX fill:#e1f5ff
    style PAR fill:#e1f5ff
    style SEM fill:#fff3e0
    style EXEC fill:#f3e5f5
    style COACH fill:#e8f5e9
```

---

## Detailed Component Architecture

### 1. Lexer/Tokenizer

```mermaid
flowchart LR
    subgraph "Lexer State Machine"
        INPUT[Input Stream] --> SCAN[Scanner]
        SCAN --> MATCH{Token Match?}
        MATCH -->|Keyword| KW[Keyword Token]
        MATCH -->|Number| NUM[Number Token]
        MATCH -->|String| STR[String Token]
        MATCH -->|Operator| OP[Operator Token]
        MATCH -->|Identifier| ID[Identifier Token]
        MATCH -->|Whitespace| WS[Skip]
        MATCH -->|Comment| COM[Comment Token]
        MATCH -->|Invalid| ERR[Lexer Error]
        
        KW --> STREAM[Token Stream]
        NUM --> STREAM
        STR --> STREAM
        OP --> STREAM
        ID --> STREAM
        COM --> STREAM
        WS --> SCAN
        ERR --> ERROR[Error Handler]
    end
    
    style MATCH fill:#fff9c4
    style ERR fill:#ffcdd2
```

**Token Types:**
```
Keywords: question, driver, evidence, agent, model, simulate, 
          continuous, binary, triangular, normal, if, then

Literals: Number (42, 3.14), Probability (0.5p, 75%), 
          String ("..."), Date (2026-12-31)

Identifiers: variable_name, function_name

Operators: +, -, *, /, =, >, <, >=, <=

Delimiters: { } ( ) [ ] , : ;
```

---

### 2. Parser - AST Construction

```mermaid
flowchart TD
    subgraph "Recursive Descent Parser"
        TOKEN[Token Stream] --> PROG[parse_program]
        
        PROG --> STMT{Statement Type?}
        
        STMT -->|question| QSTMT[parse_question]
        STMT -->|driver| DSTMT[parse_driver]
        STMT -->|evidence| ESTMT[parse_evidence]
        STMT -->|agent| ASTMT[parse_agent]
        STMT -->|model| MSTMT[parse_model]
        STMT -->|simulate| SSTMT[parse_simulate]
        
        QSTMT --> QAST[QuestionNode]
        DSTMT --> DPARSE[parse_driver_body]
        DPARSE --> DTYPE{Driver Type?}
        DTYPE -->|continuous| DCONT[ContinuousDriverNode]
        DTYPE -->|binary| DBIN[BinaryDriverNode]
        
        DCONT --> DIST[parse_distribution]
        DIST --> DISTTYPE{Distribution Type?}
        DISTTYPE -->|triangular| DTRI[TriangularDistNode]
        DISTTYPE -->|normal| DNORM[NormalDistNode]
        DISTTYPE -->|lognormal| DLOG[LognormalDistNode]
        DISTTYPE -->|uniform| DUNI[UniformDistNode]
        DISTTYPE -->|beta| DBETA[BetaDistNode]
        
        ESTMT --> EAST[EvidenceNode]
        ASTMT --> AAST[AgentNode]
        MSTMT --> MAST[ModelNode]
        SSTMT --> SAST[SimulateNode]
        
        QAST --> AST[Abstract Syntax Tree]
        DCONT --> AST
        DBIN --> AST
        EAST --> AST
        AAST --> AST
        MAST --> AST
        SAST --> AST
    end
    
    style STMT fill:#fff9c4
    style DTYPE fill:#fff9c4
    style DISTTYPE fill:#fff9c4
```

**AST Node Types:**
```rust
enum ASTNode {
    Program { statements: Vec<Statement> },
    
    Question {
        text: String,
        target_date: Date,
        resolution_criteria: Option<String>
    },
    
    Driver {
        name: String,
        driver_type: DriverType,
        distribution: Distribution,
        constraints: Vec<Constraint>,
        evidence_refs: Vec<String>
    },
    
    Evidence {
        id: String,
        source: String,
        summary: String,
        relevance: f64,
        date: Date
    },
    
    Agent {
        name: String,
        query: String,
        schedule: Option<Schedule>
    },
    
    Model {
        expression: Expression,
        drivers: Vec<String>
    },
    
    Simulate {
        iterations: u32,
        target: Expression
    }
}
```

---

### 3. Semantic Analyzer

```mermaid
flowchart TD
    subgraph "Semantic Analysis Pipeline"
        AST[AST Input] --> PHASE1[Phase 1: Symbol Resolution]
        
        PHASE1 --> BUILD[Build Symbol Table]
        BUILD --> DECL{Check Declarations}
        DECL -->|Duplicate?| ERRDECL[Error: Redeclaration]
        DECL -->|Undefined Ref?| ERRUNDEF[Error: Undefined Symbol]
        DECL -->|Valid| PHASE2[Phase 2: Type Checking]
        
        PHASE2 --> INFER[Type Inference]
        INFER --> CHECK{Type Compatible?}
        CHECK -->|Mismatch| ERRTYPE[Error: Type Mismatch]
        CHECK -->|Valid| PHASE3[Phase 3: Validation]
        
        PHASE3 --> RULE1[Triangular Ordering]
        PHASE3 --> RULE2[Probability Range]
        PHASE3 --> RULE3[Positive Values]
        PHASE3 --> RULE4[All Drivers Used]
        PHASE3 --> RULE5[Date Ordering]
        PHASE3 --> RULE6[Minimum Drivers]
        PHASE3 --> RULE7[Evidence Presence]
        
        RULE1 --> COLLECT[Collect Issues]
        RULE2 --> COLLECT
        RULE3 --> COLLECT
        RULE4 --> COLLECT
        RULE5 --> COLLECT
        RULE6 --> COLLECT
        RULE7 --> COLLECT
        
        COLLECT --> ANY{Any Errors?}
        ANY -->|Yes| REPORT[Report Errors]
        ANY -->|No| ANNOT[Annotated AST]
        
        REPORT --> COACH[Coaching Intervention]
        COACH --> FIX[Suggest Fixes]
    end
    
    style DECL fill:#fff9c4
    style CHECK fill:#fff9c4
    style ANY fill:#fff9c4
    style ERRDECL fill:#ffcdd2
    style ERRUNDEF fill:#ffcdd2
    style ERRTYPE fill:#ffcdd2
```

**Validation Rules:**
```rust
enum ValidationRule {
    // Distribution constraints
    TriangularOrdering,      // p5 < p50 < p95
    ProbabilityRange,        // 0 <= p <= 1
    PositiveValues,          // values > 0 where required
    
    // Semantic constraints
    AllDriversUsed,          // All declared drivers in model
    NoUndefinedReferences,   // All refs exist
    TypeConsistency,         // Types match across operations
    
    // Domain constraints
    DateOrdering,            // target_date > today
    ScheduleValidity,        // Valid cron expression
    
    // Forecast quality
    MinimumDrivers,          // >= 3 drivers recommended
    EvidencePresence,        // Each driver has evidence
    UncertaintyReasonable,   // Ranges not too narrow
}
```

---

### 4. Execution Engine

```mermaid
flowchart TD
    subgraph "FPL Execution Engine"
        IR[Intermediate Representation] --> INIT[Initialize Environment]
        
        INIT --> EXEC{Execute Statement}
        
        EXEC -->|question| SETQ[Set Question Context]
        EXEC -->|driver| DEFDRV[Define Driver]
        EXEC -->|evidence| ADDEV[Add Evidence]
        EXEC -->|agent| RUNAGENT[Execute Agent]
        EXEC -->|model| BUILDMOD[Build Model]
        EXEC -->|simulate| RUNSIM[Run Simulation]
        
        SETQ --> CTX[Update Context]
        
        DEFDRV --> SAMPLE[Create Sampler]
        SAMPLE --> DIST{Distribution Type}
        DIST -->|triangular| TRITRANS[TriangularTransform]
        DIST -->|normal| NORMTRANS[NormalTransform]
        DIST -->|lognormal| LOGTRANS[LognormalTransform]
        DIST -->|uniform| UNITRANS[UniformTransform]
        DIST -->|beta| BETATRANS[BetaTransform]
        
        TRITRANS --> STORE[Store in Environment]
        NORMTRANS --> STORE
        LOGTRANS --> STORE
        UNITRANS --> STORE
        BETATRANS --> STORE
        
        ADDEV --> ATTACH[Attach to Driver]
        ATTACH --> CTX
        
        RUNAGENT --> LLM[Call LLM API]
        LLM --> PARSE[Parse Response]
        PARSE --> EVIDENCE[Generate Evidence]
        EVIDENCE --> CTX
        
        BUILDMOD --> EXPR[Parse Expression]
        EXPR --> COMPILE[Compile to Function]
        COMPILE --> CTX
        
        RUNSIM --> MONTE[Monte Carlo Loop]
        MONTE --> DRAW[Draw Samples]
        DRAW --> EVAL[Evaluate Model]
        EVAL --> ACCUM[Accumulate Results]
        ACCUM --> DONE{Iterations Done?}
        DONE -->|No| DRAW
        DONE -->|Yes| STATS[Compute Statistics]
        STATS --> RESULT[Simulation Result]
    end
    
    style EXEC fill:#fff9c4
    style DIST fill:#fff9c4
    style DONE fill:#fff9c4
```

---

### 5. Fermi Coach - Intelligence Layer

```mermaid
flowchart TD
    subgraph "Coaching Intelligence System"
        INPUT[User Input] --> ANALYZE[Analyze Context]
        
        ANALYZE --> PROFILE[Load User Profile]
        PROFILE --> LEVEL{User Level?}
        
        LEVEL -->|New User| L1[Beginner Coaching]
        LEVEL -->|Learning| L2[Intermediate Coaching]
        LEVEL -->|Advanced| L3[Minimal Coaching]
        LEVEL -->|Superforecaster| L4[Peer Mode]
        
        L1 --> GUIDE1[High Guidance]
        L2 --> GUIDE2[Moderate Guidance]
        L3 --> GUIDE3[Low Guidance]
        L4 --> GUIDE4[Collaboration]
        
        GUIDE1 --> DETECT[Detect Issues]
        GUIDE2 --> DETECT
        GUIDE3 --> DETECT
        GUIDE4 --> DETECT
        
        DETECT --> PATTERN{Issue Pattern?}
        
        PATTERN -->|Missing Drivers| HINT1[Suggest Drivers]
        PATTERN -->|No Evidence| HINT2[Suggest Research]
        PATTERN -->|Narrow Range| HINT3[Warn Overconfidence]
        PATTERN -->|No Base Rate| HINT4[Suggest Reference Class]
        PATTERN -->|No Premortem| HINT5[Suggest Failure Modes]
        
        HINT1 --> PRIOR[Check Prior Hints]
        HINT2 --> PRIOR
        HINT3 --> PRIOR
        HINT4 --> PRIOR
        HINT5 --> PRIOR
        
        PRIOR --> DISMISS{Recently Dismissed?}
        DISMISS -->|Yes| SKIP[Skip Hint]
        DISMISS -->|No| DELIVER[Deliver Intervention]
        
        DELIVER --> LOG[Log Interaction]
        LOG --> UPDATE[Update User Profile]
        
        SKIP --> NEXT[Continue]
        UPDATE --> NEXT
    end
    
    style LEVEL fill:#fff9c4
    style PATTERN fill:#fff9c4
    style DISMISS fill:#fff9c4
```

**Coaching Strategies:**
```rust
struct CoachingEngine {
    user_profile: UserProfile,
    hint_history: Vec<HintInteraction>,
    mistake_patterns: HashMap<String, u32>,
    
    // Adaptive coaching
    fn suggest_next_action(&self, context: &ForecastContext) -> Vec<Suggestion>,
    fn detect_mistakes(&self, forecast: &Forecast) -> Vec<Issue>,
    fn explain_error(&self, error: &FPLError) -> String,
    fn provide_example(&self, concept: &str) -> String,
    
    // Learning
    fn track_improvement(&mut self, outcome: &ForecastOutcome),
    fn adjust_guidance_level(&mut self),
}

struct Suggestion {
    priority: Priority,        // High, Medium, Low
    category: Category,        // Command, Concept, Warning
    message: String,
    action: Option<String>,    // Auto-executable command
    explanation: String,
    example: Option<String>,
}
```

---

### 6. Distribution Sampling Engine

```mermaid
flowchart LR
    subgraph "Distribution Sampling System"
        subgraph "Input Distributions"
            TRI[Triangular p5, p50, p95]
            NORM[Normal mean, stddev]
            LOG[Lognormal median, sigma]
            UNI[Uniform low, high]
            BETA[Beta alpha, beta]
        end
        
        subgraph "Transform to Standard"
            TRI --> TRANS1[Triangular Transform]
            NORM --> TRANS2[Normal Transform]
            LOG --> TRANS3[Lognormal Transform]
            UNI --> TRANS4[Uniform Transform]
            BETA --> TRANS5[Beta Transform]
        end
        
        subgraph "Sampling"
            TRANS1 --> RNG[RNG - PCG]
            TRANS2 --> RNG
            TRANS3 --> RNG
            TRANS4 --> RNG
            TRANS5 --> RNG
            
            RNG --> SAMPLE[Generate Sample]
        end
        
        subgraph "Monte Carlo"
            SAMPLE --> COMBINE[Combine Drivers]
            COMBINE --> EVAL[Evaluate Model]
            EVAL --> ITER{N iterations?}
            ITER -->|No| SAMPLE
            ITER -->|Yes| STATS[Compute Stats]
        end
        
        subgraph "Output"
            STATS --> P10[P10]
            STATS --> P50[P50 Median]
            STATS --> P90[P90]
            STATS --> MEAN[Mean]
            STATS --> SD[Std Dev]
            STATS --> HIST[Histogram]
        end
    end
    
    style ITER fill:#fff9c4
```

---

### 7. Agent Orchestration System

```mermaid
flowchart TD
    subgraph "Agent Research System"
        TRIGGER[Agent Trigger] --> SELECT[Select Agent Type]
        
        SELECT --> TYPE{Agent Type?}
        
        TYPE -->|Research| RA[Research Analyst]
        TYPE -->|Sentiment| SA[Sentiment Monitor]
        TYPE -->|Competitive| CI[Competitive Intel]
        TYPE -->|Financial| FA[Financial Analyst]
        TYPE -->|Market| MR[Market Researcher]
        TYPE -->|Expert| ES[Expert Synthesizer]
        
        RA --> PROMPT1[Build Research Prompt]
        SA --> PROMPT2[Build Sentiment Prompt]
        CI --> PROMPT3[Build Intel Prompt]
        FA --> PROMPT4[Build Financial Prompt]
        MR --> PROMPT5[Build Market Prompt]
        ES --> PROMPT6[Build Synthesis Prompt]
        
        PROMPT1 --> LLM[LLM API Call]
        PROMPT2 --> LLM
        PROMPT3 --> LLM
        PROMPT4 --> LLM
        PROMPT5 --> LLM
        PROMPT6 --> LLM
        
        LLM --> PARSE[Parse Response]
        PARSE --> STRUCT[Structure Evidence]
        
        STRUCT --> EXTRACT[Extract Elements]
        EXTRACT --> SUMMARY[Summary]
        EXTRACT --> FINDINGS[Key Findings]
        EXTRACT --> SOURCES[Sources]
        EXTRACT --> CONF[Confidence]
        
        SUMMARY --> EVID[Evidence Object]
        FINDINGS --> EVID
        SOURCES --> EVID
        CONF --> EVID
        
        EVID --> ATTACH[Attach to Driver]
        ATTACH --> NOTIFY[Notify User]
    end
    
    style TYPE fill:#fff9c4
```

---

## Complete Data Flow Example

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Lexer
    participant Parser
    participant Semantic
    participant Coach
    participant Executor
    participant Sampler
    participant Result

    User->>CLI: Input FPL code
    CLI->>Lexer: Tokenize
    Lexer->>Parser: Token stream
    Parser->>Semantic: AST
    
    alt Semantic Error
        Semantic->>Coach: Request explanation
        Coach->>User: Helpful error + suggestion
    else Valid
        Semantic->>Executor: Validated AST
        Executor->>Sampler: Initialize distributions
        
        loop Monte Carlo
            Sampler->>Executor: Sample values
            Executor->>Executor: Evaluate model
        end
        
        Executor->>Result: Statistics
        Result->>Coach: Quality check
        
        alt Quality Issues
            Coach->>User: Warnings + suggestions
        else Good Quality
            Result->>User: Simulation results
        end
    end
```

---

## FPL Language Processing Pipeline

```mermaid
stateDiagram-v2
    [*] --> Lexing: Input text
    
    Lexing --> Parsing: Token stream
    Parsing --> SemanticAnalysis: AST
    
    SemanticAnalysis --> TypeError: Type error
    SemanticAnalysis --> ValidationError: Validation error
    SemanticAnalysis --> Compilation: Valid AST
    
    TypeError --> Coach: Request help
    ValidationError --> Coach: Request help
    Coach --> [*]: Error + suggestion
    
    Compilation --> Execution: IR code
    
    Execution --> AgentCall: agent statement
    Execution --> Simulation: simulate statement
    Execution --> Update: Other statements
    
    AgentCall --> LLM: Research request
    LLM --> EvidenceGen: LLM response
    EvidenceGen --> Update: Evidence object
    
    Simulation --> MonteCarlo: Run iterations
    MonteCarlo --> Statistics: Raw samples
    Statistics --> QualityCheck: Results
    
    QualityCheck --> CoachingCheck: Check quality
    CoachingCheck --> [*]: Final results
    
    Update --> Execution: Continue
```

---

## Symbol Table Structure

```mermaid
graph TD
    subgraph "Symbol Table"
        GLOBAL[Global Scope]
        
        GLOBAL --> QUESTION[Question Context]
        QUESTION --> QTEXT[text: String]
        QUESTION --> QTARG[target_date: Date]
        QUESTION --> QCRIT[criteria: String]
        
        GLOBAL --> DRIVERS[Drivers Map]
        DRIVERS --> D1[Driver: market_size]
        DRIVERS --> D2[Driver: growth_rate]
        DRIVERS --> D3[Driver: churn]
        
        D1 --> D1TYPE[type: Continuous]
        D1 --> D1DIST[distribution: Triangular]
        D1 --> D1EV[evidence: Array]
        
        D1DIST --> D1P5[p5: 500M]
        D1DIST --> D1P50[p50: 1.2B]
        D1DIST --> D1P95[p95: 2.5B]
        
        GLOBAL --> EVIDENCE[Evidence Map]
        EVIDENCE --> E1[Evidence: market_report]
        EVIDENCE --> E2[Evidence: analyst_note]
        
        E1 --> E1SRC[source: Gartner]
        E1 --> E1DATE[date: 2025-09-15]
        E1 --> E1REL[relevance: 0.9]
        
        GLOBAL --> AGENTS[Agents Map]
        AGENTS --> A1[Agent: research]
        AGENTS --> A2[Agent: sentiment]
        
        A1 --> A1Q[query: String]
        A1 --> A1SCH[schedule: weekly]
        
        GLOBAL --> MODEL[Model Expression]
        MODEL --> EXPR[Expression Tree]
    end
    
    style GLOBAL fill:#e3f2fd
    style QUESTION fill:#fff3e0
    style DRIVERS fill:#f3e5f5
    style EVIDENCE fill:#e8f5e9
```

---

## Error Handling & Recovery

```mermaid
flowchart TD
    subgraph "Error Handling System"
        ERR[Error Detected] --> CLASSIFY{Error Type?}
        
        CLASSIFY -->|Syntax| SYNERR[Syntax Error]
        CLASSIFY -->|Type| TYPEERR[Type Error]
        CLASSIFY -->|Validation| VALERR[Validation Error]
        CLASSIFY -->|Runtime| RUNERR[Runtime Error]
        
        SYNERR --> SYNREC[Syntax Recovery]
        TYPEERR --> TYPEREC[Type Recovery]
        VALERR --> VALREC[Validation Recovery]
        RUNERR --> RUNREC[Runtime Recovery]
        
        SYNREC --> RECOVER{Can Recover?}
        TYPEREC --> RECOVER
        VALREC --> RECOVER
        RUNREC --> RECOVER
        
        RECOVER -->|Yes| SUGGEST[Generate Suggestions]
        RECOVER -->|No| ABORT[Abort Execution]
        
        SUGGEST --> EXPLAIN[Explain Error]
        EXPLAIN --> EXAMPLE[Provide Example]
        EXAMPLE --> FIX[Suggest Fix]
        
        FIX --> AUTO{Auto-fixable?}
        AUTO -->|Yes| APPLY[Apply Fix]
        AUTO -->|No| USER[Request User Input]
        
        APPLY --> RETRY[Retry]
        USER --> WAIT[Wait for Input]
        
        ABORT --> REPORT[Error Report]
        REPORT --> LOG[Log Error]
    end
    
    style CLASSIFY fill:#fff9c4
    style RECOVER fill:#fff9c4
    style AUTO fill:#fff9c4
```

---

## Type System Lattice

```mermaid
graph TD
    ANY[Any]
    
    ANY --> NUM[Number]
    ANY --> PROB[Probability]
    ANY --> STR[String]
    ANY --> BOOL[Boolean]
    ANY --> DATE[Date]
    ANY --> DIST[Distribution]
    ANY --> DRV[Driver]
    ANY --> EV[Evidence]
    ANY --> AG[Agent]
    ANY --> ARR[Array]
    
    DIST --> TRI[Triangular]
    DIST --> NORM[Normal]
    DIST --> LOG[Lognormal]
    DIST --> UNI[Uniform]
    DIST --> BETA[Beta]
    
    DRV --> CONT[Continuous]
    DRV --> BIN[Binary]
    
    NUM -.coerce.-> PROB
    PROB -.coerce.-> NUM
    
    style ANY fill:#e3f2fd
    style NUM fill:#fff3e0
    style DIST fill:#f3e5f5
    style DRV fill:#e8f5e9
```

---

## Language Server Protocol Integration

```mermaid
flowchart LR
    subgraph "LSP Server"
        LISTEN[Listen for Events]
        
        LISTEN --> EVENT{Event Type?}
        
        EVENT -->|textDocument/didChange| CHANGE[Text Changed]
        EVENT -->|textDocument/completion| COMPLETE[Completion Request]
        EVENT -->|textDocument/hover| HOVER[Hover Request]
        EVENT -->|textDocument/definition| DEF[Definition Request]
        
        CHANGE --> PARSE[Parse Document]
        PARSE --> DIAG[Generate Diagnostics]
        DIAG --> SEND1[Send Diagnostics]
        
        COMPLETE --> CONTEXT[Analyze Context]
        CONTEXT --> GEN[Generate Completions]
        GEN --> COACH1[Coach Suggestions]
        COACH1 --> SEND2[Send Completions]
        
        HOVER --> LOOKUP[Symbol Lookup]
        LOOKUP --> INFO[Generate Hover Info]
        INFO --> COACH2[Add Coaching Tips]
        COACH2 --> SEND3[Send Hover Info]
        
        DEF --> FIND[Find Definition]
        FIND --> LOC[Get Location]
        LOC --> SEND4[Send Location]
    end
    
    style EVENT fill:#fff9c4
```

---

## Memory and State Management

```mermaid
graph TD
    subgraph "Runtime State"
        HEAP[Heap]
        STACK[Call Stack]
        SYMTAB[Symbol Table]
        
        HEAP --> DIST[Distribution Objects]
        HEAP --> EV[Evidence Objects]
        HEAP --> RES[Results Cache]
        
        STACK --> FRAME1[Frame: main]
        STACK --> FRAME2[Frame: simulate]
        
        FRAME2 --> LOCAL[Local Vars]
        FRAME2 --> TEMP[Temp Values]
        
        SYMTAB --> GLOBAL[Global Symbols]
        SYMTAB --> FORECAST[Forecast Context]
        
        FORECAST --> DRIVERS[Active Drivers]
        FORECAST --> MODEL[Model Expression]
        FORECAST --> EVIDENCE[Evidence Store]
    end
    
    subgraph "Persistent State"
        DB[(Database)]
        CACHE[(Cache)]
        
        DB --> FORECASTS[Saved Forecasts]
        DB --> HISTORY[User History]
        DB --> PROFILE[User Profile]
        
        CACHE --> AGENT[Agent Results]
        CACHE --> COMPUTE[Computed Values]
    end
    
    RES -.sync.-> CACHE
    FORECAST -.persist.-> DB
```

---

## Key Algorithms

### Triangular Distribution Sampling
```mermaid
flowchart TD
    START[Input: p5, p50, p95] --> U[Generate U ~ Uniform 0,1]
    U --> CMP{U < F_c?}
    CMP -->|Yes| LEFT[X = p5 + sqrt U * F_c * p5-p50]
    CMP -->|No| RIGHT[X = p95 - sqrt 1-U * 1-F_c * p95-p50]
    LEFT --> RET[Return X]
    RIGHT --> RET
    
    style CMP fill:#fff9c4
```

### Monte Carlo Loop
```mermaid
flowchart TD
    START[Initialize] --> LOOP{i < N?}
    LOOP -->|Yes| SAMPLE[Sample all drivers]
    SAMPLE --> EVAL[Evaluate model]
    EVAL --> STORE[Store result]
    STORE --> INC[i++]
    INC --> LOOP
    LOOP -->|No| STATS[Compute statistics]
    STATS --> HIST[Build histogram]
    HIST --> RETURN[Return results]
    
    style LOOP fill:#fff9c4
```

---

## Summary

This architecture represents Fermi's "Broca brain" - the language processing and reasoning center for FPL. The key components are:

1. **Lexer/Parser**: Transforms text → tokens → AST
2. **Semantic Analyzer**: Validates and type-checks AST
3. **Executor**: Runs the forecast model
4. **Distribution Engine**: Samples from probability distributions
5. **Agent System**: Orchestrates research through LLMs
6. **Coaching Engine**: Provides intelligent guidance
7. **LSP Server**: IDE integration for real-time feedback

The system is designed to be:
- **Declarative**: Users describe what they want, not how to compute it
- **Type-safe**: Strong type system catches errors early
- **Coached**: Intelligent guidance throughout
- **Extensible**: Easy to add new distributions, agents, validation rules

Next steps:
1. Implement lexer and parser
2. Build semantic analyzer with validation rules
3. Create execution engine with distribution sampling
4. Integrate coaching system
5. Build LSP server for IDE support
