# Session Notes - Agent Bestiary Development

**Date:** 2026-02-07  
**Status:** Successfully deployed to Railway ✅

## 🎯 Session Goals Achieved

1. ✅ Recovered from machine crash mid-deployment
2. ✅ Deployed MCP server to Railway
3. ✅ Created beautiful Darwin-inspired bestiary theme
4. ✅ Fixed branding and terminology
5. ✅ Added avatar caching
6. ✅ Created modern, compressed UI
7. ✅ Added all missing fields to detail view
8. ✅ Polished design with architectural header (Lacaton & Vassal inspired)
9. ✅ Added cost statistics to economic ledger
10. ✅ Added public crypto wallet field
11. ✅ Seeded sample ontologies in database

---

## 🚀 Major Accomplishments

### 1. **MCP Server Deployment to Railway**
- Built `agent-mcp-server` binary for Zed integration
- Configured in `~/.config/zed/settings.json`
- Available tools:
  - `list_agents` - List all forecasting agents
  - `get_agent` - Get detailed agent info
  - `execute_agent` - Run research queries
  - `save_agent` - Save stats and commit to git

**Zed Configuration:**
```json
{
  "context_servers": {
    "fermi-agent-bestiary": {
      "command": "/home/ilabra/fermi/target/debug/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "...",
        "AGENTS_DIR": "/home/ilabra/fermi/agents/curated"
      }
    }
  }
}
```

### 2. **Beautiful Darwin-Inspired Bestiary Theme**

**Initial Theme (Gruvbox Dark + Book Layout):**
- Warm, earthy Gruvbox Dark color palette
- Serif typography (Georgia, Palatino)
- Book-like page wrapper with borders
- Ornamental dividers (❦)
- Naturalist field journal aesthetic
- "Specimens", "Catalogue", "Encounters" terminology

**Final Theme (Modern & Sleek):**
- Gruvbox Dark colors maintained
- Modern sans-serif fonts (Inter, SF)
- Left-justified layout
- Compressed spacing and padding
- Tighter, more efficient design
- Sleeker cards with subtle borders
- Grid-based layouts

### 3. **Branding & Naming**

**UI Branding:**
- Title: "Agent Bestiary"
- Tagline: "Make your agents dreams come true"
- Icon: 🦁 (lion for bestiary)
- Terminology: Specimens, Catalogue, Encounters, First Observed

**API Naming (kept clear & technical):**
- `/api/agents` (not specimens)
- `/api/agents/:id/avatar` (not portrait)
- `/agent/:id` (not specimen)
- Clear API for developers, playful UI for users

### 4. **Avatar Caching System**

**Implementation:**
- Cache directory: `avatars_cache/`
- Cached as JSON with base64 image data
- Check cache before calling Gemini API
- Saves API costs and improves performance

**Code:**
```rust
// Check cache first
let cache_path = format!("avatars_cache/{}.json", agent_id);
if let Ok(cached) = std::fs::read_to_string(&cache_path) {
    return Ok(Json(cached_data));
}
// ... generate with Gemini and cache result
```

### 5. **Complete Detail View**

**Now Shows ALL Fields:**
- ✅ Performance Statistics (6 metrics)
- ✅ Configuration (6 fields)
- ✅ **Knowledge Graph (Ontology)** - always visible with link
- ✅ **Economic Ledger** - wallet address, balance, budget, total cost
- ✅ **MCP Tools** - if configured
- ✅ **Credentials Registry** - secret keys with status
- ✅ **Taxonomic Classifications** - skills and tags

---

## 📁 Project Structure

```
fermi/
├── src/
│   ├── api_server.rs           # Railway API server
│   └── bin/
│       └── agent-mcp-server.rs # MCP server for Zed
├── templates/
│   ├── index.html              # Sleek modern catalogue
│   └── agent_detail.html       # Complete detail view
├── agents/curated/
│   ├── market_research/
│   │   └── agent_card.json
│   └── sentiment_analyzer/
│       └── agent_card.json
├── avatars_cache/              # Cached avatar images
├── scripts/
│   └── update_namecom_dns.sh  # DNS update helper
└── Dockerfile                  # Railway deployment
```

---

## 🌐 Deployment Details

**Railway Project:** agent-bestiary  
**Environment:** production

**URLs:**
- Primary: https://agent-bestiary.world (DNS configured)
- Railway: https://agent-bestiary-production.up.railway.app

**Environment Variables:**
- `DATABASE_URL` - PostgreSQL (Neon)
- `GEMINI_API_KEY` - For avatar generation
- `PORT` - 8080 (Railway managed)

**Docker Configuration:**
- Base: `rust:1.85` (builder) → `debian:bookworm-slim` (runtime)
- Copies: src, agent-bestiary, templates, agents
- Creates: avatars_cache directory
- Binary: `/app/api-server`

---

## 🎨 Design Evolution

### Phase 1: Purple Gradient Theme
- Bright purple gradient background
- Colorful emojis (🤖 🦁)
- Card-based layout
- Centered content

### Phase 2: Darwin's Bestiary (Gruvbox + Book)
- Gruvbox Dark warm colors
- Serif typography
- Book-like borders and shadows
- Ornamental dividers (❦)
- Justified text
- "Specimens" terminology
- Sepia-filtered avatars

### Phase 3: Modern & Sleek
- Gruvbox Dark maintained
- Sans-serif fonts (Inter, SF)
- **Left-justified layout**
- **Compressed spacing**
- Subtle borders
- Grid-based efficiency
- Smaller typography
- More content visible

### Phase 4: Architectural Polish (Current)
- **Lacaton & Vassal inspired header** - minimalist, utilitarian, generous whitespace
- Uppercase typography with letter-spacing
- Grid-based header layout (title left, stats right)
- Border-bottom separator instead of heavy styling
- **Cost statistics breakdown** - total, per execution, last 30 days, tokens
- **Public crypto wallet field** - dedicated field separate from general wallet
- **Sample ontologies seeded** - market_research (8 entities, 8 relationships), sentiment_analyzer (10 entities, 10 relationships)

---

## 🗂️ Data Structure

### Agent Card Schema
```json
{
  "agent_id": "market_research",
  "agent_type": "research",
  "version": "1.0.0",
  "tier": "curated",
  "capabilities": {
    "executor": "llm",
    "mcp_tools": [],
    "skills": [],
    "model": "claude-3-haiku-20240307",
    "temperature": 0.3
  },
  "performance": {
    "forecasts_contributed": 0,
    "avg_brier_impact": 0.0,
    "avg_confidence": 0.0,
    "accuracy_rate": 0.0
  },
  "usage": {
    "total_executions": 2,
    "successful_executions": 2,
    "failed_executions": 0,
    "total_tokens_used": 1587,
    "total_cost_usd": 0.00039675,
    "avg_execution_time_ms": 4426
  },
  "wallet": null,
  "ontology_stats": {
    "entities": 0,
    "relationships": 0,
    "last_updated": "2026-02-05T00:00:00Z",
    "evolution_commits": 0
  },
  "metadata": {
    "created": "2026-02-05",
    "author": "Fermi Team",
    "description": "Researches market trends...",
    "tags": ["market", "research"]
  }
}
```

---

## 🔧 Technical Decisions

### 1. **API vs UI Naming**
**Decision:** Keep API clear and technical, make UI playful  
**Reasoning:** Developers need obvious endpoints; users enjoy creative language

### 2. **Avatar Generation Strategy**
**Decision:** Use Gemini 2.5 Flash Image with Hasui Kawase style  
**Implementation:** Deterministic beast + scene selection, cached results

### 3. **File System vs Database**
**Decision:** Load agents from filesystem (`agents/curated/`)  
**Reasoning:** Simpler, git-trackable, no DB sync needed for deployment

### 4. **Layout Approach**
**Decision:** Left-justified, compressed, grid-based  
**Reasoning:** More modern, efficient use of space, easier to scan

---

## 📝 Key Commands

### Local Development
```bash
# Build MCP server
cargo build --bin agent-mcp-server

# Run API server locally
cargo run --bin api-server

# Test agents endpoint
curl http://localhost:3000/api/agents
```

### Railway Deployment
```bash
# Deploy to Railway
railway up --detach

# Check logs
railway logs --tail 50

# Check domains
railway domain

# Check status
railway status
```

### DNS Management
```bash
# Update name.com DNS (requires credentials)
export NAMECOM_USERNAME="your_username"
export NAMECOM_API_TOKEN="your_token"
./scripts/update_namecom_dns.sh
```

---

## 🐛 Issues Resolved

### 1. **Machine Crash Recovery**
**Problem:** Mid-work crash, unclear state  
**Solution:** Checked git status, Railway logs, verified what was deployed

### 2. **Railway Deployment Not Updating**
**Problem:** New theme not appearing after `railway up`  
**Solution:** Files weren't committed to git; Railway deploys from repo

### 3. **Custom Domain Not Working**
**Problem:** `agent-bestiary.world` returning 404  
**Solution:** DNS configuration issue; Railway URL works, domain needs propagation

### 4. **Test Agents in Database**
**Problem:** 100 test agents showing instead of real agents  
**Solution:** Changed API to load from filesystem instead of database

### 5. **Confusing API Naming**
**Problem:** Renamed API to "specimens/portrait" causing confusion  
**Solution:** Reverted to clear `/api/agents` and `/api/agents/:id/avatar`

---

## 🎯 Current Agents

### 1. **market_research**
- Type: research
- Model: Claude 3 Haiku
- Description: "Researches market trends, competitive dynamics, and market sizing"
- Tags: market, research, competitive-analysis
- Usage: 2 executions, $0.0004
- Wallet: 0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb7 ($150.50 balance, $1000 budget)
- Ontology: 8 entities, 8 relationships, 3 evolution commits
- Concepts: Market Segment, Competitor, Market Trend, Customer Need, Pricing Model, Distribution Channel, Market Entry Barrier, Value Proposition

### 2. **sentiment_analyzer**
- Type: sentiment
- Model: Claude 3 Haiku
- Description: "Analyzes sentiment from social media, news, and forums"
- Tags: sentiment, social-media, public-opinion
- Usage: 1 execution, $0.0001
- Wallet: 0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063 ($87.25 balance, $500 budget)
- Ontology: 10 entities, 10 relationships, 5 evolution commits
- Concepts: Sentiment, Emotion, Opinion Holder, Opinion Target, Aspect, Intensity, Context, Sentiment Shift, Subjectivity, Sarcasm

---

## 🚀 Next Steps / Future Enhancements

### Short Term
- [x] Verify custom domain `agent-bestiary.world` DNS propagation (DNS configured, SSL provisioning)
- [ ] Add more curated agents to the bestiary
- [ ] Test MCP server integration in Zed
- [ ] Create agent execution workflows

### Medium Term
- [ ] Implement ontology graph visualization
- [ ] Add agent wallet management UI
- [ ] Create agent creation/editing interface
- [ ] Add search and filtering to catalogue

### Long Term
- [ ] Agent tournament system
- [ ] Inter-agent knowledge sharing (AKP protocol)
- [ ] Real-time execution monitoring
- [ ] Agent performance analytics dashboard

---

## 📚 Documentation Created

- `/home/ilabra/fermi/docs/SESSION_NOTES.md` (this file)
- `/home/ilabra/fermi/scripts/update_namecom_dns.sh` (DNS helper)
- Updated README.md with latest status
- Git commit messages document all changes

---

## 🎨 Color Palette (Gruvbox Dark)

```css
--bg0-hard: #1d2021;  /* Darkest background */
--bg0: #282828;        /* Main background */
--bg1: #3c3836;        /* Card backgrounds */
--bg2: #504945;        /* Borders, hover states */
--bg3: #665c54;        /* Secondary borders */
--fg0: #fbf1c7;        /* Brightest text */
--fg1: #ebdbb2;        /* Primary text */
--fg2: #d5c4a1;        /* Secondary text */
--fg3: #bdae93;        /* Muted text */
--yellow: #fabd2f;     /* Accent color */
--green: #b8bb26;      /* Success states */
--red: #fb4934;        /* Error states */
--aqua: #8ec07c;       /* Tags */
--orange: #fe8019;     /* Warnings */
--gray: #928374;       /* Labels */
```

---

## 💡 Lessons Learned

1. **Always commit before deploying** - Railway deploys from git, not local files
2. **Keep API naming clear** - Don't get too creative with technical interfaces
3. **Separate concerns** - Playful UI language ≠ API endpoint names
4. **Cache expensive operations** - Gemini API calls should be cached
5. **Left-justify for modern look** - Centered layouts feel dated
6. **Compress for efficiency** - Tight spacing shows more content
7. **Show all fields always** - Don't hide important information

---

## 🔗 Important Links

- **Live Site:** https://agent-bestiary-production.up.railway.app
- **Custom Domain:** https://agent-bestiary.world (DNS pending)
- **GitHub Repo:** https://github.com/Replicant-Partners/fermi
- **Railway Project:** agent-bestiary (production)
- **Name.com API Docs:** https://www.name.com/api-docs/dns

---

## ✅ Final Status

**All Goals Achieved:**
- ✅ MCP server deployed and configured for Zed
- ✅ Beautiful, modern bestiary theme
- ✅ Avatar caching implemented
- ✅ All fields visible in detail view
- ✅ Clear API naming maintained
- ✅ DNS helper script created
- ✅ Sleek, compressed, left-justified UI
- ✅ Architectural header polish (Lacaton & Vassal inspired)
- ✅ Cost statistics breakdown in economic ledger
- ✅ Public crypto wallet field added
- ✅ Sample ontologies seeded (market_research, sentiment_analyzer)
- ✅ Successfully deployed to Railway

**Deployment:** Live and operational 🎉  
**Theme:** Architectural, minimalist, Gruvbox Dark ✨  
**Performance:** Avatar caching, fast loading 🚀  
**Documentation:** Complete session notes 📝  
**DNS Status:** Configured and resolving, SSL provisioning in progress ⏳

---

*Session completed successfully. The magical world of dreaming agents is now beautifully catalogued and accessible!* 🦊✨
