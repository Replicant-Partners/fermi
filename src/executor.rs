/// Monte Carlo Executor
///
/// Runs Monte Carlo simulations on FPL programs:
/// 1. Sampling from driver distributions
/// 2. Evaluating the model expression for each iteration
/// 3. Collecting statistics from the results

use crate::ast::{Program, Statement, DriverStmt, ModelStmt, Distribution};
use crate::evaluator::{EvaluationContext, evaluate};
use crate::distributions::{
    sample_triangular, sample_normal, sample_lognormal,
    sample_uniform, sample_beta, calculate_statistics
};
use rand::rngs::StdRng;
use rand::{SeedableRng, Rng};
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

        histogram.into_iter()
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
        }
    }

    pub fn with_seed(iterations: usize, seed: u64) -> Self {
        Self {
            iterations,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Execute a Monte Carlo simulation on the given program
    pub fn execute(&mut self, program: &Program) -> ExecutionResult2<ExecutionResults> {
        // Find drivers and model
        let mut drivers: HashMap<String, &Distribution> = HashMap::new();
        let mut model_expr = None;

        for stmt in &program.statements {
            match stmt {
                Statement::Driver(driver) => {
                    drivers.insert(driver.name.clone(), &driver.distribution);
                }
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
            // Sample from each driver distribution
            let mut ctx = EvaluationContext::new();

            for (name, dist) in &drivers {
                let sample = self.sample_distribution(dist, &ctx)?;
                ctx.set_variable(name, sample);
            }

            // Evaluate model expression
            let result = evaluate(model_expr, &ctx)
                .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;

            samples.push(result);
        }

        // Calculate statistics
        let (mean, median, std_dev, p5, p25, p75, p95, min, max) = calculate_statistics(&samples);

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
    fn sample_distribution(&mut self, dist: &Distribution, ctx: &EvaluationContext) -> ExecutionResult2<f64> {
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
            Distribution::Beta { alpha, beta, min, max } => {
                let alpha_val = evaluate(alpha, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let beta_val = evaluate(beta, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let min_val = evaluate(min, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                let max_val = evaluate(max, ctx)
                    .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;
                Ok(sample_beta(&mut self.rng, alpha_val, beta_val, min_val, max_val))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    #[test]
    fn test_executor_simple() {
        // forecast "test" {
        //     driver x triangular(10, 20, 30)
        //     estimate x
        // }
        let program = Program {
            statements: vec![
                Statement::Forecast(ForecastStmt {
                    title: Expression::String("test".to_string()),
                }),
                Statement::Driver(DriverStmt {
                    name: "x".to_string(),
                    distribution: Distribution::Triangular {
                        p5: Expression::Number(10.0),
                        p50: Expression::Number(20.0),
                        p95: Expression::Number(30.0),
                    },
                }),
                Statement::Model(ModelStmt {
                    expression: Expression::Identifier("x".to_string()),
                }),
            ],
        };

        let mut executor = Executor::with_seed(1000, 42);
        let results = executor.execute(&program).unwrap();

        assert_eq!(results.iterations, 1000);
        assert!(results.mean > 15.0 && results.mean < 25.0);
        assert!(results.median > 15.0 && results.median < 25.0);
    }

    #[test]
    fn test_executor_arithmetic() {
        // forecast "test" {
        //     driver x normal(100, 10)
        //     driver y normal(50, 5)
        //     estimate x + y
        // }
        let program = Program {
            statements: vec![
                Statement::Forecast(ForecastStmt {
                    title: Expression::String("test".to_string()),
                }),
                Statement::Driver(DriverStmt {
                    name: "x".to_string(),
                    distribution: Distribution::Normal {
                        mean: Expression::Number(100.0),
                        stddev: Expression::Number(10.0),
                    },
                }),
                Statement::Driver(DriverStmt {
                    name: "y".to_string(),
                    distribution: Distribution::Normal {
                        mean: Expression::Number(50.0),
                        stddev: Expression::Number(5.0),
                    },
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
        };

        let histogram = results.histogram(5);
        assert_eq!(histogram.len(), 5);
    }
}
