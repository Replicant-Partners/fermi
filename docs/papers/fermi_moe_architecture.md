# Fermi: Probabilistic Forecasting as a Domain-Constrained Mixture-of-Experts System

**Authors:** Ivan Labra — axelotl partners  
**Version:** 1.0 — June 2026  
**Status:** Working paper

---

## Abstract

Fermi is an instantiation of the Agent Bestiary World (ABW) substrate for the domain of probabilistic forecasting. It demonstrates the domain-constrained Mixture-of-Experts (MoE) pattern in a domain where the output contract is a probability distribution, the calibration signal is the Brier score computed against resolved ground truth, and the prior elicitation methodology is grounded in Tetlock's superforecasting principles (Tetlock & Gardner, 2015). This paper describes Fermi as a *logical* system: the Calibrated Evidence Protocol (CEP) that constitutes its output contract, the decomposition-and-routing procedure by which the conductor agent decomposes a forecast question and assigns each driver to a domain-specialist expert, the FPL (Fermi Probabilistic Language) that encodes the forecaster's model as an executable Monte Carlo program, and the calibration feedback loop that allows routing to become more accurate over time. We argue that Fermi's architecture separates three problems that are typically conflated in forecasting systems — decomposition (what are the independent drivers?), evidence gathering (what does each driver's domain evidence suggest?), and probabilistic integration (how do the drivers combine into a forecast distribution?) — and that this separation is what enables systematic improvement through calibration feedback.

**Keywords:** probabilistic forecasting, Tetlock methodology, mixture of experts, Brier score, Monte Carlo simulation, domain-constrained MoE, calibration

---

## 1. Introduction

### 1.1 The forecasting problem

Probabilistic forecasting — assigning calibrated probability distributions to future events — is a well-studied problem with a substantial empirical literature. Tetlock's forecasting tournaments (2005, 2015) establish that human forecasters can achieve calibration significantly above chance when they adopt structured decomposition practices: identify independent drivers, anchor to historical base rates, apply Bayesian updating as evidence arrives, and maintain explicit uncertainty quantification.

The computational challenge is to instantiate these practices at scale in a multi-agent system. A single LLM can produce a probability estimate for a forecast question, but such estimates are typically neither decomposed, nor calibrated against historical base rates, nor explicitly uncertainty-quantified. They produce numbers without the epistemic scaffolding that makes those numbers trustworthy.

Fermi's architecture addresses this by decomposing the forecasting problem into three structurally distinct sub-problems and assigning each to a specialised component.

### 1.2 Three separable problems

**Problem 1 — Decomposition:** Given a forecast question, what are the independent drivers of its outcome probability? Drivers must be orthogonal (no double-counting causal paths), bounded in number (3–6 for tractable integration), and typed (continuous probability multipliers vs. discrete binary events). This is a structural problem: it requires understanding the causal structure of the domain.

**Problem 2 — Evidence gathering:** For each identified driver, what does the current domain evidence suggest about its distribution? This is a domain knowledge problem: it requires access to current data, historical base rates, and domain-specific expertise.

**Problem 3 — Probabilistic integration:** Given a set of driver distributions, how do they combine to produce a forecast distribution for the outcome? This is a mathematical problem: it requires Monte Carlo simulation over the joint distribution of drivers, producing a probability distribution over outcomes with correct uncertainty propagation.

Fermi assigns each problem to a separate component. The conductor agent (§2) handles decomposition. Domain-specialist expert agents (§3) handle evidence gathering. The FPL executor (§4) handles probabilistic integration. The three components interact through well-defined interfaces, enabling each to be improved independently.

### 1.3 Why domain-constrained MoE

The MoE structure is natural for the evidence-gathering problem: different forecast questions involve different domains (macroeconomic, market, biotech, sports), and different specialist agents have demonstrated accuracy in different domains. A general-purpose agent handling all domains is less accurate than a routing system that assigns each driver to the most calibrated specialist.

The domain constraint is supplied by the output contract (§2.3): all specialist agents must produce outputs in the CEP format — a structured evidence summary with labeled findings and a probability multiplier with explicit confidence interval. This uniform output type enables the conductor to integrate specialist outputs into a single FPL model regardless of which specialists were used.

The calibration loop (§5) closes the MoE with ground truth: as forecast questions resolve, Brier scores accumulate per specialist agent, enabling Loop 5 routing to weight agents by demonstrated accuracy rather than declared capability alone.

---

## 2. The Conductor Agent

### 2.1 Role and responsibilities

The conductor agent (`fermi`) is the orchestrator of the Fermi MoE composition. It receives a forecast question and is responsible for:

1. **Decomposition**: Breaking the question into 3–6 independent drivers, each typed as either a continuous probability multiplier (range with p5/p50/p95) or a binary event (probability and conditional impact)
2. **Driver-to-specialist assignment**: Matching each driver to the most appropriate specialist agent based on domain alignment, capability contracts, and calibration history
3. **Synthesis**: Integrating specialist outputs into an FPL model and computing the forecast distribution
4. **CEP compliance**: Ensuring the output satisfies the Calibrated Evidence Protocol's structural requirements

The conductor does not perform domain research itself. It is explicitly a routing and integration agent, not a knowledge agent. Its value is in the quality of decomposition and the precision of agent assignment.

### 2.2 The decomposition procedure

The conductor follows a structured decomposition procedure grounded in Tetlock's outside-view methodology:

**Step 1 — Reference class identification:** Identify the most specific applicable reference class for the question. What is the historical base rate for this class of event? The base rate anchors the forecast before any case-specific reasoning is applied.

**Step 2 — Driver identification:** Identify the independent causal factors that would cause the actual outcome to differ from the base rate. Drivers must be orthogonal — they should not share causal pathways — to avoid double-counting.

**Step 3 — Driver typing:** Classify each driver as continuous (a probability multiplier: a factor that scales the base rate by a ratio distributed over some range) or binary (a discrete event with a probability and a conditional impact multiplier). Continuous drivers are appropriate when the factor varies smoothly; binary drivers are appropriate when the factor is a discrete switch.

**Step 4 — Agent assignment:** For each driver, select the specialist agent whose domain coverage best matches the driver's domain, using the driver taxonomy:

| Driver domain | Assigned specialist |
|---|---|
| Macroeconomic conditions, monetary policy, geopolitics | `macro_forecaster` |
| Market sizing, competitive dynamics, technology adoption | `market_research` |
| Public opinion, social media, sentiment | `sentiment_analyzer` |
| Entity behaviour, regulatory exposure, OSINT | `entity_investigator` |
| Public equity valuation, earnings, financial ratios | `equity_analyst` |
| Clinical trials, drug approval, biotech pipeline | `biotech_analyst` |
| NBA game outcomes, basketball metrics | `nba_analyst` |
| Soccer/football match outcomes | `football_analyst` |

**Step 5 — Query formulation:** For each driver, formulate a precise query to the assigned specialist: specify the exact metric, the output format (p5/p50/p95 multipliers relative to base rate), and the context from the main question needed for calibration.

### 2.3 The output contract

The Fermi MoE has a typed output contract: all specialist agents must produce outputs conforming to the Calibrated Evidence Protocol (CEP). CEP is enforced through the `fermi_contract` field on each specialist agent card, which declares:

- `finding_labels`: the structured labels the agent will use in its `key_findings` (e.g., `["BASE RATE", "INDICATOR", "POLICY", "MULTIPLIER"]` for `macro_forecaster`)
- `multiplier_range`: the permissible range for the agent's multiplier suggestions
- `kg_fact_categories`: the categories of knowledge graph facts the agent maintains in its memory
- `seed_facts`: initial knowledge graph entries seeded at agent creation

The CEP contract makes specialist outputs machine-parseable: the `extract_suggested_p50()` function in the execution pipeline reads the structured `key_findings` to extract the agent's probability estimate for use in constructing the FPL model. Without the contract, specialist outputs are free-form prose; with it, they are structured evidence packages.

---

## 3. Specialist Expert Agents

Each specialist agent is an ABW Agent configured for a specific forecasting domain. Their common structure:

- **System prompt** encoding domain expertise, calibration norms (reference class anchoring, uncertainty quantification), and output format requirements
- **CEP contract** specifying finding labels and multiplier range
- **Tool access** appropriate to domain (e.g., `equity_analyst` has access to Financial Modeling Prep API; `biotech_analyst` has access to BioPortal ontology)
- **KG seed facts** providing domain base rates that anchor the agent's reasoning before any case-specific evidence is gathered

### 3.1 Domain base rates as seed knowledge

The seed facts in each specialist's `fermi_contract.seed_facts` are not decorative — they are the outside-view anchors that Tetlock's methodology identifies as the primary determinant of forecast calibration. The `macro_forecaster`'s seed facts include:

- US recession base rate: 0.15 (annual probability, postwar average, source: NBER)
- PMI below 50 recession signal accuracy: 0.72 (source: ISM/NBER historical)
- Yield curve inversion false positive rate: 0.25 (source: Fed/NBER 1955–2023)
- Bear market average duration: 14 months (source: S&P 500 history 1950–2023)

These are not LLM-generated estimates. They are empirically grounded reference class frequencies that the agent reads from its own KG context before reasoning about any specific question. The agent's first response to any macroeconomic forecast question is anchored to these base rates; it then adjusts based on the specific evidence gathered.

This is the operational implementation of Tetlock's outside-view principle: *start with the base rate, then adjust*. The architecture makes the base rate explicit and separates it from the case-specific adjustment.

### 3.2 Calibration accumulation

Each specialist agent accumulates calibration data through Loop 5 (Section 5). As forecast questions that involved the agent resolve, Brier scores are written to the agent's `eval_signals` table and surfaced through the `get_agent_calibration` endpoint. The endpoint provides:

- `calibration_score`: composite calibration (0–1, higher = better calibrated)
- `trend`: improving / stable / degrading
- `domain_calibration`: per-domain breakdown keyed to the agent's tags
- `n_resolved_forecasts`: sample size for confidence calibration

The conductor reads these profiles in Stage 0 of its MoE routing cycle, weighting specialists by demonstrated calibration on domain-matched queries.

---

## 4. The Fermi Probabilistic Language

### 4.1 FPL as an integration language

FPL (Fermi Probabilistic Language) is the executable representation of a forecast model. It is the output of the conductor's synthesis step and the input to the Monte Carlo executor. FPL separates the forecaster's *model* (the causal structure of drivers and their relationships) from the *computation* (the Monte Carlo simulation that converts that model into a probability distribution).

A minimal FPL program:

```fpl
Question "Will X happen by date Y?" resolves 2026-12-31
  BaseRate 0.12 from "FDA historical approval rates 2000-2024" sample_size 847

Driver phase2_signal: Binary(0.70, 1.8)
  Evidence "Strong Phase 2 data, experienced team"

Driver market_size_risk: Triangular(0.85, 1.0, 1.2)
  Evidence "Market growing 35% YoY, 3 competitors"

Model BaseRate * phase2_signal * market_size_risk

Simulate 50000
```

This program encodes:
- A base rate from an empirical reference class (not elicited from the LLM)
- Two drivers with typed distributions (binary and triangular)
- A multiplicative model (the drivers scale the base rate)
- A simulation directive (50,000 Monte Carlo samples)

The executor produces: mean, median, standard deviation, p5, p25, p75, p95 of the outcome distribution, plus the Sobol sensitivity indices identifying which drivers contribute most to forecast variance.

### 4.2 Distribution types

FPL supports five distribution types, each with a specific interpretive commitment:

| Type | Parameters | Interpretation |
|---|---|---|
| `Triangular(p5, p50, p95)` | Three percentiles | Continuous multiplier with explicit uncertainty |
| `Normal(μ, σ)` | Mean and standard deviation | Symmetric uncertainty around a point estimate |
| `Lognormal(median, σ)` | Log-scale parameters | Right-skewed multiplier (cannot go negative) |
| `Beta(α, β)` | Shape parameters | Bounded probability (0–1); from empirical count data |
| `Binary(p, impact)` | Probability and conditional multiplier | Discrete event with impact if it occurs |

The choice of distribution type encodes the forecaster's belief about the driver's shape. `Triangular` is appropriate for human-elicited uncertainty ranges. `Beta` is appropriate when the distribution is derived from count data (successes/trials) — this is the distribution that BayesOps produces when fitting from historical observation data, enabling a clean substitution of analyst-elicited parameters with data-derived posteriors.

### 4.3 The BaseRate as epistemic anchor

The `BaseRate` declaration is structurally distinct from `Driver` declarations. It is the outside-view anchor — the reference class frequency that the forecast starts from before any case-specific adjustment. The `from` clause and `sample_size` field enforce transparency: a base rate must cite its source and sample size. A base rate derived from 847 historical cases carries more weight in subsequent Bayesian updating than one derived from 8 cases; the architecture makes this explicit.

The `BaseRate` feeds into the Brier divergence calculation: how much did the final forecast diverge from the reference class? Large divergence requires justification. The executor reports `divergence_relative` and `divergence_absolute` alongside the forecast distribution, making the degree of inside-view adjustment visible and auditable.

### 4.4 Sobol sensitivity analysis

The FPL executor computes first-order and total-order Sobol sensitivity indices (Sobol, 2001) over the driver distributions. For a forecast with drivers {D₁, D₂, ..., Dₙ}, the first-order Sobol index Sᵢ measures the fraction of forecast variance attributable to Dᵢ alone; the total-order index Tᵢ measures the fraction attributable to Dᵢ including all interactions.

This analysis serves two functions:

1. **Forecaster feedback**: "Your forecast is 73% driven by the phase2_signal driver. Improving confidence on that driver would reduce forecast uncertainty more than any other single action."

2. **Routing feedback**: Drivers with high Sobol indices on unresolved questions identify which specialist agents have the most impact on the forecast. When such forecasts resolve, the Brier contribution attributed to high-Sobol agents is larger, creating stronger calibration signal for those agents.

---

## 5. The Calibration Feedback Loop

### 5.1 Brier score as the ground truth signal

The Brier score (Brier, 1950) for a binary forecast question is:

*BS = (p − o)²*

where *p* ∈ [0,1] is the forecast probability and *o* ∈ {0,1} is the resolved outcome. A Brier score of 0 is perfect; 0.25 is equivalent to constant prediction of 50% (chance level). The Brier score is a *strictly proper scoring rule* (Gneiting & Raftery, 2007): it is uniquely minimised by reporting the true probability. An agent cannot improve its expected Brier score by misreporting its beliefs.

For distributional forecasts (where the outcome is a continuous variable rather than a binary event), the equivalent measure is Negative Log Predictive Density (NLPD), which penalises both incorrect means and miscalibrated uncertainty. AutoStan (arXiv:2603.27766) demonstrated that NLPD-guided model improvement produces correctly calibrated Bayesian models without domain-specific instruction.

### 5.2 The calibration loop mechanics

When a `fermi_forecasts` row resolves (resolution criteria met, outcome coded), the resolution handler:

1. Computes the Brier score between the forecast's `predicted_probability` and the binary outcome
2. Writes the score to `fermi_forecasts.brier_score`
3. Triggers `BrierEvaluator`: reads resolved forecasts for each agent in `agents_used`, updates `eval_signals.dimension = 'forecast_calibration'`
4. Annotates routing-decision episodes: finds episodes tagged `moe_routing_decision` from agents used in this forecast within the last 7 days, writes `outcome_quality = 1 − brier_score` to episode context
5. The routing strategist's next dreaming cycle consolidates annotated routing episodes into rules: "for macroeconomic questions, `macro_forecaster` has historically outperformed `sentiment_analyzer` by 0.12 Brier points"

### 5.3 Cold start and degradation

A Fermi composition without calibration data routes based on semantic matching against capability declarations. This is a reasonable prior: a specialist declared as a macroeconomic expert is more likely to produce accurate macroeconomic forecasts than a sports analyst. The architecture degrades gracefully to semantic matching at low data volume.

The calibration curve requires approximately 20 resolved forecasts per agent before confidence saturates (`BrierEvaluator` confidence weight: `min(n_resolved / 20, 1.0)`). At low n, calibration scores are informative but not actionable. The progression:

- **Phase 0–2 months:** Semantic matching (cold start)
- **Phase 2–4 months:** Emerging calibration signal (n = 5–20 per agent)
- **Phase 4+ months:** Calibrated routing (n > 20 per agent, cross-domain specialisation visible)

Historical backtest seeding — replaying known-resolved Polymarket events through specialist agents — bootstraps the calibration curve without waiting for new forecasts to resolve.

---

## 6. The FPL Executor as Loop B

### 6.1 Loop A / Loop B separation

A conceptually important distinction in the Fermi architecture is between Loop A (parameter fitting, offline) and Loop B (forecast simulation, online).

**Loop B** is the FPL executor: given a FPL program with distribution parameters, run Monte Carlo simulation and return the outcome distribution. Loop B is stateless — it takes a program and returns results. It does not learn, it does not adapt, it does not retain state between calls. The executor (`src/executor.rs`) is unchanged by any learning or calibration activity in the system.

**Loop A** (BayesOps, Labra, 2026d) is the offline parameter-fitting layer: given historical observations, fit a posterior distribution over outcome parameters and produce `Beta(α, β)` or `Normal(μ, σ)` parameters that can be written into FPL `Driver` declarations. Loop A feeds Loop B — its outputs are the parameters Loop B runs with — but it operates on a different timescale (triggered by data accumulation) and is not part of the real-time forecast execution path.

This separation is architecturally deliberate. It means that the FPL executor, the AST, and the parser are stable across all learning activity in the system. Parameter improvements are explicit: they appear as changed numbers in FPL programs, not as invisible weight updates inside a black-box model.

### 6.2 Sobol over analyst-elicited vs data-derived parameters

The FPL executor's Sobol sensitivity analysis has different interpretive value depending on whether driver parameters were analyst-elicited or data-derived:

- **Analyst-elicited parameters** (current state): Sobol indices identify which of the analyst's uncertainty ranges drives the forecast. High Sobol index on a driver means "collecting better evidence on this driver would most reduce your uncertainty." This guides the conductor's next round of evidence gathering.

- **Data-derived parameters** (with BayesOps): Sobol indices identify which input variables drive outcome variance in the fitted model. High Sobol index on a driver means "this variable causally dominates outcomes in your operational history." This guides process design and investment decisions — not just evidence gathering.

The transition from analyst-elicited to data-derived parameters changes the interpretation of Sobol results from epistemic (what we don't know) to causal (what actually drives outcomes). This is a qualitative shift in the forecast's meaning.

---

## 7. Relationship to Prior Work

### 7.1 Tetlock's superforecasting methodology

Tetlock & Gardner (2015) identify several practices that distinguish accurate ("superforecaster") from inaccurate forecasters: breaking questions into independent sub-questions, seeking reference classes, updating beliefs incrementally as evidence arrives, and maintaining explicit calibration metrics. Fermi's architecture operationalises these practices as structural constraints:

- Decomposition is mandatory (the conductor must identify 3–6 drivers)
- Base rates are explicitly declared with sources (the `BaseRate` directive)
- Driver distributions are typed and uncertainty-explicit (no point estimates)
- Brier scoring makes calibration a measured property, not a subjective assessment

### 7.2 AutoStan and NLPD-guided model improvement

AutoStan (arXiv:2603.27766) demonstrates that an agent iteratively improving a Stan statistical model guided by NLPD on held-out data discovers structurally correct models — hierarchical pooling, outlier robustness, heteroscedastic variance — without domain-specific instruction. The mechanism is identical to Fermi's calibration loop: a strictly proper scoring rule applied to held-out ground truth guides model improvement without instructing the agent on what to change.

The connection is direct: Fermi's FPL `Driver` distributions are the Bayesian prior distributions that AutoStan's models infer from data. When BayesOps (Labra, 2026d) is deployed, the analyst-elicited driver distributions are replaced by data-fitted posteriors using the same NLPD-guided improvement logic. The difference is substrate: AutoStan uses Stan/MCMC; BayesOps uses conjugate updates and (in Phase 2) HMC in pure Rust.

### 7.3 Mixture-of-Experts architectures

The sparse MoE architecture in large language models (Shazeer et al., 2017; Fedus et al., 2022) routes tokens to expert sub-networks via a gating function. Fermi's MoE shares the routing principle but differs in key respects:

- **Routing is at the semantic level**, not the token level: the conductor classifies the forecast question's domain, not individual tokens
- **Experts are independent agents**, not weight matrices: each expert has its own memory, calibration history, and domain knowledge
- **The output contract is typed**: all experts produce CEP-structured outputs, not arbitrary activations
- **Routing is calibration-corrected**: the gating function incorporates historical accuracy, not just learned attention weights

This is a *logical* MoE — the experts are reasoning systems, not differentiable functions — and the routing is *evidence-based*, not gradient-based.

---

## 8. Open Questions

### 8.1 Optimal decomposition granularity

The conductor is instructed to identify 3–6 drivers. This range was chosen heuristically based on Tetlock's practice norms. Whether this is the optimal range for forecast calibration, or whether it depends on question complexity, is an empirical question. Systematic comparison of 2-driver vs 4-driver vs 6-driver forecasts on a common question set would address this.

### 8.2 Driver orthogonality verification

The decomposition procedure instructs the conductor to identify orthogonal drivers, but orthogonality is not mechanically verified. Two drivers may share causal pathways that are not apparent at decomposition time, leading to double-counting. The FPL model structure (multiplicative) partially mitigates this: in a multiplicative model, correlated drivers produce less forecast variance than in an additive model. But it does not eliminate the problem. A formal orthogonality check — using driver correlation estimated from the specialist agents' evidence outputs — would improve decomposition quality.

### 8.3 Calibration of the calibration signal

The Brier score measures calibration on resolved binary questions. Many forecast questions resolve on continuous outcomes (e.g., GDP growth next year), which are typically binarised for scoring (e.g., "will GDP growth exceed 2%?"). The binarisation threshold affects the measured calibration. Whether specialist agents are calibrated across threshold choices, or only at the specific threshold used for scoring, is an open question relevant to the routing loop's signal quality.

### 8.4 Multi-specialist synthesis

The current synthesis procedure is multiplicative: driver distributions are combined via the FPL model expression. This is appropriate when drivers are independent. When drivers are correlated — macro conditions and market sentiment are not independent — the multiplicative combination overestimates forecast confidence. A copula-based synthesis that allows for specified driver correlations would improve calibration at the cost of model complexity.

---

## 9. Conclusion

Fermi demonstrates the domain-constrained MoE pattern in the probabilistic forecasting domain. Its distinctive contribution is the separation of decomposition, evidence gathering, and probabilistic integration into structurally distinct components, each improvable independently:

- The conductor improves through better decomposition norms (human-refinable) and calibration-weighted routing (Loop 5)
- Specialist agents improve through domain knowledge accumulation (Loop 1) and calibration feedback (Loop 5)
- The FPL executor is stable — it is improved by better distribution parameters (BayesOps, Loop A) rather than by changing the simulation mechanism

The architecture makes calibration a measurable, improvable property of the system rather than an implicit characteristic of individual LLM outputs. The Brier score provides the strictly proper scoring rule that grounds the feedback loop in epistemic honesty: an agent cannot improve its long-run calibration score by misreporting its beliefs.

The formal open questions — optimal decomposition granularity, orthogonality verification, continuous outcome calibration, multi-specialist synthesis under correlation — define the research agenda that operational deployment will generate data to address.

---

## References

1. Brier, G. W. (1950). Verification of forecasts expressed in terms of probability. *Monthly Weather Review*, 78(1), 1–3.

2. Fedus, W., et al. (2022). Switch Transformers: Scaling to Trillion Parameter Models with Simple and Efficient Sparsity. *JMLR*, 23(120), 1–39.

3. Gneiting, T., & Raftery, A. E. (2007). Strictly proper scoring rules, prediction, and estimation. *Journal of the American Statistical Association*, 102(477), 359–378.

4. Labra, I. (2026a). Agent Bestiary World: A Logical Architecture for Recursive Self-Improving Multi-Agent Systems. `docs/papers/abw_logical_architecture.md`

5. Labra, I. (2026b). ABW Distribution Topology — Design Proposal. `docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md`

6. Labra, I. (2026c). Explanatory Coherence Modeling as an Improvement Loop in Agent-to-Agent and Agent-to-Human Collaboration. `docs/papers/coherence_improvement_loop.md`

7. Labra, I. (2026d). BayesOps: Data-Informed Distribution Fitting for Fermi. `docs/specs/14_BAYESOPS_SPEC.md`

8. Shazeer, N., et al. (2017). Outrageously Large Neural Networks: The Sparsely-Gated Mixture-of-Experts Layer. *ICLR 2017*.

9. Sobol, I. M. (2001). Global sensitivity indices for nonlinear mathematical models and their Monte Carlo estimates. *Mathematics and Computers in Simulation*, 55(1–3), 271–280.

10. Tetlock, P. E. (2005). *Expert Political Judgment: How Good Is It? How Can We Know?* Princeton University Press.

11. Tetlock, P. E., & Gardner, D. (2015). *Superforecasting: The Art and Science of Prediction*. Crown Publishers.

12. arXiv:2603.27766. AutoStan: Automated Bayesian Statistical Modelling. *Preprint, 2026*.

---

*Working paper. Not for external distribution without author consent.*
