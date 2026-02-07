# Session Notes: Base Rate & External View Feature Design

**Date:** 2026-02-05  
**Status:** 🎯 Design Complete - Ready for Implementation  
**Git Baseline:** commit `ef2290c` (sensitivity analysis complete)

---

## Context

During review of the system, we discovered that **base rate and external view** (core Tetlock methodology concepts) were documented in DOMAIN_MODEL.md but **not implemented** in the actual FPL language or execution system.

**Found in design docs:**
- ✅ DOMAIN_MODEL.md has `BaseRate` struct definition
- ✅ Validation logic mentions base rate warnings
- ✅ Tetlock methodology references

**Missing from implementation:**
- ❌ No base_rate in AST (`src/ast.rs`)
- ❌ No base_rate parsing in lexer/parser
- ❌ No base_rate in any example FPL files
- ❌ No divergence calculation or display

**Decision:** Git commit baseline (`ef2290c`) before touching core language structures (lexer, parser, AST, semantic analyzer).

---

## Tetlock Methodology: Outside View First

### The Principle
**"Start with the outside view (base rate), then adjust with inside view"**

1. **Outside View (Base Rate):** Historical frequency for reference class
   - Example: "Tech stocks reaching new highs in bull markets: 35%"
   - Represents conventional wisdom
   - Anchoring point to prevent overconfidence

2. **Inside View (Fermi Decomposition):** Specific analysis with drivers
   - Example: Market size × growth rate × market share × binary events
   - Your unique knowledge and analysis
   - May diverge significantly from base rate

3. **Bayesian Synthesis:** Combine both views
   - Start anchored at base rate
   - Update based on inside analysis
   - Question large divergences (do you have special knowledge?)

---

## Design Decisions

### 1. Syntax: Nested in Question Block

**Requirement:** Base rate is ALWAYS required (no backwards compatibility)

```fpl
// NEW SYNTAX (required)
question "Will AMD reach $200 by 2026-12-31?" {
    base_rate {
        reference_class: "Tech stocks reaching new highs in bull markets"
        historical_frequency: 0.35p
        sample_size: 127
        source: "Analysis of NASDAQ 100 (2010-2025)"
        reasoning: "AMD is a large-cap tech stock in growth phase with strong fundamentals"
        generated_by: agent  // or: human
    }
}

driver market_size continuous { ... }
```

**Rationale:**
- Base rate is foundational to forecasting process
- Missing base rate = missing anchoring = overconfidence risk
- Question block naturally contains forecast metadata
- Semantic analyzer will warn if base_rate missing

**No longer supported:**
```fpl
// OLD SYNTAX (breaks)
question "Will AMD reach $200?"  // Simple string - NO LONGER VALID
```

---

### 2. Agent vs Human Generation

**Track provenance:**

```rust
pub enum GeneratedBy {
    Agent(String),  // Agent name that generated it
    Human,          // Manually entered by user
}
```

**Future workflow (agent-driven):**
1. User writes question text
2. Agent parses question → identifies reference class
3. Agent researches → finds historical base rates
4. Agent sets base_rate with evidence/reasoning
5. Agent suggests initial drivers
6. User reviews/refines
7. System runs simulation

**Current workflow (manual, with hooks):**
- User manually enters base_rate block
- System parses and validates
- Future: Replace with agent generation

---

### 3. Base Rate as Reference (Not Constraint)

**What base rate IS:**
- ✅ Reference point showing conventional wisdom
- ✅ Displayed in reports (outside view vs inside view)
- ✅ Used to calculate divergence
- ✅ Can be updated as conventional wisdom evolves
- ✅ Used at resolution to measure skill vs base rate

**What base rate is NOT:**
- ❌ Hard constraint on forecast
- ❌ Prior in Bayesian calculation (not yet, could be future)
- ❌ Accuracy indicator (divergence ≠ error)

---

### 4. Divergence: Understanding Your Thesis

**Key Insight:** Divergence is NOT about accuracy - it's about thesis strength

**Divergence = How much your view differs from conventional wisdom**

```rust
// In ExecutionResults
pub divergence_relative: Option<f64>,   // (forecast - base_rate) / base_rate
pub divergence_absolute: Option<f64>,   // forecast - base_rate
```

**Example Interpretations:**

| Base Rate | Forecast | Divergence | Meaning |
|-----------|----------|------------|---------|
| 35% | 37% | +6% | Confirms conventional wisdom |
| 35% | 68% | +94% | Moderately contrarian thesis |
| 35% | 5% | -86% | Strongly bearish vs base rate |
| 35% | 95% | +171% | Strongly bullish - special knowledge? |

**NOT a warning system:**
- Don't warn on large divergence
- Just show the data
- Let user reflect on why they diverge
- Track over time for meta-learning

**Report Display:**
```
═══════════════════════════════════════
OUTSIDE VIEW vs INSIDE VIEW
═══════════════════════════════════════

Outside View (Base Rate):
  Reference Class: "Tech stocks reaching highs in bull markets"
  Historical Frequency: 35%
  Sample Size: 127 cases
  Source: NASDAQ 100 analysis (2010-2025)

Inside View (Fermi Model):
  Mean: 68%
  Median: 72%
  90% CI: [45%, 89%]

Divergence: +94% (relative)
  Your forecast is nearly 2x higher than the base rate.
  This represents a moderately contrarian thesis.

Reasoning: "AMD has specific advantages in AI/datacenter 
markets not captured by broad tech stock reference class"
```

---

### 5. Base Rate Updates (Feature, Not Bug)

**Tetlock's View:** Update base rates as you learn!

**Why update base rates?**

1. **Better Reference Class:**
   ```
   v1.0: "Tech stocks in bull markets" → 35%
   v1.1: "Semiconductor stocks in AI booms" → 48%
   v1.2: "AMD-like stocks (gaming+datacenter) in AI cycles" → 52%
   ```

2. **New Historical Data:**
   ```
   v1.0: NASDAQ 100 (2010-2025) → 35%
   v1.1: NASDAQ 100 (2015-2025) → 42% (more recent)
   v1.2: NASDAQ 100 (2020-2025) → 48% (AI era only)
   ```

3. **Agent Research:**
   - Agent continuously monitors for better analogies
   - Agent updates base rate when finds more relevant data
   - Each update is versioned in git with reasoning

**Time Travel Shows Evolution:**
```
Forecast Version History:
  v1.0 (2026-02-05): base_rate 35%, forecast 68%, divergence +94%
  v1.1 (2026-02-15): base_rate 42%, forecast 71%, divergence +69%
  v1.2 (2026-03-01): base_rate 48%, forecast 73%, divergence +52%

Analysis: Both conventional wisdom (base rate) and your view 
(forecast) moved higher. Divergence narrowed as better reference 
class reduced gap.
```

---

### 6. Resolution: Measuring Skill

**At resolution, calculate TWO Brier scores:**

```rust
pub struct Resolution {
    pub outcome: bool,
    pub resolved_at: DateTime<Utc>,
    
    // Your performance
    pub forecast_probability: f64,      // Your final forecast
    pub your_brier_score: f64,          // (forecast - outcome)²
    
    // Base rate performance  
    pub base_rate_at_resolution: f64,   // What base rate was at resolution
    pub base_rate_brier_score: f64,     // (base_rate - outcome)²
    
    // Your edge
    pub brier_improvement: f64,         // base_rate_brier - your_brier
}
```

**Example:**

Forecast: "Will AMD reach $200?"
- Base rate: 35%
- Your forecast: 68%
- Outcome: TRUE (AMD reached $215)

**Brier Scores:**
- Base rate Brier: (0.35 - 1.0)² = 0.4225
- Your Brier: (0.68 - 1.0)² = 0.1024
- **Your improvement: 0.32** (76% better than base rate!) 🎯

**Meta-Analysis (Future):**
```
Your Forecasting Performance (30 resolved forecasts):
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Average Brier Score:
  Yours:     0.18
  Base Rate: 0.31
  Improvement: 0.13 (42% better)

Beat Base Rate: 23/30 forecasts (77%)

When You Diverge Significantly (+50% or more):
  Beat base rate: 8/12 times (67%)
  Average improvement: 0.21
  
Interpretation: Your contrarian views often correct!
Strong evidence of genuine forecasting skill.
```

**The Goal:** Consistently outperform base rates = Real skill/edge

---

## Data Structures

### AST Changes (`src/ast.rs`)

```rust
// BEFORE (current)
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionStmt {
    pub text: String,
}

// AFTER (new)
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionStmt {
    pub text: String,
    pub base_rate: Option<BaseRate>,    // Will warn if None
    pub target_date: Option<NaiveDate>,
    pub resolution_criteria: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaseRate {
    pub reference_class: String,
    pub historical_frequency: f64,      // 0.0 to 1.0
    pub sample_size: Option<usize>,
    pub source: String,
    pub reasoning: Option<String>,
    pub generated_by: GeneratedBy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeneratedBy {
    Agent(String),  // Agent name
    Human,
}
```

### Execution Results (`src/executor.rs`)

```rust
// Add to ExecutionResults
pub struct ExecutionResults {
    // ... existing fields ...
    
    // NEW: Divergence tracking
    pub base_rate: Option<f64>,           // Store base rate if available
    pub divergence_relative: Option<f64>, // (mean - base_rate) / base_rate
    pub divergence_absolute: Option<f64>, // mean - base_rate
}
```

### Resolution (Future - not yet implemented)

```rust
pub struct Resolution {
    pub forecast_id: String,
    pub resolved_at: DateTime<Utc>,
    pub resolved_by: String,
    
    // Outcome
    pub outcome: bool,  // For binary forecasts
    pub outcome_value: Option<f64>,  // For continuous
    
    // Your performance
    pub forecast_probability: f64,
    pub your_brier_score: f64,
    
    // Base rate performance
    pub base_rate_at_resolution: f64,
    pub base_rate_brier_score: f64,
    
    // Skill measurement
    pub brier_improvement: f64,  // How much you beat base rate
}
```

---

## Implementation Tasks

### Phase 1: Core Language Support

1. **Lexer (`src/lexer.rs`)**
   - Add tokens: `base_rate`, `reference_class`, `historical_frequency`, `sample_size`, `generated_by`
   - Handle `agent` and `human` keywords

2. **Parser (`src/parser.rs`)**
   - Parse question block with optional base_rate
   - Parse base_rate fields
   - Return updated QuestionStmt with BaseRate

3. **AST (`src/ast.rs`)**
   - Add BaseRate struct
   - Add GeneratedBy enum
   - Update QuestionStmt
   - Implement Display traits

4. **Semantic Analyzer (`src/semantic.rs`)**
   - Validate base_rate fields
   - Check historical_frequency is 0.0 to 1.0
   - **Warn if base_rate is None** (critical!)
   - Validate reference_class is meaningful

### Phase 2: Execution & Divergence

5. **Executor (`src/executor.rs`)**
   - Extract base_rate from QuestionStmt
   - Calculate divergence_relative and divergence_absolute
   - Store in ExecutionResults

6. **Report Generation (`src/report/`)**
   - Display Outside View section with base rate details
   - Display Inside View section with forecast results
   - Display divergence prominently
   - Add interpretation text

### Phase 3: Examples & Documentation

7. **Update Examples**
   - Convert all example FPL files to new syntax
   - Add base_rate blocks with realistic data

8. **Update Tests**
   - Add parser tests for base_rate
   - Add semantic tests for validation
   - Update executor tests

9. **Documentation**
   - Update README with base_rate example
   - Update language guide
   - Session notes (this document)

---

## Example FPL (Before & After)

### BEFORE (Current - Invalid after implementation)
```fpl
question "Will AMD reach $200 by 2026-12-31?"

driver market_size continuous {
    distribution: triangular(100, 200, 500)
}

model: market_size * growth_rate
simulate 10000 iterations
```

### AFTER (New Required Syntax)
```fpl
question "Will AMD reach $200 by 2026-12-31?" {
    base_rate {
        reference_class: "Tech stocks reaching new highs in bull markets"
        historical_frequency: 0.35p
        sample_size: 127
        source: "Analysis of NASDAQ 100 stocks (2010-2025)"
        reasoning: "AMD is a large-cap tech stock in a growth phase. Historical data shows 35% of similar stocks in bull markets reach new highs within 12 months."
        generated_by: agent  // or: human
    }
}

driver market_size continuous {
    distribution: triangular(100, 200, 500)
    unit: "billions"
}

driver growth_rate continuous {
    distribution: normal(0.25, 0.10)
}

model: market_size * (1 + growth_rate)
simulate 10000 iterations
```

---

## Testing Strategy

### Parser Tests
```rust
#[test]
fn test_parse_question_with_base_rate() {
    let input = r#"
        question "Will X happen?" {
            base_rate {
                reference_class: "Similar events"
                historical_frequency: 0.35p
                sample_size: 100
                source: "Historical data"
                generated_by: agent
            }
        }
    "#;
    
    let result = parse(input);
    assert!(result.is_ok());
    
    let question = get_question(&result.unwrap());
    assert!(question.base_rate.is_some());
    
    let base_rate = question.base_rate.unwrap();
    assert_eq!(base_rate.reference_class, "Similar events");
    assert_eq!(base_rate.historical_frequency, 0.35);
}

#[test]
fn test_parse_question_without_base_rate() {
    let input = r#"question "Will X happen?" {}"#;
    let result = parse(input);
    assert!(result.is_ok());
    
    let question = get_question(&result.unwrap());
    assert!(question.base_rate.is_none());  // Allowed but will warn
}
```

### Semantic Tests
```rust
#[test]
fn test_semantic_warns_missing_base_rate() {
    let program = create_program_without_base_rate();
    let analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze(&program);
    
    assert!(result.warnings.iter().any(|w| 
        w.message.contains("base rate")
    ));
}

#[test]
fn test_semantic_validates_historical_frequency() {
    let program = create_program_with_invalid_frequency(1.5);  // > 1.0
    let analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze(&program);
    
    assert!(result.errors.iter().any(|e| 
        e.message.contains("historical_frequency must be between 0.0 and 1.0")
    ));
}
```

### Executor Tests
```rust
#[test]
fn test_divergence_calculation() {
    let program = create_program_with_base_rate(0.35);
    let mut executor = Executor::new(1000);
    let results = executor.execute(&program).unwrap();
    
    assert!(results.base_rate.is_some());
    assert_eq!(results.base_rate.unwrap(), 0.35);
    
    // Assuming mean is ~0.68
    assert!(results.divergence_relative.is_some());
    let div = results.divergence_relative.unwrap();
    assert!(div > 0.8 && div < 1.0);  // ~94% divergence
}
```

---

## Migration Strategy

### Breaking Change
This is a **breaking change** to FPL syntax. All existing FPL files will need updating.

### Migration Steps

1. **Identify all FPL files:**
   ```bash
   find . -name "*.fpl"
   ```

2. **Update each file:**
   - Wrap question text in block: `question "text" { }`
   - Add base_rate block (manually for now)
   - Test parsing

3. **Example migration script concept:**
   ```python
   # Pseudo-code for future automation
   for file in fpl_files:
       if not has_base_rate_block(file):
           suggest_base_rate = agent_generate_base_rate(file.question)
           insert_base_rate(file, suggest_base_rate)
           review_required = True
   ```

### Deprecation Warning Period
- Option: Support both syntaxes for 1-2 versions with deprecation warning
- Or: Clean break (preferred for cleaner codebase)
- **Decision:** Clean break - base rate is foundational

---

## Future Enhancements

### Agent Integration (Phase 4)
1. **Question Parsing Agent**
   - Parse user question
   - Identify domain and key entities
   - Determine reference class

2. **Base Rate Research Agent**
   - Search historical databases
   - Find similar events/analogies
   - Calculate historical frequency
   - Cite sources

3. **Driver Suggestion Agent**
   - Based on question and base rate
   - Suggest initial Fermi decomposition
   - Propose distributions based on research

### Multi-Model Comparison (Phase 5)
```fpl
question "Will AMD reach $200?" {
    base_rate primary {
        reference_class: "Tech stocks in bull markets"
        historical_frequency: 0.35p
    }
    
    base_rate alternative {
        reference_class: "Semiconductor stocks in AI booms"
        historical_frequency: 0.48p
    }
}
```

### Bayesian Integration (Phase 6)
- Use base rate as Bayesian prior
- Fermi model provides likelihood
- Calculate posterior
- Compare naive forecast vs Bayesian synthesis

---

## Success Criteria

✅ **Parser**: Can parse question blocks with base_rate  
✅ **Semantic**: Warns when base_rate missing  
✅ **Executor**: Calculates and stores divergence  
✅ **Reports**: Displays outside view vs inside view  
✅ **Examples**: All FPL files updated to new syntax  
✅ **Tests**: Full coverage of new functionality  
✅ **Documentation**: Clear explanation of Tetlock methodology  

---

## Open Questions (Resolved)

**Q: Should base rate be required or optional?**  
✅ Required - missing base rate = missing critical anchoring

**Q: Can base rates be updated?**  
✅ Yes - feature, not bug. Reflects learning and better reference classes

**Q: How to handle divergence?**  
✅ Display as data, not warning. Track for meta-learning

**Q: Single or multiple base rates?**  
✅ Single for now, can add multiple later if needed

**Q: Where to store divergence?**  
✅ In ExecutionResults (part of simulation output)

---

## Next Steps

1. ✅ Capture context in session notes (this document)
2. 🎯 Implement Phase 1: Core Language Support
   - Lexer tokens
   - Parser logic
   - AST structures
3. 🎯 Implement Phase 2: Execution & Divergence
4. 🎯 Implement Phase 3: Examples & Documentation
5. 🎯 Test thoroughly
6. 🎯 Commit with detailed message
7. 🎯 Push to git

---

**Context Captured:** 2026-02-05  
**Ready for Implementation:** YES  
**Git Baseline:** `ef2290c` (restore point if needed)  
**Estimated Scope:** 4-6 hours of implementation
