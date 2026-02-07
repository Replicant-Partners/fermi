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

## ✅ Step 5: Embedding Configuration

### How will your agent store and retrieve knowledge?

Fermi ADM uses embeddings to store episodic and semantic memory. You can choose the embedding provider that best fits your agent's needs.

- [ ] **Embedding provider:**
  - [ ] **Anthropic (Default)** - Voyage AI embeddings, optimized for retrieval
  - [ ] **OpenAI** - Widely tested, flexible dimensionality
  - [ ] **Mistral** - European data residency, open architecture
  - [ ] **Qwen** - Strong multilingual support, cost-effective

- [ ] **Model selection:**
  - [ ] Using default model for provider
  - [ ] Custom model: _________________________________________

- [ ] **Embedding dimensions:**
  - [ ] **1024 (REQUIRED - matches current PostgreSQL schema)** ✅
  - [ ] 1536 (requires schema migration)
  - [ ] 3072 (requires schema migration, OpenAI only)
  
  ⚠️ **Note**: The database schema currently uses 1024-dimensional vectors. All embedding models must output 1024 dimensions. OpenAI models can be configured to 1024d via the API. If you need different dimensions, see the [Embedding Migration Guide](../../docs/guides/EMBEDDING_MIGRATION.md).

- [ ] **Language considerations:**
  - [ ] English-only → Anthropic or OpenAI recommended
  - [ ] Multilingual → Consider Qwen
  - [ ] Chinese content → Qwen strongly recommended
  - [ ] Code-heavy → Consider Voyage-code-2 (Anthropic)

- [ ] **Cost considerations:**
  - [ ] Budget-friendly → Mistral or OpenAI text-embedding-3-small
  - [ ] Quality-focused → Anthropic voyage-large-2 or OpenAI text-embedding-3-large
  - [ ] Balanced → Anthropic voyage-2 (default)

- [ ] **Data residency requirements:**
  - [ ] No specific requirements → Any provider
  - [ ] European data residency → Mistral
  - [ ] Asian deployment → Qwen
  - [ ] US-based → Anthropic or OpenAI

### Configuration Example

```toml
[knowledge]
# Choose your embedding provider
embeddings_provider = "anthropic"  # or "openai", "mistral", "qwen"
embeddings_model = "voyage-2"      # provider-specific model
dimensions = 1024                  # must match model's output

# Provider-specific examples:
# Anthropic: voyage-2, voyage-large-2, voyage-code-2
# OpenAI: text-embedding-3-small, text-embedding-3-large
# Mistral: mistral-embed
# Qwen: text-embedding-v3, text-embedding-v2
```

### Important Notes

⚠️ **Migration Warning:** Once you choose an embedding provider for your agent, changing it later requires re-embedding all existing memories. Choose carefully at design time.

✅ **Best Practice:** Use the default (Anthropic voyage-2) unless you have specific requirements for language support, data residency, or cost optimization.

📚 **For detailed provider comparison:** See [Agent Cards - Embedding Configuration](../../docs/api/agent-cards.md#embedding-configuration)

---

## ✅ Step 6: Ontology Design

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

## ✅ Step 7: Error Handling

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

## ✅ Step 8: Verification & Quality

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

## ✅ Step 9: Deployment Planning

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

## ✅ Step 10: Documentation

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

## ✅ Step 11: Ready to Build

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
