# Agent Research Quality — Making Automated Research Actually Useful

**Date:** 2026-03-09
**Status:** Design proposal
**Priority:** Critical — this is the core value proposition
**Context:** Infrastructure works (SSE, ABW, decomposition). But agents frequently apologize, return generic content, or miss the domain entirely. Without quality research, the system creates noise, not signal.

---

## The Problem

Today's agent assignment workflow:

1. Fermi decomposes → drivers appear
2. User clicks "+ Assign Agent" on a driver
3. Fermi suggests an agent (often wrong — suggests market_research for NBA questions)
4. User picks agent, types a query (or accepts the default)
5. Agent runs 30-60 seconds
6. Result comes back: "I don't have access to real-time tools..." or generic analysis

**Why this fails:**
- **Wrong agent for the domain.** The skill/tag matcher is superficial. It matches "market" in driver names to market_research, even when the driver is about NBA betting markets.
- **Bad default queries.** The auto-generated query is "Research evidence for the 'X' driver in the forecast: 'Y'" — too generic. The agent doesn't know what specific data points would move the probability.
- **Agents apologize instead of analyzing.** Despite the CARDINAL RULES preamble, agents still say "I don't have access to" or "I need to know the specific opponent." They've been trained to be cautious.
- **No feedback loop.** There's no way to rate evidence quality, no learning from which evidence actually improved forecasts, no way to "re-research with better query."
- **Manual everything.** The user has to click each driver, pick each agent, formulate each query. For 5 drivers, that's 5 rounds of manual work before any research happens.

---

## Design Principles

1. **Auto-assign on decomposition.** When Fermi creates drivers, the best agent should be assigned to each driver automatically. The user reviews and overrides, not initiates.

2. **Query formulation is the product.** The difference between "Research market conditions" and "What is the current Fed Funds rate, what are market expectations for the next 3 FOMC meetings, and how do rate-sensitive tech stocks typically perform in tightening vs easing cycles? Provide specific data points with dates." — that difference IS the value of the system.

3. **Evidence must be specific and quantitative.** "The market is growing" is worthless. "The global satellite direct-to-device market grew 340% YoY to $1.2B in 2025 (Northern Sky Research, March 2025)" is valuable. Agents must be prompted to produce the latter.

4. **Domain routing > keyword matching.** NBA questions go to nba_analyst, not market_research. Biotech questions go to biotech_analyst, not entity_investigator. The routing must be semantic, not string matching.

5. **Scheduled refresh keeps forecasts alive.** A forecast isn't a snapshot — it's a living document. Evidence goes stale. Agents should re-research on schedule, detecting when new information could change the probability.

---

## Architecture: The Research Orchestrator

Currently, agent assignment is a dumb pipeline:

```
User clicks → Fermi recommends → User confirms → Agent fires → Evidence returns
```

The proposed architecture introduces a **Research Orchestrator** layer:

```
Fermi decomposes → Orchestrator auto-assigns best agent per driver
                 → Orchestrator formulates domain-specific query per driver
                 → Orchestrator fires all agents in parallel
                 → Evidence streams in via SSE
                 → Orchestrator evaluates evidence quality
                 → Orchestrator suggests follow-up research if gaps found
                 → User reviews, adjusts, re-runs as needed
```

The Orchestrator is not a new LLM call — it's deterministic logic in the console that uses agent metadata (skills, tags, domain_knowledge) to make routing and query decisions.

---

## Component 1: Smart Agent Routing

### Current: Keyword Matching (Broken)

```rust
// Current code in cockpit.rs — matches single keywords against driver names
for skill in &card.capabilities.skills {
    let skill_words: Vec<&str> = skill.split('-').collect();
    for word in &skill_words {
        if word.len() > 2 && search_text.contains(word) {
            score += 2;
        }
    }
}
```

This produces: "nba_analyst" for a driver called "market_sentiment" because "market" appears in the NBA agent's description.

### Proposed: Domain-First Routing

```
Step 1: Detect forecast domain from question text
  "Will the Lakers win?" → sports_nba
  "Will ASTS hit 200M?"  → finance_space
  "FDA approval for X?"  → biotech_pharma

Step 2: Route ALL drivers to the domain-specific agent first
  sports_nba → nba_analyst for every driver
  finance    → market_research + macro_forecaster
  biotech    → biotech_analyst

Step 3: For drivers that need cross-domain expertise, add a second agent
  "public_sentiment" driver in an NBA forecast → sentiment_analyzer (secondary)
  "regulatory_risk" driver in a finance forecast → entity_investigator (secondary)

Step 4: For generic drivers with no clear domain match, use the general agents
  "strength_factor" → macro_forecaster (broad analytical capability)
```

### Implementation

```rust
struct AgentAssignment {
    agent_id: String,
    query: String,           // Domain-specific, detailed query
    priority: AssignmentPriority, // Primary (domain expert) or Secondary (cross-domain)
    schedule: Schedule,      // Once, Daily, Weekly
}

enum AssignmentPriority {
    Primary,    // Domain expert for this driver
    Secondary,  // Cross-domain supplement
    Monitoring, // Scheduled re-check
}

fn auto_assign_agents(
    question: &str,
    domain: &str,
    drivers: &[DriverStmt],
    orchestra: &[AgentCard],
) -> Vec<AgentAssignment> {
    let mut assignments = Vec::new();

    // Step 1: Find the domain-specific agent
    let domain_agent = match domain {
        "sports_nba" => Some("nba_analyst"),
        "biotech" | "pharma" => Some("biotech_analyst"),
        "finance" | "stocks" => Some("macro_forecaster"),
        _ => None,
    };

    for driver in drivers {
        // Step 2: Assign domain agent to every driver
        if let Some(agent) = domain_agent {
            assignments.push(AgentAssignment {
                agent_id: agent.to_string(),
                query: formulate_query(question, driver, agent, domain),
                priority: AssignmentPriority::Primary,
                schedule: Schedule::Once,
            });
        }

        // Step 3: Add cross-domain agents for specific driver types
        let driver_lower = driver.name.to_lowercase();
        if driver_lower.contains("sentiment") || driver_lower.contains("opinion") {
            assignments.push(AgentAssignment {
                agent_id: "sentiment_analyzer".to_string(),
                query: formulate_query(question, driver, "sentiment_analyzer", domain),
                priority: AssignmentPriority::Secondary,
                schedule: Schedule::Once,
            });
        }
        if driver_lower.contains("regulatory") || driver_lower.contains("legal") {
            assignments.push(AgentAssignment {
                agent_id: "entity_investigator".to_string(),
                query: formulate_query(question, driver, "entity_investigator", domain),
                priority: AssignmentPriority::Secondary,
                schedule: Schedule::Once,
            });
        }

        // Step 4: If no domain agent, use general agents
        if domain_agent.is_none() {
            assignments.push(AgentAssignment {
                agent_id: best_general_agent(driver),
                query: formulate_query(question, driver, "macro_forecaster", "general"),
                priority: AssignmentPriority::Primary,
                schedule: Schedule::Once,
            });
        }
    }

    assignments
}
```

---

## Component 2: Query Formulation Engine

This is the **highest-leverage improvement**. The query is what the agent sees. A good query produces good evidence. A generic query produces apologies.

### Current: Generic Template

```
"Research evidence for the 'home_court_advantage' driver in the forecast:
'Will the Lakers win their next game?'"
```

### Proposed: Domain-Aware Query Templates

Each domain + driver type combination gets a specific query template that asks for the exact data points that would move the probability curve.

```rust
fn formulate_query(
    question: &str,
    driver: &DriverStmt,
    agent_id: &str,
    domain: &str,
) -> String {
    let driver_name = driver.display_name.as_deref().unwrap_or(&driver.name);
    let rationale = driver.rationale.as_deref().unwrap_or("");
    let (p5, p50, p95) = extract_driver_params(driver);

    // Domain-specific query templates
    match (domain, agent_id) {
        ("sports_nba", "nba_analyst") => format!(
            "For the forecast: \"{question}\"\n\n\
             Analyze the '{driver_name}' driver (current estimate: p50={p50:.2}).\n\n\
             PROVIDE SPECIFIC DATA:\n\
             1. Current stats relevant to this driver (NetRtg, record, recent form)\n\
             2. Historical base rate for this situation (with sample size)\n\
             3. How this driver should adjust the probability multiplier\n\
             4. Confidence in your assessment (0.0-1.0)\n\n\
             Context: {rationale}\n\n\
             Be quantitative. Cite specific numbers and timeframes."
        ),

        ("finance", "macro_forecaster") => format!(
            "For the forecast: \"{question}\"\n\n\
             Research the '{driver_name}' driver (current estimate: p50={p50:.2}).\n\n\
             PROVIDE:\n\
             1. Current value of the key metric for this driver\n\
             2. Historical trend (3-month, 12-month, and relevant cycle)\n\
             3. Analyst consensus or market expectation (if available)\n\
             4. Comparable precedents (what happened in similar situations?)\n\
             5. How this data should adjust the probability multiplier\n\n\
             Context: {rationale}\n\
             Current parameters: p5={p5:.2}, p50={p50:.2}, p95={p95:.2}"
        ),

        ("biotech", "biotech_analyst") => format!(
            "For the forecast: \"{question}\"\n\n\
             Analyze the '{driver_name}' driver (current estimate: p50={p50:.2}).\n\n\
             PROVIDE:\n\
             1. Clinical trial phase and success rate for this indication/modality\n\
             2. Key data readouts or regulatory milestones ahead\n\
             3. Competitive landscape (similar drugs/therapies in development)\n\
             4. Historical base rate for this type of outcome\n\
             5. How this evidence should adjust the probability multiplier\n\n\
             Context: {rationale}\n\
             Use standard ontology terms (NCIT, MONDO, HPO) where applicable."
        ),

        // Sentiment analysis across all domains
        (_, "sentiment_analyzer") => format!(
            "For the forecast: \"{question}\"\n\n\
             Analyze sentiment around the '{driver_name}' driver.\n\n\
             PROVIDE:\n\
             1. Overall sentiment classification (strongly bearish → strongly bullish)\n\
             2. Key narrative themes in recent coverage (last 30 days)\n\
             3. Sentiment trend direction (improving, stable, deteriorating)\n\
             4. Expert vs public opinion divergence (if any)\n\
             5. How sentiment should adjust the probability multiplier\n\n\
             Context: {rationale}"
        ),

        // Entity investigation across all domains
        (_, "entity_investigator") => format!(
            "For the forecast: \"{question}\"\n\n\
             Investigate entities relevant to the '{driver_name}' driver.\n\n\
             PROVIDE:\n\
             1. Key decision-makers and their positions/incentives\n\
             2. Organizational dynamics (mergers, leadership changes, strategy shifts)\n\
             3. Financial health or resource position of key entities\n\
             4. Relationships and dependencies between entities\n\
             5. How these findings should adjust the probability multiplier\n\n\
             Context: {rationale}"
        ),

        // Default fallback
        _ => format!(
            "For the forecast: \"{question}\"\n\n\
             Research evidence for the '{driver_name}' driver.\n\
             Current estimate: p5={p5:.2}, p50={p50:.2}, p95={p95:.2}\n\n\
             PROVIDE:\n\
             1. Key data points relevant to this driver (with sources)\n\
             2. Historical base rate or precedent\n\
             3. How this evidence should adjust the probability multiplier\n\
             4. Your confidence (0.0-1.0) in the findings\n\n\
             Context: {rationale}\n\
             Be specific and quantitative."
        ),
    }
}
```

### Key Insight: The Query Includes the Current Parameters

By telling the agent "current estimate: p50=1.1", the agent can respond with "Based on X evidence, the multiplier should be higher (1.25) because Y" — directly actionable for the user to adjust the driver.

---

## Component 3: The Research Panel (Right Panel Redesign)

The current right panel has three tabs: Edit / FPL / Wiki. The Edit tab shows either the driver editor or the agent picker. The agent picker is where research happens, but it's cramped and unclear.

### Proposed: Four Tabs

```
┌──────┬──────┬──────┬──────┐
│ Edit │ Rsch │ FPL  │ Wiki │
└──────┴──────┴──────┴──────┘
```

**Research tab ("Rsch")** — the dedicated research panel:

```
┌─────────────────────────────────┐
│ Research: home_court_advantage  │
│                                 │
│ ┌─────────────────────────────┐ │
│ │ nba_analyst (primary)       │ │
│ │ ✓ 3 findings · 42s · 15cr  │ │
│ │                             │ │
│ │ 🔍 Lakers are 22-8 at home │ │
│ │    (.733 win rate, +6.2     │ │
│ │    NetRtg, 4th best in NBA) │ │
│ │                             │ │
│ │ 🔍 Home court advantage     │ │
│ │    worth ~8-10% win prob    │ │
│ │    increase (historical)    │ │
│ │                             │ │
│ │ 🔍 MSG crowd factor: Knicks │ │
│ │    +4.3 NetRtg home vs away │ │
│ │                             │ │
│ │ Suggested p50: 1.12         │ │
│ │ [Accept] [Adjust] [Re-run]  │ │
│ └─────────────────────────────┘ │
│                                 │
│ ┌─────────────────────────────┐ │
│ │ sentiment_analyzer (second) │ │
│ │ ⟳ researching… (12s)        │ │
│ │                             │ │
│ │ 🔍 Fan sentiment is bullish │ │
│ │    after 3-game win streak  │ │
│ └─────────────────────────────┘ │
│                                 │
│ [+ Add Research Agent]          │
│ [Research All Drivers]          │
│                                 │
│ ── Query ──────────────────── │ │
│ │ Analyze home court advant… │ │
│ │ [Edit Query] [Re-run]      │ │
│ └────────────────────────────┘ │
└─────────────────────────────────┘
```

### Key Features:

1. **Per-agent result cards** — each agent's findings displayed as a card with:
   - Agent name + role (primary/secondary)
   - Finding count, execution time, credits charged
   - Individual findings as bullet points
   - **Suggested p50 adjustment** — the agent's recommendation for the driver parameter
   - Accept/Adjust/Re-run buttons

2. **Suggested p50** — the evidence isn't just informational. The agent suggests how the driver's p50 should change based on findings. User can accept (one click) or adjust manually.

3. **Query display** — the actual query sent to the agent is visible and editable. User can refine and re-run.

4. **"Research All Drivers"** button — fires all assigned agents for all drivers in parallel. The nuclear option for fast research.

5. **Live SSE updates** — findings stream in as the agent works. No more 30-second void.

---

## Component 4: Evidence Quality Scoring

Not all evidence is equal. A specific data point with a source is worth more than a vague assertion.

### Quality Dimensions

| Dimension | Weight | Measurement |
|-----------|--------|-------------|
| Specificity | 30% | Contains specific numbers, dates, named entities |
| Source citation | 25% | References a named source (report, database, publication) |
| Recency | 20% | Data is from within the relevant timeframe |
| Relevance | 15% | Directly addresses the driver's question |
| Actionability | 10% | Suggests a specific parameter adjustment |

### Automatic Quality Scoring

```rust
fn score_evidence_quality(evidence: &EvidenceStmt) -> f64 {
    let text = evidence.summary.as_deref().unwrap_or("");
    let findings = &evidence.key_findings;

    let mut score = 0.0;

    // Specificity: contains numbers
    let number_count = text.chars().filter(|c| c.is_ascii_digit()).count();
    let has_percentages = text.contains('%');
    let has_currency = text.contains('$') || text.contains('€');
    score += (number_count as f64 / 20.0).min(0.3) * 0.7;
    if has_percentages { score += 0.1; }
    if has_currency { score += 0.05; }

    // Source citation: named sources
    let source_patterns = [
        "according to", "source:", "report", "Bloomberg", "Reuters",
        "ClinicalTrials.gov", "FDA", "SEC", "analyst", "published",
        "study", "research", "data from", "survey",
    ];
    let has_source = source_patterns.iter()
        .any(|p| text.to_lowercase().contains(&p.to_lowercase()));
    if has_source { score += 0.25; }

    // Recency: mentions recent dates
    let has_recent_date = text.contains("2025") || text.contains("2026")
        || text.contains("Q1") || text.contains("Q2")
        || text.contains("recent") || text.contains("latest");
    if has_recent_date { score += 0.15; }

    // Relevance: provided by the ABW API (agent confidence)
    score += evidence.relevance.unwrap_or(0.5) * 0.15;

    // Findings count (more findings = more thorough)
    score += (findings.len() as f64 / 5.0).min(0.1);

    score.clamp(0.0, 1.0)
}
```

### Visual Quality Indicator

On each evidence card in the Research panel:

```
┌─────────────────────────────────┐
│ Agent: nba_analyst · 85% ████▓  │  ← quality bar
│ Source: NBA.com Advanced Stats   │
│ ...                              │
└─────────────────────────────────┘

┌─────────────────────────────────┐
│ Agent: market_research · 32% █▒  │  ← low quality flag
│ ⚠ Low specificity — consider    │
│   re-running with more specific  │
│   query                          │
└─────────────────────────────────┘
```

When quality is below 40%, the system suggests re-running with a more specific query or using a different agent.

---

## Component 5: Auto-Assign on Decomposition

Today, after Fermi decomposes, the user manually assigns agents one by one. The proposal: **auto-assign immediately** and let the user override.

### Flow

```
User: Ctrl+Enter on "Will the Lakers win?"

1. Fermi decomposes → 5 drivers appear
   (home_court, opponent_strength, recent_form, injury_impact, schedule)

2. Orchestrator auto-assigns:
   - home_court → nba_analyst (primary, domain match)
   - opponent_strength → nba_analyst (primary)
   - recent_form → nba_analyst (primary)
   - injury_impact → nba_analyst (primary)
   - schedule → nba_analyst (primary)
   + sentiment_analyzer → assigned to all (secondary, for public perception)

3. All 6 agents fire in parallel immediately

4. User sees:
   - 5 driver cards, each with "⟳ nba_analyst researching…"
   - Fermi banner: "⟳ Researching: nba_analyst (5 drivers), sentiment_analyzer"
   - Findings pop in via SSE as agents complete

5. Within 30-40 seconds, all drivers have evidence
   - No manual intervention needed
   - User reviews and adjusts
```

### User Override Points

- **Before research:** User can remove auto-assigned agents, add different ones, edit queries
- **After research:** User can re-run with different agent/query, mark evidence as irrelevant
- **On the driver card:** Agent badge shows "auto-assigned" vs "user-assigned"

### UX: The "Research All" Button

After decomposition, the drivers appear with auto-assigned agents in a "pending" state:

```
home_court_advantage [continuous] 0.8 – 1.0 – 1.1
  nba_analyst (auto) — pending
  [Start Research] [Change Agent] [Edit Query]
```

Or a single button at the top: **"🔬 Research All Drivers"** — fires everything at once.

**Most aggressive option:** Auto-fire immediately after decomposition. No button needed. The user's Ctrl+Enter IS the research trigger. Findings start streaming 3-5 seconds after the base rate appears.

---

## Component 6: Research Scheduling

For forecasts that matter over time, one-shot research isn't enough. The user should be able to schedule re-research:

### Schedule Options

| Schedule | Use Case | Cost |
|----------|----------|------|
| Once | Default, one-time research | 5-15 credits |
| Daily | Fast-moving events (earnings, elections, sports) | 5-15 credits/day |
| Weekly | Slow-moving forecasts (geopolitics, technology) | 5-15 credits/week |
| On trigger | Re-research when probability diverges >10pp from base | Variable |

### Staleness Indicator

In the driver card, evidence shows age:

```
Evidence: ●●● Strong (3 items)
  └ Latest: 2 hours ago          ← green, fresh
  └ Latest: 5 days ago           ← gold, getting stale
  └ Latest: 3 weeks ago          ← red, stale — suggest refresh
```

### Auto-refresh Notification

When evidence goes stale (configurable per forecast):

```
🦊 Fermi: Evidence for 'home_court_advantage' is 7 days old.
          The Lakers played 3 games since last research.
          [Refresh Now] [Schedule Daily] [Dismiss]
```

---

## Component 7: Evidence → Parameter Suggestion Pipeline

The most valuable feature: evidence directly suggests parameter adjustments.

### Current: Evidence is Informational Only

Agent returns "Lakers are 22-8 at home (.733)" → user reads it → user manually decides to set p50=1.12.

### Proposed: Evidence Suggests Adjustments

Agent prompt includes: "Based on your findings, suggest what the driver's p50 multiplier should be."

Agent returns:
```json
{
  "findings": [...],
  "suggested_p50": 1.12,
  "suggested_p5": 0.95,
  "suggested_p95": 1.25,
  "adjustment_reasoning": "Lakers' .733 home win rate is well above the .580 league average, suggesting a 12% positive multiplier. However, uncertainty in opponent quality and schedule creates wide p5-p95 range."
}
```

### UI: One-Click Accept

```
┌─────────────────────────────────┐
│ nba_analyst suggests:           │
│                                 │
│ p50: 1.00 → 1.12 (+12%)        │
│ p5:  0.85 → 0.95               │
│ p95: 1.15 → 1.25               │
│                                 │
│ "Lakers' .733 home win rate…"   │
│                                 │
│ [✓ Accept] [✎ Adjust] [✗ Skip] │
└─────────────────────────────────┘
```

Clicking "Accept" updates the driver parameters, bumps the version, and recalculates the probability. One click from evidence to updated forecast.

---

## Implementation Roadmap

### Sprint A: Core Quality (This Sprint)

1. **Domain-first agent routing** — replace keyword matcher with domain lookup
2. **Query formulation engine** — domain-specific templates with driver parameters
3. **Auto-assign on decomposition** — agents assigned immediately, user overrides
4. **"Research All" button** — fire all assigned agents in parallel
5. **Evidence quality scoring** — automatic quality bar on each evidence item

### Sprint B: Research Panel (Next Sprint)

6. **Research tab** in right panel — dedicated research view per driver
7. **Per-agent result cards** with findings, quality score, timing
8. **Suggested p50** — evidence proposes parameter adjustments
9. **One-click accept** — apply agent's suggested parameters
10. **Re-run with edited query** — refine and retry

### Sprint C: Scheduling & Lifecycle (Sprint After)

11. **Evidence staleness indicator** — age badge on evidence
12. **Scheduled re-research** — daily/weekly/on-trigger
13. **Refresh notification** — Fermi banner suggests when to re-research
14. **Evidence history** — track how evidence changed over time

### Sprint D: Learning & Calibration (Longer Term)

15. **Evidence impact tracking** — which evidence items actually moved the probability in the right direction
16. **Agent performance scoring** — which agents produce the highest-quality evidence per domain
17. **Query optimization** — learn which query formulations produce better results
18. **Cross-forecast evidence** — evidence from one forecast relevant to another

---

## Metrics

| Metric | Current | Target | How |
|--------|---------|--------|-----|
| Time from question to first evidence | 30-60s (manual) | <5s (auto-assign + SSE) | Auto-assign + parallel fire |
| Evidence quality score (avg) | ~0.3 (estimated) | >0.6 | Query formulation + quality scoring |
| Agent domain accuracy | ~50% (wrong agent often) | >90% | Domain-first routing |
| User manual interventions per forecast | 5-10 clicks | 0-2 clicks | Auto-assign + one-click accept |
| Evidence with specific numbers | ~30% | >70% | Query templates demand quantitative data |
| Forecasts with stale evidence (>7 days) | 100% | <20% | Scheduled refresh |

---

## Key Insight

The agent assignment panel isn't a settings page — it's the **research command center**. The user should feel like they're directing a team of analysts, not configuring a pipeline. Every interaction should produce visible, specific, actionable evidence that directly informs the probability estimate.

The difference between "here's some background on the topic" and "based on this specific data, your p50 should be 1.12 instead of 1.00" — that's the difference between a toy and a tool.

---

## References

- [Agent Development Guide](../guides/AGENT_DEVELOPMENT.md) — agent card spec, skills/tags
- [Latency Reduction Design](LATENCY_REDUCTION.md) — SSE streaming, parallel execution
- [Research Cockpit Design](RESEARCH_COCKPIT.md) — original OODA loop UX
- [Console MVP Architecture](CONSOLE_MVP_ARCHITECTURE.md) — FPL as source of truth