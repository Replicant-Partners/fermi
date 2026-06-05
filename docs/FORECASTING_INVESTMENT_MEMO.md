# Fermi: AI-Augmented Forecasting Infrastructure
## A Brief Investment Memo

*June 2026*

---

## The Problem Tetlock Identified

Philip Tetlock spent twenty years studying forecasting accuracy. His finding, replicated across thousands of forecasters and hundreds of thousands of predictions: most experts perform no better than chance, but a small group — superforecasters — consistently outperform. What separates them is not domain expertise. It is method:

1. **Decompose** complex questions into independent, estimable components
2. **Anchor** estimates to base rates and outside-view reference classes
3. **Update** when new evidence arrives, without over- or under-reacting
4. **Score** every outcome against calibrated probability estimates
5. **Iterate** — the scoring loop is what drives improvement

The Good Judgment Project proved this method works at scale. Prediction markets grew around it.

What nobody built is the infrastructure to automate the hard parts.

---

## Why the System Is Called Fermi

Tetlock didn't invent this methodology from scratch. He borrowed its core move from Enrico Fermi — the physicist famous for estimating unknown quantities from first principles rather than direct measurement. Fermi could calculate the yield of the Trinity test from scraps of paper falling in the blast wave. His technique: decompose the problem into components you *can* estimate, then combine them. Tetlock's superforecasters do exactly this with probability questions.

The name is intentional. **Fermi — the system — is Enrico Fermi on your forecasting team.** It takes a question you can't answer directly and decomposes it into components you can, combines them correctly, and tells you where your uncertainty actually lives.

---

## What Fermi Is

Fermi is an agentic forecasting platform that executes the Tetlock methodology at machine speed.

A user poses a question — *"Will this merger close before year-end?"* or *"What is the probability of a recession in Q3?"* Fermi:

1. **Decomposes** the question into probability drivers using FPL, a purpose-built probabilistic decomposition language
2. **Deploys research agents** that gather evidence for each driver — base rates, recent signals, expert consensus, market prices
3. **Runs Monte Carlo simulation** across the driver tree, propagating uncertainty correctly
4. **Returns a calibrated probability** with confidence intervals, sensitivity analysis showing which drivers dominate the output, and the full reasoning chain
5. **Tracks resolution** and scores every outcome using Brier scoring
6. **Learns** — agent performance is tracked by domain and question type; the system gets measurably better with each resolved question

This is not a chatbot that gives you a probability when asked. It is infrastructure for epistemically rigorous, evidence-grounded, self-improving probabilistic reasoning.

---

## The Polymarket Data Flywheel

Polymarket currently handles $300M+ in daily prediction market volume. It is the richest real-time signal about what informed, financially-committed forecasters collectively believe.

Fermi integrates Polymarket prices as live evidence in its research pipeline. But the relationship goes deeper than data consumption:

- Fermi's decomposition models generate *reasons* for probability estimates — the underlying driver structure that a market price summarises but doesn't explain
- As Fermi forecasts resolve against Polymarket outcomes, the system builds a calibration record: which decomposition approaches, which research agents, and which driver weightings produced the most accurate predictions
- That calibration record is the flywheel — every resolved question makes the next forecast better, and every Polymarket resolution is a free, high-quality ground-truth label

Over time this creates a dataset that doesn't exist anywhere else: structured probabilistic reasoning chains, each tagged with an outcome score. This is what would be needed to train the next generation of forecasting-specific models — and it accumulates automatically as a byproduct of the platform's normal operation.

---

## Why This Is Differentiated

**Against prediction markets**: Markets aggregate revealed preferences; they don't explain their reasoning, don't decompose drivers, and don't improve methodology. Fermi produces reasoning, not just a price. The two are complementary — Fermi treats markets as evidence, not competition.

**Against LLM assistants**: A language model asked for a probability gives you a confident-sounding number with no epistemics behind it. Fermi gives you a *model* — a decomposed structure where you can see exactly where the uncertainty comes from and what evidence drives each component.

**Against manual superforecasting platforms**: Good Judgment Project and Metaculus require human forecasters to do the research and decomposition manually. Fermi automates those steps. Human judgment is applied at the level of model structure, not data gathering.

**The moat**: Fermi's agents accumulate knowledge. An agent that has processed 500 macroeconomic questions has a richer understanding of base rates, signal patterns, and resolution dynamics than one that has processed 5. This knowledge is encoded in knowledge graphs and episodic memory — it cannot be replicated by cloning the code. The moat is accumulated calibration, not technology.

---

## The Market

Forecasting is the core operation of every institution that makes decisions under uncertainty:

- **Finance**: Quantitative funds, macro desks, risk functions. Probability-over-time is their native language.
- **Policy and think tanks**: Scenario planning, election forecasting, geopolitical risk assessment.
- **Corporate strategy**: "What is the probability our lead compound reaches Phase 3?" is a Fermi question.
- **Journalism and research**: Forecasting-as-accountability — public predictions, scored.

The immediate beachhead is the community of practice already formed around Tetlock's work: Metaculus users, Good Judgment alumni, Polymarket traders. This is a small, high-signal group with strong network effects and a demonstrated willingness to invest time in forecasting methodology. They are the seed users who generate the calibration data that makes the platform valuable to everyone who follows.

---

## Current State

- FPL language: designed, implemented, parsing and Monte Carlo execution in production
- Agent executor: 34 agents in production across research, synthesis, and decomposition domains
- Brier scoring and resolution tracking: built
- Sensitivity analysis (Sobol): built
- Polymarket integration: built
- Multi-provider LLM routing: built (no single-vendor dependency)
- Fermi console (desktop forecasting environment): in development

What is needed: user acquisition, the forecasting-specific UX layer on top of the engine, and domain-specific agent packs (macroeconomic, geopolitical, biomedical).

---

## The Tetlock Connection

Fermi is, in one framing, an attempt to automate what Tetlock's superforecasters do manually: structured decomposition, base-rate anchoring, evidence synthesis, calibrated updating. We have built the infrastructure. The Good Judgment Project has the methodology, the validated scoring framework, the forecasting track record, and the community.

A collaboration would be bidirectional: Fermi provides the automation layer and the scaling infrastructure; Good Judgment provides methodological validation, research partnerships, and access to the community of practice that already understands why this matters.

This is not a sponsorship pitch. It is a thesis that the next phase of the Good Judgment Project is computational — and that the infrastructure for it exists.

---

## What We Are Looking For

A pre-seed round ($500K–$1.5M) to fund:

- Forecasting UX layer (question intake, driver editor, calibration dashboard)
- Domain agent packs (macroeconomic, geopolitical, biomedical)
- Community seeding (free access for Good Judgment alumni and Metaculus power users)
- One enterprise pilot (financial institution or policy research organization)

Break-even on infrastructure is immediate. Operational break-even at approximately 500 active forecasters.

---

*Enrico Fermi could estimate the yield of a nuclear test from scraps of paper in the blast wave. The technique was decomposition. Tetlock proved it works for forecasting. We built the machine.*

