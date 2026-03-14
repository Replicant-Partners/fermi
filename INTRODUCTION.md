# Welcome to Fermi

## AI-Coordinated Probabilistic Forecasting with Built-in Learning

---

## What is Fermi?

Fermi is an **agentic forecasting platform** — a system of coordinated AI agents that help you research, estimate, and track probabilistic forecasts. It's built on a next-generation agent coordination and execution platform that orchestrates specialized agents to do the heavy lifting.

Think of it like having a research department: you ask a question, Fermi deploys agents to gather evidence, synthesizes their findings, runs Monte Carlo simulations, and delivers a probability estimate. When the question resolves, Fermi learns what worked — and gets better the next time.

### The Core Idea

You: *"Will AMD hit $200M revenue in FY2026?"*

Fermi: *deploys agents → synthesizes research → runs simulation → gives you a probability*

Then: *watches for resolution → calculates Brier score → learns for next time*

This is a **self-learning system** — every outcome makes the next forecast better.

---

## Key Features

### Guided Forecasting (Tetlock Methodology)

Fermi guides you through the forecasting process based in SUperforecasting practices:

1. **Decomposition** — Break complex questions into independent drivers
2. **Outside view** — Find base rates and comparable historical cases
3. **Inside view** — Apply specific knowledge about the case
4. **Calibration** — Train your probabilistic instincts
5. **Tracking** — Score outcomes and learn from errors

Fermi walks you through each step. You're not just guessing — you're following a methodology used by the world's best forecasters.

### Agentic Coordination

Fermi runs on a next-generation agent coordination platform that:

- **Perceives** your question and extracts the domain and key entities
- **Plans** which agents to deploy based on what needs research
- **Acts** by executing multiple specialized agents in parallel
- **Synthesizes** findings into coherent probabilistic drivers
- **Reflects** on outcomes to improve future performance

The platform manages agent lifecycles, handles retries, and ensures reliable execution. You focus on the question; Fermi handles the orchestration.

### Extensible Futures Integration

Polymarket integration is the first implementation — but the architecture supports any futures market:

- **Polymarket** — Prediction markets with real money ($300M+ daily volume)
- **Pwin** — Proabaility win analysis
- **Strategic forecasting** — Corporate planning and scenario analysis
- **Predictive maintenance** — Industrial equipment failure prediction
- **Custom resolution sources** — Your own data feeds and oracles

When a market resolves (however it resolves), Fermi auto-calculates your Brier score.

### The Feedback Loop (Self-Learning)

This is Fermi's superpower — a closed-loop learning system:

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│   Question → Agents Research → Synthesis → Simulation       │
│                                                      │       │
│                                                      ▼       │
│   Better Forecasts ← Learn ← Brier Score ← Resolution       │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

Every forecast that resolves:
- Calculates your Brier score: (prediction - outcome)²
- Identifies which agents and strategies helped
- Updates your calibration profile
- Compounds learning for future forecasts

Over time, this creates genuine probabilistic intuition rather than overconfidence.

---

## How It Works

### 1. Ask a Question

Pose any question with a clear resolution criteria and date:

*"Will ASTS hit $200M revenue in FY2026, verified by their 10-K filing?"*

### 2. Fermi Guides You Through

Following Tetlock methodology, Fermi helps you:

- **Decompose** — Identify the key factors driving the outcome
- **Find base rates** — What's the historical frequency?
- **Gather evidence** — Deploy agents to research each factor
- **Synthesize** — Combine findings into probability distributions

### 3. Agents Do the Research

Fermi's coordination platform deploys specialized agents:

- Market research agents analyze trends and competition
- Sentiment agents gauge public and investor opinion
- Financial agents dig into company fundamentals
- Entity agents map relationships and dependencies

You see which agents were consulted and what they found.

### 4. Get Your Estimate

You receive:

- **Probability distribution** — Not a point estimate, but a range
- **Confidence interval** — How sure you should be
- **Key drivers** — What factors matter most
- **Market comparison** — How your estimate compares to Polymarket (if linked)

### 5. Track & Learn

Publish to the leaderboard. When resolved:

- **Auto-resolution** — Fermi detects the outcome
- **Brier scoring** — Your accuracy is quantified
- **Learning** — The system updates what it knows about good forecasting

---

## What You Can Do

### Ask Questions in Plain Language

No code required:

> "Will the Fed cut rates in March 2026?"
> "Will SpaceX land on Mars by 2030?"
> "Will this equipment fail within 90 days?"

Fermi handles the rest.

### Get AI-Assisted Research

Agents work for you:

- Research competitors, market trends, sentiment
- Find comparable historical cases
- Extract key metrics and data points

You make the final call, but you're informed.

### Link to Real-World Outcomes

Connect forecasts to resolution sources:

- **Polymarket** — Import markets, get auto-resolution
- **Custom oracles** — Define your own resolution criteria
- **Manual resolution** — Mark outcomes yourself

### Track Your Calibration

Your dashboard shows:

- **Brier score** — Lower is better (0.00 = perfect, 0.25 = coin flip)
- **Calibration curve** — Are you overconfident or underconfident?
- **Agent performance** — Which agents improve your predictions?
- **Leaderboard** — How you rank against other forecasters

---

## Why Fermi Works

### Methodology + AI

Tetlock's research shows that trained forecasters beat experts. Fermi combines:

- **Proven methodology** — Decomposition, base rates, calibration
- **AI agents** — Process more information than any human
- **Monte Carlo** — Simulate thousands of scenarios
- **Closed-loop learning** — Every outcome makes you better

### Agentic Architecture

The underlying platform treats forecasting as an agentic problem:

- **Perception** — Understand the question, extract entities
- **Reasoning** — Plan which agents to deploy
- **Action** — Execute research in parallel
- **Learning** — Update beliefs based on outcomes

This isn't just automation — it's genuine agentic coordination.

### The Feedback Loop

Most forecasting tools stop at "here's my prediction." Fermi continues:

1. Question resolves
2. Brier score calculated
3. System learns: which agents helped? which strategies?
4. Next forecast incorporates that learning

You don't just make predictions — you develop genuine probabilistic judgment.

---

## Getting Started

### Option 1: Desktop App

```bash
git clone https://github.com/Replicant-Partners/fermi
cd fermi
cargo build -p fermi-console
cargo run -p fermi-console
```

See [FERMI_CONSOLE.md](./FERMI_CONSOLE.md)

### Option 2: Zed Editor

Install the Fermi extension for FPL syntax highlighting and direct agent execution. You can add the ABW MCP to develop and deploy new fermi specific agents (need to define how but thats there already)

---

## Learn More

- **[FERMI_CONSOLE.md](./FERMI_CONSOLE.md)** — Build and run the desktop app
- **[docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md](./docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md)** — Polymarket integration details

---

*Ask questions. Deploy agents. Learn from outcomes.*
