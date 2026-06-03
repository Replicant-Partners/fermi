# SimOps: A Domain Instantiation of ABW for Biomanufacturing Process Optimisation

**Authors:** Ivan Labra — axelotl partners  
**Version:** 1.0 — June 2026  
**Status:** Working paper

---

## Abstract

SimOps is an instantiation of the Agent Bestiary World (ABW) substrate for the domain of biomanufacturing process modelling, optimisation, and forecasting. It demonstrates how ABW's five core primitives — Agent, Episode, Composition, Workspace, and Feedback Loop — specialise to a physical-process domain where computational models are validated against real sensor observations, uncertainty is quantified from operational history, and process decisions are supported by calibrated probabilistic forecasts. This paper describes SimOps as a *logical* system: the conceptual architecture, the domain-specific extensions to the ABW vocabulary, and the mechanisms by which physical-world ground truth closes the feedback loops that would otherwise rely on LLM judgment alone. We show that SimOps constitutes a domain-constrained Mixture-of-Experts (MoE) system in which the constraint is supplied by the SOSA/SSN observation ontology (W3C, 2017), the output contract is defined by physical process KPIs, and the calibration signal is the delta between computational projection and physical measurement. We describe the BayesOps extension (Labra, 2026d) as the mechanism by which posterior distributions fitted from operational data replace analyst-elicited parameters, closing the loop from raw observations to calibrated probabilistic forecasts.

**Keywords:** biomanufacturing, process optimisation, SOSA/SSN, domain-constrained MoE, Bayesian inference, feedback loops, digital twin

---

## 1. Introduction

### 1.1 The process modelling problem

Biomanufacturing processes — fermentation, cultivation, extraction, downstream processing — are characterised by complex, nonlinear dynamics, high measurement cost, and significant batch-to-batch variability. The dominant approach to process modelling is deterministic simulation: a mathematical model of the process physics (mass balances, kinetic rates, energy flows) is parameterised and used to predict outcomes given input conditions. This approach has known limitations:

1. **Parameters are estimated once and not updated.** Real process parameters drift with changes in strain, media, equipment, and environmental conditions. A model parameterised on historical data may be systematically miscalibrated for current conditions.

2. **Uncertainty is not propagated.** Deterministic models return point estimates. Decision-makers receive "predicted yield: 4.8 kg" without any quantification of the uncertainty in that prediction.

3. **Model selection is not calibrated.** Multiple model variants may exist for the same process (e.g., Monod kinetics vs. logistic growth for biomass accumulation). Which model is most accurate for which operating conditions is typically answered by domain expertise rather than by evidence from operational data.

4. **What-if scenario analysis is manual.** Evaluating "what yield would I get at lighting=160 kWh instead of 120 kWh?" requires running the deterministic model at both settings — a manual process that does not scale to systematic scenario comparison.

SimOps addresses all four limitations by instantiating ABW's RSI primitive in the biomanufacturing domain.

### 1.2 The ABW instantiation approach

SimOps does not build a new platform. It instantiates ABW's primitives with domain-specific specialisations:

- ABW *Agents* become process-domain specialists with SOSA-typed input/output contracts
- ABW *Episodes* carry SOSA observation identifiers enabling linkage to physical measurements
- ABW *Compositions* become process analysis teams with a domain-constrained MoE routing strategy
- ABW *Workspaces* become per-process operational environments with git-backed YAML process configurations
- ABW *Feedback Loops* are grounded in physical measurement deltas rather than LLM judgment

The domain-specific contribution is the SOSA observation vocabulary, the cascade physics engine, and the deferred hard-verifier scoring mechanism. Everything else is inherited from ABW.

---

## 2. The Domain Vocabulary

### 2.1 SOSA/SSN as the observation ontology

SimOps adopts the W3C Semantic Sensor Network (SSN) ontology and its core module SOSA (Sensor, Observation, Sample, Actuator) as the vocabulary for all physical observations (W3C, 2017). SOSA provides four key concepts:

- **Sensor**: a device that observes a property of a feature of interest
- **Observation**: a single act of observing, with a phenomenon time, a result value, and a result unit
- **Feature of Interest**: the entity being observed (a cultivation vessel, a process stage, a substrate)
- **Observable Property**: what is being measured (biomass concentration, dissolved oxygen, temperature)

SimOps extends SOSA through the `xi:simops/` namespace for domain-specific concepts not covered by the standard vocabulary: process stage types (fermentation, cultivation, extraction), equipment classes (bioreactor, centrifuge, membrane filter), and derived process KPIs (net energy ratio, lifecycle carbon intensity, specific cost per unit output).

The SOSA vocabulary serves three functions in SimOps:

1. **Observation ingestion**: Real sensor data from cultivation runs is ingested as SOSA observations, enabling standard-vocabulary querying across different sensor types and deployments.

2. **Synthetic observation production**: The cascade physics engine produces synthetic SOSA observations (predicted values at modelled process conditions), tagged with `source: simops_simulation` to distinguish them from real measurements.

3. **Ground-truth comparison**: When a real observation arrives for the same `(observable_property, feature_of_interest)` as a prior synthetic observation, the delta between predicted and actual becomes the hard-verified calibration signal that feeds Loop 5 (§4.2).

### 2.2 The ProcessConfig

A ProcessConfig is the logical model of a biomanufacturing process. It is the unit of configuration in SimOps, analogous to the Agent card in ABW's core architecture. A ProcessConfig is a directed graph of Stages, where each Stage has:

- **Input resource**: what the stage consumes (substrate, energy, media)
- **Output resource**: what the stage produces (biomass, product, by-product)
- **Efficiency**: the fraction of input that is converted to output
- **Carbon intensity**: kg CO₂-equivalent per kg output (negative for carbon-sequestering stages)
- **OpEx profile**: cost per unit input

The ProcessConfig is the object that the cascade physics engine operates on. It encodes the process's structural properties (stage connectivity, mass balance constraints) and parameters (efficiency, carbon intensity, cost). The parameters are initially elicited from domain expertise; BayesOps (Labra, 2026d) provides the mechanism for updating them from operational data.

### 2.3 The cascade physics engine

The cascade physics engine (`crates/simops/cascade_v2`) implements deterministic forward and backward simulation over a ProcessConfig. Forward cascade computes outputs given inputs: given an input quantity at stage 1, compute the output quantities at each downstream stage, accumulating opex and carbon at each step. Backward cascade solves the inverse problem: given a target output, compute the required input.

The engine is deterministic and physics-grounded: it enforces mass balance at each stage boundary, propagates uncertainty only through the parameters (efficiency ranges, carbon intensity distributions). Every cascade run produces a `CascadeResponseV2` with a `projection_id` — a stable UUID that identifies this specific simulation run. This identifier flows into synthetic SOSA observations and the agent episode context, enabling the `ProjectionScoringEvaluator` to match real measurements back to the projection that preceded them.

---

## 3. The Agent Roster

SimOps instantiates seven domain-specialist agents, each with typed SOSA input/output contracts:

### 3.1 `simops_cascade`

The cascade agent wraps the deterministic physics engine with an LLM reasoning layer. It accepts a ProcessConfig and a set of input conditions, invokes the cascade physics engine, interprets the results in context (comparing against benchmarks, flagging anomalies, suggesting parameter adjustments), and returns per-stage outputs with provenance-tagged synthetic SOSA observations. Every cascade execution produces a `projection_id` used for subsequent deferred scoring.

*Output contract:* Per-stage flow values (input/output quantities, efficiency, carbon delta, opex) + aggregate process KPIs (NER, LCC, SEC) + synthetic SOSA observations.

### 3.2 `simops_dynamics_runner`

The dynamics runner operates the coupled ODE system for transient process modelling — biomass growth kinetics, substrate consumption, dissolved oxygen dynamics. Unlike the cascade agent (which uses steady-state mass balance), the dynamics runner models temporal trajectories within a stage. It is the appropriate agent when trajectory shape matters (e.g., predicting when a batch will reach target density) rather than just steady-state output.

*Hard-verified signal:* `projection_accuracy` from `ProjectionScoringEvaluator` — the delta between the runner's predicted trajectory and the actual measurements from the completed batch.

### 3.3 `simops_predictor`

The predictor fits a regression model (currently OLS; BayesOps Phase 4 introduces Bayesian regression) from SOSA observations to predict process outcomes given input conditions. It is the agent that answers conditional prediction queries: "given lighting=160 kWh and temperature=28°C, what yield distribution does the data support?"

*BayesOps dependency:* Phase 4 of BayesOps (Labra, 2026d, §10 Phase 4) replaces the OLS engine with `ConditionalPosterior::predict()`, enabling full posterior predictive distributions and input sensitivity analysis.

### 3.4 `simops_optimizer`

The optimizer solves the inverse prediction problem: given a target output and a trained predictor, find the input conditions that maximise the probability of achieving the target. It supports single-input solve (one free variable, all others fixed) and proportional scaling (all inputs scaled from a reference configuration).

### 3.5 `simops_advisor`

The advisor conducts structured discovery conversations with operators to elicit a ProcessConfig from domain knowledge. It is a dialogue agent, not a computation agent. It asks structured questions about process stages, resources, and parameters, and produces a ProcessConfig as its output. The advisor is the entry point for operators without prior simulation experience.

### 3.6 `simops_narrator`

The narrator translates numerical cascade outputs and prediction results into natural-language process narratives for operator communication. It is a presentation agent that sits downstream of the computation agents.

### 3.7 `supply_chain_oracle`

The oracle resolves bill-of-materials pricing for process inputs using market data. It accepts a list of input materials and quantities and returns mid-market prices with supply risk flags. It is invoked by the cascade agent when OpEx calculations require current market prices rather than static cost assumptions.

---

## 4. The SimOps Feedback Loop Architecture

SimOps instantiates all five ABW feedback loops, but Loop 5 is qualitatively different from the general ABW case because the ground truth signal is physical measurement rather than market resolution.

### 4.1 Loop 1 in SimOps: Projection-accuracy-grounded learning

In general ABW, Loop 1 consolidates LLM-judged eval signals. In SimOps, Loop 1 additionally consolidates `projection_accuracy` signals — hard-verified deltas between the dynamics runner's predictions and real batch measurements.

The mechanism (Labra, 2026e, Spec 20):

1. The dynamics runner produces a cascade projection and writes synthetic SOSA observations tagged with `projection_id`
2. The real batch runs; the operator enters actual measurements as SOSA observations
3. `ProjectionScoringEvaluator` matches the real observation to the prior synthetic observation via `projection_id` (or fallback lookup by `(observable_property, feature_of_interest)`)
4. Score: `1 − |predicted − actual| / |actual|`, range [0, 1], 1 = exact match
5. `EvalSignal` (dimension: `projection_accuracy`) written to the episode that produced the projection
6. `ConsolidationWorker` clusters low-scoring episodes and extracts semantic rules: "model X overestimates yield by ~15% when temperature > 65°C"
7. Rules injected into dynamics runner KG context on next execution

The critical property: **the batch does not know what was predicted**. The ground truth is physically independent of the agent's output. This is the same epistemic structure as Brier scoring on resolved forecasts — the future resolves independently — but at the timescale of cultivation batch cycles (days to weeks) rather than forecast resolution (months).

### 4.2 Loop 5 in SimOps: Model selection calibration

Loop 5 in SimOps routes queries to the most accurate dynamics model for the current operating conditions. The routing strategist reads `projection_accuracy` calibration profiles per model URI from the `get_agent_calibration` endpoint, which aggregates `projection_accuracy` eval signals per model via the `model_accuracy` breakdown in the response.

Over time, the routing strategist accumulates evidence about which model is most accurate for which conditions:

```
batch 1: kombucha_fermentation at 30°C, projection_accuracy = 0.94 → reinforce
batch 7: kombucha_fermentation at 67°C, projection_accuracy = 0.61 → flag
batch 12: bc_optimization at 30°C, projection_accuracy = 0.91 → reinforce
```

The strategist's dreaming cycle consolidates these episodes into routing rules: "for cultivation at temperature > 60°C, bc_optimization has historically outperformed kombucha_fermentation by 0.15 accuracy points." These rules enter the strategist's KG context and inform future routing decisions.

### 4.3 Loop A in SimOps: BayesOps parameter fitting

Loop A (BayesOps, Labra, 2026d) is not yet implemented. When it is, it provides the mechanism for updating ProcessConfig parameters from operational data:

- `fit_marginal(yield_observations, weights=real:1.0/synthetic:0.2)` → `FittedDistribution` for the yield base rate
- `fit_conditional(observations_with_inputs)` → `ConditionalPosterior` supporting `predict(query_inputs)`, `input_sensitivity()`, `compare_scenarios()`, `prob_exceeds(threshold)`

Loop A operates offline (triggered by data accumulation) and produces parameters that are written into FPL Driver declarations, feeding Loop B (the Fermi MC executor). The composition of Loop A (parameter fitting), Loop 1 (semantic rule accumulation), and Loop B (probabilistic simulation) is the full SimOps intelligence stack.

---

## 5. The SimOps Composition as Domain-Constrained MoE

The SimOps agent roster constitutes a domain-constrained MoE composition. The domain constraint is enforced by the SOSA vocabulary and the ProcessConfig schema:

- **Output contract**: all agent outputs resolve to SOSA-typed observations or ProcessConfig-typed predictions. The output space is physically bounded.
- **Calibration signal**: `projection_accuracy` against real batch measurements. This is harder to game than LLM-judged quality because it requires real process data.
- **Routing criteria**: per-model calibration profiles, stratified by `(observable_property, temperature_range, n_instances)`.

The routing strategist operates the three-stage MoE cycle (classify → route → synthesise) with domain-specific priors: models that have demonstrated accuracy on the current operating conditions are weighted more highly than models with no track record in those conditions.

### 5.1 The what-if scenario interface

A distinctive capability of the SimOps MoE is systematic scenario comparison. An operator specifies two or more scenario configurations (different input quantities, different process parameters, different model selections). The composition:

1. Runs each scenario through the cascade agent (deterministic physical model)
2. Runs each scenario through the dynamics runner (transient trajectory model) where relevant
3. With BayesOps: runs each scenario through `ConditionalPosterior::compare_scenarios()` to produce full predictive distributions and `prob_exceeds(target)` evaluations
4. Returns a structured comparison with confidence intervals, sensitivity rankings, and recommended operating conditions

The comparison is grounded: it does not rely on LLM-generated narrative alone. The numerical results from the physics engine and the statistical posterior are the primary outputs; the narrator agent translates them into operator-facing language.

---

## 6. Physical-World Grounding

### 6.1 Why physical grounding changes the architecture

The fundamental architectural distinction between SimOps and a domain-agnostic ABW deployment is the availability of physical ground truth. When a cultivation batch completes, the real yield is a fact — not an opinion, not an LLM judgment, not a preference. This fact can be compared to any prior prediction, and the comparison is meaningful regardless of how the prediction was produced.

This changes the epistemic properties of the feedback loops. In a general ABW deployment, Loop 5's calibration signal (Brier score on resolved forecasts) requires months to accumulate because forecast resolution cadences are slow. In SimOps, Loop 5b's calibration signal (projection accuracy on completed batches) accumulates on a batch cycle timescale — typically days to weeks. The feedback is faster, harder to game, and more directly connected to operational decisions.

### 6.2 The sensor-to-learning pipeline

The full pipeline from physical sensor to learned model improvement:

```
Physical sensor records batch measurement
  → SOSA observation ingested via POST /api/simops/ingest-observations
  → ProjectionScoringEvaluator: match to prior projection_id
  → EvalSignal (projection_accuracy) written to episode
  → ConsolidationWorker: cluster low-scoring episodes
  → Semantic rule: "model X unreliable at condition Y"
  → Rule injected into dynamics_runner KG context
  → Next projection benefits from accumulated calibration knowledge
```

This pipeline implements what the SIA research programme (arXiv:2603.27766) identifies as the critical distinction between harness-level learning (this pipeline changes what the agent considers when selecting and parameterising models) and weight-level learning (which would change the model's internal parameters). SimOps implements harness-level learning. The BayesOps extension implements parameter-level learning as a separate, complementary loop.

---

## 7. Open Questions

### 7.1 Multi-stage calibration

The current `ProjectionScoringEvaluator` scores per-property deltas. A cascade projection involves multiple stages with multiple output properties. Whether aggregate cascade accuracy (across all stages) or per-stage accuracy is the right calibration signal depends on the operator's decision context. A downstream stage's accuracy may depend on upstream stage accuracy in ways the current scorer does not capture. Multi-stage calibration is an open design question.

### 7.2 Synthetic data weight calibration

BayesOps (Labra, 2026d) proposes discounting synthetic cascade data at weight=0.2 relative to real observations at weight=1.0. This weighting was chosen heuristically. The optimal weight depends on the quality of the physics model: a highly accurate deterministic model should contribute more to the posterior than a poorly calibrated one. Adaptive weighting — using `projection_accuracy` to calibrate the synthetic data weight — is a natural extension.

### 7.3 Strain and condition generalisation

A model calibrated on one strain of bacterial cellulose may not generalise to a different strain with different growth kinetics. The current architecture treats each workspace as independent. Cross-workspace calibration — pooling evidence across different operators running the same fundamental process with different strains — is architecturally feasible (BayesOps' `HierarchicalNormal` model in Phase 4) but not yet implemented.

---

## 8. Conclusion

SimOps demonstrates the ABW instantiation pattern for a physical-process domain. Its distinctive contribution is the grounding of ABW's feedback loops in physical measurement: the `ProjectionScoringEvaluator` closes Loop 1 and Loop 5 against real batch outcomes rather than LLM judgment, producing hard-verified calibration signals at batch-cycle timescales. The SOSA/SSN observation vocabulary provides the interoperability layer that makes this grounding standard-compliant and portable across sensor types and deployments.

The architecture is modular: the cascade physics engine, the SOSA vocabulary, and the BayesOps parameter-fitting layer are each independently useful and independently verifiable against physical ground truth. Their composition produces a system that learns from operational data, improves its model selection under changing conditions, and supports calibrated probabilistic scenario analysis — capabilities that deterministic process simulation alone cannot provide.

---

## References

1. Labra, I. (2026a). ABW as Allosteric Substrate: Signal Transduction Concepts in a Recursive Agent Architecture. `docs/papers/abw_as_allosteric_substrate.md`

2. Labra, I. (2026b). Agent Bestiary World: A Logical Architecture for Recursive Self-Improving Multi-Agent Systems. `docs/papers/abw_logical_architecture.md`

3. Labra, I. (2026c). ABW Distribution Topology — Design Proposal. `docs/architecture/DISTRIBUTION_TOPOLOGY_PROPOSAL.md`

4. Labra, I. (2026d). BayesOps: Data-Informed Distribution Fitting for Fermi. `docs/specs/14_BAYESOPS_SPEC.md`

5. Labra, I. (2026e). SimOps Projection Scoring: Deferred Hard Verifier Loop. `docs/specs/20_SIMOPS_PROJECTION_SCORING.md`

6. W3C. (2017). *Semantic Sensor Network Ontology*. https://www.w3.org/TR/vocab-ssn/

7. Monod, J., Wyman, J., & Changeux, J.-P. (1965). On the Nature of Allosteric Transitions: A Plausible Model. *Journal of Molecular Biology*, 12(2), 88–118.

8. arXiv:2603.27766. SIA: Scalable Intelligence Architecture. *Preprint, 2026*.

---

*Working paper. Not for external distribution without author consent.*
