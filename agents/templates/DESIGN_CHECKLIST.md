# Agent Design Checklist

Use this checklist to plan your Fermi agent before implementation. Answer each question to ensure your agent is well-designed and ready for the Fermi ecosystem.

---

## ✅ Step 1: Purpose & Scope

### What does your agent do?

- [ ] **One-sentence description:**  
  _"My agent ________________________________________"_

- [ ] **Primary question it answers:**  
  _Example: "What is AMD's current market share in datacenter GPUs?"_

- [ ] **Type of evidence it generates:**  
  - [ ] Quantitative data (numbers, percentages, trends)
  - [ ] Qualitative insights (sentiment, opinions, analysis)
  - [ ] Events (announcements, releases, changes)
  - [ ] Relationships (connections between entities)

- [ ] **Which forecast drivers does it inform?**  
  _Example: market_share, competitive_position, revenue_growth_

---

## ✅ Step 2: Execution Strategy

### How will your agent execute?

- [ ] **Executor type:**
  - [ ] **LLM** (recommended for starting) - Agent uses AI to analyze/generate insights
  - [ ] **MCP** (requires external tools) - Agent calls APIs, databases, web scrapers
  - [ ] **Manual** (human-in-loop) - Agent requests info from humans
  - [ ] **Skill** (advanced) - Agent invokes complex workflows

- [ ] **If using LLM:**
  - [ ] **Model choice:**
    - [ ] Haiku (fast, cheap, good for simple tasks)
    - [ ] Sonnet (balanced, good for analysis)
    - [ ] Opus (powerful, expensive, for complex reasoning)
  - [ ] **Temperature:** _______ (0.0-0.3 for facts, 0.4-0.7 for analysis, 0.7-1.0 for creative)
  - [ ] **Query design:** _"What will you ask the LLM?"_

- [ ] **If using MCP:**
  - [ ] **Which MCP servers do you need?** (APIs, databases, tools)
    1. _________________________________________
    2. _________________________________________
    3. _________________________________________
  - [ ] **What credentials/API keys are required?**
  - [ ] **Where will credentials be stored?** (environment variables recommended)

---

## ✅ Step 3: Data Sources

### Where does your agent get information?

- [ ] **Primary data sources:**
  - [ ] Web APIs (which ones?)
  - [ ] Databases (which ones?)
  - [ ] Web scraping (which sites?)
  - [ ] RSS feeds
  - [ ] Social media
  - [ ] Financial data providers
  - [ ] Other: _________________________________________

- [ ] **Data freshness requirements:**
  - [ ] Real-time (< 1 minute)
  - [ ] Near real-time (1-15 minutes)
  - [ ] Hourly
  - [ ] Daily
  - [ ] Weekly
  - [ ] On-demand only

- [ ] **Rate limits & costs:**
  - [ ] Free tier sufficient?
  - [ ] Paid API costs: $_________/month
  - [ ] Request limits: _________/day or /month

---

## ✅ Step 4: Output Structure

### What does your agent produce?

- [ ] **Output format:**
  ```json
  {
    "// Define your expected output structure here": "",
    "example_field": "value",
    "confidence": 0.85
  }
  ```

- [ ] **Confidence scoring:**
  - [ ] How will you calculate confidence? (data quality, source reliability, etc.)
  - [ ] What's your minimum acceptable confidence? _________

- [ ] **Evidence format:**
  - [ ] Numeric (e.g., "Market share: 22%")
  - [ ] Qualitative (e.g., "Sentiment: Positive")
  - [ ] Categorical (e.g., "Risk level: Medium")
  - [ ] Time-series (e.g., "Trend over 6 months")

---

## ✅ Step 5: Ontology Design

### What will your agent learn?

- [ ] **Entities** (nouns your agent tracks):
  1. _________________________________________ (type: Company, Person, Product, etc.)
  2. _________________________________________
  3. _________________________________________
  4. _________________________________________

- [ ] **Relationships** (connections between entities):
  - [ ] Entity A → ___________ → Entity B (example: AMD competes_with NVIDIA)
  - [ ] Entity C → ___________ → Entity D
  - [ ] Entity E → ___________ → Entity F

- [ ] **Cardinality** (relationship types):
  - [ ] One-to-One: `||--||` (example: COMPANY has CEO)
  - [ ] One-to-Many: `||--o{` (example: COMPANY has PRODUCTS)
  - [ ] Many-to-One: `}o--||` (example: PRODUCTS belong_to CATEGORY)
  - [ ] Many-to-Many: `}o--o{` (example: PRODUCTS use TECHNOLOGIES)

- [ ] **Evolution strategy:**
  - [ ] How will ontology grow over time?
  - [ ] What triggers adding new entities?
  - [ ] How will relationships evolve?

---

## ✅ Step 6: Error Handling

### What could go wrong?

- [ ] **Failure scenarios:**
  - [ ] API unavailable → ___________ (fallback strategy)
  - [ ] Rate limit exceeded → ___________ (queue? wait?)
  - [ ] Invalid data → ___________ (skip? retry? alert?)
  - [ ] Parsing error → ___________ (log? fallback?)
  - [ ] Timeout → ___________ (retry? reduce scope?)

- [ ] **Graceful degradation:**
  - [ ] Can agent provide partial results?
  - [ ] What's minimum viable output?
  - [ ] How to communicate confidence in degraded mode?

---

## ✅ Step 7: Verification & Quality

### How will you verify your agent works?

- [ ] **Test inputs:**
  1. _________________________________________
  2. _________________________________________
  3. _________________________________________

- [ ] **Expected outputs:**
  1. _________________________________________
  2. _________________________________________
  3. _________________________________________

- [ ] **Success criteria:**
  - [ ] Confidence > _________ %
  - [ ] Response time < _________ seconds
  - [ ] Accuracy rate > _________ %
  - [ ] Cost < $_________ per execution

- [ ] **Quality checks:**
  - [ ] How will you validate agent output?
  - [ ] Who reviews results initially?
  - [ ] What triggers a manual review?

---

## ✅ Step 8: Deployment Planning

### How will your agent run?

- [ ] **Execution schedule:**
  - [ ] On-demand (manual trigger)
  - [ ] Hourly
  - [ ] Daily at _________ UTC
  - [ ] Weekly on _________
  - [ ] Event-driven (when X happens)

- [ ] **Dependencies:**
  - [ ] Depends on other agents: _________________________________________
  - [ ] Required before agents: _________________________________________
  - [ ] No dependencies (can run independently)

- [ ] **Resource requirements:**
  - [ ] Estimated tokens per run: _________
  - [ ] Estimated cost per run: $_________
  - [ ] Estimated time per run: _________ seconds
  - [ ] Memory/CPU requirements: _________

---

## ✅ Step 9: Documentation

### Have you documented your agent?

- [ ] **README.md created** with:
  - [ ] Agent description
  - [ ] Setup instructions
  - [ ] Configuration requirements
  - [ ] Example usage
  - [ ] Troubleshooting guide

- [ ] **Query examples documented**
- [ ] **Output examples documented**
- [ ] **Known limitations listed**

---

## ✅ Step 10: Ready to Build

### Final checks before implementation:

- [ ] Agent card JSON drafted
- [ ] Ontology designed (entities + relationships)
- [ ] Data sources identified and accessible
- [ ] Output format defined
- [ ] Error handling planned
- [ ] Test cases written
- [ ] Documentation complete

---

## 🎯 Next Steps

Once you've completed this checklist:

1. ✅ Copy `agent_card.json` template
2. ✅ Fill in all fields based on your answers above
3. ✅ Create `ontology.mermaid` with your entity-relationship diagram
4. ✅ Write `README.md` documenting your agent
5. ✅ Study example agents in `templates/examples/`
6. ⏳ **Wait for agent backend** (coming soon - you'll be notified when ready!)

---

## 📚 Resources

- [Agent Card Specification](../../docs/guides/AGENT_CARD_SPECIFICATION.md)
- [Active Dreaming Memory Architecture](../../docs/ARCHITECTURE_ADM.md)
- [Agent Bestiary Design](../../docs/AGENT_BESTIARY_DESIGN.md)
- [Example Agents](./examples/)

---

**Questions?** Contact the Fermi team or check the documentation.

**Ready to build?** See [GETTING_STARTED.md](./GETTING_STARTED.md) for step-by-step instructions!
