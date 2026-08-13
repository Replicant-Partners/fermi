//! Counterfactual value function — turns one real forecast into `2^n` synthetic
//! ones so exact Shapley credit can be computed without real-world permutations.
//!
//! # What this does
//!
//! [`super::exact_shapley`] needs `v(S)` for every subset `S` of the agents on a
//! composition. Observing all those subsets in the real world is impossible (you
//! would have to run the tournament once per subset). But the forecast model is
//! a *program*, so we can re-run it with any subset's claims applied and the rest
//! neutralised. That produces `p_S`, hence `v(S)`, for all `2^n` subsets from a
//! single resolved forecast.
//!
//! # How claims enter the model
//!
//! The factor templates declare each driver's distribution over parameters, e.g.
//!
//! ```text
//!   param socio_p5: real
//!   param socio_p50: real
//!   param socio_p95: real
//!
//!   driver socio_capital continuous {
//!       distribution: triangular(socio_p5, socio_p50, socio_p95)
//!   }
//! ```
//!
//! so injecting an agent's claim is exactly `set_param("socio_p5", ..)` and
//! friends. No AST rewriting, and the same path the live refit uses — which
//! matters, because a counterfactual computed through a different code path than
//! the real forecast would not be measuring the real forecast.
//!
//! # Neutralisation is a modelling choice, not an implementation detail
//!
//! "Agent absent" is ambiguous, and the two readings answer different questions.
//! [`Neutralisation`] makes the choice explicit and recorded rather than
//! implicit:
//!
//! - [`Neutralisation::Identity`] — the driver collapses to its identity value
//!   (`1.0` for a multiplier). Question: *what if this agent had said nothing?*
//!   For the World Cup template this is exactly the designed neutral: the model
//!   comment notes that all-`p50 = 1.0` reproduces the base rate, so
//!   `v(∅) = 0` and `v(N)` is the team's total improvement over the uniform
//!   prior. Credit is measured against silence.
//!
//! - [`Neutralisation::Reference`] — the driver takes a supplied reference triple
//!   (a BayesOps posterior mean, or the pooled mean of all agents' historical
//!   claims for that driver). Question: *what if this agent were replaced by an
//!   average one?* This is the fairer question for routing decisions, because a
//!   real composition change swaps one agent for another rather than removing it.
//!   Credit is measured against replacement.
//!
//! Identity flatters an agent whose driver has a large exponent (it gets credit
//! for the whole distance from neutral), while Reference isolates the agent's
//! edge over the field. Report the mode alongside the number; a `φ` computed
//! under one mode is not comparable to a `φ` computed under the other.
//!
//! # Determinism is load-bearing
//!
//! `v(S)` must be deterministic or efficiency (`Σφᵢ = v(N) − v(∅)`) silently
//! stops holding: Monte Carlo noise between subset evaluations shows up as
//! phantom credit. Every subset here is evaluated with the *same* fixed seed, so
//! the only difference between two runs is the claims themselves. Callers should
//! still assert [`super::ShapleyAttribution::efficiency_residual`] is ~0 before
//! persisting; it is the canary for this whole construction.

use std::collections::HashMap;

use crate::ast::Program;
use crate::executor::Executor;

use super::{brier, exact_shapley, ShapleyAttribution, MAX_EXACT_PLAYERS};

/// How to represent an agent that is absent from a counterfactual subset.
///
/// See the module docs — this is a modelling decision that changes what `φ`
/// means, so it is always recorded with the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neutralisation {
    /// Driver collapses to its identity value. "What if the agent said nothing?"
    Identity,
    /// Driver takes its reference triple. "What if an average agent replaced it?"
    /// Falls back to [`Neutralisation::Identity`] for any driver lacking a
    /// reference, so a partially-specified reference set degrades predictably
    /// rather than erroring mid-enumeration.
    Reference,
}

impl Neutralisation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Neutralisation::Identity => "identity",
            Neutralisation::Reference => "reference",
        }
    }
}

/// One driver an agent claimed, as recorded in `forecast_agent_claims`.
///
/// `driver` is the *param prefix* (`socio`, `institutional`, `dynamic`, …), not
/// the FPL driver name, because the prefix is what the params are keyed on and
/// what the claim ledger stores.
#[derive(Debug, Clone, PartialEq)]
pub struct DriverClaim {
    pub driver: String,
    pub p5: f64,
    pub p50: f64,
    pub p95: f64,
    /// Identity for this driver's combination rule (`1.0` for a multiplier).
    pub neutral_value: f64,
    /// Optional `(p5, p50, p95)` used under [`Neutralisation::Reference`].
    pub reference: Option<(f64, f64, f64)>,
}

impl DriverClaim {
    /// A multiplier claim with the usual `1.0` identity and no reference.
    pub fn multiplier(driver: impl Into<String>, p5: f64, p50: f64, p95: f64) -> Self {
        Self {
            driver: driver.into(),
            p5,
            p50,
            p95,
            neutral_value: 1.0,
            reference: None,
        }
    }

    pub fn with_reference(mut self, p5: f64, p50: f64, p95: f64) -> Self {
        self.reference = Some((p5, p50, p95));
        self
    }
}

/// The claims one agent contributed to one forecast. An agent covering several
/// drivers (e.g. `football_analyst` → dynamic/squad/tactical) carries several,
/// and is treated as a single Shapley player: credit is assigned to the agent,
/// not the driver. Score drivers as players instead if you want the finer
/// decomposition — the machinery is identical, only the grouping changes.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentClaims {
    pub agent_name: String,
    pub drivers: Vec<DriverClaim>,
}

/// A resolved forecast's model, ready for repeated subset evaluation.
pub struct CounterfactualModel {
    program: Program,
    iterations: usize,
    seed: u64,
    /// Every driver prefix the program declares parameters for, e.g. `socio`
    /// from `param socio_p50: real`.
    ///
    /// Needed because a model may declare drivers that no agent claimed — an
    /// agent failed, a driver has no owner, or the claim ledger predates that
    /// specialist. Those parameters would be unbound and the run would die with
    /// `Undefined variable: <driver>_p5`, failing attribution for the whole
    /// forecast. Instead every declared prefix is pre-seeded at the multiplier
    /// identity and claims overwrite it, so an unclaimed driver simply
    /// contributes nothing — which is exactly what "no agent spoke for it"
    /// should mean.
    declared_prefixes: Vec<String>,
}

impl CounterfactualModel {
    /// `seed` is fixed for the whole enumeration; see the determinism note in
    /// the module docs. Derive it from the forecast id so a re-computation
    /// months later reproduces the same attribution exactly.
    pub fn new(program: Program, iterations: usize, seed: u64) -> Self {
        let declared_prefixes = Self::declared_driver_prefixes(&program);
        Self {
            program,
            iterations,
            seed,
            declared_prefixes,
        }
    }

    /// Driver prefixes with declared `_p5` / `_p50` / `_p95` parameters.
    fn declared_driver_prefixes(program: &Program) -> Vec<String> {
        let mut out: std::collections::BTreeSet<String> = Default::default();
        for stmt in &program.statements {
            if let crate::ast::Statement::Param(p) = stmt {
                for suffix in ["_p50", "_p95", "_p5"] {
                    if let Some(prefix) = p.name.strip_suffix(suffix) {
                        if !prefix.is_empty() {
                            out.insert(prefix.to_string());
                        }
                        break;
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    /// Identity value used for a driver the program declares but nobody claimed.
    /// `1.0` is the multiplicative identity the factor templates are built
    /// around (their own comments note that all-`p50 = 1.0` reproduces the base
    /// rate).
    const UNCLAIMED_IDENTITY: f64 = 1.0;

    /// Parse FPL source into a model. Convenience for reading
    /// `forecast_spacetime.fpl_snapshot`.
    pub fn from_source(source: &str, iterations: usize, seed: u64) -> Result<Self, String> {
        let tokens = crate::lexer::Lexer::new(source)
            .tokenize()
            .map_err(|e| format!("FPL lex error: {:?}", e))?;
        let program = crate::parser::Parser::new(tokens)
            .parse()
            .map_err(|e| format!("FPL parse error: {:?}", e))?;
        Ok(Self::new(program, iterations, seed))
    }

    /// Parameter bindings for one subset: present agents contribute their
    /// claims, absent agents are neutralised per `mode`.
    fn params_for_subset(
        &self,
        agents: &[AgentClaims],
        mask: u32,
        mode: Neutralisation,
    ) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        // Pre-seed every declared driver at identity so an unclaimed one is
        // neutral rather than unbound. Claims below overwrite these.
        for prefix in &self.declared_prefixes {
            for suffix in ["p5", "p50", "p95"] {
                params.insert(format!("{prefix}_{suffix}"), Self::UNCLAIMED_IDENTITY);
            }
        }
        for (i, agent) in agents.iter().enumerate() {
            let present = mask & (1u32 << i) != 0;
            for d in &agent.drivers {
                let (p5, p50, p95) = if present {
                    (d.p5, d.p50, d.p95)
                } else {
                    match (mode, d.reference) {
                        (Neutralisation::Reference, Some(r)) => r,
                        // Identity, or Reference with no reference supplied:
                        // a degenerate triangular at the identity value, which
                        // is a point mass — the driver stops varying at all.
                        _ => (d.neutral_value, d.neutral_value, d.neutral_value),
                    }
                };
                params.insert(format!("{}_p5", d.driver), p5);
                params.insert(format!("{}_p50", d.driver), p50);
                params.insert(format!("{}_p95", d.driver), p95);
            }
        }
        params
    }

    /// Probability the model yields for one subset. `mask` bit `i` set means
    /// agent `i` contributed.
    pub fn probability_for_subset(
        &self,
        agents: &[AgentClaims],
        mask: u32,
        mode: Neutralisation,
    ) -> Result<f64, String> {
        let mut exec = Executor::with_seed(self.iterations, self.seed);
        exec.set_params(self.params_for_subset(agents, mask, mode));
        let results = exec
            .execute(&self.program)
            .map_err(|e| format!("FPL execution failed: {:?}", e))?;
        // `model:` IS the forecast for the factor templates — the mean of the
        // model expression is taken as the predicted probability without
        // further transformation. Clamp because a mis-specified model can
        // exceed [0,1] and a Brier computed on that would be meaningless.
        Ok(results.mean.clamp(0.0, 1.0))
    }
}

/// A per-agent attribution for one resolved forecast, with the provenance needed
/// to interpret and audit it.
#[derive(Debug, Clone)]
pub struct ForecastAttribution {
    pub agent_names: Vec<String>,
    /// Shapley credit per agent, same order as `agent_names`. Positive means the
    /// agent moved the forecast toward the realised outcome.
    pub shapley: ShapleyAttribution,
    /// Probability with every agent's claim applied — the real forecast.
    pub p_full: f64,
    /// Probability with every agent neutralised — the counterfactual baseline.
    pub p_baseline: f64,
    pub outcome: bool,
    pub neutralisation: Neutralisation,
    pub seed: u64,
    /// `p_S` for every subset, indexed by bitmask.
    ///
    /// Retained because the model runs are the expensive part: deriving the
    /// interaction indices from this costs nothing, whereas recomputing them
    /// through the model would double an already `2^n` workload. It also
    /// guarantees the marginals and the interactions are computed from the
    /// *same* randomness, so they cannot disagree.
    pub subset_probabilities: Vec<f64>,
}

impl ForecastAttribution {
    /// Team-level improvement the per-agent credits decompose. Equals
    /// `shapley.total` up to float error; exposed separately so the two can be
    /// cross-checked.
    pub fn team_improvement(&self) -> f64 {
        brier(self.p_baseline, self.outcome) - brier(self.p_full, self.outcome)
    }

    /// Pairwise Shapley interaction indices, from the cached subset
    /// probabilities — no further model runs.
    ///
    /// `> 0` synergy (worth more together), `< 0` redundancy (they substitute).
    /// `None` for a single-agent roster, where no pair exists.
    pub fn interactions(&self) -> Option<HashMap<(usize, usize), f64>> {
        let baseline = brier(self.p_baseline, self.outcome);
        super::pairwise_interactions(self.agent_names.len(), |mask| {
            baseline - brier(self.subset_probabilities[mask as usize], self.outcome)
        })
    }
}

/// Exact Shapley credit for every agent on one resolved forecast.
///
/// Evaluates the model `2^n` times under a fixed seed. Errors rather than
/// guessing if the roster is empty or larger than [`MAX_EXACT_PLAYERS`].
pub fn attribute_forecast(
    model: &CounterfactualModel,
    agents: &[AgentClaims],
    outcome: bool,
    mode: Neutralisation,
) -> Result<ForecastAttribution, String> {
    let n = agents.len();
    if n == 0 {
        return Err("no agents to attribute".into());
    }
    if n > MAX_EXACT_PLAYERS {
        return Err(format!(
            "{} agents exceeds exact-enumeration limit {}",
            n, MAX_EXACT_PLAYERS
        ));
    }

    // Evaluate every subset up front. Errors surface here rather than inside the
    // Shapley loop, where a closure cannot propagate them.
    let n_subsets = 1usize << n;
    let mut probs = Vec::with_capacity(n_subsets);
    for mask in 0..n_subsets {
        probs.push(model.probability_for_subset(agents, mask as u32, mode)?);
    }

    let baseline_brier = brier(probs[0], outcome);
    let shapley = exact_shapley(n, |mask| {
        baseline_brier - brier(probs[mask as usize], outcome)
    })
    .ok_or_else(|| "shapley enumeration rejected player count".to_string())?;

    Ok(ForecastAttribution {
        agent_names: agents.iter().map(|a| a.agent_name.clone()).collect(),
        shapley,
        p_full: probs[n_subsets - 1],
        p_baseline: probs[0],
        outcome,
        neutralisation: mode,
        seed: model.seed,
        subset_probabilities: probs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature of the World Cup factor template: multiplicative drivers over
    /// params, with the model expression as the forecast.
    fn wc_like_program() -> &'static str {
        r#"
question "Will the team win?"

param socio_p5: real
param socio_p50: real
param socio_p95: real
param dynamic_p5: real
param dynamic_p50: real
param dynamic_p95: real

driver socio_capital continuous {
    distribution: triangular(socio_p5, socio_p50, socio_p95)
}

driver dynamic_performance continuous {
    distribution: triangular(dynamic_p5, dynamic_p50, dynamic_p95)
}

model: 0.0208 * (socio_capital ^ 0.5) * (dynamic_performance ^ 1.8)

simulate 4000 iterations
"#
    }

    fn two_agents() -> Vec<AgentClaims> {
        vec![
            AgentClaims {
                agent_name: "macro_data_agent".into(),
                drivers: vec![DriverClaim::multiplier("socio", 1.05, 1.15, 1.30)],
            },
            AgentClaims {
                agent_name: "football_analyst".into(),
                drivers: vec![DriverClaim::multiplier("dynamic", 1.20, 1.40, 1.65)],
            },
        ]
    }

    #[test]
    fn program_parses_and_runs() {
        let m = CounterfactualModel::from_source(wc_like_program(), 2000, 7).unwrap();
        let p = m
            .probability_for_subset(&two_agents(), 0b11, Neutralisation::Identity)
            .unwrap();
        assert!(p > 0.0 && p < 1.0, "probability {}", p);
    }

    /// Identity neutralisation of everyone must reproduce the model's base rate
    /// (all drivers pinned to 1.0 ⇒ product collapses to the 0.0208 prior).
    /// This is what makes `v(∅) = 0` meaningful rather than arbitrary.
    #[test]
    fn empty_subset_reproduces_base_rate() {
        let m = CounterfactualModel::from_source(wc_like_program(), 4000, 11).unwrap();
        let p = m
            .probability_for_subset(&two_agents(), 0b00, Neutralisation::Identity)
            .unwrap();
        assert!(
            (p - 0.0208).abs() < 1e-6,
            "neutral subset should equal the base rate, got {}",
            p
        );
    }

    /// Determinism: the same subset must yield the identical probability across
    /// evaluations. If this fails, efficiency becomes meaningless and every
    /// attribution is polluted by Monte Carlo noise.
    #[test]
    fn subset_evaluation_is_deterministic() {
        let m = CounterfactualModel::from_source(wc_like_program(), 3000, 99).unwrap();
        let agents = two_agents();
        let a = m
            .probability_for_subset(&agents, 0b01, Neutralisation::Identity)
            .unwrap();
        let b = m
            .probability_for_subset(&agents, 0b01, Neutralisation::Identity)
            .unwrap();
        assert_eq!(a, b, "identical subset must give identical probability");
    }

    /// The headline property, end to end through a real FPL execution:
    /// per-agent credit sums exactly to the team's improvement.
    #[test]
    fn efficiency_holds_through_real_fpl_execution() {
        let m = CounterfactualModel::from_source(wc_like_program(), 4000, 42).unwrap();
        let att = attribute_forecast(&m, &two_agents(), true, Neutralisation::Identity).unwrap();

        assert!(
            att.shapley.efficiency_residual() < 1e-9,
            "residual {} — credits must decompose team improvement exactly",
            att.shapley.efficiency_residual()
        );
        // And the independently-computed team improvement agrees with Shapley's.
        assert!(
            (att.team_improvement() - att.shapley.total).abs() < 1e-9,
            "team {} vs shapley total {}",
            att.team_improvement(),
            att.shapley.total
        );
    }

    /// Both agents pushed the probability up and the outcome was YES, so both
    /// must earn positive credit — and the agent on the heavily-weighted driver
    /// (exponent 1.8 vs 0.5) must earn more. This is the discrimination a team
    /// Brier cannot produce: same forecast, same outcome, different credit.
    #[test]
    fn credit_discriminates_between_agents_on_one_forecast() {
        let m = CounterfactualModel::from_source(wc_like_program(), 6000, 2024).unwrap();
        let att = attribute_forecast(&m, &two_agents(), true, Neutralisation::Identity).unwrap();

        let socio = att.shapley.values[0];
        let dynamic = att.shapley.values[1];
        assert!(socio > 0.0, "socio credit {}", socio);
        assert!(dynamic > 0.0, "dynamic credit {}", dynamic);
        assert!(
            dynamic > socio,
            "the dominant driver's agent must earn more: dynamic {} vs socio {}",
            dynamic,
            socio
        );
    }

    /// An agent that claims the identity value is a dummy player: it changes no
    /// subset's value, so it must receive ~zero credit rather than sharing in
    /// the team's success.
    #[test]
    fn agent_claiming_neutral_earns_nothing() {
        let m = CounterfactualModel::from_source(wc_like_program(), 4000, 5).unwrap();
        let agents = vec![
            AgentClaims {
                agent_name: "says_nothing".into(),
                drivers: vec![DriverClaim::multiplier("socio", 1.0, 1.0, 1.0)],
            },
            AgentClaims {
                agent_name: "football_analyst".into(),
                drivers: vec![DriverClaim::multiplier("dynamic", 1.20, 1.40, 1.65)],
            },
        ];
        let att = attribute_forecast(&m, &agents, true, Neutralisation::Identity).unwrap();
        assert!(
            att.shapley.values[0].abs() < 1e-9,
            "neutral claimant must earn ~0, got {}",
            att.shapley.values[0]
        );
        assert!(att.shapley.values[1] > 0.0);
    }

    /// When the outcome is NO, agents that pushed the probability up must be
    /// penalised — the same claims that earned credit above now cost it.
    #[test]
    fn credit_flips_sign_with_the_outcome() {
        let m = CounterfactualModel::from_source(wc_like_program(), 4000, 17).unwrap();
        let agents = two_agents();
        let yes = attribute_forecast(&m, &agents, true, Neutralisation::Identity).unwrap();
        let no = attribute_forecast(&m, &agents, false, Neutralisation::Identity).unwrap();
        for i in 0..2 {
            assert!(yes.shapley.values[i] > 0.0, "yes[{}]", i);
            assert!(
                no.shapley.values[i] < 0.0,
                "no[{}] = {} should be negative",
                i,
                no.shapley.values[i]
            );
        }
    }

    /// Reference neutralisation measures the agent's edge over a replacement
    /// rather than over silence, so it must yield a *smaller* credit when the
    /// reference already captures most of the agent's claim. Both modes remain
    /// internally consistent (efficiency holds in each).
    #[test]
    fn reference_mode_measures_edge_over_replacement() {
        let m = CounterfactualModel::from_source(wc_like_program(), 6000, 31).unwrap();
        let agents = vec![
            AgentClaims {
                agent_name: "macro_data_agent".into(),
                drivers: vec![DriverClaim::multiplier("socio", 1.05, 1.15, 1.30)
                    .with_reference(1.04, 1.14, 1.28)],
            },
            AgentClaims {
                agent_name: "football_analyst".into(),
                drivers: vec![DriverClaim::multiplier("dynamic", 1.20, 1.40, 1.65)
                    .with_reference(1.19, 1.38, 1.62)],
            },
        ];
        let identity = attribute_forecast(&m, &agents, true, Neutralisation::Identity).unwrap();
        let reference = attribute_forecast(&m, &agents, true, Neutralisation::Reference).unwrap();

        assert!(identity.shapley.efficiency_residual() < 1e-9);
        assert!(reference.shapley.efficiency_residual() < 1e-9);

        // Against a near-identical replacement the agent's measured edge nearly
        // vanishes, while against silence it looks large.
        assert!(
            reference.shapley.values[1].abs() < identity.shapley.values[1].abs(),
            "reference {} should be smaller than identity {}",
            reference.shapley.values[1],
            identity.shapley.values[1]
        );
        assert_eq!(reference.neutralisation.as_str(), "reference");
    }

    /// Interactions derived from the cached subset probabilities must equal
    /// those computed by re-running the model. This pins the optimisation that
    /// halves the workload: if the cache ever drifted from the model, Loop 4
    /// would be acting on synergy numbers that don't match the credit numbers.
    #[test]
    fn cached_interactions_match_fresh_model_runs() {
        let m = CounterfactualModel::from_source(wc_like_program(), 4000, 77).unwrap();
        let agents = two_agents();
        let att = attribute_forecast(&m, &agents, true, Neutralisation::Identity).unwrap();

        let cached = att.interactions().unwrap();
        let baseline = brier(att.p_baseline, true);
        let fresh = super::super::pairwise_interactions(agents.len(), |mask| {
            baseline
                - brier(
                    m.probability_for_subset(&agents, mask, Neutralisation::Identity)
                        .unwrap(),
                    true,
                )
        })
        .unwrap();

        assert_eq!(cached.len(), fresh.len());
        for (k, v) in &cached {
            assert!(
                (v - fresh[k]).abs() < 1e-12,
                "pair {:?}: cached {} vs fresh {}",
                k,
                v,
                fresh[k]
            );
        }
    }

    /// A model may declare drivers nobody claimed — an agent failed, a driver
    /// has no owner, or the ledger predates that specialist. Those must
    /// neutralise, not abort the whole attribution with an unbound-parameter
    /// error. Here `dynamic_*` is declared by the program but unclaimed.
    #[test]
    fn unclaimed_declared_drivers_neutralise_instead_of_failing() {
        let m = CounterfactualModel::from_source(wc_like_program(), 4000, 23).unwrap();
        let agents = vec![AgentClaims {
            agent_name: "macro_data_agent".into(),
            drivers: vec![DriverClaim::multiplier("socio", 1.05, 1.15, 1.30)],
        }];
        // Runs at all — the unclaimed dynamic driver is pinned at identity.
        let att = attribute_forecast(&m, &agents, true, Neutralisation::Identity).unwrap();
        assert!(att.shapley.efficiency_residual() < 1e-9);
        // And with every driver neutral the baseline is still the base rate,
        // so the unclaimed driver genuinely contributed nothing.
        assert!(
            (att.p_baseline - 0.0208).abs() < 1e-6,
            "baseline {}",
            att.p_baseline
        );
        assert!(att.shapley.values[0] > 0.0);
    }

    /// A single-agent roster has no pairs, so interactions are absent rather
    /// than an empty-but-present map that a caller might read as "no synergy".
    #[test]
    fn single_agent_has_no_interactions() {
        let m = CounterfactualModel::from_source(wc_like_program(), 2000, 3).unwrap();
        let agents = vec![AgentClaims {
            agent_name: "solo".into(),
            drivers: vec![DriverClaim::multiplier("socio", 1.05, 1.15, 1.30)],
        }];
        let att = attribute_forecast(&m, &agents, true, Neutralisation::Identity).unwrap();
        assert!(att.interactions().is_none());
        // Efficiency is trivially the whole improvement for a lone player.
        assert!((att.shapley.values[0] - att.shapley.total).abs() < 1e-12);
    }

    #[test]
    fn rejects_empty_roster() {
        let m = CounterfactualModel::from_source(wc_like_program(), 500, 1).unwrap();
        assert!(attribute_forecast(&m, &[], true, Neutralisation::Identity).is_err());
    }
}
