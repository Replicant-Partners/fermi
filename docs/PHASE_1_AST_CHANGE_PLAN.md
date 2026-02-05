# Phase 1: AST Extension - Detailed Change Plan

**Status:** Ready for Implementation  
**Date:** 2026-02-05  
**Duration:** 2 weeks (being extremely careful)  

---

## Objective

Extend FPL AST to support Agent Bestiary features while maintaining 100% hover/autocomplete/highlighting functionality.

---

## Changes Required

### **New Types**

```rust
pub enum ExecutorType {
    LLM,
    MCP,
    Manual,
    Skill,
}
```

### **Modified AgentStmt**

```rust
pub struct AgentStmt {
    pub name: String,
    pub agent_type: AgentType,              // EXISTS
    pub query: String,                      // EXISTS  
    pub executor: ExecutorType,             // NEW
    pub schedule: Option<Schedule>,         // EXISTS
    pub driver_refs: Vec<String>,           // EXISTS but NOT PARSED
    pub depends_on: Vec<String>,            // NEW
    pub confidence_threshold: Option<f64>,  // NEW
}
```

---

## Change 1: ExecutorType Enum (Days 1-3)

### **Day 1: Research & Planning**

**Files to read:**
- [x] `src/ast.rs` (lines 125-150) - Current AgentStmt
- [x] `src/parser.rs` (lines 618-708) - parse_agent()
- [x] `src/lexer.rs` - Check if "executor" keyword exists
- [x] `fermi-lsp/src/hover/keywords.rs` - Agent hover docs
- [x] `fermi-lsp/src/hover/properties.rs` - Property docs
- [x] `fermi-lsp/src/completions/keywords.rs` - Agent completion
- [x] `fermi-lsp/src/completions/mod.rs` - Context detection

**Create detailed synchronization checklist:**

```markdown
## ExecutorType Implementation Checklist

### AST Changes
- [ ] Add ExecutorType enum to `src/ast.rs`
- [ ] Add `executor: ExecutorType` to AgentStmt
- [ ] Add Display impl for ExecutorType
- [ ] Add Debug impl for ExecutorType
- [ ] Update agent_type to use enum (if currently string)

### Parser Changes
- [ ] Add "executor" keyword to lexer (if not exists)
- [ ] Add `parse_executor_type()` helper in parser
- [ ] Update `parse_agent()` to parse executor field
- [ ] Add error handling for invalid executor values
- [ ] Add test cases for parser

### Semantic Analysis
- [ ] No validation needed for ExecutorType (all values valid)

### LSP Hover Documentation
- [ ] Add "executor" property to hover/properties.rs
- [ ] Document valid values: llm, mcp, manual, skill
- [ ] Add examples for each executor type

### LSP Completions
- [ ] Update agent snippet in completions/keywords.rs
- [ ] Add executor field to snippet with enum values
- [ ] Use snippet placeholders: ${N|llm,mcp,manual,skill|}

### Test Files
- [ ] Create examples/test_executor_types.fpl
- [ ] Test all four executor types
- [ ] Test missing executor (should work, optional field)

### Validation
- [ ] Run ./scripts/validate-components.sh
- [ ] Build: cargo build --release
- [ ] Parse test file: ./target/release/fermi examples/test_executor_types.fpl
- [ ] Rebuild extension: bash scripts/install-extension.sh
- [ ] Clear Zed cache: rm -rf ~/.cache/zed/*
- [ ] Restart Zed completely
- [ ] Test hover on "executor" keyword
- [ ] Test autocomplete inside agent block
- [ ] Test syntax highlighting
- [ ] Verify no parser errors

### Git Commit
- [ ] Stage all related files
- [ ] Commit message: "feat(ast): add ExecutorType enum for agent execution"
- [ ] Verify commit includes ALL changed files
```

### **Day 2: Implementation**

**Step-by-step execution:**

1. **Read current AgentStmt structure**
   ```bash
   cat src/ast.rs | grep -A 20 "pub struct AgentStmt"
   ```

2. **Add ExecutorType enum to ast.rs**
   ```rust
   // After existing enums, before AgentStmt
   #[derive(Debug, Clone, PartialEq)]
   pub enum ExecutorType {
       LLM,
       MCP,
       Manual,
       Skill,
   }
   
   impl Display for ExecutorType {
       fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
           match self {
               ExecutorType::LLM => write!(f, "llm"),
               ExecutorType::MCP => write!(f, "mcp"),
               ExecutorType::Manual => write!(f, "manual"),
               ExecutorType::Skill => write!(f, "skill"),
           }
       }
   }
   ```

3. **Check if agent_type needs enum conversion**
   - If currently `agent_type: Option<String>`, leave for now
   - Focus on executor field only

4. **Add executor field to AgentStmt**
   ```rust
   pub struct AgentStmt {
       pub name: String,
       pub agent_type: Option<String>,  // Leave as-is for now
       pub query: String,
       pub schedule: Option<Schedule>,
       pub driver_refs: Vec<String>,
       pub executor: Option<ExecutorType>,  // NEW - optional for now
   }
   ```

5. **Check lexer for "executor" keyword**
   ```bash
   grep -n "executor" src/lexer.rs
   ```
   - If not present, add to keywords list

6. **Add parser helper**
   ```rust
   fn parse_executor_type(&mut self) -> ParseResult<ExecutorType> {
       let executor_str = self.consume_string()?;
       match executor_str.as_str() {
           "llm" => Ok(ExecutorType::LLM),
           "mcp" => Ok(ExecutorType::MCP),
           "manual" => Ok(ExecutorType::Manual),
           "skill" => Ok(ExecutorType::Skill),
           _ => Err(ParseError::InvalidValue {
               expected: "executor type (llm, mcp, manual, skill)".to_string(),
               found: executor_str,
               line: self.current_line(),
           }),
       }
   }
   ```

7. **Update parse_agent() function**
   ```rust
   fn parse_agent(&mut self) -> ParseResult<AgentStmt> {
       // ... existing code ...
       
       let mut executor = None;  // NEW
       
       // In property parsing loop:
       "executor" => {
           executor = Some(self.parse_executor_type()?);
       }
       
       // ... rest of function ...
       
       Ok(AgentStmt {
           name,
           agent_type,
           query,
           schedule,
           driver_refs,
           executor,  // NEW
       })
   }
   ```

8. **Update LSP hover for "executor" property**
   
   In `fermi-lsp/src/hover/properties.rs`:
   ```rust
   "executor" => "**executor** - Agent execution backend\n\nSpecifies how this agent executes its query.\n\n**Values:**\n- `llm` - Use LLM (Claude) to research query\n- `mcp` - Call MCP tools for data\n- `manual` - Human-in-the-loop\n- `skill` - Invoke Anthropic skill\n\n**Example:**\n```fpl\nagent market_research {\n    executor: \"llm\"\n}\n```\n\n**Default:** llm (if not specified)",
   ```

9. **Update LSP completions**
   
   In `fermi-lsp/src/completions/keywords.rs`:
   ```rust
   // Update agent snippet
   CompletionBuilder::keyword("agent")
       .detail("Define autonomous research agent")
       .docs("Agent that researches and generates evidence on schedule")
       .snippet("agent ${1:name} {\n\ttype: \"${2|research,sentiment,competitive,market|}\"\n\tquery: \"${3:research query}\"\n\texecutor: \"${4|llm,mcp,manual,skill|}\"\n\tschedule: every ${5:1} ${6|day,week,month|}\n}")
       .sort("05_agent")
       .build()
   ```

10. **Create test file**
    
    `examples/test_executor_types.fpl`:
    ```fpl
    # Test all executor types
    
    agent llm_agent {
        type: "research"
        query: "Test LLM executor"
        executor: "llm"
    }
    
    agent mcp_agent {
        type: "research"
        query: "Test MCP executor"
        executor: "mcp"
    }
    
    agent manual_agent {
        type: "research"
        query: "Test manual executor"
        executor: "manual"
    }
    
    agent skill_agent {
        type: "research"
        query: "Test skill executor"
        executor: "skill"
    }
    
    # Test optional (should work without executor)
    agent no_executor {
        type: "research"
        query: "No executor specified"
    }
    ```

### **Day 3: Validation & Testing**

**Validation checklist:**

1. **Run validation script**
   ```bash
   ./scripts/validate-components.sh
   ```
   - Should pass with no errors
   - Note any warnings

2. **Build project**
   ```bash
   cargo build --release
   ```
   - Should compile without errors
   - Note any warnings

3. **Parse test file**
   ```bash
   ./target/release/fermi examples/test_executor_types.fpl
   ```
   - Should parse successfully
   - Should show all agents with executor types
   - Should handle missing executor gracefully

4. **Rebuild Zed extension**
   ```bash
   bash scripts/install-extension.sh
   ```
   - Should complete without errors
   - Note new version number

5. **Clear Zed cache**
   ```bash
   rm -rf ~/.cache/zed/*
   ```

6. **Restart Zed COMPLETELY**
   - Not just "Reload Extensions"
   - Quit Zed, restart

7. **Test in Zed**
   - Open `examples/test_executor_types.fpl`
   - Hover over "executor" → Should show documentation
   - Type "executor: " → Should autocomplete with llm|mcp|manual|skill
   - Verify syntax highlighting works
   - Verify no red squiggles

8. **Edge case testing**
   - Try invalid executor: `executor: "invalid"` → Should error
   - Try missing executor → Should work (optional)
   - Try executor in wrong location → Should error

**If ALL tests pass, proceed to commit. If ANY fail, debug before proceeding.**

### **Day 3: Git Commit**

```bash
git add src/ast.rs
git add src/parser.rs
git add src/lexer.rs
git add fermi-lsp/src/hover/properties.rs
git add fermi-lsp/src/completions/keywords.rs
git add examples/test_executor_types.fpl

git commit -m "feat(ast): add ExecutorType enum for agent execution

- Add ExecutorType enum (LLM, MCP, Manual, Skill)
- Add executor field to AgentStmt (optional)
- Add parse_executor_type() parser helper
- Add 'executor' keyword to lexer
- Add LSP hover documentation for executor property
- Update agent completion snippet with executor field
- Add test file with all executor types
- Tests passing, validation script OK
- Zed hover/autocomplete working"

git push origin feature/agent-executor-types
```

**CHECKPOINT: Do not proceed to Change 2 until Change 1 is 100% verified in Zed.**

---

## Change 2: Parse driver_refs (Days 4-5)

**Note:** `driver_refs: Vec<String>` already exists in AgentStmt but is never parsed (always empty)

### **Day 4: Implementation**

**Files to modify:**
- `src/parser.rs` - Add parsing for driver_refs
- `src/semantic.rs` - Validate driver_refs point to real drivers
- `fermi-lsp/src/hover/properties.rs` - Document driver_refs
- `examples/test_driver_refs.fpl` - Test file

**Steps:**

1. **Verify driver_refs exists in AST**
   ```bash
   grep -n "driver_refs" src/ast.rs
   ```

2. **Add parsing in parse_agent()**
   ```rust
   "driver_refs" => {
       driver_refs = self.parse_string_array()?;
   }
   ```
   
   **Note:** `parse_string_array()` already exists (used for evidence key_findings)

3. **Add semantic validation**
   
   In `src/semantic.rs`, `analyze_agent()`:
   ```rust
   // Validate driver_refs point to defined drivers
   for driver_ref in &agent.driver_refs {
       if !self.symbol_table.contains(driver_ref) {
           self.errors.push(SemanticError::UndefinedSymbol {
               name: driver_ref.clone(),
               message: format!(
                   "Agent '{}' references undefined driver '{}'",
                   agent.name, driver_ref
               ),
           });
       }
   }
   ```

4. **Add LSP hover**
   ```rust
   "driver_refs" => "**driver_refs** - Drivers this agent supports\n\nLinks agent to forecast drivers it provides evidence for.\n\n**Example:**\n```fpl\nagent market_research {\n    driver_refs: [\"market_share\", \"competitive_position\"]\n}\n```\n\n**Validation:** All referenced drivers must exist in forecast.",
   ```

5. **Create test file**
   
   `examples/test_driver_refs.fpl`:
   ```fpl
   driver market_share continuous {
       distribution: triangular(0.15, 0.20, 0.25)
   }
   
   driver competitive_position continuous {
       distribution: normal(0.7, 0.1)
   }
   
   agent market_research {
       type: "research"
       query: "AMD market position"
       executor: "llm"
       driver_refs: ["market_share", "competitive_position"]
   }
   
   # Test error: undefined driver
   agent bad_agent {
       driver_refs: ["nonexistent_driver"]  # Should error
   }
   ```

### **Day 5: Validation & Commit**

**Same validation protocol as Change 1:**
- [ ] Validate script
- [ ] Build
- [ ] Parse test file (should show error for bad_agent)
- [ ] Rebuild extension
- [ ] Clear cache
- [ ] Restart Zed
- [ ] Test hover on driver_refs
- [ ] Test autocomplete
- [ ] Verify error for undefined driver

**Commit:**
```bash
git commit -m "feat(parser): parse driver_refs in agent blocks

- Parse driver_refs as string array
- Add semantic validation for undefined drivers
- Add LSP hover for driver_refs property
- Add test with valid and invalid driver refs
- Validation working correctly"
```

---

## Change 3: Add depends_on (Days 6-7)

**NEW field:** `depends_on: Vec<String>` for agent dependencies

### **Day 6: Implementation**

**Steps:**

1. **Add field to AgentStmt**
   ```rust
   pub struct AgentStmt {
       // ... existing fields ...
       pub depends_on: Vec<String>,  // NEW
   }
   ```

2. **Parse depends_on**
   ```rust
   "depends_on" => {
       depends_on = self.parse_string_array()?;
   }
   ```

3. **Add semantic validation**
   
   Validate depends_on references exist:
   ```rust
   for dep in &agent.depends_on {
       if !self.symbol_table.contains(dep) {
           self.errors.push(SemanticError::UndefinedSymbol {
               name: dep.clone(),
               message: format!(
                   "Agent '{}' depends on undefined agent '{}'",
                   agent.name, dep
               ),
           });
       }
   }
   ```
   
   **Circular dependency detection:**
   ```rust
   fn detect_agent_cycle(&self, agent: &AgentStmt) -> Option<Vec<String>> {
       let mut visited = HashSet::new();
       let mut stack = vec![agent.name.clone()];
       
       self.dfs_cycle_check(&agent.name, &mut visited, &mut stack)
   }
   
   fn dfs_cycle_check(
       &self,
       current: &str,
       visited: &mut HashSet<String>,
       stack: &mut Vec<String>,
   ) -> Option<Vec<String>> {
       if stack.contains(&current.to_string()) {
           // Found cycle
           return Some(stack.clone());
       }
       
       if visited.contains(current) {
           return None;  // Already checked this path
       }
       
       visited.insert(current.to_string());
       stack.push(current.to_string());
       
       // Get agent's dependencies
       if let Some(agent) = self.get_agent(current) {
           for dep in &agent.depends_on {
               if let Some(cycle) = self.dfs_cycle_check(dep, visited, stack) {
                   return Some(cycle);
               }
           }
       }
       
       stack.pop();
       None
   }
   ```

4. **Add LSP hover**
   ```rust
   "depends_on" => "**depends_on** - Agent dependencies\n\nSpecify which agents must run before this agent.\n\n**Example:**\n```fpl\nagent competitive_analysis {\n    depends_on: [\"market_research\", \"sentiment_analyzer\"]\n}\n```\n\n**Note:** Circular dependencies are detected and will cause errors.",
   ```

5. **Create test file**
   
   `examples/test_depends_on.fpl`:
   ```fpl
   agent base_research {
       type: "research"
       query: "Base market data"
   }
   
   agent sentiment {
       type: "sentiment"
       query: "Market sentiment"
   }
   
   agent competitive {
       type: "competitive"
       query: "Competitive analysis"
       depends_on: ["base_research", "sentiment"]
   }
   
   # Test circular dependency (should error)
   agent circular_a {
       depends_on: ["circular_b"]
   }
   
   agent circular_b {
       depends_on: ["circular_a"]
   }
   ```

### **Day 7: Validation & Commit**

**Validation:**
- Verify circular dependency detection works
- Verify undefined agent reference errors
- Test dependency chain (3+ levels deep)

**Commit:**
```bash
git commit -m "feat(parser): add depends_on for agent dependencies

- Add depends_on field to AgentStmt
- Parse depends_on as string array
- Validate dependencies exist
- Detect circular dependencies (DFS)
- Add LSP hover for depends_on
- Add comprehensive tests"
```

---

## Change 4: Add confidence_threshold (Days 8-9)

**NEW field:** `confidence_threshold: Option<f64>`

### **Day 8: Implementation**

**Steps:**

1. **Add field to AgentStmt**
   ```rust
   pub confidence_threshold: Option<f64>,
   ```

2. **Parse confidence_threshold**
   ```rust
   "confidence_threshold" => {
       confidence_threshold = Some(self.parse_probability_value()?);
   }
   ```
   
   **Note:** `parse_probability_value()` already exists (used for probabilities)

3. **Add semantic validation**
   ```rust
   if let Some(threshold) = agent.confidence_threshold {
       if threshold < 0.0 || threshold > 1.0 {
           self.errors.push(SemanticError::ValidationError {
               rule: "confidence_threshold_range".to_string(),
               message: format!(
                   "Agent '{}' confidence_threshold must be between 0.0 and 1.0, got {}",
                   agent.name, threshold
               ),
           });
       }
   }
   ```

4. **Add LSP hover**
   ```rust
   "confidence_threshold" => "**confidence_threshold** - Minimum confidence to accept\n\nAgent-generated evidence below this confidence will be rejected.\n\n**Range:** 0.0 to 1.0\n\n**Example:**\n```fpl\nagent market_research {\n    confidence_threshold: 0.75  # Only accept evidence >= 75% confident\n}\n```\n\n**Default:** No threshold (accept all evidence)",
   ```

5. **Update agent completion snippet**
   ```rust
   // Add to snippet (optional):
   .snippet("agent ${1:name} {\n\t...\n\tconfidence_threshold: ${7:0.75}\n}")
   ```

6. **Create test file**
   
   `examples/test_confidence_threshold.fpl`:
   ```fpl
   agent high_confidence {
       type: "research"
       query: "Needs high confidence"
       confidence_threshold: 0.9
   }
   
   agent medium_confidence {
       type: "research"
       query: "Medium confidence OK"
       confidence_threshold: 0.7
   }
   
   # Test invalid value (should error)
   agent invalid_threshold {
       confidence_threshold: 1.5  # > 1.0, should error
   }
   ```

### **Day 9: Validation & Commit**

**Validation:**
- Test valid thresholds (0.5, 0.75, 0.9)
- Test boundary values (0.0, 1.0)
- Test invalid values (-0.1, 1.5) → should error

**Commit:**
```bash
git commit -m "feat(parser): add confidence_threshold for agent quality control

- Add confidence_threshold field to AgentStmt
- Parse as probability value (0.0-1.0)
- Validate range in semantic analysis
- Add LSP hover documentation
- Update agent completion snippet
- Add tests for valid/invalid values"
```

---

## Final Validation (Day 10)

### **Comprehensive Testing**

1. **Parse all test files**
   ```bash
   ./target/release/fermi examples/test_executor_types.fpl
   ./target/release/fermi examples/test_driver_refs.fpl
   ./target/release/fermi examples/test_depends_on.fpl
   ./target/release/fermi examples/test_confidence_threshold.fpl
   ```

2. **Create comprehensive example**
   
   `examples/agent_complete.fpl`:
   ```fpl
   driver market_share continuous {
       distribution: triangular(0.15, 0.20, 0.25)
   }
   
   agent base_research {
       type: "research"
       query: "Base market research"
       executor: "llm"
       driver_refs: ["market_share"]
   }
   
   agent competitive_analysis {
       type: "competitive"
       query: "Competitive dynamics"
       executor: "mcp"
       driver_refs: ["market_share"]
       depends_on: ["base_research"]
       confidence_threshold: 0.8
       schedule: every 1 week
   }
   
   model forecast_model {
       market_share
   }
   
   simulate {
       iterations: 10000
   }
   ```

3. **Test in Zed**
   - Open `agent_complete.fpl`
   - Hover over every keyword
   - Test autocomplete inside agent block
   - Verify syntax highlighting
   - Verify no errors

4. **Run validation script**
   ```bash
   ./scripts/validate-components.sh
   ```
   - Should pass completely

5. **Document changes**
   - Update CHANGELOG.md
   - Update documentation

### **Success Criteria**

✅ All 4 changes implemented  
✅ All tests passing  
✅ Hover working on all new keywords/properties  
✅ Autocomplete suggesting all new fields  
✅ Syntax highlighting correct  
✅ No parser errors  
✅ Validation script passing  
✅ Comprehensive example parses perfectly  

---

## Rollback Plan

**If anything breaks:**

1. **Identify broken commit**
   ```bash
   git log --oneline
   ```

2. **Revert specific commit**
   ```bash
   git revert <commit-hash>
   ```

3. **Or reset to last working state**
   ```bash
   git reset --hard <last-working-commit>
   ```

4. **Rebuild and retest**
   ```bash
   cargo build --release
   bash scripts/install-extension.sh
   rm -rf ~/.cache/zed/*
   ```

---

## Post-Phase 1 Checklist

- [ ] All 4 changes committed
- [ ] All tests passing
- [ ] Zed extension rebuilt and tested
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
- [ ] Feature branch merged to develop
- [ ] Tag release: `git tag v0.3.0-agent-ast`
- [ ] Push tags: `git push origin --tags`

---

## Next Steps (Phase 2)

Once Phase 1 complete:
- Begin Agent Backend scaffold
- Implement agent registry
- Create Mock executor
- Build REST API

**Do not start Phase 2 until Phase 1 is 100% complete and verified.**

---

**Status:** Ready to Execute  
**Duration:** 2 weeks (10 working days)  
**Risk Level:** Medium (AST changes always risky)  
**Mitigation:** Extreme discipline, atomic commits, thorough testing
