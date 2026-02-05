/// Monte Carlo Executor
///
/// Runs Monte Carlo simulations on FPL programs:
/// 1. Sampling from driver distributions
/// 2. Evaluating the model expression for each iteration
/// 3. Collecting statistics from the results
use crate::ast::{Distribution, DriverType, Program, Statement};
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
        }
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
        }
    }

    /// Execute a Monte Carlo simulation on the given program
    pub fn execute(&mut self, program: &Program) -> ExecutionResult2<ExecutionResults> {
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
        };

        let histogram = results.histogram(5);
        assert_eq!(histogram.len(), 5);
    }
}
