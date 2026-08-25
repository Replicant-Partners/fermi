# Fermi: Probabilistic Forecasting as a Domain-Constrained Mixture-of-Experts System

**Authors:** Ivan Labra — axelotl partners  
**Version:** 2.0 — June 2026  
**Status:** Working paper

---

## Abstract

Fermi is a probabilistic forecasting system built as a domain-constrained Mixture-of-Experts (MoE) composition. Its output contract is a probability distribution; its calibration signal is the Brier score computed against resolved ground truth; and its prior elicitation methodology is grounded in structured decomposition principles drawn from the superforecasting literature. This paper describes Fermi as a *logical* system: the Calibrated Evidence Protocol (CEP) that defines its output contract, the decomposition-and-routing procedure by which a conductor agent assigns each forecast driver to the most appropriate domain specialist, the FPL (Fermi Probabilistic Language) that encodes a forecast as an executable Monte Carlo program, and the calibration feedback loop that enables routing accuracy to improve over time. The architecture separates three problems that are typically conflated in forecasting systems — decomposition (what are the independent drivers?), evidence gathering (what does each driver's domain evidence suggest?), and probabilistic integration (how do drivers combine into a forecast distribution?) — and it is this separation that enables systematic, measurable improvement. A planned extension, BayesOps, will replace analyst-elicited driver distributions with distributions fitted from historical observation data; this paper describes BayesOps as an architectural component and specifies its design, but it is not yet implemented.

**Keywords:** probabilistic forecasting, mixture of experts, Brier score, Monte Carlo simulation, domain-constrained MoE, calibration

---

## 1. Introduction

### 1.1 The forecasting problem

Probabilistic forecasting — assigning calibrated probability distributions to future events — has a well-established empirical foundation. Forecasting tournaments have shown that human forecasters can achieve significantly better-than-chance calibration when they adopt structured practices: identify independent drivers, anchor to historical base rates, apply Bayesian updating as evidence arrives, and maintain explicit uncertainty quantification. These practices are the methodological foundation of the superforecasting discipline.

The computational challenge is to instantiate these practices at scale in a multi-agent system. A single language model can produce a probability estimate for a forecast question, but such estimates are typically neither decomposed into independent drivers, nor anchored to historical base rates, nor explicitly uncertainty-quantified. They produce numbers without the epistemic scaffolding that makes those numbers trustworthy or improvable.

Fermi addresses this by decomposing the forecasting problem into three structurally distinct sub-problems and assigning each to a specialised component.

### 1.2 Three separable problems

**Problem 1 — Decomposition:** Given a forecast question, what are the independent drivers of its outcome probability? Drivers must be orthogonal (no double-counting causal paths), bounded in number (3–6 for tractable integration), and typed (continuous probability multipliers vs. discrete binary events). This is a structural problem: it requires understanding the causal structure of the domain.

**Problem 2 — Evidence gathering:** For each identified driver, what does the current domain evidence suggest about its distribution? This is a domain knowledge problem: it requires access to current data, historical base rates, and domain-specific expertise.

**Problem 3 — Probabilistic integration:** Given a set of driver distributions, how do they combine to produce a forecast distribution for the outcome? This is a mathematical problem: it requires Monte Carlo simulation over the joint distribution of drivers, producing a probability distribution over outcomes with correct uncertainty propagation.

Fermi assigns each problem to a separate component. The conductor agent (§2) handles decomposition. Domain-specialist expert agents (§3) handle evidence gathering. The FPL executor (§4) handles probabilistic integration. The three components interact through well-defined interfaces, enabling each to be improved independently.

### 1.3 Why domain-constrained MoE

The Mixture-of-Experts structure is natural for the evidence-gathering problem: different forecast questions involve different domains (macroeconomic, market, biotech, sports), and specialist agents accumulate demonstrated accuracy in specific domains. A general-purpose agent handling all domains is less accurate than a routing system that assigns each driver to the most calibrated specialist.

The domain constraint is supplied by the output contract (§2.3): all specialist agents must produce outputs in the CEP format — a structured evidence summary with labeled findings and a probability multiplier with an explicit confidence interval. This uniform output type enables the conductor to integrate specialist outputs into a single FPL model regardless of which specialists were used.

The calibration feedback loop (§5) closes the MoE with ground truth: as forecast questions resolve, Brier scores accumulate per specialist agent, enabling routing to weight agents by demonstrated accuracy rather than declared capability alone. This routing improvement is gradual and data-dependent — it is a design property of the architecture, not an immediate capability.

---

## 2. The Conductor Agent

### 2.1 Role and responsibilities

The conductor agent (`fermi`) is the orchestrator of the Fermi MoE composition. It receives a forecast question and is responsible for:

1. **Decomposition**: Breaking the question into 3–6 independent drivers, each typed as either a continuous probability multiplier (range with p5/p50/p95) or a binary event (probability and conditional impact)
2. **Driver-to-specialist assignment**: Matching each driver to the most appropriate specialist agent based on domain alignment and, as forecasts accumulate, calibration history
3. **Synthesis**: Integrating specialist outputs into an FPL model and computing the forecast distribution
4. **CEP compliance**: Ensuring the output satisfies the Calibrated Evidence Protocol's structural requirements

The conductor does not perform domain research itself. It is a routing and integration agent, not a knowledge agent. Its value is in the quality of decomposition and the precision of agent assignment.

### 2.2 The decomposition procedure

The conductor follows a structured decomposition procedure:

**Step 1 — Reference class identification:** Identify the most specific applicable reference class for the question. What is the historical base rate for this class of event? The base rate anchors the forecast before any case-specific reasoning is applied.

**Step 2 — Driver identification:** Identify the independent causal factors that would cause the actual outcome to differ from the base rate. Drivers must be orthogonal — they should not share causal pathways — to avoid double-counting variance contributions.

**Step 3 — Driver typing:** Classify each driver as continuous (a probability multiplier: a factor that scales the base rate by a ratio distributed over some range) or binary (a discrete event with a probability and a conditional impact multiplier). Continuous drivers are appropriate when the factor varies smoothly; binary drivers are appropriate when the factor is a discrete switch.

**Step 4 — Agent assignment:** For each driver, select the specialist agent whose domain coverage best matches the driver's domain. The current specialist roster and their assignment domains:

| Driver domain | Assigned specialist |
|---|---|
| Macroeconomic conditions, monetary policy, geopolitics | `macro_forecaster` |
| Market sizing, competitive dynamics, technology adoption | `market_research` |
| Public opinion, social media, narrative sentiment | `sentiment_analyzer` |
| Entity behaviour, regulatory exposure, OSINT | `entity_investigator` |
| Public equity valuation, earnings, financial ratios | `equity_analyst` |
| Clinical trials, drug approval, biotech pipeline | `biotech_analyst` |
| NBA game outcomes, basketball metrics | `nba_analyst` |
| Soccer/football match outcomes | `football_analyst` |
| Supply chain risk, logistics, procurement | `supply_chain_oracle` |
| Regulatory landscape, legal risk, compliance | `regulatory_scanner` |

This roster is a proof-of-concept instantiation; the MoE architecture is domain-agnostic. Any domain with a structured evidence base and a CEP-compliant output contract can be added as a specialist without changes to the conductor or the FPL executor. Section 8 discusses the specialist expansion design.

**Step 5 — Query formulation:** For each driver, formulate a precise query to the assigned specialist: specify the exact metric, the output format (p5/p50/p95 multipliers relative to base rate), and the context from the main question needed for calibration.

### 2.3 The output contract

The Fermi MoE has a typed output contract: all specialist agents must produce outputs conforming to the Calibrated Evidence Protocol (CEP). CEP is enforced through the `fermi_contract` field on each specialist agent card, which declares:

- `finding_labels`: the structured labels the agent uses in its `key_findings` (e.g., `["BASE RATE", "INDICATOR", "POLICY", "MULTIPLIER"]` for `macro_forecaster`)
- `multiplier_range`: the permissible range for the agent's multiplier outputs
- `kg_fact_categories`: the categories of knowledge graph facts the agent maintains in its memory
- `seed_facts`: initial knowledge graph entries seeded at agent creation, providing domain base rates

The CEP contract makes specialist outputs machine-parseable: the `extract_suggested_p50()` function in the execution pipeline reads the structured `key_findings` to extract the agent's probability estimate for use in constructing the FPL model. Without the contract, specialist outputs are free-form prose; with it, they are structured evidence packages that the conductor can integrate algorithmically.

---

## 3. Specialist Expert Agents

Each specialist agent is a reasoning agent configured for a specific forecasting domain. Their common structure:

- **System prompt** encoding domain expertise, calibration norms (reference class anchoring, uncertainty quantification), and output format requirements
- **CEP contract** specifying finding labels and multiplier range
- **Tool access** appropriate to domain (e.g., `equity_analyst` has access to Financial Modeling Prep API; `biotech_analyst` has access to BioPortal ontology)
- **Seed facts** providing domain base rates that anchor the agent's reasoning before any case-specific evidence is gathered

### 3.1 Domain base rates as seed knowledge

The seed facts in each specialist's `fermi_contract.seed_facts` are the outside-view anchors that structured forecasting methodology identifies as the primary determinant of forecast calibration. They are the reference class frequencies a rational forecaster should start from before adjusting for the specifics of a particular question. The `macro_forecaster`'s seed facts include:

- US recession base rate: 0.15 (annual probability, postwar average, source: NBER)
- PMI below 50 recession signal accuracy: 0.72 (source: ISM/NBER historical)
- Yield curve inversion false positive rate: 0.25 (source: Fed/NBER 1955–2023)
- Bear market average duration: 14 months (source: S&P 500 history 1950–2023)

These are not LLM-generated estimates. They are empirically grounded reference class frequencies that the agent reads from its own context before reasoning about any specific question. The agent's first response to any macroeconomic forecast question is anchored to these base rates; it then adjusts based on the specific evidence gathered.

This is the operational implementation of the outside-view principle: *start with the base rate for the reference class, then apply case-specific adjustments*. The architecture makes the base rate explicit and structurally separates it from case-specific reasoning.

### 3.2 Calibration accumulation

Each specialist agent accumulates calibration data as forecast questions resolve. The Brier score for each resolved forecast is attributed to the specialist agents that contributed drivers to that forecast, creating a per-agent accuracy record. This record is surfaced through the `get_agent_calibration` endpoint, which provides:

- `calibration_score`: composite calibration (0–1, higher = better calibrated)
- `trend`: improving / stable / degrading
- `domain_calibration`: per-domain breakdown keyed to the agent's tags
- `n_resolved_forecasts`: sample size for confidence weighting

The conductor reads these profiles when selecting agents for new forecasts, weighting specialists by demonstrated calibration on domain-matched queries. In the system's early life, before sufficient calibration data accumulates, routing falls back to semantic matching against capability declarations — see §5.3 for the cold-start progression.

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

The executor produces: mean, median, standard deviation, p5, p25, p75, p95 of the outcome distribution, plus Sobol sensitivity indices identifying which drivers contribute most to forecast variance.

### 4.2 Distribution types

FPL supports five distribution types, each with a specific interpretive commitment:

| Type | Parameters | Interpretation |
|---|---|---|
| `Triangular(p5, p50, p95)` | Three percentiles | Continuous multiplier with explicit uncertainty bounds |
| `Normal(μ, σ)` | Mean and standard deviation | Symmetric uncertainty around a point estimate |
| `Lognormal(median, σ)` | Log-scale parameters | Right-skewed multiplier (cannot go negative) |
| `Beta(α, β)` | Shape parameters | Bounded probability (0–1); appropriate for count-derived estimates |
| `Binary(p, impact)` | Probability and conditional multiplier | Discrete event with impact if it occurs |

The choice of distribution type encodes the forecaster's belief about the driver's shape. `Triangular` is appropriate for human-elicited uncertainty ranges where three representative percentiles are the natural way to express confidence. `Beta` is appropriate when the distribution is derived from count data (successes/trials) — this is the distribution that BayesOps (§6) is designed to produce when fitting from historical observations, enabling a clean substitution of analyst-elicited parameters with data-derived posteriors.

### 4.3 The BaseRate as epistemic anchor

The `BaseRate` declaration is structurally distinct from `Driver` declarations. It is the outside-view anchor — the reference class frequency that the forecast starts from before any case-specific adjustment. The `from` clause and `sample_size` field enforce transparency: a base rate must cite its source and sample size. A base rate derived from 847 historical cases carries more evidential weight than one derived from 8; the architecture makes this explicit and forces the forecaster to confront it.

The `BaseRate` feeds into the divergence calculation: how much did the final forecast diverge from the reference class? Large divergence represents a strong inside-view adjustment and requires justification. The executor reports `divergence_relative` and `divergence_absolute` alongside the forecast distribution, making the degree of adjustment visible and auditable.

### 4.4 Sobol sensitivity analysis

The FPL executor computes first-order and total-order Sobol sensitivity indices over the driver distributions. For a forecast with drivers {D₁, D₂, ..., Dₙ}, the first-order Sobol index Sᵢ measures the fraction of forecast variance attributable to Dᵢ alone; the total-order index Tᵢ measures the fraction attributable to Dᵢ including all interactions with other drivers.

This analysis serves two functions:

1. **Forecaster guidance**: "Your forecast variance is 73% driven by the `phase2_signal` driver. Collecting better evidence on that driver would reduce forecast uncertainty more than any other single action."

2. **Calibration weighting**: Drivers with high Sobol indices identify which specialist agents have the largest impact on a given forecast. When such forecasts resolve, the Brier contribution attributed to high-Sobol agents carries more signal weight, creating stronger and more targeted calibration feedback for those agents.

---

## 5. The Calibration Feedback Loop

### 5.1 Brier score as the ground truth signal

The Brier score for a binary forecast question is:

*BS = (p − o)²*

where *p* ∈ [0,1] is the forecast probability and *o* ∈ {0,1} is the resolved outcome. A Brier score of 0 is perfect; 0.25 is equivalent to constant prediction of 50% (chance level). The Brier score is a *strictly proper scoring rule*: it is uniquely minimised by reporting the true probability. An agent that systematically misreports its beliefs will have a higher expected Brier score than one that reports honestly — the scoring rule makes honesty the optimal strategy.

This property is critical for the calibration loop. Because the Brier score is strictly proper, improving an agent's Brier score requires actually improving its probability estimates, not gaming the metric. The loop's ground truth signal is therefore epistemically clean.

For distributional forecasts — where the outcome is a continuous variable — the equivalent measure is Negative Log Predictive Density (NLPD), which penalises both incorrect means and miscalibrated uncertainty. NLPD is the signal that BayesOps (§6) uses when it is operational.

### 5.2 The calibration loop mechanics

When a forecast question resolves (resolution criteria met, outcome coded), the resolution handler:

1. Computes the Brier score between the forecast's `predicted_probability` and the binary outcome
2. Stores the score against the forecast record
3. Attributes Brier contributions to each specialist agent involved in the forecast, weighted by that agent's drivers' Sobol indices (high-impact drivers receive proportionally more attribution)
4. Updates each agent's calibration profile: rolling Brier average, trend direction, domain-segmented accuracy
5. The conductor reads updated calibration profiles on subsequent routing decisions, progressively weighting specialists by demonstrated accuracy on domain-matched questions

The attribution by Sobol index is a design choice: it ensures that agents whose drivers dominated the forecast outcome receive stronger learning signal than agents whose drivers had negligible variance contribution.

### 5.3 Cold start and calibration maturity

A Fermi deployment without calibration data routes based on semantic matching against capability declarations. This is a reasonable prior: a specialist declared as a macroeconomic expert is more likely to produce accurate macroeconomic forecasts than a sports analyst. The system degrades gracefully to semantic matching at low data volume.

Calibration profiles gain actionable signal at approximately 20 resolved forecasts per agent (below this threshold, calibration estimates are informative but have wide confidence intervals). The progression has three phases:

- **Phase 0–2 months:** Semantic-only routing. Agents are assigned based on domain alignment; no calibration weighting.
- **Phase 2–4 months:** Emerging calibration. Brier data begins accumulating; routing starts incorporating calibration signals alongside semantic matching, but with low confidence weight.
- **Phase 4+ months:** Calibrated routing. Sufficient resolved forecasts per agent for calibration to dominate routing decisions; cross-domain specialisation differentials become visible.

The cold-start period can be shortened by replaying known-resolved historical forecast questions through specialist agents — bootstrapping calibration curves without waiting for new forecasts to resolve.

---

## 6. Two Monte Carlo Loops: Simulation and Parameter Fitting

### 6.1 The architectural separation

A conceptually important distinction in the Fermi architecture is between two entirely separate Monte Carlo loops that serve different purposes and operate on different timescales.

**The simulation loop — Forecast Simulation (online, per question):** The FPL executor. Given a FPL program with distribution parameters, it runs Monte Carlo simulation and returns the outcome distribution. The simulation loop is stateless — it takes a program and returns results. It does not learn, does not adapt, and does not retain state between calls. The executor is unchanged by any learning or calibration activity in the system. This loop runs on every forecast question, in real time.

**The fitting loop — Parameter Fitting (offline, per data accumulation):** BayesOps. Given a historical observation dataset, it fits a posterior distribution over outcome parameters and produces typed distribution parameters (`Beta(α, β)`, `Normal(μ, σ)`, etc.) that can be written into FPL `Driver` declarations. The fitting loop is triggered by data accumulation, not by forecast requests. It operates on a longer timescale and its outputs are explicit: changed parameter numbers in FPL programs, not invisible weight updates inside a model.

The fitting loop feeds the simulation loop — its output is the parameters the simulation loop runs with — but they are fully independent in operation. The seam between them is the `Distribution` type in the FPL AST. Whether driver parameters were typed by a human analyst or produced by BayesOps, the simulation loop treats them identically.

This separation is architecturally deliberate. It means parameter improvements are explicit and auditable: every change to a forecast model appears as a change to specific numbers in a specific FPL program, with a documented source (analyst judgment or fitted posterior). There are no opaque weight updates.

### 6.2 BayesOps: design and current status

BayesOps is the fitting-loop component. Its specification is complete; implementation is not yet started. When deployed, it will:

1. Accept a historical observation dataset as input (e.g., historical outcomes for a class of events)
2. Fit a posterior distribution using conjugate methods (Phase 1) or Hamiltonian Monte Carlo (Phase 2+)
3. Produce typed distribution parameters with uncertainty quantification (`n_eff`, NLPD, confidence interval)
4. Expose a `to_fpl_params()` method that writes the fitted parameters directly into FPL `Driver` declarations

The design priority for Phase 1 is simple marginal fitting: given a sequence of binary outcomes, produce a `Beta(α, β)` posterior over the success rate. This is the direct computational implementation of the outside-view principle — the `Beta` posterior is exactly what Bayesian updating of a reference class frequency produces.

Phase 2 extends to conditional distributions (regression): given input features and outcomes, produce `P(outcome | inputs)` as a parametric distribution. This enables the Sobol analysis to shift from epistemic (which of the analyst's uncertainty ranges dominates?) to causal (which input features causally dominate outcomes in the operational record?).

### 6.3 Interpretive shift from analyst-elicited to data-derived parameters

The Sobol sensitivity analysis has different interpretive value depending on whether driver parameters were analyst-elicited or data-derived:

- **Analyst-elicited parameters (current state):** Sobol indices identify which of the analyst's uncertainty ranges drives forecast variance. High Sobol index on a driver means "collecting better evidence on this driver would most reduce your uncertainty." This guides the conductor's next round of evidence gathering.

- **Data-derived parameters (with BayesOps):** Sobol indices identify which input variables drive outcome variance in the fitted model. High Sobol index on a driver means "this variable causally dominates outcomes in the operational record." This guides process design and investment decisions, not just evidence gathering.

The transition from analyst-elicited to data-derived parameters changes the interpretation of Sobol results from epistemic (what we don't know) to causal (what actually drives outcomes). This is a qualitative shift in what the forecast means and how it should be acted upon.

---

## 7. Relationship to Prior Methodologies

### 7.1 Structured decomposition in forecasting

The superforecasting literature identifies several practices that distinguish accurate from inaccurate forecasters: breaking questions into independent sub-questions, seeking reference classes, updating beliefs incrementally as evidence arrives, and maintaining explicit calibration metrics. Fermi's architecture operationalises these practices as structural constraints rather than guidelines:

- Decomposition is mandatory (the conductor must identify 3–6 drivers; single-number guesses are not valid FPL programs)
- Base rates are explicitly declared with sources (the `BaseRate` directive with `from` and `sample_size` fields)
- Driver distributions are typed and uncertainty-explicit (FPL has no syntax for point estimates without uncertainty)
- Brier scoring makes calibration a measured property of the system, not a subjective assessment of individual responses

The key difference from human superforecasting practice is that Fermi enforces these constraints structurally. A superforecaster can skip steps under time pressure; the FPL executor will not run a program that omits a `BaseRate` declaration.

### 7.2 NLPD-guided model improvement

The pattern of using a strictly proper scoring rule applied to held-out ground truth to guide model improvement — without instructing the model on what to change — is well-established in the Bayesian statistical literature. The mechanism is: score the model's predictive distribution against held-out outcomes, identify which structural changes reduce NLPD, iterate. This is exactly the loop Fermi implements for binary forecasts (Brier score) and what BayesOps will implement for distributional forecasts (NLPD).

The connection between Brier-guided agent calibration and NLPD-guided parameter fitting is not coincidental — both are instances of the same principle applied at different levels of the architecture. The calibration loop (§5) operates at the agent routing level; BayesOps operates at the distribution parameter level. Both use strictly proper scoring rules to ensure that improving the score requires actually improving the model.

### 7.3 Mixture-of-Experts architectures

Sparse MoE architectures in language model research route tokens to expert sub-networks via a gating function. Fermi's MoE shares the routing principle but differs in several respects:

- **Routing is at the semantic level**, not the token level: the conductor classifies forecast question domains, not individual tokens
- **Experts are independent reasoning agents**, not weight matrices: each expert has its own context, calibration history, and domain knowledge
- **The output contract is typed**: all experts produce CEP-structured outputs, not arbitrary activations
- **Routing is calibration-corrected**: the gating function incorporates historical accuracy, not just learned attention weights
- **Experts are human-interpretable**: each expert's reasoning is legible prose, not a learned vector transformation

This is a *logical* MoE — the experts are reasoning systems, not differentiable functions — and the routing is *evidence-based*, not gradient-based.

---

## 8. Architecture for Expansion

The current specialist roster is a proof-of-concept. The MoE architecture is explicitly designed to accommodate new specialist domains without structural changes to the conductor, FPL, or calibration loop. A new specialist requires only: a system prompt encoding domain expertise, a CEP contract specifying finding labels and multiplier range, domain-appropriate tool access, and seed facts providing reference class base rates.

### 8.1 Specialist expansion directions

Several specialist domains are natural extensions of the current roster and have clear base rate libraries to draw from:

**Clinical / epidemiological:** Extending `biotech_analyst` with epidemiological base rates (disease transmission rates, vaccine efficacy reference distributions, outbreak duration distributions). The FDA approval pipeline is already covered; the extension is to population health forecasting.

**Real estate and infrastructure:** Market-level real estate forecasting (price appreciation distributions, vacancy rate base rates, development completion rates). The driver structure maps naturally to macro conditions × local supply/demand × regulatory environment.

**Policy and regulatory outcomes:** Legislative passage probabilities, regulatory approval timelines, court outcome base rates. Entity-level regulatory risk is already covered by `entity_investigator`; the extension is to systemic policy forecasting.

**Technology diffusion:** S-curve adoption modelling for new technologies, with reference class base rates drawn from historical technology adoption histories. Distinct from general market research in that the primary model is a diffusion curve with uncertainty over inflection timing and ceiling.

**Geopolitical events:** Conflict escalation probabilities, treaty outcome distributions, election results. `macro_forecaster` covers macroeconomic geopolitical risk; dedicated geopolitical specialists would carry military, diplomatic, and historical conflict base rates that are outside `macro_forecaster`'s current seed facts.

### 8.2 The specialist interface is the expansion constraint

The CEP output contract is intentionally minimal: labeled findings, a multiplier with confidence interval, and a base rate citation. Any domain that can produce estimates in this format is immediately composable with the conductor and the FPL executor. The expansion constraint is not architectural — it is epistemological: a new specialist is only as useful as the quality of its base rate library and its domain-specific calibration.

The calibration loop (§5) provides a natural quality gate: a specialist that consistently produces poorly calibrated multipliers will be de-weighted in routing over time, regardless of how confidently it expresses its estimates. This creates a selection pressure for specialists with genuine domain knowledge rather than plausible-sounding prose.

---

## 9. Open Research Questions

The following questions are not implementation details — they are architectural claims that require empirical evidence from operational deployment to resolve. Each is stated as a testable claim.

### 9.1 Optimal decomposition granularity

**Claim:** Forecast calibration is not monotonically increasing in the number of drivers. There is an optimal range (hypothesised: 3–6) beyond which additional drivers introduce more correlation noise than independent signal.

**Test:** Systematic comparison of 2-driver, 4-driver, and 6-driver forecasts on a common question set with resolved outcomes. Measure: Brier score by driver count, controlling for question complexity.

**Architectural implication:** If the optimal range is question-type-dependent, the conductor should estimate optimal driver count from question complexity signals before decomposing, rather than applying a fixed 3–6 rule.

### 9.2 Driver orthogonality verification

**Claim:** The decomposition procedure's instruction to identify orthogonal drivers is insufficient without mechanical orthogonality checking. Correlated drivers in a multiplicative model produce overconfident (narrow) forecast distributions.

**Test:** Measure correlation between specialist agents' multiplier outputs on the same forecast questions over many forecasts. High correlation between nominally independent drivers indicates a systematic decomposition failure mode.

**Architectural implication:** A correlation estimator over specialist outputs — using the cross-correlation matrix of multiplier suggestions as an orthogonality signal — could flag decompositions with high driver correlation before they propagate to the FPL model. The conductor should receive this signal and re-decompose when correlation exceeds a threshold.

### 9.3 Calibration signal quality under binarisation

**Claim:** Many forecast questions resolve on continuous outcomes (e.g., GDP growth, price levels) and are binarised for Brier scoring (e.g., "will GDP growth exceed 2%?"). Agent calibration may be threshold-sensitive: an agent calibrated at the 2% threshold may not be calibrated at the 1% or 3% threshold.

**Test:** For continuous-outcome questions, score agents at multiple binarisation thresholds. Measure: variance in calibration score as a function of threshold.

**Architectural implication:** Threshold-robust calibration requires either distributional scoring (NLPD over the full forecast distribution) or calibration profiling across multiple thresholds. BayesOps's NLPD-based evaluation is designed to address this; the claim motivates the transition from Brier to NLPD as the primary calibration signal.

### 9.4 Multi-specialist synthesis under driver correlation

**Claim:** The current multiplicative synthesis model assumes driver independence. When drivers are correlated — macroeconomic conditions and market sentiment move together — the multiplicative combination underestimates joint uncertainty.

**Test:** Estimate the empirical correlation between `macro_forecaster` and `sentiment_analyzer` multipliers on the same questions over many forecasts. Compare forecast Brier scores on questions where these agents were used jointly against questions where only one was used.

**Architectural implication:** A copula-based synthesis model would allow the conductor to specify driver correlation structure, producing forecast distributions with correct joint uncertainty. The implementation path is: extend FPL with a `Correlation` declaration, and extend the executor to sample from a correlated joint distribution rather than independently. The CEP output contract would remain unchanged — the correlation structure is specified at the FPL level, not at the specialist output level.

---

## 10. Conclusion

Fermi demonstrates the domain-constrained MoE pattern in the probabilistic forecasting domain. Its distinctive contribution is the separation of decomposition, evidence gathering, and probabilistic integration into structurally distinct components, each improvable independently and through different mechanisms:

- The conductor improves through better decomposition norms (human-refinable through prompt revision) and through calibration-weighted routing (machine-improvable through the feedback loop in §5)
- Specialist agents improve through domain knowledge accumulation and calibration feedback
- The FPL executor is stable — it is improved by better distribution parameters (BayesOps, when deployed) rather than by changing the simulation mechanism itself

The architecture makes calibration a measurable, improvable property of the system rather than an implicit characteristic of individual language model outputs. The Brier score provides the strictly proper scoring rule that grounds the feedback loop in epistemic honesty: an agent cannot improve its long-run calibration score by misreporting its beliefs.

The key architectural claim is that the separation of these three problems is not merely organisational convenience — it is what makes systematic improvement possible. When decomposition, evidence gathering, and probabilistic integration share a single unstructured inference step, there is no mechanism to identify which component failed when a forecast is wrong, and therefore no mechanism to fix it. The structure of Fermi is the structure of its improvement loop.

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
