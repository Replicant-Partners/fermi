# Prompt Engineering Guide for Agent Development

This guide provides proven prompts to help you generate high-quality agent designs, ontologies, and documentation using AI assistants (Claude, GPT-4, etc.).

## Table of Contents

1. [Agent Card Generation](#agent-card-generation)
2. [Ontology Design](#ontology-design)
3. [Documentation Writing](#documentation-writing)
4. [Example Query Generation](#example-query-generation)
5. [Error Handling Scenarios](#error-handling-scenarios)
6. [Performance Estimation](#performance-estimation)
7. [Iterative Refinement](#iterative-refinement)

---

## Agent Card Generation

### Prompt 1: Initial Agent Card from Concept

```
I'm designing a Fermi forecasting agent. Help me create an agent_card.json file.

**Agent Concept:** [Describe what your agent does in 2-3 sentences]

**Domain:** [e.g., market analysis, sentiment, risk assessment, technical specs]

**Data Sources:** [e.g., APIs, databases, LLM analysis, web scraping]

Please generate a complete agent_card.json following the Fermi specification with:
- Appropriate executor type (llm, mcp, manual, skill)
- Model selection and temperature tuning
- MCP server configurations (if needed)
- Realistic performance estimates
- Proper JSON structure with all required fields

Reference the template at: agents/templates/agent_card.json
```

**Example:**
```
I'm designing a Fermi forecasting agent. Help me create an agent_card.json file.

**Agent Concept:** Tracks cryptocurrency market sentiment by analyzing social media posts, news headlines, and trading volume. Provides sentiment scores and identifies emerging trends.

**Domain:** Sentiment analysis + market intelligence

**Data Sources:** Twitter API, Reddit API, CoinGecko API, news RSS feeds

Please generate a complete agent_card.json following the Fermi specification...
```

### Prompt 2: Refine Existing Agent Card

```
I have an agent card but need to improve it. Please review and suggest improvements.

**Current agent_card.json:**
[Paste your JSON]

**Issues I'm experiencing:**
- [e.g., Low confidence scores, slow execution, high costs]

**Goals:**
- [e.g., Improve accuracy to >85%, reduce response time to <3s]

Please suggest:
1. Better model selection (Haiku/Sonnet/Opus)
2. Temperature tuning
3. System prompt improvements
4. MCP server optimizations
5. Any missing fields or configurations
```

### Prompt 3: Convert User Story to Agent Card

```
Convert this user story into a Fermi agent card:

**User Story:** "As a [role], I want to [action] so that [benefit]"

Example: "As a product manager, I want to track competitor feature releases so that I can prioritize our roadmap effectively."

Please generate:
1. agent_name (lowercase_with_underscores)
2. agent_type (classification)
3. Appropriate executor configuration
4. Data source recommendations (APIs, databases, etc.)
5. Complete agent_card.json

Make it production-ready with realistic configurations.
```

---

## Ontology Design

### Prompt 4: Generate Entity-Relationship Diagram

```
I'm designing the ontology for a Fermi agent. Generate a Mermaid ER diagram.

**Agent Purpose:** [What does your agent do?]

**Key Concepts:** [List 5-10 important nouns your agent tracks]
Examples: Companies, Products, Technologies, Market Segments, Vulnerabilities, etc.

**Relationships:** [Describe how concepts connect]
Example: "Companies produce Products, Products use Technologies"

Please generate:
1. Mermaid erDiagram syntax
2. 5-15 core entities (not too many!)
3. Proper cardinality (||--||, ||--o{, }o--||, }o--o{)
4. Appropriate attributes for each entity (id, name, timestamps, scores, etc.)
5. Clear relationship labels

Keep it simple and focused. Avoid over-engineering.

Reference: https://mermaid.js.org/syntax/entityRelationshipDiagram.html
```

**Example:**
```
I'm designing the ontology for a Fermi agent. Generate a Mermaid ER diagram.

**Agent Purpose:** Tracks technology adoption trends across enterprises

**Key Concepts:** 
- Companies (enterprises adopting technology)
- Technologies (software, frameworks, tools)
- Adoption Events (when companies start using tech)
- Use Cases (how tech is being used)
- Decision Makers (people influencing adoption)

**Relationships:**
- Companies adopt Technologies through Adoption Events
- Technologies are used for Use Cases
- Decision Makers work at Companies and influence Adoption Events

Please generate a complete Mermaid erDiagram...
```

### Prompt 5: Validate and Improve Ontology

```
Review my ontology design and suggest improvements.

**Current ontology.mermaid:**
[Paste your Mermaid code]

**Questions:**
1. Are there redundant relationships?
2. Is cardinality correct?
3. Are entity attributes appropriate?
4. Is it over-engineered or too simple?
5. Will it scale as the agent learns?
6. Are there missing entities or relationships?

Please provide:
- Specific improvements with justification
- Updated Mermaid code
- Best practices I should follow
```

### Prompt 6: Expand Ontology for New Capabilities

```
I need to expand my agent's ontology to support new capabilities.

**Current ontology:** [Brief description or paste Mermaid]

**New capability:** [What new thing should the agent track?]

**New concepts:** [List new entities/relationships needed]

Please:
1. Identify new entities to add
2. Define relationships to existing entities
3. Ensure normalization (avoid redundancy)
4. Update Mermaid diagram
5. Explain the design decisions

Keep backward compatibility where possible.
```

---

## Documentation Writing

### Prompt 7: Generate Agent README

```
Write a comprehensive README.md for my Fermi agent.

**Agent Card:** [Paste key fields: name, type, executor, capabilities]

**Ontology:** [Brief description of entities]

**Example Queries:** [2-3 questions the agent answers]

Please generate a README with these sections:
1. Overview (2-3 sentences)
2. Key Capabilities (bullet list)
3. Data Sources (if MCP: list APIs with endpoints)
4. Ontology Structure (entity summary)
5. Usage Examples (3 detailed examples with inputs/outputs)
6. Performance Metrics (based on agent card)
7. Configuration (environment variables, API keys)
8. Error Handling (failure modes and fallbacks)
9. Best Practices (for users of this agent)
10. Limitations (honest about what it can't do)
11. Troubleshooting (common issues and solutions)

Target length: 400-800 lines
Tone: Professional but accessible
Include: Code blocks, examples, realistic outputs
```

### Prompt 8: Write Usage Examples

```
Generate 5 realistic usage examples for my agent.

**Agent:** [Name and brief description]

**Capabilities:** [What it can do]

For each example, provide:
1. **User Query:** Natural language question
2. **Agent Process:** Step-by-step what agent does
3. **Output:** Realistic JSON response with:
   - Data/insights
   - Confidence score (0.0-1.0)
   - Evidence/justification
   - Sources (if applicable)

Make outputs realistic - include:
- Appropriate confidence levels (not always 0.95!)
- Real-world data patterns
- Honest limitations (e.g., "Data unavailable for...")
- Proper timestamps and units

Examples should range from simple to complex queries.
```

### Prompt 9: Document Error Scenarios

```
Help me document error handling for my agent.

**Agent:** [Name and executor type]

**Data Sources:** [APIs, databases, etc.]

Please generate a "Error Handling" section covering:

1. **Common Failures:**
   - API unavailable
   - Rate limit exceeded
   - Invalid/malformed data
   - Timeout
   - Authentication failure
   - Parse error

2. **For Each Failure:**
   - **Scenario:** When does this happen?
   - **Fallback:** What does agent do?
   - **Output:** How is error communicated?
   - **User Action:** What should user do?
   - **Confidence Impact:** How does confidence score change?

3. **Graceful Degradation Examples:**
   Show realistic degraded outputs with lower confidence

Make it practical and user-friendly.
```

---

## Example Query Generation

### Prompt 10: Generate Test Queries

```
Generate 10 test queries for my Fermi agent.

**Agent:** [Name and description]

**Capabilities:** [What it can do]

**Data Sources:** [Where it gets information]

Please provide:
1. **5 Expected Success Cases:**
   - Queries agent should handle well
   - Include simple, medium, complex examples
   - Specify expected confidence levels (>0.7)

2. **3 Edge Cases:**
   - Queries at limits of agent capabilities
   - Expected confidence: 0.5-0.7
   - Should still provide useful output

3. **2 Failure Cases:**
   - Queries agent cannot answer
   - Should gracefully decline or return low confidence (<0.5)
   - Clear explanation of why

For each query:
- Natural language question
- Expected output structure
- Confidence score estimate
- Notes on why this tests specific capability
```

### Prompt 11: Create Forecast Scenarios

```
Create realistic forecast scenarios for testing my agent.

**Agent Domain:** [e.g., market analysis, risk assessment]

**Forecast Horizon:** [e.g., 3 months, 1 year, 5 years]

Generate 5 scenarios with:

1. **Scenario Description:** Context and assumptions
2. **Key Questions:** What needs forecasting?
3. **Required Evidence:** What data agent needs to gather
4. **Expected Insights:** What agent should discover
5. **Uncertainty Factors:** Sources of uncertainty
6. **Confidence Range:** Expected confidence levels
7. **Decision Impact:** How forecast informs decisions

Make scenarios realistic and actionable.
```

---

## Error Handling Scenarios

### Prompt 12: Design Fallback Strategies

```
Design fallback strategies for my agent's error scenarios.

**Agent:** [Name and type]

**Critical Dependencies:** [APIs, databases, services it relies on]

For each dependency, please design:

1. **Primary Strategy:** Normal operation
2. **Fallback Level 1:** What to try if primary fails
3. **Fallback Level 2:** If Level 1 also fails
4. **Ultimate Fallback:** Last resort (manual review, cached data, etc.)

For each level, specify:
- How to detect failure
- When to trigger fallback
- Expected output quality
- Confidence score adjustment
- User notification message

Make it production-ready with realistic retry logic and timeouts.
```

---

## Performance Estimation

### Prompt 13: Estimate Performance Metrics

```
Help me estimate realistic performance metrics for my agent.

**Agent Configuration:**
- Executor: [llm/mcp/manual/skill]
- Model: [if LLM: Haiku/Sonnet/Opus]
- Data Sources: [List APIs, databases]
- Complexity: [Simple/Medium/Complex]

Please estimate:

1. **Accuracy Rate:**
   - Expected accuracy (0.0-1.0)
   - Justification based on task complexity
   - Factors affecting accuracy

2. **Average Confidence:**
   - Typical confidence score
   - Confidence distribution (high/medium/low)

3. **Execution Time:**
   - Average response time (ms)
   - Breakdown by component (LLM call, API calls, processing)
   - Best/worst case scenarios

4. **Cost per Execution:**
   - LLM costs (if applicable)
   - API costs (if applicable)
   - Total cost estimate

5. **Throughput:**
   - Queries per minute/hour
   - Rate limiting factors
   - Scalability considerations

Base estimates on similar systems and realistic assumptions.
```

### Prompt 14: Design Calibration Plan

```
Design a calibration plan for my agent's confidence scores.

**Agent:** [Name and type]

**Current State:** [Initial deployment / needs recalibration]

Please provide:

1. **Calibration Methodology:**
   - How to collect ground truth data
   - Sample size needed for calibration
   - Calibration frequency (weekly/monthly)

2. **Evaluation Metrics:**
   - Accuracy vs. predicted confidence
   - Calibration curve analysis
   - Over/under-confidence detection

3. **Adjustment Strategy:**
   - When to adjust confidence thresholds
   - How to tune confidence calculation
   - Version control for calibration updates

4. **Monitoring:**
   - Ongoing metrics to track
   - Drift detection
   - Recalibration triggers

Make it practical for production deployment.
```

---

## Iterative Refinement

### Prompt 15: Improve Agent Based on Feedback

```
Improve my agent based on real-world feedback.

**Agent:** [Name]

**Current Performance:**
- Accuracy: [X%]
- Avg Confidence: [X]
- Execution Time: [X ms]
- User Satisfaction: [Low/Medium/High]

**User Feedback:**
[List specific complaints or issues]
Examples:
- "Responses too slow"
- "Low confidence on simple queries"
- "Missing important data sources"
- "Errors on edge cases"

Please suggest:

1. **Quick Wins:** Changes that immediately improve performance
2. **Configuration Tuning:** Temperature, model, thresholds
3. **Data Source Additions:** New APIs or data to integrate
4. **Prompt Engineering:** Better system prompts
5. **Ontology Evolution:** Entities/relationships to add
6. **Documentation Updates:** Clarifications needed

Prioritize by impact vs. effort.
```

### Prompt 16: Compare Agent Approaches

```
Compare different approaches for my agent design.

**Agent Goal:** [What it should accomplish]

**Approach A:** [Current design]
- Executor: [Type]
- Model: [If LLM]
- Data Sources: [List]
- Pros: [Advantages]
- Cons: [Disadvantages]

**Approach B:** [Alternative design]
- Executor: [Type]
- Model: [If LLM]
- Data Sources: [List]
- Pros: [Advantages]
- Cons: [Disadvantages]

Please compare on:
1. **Accuracy:** Which will be more accurate?
2. **Speed:** Response time comparison
3. **Cost:** Operational cost per query
4. **Complexity:** Implementation and maintenance effort
5. **Scalability:** Which scales better?
6. **Reliability:** Failure modes and resilience
7. **User Experience:** Which provides better UX?

Recommend the better approach with justification.
```

---

## Advanced Prompts

### Prompt 17: Design Multi-Agent System

```
Design a multi-agent system for a complex forecasting task.

**Forecast Question:** [High-level question requiring multiple types of evidence]

Example: "What will AMD's datacenter GPU market share be in 2027?"

Please design:

1. **Agent Decomposition:**
   - List 3-5 specialized agents needed
   - What each agent is responsible for
   - Why specialization is beneficial

2. **Agent Interactions:**
   - How agents share information
   - Dependencies between agents
   - Execution order or parallelization

3. **Evidence Aggregation:**
   - How to combine outputs from multiple agents
   - Confidence propagation strategy
   - Conflict resolution (if agents disagree)

4. **System-Level Ontology:**
   - Shared entities across agents
   - Integration points
   - Ontology evolution strategy

Provide agent card stubs for each agent in the system.
```

### Prompt 18: Generate System Prompt for Agent

```
Generate an optimized system prompt for my agent's LLM executor.

**Agent:** [Name and purpose]

**Task:** [Specific task agent performs]

**Output Format:** [JSON structure, text, classification, etc.]

**Key Requirements:**
- Evidence-based reasoning
- Confidence scoring
- Cite sources
- Handle uncertainty gracefully
- [Other requirements]

Please generate a system prompt that:
1. Clearly defines agent role and capabilities
2. Specifies output format requirements
3. Emphasizes evidence-based reasoning
4. Includes confidence scoring guidelines
5. Defines tone and style (objective, analytical)
6. Provides examples of good outputs
7. Sets boundaries (what agent should NOT do)

Length: 150-300 words
Tone: Professional, directive, clear
```

### Prompt 19: Ontology Migration Plan

```
I need to evolve my agent's ontology. Design a migration plan.

**Current Ontology:** [Summary or paste Mermaid]

**Current Stats:**
- Entity count: [X]
- Relationship count: [Y]
- Data volume: [Z episodic memories]

**Proposed Changes:**
[List changes: new entities, new relationships, renamed fields, etc.]

**Constraints:**
- Must maintain backward compatibility for existing queries
- Minimize data migration effort
- No downtime

Please provide:

1. **Migration Strategy:**
   - Step-by-step migration plan
   - Data transformation requirements
   - Rollback plan if issues occur

2. **Versioning:**
   - Version numbering scheme
   - Changelog for ontology changes
   - Git commit strategy

3. **Testing:**
   - Validation tests for migrated data
   - Regression tests for existing functionality
   - Performance benchmarks

4. **Updated Mermaid Diagram:**
   - New ontology structure
   - Clearly marked changes

Make it production-safe with minimal risk.
```

---

## Tips for Better Prompts

### 1. Provide Context

❌ **Bad:** "Generate an agent card"

✅ **Good:** "Generate an agent card for a market analysis agent that tracks AMD's datacenter GPU market share using Yahoo Finance and SEC filings APIs."

### 2. Specify Output Format

❌ **Bad:** "Help with my ontology"

✅ **Good:** "Generate a Mermaid erDiagram with 8-12 entities showing relationships between companies, products, and market segments."

### 3. Include Examples

❌ **Bad:** "Create test queries"

✅ **Good:** "Create 5 test queries like: 'What is AMD's Q4 2024 datacenter revenue?' Include expected outputs with confidence scores."

### 4. Set Constraints

❌ **Bad:** "Design an agent"

✅ **Good:** "Design an agent with: (1) LLM executor, (2) <$0.01 per query cost, (3) <3s response time, (4) >85% accuracy."

### 5. Request Justification

❌ **Bad:** "Pick a model for my agent"

✅ **Good:** "Recommend Haiku, Sonnet, or Opus for sentiment analysis. Justify based on speed, cost, and accuracy trade-offs."

### 6. Iterate Incrementally

Start with high-level design, then drill down:
1. First prompt: "Design agent concept"
2. Second prompt: "Refine executor configuration"
3. Third prompt: "Optimize for cost and speed"
4. Fourth prompt: "Add error handling"

### 7. Reference Documentation

Always point to relevant docs:
- "Following the Fermi agent card specification at..."
- "Using Mermaid ER syntax from..."
- "Based on examples in agents/templates/examples/..."

### 8. Request Multiple Options

"Provide 3 different approaches: (A) LLM-only, (B) MCP-heavy, (C) Hybrid. Compare pros/cons."

---

## Using These Prompts

### In Claude/ChatGPT

1. Copy the prompt template
2. Fill in the bracketed placeholders `[like this]`
3. Paste into chat interface
4. Review and iterate on the output
5. Copy results into your agent files

### In VS Code with Copilot

1. Create the target file (e.g., `agent_card.json`)
2. Add a comment with the prompt at the top
3. Let Copilot generate the content
4. Review and refine

### In Cursor/Windsurf

1. Use the prompt in the chat panel
2. Reference your existing files with `@filename`
3. Ask AI to directly edit files
4. Review diffs before accepting

### Command Line with Claude API

```bash
echo "Your prompt here" | claude --model sonnet --output agent_card.json
```

---

## Common Issues and Solutions

### Issue: Output Too Generic

**Solution:** Add specific examples and constraints
```
❌ "Generate an agent"
✅ "Generate an agent that tracks CVE-2024-* vulnerabilities affecting Node.js dependencies, using NVD API, with <2s response time"
```

### Issue: JSON Syntax Errors

**Solution:** Explicitly request validation
```
"Generate valid JSON following this schema: [paste schema]. Validate syntax before responding."
```

### Issue: Unrealistic Performance Estimates

**Solution:** Provide benchmarks
```
"Estimate performance metrics. For reference: Haiku costs ~$0.001/query, typical API calls take 500ms, LLM analysis takes 1-2s."
```

### Issue: Over-Engineered Ontology

**Solution:** Set entity limits
```
"Design ontology with 5-10 core entities (not more!). Keep it simple. I can expand later."
```

### Issue: Missing Error Handling

**Solution:** Explicitly ask for failure scenarios
```
"Include error handling for: API timeout, rate limit, invalid data, network failure. Provide fallback strategies."
```

---

## Prompt Templates Library

Save your best prompts in a personal library:

```
~/fermi-prompts/
├── agent_card_generation.md
├── ontology_design.md
├── documentation.md
├── testing.md
└── refinement.md
```

Build a collection of prompts that work well for your workflow.

---

## Additional Resources

- **Claude Prompt Engineering:** https://docs.anthropic.com/claude/docs/prompt-engineering
- **GPT Best Practices:** https://platform.openai.com/docs/guides/prompt-engineering
- **Mermaid Syntax:** https://mermaid.js.org/syntax/entityRelationshipDiagram.html
- **JSON Schema:** https://json-schema.org/understanding-json-schema/

---

**Pro Tip:** Save successful prompts and their outputs. Build your own prompt library customized to your agent development workflow.

**Last Updated:** 2026-02-07
