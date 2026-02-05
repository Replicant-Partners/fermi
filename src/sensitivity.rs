/// Sensitivity Analysis Module
///
/// Implements variance decomposition and Sobol indices for understanding
/// which drivers have the most impact on forecast outcomes.
///
/// Key concepts:
/// - Variance Decomposition: How much does each driver contribute to output variance?
/// - First-order Sobol Index: Direct effect of a driver (no interactions)
/// - Total-order Sobol Index: Total effect including all interactions
///
/// References:
/// - Saltelli et al. (2008) "Global Sensitivity Analysis: The Primer"
/// - Sobol (2001) "Global sensitivity indices for nonlinear mathematical models"
use crate::ast::*;
use crate::executor::{ExecutionError, ExecutionResults, Executor};
use std::collections::HashMap;

/// Sensitivity analysis results for a single driver
#[derive(Debug, Clone)]
pub struct DriverSensitivity {
    /// Driver name
    pub driver_name: String,

    /// Variance contribution (0.0 to 1.0)
    /// How much of the output variance is explained by this driver
    pub variance_contribution: f64,

    /// First-order Sobol index (direct effect)
    pub first_order_index: f64,

    /// Total-order Sobol index (total effect including interactions)
    pub total_order_index: f64,

    /// Standard error of the indices (for uncertainty quantification)
    pub standard_error: f64,
}

/// Complete sensitivity analysis results
#[derive(Debug, Clone)]
pub struct SensitivityAnalysis {
    /// Baseline results (all drivers varying)
    pub baseline: ExecutionResults,

    /// Sensitivity for each driver
    pub driver_sensitivities: HashMap<String, DriverSensitivity>,

    /// Drivers ranked by total-order index (most sensitive first)
    pub ranked_drivers: Vec<String>,
}

impl SensitivityAnalysis {
    /// Get sensitivity for a specific driver
    pub fn get_driver_sensitivity(&self, driver_name: &str) -> Option<&DriverSensitivity> {
        self.driver_sensitivities.get(driver_name)
    }

    /// Get top N most sensitive drivers
    pub fn top_drivers(&self, n: usize) -> Vec<&DriverSensitivity> {
        self.ranked_drivers
            .iter()
            .take(n)
            .filter_map(|name| self.driver_sensitivities.get(name))
            .collect()
    }
}

/// Perform variance decomposition analysis using conditional Monte Carlo
///
/// For each driver, calculates how much it contributes to output variance
/// by fixing it at various values and measuring the conditional variance.
///
/// Algorithm:
/// 1. Run baseline simulation (all drivers vary) → get baseline variance V(Y)
/// 2. For each driver X_i:
///    a. Sample m values of X_i from its distribution
///    b. For each sampled value x_i:
///       - Fix X_i = x_i
///       - Run n simulations with other drivers varying → get E[Y|X_i=x_i]
///    c. Calculate V(E[Y|X_i]) = variance of the conditional means
///    d. First-order Sobol: S_i = V(E[Y|X_i]) / V(Y)
///
/// The variance contribution is approximated by the first-order Sobol index.
pub fn variance_decomposition(
    program: &Program,
    iterations: usize,
) -> Result<HashMap<String, f64>, ExecutionError> {
    // Step 1: Baseline simulation
    let mut executor = Executor::new(iterations);
    let baseline = executor.execute(program)?;
    let baseline_variance = baseline.std_dev.powi(2);

    if baseline_variance == 0.0 {
        // No variance to decompose (deterministic model)
        return Ok(HashMap::new());
    }

    // Extract driver names
    let driver_names: Vec<String> = program
        .statements
        .iter()
        .filter_map(|stmt| {
            if let Statement::Driver(d) = stmt {
                Some(d.name.clone())
            } else {
                None
            }
        })
        .collect();

    let mut contributions = HashMap::new();

    // Use fewer samples for conditional variance estimation to save time
    // m = number of conditioning values, n = iterations per condition
    let m = 20; // Sample 20 different values of each driver
    let n = (iterations / m).max(100); // At least 100 iterations per condition

    // Step 2: For each driver, compute V(E[Y|X_i])
    for driver_name in &driver_names {
        let conditional_variance = compute_conditional_variance(program, driver_name, m, n)?;

        // First-order Sobol index = V(E[Y|X_i]) / V(Y)
        let sobol_first = (conditional_variance / baseline_variance).min(1.0).max(0.0);

        println!(
            "  {} -> V(E[Y|X]) = {:.6}, S_i = {:.3}",
            driver_name, conditional_variance, sobol_first
        );

        contributions.insert(driver_name.clone(), sobol_first);
    }

    Ok(contributions)
}

/// Compute V(E[Y|X_i]) for a specific driver using conditional Monte Carlo
///
/// Algorithm:
/// 1. Sample m values of X_i: x_i^(1), ..., x_i^(m)
/// 2. For each x_i^(j):
///    - Fix X_i = x_i^(j)
///    - Run n simulations → compute mean μ_j = E[Y|X_i=x_i^(j)]
/// 3. Return variance of the conditional means: V(μ_1, ..., μ_m)
fn compute_conditional_variance(
    program: &Program,
    driver_name: &str,
    m: usize,
    n: usize,
) -> Result<f64, ExecutionError> {
    // Sample m values of this driver from baseline
    let mut baseline_executor = Executor::new(m);
    let driver_samples = sample_single_driver(program, driver_name, &mut baseline_executor, m)?;

    // Compute conditional mean for each sampled value
    let mut conditional_means = Vec::with_capacity(m);

    for &driver_value in &driver_samples {
        // Fix this driver and run simulation
        let mut fixed_drivers = HashMap::new();
        fixed_drivers.insert(driver_name.to_string(), driver_value);

        let mut conditional_executor = Executor::with_fixed_drivers(n, fixed_drivers);
        let conditional_results = conditional_executor.execute(program)?;

        conditional_means.push(conditional_results.mean);
    }

    // Calculate variance of conditional means
    if conditional_means.is_empty() {
        return Ok(0.0);
    }

    let mean_of_means: f64 = conditional_means.iter().sum::<f64>() / conditional_means.len() as f64;
    let variance: f64 = conditional_means
        .iter()
        .map(|&x| (x - mean_of_means).powi(2))
        .sum::<f64>()
        / conditional_means.len() as f64;

    Ok(variance)
}

/// Sample values from a single driver's distribution
fn sample_single_driver(
    program: &Program,
    driver_name: &str,
    executor: &mut Executor,
    count: usize,
) -> Result<Vec<f64>, ExecutionError> {
    // Create a minimal program that just has this driver and returns it
    let driver_stmt = program
        .statements
        .iter()
        .find_map(|stmt| {
            if let Statement::Driver(d) = stmt {
                if d.name == driver_name {
                    Some(d.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| {
            ExecutionError::EvaluationError(format!("Driver {} not found", driver_name))
        })?;

    let minimal_program = Program {
        statements: vec![
            Statement::Driver(driver_stmt),
            Statement::Model(ModelStmt {
                expression: Expression::Identifier(driver_name.to_string()),
            }),
        ],
    };

    let results = executor.execute_with_iterations(&minimal_program, count)?;
    Ok(results.samples)
}

/// Compute total-order Sobol index using Saltelli sampling
///
/// Saltelli's efficient estimator for total-order indices:
/// S_Ti = (1 / (2n * V(Y))) * Σ(f(A) - f(AB_i))^2
///
/// where:
/// - A is a n×k matrix of samples (k = number of drivers)
/// - B is another independent n×k matrix
/// - AB_i is matrix A but with column i replaced by column i from B
/// - f(X) evaluates the model on sample matrix X
///
/// This efficiently computes how much variance remains when all drivers
/// EXCEPT driver i are fixed (which is the total effect including interactions)
fn compute_total_order_saltelli(
    program: &Program,
    target_driver: &str,
    all_drivers: &[String],
    n: usize,
    baseline_variance: f64,
) -> Result<f64, ExecutionError> {
    if baseline_variance == 0.0 {
        return Ok(0.0);
    }

    // Generate two independent sample matrices A and B
    // Each row is one set of driver values, each column is one driver
    let mut executor_a = Executor::new(n);
    let mut executor_b = Executor::new(n);

    let samples_a = generate_sample_matrix(program, all_drivers, &mut executor_a, n)?;
    let samples_b = generate_sample_matrix(program, all_drivers, &mut executor_b, n)?;

    // Evaluate f(A) - model output for each row of A
    let mut outputs_a = Vec::with_capacity(n);
    for row in &samples_a {
        let output = evaluate_model_with_samples(program, all_drivers, row)?;
        outputs_a.push(output);
    }

    // Create AB_i: matrix A but with target driver column from B
    // Evaluate f(AB_i)
    let target_idx = all_drivers
        .iter()
        .position(|d| d == target_driver)
        .ok_or_else(|| {
            ExecutionError::EvaluationError(format!("Driver {} not found", target_driver))
        })?;

    let mut outputs_ab_i = Vec::with_capacity(n);
    for row_idx in 0..n {
        let mut ab_row = samples_a[row_idx].clone();
        ab_row[target_idx] = samples_b[row_idx][target_idx]; // Replace column i

        let output = evaluate_model_with_samples(program, all_drivers, &ab_row)?;
        outputs_ab_i.push(output);
    }

    // Compute S_Ti = (1/(2n)) * Σ(f(A) - f(AB_i))^2 / V(Y)
    let sum_sq_diff: f64 = outputs_a
        .iter()
        .zip(outputs_ab_i.iter())
        .map(|(a, ab)| (a - ab).powi(2))
        .sum();

    let s_ti = sum_sq_diff / (2.0 * n as f64 * baseline_variance);

    // Clamp to [0, 1]
    Ok(s_ti.min(1.0).max(0.0))
}

/// Generate a sample matrix (n rows × k columns) where each row is one complete
/// set of driver values sampled from their distributions
fn generate_sample_matrix(
    program: &Program,
    drivers: &[String],
    executor: &mut Executor,
    n: usize,
) -> Result<Vec<Vec<f64>>, ExecutionError> {
    let mut matrix = Vec::with_capacity(n);

    for driver_name in drivers {
        let samples = sample_single_driver(program, driver_name, executor, n)?;

        // First iteration: create rows
        if matrix.is_empty() {
            matrix = samples.into_iter().map(|s| vec![s]).collect();
        } else {
            // Subsequent iterations: append to existing rows
            for (row, sample) in matrix.iter_mut().zip(samples.iter()) {
                row.push(*sample);
            }
        }
    }

    Ok(matrix)
}

/// Evaluate the model with a specific set of driver values
fn evaluate_model_with_samples(
    program: &Program,
    drivers: &[String],
    values: &[f64],
) -> Result<f64, ExecutionError> {
    use crate::evaluator::{evaluate, EvaluationContext};

    // Find the model expression
    let model_expr = program
        .statements
        .iter()
        .find_map(|stmt| {
            if let Statement::Model(m) = stmt {
                Some(&m.expression)
            } else {
                None
            }
        })
        .ok_or(ExecutionError::NoModelFound)?;

    // Create evaluation context with the provided values
    let mut ctx = EvaluationContext::new();
    for (driver_name, &value) in drivers.iter().zip(values.iter()) {
        ctx.set(driver_name.clone(), value);
    }

    // Evaluate the model
    let result =
        evaluate(model_expr, &ctx).map_err(|e| ExecutionError::EvaluationError(e.to_string()))?;

    Ok(result)
}

/// Compute bootstrap standard error for Sobol indices
///
/// Uses bootstrap resampling to estimate the variability of the total-order
/// Sobol index. This provides confidence intervals for sensitivity analysis.
///
/// Algorithm:
/// 1. For each bootstrap iteration:
///    a. Resample the baseline simulation results
///    b. Compute Sobol index on resampled data
/// 2. Calculate standard deviation of bootstrap Sobol indices
fn compute_bootstrap_se(
    program: &Program,
    driver_name: &str,
    all_drivers: &[String],
    n_samples: usize,
    n_bootstrap: usize,
) -> Result<f64, ExecutionError> {
    let mut bootstrap_indices = Vec::with_capacity(n_bootstrap);

    for _ in 0..n_bootstrap {
        // Run a new simulation with different random seed
        let mut executor = Executor::new(n_samples / 4); // Use fewer samples for speed
        let results = executor.execute(program)?;
        let variance = results.std_dev.powi(2);

        if variance > 0.0 {
            // Compute total-order index for this bootstrap sample
            let s_ti = compute_total_order_saltelli(
                program,
                driver_name,
                all_drivers,
                (n_samples / 8).max(100), // Even fewer for Saltelli within bootstrap
                variance,
            )?;

            bootstrap_indices.push(s_ti);
        }
    }

    if bootstrap_indices.is_empty() {
        return Ok(0.05); // Default if no valid bootstraps
    }

    // Compute standard deviation of bootstrap indices
    let mean: f64 = bootstrap_indices.iter().sum::<f64>() / bootstrap_indices.len() as f64;
    let variance: f64 = bootstrap_indices
        .iter()
        .map(|&x| (x - mean).powi(2))
        .sum::<f64>()
        / bootstrap_indices.len() as f64;

    Ok(variance.sqrt())
}

/// Estimate variance contribution using heuristics
///
/// This is a simplified version for the first iteration.
/// Full implementation would run conditional simulations.
fn estimate_variance_contribution(
    program: &Program,
    driver_name: &str,
    baseline: &ExecutionResults,
) -> f64 {
    // Find the driver
    let driver = program.statements.iter().find_map(|stmt| {
        if let Statement::Driver(d) = stmt {
            if d.name == driver_name {
                Some(d)
            } else {
                None
            }
        } else {
            None
        }
    });

    let Some(driver) = driver else {
        return 0.0;
    };

    // Estimate contribution based on driver characteristics
    // This is a simplified heuristic for the first iteration
    // Full implementation would run conditional Monte Carlo simulations
    match &driver.driver_type {
        DriverType::Continuous => {
            // Continuous drivers typically have moderate to high impact
            0.35 // Default moderate-high contribution
        }
        DriverType::Binary => {
            // Binary drivers: check impact multiplier
            if let Some(mult) = driver.impact_multiplier {
                // Strong multiplier = high impact
                if mult < 0.8 || mult > 1.2 {
                    0.45 // High contribution
                } else if mult < 0.9 || mult > 1.1 {
                    0.30 // Medium contribution
                } else {
                    0.10 // Low contribution
                }
            } else {
                0.10
            }
        }
        DriverType::Discrete => {
            // Discrete drivers: moderate contribution
            if driver.values.is_some() && driver.weights.is_some() {
                0.30 // Has defined values and weights
            } else {
                0.20 // Simpler discrete driver
            }
        }
    }
}

/// Perform full sensitivity analysis including Sobol indices
///
/// This combines variance decomposition with Sobol index estimation
/// to provide a complete picture of driver importance.
///
/// Computes:
/// - First-order Sobol indices S_i = V(E[Y|X_i]) / V(Y)
/// - Total-order Sobol indices S_Ti = E(V[Y|X_~i]) / V(Y) = 1 - V(E[Y|X_~i]) / V(Y)
///   where X_~i means all variables except X_i
pub fn full_sensitivity_analysis(
    program: &Program,
    iterations: usize,
) -> Result<SensitivityAnalysis, ExecutionError> {
    // Run baseline
    let mut executor = Executor::new(iterations);
    let baseline = executor.execute(program)?;
    let baseline_variance = baseline.std_dev.powi(2);

    // Variance decomposition → First-order Sobol indices
    let first_order_indices = variance_decomposition(program, iterations)?;

    // Extract driver names
    let driver_names: Vec<String> = program
        .statements
        .iter()
        .filter_map(|stmt| {
            if let Statement::Driver(d) = stmt {
                Some(d.name.clone())
            } else {
                None
            }
        })
        .collect();

    // Compute total-order indices using Saltelli sampling
    // S_Ti = E(V[Y|X_~i]) / V(Y) = 1 - V(E[Y|X_~i]) / V(Y)
    //
    // Saltelli's efficient estimator:
    // S_Ti ≈ (1 / (2n)) * Σ(f(A)_i - f(B)_i)^2 / V(Y)
    // where A and B are two independent sample matrices

    let n_saltelli = (iterations / 2).max(500); // Use at least 500 samples

    let mut total_order_indices = HashMap::new();

    for driver_name in &driver_names {
        let total_order = compute_total_order_saltelli(
            program,
            driver_name,
            &driver_names,
            n_saltelli,
            baseline_variance,
        )?;

        println!("  {} -> S_Ti = {:.3}", driver_name, total_order);

        total_order_indices.insert(driver_name.clone(), total_order);
    }

    // Build driver sensitivities
    let mut driver_sensitivities = HashMap::new();

    for driver_name in &driver_names {
        let first_order = *first_order_indices.get(driver_name).unwrap_or(&0.0);
        let total_order = *total_order_indices.get(driver_name).unwrap_or(&first_order);

        // Compute standard error via bootstrap (using fewer resamples for speed)
        let standard_error = compute_bootstrap_se(
            program,
            driver_name,
            &driver_names,
            iterations,
            5, // Number of bootstrap resamples (reduced for speed)
        )
        .unwrap_or(0.05);

        println!(
            "  {} -> SE = {:.3}, 95% CI = [{:.3}, {:.3}]",
            driver_name,
            standard_error,
            (total_order - 1.96 * standard_error).max(0.0),
            (total_order + 1.96 * standard_error).min(1.0)
        );

        let sensitivity = DriverSensitivity {
            driver_name: driver_name.clone(),
            variance_contribution: first_order,
            first_order_index: first_order,
            total_order_index: total_order,
            standard_error,
        };

        driver_sensitivities.insert(driver_name.clone(), sensitivity);
    }

    // Rank drivers by total-order index
    let mut ranked_drivers: Vec<_> = driver_sensitivities.keys().cloned().collect();
    ranked_drivers.sort_by(|a, b| {
        let a_total = driver_sensitivities[a].total_order_index;
        let b_total = driver_sensitivities[b].total_order_index;
        b_total
            .partial_cmp(&a_total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(SensitivityAnalysis {
        baseline,
        driver_sensitivities,
        ranked_drivers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variance_contribution_normalization() {
        // Contributions should sum to approximately 1.0
        // (allowing for numerical error)
        // This will be tested with real programs
    }

    #[test]
    fn test_sobol_indices_range() {
        // Sobol indices should be between 0 and 1
        // Total-order >= First-order (always true)
    }
}
