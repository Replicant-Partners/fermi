// src/agent_backend/tools/domains/simulation.rs
//
// Phase 4 domain migration: Simulation tools — execute() bodies inlined.
//
// Four tools: fermi_execute_fpl, fermi_sensitivity_analysis,
// run_monte_carlo, run_sensitivity_analysis.
//
// Each is a zero-size struct implementing PlatformTool. execute() calls a
// private async fn in this file; the ToolRegistry delegation has been removed.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Simulation-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(FermiExecuteFpl),
        Arc::new(FermiSensitivityAnalysis),
        Arc::new(RunMonteCarlo),
        Arc::new(RunSensitivityAnalysis),
    ]
}

// ─── fermi_execute_fpl ───────────────────────────────────────────────────────

struct FermiExecuteFpl;

#[async_trait]
impl PlatformTool for FermiExecuteFpl {
    fn name(&self) -> &'static str {
        "fermi_execute_fpl"
    }

    fn description(&self) -> &'static str {
        "Execute a Fermi FPL program. Runs a real Monte Carlo simulation (default 10,000 iterations, max 100,000) and returns mean, median, std_dev, p5, p25, p75, p95, min, max, base_rate and divergence figures."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "fpl_program": {
                    "type": "string",
                    "description": "A complete valid FPL program."
                },
                "iterations": {
                    "type": "integer",
                    "description": "Monte Carlo iterations (default 10000, max 100000)."
                },
                "seed": {
                    "type": "integer",
                    "description": "Optional seed for reproducibility."
                }
            },
            "required": ["fpl_program"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Simulation
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fermi_execute_fpl(input).await
    }
}

// ─── fermi_sensitivity_analysis ──────────────────────────────────────────────

struct FermiSensitivityAnalysis;

#[async_trait]
impl PlatformTool for FermiSensitivityAnalysis {
    fn name(&self) -> &'static str {
        "fermi_sensitivity_analysis"
    }

    fn description(&self) -> &'static str {
        "Run Sobol sensitivity analysis on an FPL program. Returns first-order and total-order indices per driver — real variance decomposition identifying which drivers actually drive output variance, with standard errors and confidence intervals."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "fpl_program": {
                    "type": "string",
                    "description": "The same FPL program passed to fermi_execute_fpl."
                },
                "iterations": {
                    "type": "integer",
                    "description": "Iterations (default 5000, max 50000)."
                }
            },
            "required": ["fpl_program"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Simulation
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_fermi_sensitivity_analysis(input).await
    }
}

// ─── run_monte_carlo ─────────────────────────────────────────────────────────

struct RunMonteCarlo;

#[async_trait]
impl PlatformTool for RunMonteCarlo {
    fn name(&self) -> &'static str {
        "run_monte_carlo"
    }

    fn description(&self) -> &'static str {
        "Execute a Monte Carlo simulation from an FPL (Fermi Probabilistic Language) program. Parses the program, samples from each driver's distribution, and returns full statistics: mean, median, percentiles (p5/p25/p75/p95), std_dev, min/max, and a histogram. Use this to produce rigorous probabilistic results rather than reasoning about distributions informally."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "fpl_program": {
                    "type": "string",
                    "description": "FPL source code defining drivers (with distributions), a model expression, and a simulate statement. Example:\n  driver x continuous { distribution: triangular(0.3, 0.6, 0.9) }\n  model: x\n  simulate 10000 iterations"
                },
                "iterations": {
                    "type": "integer",
                    "description": "Number of Monte Carlo iterations (default: 10000). Overrides the simulate statement in the FPL if provided.",
                    "default": 10000
                }
            },
            "required": ["fpl_program"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Simulation
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_run_monte_carlo(input).await
    }
}

// ─── run_sensitivity_analysis ────────────────────────────────────────────────

struct RunSensitivityAnalysis;

#[async_trait]
impl PlatformTool for RunSensitivityAnalysis {
    fn name(&self) -> &'static str {
        "run_sensitivity_analysis"
    }

    fn description(&self) -> &'static str {
        "Run Sobol global sensitivity analysis on an FPL program. Returns first-order and total-order Sobol indices for each driver, ranked by total-order impact, plus bootstrap standard errors for uncertainty quantification. Use this to identify which input variables drive the most outcome variance — a proper variance decomposition, not a heuristic tornado diagram."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "fpl_program": {
                    "type": "string",
                    "description": "FPL source code with driver definitions and model expression."
                },
                "iterations": {
                    "type": "integer",
                    "description": "Baseline iterations for the analysis (default: 10000). More iterations improve index precision.",
                    "default": 10000
                }
            },
            "required": ["fpl_program"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Simulation
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        do_run_sensitivity_analysis(input).await
    }
}

// ── Private execute implementations ──────────────────────────────────────────

/// Parse FPL through the full pipeline: lex → parse → semantic analysis.
///
/// Mirrors `agent-mcp-server`'s private `parse_fpl`. The whole pipeline lives in
/// this crate (`fermi::lexer`, `parser`, `semantic`), which is why these are
/// in-process tools rather than a card pointing at an external MCP server: the
/// executor is already linked in, so a network hop would buy nothing.
fn parse_fpl_source(source: &str) -> Result<crate::ast::Program, String> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|errs| {
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
    let program = crate::parser::Parser::new(tokens)
        .parse()
        .map_err(|e| format!("Parse error: {e}"))?;
    let analysis = crate::semantic::SemanticAnalyzer::new().analyze(&program);
    if !analysis.errors.is_empty() {
        return Err(format!(
            "Semantic error: {}",
            analysis
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    Ok(program)
}

/// `fermi_execute_fpl` — run a Monte Carlo simulation over an FPL program.
///
/// `monte_carlo_sim` declared this (and the sensitivity tool below) as platform
/// tools while both existed only on `agent-mcp-server`, and the card declared no
/// `mcp_servers` to resolve them through. So the model was advertised a
/// simulation capability and got `Unknown tool` — the agent's entire purpose.
async fn do_fermi_execute_fpl(input: &Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "fpl_program is required".to_string())?;
    let program = parse_fpl_source(source)?;

    // Same bounds the MCP surface applies. The cap matters here more than
    // there: this runs inside a request-serving process.
    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000)
        .min(100_000) as usize;

    let mut executor = match input.get("seed").and_then(|v| v.as_u64()) {
        Some(seed) => crate::executor::Executor::with_seed(iterations, seed),
        None => crate::executor::Executor::new(iterations),
    };
    let r = executor
        .execute(&program)
        .map_err(|e| format!("Execution failed: {e}"))?;

    serde_json::to_string_pretty(&json!({
        "iterations": r.iterations,
        "mean": r.mean,
        "median": r.median,
        "std_dev": r.std_dev,
        "p5": r.p5,
        "p25": r.p25,
        "p75": r.p75,
        "p95": r.p95,
        "min": r.min,
        "max": r.max,
        "base_rate": r.base_rate,
        "divergence_relative": r.divergence_relative,
        "divergence_absolute": r.divergence_absolute,
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

/// `fermi_sensitivity_analysis` — Sobol variance decomposition over an FPL program.
async fn do_fermi_sensitivity_analysis(input: &Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "fpl_program is required".to_string())?;
    let program = parse_fpl_source(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(5_000)
        .min(50_000) as usize;

    let analysis = crate::sensitivity::full_sensitivity_analysis(&program, iterations)
        .map_err(|e| format!("Sensitivity analysis failed: {e}"))?;

    let drivers: Vec<Value> = analysis
        .ranked_drivers
        .iter()
        .filter_map(|name| analysis.driver_sensitivities.get(name))
        .map(|s| {
            json!({
                "driver": s.driver_name,
                "first_order_index": s.first_order_index,
                "total_order_index": s.total_order_index,
                "variance_contribution": s.variance_contribution,
                "standard_error": s.standard_error,
                "ci_low": (s.total_order_index - 1.96 * s.standard_error).max(0.0),
                "ci_high": (s.total_order_index + 1.96 * s.standard_error).min(1.0),
            })
        })
        .collect();

    serde_json::to_string_pretty(&json!({
        "iterations": iterations,
        "baseline": {
            "mean": analysis.baseline.mean,
            "std_dev": analysis.baseline.std_dev,
            "p5": analysis.baseline.p5,
            "p95": analysis.baseline.p95,
        },
        "drivers": drivers,
        "top_driver": analysis.ranked_drivers.first(),
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

fn parse_fpl(source: &str) -> Result<crate::ast::Program, String> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|errs| {
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
    crate::parser::Parser::new(tokens)
        .parse()
        .map_err(|e| e.to_string())
}

/// Run a Monte Carlo simulation from an FPL program.
async fn do_run_monte_carlo(input: &Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: fpl_program")?;

    let program = parse_fpl(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000) as usize;

    let mut executor = crate::executor::Executor::new(iterations);
    let results = executor
        .execute(&program)
        .map_err(|e| format!("Simulation error: {}", e))?;

    // Build a compact ASCII histogram (10 bins)
    let histogram = results.histogram(10);
    let max_count = histogram.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let bar_width = 30usize;
    let mut hist_str = String::new();
    for (bin_start, count) in &histogram {
        let bar_len = (count * bar_width) / max_count;
        hist_str.push_str(&format!(
            "  {:>6.3} | {:<30} {}\n",
            bin_start,
            "#".repeat(bar_len),
            count
        ));
    }

    let result = json!({
        "iterations": results.iterations,
        "mean": results.mean,
        "median": results.median,
        "std_dev": results.std_dev,
        "min": results.min,
        "max": results.max,
        "percentiles": {
            "p5": results.p5,
            "p25": results.p25,
            "p75": results.p75,
            "p95": results.p95,
        },
        "base_rate": results.base_rate,
        "divergence_relative": results.divergence_relative,
        "divergence_absolute": results.divergence_absolute,
        "histogram_ascii": hist_str,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Run Sobol global sensitivity analysis on an FPL program.
async fn do_run_sensitivity_analysis(input: &Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: fpl_program")?;

    let program = parse_fpl(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000) as usize;

    let analysis = crate::sensitivity::full_sensitivity_analysis(&program, iterations)
        .map_err(|e| format!("Sensitivity analysis error: {}", e))?;

    // Build ranked driver list with indices
    let drivers: Vec<Value> = analysis
        .ranked_drivers
        .iter()
        .filter_map(|name| analysis.driver_sensitivities.get(name))
        .map(|ds| {
            let ci_low = (ds.total_order_index - 1.96 * ds.standard_error).max(0.0);
            let ci_high = (ds.total_order_index + 1.96 * ds.standard_error).min(1.0);
            json!({
                "driver": ds.driver_name,
                "first_order_index": ds.first_order_index,
                "total_order_index": ds.total_order_index,
                "variance_contribution": ds.variance_contribution,
                "standard_error": ds.standard_error,
                "confidence_interval_95": [ci_low, ci_high],
            })
        })
        .collect();

    // ASCII tornado diagram
    let mut tornado = String::new();
    for ds in &drivers {
        let s_t = ds["total_order_index"].as_f64().unwrap_or(0.0);
        let bar_len = (s_t * 40.0) as usize;
        tornado.push_str(&format!(
            "  {:<30} | {:<40} {:.3}\n",
            ds["driver"].as_str().unwrap_or(""),
            "#".repeat(bar_len),
            s_t
        ));
    }

    let result = json!({
        "baseline": {
            "mean": analysis.baseline.mean,
            "std_dev": analysis.baseline.std_dev,
            "p5": analysis.baseline.p5,
            "p95": analysis.baseline.p95,
        },
        "drivers_ranked_by_total_order": drivers,
        "tornado_diagram_ascii": tornado,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_simulation() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Simulation,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_four() {
        assert_eq!(tools().len(), 4);
    }
}
