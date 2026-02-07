# Phase 1 Day 1: Current State Analysis

**Date:** 2026-02-05  
**Purpose:** Understand current AST/Parser/LSP before making ANY changes  
**Status:** In Progress

---

## Current AST Structure

**File:** `/home/ilabra/fermi/src/ast.rs`

### **AgentStmt (Current)**

```rust
/// Agent statement: defines a research agent
#[derive(Debug, Clone, PartialEq)]
pub struct AgentStmt {
    pub name: String,
    pub agent_type: Option<String>,  // Currently Option<String>, not enum
    pub query: String,
    pub schedule: Option<Schedule>,
    pub driver_refs: Vec<String>,    // EXISTS but NEVER POPULATED by parser
}
```

**Key Observations:**
1. ✅ `driver_refs` field exists (but parser doesn't populate it)
2. ❌ `agent_type` is `Option<String>` (should be enum)
3. ❌ No `executor` field
4. ❌ No `depends_on` field
5. ❌ No `confidence_threshold` field

### **Existing Enums**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Schedule {
    Once,
    Every { interval: u32, unit: TimeUnit },
    Cron(String),  // Defined but not parsed yet
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeUnit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeneratedBy {
    Agent(String),  // Already used in BaseRate
    Human,
}
```

**Key Observations:**
1. ✅ Schedule/TimeUnit enums exist and work
2. ✅ GeneratedBy enum exists (used in BaseRate)
3. ✅ Good pattern to follow for ExecutorType

### **Display Implementations**

**Checked:** `grep -n "impl.*Display" src/ast.rs`

**Found:**
- `impl fmt::Display for Program` (line 210)
- `impl fmt::Display for Statement` (line 216)
- `impl fmt::Display for Distribution` (line 229)
- `impl fmt::Display for Expression` (line 241)

**Missing:**
- No Display for AgentStmt
- No Display for Schedule/TimeUnit
- Will need Display for ExecutorType

---

## Current Parser Implementation

**File:** `/home/ilabra/fermi/src/parser.rs`

### **parse_agent() Function** (lines 618-681)

```rust
fn parse_agent(&mut self) -> ParseResult<AgentStmt> {
    self.consume_keyword(TokenType::Agent, "agent")?;
    let name = self.consume_identifier()?;
    self.consume_token(TokenType::LBrace, "{")?;

    let mut agent_type = None;
    let mut query = String::new();
    let mut schedule = None;
    let driver_refs = Vec::new();  // INITIALIZED BUT NEVER MODIFIED!

    while !self.check(&TokenType::RBrace) && !self.is_at_end() {
        let field_token = self.peek().token_type.clone();

        let field: String = match &field_token {
            TokenType::Schedule => {
                self.advance();
                "schedule".to_string()
            }
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                name
            }
            _ => {
                return Err(ParseError::UnexpectedToken { ... });
            }
        };

        self.consume_token(TokenType::Colon, ":")?;

        match field.as_str() {
            "type" => {
                agent_type = Some(self.consume_string()?);
            }
            "query" => {
                query = self.consume_string()?;
            }
            "schedule" => {
                schedule = Some(self.parse_schedule()?);
            }
            _ => {
                self.skip_until_newline_or_rbrace();
            }
        }
    }

    self.consume_token(TokenType::RBrace, "}")?;

    Ok(AgentStmt {
        name,
        agent_type,
        query,
        schedule,
        driver_refs,  // Always empty Vec!
    })
}
```

**Key Observations:**
1. ✅ Good field parsing pattern (match on field name)
2. ❌ `driver_refs` is declared but never parsed
3. ❌ Unknown fields are silently skipped (`skip_until_newline_or_rbrace`)
4. ✅ Uses helper `parse_schedule()` for complex fields

### **parse_schedule() Function** (lines 684-708)

```rust
fn parse_schedule(&mut self) -> ParseResult<Schedule> {
    if self.match_token(&TokenType::Every) {
        let interval = self.parse_number()? as u32;
        let unit_str = self.consume_identifier()?;

        let unit = match unit_str.as_str() {
            "minute" | "minutes" => TimeUnit::Minute,
            "hour" | "hours" => TimeUnit::Hour,
            "day" | "days" => TimeUnit::Day,
            "week" | "weeks" => TimeUnit::Week,
            "month" | "months" => TimeUnit::Month,
            _ => {
                return Err(ParseError::InvalidExpression {
                    message: format!("Invalid time unit: {}", unit_str),
                    ...
                })
            }
        };

        Ok(Schedule::Every { interval, unit })
    } else {
        Ok(Schedule::Once)
    }
}
```

**Key Observations:**
1. ✅ Good pattern for parsing enums from strings
2. ✅ Proper error handling for invalid values
3. ✅ Can use same pattern for ExecutorType

### **Existing Helper Functions**

**Relevant helpers found:**
- `consume_string()` - For string fields
- `consume_identifier()` - For identifiers
- `parse_number()` - For numbers
- `parse_probability_value()` - For 0.0-1.0 values
- `parse_string_array()` - **For arrays like driver_refs!**
- `skip_until_newline_or_rbrace()` - Skip unknown fields

**Key Finding:** `parse_string_array()` already exists! Used for `key_findings` in evidence. We can use it for `driver_refs` and `depends_on`.

---

## LSP Current State

### **Hover Documentation**

**File:** `/home/ilabra/fermi/fermi-lsp/src/hover/keywords.rs`

**Current agent hover:**
```rust
"agent" => "**agent** - Create automated research agent

Scheduled agent that monitors and tracks information over time.

**Syntax:** `agent <name> { ... }`

**Example:**
```fpl
agent market_monitor {
    query: \"semiconductor market growth\"
    schedule: every 1 week
}
```"
```

**Key Observations:**
1. ✅ Agent keyword documented
2. ❌ No mention of executor, depends_on, confidence_threshold

**File:** `/home/ilabra/fermi/fermi-lsp/src/hover/properties.rs`

**Agent properties documented:**
- ❌ No "executor" property
- ❌ No "depends_on" property  
- ❌ No "confidence_threshold" property
- ❌ No "driver_refs" property (field exists but not documented)

### **Completions**

**File:** `/home/ilabra/fermi/fermi-lsp/src/completions/keywords.rs`

**Current agent completion:**
```rust
CompletionBuilder::keyword("agent")
    .detail("Define autonomous research agent")
    .docs("Agent that researches and generates evidence on schedule")
    .snippet("agent ${1:name} {\n\tquery: \"${2:search query}\"\n\tschedule: every ${3:1} ${4|day,week,month|}\n}")
    .sort("05_agent")
    .build(),
```

**Key Observations:**
1. ✅ Good snippet pattern with placeholders
2. ❌ Missing executor field
3. ❌ Missing other new fields

**File:** `/home/ilabra/fermi/fermi-lsp/src/completions/mod.rs`

**Context detection:**
```rust
} else if line.starts_with("agent ") {
    in_agent = true;
    break;
}
```

**Then provides completions:**
```rust
if in_agent {
    completions.extend(get_agent_property_completions());
}
```

**File:** `/home/ilabra/fermi/fermi-lsp/src/completions/mod.rs` (agent property completions)

**Current properties:**
1. `query` - Search query string
2. `schedule` - Execution schedule with snippet

**Missing:**
- executor
- driver_refs
- depends_on
- confidence_threshold

---

## Lexer State

**File:** `/home/ilabra/fermi/src/lexer.rs`

**Need to check for keywords:**

```bash
grep -n "executor\|depends_on\|confidence_threshold" src/lexer.rs
```

**Result:** Need to verify if these keywords exist

---

## Grammar/Highlighting State

**Files to check:**
- `extensions/fermi/grammars/fpl/grammar.js`
- `extensions/fermi/grammars/fpl/queries/highlights.scm`

**Need to verify:** Do these need updates for new keywords?

---

## Semantic Analysis State

**File:** `/home/ilabra/fermi/src/semantic.rs`

**Current agent handling (lines 140-142):**
```rust
Statement::Agent(_) => {
    // Agents are validated separately
}
```

**Key Observations:**
1. ❌ Placeholder only - no validation
2. ❌ No circular dependency detection
3. ❌ No driver_refs validation
4. ❌ No depends_on validation

**Need to implement:**
```rust
Statement::Agent(a) => self.analyze_agent(a),
```

With full validation including cycle detection.

---

## Symbol Table State

**File:** `/home/ilabra/fermi/src/symbol_table.rs` (lines 174-180)

```rust
Statement::Agent(agent) => {
    if let Err(e) = self.table.define(
        agent.name.clone(),
        SymbolType::Agent,
        Type::String, // Agents produce evidence
        None,
    ) {
        self.errors.push(e);
    }
}
```

**Key Observations:**
1. ✅ Agents registered in symbol table
2. ✅ Type is `SymbolType::Agent`
3. ✅ Can be referenced by name (for depends_on validation)

---

## Validation Script State

**File:** `scripts/validate-components.sh`

**Checks:**
- Keyword coverage (lexer vs LSP)
- Property coverage (AST vs LSP)
- Grammar sync

**Will catch:**
- Missing hover docs
- Missing completions
- Out-of-sync keywords

---

## Summary: What Needs to Change

### **Change 1: Add ExecutorType enum**

**Files to modify:**
1. ✅ `src/ast.rs` - Add enum, add field to AgentStmt
2. ✅ `src/lexer.rs` - Check if "executor" keyword exists
3. ✅ `src/parser.rs` - Add parse_executor_type(), update parse_agent()
4. ✅ `fermi-lsp/src/hover/properties.rs` - Document executor
5. ✅ `fermi-lsp/src/completions/keywords.rs` - Update agent snippet
6. ✅ Test file: `examples/test_executor_types.fpl`

### **Change 2: Parse driver_refs** (field exists, not parsed)

**Files to modify:**
1. ✅ `src/parser.rs` - Add parsing (use parse_string_array)
2. ✅ `src/semantic.rs` - Validate references exist
3. ✅ `fermi-lsp/src/hover/properties.rs` - Document driver_refs
4. ✅ Test file: `examples/test_driver_refs.fpl`

### **Change 3: Add depends_on field**

**Files to modify:**
1. ✅ `src/ast.rs` - Add field
2. ✅ `src/parser.rs` - Parse string array
3. ✅ `src/semantic.rs` - Validate + circular dependency detection
4. ✅ `fermi-lsp/src/hover/properties.rs` - Document depends_on
5. ✅ Test file: `examples/test_depends_on.fpl`

### **Change 4: Add confidence_threshold field**

**Files to modify:**
1. ✅ `src/ast.rs` - Add field
2. ✅ `src/parser.rs` - Parse probability
3. ✅ `src/semantic.rs` - Validate range [0.0, 1.0]
4. ✅ `fermi-lsp/src/hover/properties.rs` - Document confidence_threshold
5. ✅ Test file: `examples/test_confidence_threshold.fpl`

---

## Critical Dependencies Map

### **Change 1 (ExecutorType) Dependencies:**

```
src/ast.rs (add enum + field)
    ↓
src/parser.rs (parse executor)
    ↓
fermi-lsp/src/hover/properties.rs (document it)
    ↓
fermi-lsp/src/completions/keywords.rs (autocomplete it)
    ↓
examples/test_executor_types.fpl (test it)
    ↓
validate-components.sh (verify sync)
    ↓
Zed rebuild + test
```

**MUST BE ATOMIC:** All changes in ONE commit

### **File Modification Order (for safety):**

1. AST first (data structures)
2. Parser second (populate structures)
3. Semantic third (validate structures)
4. LSP fourth (document structures)
5. Tests fifth (verify everything)
6. Validate sixth (ensure sync)
7. Rebuild extension seventh
8. Test in Zed eighth
9. Commit only if ALL pass

---

## Risks Identified

### **High Risk:**
1. ❌ Zed extension caching (must clear `~/.cache/zed/*`)
2. ❌ Out-of-sync files (AST, Parser, LSP must all match)
3. ❌ Grammar highlighting (may need updates)

### **Medium Risk:**
1. ⚠️ Parse existing .fpl files (backward compatibility)
2. ⚠️ Unknown field handling (currently skipped silently)

### **Low Risk:**
1. ✅ Symbol table (already handles agents)
2. ✅ Validation script (catches most issues)

---

## Mitigation Strategies

### **For High Risks:**

**Zed Caching:**
```bash
# After EVERY change:
rm -rf ~/.cache/zed/*
# Restart Zed COMPLETELY (not just reload)
```

**File Sync:**
```bash
# Checklist for EVERY commit:
- [ ] AST updated
- [ ] Parser updated
- [ ] Semantic updated (if needed)
- [ ] LSP hover updated
- [ ] LSP completions updated
- [ ] Test file created
- [ ] Validation script passes
- [ ] Extension rebuilt
- [ ] Zed tested
```

**Grammar Updates:**
```bash
# Check if grammar needs updates:
git diff extensions/fermi/grammars/fpl/grammar.js
# Usually not needed for property additions
```

### **For Medium Risks:**

**Backward Compatibility:**
- All new fields should be Optional
- Parser should handle missing fields gracefully
- Existing forecasts should parse without errors

**Unknown Fields:**
- Currently silently skipped (good)
- Warning message? (future enhancement)

---

## Next Steps (Day 2-3)

**After this analysis complete:**
1. Get approval on change plan
2. Create detailed Change 1 checklist
3. Execute Change 1 (ExecutorType)
4. Test exhaustively
5. Commit ONLY if perfect
6. Move to Change 2

---

**Status:** Analysis Complete - Ready for detailed planning  
**Next:** Create Change 1 detailed execution checklist
