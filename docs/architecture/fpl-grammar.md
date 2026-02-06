# FPL: Forecasting Programming Language Grammar
## Complete Language Specification

**Version**: 0.1.0  
**Date**: 2026-02-04

---

## Overview

FPL (Forecasting Programming Language) is a domain-specific language for programming probabilistic forecasts. It combines declarative forecast definition with imperative driver configuration and agent coordination.

---

## EBNF Grammar

```ebnf
(* ============================================ *)
(* TOP-LEVEL STRUCTURE *)
(* ============================================ *)

program = question_block , { driver_block | evidence_block | agent_block } ,
          [ model_block ] , [ simulate_block ] , [ external_block ] ,
          [ arbitrage_block ] , [ on_resolve_block ] ;

(* ============================================ *)
(* QUESTION DEFINITION *)
(* ============================================ *)

question_block = "question" , string_literal , "{" ,
                 [ "domain:" , identifier ] ,
                 [ "resolution_criteria:" , string_literal ] ,
                 [ "target_date:" , date_literal ] ,
                 [ base_rate_block ] ,
                 [ model_ref ] ,
                 "}" ;

base_rate_block = "base_rate:" , "reference_class(" ,
                  string_literal ,
                  [ "," , "confidence:" , confidence_level ] ,
                  ")" ;

confidence_level = "low" | "medium" | "high" ;

model_ref = "model:" , identifier ;

(* ============================================ *)
(* DRIVER DEFINITION *)
(* ============================================ *)

driver_block = "driver" , identifier , ":" , driver_type , "{" ,
               driver_properties ,
               "}" ;

driver_type = "continuous" | "binary" ;

driver_properties = { driver_property } ;

driver_property = name_property
                | unit_property
                | dist_property
                | prob_property
                | rationale_property
                | agents_property
                | evidence_property
                | constraints_property
                | if_true_property ;

name_property = "name:" , string_literal ;

unit_property = "unit:" , identifier ;

dist_property = "dist:" , distribution_spec ;

distribution_spec = triangular_dist
                  | normal_dist
                  | lognormal_dist
                  | uniform_dist
                  | beta_dist ;

triangular_dist = "triangular" , "{" ,
                  "p5:" , number_literal , "," ,
                  "p50:" , number_literal , "," ,
                  "p95:" , number_literal ,
                  "}" ;

normal_dist = "normal" , "{" ,
              "mean:" , number_literal , "," ,
              "stddev:" , number_literal ,
              "}" ;

lognormal_dist = "lognormal" , "{" ,
                 "median:" , number_literal , "," ,
                 "sigma:" , number_literal ,
                 "}" ;

uniform_dist = "uniform" , "{" ,
               "low:" , number_literal , "," ,
               "high:" , number_literal ,
               "}" ;

beta_dist = "beta" , "{" ,
            "alpha:" , number_literal , "," ,
            "beta:" , number_literal ,
            "}" ;

prob_property = "prob:" , probability_literal ;

rationale_property = "rationale:" , multiline_string ;

agents_property = "agents:" , "[" , [ agent_config_list ] , "]" ;

agent_config_list = agent_config , { "," , agent_config } ;

agent_config = identifier , "{" ,
               [ "query:" , string_literal ] ,
               [ "schedule:" , schedule_spec ] ,
               [ "triggers:" , trigger_list ] ,
               [ agent_config_properties ] ,
               "}" ;

schedule_spec = "on-demand"
              | "daily"
              | "weekly" , [ "(" , weekday , "," , time_literal , ")" ]
              | "monthly"
              | schedule_cron ;

weekday = "monday" | "tuesday" | "wednesday" | "thursday" | 
          "friday" | "saturday" | "sunday" ;

schedule_cron = "cron(" , string_literal , ")" ;

trigger_list = "[" , trigger , { "," , trigger } , "]" ;

trigger = string_literal
        | "sentiment_change(" , "threshold:" , number_literal , ")" ;

evidence_property = "evidence:" , "[" , [ evidence_list ] , "]" ;

evidence_list = evidence_item , { "," , evidence_item } ;

evidence_item = "manual" , "{" ,
                "source:" , string_literal , "," ,
                "date:" , date_literal , "," ,
                "content:" , string_literal , "," ,
                "impact:" , impact_spec ,
                "}" ;

impact_spec = "increase(" , magnitude , ")"
            | "decrease(" , magnitude , ")"
            | "neutral" ;

magnitude = "weak" | "moderate" | "strong" ;

constraints_property = "constraints:" , "[" , [ constraint_list ] , "]" ;

constraint_list = constraint , { "," , constraint } ;

constraint = "if" , condition , "then" , action ;

condition = comparison_expr
          | boolean_expr
          | identifier , "in" , range_expr ;

comparison_expr = identifier , comparison_op , expression ;

comparison_op = ">" | "<" | ">=" | "<=" | "==" | "!=" ;

action = "clamp(" , identifier , "," , clamp_spec , ")"
       | "shift(" , identifier , "," , shift_amount , ")"
       | "scale(" , identifier , "," , "factor:" , number_literal , ")"
       | assignment ;

clamp_spec = "max:" , number_literal
           | "min:" , number_literal
           | "min:" , number_literal , "," , "max:" , number_literal ;

shift_amount = [ "+" | "-" ] , number_literal ;

assignment = identifier , assign_op , expression ;

assign_op = "=" | "*=" | "+=" | "-=" | "/=" ;

if_true_property = "if_true:" , "{" , { assignment } , "}" ;

(* ============================================ *)
(* MODEL DEFINITION *)
(* ============================================ *)

model_block = "model:" , model_type , "{" ,
              model_equation ,
              "}" ;

model_type = "multiplicative" | "additive" | "scenario_weighted" ;

model_equation = identifier , "=" , expression ;

expression = term , { ( "+" | "-" ) , term } ;

term = factor , { ( "*" | "/" ) , factor } ;

factor = identifier
       | number_literal
       | "(" , expression , ")" ;

(* ============================================ *)
(* SIMULATION *)
(* ============================================ *)

simulate_block = "simulate:" , "monte_carlo" , "{" ,
                 [ "iterations:" , integer_literal ] ,
                 [ "seed:" , ( "random" | integer_literal ) ] ,
                 "output:" , "{" , output_specs , "}" ,
                 "}" ;

output_specs = { output_spec } ;

output_spec = "probability:" , probability_expr
            | "distribution:" , distribution_output
            | "sensitivity:" , "variance_decomposition"
            | "confidence_interval:" , interval_spec ;

probability_expr = "p(" , comparison_expr , ")" ;

distribution_output = "histogram(" , "bins:" , integer_literal , ")" ;

interval_spec = "[" , percentile_list , "]" ;

percentile_list = percentile , { "," , percentile } ;

percentile = "p" , integer_literal ;

(* ============================================ *)
(* EXTERNAL SIGNALS *)
(* ============================================ *)

external_block = "external_signals:" , "[" , external_signal_list , "]" ;

external_signal_list = external_signal , { "," , external_signal } ;

external_signal = identifier , "{" ,
                  "source:" , string_literal ,
                  [ "query:" , string_literal ] ,
                  [ "instrument:" , string_literal ] ,
                  [ "update:" , update_frequency ] ,
                  [ "implied_probability:" , extraction_method ] ,
                  "}" ;

update_frequency = "realtime" | "daily" | "weekly" ;

extraction_method = "extract_from_options"
                  | "extract_from_consensus"
                  | "direct" ;

(* ============================================ *)
(* ARBITRAGE DETECTION *)
(* ============================================ *)

arbitrage_block = "arbitrage:" , "detect" , "{" ,
                  "internal:" , identifier ,
                  "external:" , "[" , identifier_list , "]" ,
                  "when" , condition , "{" ,
                  arbitrage_actions ,
                  "}" ,
                  "}" ;

arbitrage_actions = { arbitrage_action } ;

arbitrage_action = "alert:" , string_literal
                 | "hypothesis:" , "generate_explanation" , "(" , identifier_list , ")"
                 | "trade_signal:" , trade_signal_expr ;

trade_signal_expr = "if" , condition , "then" , string_literal ,
                    "else" , string_literal ;

(* ============================================ *)
(* RESOLUTION *)
(* ============================================ *)

on_resolve_block = "on_resolve:" , "{" ,
                   [ outcome_fetch ] ,
                   [ brier_calculation ] ,
                   [ calibration_update ] ,
                   [ retrospective ] ,
                   "}" ;

outcome_fetch = "outcome:" , "fetch_from(" , string_literal , "," , string_literal , ")" ;

brier_calculation = "brier_score:" , "calculate(" , identifier , "," , identifier , ")" ;

calibration_update = "calibration:" , "update_user_profile(" , identifier , ")" ;

retrospective = "retrospective:" , "{" , retrospective_actions , "}" ;

retrospective_actions = { retrospective_action } ;

retrospective_action = identifier , ":" , function_call ;

function_call = identifier , [ "(" , [ argument_list ] , ")" ] ;

argument_list = identifier , { "," , identifier } ;

(* ============================================ *)
(* LITERALS *)
(* ============================================ *)

string_literal = '"' , { character - '"' } , '"'
               | "'''" , { character } , "'''" ;  (* multiline *)

multiline_string = '"""' , { character } , '"""' ;

number_literal = [ "-" ] , ( integer_literal | float_literal ) ,
                 [ size_suffix ] ;

integer_literal = digit , { digit | "_" } ;

float_literal = digit , { digit | "_" } , "." , digit , { digit | "_" } ,
                [ exponent ] ;

exponent = ( "e" | "E" ) , [ "+" | "-" ] , digit , { digit } ;

size_suffix = "K" | "M" | "B" | "T" ;  (* 1K = 1000, 1M = 1000000, etc. *)

probability_literal = ( "0" | "1" ) , [ "." , digit , { digit } ]  (* 0.0 to 1.0 *)
                    | integer_literal , "%" ;  (* 0% to 100% *)

date_literal = digit , digit , digit , digit , "-" ,
               digit , digit , "-" ,
               digit , digit ;  (* YYYY-MM-DD *)

time_literal = digit , digit , ":" , digit , digit ;  (* HH:MM *)

identifier = ( letter | "_" ) , { letter | digit | "_" } ;

letter = "a" .. "z" | "A" .. "Z" ;

digit = "0" .. "9" ;

(* ============================================ *)
(* COMMENTS *)
(* ============================================ *)

comment = "//" , { character - newline } , newline
        | "/*" , { character } , "*/" ;
```

---

## Type System

```rust
// FPL Type System

enum FPLType {
    // Primitives
    Number,
    Probability,  // 0.0-1.0
    String,
    Boolean,
    Date,
    
    // Composite
    Distribution(DistributionType),
    Driver(DriverType),
    Evidence,
    Agent,
    
    // Collections
    Array(Box<FPLType>),
    
    // Special
    Expression,  // Mathematical expression
    Constraint,  // Conditional rule
}

enum DistributionType {
    Triangular { p5: f64, p50: f64, p95: f64 },
    Normal { mean: f64, stddev: f64 },
    Lognormal { median: f64, sigma: f64 },
    Uniform { low: f64, high: f64 },
    Beta { alpha: f64, beta: f64 },
}

enum DriverType {
    Continuous,
    Binary,
}
```

---

## Keywords

```
Reserved keywords:
  question, driver, evidence, agent, model, simulate, external_signals,
  arbitrage, on_resolve, continuous, binary, triangular, normal, lognormal,
  uniform, beta, if, then, when, detect, multiplicative, additive,
  scenario_weighted, monte_carlo, reference_class, confidence, name, unit,
  dist, prob, rationale, agents, constraints, schedule, query, triggers,
  source, date, content, impact, increase, decrease, neutral, weak, moderate,
  strong, clamp, shift, scale, iterations, seed, output, probability,
  distribution, sensitivity, histogram, bins, fetch_from, calculate,
  update_user_profile, generate_explanation, true, false
```

---

## Operator Precedence

```
Highest:
  ()                  // Grouping
  * / %              // Multiplication, division, modulo
  + -                // Addition, subtraction
  < <= > >= == !=    // Comparison
  &&                 // Logical AND
  ||                 // Logical OR
  =                  // Assignment
Lowest:
```

---

## Complete Example

```fpl
// ASTS Revenue Forecast
question "Will ASTS reach $200M revenue by 2026-12-31?" {
    domain: satellite_telecom
    resolution_criteria: "Audited annual revenue ≥ $200M in fiscal 2026"
    target_date: 2026-12-31
    
    base_rate: reference_class(
        "pre-revenue satellite companies reaching 24mo targets",
        confidence: medium
    )
    
    model: revenue_model
}

// Fermi Decomposition
model: multiplicative {
    revenue = market_tam * market_share * arpu * (months_active / 12)
}

// Driver 1: Market TAM
driver market_tam: continuous {
    name: "Total Addressable Market"
    unit: USD
    
    dist: triangular {
        p5:  2_000_000_000,  // $2B
        p50: 5_000_000_000,  // $5B
        p95: 7_000_000_000   // $7B
    }
    
    rationale: """
    Conservative estimate based on:
    - IoT connectivity growth
    - Maritime/aviation demand  
    - Emergency services adoption
    
    FCC approval (2024-11-01) reduces regulatory risk.
    """
    
    agents: [
        research_analyst {
            query: "satellite connectivity market TAM 2026"
            schedule: weekly(monday, 09:00)
            confidence_threshold: 0.7
        },
        
        market_researcher {
            query: "global satellite phone adoption trends"
            schedule: monthly
            regions: ["north_america", "asia_pacific"]
        }
    ]
    
    evidence: [
        manual {
            source: "Morgan Stanley"
            date: 2024-11-15
            content: "TAM $5.8B by 2026, 28% CAGR"
            impact: increase(moderate)
        }
    ]
}

// Driver 2: Market Share
driver market_share: continuous {
    name: "Market Share (%)"
    unit: percent
    
    dist: triangular {
        p5:  0.08,
        p50: 0.15,
        p95: 0.25
    }
    
    // Constraints based on competitive dynamics
    constraints: [
        if competitive_pressure > 0.7 
            then clamp(market_share.p95, max: 0.20),
        
        if regulatory_approval == true 
            then shift(market_share, +0.03)
    ]
    
    agents: [
        competitive_intel {
            query: "ASTS vs Starlink OneWeb market positioning"
            schedule: weekly
            competitors: ["Starlink", "OneWeb", "Lynk"]
        }
    ]
}

// Driver 3: Regulatory Risk (Binary)
driver regulatory_risk: binary {
    name: "Major Regulatory Blocker"
    prob: 0.15  // 15% chance
    
    // If regulatory blocker occurs, impact other drivers
    if_true: {
        launch_timeline *= 1.5,  // 50% delay
        market_tam *= 0.7        // 30% TAM reduction
    }
    
    agents: [
        regulatory_monitor {
            query: "FCC spectrum AST SpaceMobile"
            schedule: daily
            triggers: ["FCC", "spectrum", "approval", "denial"]
            alert_on: sentiment_change(threshold: 0.3)
        }
    ]
}

// Monte Carlo Simulation
simulate: monte_carlo {
    iterations: 10_000
    seed: random
    
    output: {
        probability: p(revenue >= 200_000_000),
        distribution: histogram(bins: 50),
        sensitivity: variance_decomposition,
        confidence_interval: [p10, p50, p90]
    }
}

// External Market Signals
external_signals: [
    analyst_consensus {
        source: "Bloomberg Terminal"
        query: "ASTS revenue estimates 2026"
        update: daily
    },
    
    options_market {
        source: "CBOE"
        instrument: "ASTS 2026 calls"
        implied_probability: extract_from_options
    }
]

// Arbitrage Detection
arbitrage: detect {
    internal: this.probability,
    external: [analyst_consensus, options_market],
    
    when abs(internal - external.mean) > 0.15 {
        alert: "Disequilibrium detected"
        hypothesis: generate_explanation(internal, external)
        trade_signal: if internal < external then "SHORT" else "LONG"
    }
}

// Resolution & Learning
on_resolve: {
    outcome: fetch_from("SEC EDGAR", "ASTS 2026 annual revenue")
    brier_score: calculate(this.probability, outcome)
    calibration: update_user_profile(this.brier_score)
    
    retrospective: {
        decompose_error: which_drivers_were_wrong,
        pattern_detection: analyze_bias,
        learning_note: extract_lessons
    }
}
```

---

## Parser Implementation Notes

### Lexer Tokens

```rust
enum Token {
    // Keywords
    Question, Driver, Evidence, Agent, Model, Simulate,
    Continuous, Binary, Triangular, Normal, If, Then,
    
    // Literals
    String(String),
    Number(f64),
    Probability(f64),
    Date(NaiveDate),
    Identifier(String),
    
    // Operators
    Plus, Minus, Star, Slash, Equals,
    Greater, Less, GreaterEqual, LessEqual,
    
    // Delimiters
    LBrace, RBrace, LParen, RParen, LBracket, RBracket,
    Comma, Colon, Semicolon,
    
    // Special
    Newline, Comment, EOF,
}
```

### Parser Strategy

```rust
// Use Pest for PEG parsing
#[grammar = "fpl.pest"]
pub struct FPLParser;

// Parse tree -> AST transformation
pub fn parse(input: &str) -> Result<Program, ParseError> {
    let pairs = FPLParser::parse(Rule::program, input)?;
    // Transform pairs into AST
    build_ast(pairs)
}
```

### Error Messages

```rust
// Helpful error messages with Fermi's personality

"Expected '}' after driver block
   Found: 'agent'
   
   🦊 Fermi: Looks like you forgot to close your driver block.
            Did you mean to add the agent inside the driver?" 

"Type mismatch: Expected number, found string
   Line 42: prob: \"high\"
                  ^^^^^^
   
   🦊 Fermi: Probability should be a number (0.0-1.0) or percentage (0%-100%).
            Try: prob: 0.75 or prob: 75%"

"Invalid distribution parameters: p5 >= p50
   Line 18: p5: 100, p50: 50
   
   🦊 Fermi: Your pessimistic estimate (p5=100) is higher than your likely 
            estimate (p50=50). That's backwards. p5 < p50 < p95."
```

---

## Language Server Protocol (LSP)

For editor integration (Zed, VSCode, etc.):

```rust
// FPL Language Server Features:
// - Syntax highlighting
// - Autocomplete (context-aware)
// - Hover tooltips (definitions, types)
// - Go to definition
// - Rename refactoring
// - Diagnostics (errors, warnings)
// - Fermi coaching hints (inline)

struct FPLLanguageServer {
    parser: FPLParser,
    semantic_analyzer: SemanticAnalyzer,
    fermi_coach: FermiCoach,
}

impl FPLLanguageServer {
    fn autocomplete(&self, position: Position) -> Vec<CompletionItem> {
        // Context-aware suggestions
        match self.get_context(position) {
            Context::DriverBlock => vec![
                CompletionItem::new("dist: triangular"),
                CompletionItem::new("agents: []"),
                // ...
            ],
            Context::AgentList => self.available_agents(),
            // ...
        }
    }
    
    fn hover(&self, position: Position) -> Option<Hover> {
        // Show type info + Fermi hint
        match self.get_symbol(position) {
            Some(Symbol::Driver { name, type }) => {
                Some(Hover {
                    content: format!(
                        "Driver: {}\nType: {:?}\n\n🦊 Fermi: {}",
                        name, type, self.fermi_coach.hint_for_driver(name)
                    )
                })
            }
            // ...
        }
    }
}
```

---

## Validation Rules

```rust
// Semantic validation beyond syntax

enum ValidationRule {
    // Distribution constraints
    TriangularOrdering,      // p5 < p50 < p95
    ProbabilityRange,        // 0.0 <= prob <= 1.0
    PositiveValues,          // No negatives for certain types
    
    // Model constraints
    AllDriversUsed,          // Model references all drivers
    NoUndefinedReferences,   // All identifiers defined
    TypeConsistency,         // Type checking
    
    // Logical constraints
    DateOrdering,            // Target date > today
    ScheduleValidity,        // Cron expressions valid
    
    // Fermi constraints
    MinimumDrivers,          // At least 2 drivers for decomposition
    EvidencePresence,        // Warn if no evidence
    UncertaintyReasonable,   // Warn if range too wide/narrow
}

impl Validator {
    fn validate(&self, ast: &Program) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        
        for driver in &ast.drivers {
            if let Some(dist) = &driver.distribution {
                if !self.validate_distribution(dist) {
                    diagnostics.push(Diagnostic::error(
                        driver.span,
                        "Invalid distribution parameters"
                    ));
                }
            }
        }
        
        diagnostics
    }
}
```

---

## Standard Library

Built-in functions available in FPL:

```fpl
// Mathematical
abs(x)           // Absolute value
sqrt(x)          // Square root
pow(x, y)        // x^y
log(x)           // Natural log
exp(x)           // e^x

// Statistical
mean(arr)        // Average
median(arr)      // Median
stddev(arr)      // Standard deviation
percentile(arr, p)  // pth percentile

// Probability
p(condition)     // Probability of condition
cdf(dist, x)     // Cumulative distribution at x
pdf(dist, x)     // Probability density at x

// String
concat(s1, s2)   // Concatenate strings
format(fmt, ...) // String formatting

// Date
today()          // Current date
days_until(date) // Days from now to date
add_days(date, n)  // Date arithmetic

// Fermi Helpers
decompose(question)  // Suggest decomposition
base_rate(class)     // Look up historical rate
calibrate(prob, outcome)  // Calculate Brier
```

---

## Next Steps

1. **Implement Lexer/Parser** (use Pest)
2. **Build AST** (typed syntax tree)
3. **Semantic Analysis** (validation)
4. **Code Generation** (AST → executable)
5. **LSP Server** (editor integration)
6. **REPL** (interactive testing)
7. **Standard Library** (built-in functions)

See `crates/uffp-dsl/` for implementation.
