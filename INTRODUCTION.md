# Welcome to Fermi

## A Platform for AI-Powered Probabilistic Forecasting with Built-in Learning

---

## What is Fermi?

Fermi is a platform for making better decisions under uncertainty. It combines **AI agents** with **probabilistic forecasting** to help you answer questions like:

- *"Will AMD hit $200M revenue in FY2026?"*
- *"Will Sweden win Eurovision 2026?"*
- *"What are the odds SpaceX lands on Mars by 2030?"*

Instead of giving a single number, Fermi gives you a **probability distribution** — a range of likely outcomes with confidence levels. This is how the best forecasters in the world think, and now you can too.

### The Core Loop

Fermi isn't just a forecasting tool — it's a **self-improving system**:

```
Question → Decompose → Research → Simulate → Publish
                                           ↓
                                    Resolution
                                           ↓
                                    Brier Score
                                           ↓
                              Learn → Improve Forecasts
```

Every forecast that resolves teaches you something. Your Brier scores accumulate. The system learns which agents, which evidence, and which decomposition strategies lead to better predictions. Over time, you get better.

---

## Key Features

### Polymarket Integration

Fermi integrates with [Polymarket](https://polymarket.com), the world's largest prediction market ($300M+ daily volume):

1. **Import & Decompose** — Browse Polymarket markets, import a question into Fermi, and run your own decomposition on it. The link to the Polymarket market is preserved.

2. **Three-Number Outside View** — Every linked forecast shows:
   - **Historical base rate** — How often has this type of event happened?
   - **Polymarket crowd price** — What does the market think? (live updated)
   - **Your Fermi estimate** — What does your model say?

   The divergence between your estimate and the crowd is your **edge signal**.

3. **Auto-Resolution** — When Polymarket's oracle resolves a market, Fermi automatically resolves your linked forecast and computes your Brier score. Zero manual effort.

### AI Research Agents

Fermi has a roster of specialized AI agents that research topics for you:

- **market_research** — Market trends, competitive dynamics
- **sentiment_analyzer** — Public opinion, social media
- **biotech_analyst** — Clinical trials, drug approvals
- **equity_analyst** — Company financials, valuations
- **macro_forecaster** — Economic indicators, policy impacts
- **prediction_market** — Interprets Polymarket data

### Brier Score Feedback

Fermi scores every forecast using **Brier Score** — the gold standard:

| Score | Meaning |
|-------|---------|
| 0.00 | Perfect prediction |
| 0.25 | Well-calibrated (coin flip) |
| 0.50 | No better than guessing |

Your dashboard tracks:
- Active forecasts
- Resolved outcomes with Brier scores
- Calibration curve over time
- Leaderboard rank

---

## How It Works

### 1. Define Your Question

Start with a forecasting question. Good questions are:

- **Specific** — "Will X happen?" not "What about X?"
- **Time-bound** — Include a clear resolution date
- **Measurable** — The outcome must be verifiable

```
forecast "Will ASTS hit $200M revenue in FY2026?" {
    question_type: binary
    resolution_date: "2026-09-30"
}
```

### 2. Decompose into Drivers

Break your question into underlying factors. Each driver is a **probabilistic variable** with a distribution:

```fpl
driver revenue_2026 triangular(100, 180, 350)
driver path_to_revenue discrete(Direct_Sales, Partnership, Government)
driver market_growth normal(1.15, 0.1)
```

You can use:
- **Triangular** — When you have min, likely, and max
- **Normal** — When you know mean and standard deviation
- **Lognormal** — For positive skew (e.g., stock prices)
- **Beta** — For probabilities between 0 and 1
- **Uniform** — When any value in range is equally likely

### 3. Research with AI Agents

Ask an agent:
```
"Research AST SpaceMobile's revenue trajectory and competitive position"
```

The agent returns evidence you can incorporate into your drivers.

### 4. Run Monte Carlo Simulation

Fermi runs your forecast through **10,000 to 10,000,000 simulations**, sampling from each driver's distribution and computing the final estimate each time.

```
estimate revenue_2026 * path_factor * market_growth
```

### 5. Get Your Results

Fermi shows you the probability distribution:

- **Median** — 50% chance the outcome is below this
- **90% CI** — 90% confidence interval
- **Histogram** — Full distribution visualization
- **Tornado Chart** — Which drivers matter most
- **Polymarket divergence** — How your estimate compares to the crowd

### 6. Publish & Track

Publish forecasts to the leaderboard. When they resolve:

1. **Auto-resolution** — Polymarket-linked forecasts resolve automatically
2. **Brier calculation** — Your score is computed: (prediction - outcome)²
3. **Learning** — The system analyzes which agents and strategies performed best
4. **Improvement** — You adjust your approach for the next forecast

---

## What You Can Do with Fermi

### Create Forecasts

Build probabilistic forecasts for any question. The Fermi Console gives you a visual interface:

- **Question Builder** — Define what you're predicting
- **Driver Editor** — Add and configure variables
- **Simulation Config** — Set iterations and parameters
- **Results Viewer** — Histograms, sensitivity analysis

### Use AI Agents

Leverage a team of specialized AI researchers. Each agent has:
- **Domain expertise** — Trained on relevant knowledge
- **Tools** — Web search, data analysis, document review
- **Performance tracking** — Execution count, Brier impact, cost

### Link to Polymarket

Import markets from Polymarket to:

- Get real-time crowd probabilities as a benchmark
- See divergence between your model and the market
- Auto-resolve when the market resolves
- Build calibration against the crowd

### Track Your Performance

Your dashboard shows:
- Overall Brier score
- Calibration curve
- Active vs. resolved forecasts
- Leaderboard ranking
- Per-agent performance (which agents improve your scores?)

---

## The Feedback Loop

This is what makes Fermi special:

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   ┌──────────────┐    ┌──────────────┐    ┌────────────┐ │
│   │   Forecast   │───→│  Resolution  │───→│   Brier    │ │
│   │   Created    │    │   (auto)     │    │   Score    │ │
│   └──────────────┘    └──────────────┘    └──────┬─────┘ │
│                                                    │        │
│                                                    ▼        │
│   ┌──────────────┐    ┌──────────────┐    ┌────────────┐ │
│   │   Better     │←───│   Learn      │←───│  Analyze   │ │
│   │  Forecasts   │    │  what works │    │  patterns  │ │
│   └──────────────┘    └──────────────┘    └────────────┘ │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

The system learns:
- Which **agents** consistently improve forecasts
- Which **decomposition strategies** lead to better Brier scores
- When you're **overconfident** vs. **underconfident**
- How your **calibration** compares to the Polymarket crowd

---

## Getting Started

### Option 1: Web Interface

Visit **[agent-bestiary.world](https://agent-bestiary.world)** to browse agents and view the leaderboard.

### Option 2: Desktop App

Download and run the Fermi Console:

```bash
git clone https://github.com/Replicant-Partners/fermi
cd fermi
cargo build -p fermi-console
cargo run -p fermi-console
```

See [FERMI_CONSOLE.md](./FERMI_CONSOLE.md) for detailed instructions.

### Option 3: Zed Editor

If you use Zed, install the Fermi extension for:

- Syntax highlighting for FPL files
- Language server with autocomplete
- Direct agent execution from your editor

---

## The Fermi Philosophy

### Why Probabilities?

Single-point estimates are misleading. "AMD will hit $200M" is either right or wrong. "60% chance of $180-220M" is honest about uncertainty.

### Why the Feedback Loop?

The best forecasters don't just make predictions — they learn from them. Fermi makes this systematic. Every resolution is data. Every Brier score teaches you something. The platform compounds your learning.

### Why Polymarket?

The crowd is often right but not always. By comparing your model to the Polymarket crowd, you find your edge. When you're right and the crowd is wrong, that's valuable signal. When the crowd is right and you're wrong, that's valuable learning.

### Why Open?

Fermi is open source. Your forecasts are your data. The platform learns from everyone, but you own your predictions.

---

## Learn More

- **[FERMI_CONSOLE.md](./FERMI_CONSOLE.md)** — Build and run the desktop app
- **[crates/fermi-console/README.md](./crates/fermi-console/README.md)** — Technical architecture
- **[docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md](./docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md)** — Polymarket integration details

---

*Make better decisions under uncertainty. Learn from every outcome.*
