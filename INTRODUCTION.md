# Welcome to Fermi

## AI-Coordinated Probabilistic Forecasting with Built-in Learning

---

## What is Fermi?

Fermi is an **AI-coordinated forecasting platform**. Instead of manually researching and calculating probabilities, you work with a team of specialized AI agents that do the heavy lifting — and Fermi coordinates them all.

Think of it like having a research department: you ask a question, Fermi assigns the right agents, synthesizes their findings, runs simulations, and delivers a probability estimate. When the question resolves, Fermi learns what worked.

### The Core Idea

You: *"Will AMD hit $200M revenue in FY2026?"*

Fermi: *coordinates agents → synthesizes research → runs Monte Carlo simulation → gives you a probability distribution*

Then: *watches for resolution → calculates Brier score → learns for next time*

---

## Key Features

### Your AI Research Team

Fermi coordinates a growing roster of AI agents. Any agent tagged `fermi-orchestra` in Agent Bestiary becomes available to Fermi. These agents specialize in:

- Market research and competitive analysis
- Sentiment analysis from news and social media
- Company financials and valuations
- Biotech and clinical trial analysis
- Macroeconomic indicators
- Polymarket data interpretation
- Entity investigation and relationship mapping

**The key**: You don't need to know which agent to use. Fermi figures it out based on your question.

### Fermi the Coordinator

Fermi isn't just a tool — it's an **agentic orchestrator** that:

1. **Understands your question** — Parses what you're asking and identifies the domain
2. **Assigns agents** — Routes to the right specialists automatically
3. **Synthesizes findings** — Combines research from multiple agents into coherent drivers
4. **Runs simulations** — Executes Monte Carlo to generate probability distributions
5. **Tracks outcomes** — Monitors for resolution and calculates Brier scores
6. **Learns** — Analyzes what improved predictions and adjusts approach

### Polymarket Integration

Fermi integrates with [Polymarket](https://polymarket.com), the world's largest prediction market:

- **Import markets** — Browse and import any Polymarket question into Fermi
- **Three-number view** — See: historical base rate | Polymarket crowd | your estimate
- **Edge detection** — Divergence between your model and the crowd = your edge signal
- **Auto-resolution** — When Polymarket resolves, Fermi auto-resolves your linked forecast and calculates your Brier score

### The Feedback Loop

Every forecast that resolves teaches you something:

```
Question → Fermi coordinates agents → Research → Simulation → Probability
                                                                    ↓
Resolution (auto via Polymarket) → Brier Score → Learn → Better next time
```

Fermi tracks:
- Your overall Brier score (lower = better)
- Calibration curve over time
- Which agents contribute to accurate predictions
- How your estimates compare to the Polymarket crowd

---

## How It Works

### 1. Ask a Question

Pose any yes/no question with a clear resolution date:

*"Will ASTS hit $200M revenue in FY2026?"*

### 2. Fermi Does the Work

Fermi automatically:

- Identifies relevant domains (space, telecom, revenue)
- Routes to appropriate research agents
- Synthesizes findings into probabilistic drivers
- Runs Monte Carlo simulation (10K-10M iterations)
- Returns a probability distribution

### 3. Get Your Answer

You receive:

- **Probability estimate** (e.g., 68% chance)
- **Confidence interval** (90% likely range)
- **Key drivers** — what factors matter most
- **Polymarket comparison** — how you vs. the crowd

### 4. Publish & Track

Publish to the leaderboard. When resolved:

- Brier score calculated automatically
- System learns what worked
- Your calibration improves over time

---

## What You Can Do

### Ask Questions in Plain Language

No code required. Just ask:

> "Will the Fed cut rates in March 2026?"
> "Will SpaceX land on Mars by 2030?"
> "Will Bitcoin hit $200K in 2026?"

Fermi handles the rest.

### Leverage AI Agents

Fermi coordinates research agents on your behalf. You see:
- Which agents were consulted
- What they found
- How their research influenced your estimate

### Compare to the Crowd

Every Polymarket-linked forecast shows:
- Your probability
- The market's probability
- The gap (your edge or blind spot)

### Track Your Performance

Your dashboard shows:
- Brier score over time
- Calibration curve
- Leaderboard rank
- Which agents improve your predictions

---

## Why Fermi Works

### Better Than Gut Instinct

Humans are notoriously bad at probabilities. Fermi combines:
- **AI research** — processes more information than any human
- **Monte Carlo** — simulates thousands of scenarios
- **Calibration tracking** — learns from outcomes

### Better Than Going Alone

You could hire a research team. Or you could use Fermi, which:
- Coordinates multiple specialized agents
- Synthesizes conflicting findings automatically
- Tracks what leads to accurate predictions

### The Feedback Loop

This is the secret sauce. Most forecasting ends at "what do you think?" Fermi continues:

1. Question resolves
2. Brier score calculated
3. System analyzes: which agents helped? which hurt?
4. Next forecast gets better

Over time, you develop genuine insight rather than overconfidence.

---

## Getting Started

### Option 1: Web Interface

Visit **[agent-bestiary.world](https://agent-bestiary.world)**

### Option 2: Desktop App

```bash
git clone https://github.com/Replicant-Partners/fermi
cd fermi
cargo build -p fermi-console
cargo run -p fermi-console
```

See [FERMI_CONSOLE.md](./FERMI_CONSOLE.md)

### Option 3: Zed Editor

Install the Fermi extension for FPL syntax highlighting and direct agent execution.

---

## Learn More

- **[FERMI_CONSOLE.md](./FERMI_CONSOLE.md)** — Build and run the desktop app
- **[docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md](./docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md)** — Polymarket details

---

*Ask questions. Let AI agents research. Learn from outcomes.*
