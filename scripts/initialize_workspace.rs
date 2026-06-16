// initialize-workspace — run a factor-model FPL template against a params.json
// and emit structured workspace outputs as JSON.
//
// Usage:
//   initialize-workspace --template templates/world_cup/team_prior.fpl \
//                        --params .app/params.json \
//                        [--iterations 10000] [--seed 42] \
//                        [--output workspace_outputs.json]
//
// Output JSON shape (consumable by workspace_outputs PUT endpoint):
// {
//   "estimate_name": "tournament_strength",
//   "tournament_strength": { "mean": ..., "median": ..., "p5": ..., ... },
//   "factor_means": { "X1": ..., "X2": ..., ... },
//   "factor_std_devs": { ... },
//   "factor_orthogonality": { "max_abs_corr": ... },
//   "params": { ... },
//   "n_iterations": 10000
// }
//
// The wrapper script (publish_team_priors.py) consumes this JSON and PUTs each
// top-level key to /api/workspaces/:id/outputs/:key.

use clap::Parser as ClapParser;
use fermi::{Executor, Lexer, Parser as FplParser, SemanticAnalyzer};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::process::ExitCode;

#[derive(ClapParser, Debug)]
#[command(name = "initialize-workspace", about = "Run a factor-model FPL template against params.json and emit workspace outputs.")]
struct Args {
    /// Path to FPL template (e.g. templates/world_cup/team_prior.fpl)
    #[arg(short = 't', long)]
    template: String,

    /// Path to params.json (the workspace's .app/params.json)
    #[arg(short = 'p', long)]
    params: String,

    /// Monte Carlo iterations (overrides the `simulate` statement in the template)
    #[arg(short = 'i', long)]
    iterations: Option<usize>,

    /// RNG seed for reproducibility
    #[arg(short = 's', long)]
    seed: Option<u64>,

    /// Output JSON path. If absent, prints to stdout.
    #[arg(short = 'o', long)]
    output: Option<String>,

    /// Quiet mode — only emit JSON, no progress messages
    #[arg(short = 'q', long, default_value_t = false)]
    quiet: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let log = |msg: &str| {
        if !args.quiet {
            eprintln!("{}", msg);
        }
    };

    // --- Read template ----------------------------------------------------
    let source = match fs::read_to_string(&args.template) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERROR: cannot read template '{}': {}", args.template, e);
            return ExitCode::from(2);
        }
    };

    // --- Read params.json -------------------------------------------------
    let params_json: Value = match fs::read_to_string(&args.params) {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("ERROR: params.json is not valid JSON: {}", e);
                return ExitCode::from(2);
            }
        },
        Err(e) => {
            eprintln!("ERROR: cannot read params '{}': {}", args.params, e);
            return ExitCode::from(2);
        }
    };

    // Split params.json into three buckets:
    //   numeric_params  → goes into executor.params (f64 ctx; ParamRef reads)
    //   json_params     → goes into executor.json_params (objects/arrays;
    //                     BayesOps fitted-distribution overrides live here
    //                     under keys ending in `_fitted`)
    //   metadata_params → string fields kept for output traceability only
    let mut numeric_params: HashMap<String, f64> = HashMap::new();
    let mut json_params: HashMap<String, Value> = HashMap::new();
    let mut metadata_params: HashMap<String, Value> = HashMap::new();
    if let Some(obj) = params_json.as_object() {
        for (k, v) in obj {
            match v {
                Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        numeric_params.insert(k.clone(), f);
                    }
                }
                Value::Bool(b) => {
                    numeric_params.insert(k.clone(), if *b { 1.0 } else { 0.0 });
                }
                Value::Object(_) | Value::Array(_) => {
                    // Structured params — could be a BayesOps fitted
                    // distribution (e.g. `foo_fitted`) or any other
                    // domain-specific config. The executor's learnable-driver
                    // lookup checks json_params for `<name>_fitted` keys.
                    json_params.insert(k.clone(), v.clone());
                }
                _ => {
                    metadata_params.insert(k.clone(), v.clone());
                }
            }
        }
    }

    log(&format!(
        "Template: {} ({} bytes)",
        args.template,
        source.len()
    ));
    log(&format!(
        "Params: {} numeric, {} json, {} metadata",
        numeric_params.len(),
        json_params.len(),
        metadata_params.len()
    ));

    // --- Lex / Parse / Semantic -----------------------------------------
    let lexer = Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(errs) => {
            eprintln!("ERROR: lex failed:");
            for e in errs {
                eprintln!("  - {:?}", e);
            }
            return ExitCode::from(3);
        }
    };

    let parser = FplParser::new(tokens);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ERROR: parse failed: {:?}", e);
            return ExitCode::from(3);
        }
    };

    let analyzer = SemanticAnalyzer::new();
    let analysis = analyzer.analyze(&program);
    if !analysis.errors.is_empty() {
        eprintln!("ERROR: semantic analysis failed:");
        for err in &analysis.errors {
            eprintln!("  - {:?}", err);
        }
        return ExitCode::from(4);
    }

    // --- Execute factor model -------------------------------------------
    // Determine iteration count: --iterations flag, else the program's
    // `simulate` statement, else 10_000.
    let iterations = args
        .iterations
        .or_else(|| program.simulate().map(|s| s.iterations as usize))
        .unwrap_or(10_000);

    let mut executor = match args.seed {
        Some(s) => Executor::with_seed(iterations, s),
        None => Executor::new(iterations),
    };
    executor.set_params(numeric_params.clone());
    executor.set_json_params(json_params.clone());

    log(&format!(
        "Running factor-model simulation: {} iterations{}",
        iterations,
        args.seed.map(|s| format!(" (seed={})", s)).unwrap_or_default()
    ));

    let results = match executor.execute(&program) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERROR: execution failed: {}", e);
            return ExitCode::from(5);
        }
    };

    // --- Build outputs JSON ---------------------------------------------
    let estimate_name = results.estimate_name.clone().unwrap_or_else(|| "response".into());

    let mut outputs = serde_json::Map::new();

    // Top-level: estimate distribution.
    outputs.insert(
        estimate_name.clone(),
        json!({
            "mean": results.mean,
            "median": results.median,
            "std_dev": results.std_dev,
            "p5": results.p5,
            "p25": results.p25,
            "p75": results.p75,
            "p95": results.p95,
            "min": results.min,
            "max": results.max,
            "n_iterations": results.iterations,
        }),
    );

    if let Some(fm) = &results.factor_means {
        let mut obj = serde_json::Map::new();
        for (k, v) in fm {
            obj.insert(k.clone(), json!(v));
        }
        outputs.insert("factor_means".into(), Value::Object(obj));
    }

    if let Some(fsd) = &results.factor_std_devs {
        let mut obj = serde_json::Map::new();
        for (k, v) in fsd {
            obj.insert(k.clone(), json!(v));
        }
        outputs.insert("factor_std_devs".into(), Value::Object(obj));
    }

    if let Some(corr) = results.factor_corr_max {
        outputs.insert(
            "factor_orthogonality".into(),
            json!({
                "max_abs_corr": corr,
                // Threshold per Phase 4 spec: 1e-6 for full residualization,
                // 0.05 acceptable pre-Phase-4. Report which regime we're in.
                "regime": if corr < 1e-6 { "orthogonal" }
                          else if corr < 0.05 { "near-orthogonal" }
                          else { "correlated" },
            }),
        );
    }

    // Learnable manifest — the contract surface for BayesOps. Lists every
    // `learnable(...)` literal in the program with its auto-assigned name,
    // prior (initial, sigma), and owner (factor/estimate it lives in).
    //
    // BayesOps reads this, fits posteriors against match outcomes, then
    // writes updated point estimates back to the workspace's .app/params.json
    // under those same names. The next sim run will pick them up via
    // Expression::LearnablePrior fallback logic in the evaluator.
    if let Some(manifest) = &results.learnable_manifest {
        let entries: Vec<Value> = manifest.iter().map(|li| {
            json!({
                "name": li.name,
                "initial": li.initial,
                "sigma": li.sigma,
                "owner": li.owner,
                "current_value": numeric_params.get(&li.name).copied().unwrap_or(li.initial),
                "is_overridden": numeric_params.contains_key(&li.name),
            })
        }).collect();
        outputs.insert("learnable_manifest".into(), Value::Array(entries));
    }

    // Learnable drivers — the second half of the BayesOps contract surface.
    // Unlike `learnable_manifest` (scalar `learnable(...)` literals inside
    // estimates/factor formulations), this lists `learnable: true` drivers
    // and reports how each was resolved THIS RUN: from a fitted distribution
    // (with full FittedDistribution details), or from the static prior
    // because no fit was available. UI consumes this for status badges.
    if !results.learnable_drivers.is_empty() {
        let entries: Vec<Value> = results.learnable_drivers.iter().map(|r| {
            match &r.source {
                fermi::executor::LearnableSource::Fitted { fitted } => {
                    let key = format!("{}_fitted", r.name);
                    json!({
                        "name": r.name,
                        "status": "fitted",
                        "fitted": serde_json::to_value(fitted).unwrap_or(Value::Null),
                        "fpl_params": fitted.to_fpl_params(),
                        "ci_width": fitted.ci_width(),
                        "n_eff": fitted.n_eff(),
                        "params_key": key,
                    })
                }
                fermi::executor::LearnableSource::PriorFallback => json!({
                    "name": r.name,
                    "status": "prior_fallback",
                    "reason": format!("no params.{}_fitted in scope", r.name),
                }),
                fermi::executor::LearnableSource::Static => json!({
                    "name": r.name,
                    "status": "static",
                }),
            }
        }).collect();
        outputs.insert("learnable_drivers".into(), Value::Array(entries));
    }

    // Echo params for traceability (numeric + metadata).
    let mut params_out = serde_json::Map::new();
    for (k, v) in &numeric_params {
        params_out.insert(k.clone(), json!(v));
    }
    for (k, v) in &metadata_params {
        params_out.insert(k.clone(), v.clone());
    }
    outputs.insert("params".into(), Value::Object(params_out));

    outputs.insert("estimate_name".into(), Value::String(estimate_name));
    outputs.insert("n_iterations".into(), json!(results.iterations));

    let final_json = Value::Object(outputs);
    let pretty = serde_json::to_string_pretty(&final_json).unwrap();

    // --- Emit -----------------------------------------------------------
    match &args.output {
        Some(path) => {
            if let Err(e) = fs::write(path, &pretty) {
                eprintln!("ERROR: cannot write output to '{}': {}", path, e);
                return ExitCode::from(6);
            }
            log(&format!("Outputs written to {}", path));
        }
        None => {
            println!("{}", pretty);
        }
    }

    log(&format!(
        "OK — mean({})={:.4}, factor_corr_max={:.4}",
        results.estimate_name.as_deref().unwrap_or("response"),
        results.mean,
        results.factor_corr_max.unwrap_or(0.0),
    ));

    ExitCode::SUCCESS
}
