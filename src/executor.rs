/// Monte Carlo Executor
///
/// Runs Monte Carlo simulations on FPL programs:
/// 1. Sampling from driver distributions
/// 2. Evaluating the model expression for each iteration
/// 3. Collecting statistics from the results
///
/// Also supports factor-model programs (TEAM_PRIOR, TOURNAMENT_PATH, H2H_MATCH):
/// sample each orthogonal factor with noise proportional to its variance share,
/// then evaluate the Cobb-Douglas estimate expression. See `execute_factor_model`.
use crate::ast::{Distribution, DriverType, Expression, Program, Statement};
use crate::distributions::{
    calculate_statistics, sample_beta, sample_lognormal, sample_normal, sample_triangular,
    sample_uniform,
};
use crate::evaluator::{evaluate, EvaluationContext};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;

pub type ExecutionResult2<T> = Result<T, ExecutionError>;

#[derive(Debug, Clone)]
pub enum ExecutionError {
    EvaluationError(String),
    DistributionError(String),
    NoModelFound,
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ExecutionError::EvaluationError(msg) => write!(f, "Evaluation error: {}", msg),
            ExecutionError::DistributionError(msg) => write!(f, "Distribution error: {}", msg),
            ExecutionError::NoModelFound => write!(f, "No model statement found"),
        }
    }
}

impl std::error::Error for ExecutionError {}

/// Results from a Monte Carlo simulation
#[derive(Debug, Clone)]
pub struct ExecutionResults {
    pub samples: Vec<f64>,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub p5: f64,
    pub p25: f64,
    pub p75: f64,
    pub p95: f64,
    pub min: f64,
    pub max: f64,
    pub iterations: usize,

    // Base rate and divergence (Tetlock methodology)
    pub base_rate: Option<f64>, // Historical frequency from reference class
    pub divergence_relative: Option<f64>, // (mean - base_rate) / base_rate
    pub divergence_absolute: Option<f64>, // mean - base_rate

    // Factor-model outputs (only populated by execute_factor_model)
    //
    // factor_means:    mean of each factor X1..Xk across iterations
    // factor_std_devs: std-dev of each factor (≈ sqrt(variance_share))
    // factor_corr_max: max |corr(Xi, Xj)| — diagnostic for orthogonality
    // estimate_name:   name of the estimate statement (e.g. "tournament_strength")
    // param_bindings:  flattened params from .app/params.json
    // learnable_manifest: every `learnable(...)` literal with its assigned
    //                  name + prior, the contract surface for BayesOps.
    //                  Names follow the pattern `<owner>_l<idx>`; BayesOps
    //                  writes updates by setting params.<name> = new_value
    //                  in the workspace's .app/params.json.
    pub factor_means: Option<HashMap<String, f64>>,
    pub factor_std_devs: Option<HashMap<String, f64>>,
    pub factor_corr_max: Option<f64>,
    pub estimate_name: Option<String>,
    pub param_bindings: Option<HashMap<String, f64>>,
    pub learnable_manifest: Option<Vec<LearnableInfo>>,
}

impl ExecutionResults {
    pub fn histogram(&self, bins: usize) -> Vec<(f64, usize)> {
        if self.samples.is_empty() {
            return Vec::new();
        }

        let min = self.min;
        let max = self.max;
        let bin_width = (max - min) / bins as f64;

        let mut histogram = vec![0; bins];
        for &sample in &self.samples {
            let bin = ((sample - min) / bin_width).floor() as usize;
            let bin = bin.min(bins - 1); // Handle edge case where sample == max
            histogram[bin] += 1;
        }

        histogram
            .into_iter()
            .enumerate()
            .map(|(i, count)| {
                let bin_start = min + (i as f64 * bin_width);
                (bin_start, count)
            })
            .collect()
    }
}

/// Monte Carlo executor
pub struct Executor {
    iterations: usize,
    rng: StdRng,
    /// Optional fixed driver values for conditional execution
    /// Map of driver_name -> fixed_value
    fixed_drivers: std::collections::HashMap<String, f64>,
    /// Parameter bindings for factor-model programs.
    /// Populated from `.app/params.json` and accessed via Expression::ParamRef.
    params: std::collections::HashMap<String, f64>,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl Executor {
    pub fn new(iterations: usize) -> Self {
        Self {
            iterations,
            rng: StdRng::from_entropy(),
            fixed_drivers: std::collections::HashMap::new(),
            params: std::collections::HashMap::new(),
        }
    }

    /// Bind a parameter value for use by ParamRef expressions.
    pub fn set_param(&mut self, name: impl Into<String>, value: f64) {
        self.params.insert(name.into(), value);
    }

    /// Bulk-set parameters (e.g. from .app/params.json).
    pub fn set_params(&mut self, params: HashMap<String, f64>) {
        self.params.extend(params);
    }

    /// Create executor with fixed driver values for conditional simulation
    pub fn with_fixed_drivers(
        iterations: usize,
        fixed: std::collections::HashMap<String, f64>,
    ) -> Self {
        Self {
            iterations,
            rng: StdRng::from_entropy(),
            fixed_drivers: fixed,
            params: std::collections::HashMap::new(),
        }
    }

    /// Fix a specific driver at a given value
    pub fn fix_driver(&mut self, driver_name: String, value: f64) {
        self.fixed_drivers.insert(driver_name, value);
    }

    /// Clear all fixed drivers
    pub fn clear_fixed_drivers(&mut self) {
        self.fixed_drivers.clear();
    }

    pub fn with_seed(iterations: usize, seed: u64) -> Self {
        Self {
            iterations,
            rng: StdRng::seed_from_u64(seed),
            fixed_drivers: std::collections::HashMap::new(),
            params: std::collections::HashMap::new(),
        }
    }

    /// Execute a Monte Carlo simulation on the given program.
    ///
    /// Auto-dispatches: if the program is a factor model (has `factor` + `estimate`
    /// statements), runs `execute_factor_model`. Otherwise runs the classic
    /// driver+model Monte Carlo path.
    pub fn execute(&mut self, program: &Program) -> ExecutionResult2<ExecutionResults> {
        let has_factor = program.statements.iter().any(|s| matches!(s, Statement::Factor(_)));
        let has_estimate = program.statements.iter().any(|s| matches!(s, Statement::Estimate(_)));
        if has_factor && has_estimate {
            return self.execute_factor_model(program);
        }

        // Find drivers, model, and base_rate
        let mut continuous_drivers: HashMap<String, &Distribution> = HashMap::new();
        let mut binary_drivers: HashMap<String, f64> = HashMap::new();
        let mut discrete_drivers: HashMap<String, (Vec<f64>, Vec<f64>)> = HashMap::new();
        let mut model_expr = None;
        let mut base_rate: Option<f64> = None;

        for stmt in &program.statements {
            match stmt {
                Statement::Question(question) => {
                    // Extract base_rate if present
                    if let Some(br) = &question.base_rate {
                        base_rate = Some(br.historical_frequency);
                    }
                }
                Statement::Driver(driver) => match driver.driver_type {
                    DriverType::Continuous => {
                        if let Some(ref dist) = driver.distribution {
                            continuous_drivers.insert(driver.name.clone(), dist);
                        }
                    }
                    DriverType::Binary => {
                        if let Some(prob) = driver.probability {
                            binary_drivers.insert(driver.name.clone(), prob);
                        }
                    }
                    DriverType::Discrete => {
                        if let (Some(ref values), Some(ref weights)) =
                            (&driver.values, &driver.weights)
                        {
                            discrete_drivers
                                .insert(driver.name.clone(), (values.clone(), weights.clone()));
                        }
                    }
                },
                Statement::Model(model) => {
                    model_expr = Some(&model.expression);
                }
                _ => {}
            }
        }

        let model_expr = model_expr.ok_or(ExecutionError::NoModelFound)?;

        // Run Monte Carlo simulation
        let mut samples = Vec::with_capacity(self.iterations);

        for _ in 0..self.iterations {
            // Sample from each driver
            let mut ctx = EvaluationContext::new();

            // Sample continuous drivers (or use fixed value if specified)
            for (name, dist) in &continuous_drivers {
                let sample = if let Some(&fixed_value) = self.fixed_drivers.get(name) {
                    fixed_value
                } else {
                    self.sample_distribution(dist, &ctx)?
                };
                ctx.set(name.clone(), sample);
            }

            // Sample binary drivers (Bernoulli trials, or use fixed value)
            for (name, prob) in &binary_drivers {
                let sample = if let Some(&fixed_value) = self.fixed_drivers.get(name) {
                    fixed_value
                } else if self.rng.gen::<f64>() < *prob {
                    1.0
                } else {
                    0.0
                };
                ctx.set(name.clone(), sample);
            }

            // Sample discrete drivers (categorical distribution, or use fixed value)
            for (name, (values, weights)) in &discrete_drivers {
                let sample = if let Some(&fixed_value) = self.fixed_drivers.get(name) {
                    fixed_value
                } else {
                    self.sample_categorical(values, weights)
                };
                ctx.set(name.clone(), sample);
            }

            // Evaluate model expression
            let result = evaluate(model_expr, &ctx)
                .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;

            samples.push(result);
        }

        // Calculate statistics
        // calculate_statistics returns: (mean, stddev, p10, p50, p90)
        let (mean, std_dev, _p10, median, _p90) = calculate_statistics(&samples);

        // Calculate additional percentiles
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        let p5 = sorted[(sorted.len() as f64 * 0.05) as usize];
        let p25 = sorted[(sorted.len() as f64 * 0.25) as usize];
        let p75 = sorted[(sorted.len() as f64 * 0.75) as usize];
        let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];

        // Calculate divergence if base_rate is present
        let (divergence_relative, divergence_absolute) = if let Some(br) = base_rate {
            let div_abs = mean - br;
            let div_rel = if br != 0.0 { div_abs / br } else { 0.0 };
            (Some(div_rel), Some(div_abs))
        } else {
            (None, None)
        };

        Ok(ExecutionResults {
            samples,
            mean,
            median,
            std_dev,
            p5,
            p25,
            p75,
            p95,
            min,
            max,
            iterations: self.iterations,
            base_rate,
            divergence_relative,
            divergence_absolute,
            factor_means: None,
            factor_std_devs: None,
            factor_corr_max: None,
            estimate_name: None,
            param_bindings: None,
            learnable_manifest: None,
        })
    }

    /// Execute with a specific number of iterations (override default)
    pub fn execute_with_iterations(
        &mut self,
        program: &Program,
        iterations: usize,
    ) -> ExecutionResult2<ExecutionResults> {
        let old_iterations = self.iterations;
        self.iterations = iterations;
        let result = self.execute(program);
        self.iterations = old_iterations;
        result
    }

    /// Execute a factor-model program.
    ///
    /// Pipeline:
    ///   1. Bind params into an evaluation context (from `self.params`).
    ///   2. For each Monte Carlo iteration:
    ///      a. For each factor in declaration order:
    ///         - Compute deterministic component: either the explicit
    ///           `formulation` expression (typical for X2..X6) or, when absent,
    ///           the equally-weighted mean of input values (treating missing
    ///           inputs as zero — the inputs not present in params are factor
    ///           covariates whose joint contribution is reflected in noise).
    ///         - Add Gaussian noise with variance equal to `variance_share`,
    ///           interpreted as the share of total response variance attributable
    ///           to that factor's stochastic component.
    ///         - Store the realized factor value in context (so downstream
    ///           factors can residualize against it via FactorRef).
    ///      b. Evaluate the first `estimate` expression — this is the model
    ///         response (e.g. tournament_strength). One sample per iteration.
    ///   3. Aggregate: per-factor means/std-devs, response statistics, max
    ///      pairwise correlation across factors (orthogonality diagnostic).
    ///
    /// Note: true OLS residualization across the sample matrix (subtracting
    /// projections) is the Phase-4 orthogonality pipeline. This executor
    /// runs the factor model end-to-end at parser-level expressivity; the
    /// residualization wrapper in `Expression::Residual` is honored as a
    /// pass-through here (the evaluator already does so).
    pub fn execute_factor_model(
        &mut self,
        program: &Program,
    ) -> ExecutionResult2<ExecutionResults> {
        // Clone the program so we can assign learnable names without mutating
        // the caller's AST. The naming pass walks every learnable() literal
        // in factor formulations and the estimate expression, assigning a
        // stable, deterministic identifier of the form `<owner>_l<idx>`
        // (e.g. `tournament_strength_l0`, `X3_l0`). These names are how
        // BayesOps reaches in: writing `params.tournament_strength_l0 = 0.31`
        // in the workspace's .app/params.json replaces the prior's initial
        // value at the next sim. See `assign_learnable_names`.
        let mut program = program.clone();
        Self::assign_learnable_names(&mut program);
        let mut learnable_names: Vec<LearnableInfo> = Vec::new();
        Self::collect_learnable_info(&program, &mut learnable_names);

        // Collect factor declarations, the estimate, and the base rate.
        let factors: Vec<&crate::ast::FactorStmt> = program.statements.iter()
            .filter_map(|s| match s { Statement::Factor(f) => Some(f), _ => None })
            .collect();
        let estimate = program.statements.iter().find_map(|s| match s {
            Statement::Estimate(e) => Some(e),
            _ => None,
        }).ok_or(ExecutionError::NoModelFound)?;
        let base_rate: Option<f64> = program.statements.iter().find_map(|s| match s {
            Statement::Question(q) => q.base_rate.as_ref().map(|b| b.historical_frequency),
            _ => None,
        });

        if factors.is_empty() {
            return Err(ExecutionError::NoModelFound);
        }

        // Per-factor sample buffers (kept in declaration order).
        let mut factor_samples: Vec<Vec<f64>> = factors.iter().map(|_| Vec::with_capacity(self.iterations)).collect();
        let mut response_samples: Vec<f64> = Vec::with_capacity(self.iterations);

        // Pre-bind all factor-formulation identifiers that aren't in params to
        // 0.0 (neutral baseline). Without this, formulations that reference
        // covariates not present in the params CSV (e.g. league_revenue_log)
        // would abort with UndefinedVariable. The factor's variance share still
        // captures their stochastic contribution via noise. Once Phase 2 of the
        // data pipeline lands, those covariates will be real params and the
        // zero baseline simply drops out.
        let mut implicit_zero_inputs: Vec<String> = Vec::new();
        for factor in &factors {
            for input in &factor.inputs {
                if !self.params.contains_key(&input.name)
                    && !implicit_zero_inputs.contains(&input.name)
                {
                    implicit_zero_inputs.push(input.name.clone());
                }
            }
        }

        for _ in 0..self.iterations {
            let mut ctx = EvaluationContext::new();
            // Bind params.
            for (name, value) in &self.params {
                ctx.set(name.clone(), *value);
            }
            // Bind implicit-zero inputs (factor covariates not in params).
            for name in &implicit_zero_inputs {
                ctx.set(name.clone(), 0.0);
            }

            // Sample each factor in order, accumulating into ctx.
            for (idx, factor) in factors.iter().enumerate() {
                // 1) Deterministic component.
                let deterministic = if let Some(formulation) = &factor.formulation {
                    // Evaluate the formulation expression with current ctx
                    // (params + already-sampled upstream factors).
                    evaluate(formulation, &ctx)
                        .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?
                } else {
                    // No explicit formulation: equally-weighted mean of inputs.
                    // Inputs are looked up in ctx; missing ones treated as 0.
                    if factor.inputs.is_empty() {
                        0.0
                    } else {
                        let sum: f64 = factor.inputs.iter()
                            .map(|inp| ctx.get(&inp.name).unwrap_or(0.0))
                            .sum();
                        sum / factor.inputs.len() as f64
                    }
                };

                // 2) Noise scaled by variance share.
                // variance_share interpreted as the variance of the factor's
                // stochastic component. sigma = sqrt(variance_share).
                let sigma = factor.variance_share.max(0.0).sqrt();
                let noise = if sigma > 0.0 {
                    sample_normal(&mut self.rng, 0.0, sigma)
                } else {
                    0.0
                };

                // Shift to a positive baseline: factors are interpreted as
                // multiplicative strength scores in Cobb-Douglas, so we
                // center the distribution at 1.0 rather than 0.0. This makes
                // `X_k ^ alpha` well-defined regardless of param normalization,
                // and matches the intuition that a "neutral" factor multiplies
                // tournament strength by 1.0.
                //
                // Floor at a small positive value to guard against extreme
                // negative draws (very low probability for vs=0.25 noise) that
                // would otherwise NaN out the Cobb-Douglas response.
                let factor_value = (1.0 + deterministic + noise).max(1e-6);
                ctx.set(factor.name.clone(), factor_value);
                factor_samples[idx].push(factor_value);
            }

            // 3) Evaluate the response.
            let response = evaluate(&estimate.expression, &ctx)
                .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;

            // Guard NaN/Inf — Cobb-Douglas on negative bases will blow up;
            // skip those iterations rather than poison the aggregate.
            if response.is_finite() {
                response_samples.push(response);
            }
        }

        if response_samples.is_empty() {
            return Err(ExecutionError::EvaluationError(
                "Factor model produced no finite response samples (check Cobb-Douglas bases)".into(),
            ));
        }

        // Aggregate response.
        let (mean, std_dev, _p10, median, _p90) = calculate_statistics(&response_samples);
        let mut sorted = response_samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = sorted.len();
        let min = sorted[0];
        let max = sorted[n - 1];
        let pct = |q: f64| sorted[((n as f64 * q) as usize).min(n - 1)];
        let p5 = pct(0.05);
        let p25 = pct(0.25);
        let p75 = pct(0.75);
        let p95 = pct(0.95);

        // Per-factor stats.
        let mut factor_means: HashMap<String, f64> = HashMap::new();
        let mut factor_std_devs: HashMap<String, f64> = HashMap::new();
        for (idx, factor) in factors.iter().enumerate() {
            let (fm, fsd, _, _, _) = calculate_statistics(&factor_samples[idx]);
            factor_means.insert(factor.name.clone(), fm);
            factor_std_devs.insert(factor.name.clone(), fsd);
        }

        // Max pairwise correlation across factors — orthogonality diagnostic.
        let factor_corr_max = pairwise_max_abs_corr(&factor_samples);

        // Divergence.
        let (divergence_relative, divergence_absolute) = if let Some(br) = base_rate {
            let div_abs = mean - br;
            let div_rel = if br != 0.0 { div_abs / br } else { 0.0 };
            (Some(div_rel), Some(div_abs))
        } else {
            (None, None)
        };

        Ok(ExecutionResults {
            samples: response_samples,
            mean,
            median,
            std_dev,
            p5, p25, p75, p95,
            min, max,
            iterations: self.iterations,
            base_rate,
            divergence_relative,
            divergence_absolute,
            factor_means: Some(factor_means),
            factor_std_devs: Some(factor_std_devs),
            factor_corr_max: Some(factor_corr_max),
            estimate_name: Some(estimate.name.clone()),
            param_bindings: Some(self.params.clone()),
            learnable_manifest: Some(learnable_names),
        })
    }

    /// Sample from a distribution, evaluating any expression parameters
    fn sample_distribution(
        &mut self,
        dist: &Distribution,
        ctx: &EvaluationContext,
    ) -> ExecutionResult2<f64> {
        match dist {
            Distribution::Triangular { p5, p50, p95 } => {
                let p5_val = evaluate(p5, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let p50_val = evaluate(p50, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let p95_val = evaluate(p95, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                Ok(sample_triangular(&mut self.rng, p5_val, p50_val, p95_val))
            }
            Distribution::Normal { mean, stddev } => {
                let mean_val = evaluate(mean, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let stddev_val = evaluate(stddev, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                Ok(sample_normal(&mut self.rng, mean_val, stddev_val))
            }
            Distribution::Lognormal { median, sigma } => {
                let median_val = evaluate(median, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let sigma_val = evaluate(sigma, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                Ok(sample_lognormal(&mut self.rng, median_val, sigma_val))
            }
            Distribution::Uniform { low, high } => {
                let low_val = evaluate(low, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let high_val = evaluate(high, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                Ok(sample_uniform(&mut self.rng, low_val, high_val))
            }
            Distribution::Beta {
                alpha,
                beta,
                min,
                max,
            } => {
                let alpha_val = evaluate(alpha, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let beta_val = evaluate(beta, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let min_val = if let Some(min_expr) = min {
                    evaluate(min_expr, ctx)
                        .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?
                } else {
                    0.0
                };
                let max_val = if let Some(max_expr) = max {
                    evaluate(max_expr, ctx)
                        .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?
                } else {
                    1.0
                };
                Ok(sample_beta(
                    &mut self.rng,
                    alpha_val,
                    beta_val,
                    min_val,
                    max_val,
                ))
            }
        }
    }

    /// Sample from a categorical (discrete) distribution
    /// Uses inverse transform sampling with cumulative weights
    #[allow(clippy::ptr_arg)]
    fn sample_categorical(&mut self, values: &[f64], weights: &[f64]) -> f64 {
        // Generate random number between 0 and 1
        let r = self.rng.gen::<f64>();

        // Compute cumulative sum
        let mut cumulative = 0.0;
        for (i, &weight) in weights.iter().enumerate() {
            cumulative += weight;
            if r < cumulative {
                return values[i];
            }
        }

        // Fallback to last value (handles floating-point rounding)
        values[values.len() - 1]
    }
}

/// Metadata about a `learnable(...)` literal — used to publish the
/// learnable manifest as a workspace output so BayesOps knows what knobs
/// are available to update without parsing the FPL itself.
#[derive(Debug, Clone)]
pub struct LearnableInfo {
    pub name: String,
    pub initial: f64,
    pub sigma: f64,
    /// Where this learnable lives in the AST — informational only.
    pub owner: String,
}

impl Executor {
    /// Assign deterministic names to every `learnable(...)` literal in the
    /// program. Names are positional within their owning statement
    /// (factor formulation or estimate expression), e.g.
    /// `tournament_strength_l0`, `X3_l0`, etc.
    ///
    /// Idempotent: if a learnable already has Some(name) it is left alone.
    /// This is called once at the start of `execute_factor_model`.
    pub fn assign_learnable_names(program: &mut Program) {
        for stmt in &mut program.statements {
            match stmt {
                Statement::Factor(f) => {
                    let owner = f.name.clone();
                    if let Some(formulation) = &mut f.formulation {
                        let mut idx = 0usize;
                        Self::walk_assign(formulation, &owner, &mut idx);
                    }
                }
                Statement::Estimate(e) => {
                    let owner = e.name.clone();
                    let mut idx = 0usize;
                    Self::walk_assign(&mut e.expression, &owner, &mut idx);
                }
                _ => {}
            }
        }
    }

    /// Depth-first walk: assign a name to each unnamed `LearnablePrior` we
    /// encounter, in source order.
    fn walk_assign(expr: &mut Expression, owner: &str, idx: &mut usize) {
        match expr {
            Expression::LearnablePrior { name, .. } => {
                if name.is_none() {
                    *name = Some(format!("{}_l{}", owner, *idx));
                    *idx += 1;
                }
            }
            Expression::Add(a, b)
            | Expression::Subtract(a, b)
            | Expression::Multiply(a, b)
            | Expression::Divide(a, b)
            | Expression::Modulo(a, b)
            | Expression::Power(a, b)
            | Expression::Equal(a, b)
            | Expression::NotEqual(a, b)
            | Expression::Greater(a, b)
            | Expression::Less(a, b)
            | Expression::GreaterEqual(a, b)
            | Expression::LessEqual(a, b)
            | Expression::And(a, b)
            | Expression::Or(a, b) => {
                Self::walk_assign(a, owner, idx);
                Self::walk_assign(b, owner, idx);
            }
            Expression::Not(a) | Expression::Exp(a) => Self::walk_assign(a, owner, idx),
            Expression::If { condition, then_expr, else_expr } => {
                Self::walk_assign(condition, owner, idx);
                Self::walk_assign(then_expr, owner, idx);
                Self::walk_assign(else_expr, owner, idx);
            }
            Expression::FunctionCall { args, .. } => {
                for a in args { Self::walk_assign(a, owner, idx); }
            }
            Expression::Residual { raw, .. } => Self::walk_assign(raw, owner, idx),
            // Terminal nodes — nothing to descend into.
            Expression::Number(_) | Expression::Probability(_) | Expression::String(_)
            | Expression::Boolean(_) | Expression::Identifier(_) | Expression::ParamRef(_)
            | Expression::FactorRef(_) => {}
        }
    }

    /// Walk the (now-named) program collecting metadata about every
    /// learnable. Used to publish the manifest as a workspace output.
    pub fn collect_learnable_info(program: &Program, out: &mut Vec<LearnableInfo>) {
        for stmt in &program.statements {
            match stmt {
                Statement::Factor(f) => {
                    if let Some(formulation) = &f.formulation {
                        Self::walk_collect(formulation, &f.name, out);
                    }
                }
                Statement::Estimate(e) => Self::walk_collect(&e.expression, &e.name, out),
                _ => {}
            }
        }
    }

    fn walk_collect(expr: &Expression, owner: &str, out: &mut Vec<LearnableInfo>) {
        match expr {
            Expression::LearnablePrior { initial, sigma, name } => {
                if let Some(n) = name {
                    out.push(LearnableInfo {
                        name: n.clone(),
                        initial: *initial,
                        sigma: *sigma,
                        owner: owner.to_string(),
                    });
                }
            }
            Expression::Add(a, b)
            | Expression::Subtract(a, b)
            | Expression::Multiply(a, b)
            | Expression::Divide(a, b)
            | Expression::Modulo(a, b)
            | Expression::Power(a, b)
            | Expression::Equal(a, b)
            | Expression::NotEqual(a, b)
            | Expression::Greater(a, b)
            | Expression::Less(a, b)
            | Expression::GreaterEqual(a, b)
            | Expression::LessEqual(a, b)
            | Expression::And(a, b)
            | Expression::Or(a, b) => {
                Self::walk_collect(a, owner, out);
                Self::walk_collect(b, owner, out);
            }
            Expression::Not(a) | Expression::Exp(a) => Self::walk_collect(a, owner, out),
            Expression::If { condition, then_expr, else_expr } => {
                Self::walk_collect(condition, owner, out);
                Self::walk_collect(then_expr, owner, out);
                Self::walk_collect(else_expr, owner, out);
            }
            Expression::FunctionCall { args, .. } => {
                for a in args { Self::walk_collect(a, owner, out); }
            }
            Expression::Residual { raw, .. } => Self::walk_collect(raw, owner, out),
            _ => {}
        }
    }
}

/// Pearson correlation between two equal-length sample arrays.
/// Returns 0.0 for degenerate (constant) inputs rather than NaN.
fn pearson_corr(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let n = a.len() as f64;
    let mean_a: f64 = a.iter().sum::<f64>() / n;
    let mean_b: f64 = b.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for i in 0..a.len() {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    let denom = (var_a * var_b).sqrt();
    if denom == 0.0 { 0.0 } else { cov / denom }
}

/// Max absolute pairwise Pearson correlation across all factor sample columns.
/// Used as an orthogonality diagnostic for the factor model.
fn pairwise_max_abs_corr(factor_samples: &[Vec<f64>]) -> f64 {
    let k = factor_samples.len();
    let mut max_abs = 0.0_f64;
    for i in 0..k {
        for j in (i + 1)..k {
            let c = pearson_corr(&factor_samples[i], &factor_samples[j]).abs();
            if c > max_abs {
                max_abs = c;
            }
        }
    }
    max_abs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    #[test]
    fn test_executor_simple() {
        // question "test"
        // driver continuous x {
        //     distribution: triangular(10, 20, 30)
        // }
        // model x
        let program = Program {
            statements: vec![
                Statement::Question(QuestionStmt {
                    text: "test".to_string(),
                    base_rate: None,
                    target_date: None,
                    resolution_criteria: None,
                }),
                Statement::Driver(DriverStmt {
                    name: "x".to_string(),
                    display_name: None,
                    description: None,
                    driver_type: DriverType::Continuous,
                    distribution: Some(Distribution::Triangular {
                        p5: Expression::Number(10.0),
                        p50: Expression::Number(20.0),
                        p95: Expression::Number(30.0),
                    }),
                    probability: None,
                    impact_multiplier: None,
                    values: None,
                    weights: None,
                    unit: None,
                    rationale: None,
                    constraints: vec![],
                    evidence_refs: vec![],
                }),
                Statement::Model(ModelStmt {
                    expression: Expression::Identifier("x".to_string()),
                }),
            ],
        };

        let mut executor = Executor::with_seed(1000, 42);
        let results = executor.execute(&program).unwrap();

        assert_eq!(results.iterations, 1000);
        // Triangular distribution (10, 20, 30) has mean of 20
        // With the specific seed, we expect consistent results
        assert!(
            results.mean > 10.0 && results.mean < 30.0,
            "Mean {} out of range",
            results.mean
        );
        assert!(
            results.median > 10.0 && results.median < 30.0,
            "Median {} out of range",
            results.median
        );
    }

    #[test]
    fn test_executor_arithmetic() {
        // question "test"
        // driver continuous x { distribution: normal(100, 10) }
        // driver continuous y { distribution: normal(50, 5) }
        // model x + y
        let program = Program {
            statements: vec![
                Statement::Question(QuestionStmt {
                    text: "test".to_string(),
                    base_rate: None,
                    target_date: None,
                    resolution_criteria: None,
                }),
                Statement::Driver(DriverStmt {
                    name: "x".to_string(),
                    display_name: None,
                    description: None,
                    driver_type: DriverType::Continuous,
                    distribution: Some(Distribution::Normal {
                        mean: Expression::Number(100.0),
                        stddev: Expression::Number(10.0),
                    }),
                    probability: None,
                    impact_multiplier: None,
                    values: None,
                    weights: None,
                    unit: None,
                    rationale: None,
                    constraints: vec![],
                    evidence_refs: vec![],
                }),
                Statement::Driver(DriverStmt {
                    name: "y".to_string(),
                    display_name: None,
                    description: None,
                    driver_type: DriverType::Continuous,
                    distribution: Some(Distribution::Normal {
                        mean: Expression::Number(50.0),
                        stddev: Expression::Number(5.0),
                    }),
                    probability: None,
                    impact_multiplier: None,
                    values: None,
                    weights: None,
                    unit: None,
                    rationale: None,
                    constraints: vec![],
                    evidence_refs: vec![],
                }),
                Statement::Model(ModelStmt {
                    expression: Expression::Add(
                        Box::new(Expression::Identifier("x".to_string())),
                        Box::new(Expression::Identifier("y".to_string())),
                    ),
                }),
            ],
        };

        let mut executor = Executor::with_seed(1000, 42);
        let results = executor.execute(&program).unwrap();

        // Mean should be approximately 150 (100 + 50)
        assert!(results.mean > 145.0 && results.mean < 155.0);
    }

    #[test]
    fn test_histogram() {
        let results = ExecutionResults {
            samples: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            mean: 3.0,
            median: 3.0,
            std_dev: 1.0,
            p5: 1.0,
            p25: 2.0,
            p75: 4.0,
            p95: 5.0,
            min: 1.0,
            max: 5.0,
            iterations: 5,
            base_rate: None,
            divergence_relative: None,
            divergence_absolute: None,
            factor_means: None,
            factor_std_devs: None,
            factor_corr_max: None,
            estimate_name: None,
            param_bindings: None,
            learnable_manifest: None,
        };

        let histogram = results.histogram(5);
        assert_eq!(histogram.len(), 5);
    }
}
