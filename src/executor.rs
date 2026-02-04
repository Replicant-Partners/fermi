/// Execution engine for Monte Carlo simulation
///
/// This module orchestrates the execution of FPL forecasts by:
/// 1. Sampling from driver distributions
/// 2. Evaluating the model expression for each iteration
/// 3. Collecting statistics from the results

use crate::ast::{Program, Statement, DriverStmt, ModelStmt, SimulateStmt, Distribution};
use crate::evaluator::{EvaluationContext, evaluate, EvalError};
use crate::distributions::{
    sample_triangular, sample_normal, sample_lognormal,
    sample_uniform, sample_beta, calculate_statistics
};
use rand::Rng;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// Execution result containing simulation statistics
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Number of iterations run
    pub iterations: usize,

    /// Mean of the simulation
    pub mean: f64,

    /// Standard deviation of the simulation
    pub stddev: f64,

    /// 10th percentile
    pub p10: f64,

    /// 50th percentile (median)
    pub p50: f64,

    /// 90th percentile
    pub p90: f64,

    /// All sampled values (for advanced analysis)
    pub samples: Vec<f64>,
}

impl ExecutionResult {
    /// Get the 80% confidence interval (p10 to p90)
    pub fn confidence_interval_80(&self) -> (f64, f64) {
        (self.p10, self.p90)
    }

    /// Get the interquartile range (p25 to p75)
    pub fn interquartile_range(&self) -> (f64, f64) {
        let mut sorted = self.samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let n = sorted.len();
        let p25_idx = (n as f64 * 0.25) as usize;
        let p75_idx = (n as f64 * 0.75) as usize;

        (sorted[p25_idx], sorted[p75_idx])
    }
}

/// Execution error
#[derive(Debug, Clone)]
pub enum ExecutionError {
    /// No model statement found
    NoModel,

    /// No simulate statement found
    NoSimulate,

    /// Driver not found during sampling
    DriverNotFound(String),

    /// Invalid distribution parameters
    InvalidDistribution(String),

    /// Evaluation error during simulation
    EvaluationError(String),

    /// No drivers to sample
    NoDrivers,
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ExecutionError::NoModel => {
                write!(f, "No model statement found in forecast")
            }
            ExecutionError::NoSimulate => {
                write!(f, "No simulate statement found in forecast")
            }
            ExecutionError::DriverNotFound(name) => {
                write!(f, "Driver '{}' not found", name)
            }
            ExecutionError::InvalidDistribution(msg) => {
                write!(f, "Invalid distribution: {}", msg)
            }
            ExecutionError::EvaluationError(msg) => {
                write!(f, "Evaluation error: {}", msg)
            }
            ExecutionError::NoDrivers => {
                write!(f, "No drivers to sample (forecast has no uncertainty)")
            }
        }
    }
}

impl std::error::Error for ExecutionError {}

pub type ExecutionResult2<T> = Result<T, ExecutionError>;

/// Driver information for execution
#[derive(Debug, Clone)]
struct DriverInfo {
    name: String,
    driver_type: DriverType,
}

#[derive(Debug, Clone)]
enum DriverType {
    Continuous(Distribution),
    Binary { probability: f64, impact: Option<f64> },
}

/// Executor for running Monte Carlo simulations
pub struct Executor {
    /// Random number generator
    rng: StdRng,

    /// Drivers to sample
    drivers: Vec<DriverInfo>,

    /// Model expression to evaluate
    model: Option<ModelStmt>,

    /// Number of iterations
    iterations: usize,
}

impl Executor {
    /// Create a new executor with a random seed
    pub fn new() -> Self {
        Self::with_seed(rand::random())
    }

    /// Create a new executor with a specific seed (for reproducibility)
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            drivers: Vec::new(),
            model: None,
            iterations: 10_000,
        }
    }

    /// Load a program for execution
    pub fn load_program(&mut self, program: &Program) -> ExecutionResult2<()> {
        // Extract drivers
        for stmt in &program.statements {
            if let Statement::Driver(driver) = stmt {
                self.add_driver(driver)?;
            }
        }

        // Extract model
        for stmt in &program.statements {
            if let Statement::Model(model) = stmt {
                self.model = Some(model.clone());
                break;
            }
        }

        // Extract iteration count
        for stmt in &program.statements {
            if let Statement::Simulate(sim) = stmt {
                self.iterations = sim.iterations;
                break;
            }
        }

        // Validate
        if self.model.is_none() {
            return Err(ExecutionError::NoModel);
        }

        if self.drivers.is_empty() {
            return Err(ExecutionError::NoDrivers);
        }

        Ok(())
    }

    /// Add a driver to the executor
    fn add_driver(&mut self, driver: &DriverStmt) -> ExecutionResult2<()> {
        let driver_type = match &driver.driver_type {
            crate::ast::DriverType::Continuous => {
                // Must have a distribution
                let dist = driver.distribution.as_ref()
                    .ok_or_else(|| ExecutionError::InvalidDistribution(
                        format!("Continuous driver '{}' has no distribution", driver.name)
                    ))?;
                DriverType::Continuous(dist.clone())
            }
            crate::ast::DriverType::Binary => {
                // Must have a probability
                let prob = driver.probability
                    .ok_or_else(|| ExecutionError::InvalidDistribution(
                        format!("Binary driver '{}' has no probability", driver.name)
                    ))?;
                DriverType::Binary {
                    probability: prob,
                    impact: driver.impact_multiplier,
                }
            }
        };

        self.drivers.push(DriverInfo {
            name: driver.name.clone(),
            driver_type,
        });

        Ok(())
    }

    /// Run the Monte Carlo simulation
    pub fn execute(&mut self) -> ExecutionResult2<ExecutionResult> {
        let model = self.model.as_ref()
            .ok_or(ExecutionError::NoModel)?;

        let mut samples = Vec::with_capacity(self.iterations);

        // Run iterations
        for _ in 0..self.iterations {
            // Sample all drivers
            let ctx = self.sample_drivers()?;

            // Evaluate model
            let value = evaluate(&model.expression, &ctx)
                .map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;

            samples.push(value);
        }

        // Calculate statistics
        let (mean, stddev, p10, p50, p90) = calculate_statistics(&samples);

        Ok(ExecutionResult {
            iterations: self.iterations,
            mean,
            stddev,
            p10,
            p50,
            p90,
            samples,
        })
    }

    /// Sample all drivers for one iteration
    fn sample_drivers(&mut self) -> ExecutionResult2<EvaluationContext> {
        let mut ctx = EvaluationContext::new();

        for driver in &self.drivers {
            let value = match &driver.driver_type {
                DriverType::Continuous(dist) => {
                    self.sample_distribution(dist)?
                }
                DriverType::Binary { probability, impact } => {
                    self.sample_binary(*probability, *impact)
                }
            };

            ctx.set(driver.name.clone(), value);
        }

        Ok(ctx)
    }

    /// Sample from a distribution
    fn sample_distribution(&mut self, dist: &Distribution) -> ExecutionResult2<f64> {
        match dist {
            Distribution::Triangular { p5, p50, p95 } => {
                Ok(sample_triangular(&mut self.rng, *p5, *p50, *p95))
            }
            Distribution::Normal { mean, stddev } => {
                Ok(sample_normal(&mut self.rng, *mean, *stddev))
            }
            Distribution::Lognormal { median, sigma } => {
                Ok(sample_lognormal(&mut self.rng, *median, *sigma))
            }
            Distribution::Uniform { low, high } => {
                Ok(sample_uniform(&mut self.rng, *low, *high))
            }
            Distribution::Beta { alpha, beta, min, max } => {
                Ok(sample_beta(&mut self.rng, *alpha, *beta, *min, *max))
            }
        }
    }

    /// Sample a binary driver (returns 1.0 if true, impact or 0.0 if false)
    fn sample_binary(&mut self, probability: f64, impact: Option<f64>) -> f64 {
        let u: f64 = self.rng.gen();
        if u < probability {
            impact.unwrap_or(1.0)
        } else {
            if impact.is_some() {
                1.0 // If impact is specified, return 1.0 when false
            } else {
                0.0 // Otherwise return 0.0 (boolean)
            }
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to execute a program
pub fn execute_program(program: &Program) -> ExecutionResult2<ExecutionResult> {
    let mut executor = Executor::new();
    executor.load_program(program)?;
    executor.execute()
}

/// Convenience function to execute a program with a specific seed
pub fn execute_program_with_seed(program: &Program, seed: u64) -> ExecutionResult2<ExecutionResult> {
    let mut executor = Executor::with_seed(seed);
    executor.load_program(program)?;
    executor.execute()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expression, DriverType as AstDriverType};

    #[test]
    fn test_simple_forecast() {
        // Create a simple forecast:
        // driver x continuous { distribution: triangular(1, 5, 10) }
        // model: x
        // simulate 1000 iterations

        let driver = DriverStmt {
            name: "x".to_string(),
            driver_type: AstDriverType::Continuous,
            distribution: Some(Distribution::Triangular {
                p5: 1.0,
                p50: 5.0,
                p95: 10.0,
            }),
            unit: None,
            probability: None,
            impact_multiplier: None,
        };

        let model = ModelStmt {
            expression: Expression::Identifier("x".to_string()),
        };

        let simulate = SimulateStmt {
            iterations: 1000,
        };

        let program = Program {
            statements: vec![
                Statement::Driver(driver),
                Statement::Model(model),
                Statement::Simulate(simulate),
            ],
        };

        let mut executor = Executor::with_seed(42);
        executor.load_program(&program).unwrap();
        let result = executor.execute().unwrap();

        assert_eq!(result.iterations, 1000);
        assert!(result.mean > 4.0 && result.mean < 6.0); // Should be close to 5
        assert!(result.p50 > 4.0 && result.p50 < 6.0); // Median close to 5
        assert!(result.p10 > 1.0 && result.p10 < 3.0); // 10th percentile
        assert!(result.p90 > 7.0 && result.p90 < 10.0); // 90th percentile
    }

    #[test]
    fn test_arithmetic_model() {
        // driver a continuous { distribution: triangular(10, 20, 30) }
        // driver b continuous { distribution: triangular(2, 3, 4) }
        // model: a * b

        let driver_a = DriverStmt {
            name: "a".to_string(),
            driver_type: AstDriverType::Continuous,
            distribution: Some(Distribution::Triangular {
                p5: 10.0,
                p50: 20.0,
                p95: 30.0,
            }),
            unit: None,
            probability: None,
            impact_multiplier: None,
        };

        let driver_b = DriverStmt {
            name: "b".to_string(),
            driver_type: AstDriverType::Continuous,
            distribution: Some(Distribution::Triangular {
                p5: 2.0,
                p50: 3.0,
                p95: 4.0,
            }),
            unit: None,
            probability: None,
            impact_multiplier: None,
        };

        let model = ModelStmt {
            expression: Expression::Multiply(
                Box::new(Expression::Identifier("a".to_string())),
                Box::new(Expression::Identifier("b".to_string())),
            ),
        };

        let program = Program {
            statements: vec![
                Statement::Driver(driver_a),
                Statement::Driver(driver_b),
                Statement::Model(model),
                Statement::Simulate(SimulateStmt { iterations: 10_000 }),
            ],
        };

        let result = execute_program_with_seed(&program, 42).unwrap();

        // Mean should be around 20 * 3 = 60
        assert!(result.mean > 50.0 && result.mean < 70.0);
    }

    #[test]
    fn test_binary_driver() {
        // driver success binary { probability: 0.7p }
        // model: if success then 100 else 0

        let driver = DriverStmt {
            name: "success".to_string(),
            driver_type: AstDriverType::Binary,
            distribution: None,
            unit: None,
            probability: Some(0.7),
            impact_multiplier: None,
        };

        let model = ModelStmt {
            expression: Expression::If {
                condition: Box::new(Expression::Identifier("success".to_string())),
                then_expr: Box::new(Expression::Number(100.0)),
                else_expr: Box::new(Expression::Number(0.0)),
            },
        };

        let program = Program {
            statements: vec![
                Statement::Driver(driver),
                Statement::Model(model),
                Statement::Simulate(SimulateStmt { iterations: 10_000 }),
            ],
        };

        let result = execute_program_with_seed(&program, 42).unwrap();

        // Mean should be around 70 (70% success * 100)
        assert!(result.mean > 65.0 && result.mean < 75.0);

        // p50 should be 100 (because probability > 0.5)
        assert_eq!(result.p50, 100.0);
    }

    #[test]
    fn test_complex_model() {
        // driver market_size continuous { distribution: triangular(500, 1200, 2500) }
        // driver growth_rate continuous { distribution: normal(0.25, 0.05) }
        // driver major_contract binary { probability: 0.6p, impact_multiplier: 1.5 }
        // model: market_size * (1 + growth_rate) * major_contract

        let driver_market = DriverStmt {
            name: "market_size".to_string(),
            driver_type: AstDriverType::Continuous,
            distribution: Some(Distribution::Triangular {
                p5: 500.0,
                p50: 1200.0,
                p95: 2500.0,
            }),
            unit: Some("millions USD".to_string()),
            probability: None,
            impact_multiplier: None,
        };

        let driver_growth = DriverStmt {
            name: "growth_rate".to_string(),
            driver_type: AstDriverType::Continuous,
            distribution: Some(Distribution::Normal {
                mean: 0.25,
                stddev: 0.05,
            }),
            unit: Some("ratio".to_string()),
            probability: None,
            impact_multiplier: None,
        };

        let driver_contract = DriverStmt {
            name: "major_contract".to_string(),
            driver_type: AstDriverType::Binary,
            distribution: None,
            unit: None,
            probability: Some(0.6),
            impact_multiplier: Some(1.5),
        };

        let model = ModelStmt {
            expression: Expression::Multiply(
                Box::new(Expression::Multiply(
                    Box::new(Expression::Identifier("market_size".to_string())),
                    Box::new(Expression::Add(
                        Box::new(Expression::Number(1.0)),
                        Box::new(Expression::Identifier("growth_rate".to_string())),
                    )),
                )),
                Box::new(Expression::Identifier("major_contract".to_string())),
            ),
        };

        let program = Program {
            statements: vec![
                Statement::Driver(driver_market),
                Statement::Driver(driver_growth),
                Statement::Driver(driver_contract),
                Statement::Model(model),
                Statement::Simulate(SimulateStmt { iterations: 10_000 }),
            ],
        };

        let result = execute_program_with_seed(&program, 42).unwrap();

        // Rough estimate: 1200 * 1.25 * (0.6 * 1.5 + 0.4 * 1.0) = 1500 * 1.3 = 1950
        assert!(result.mean > 1500.0 && result.mean < 2500.0);
        assert!(result.p10 > 500.0);
        assert!(result.p90 < 5000.0);
    }

    #[test]
    fn test_no_model_error() {
        let program = Program {
            statements: vec![
                Statement::Simulate(SimulateStmt { iterations: 1000 }),
            ],
        };

        let result = execute_program(&program);
        assert!(matches!(result, Err(ExecutionError::NoModel)));
    }

    #[test]
    fn test_no_drivers_error() {
        let program = Program {
            statements: vec![
                Statement::Model(ModelStmt {
                    expression: Expression::Number(42.0),
                }),
                Statement::Simulate(SimulateStmt { iterations: 1000 }),
            ],
        };

        let result = execute_program(&program);
        assert!(matches!(result, Err(ExecutionError::NoDrivers)));
    }
}
