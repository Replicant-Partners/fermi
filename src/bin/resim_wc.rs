//! Batch re-simulation of all 48 WC-2026 team-prior forecasts.
//!
//! Why this exists: roughly two-thirds of the field was frozen at the
//! 0.02 seed default — those workspaces were spawned with real per-team
//! param triples but their Monte-Carlo sim was never actually run, so
//! genuinely-strong teams (Croatia, Uruguay, Morocco, …) sat at the same
//! floor as the weakest, and the one team that DID get simulated
//! (Australia, 2.67%) looked anomalously high by comparison.
//!
//! This runs the SAME `fermi::executor` the cockpit's run_simulation uses,
//! binding each forecast's stored workspace params (key='params') against
//! the canonical team_prior template, and writes the resulting mean back
//! to fermi_forecasts.predicted_probability + simulation_results (and the
//! mirrored workspace_outputs rows). Faithful to the Option-2 contract:
//! the model expression IS the forecast; we take the mean, clamp [.01,.99].
//!
//! Run:  DATABASE_URL=… cargo run --bin resim_wc -- [--dry-run]
//!
//! Idempotent — re-running just recomputes from current params. It does
//! NOT touch already-resolved forecasts (actual_outcome IS NOT NULL):
//! those are factual now and belong to the cascade path, not the sim.

use std::collections::HashMap;
use std::str::FromStr;

use fermi::executor::Executor;
use fermi::lexer::Lexer;
use fermi::parser::Parser;
use serde_json::{json, Value};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};

const TEMPLATE_PATH: &str = "templates/world_cup/team_prior.fpl";
const ITERATIONS: usize = 10_000;

#[tokio::main]
async fn main() {
    let dry_run = std::env::args().any(|a| a == "--dry-run");
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // PgBouncer transaction mode: disable the prepared-statement cache.
    let connect_options = PgConnectOptions::from_str(&database_url)
        .expect("parse DATABASE_URL")
        .statement_cache_capacity(0);
    let db: PgPool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(connect_options)
        .await
        .expect("connect to db");

    let template = std::fs::read_to_string(TEMPLATE_PATH)
        .unwrap_or_else(|e| panic!("read {TEMPLATE_PATH}: {e}"));
    // Parse once — the program is identical across teams; only the bound
    // params differ. (We re-parse per team anyway to keep each Executor
    // run independent and side-effect free.)
    let _ = Parser::new(Lexer::new(&template).tokenize().expect("tokenize template"))
        .parse()
        .expect("parse template");

    // Pull every WC team forecast that is NOT yet resolved.
    let rows = sqlx::query(
        "SELECT f.id, f.question_text, f.workspace_id
           FROM public.fermi_forecasts f
          WHERE f.question_text ILIKE '%2026 FIFA World Cup%'
            AND f.actual_outcome IS NULL
          ORDER BY f.question_text",
    )
    .fetch_all(&db)
    .await
    .expect("fetch WC forecasts");

    println!(
        "Re-simulating {} unresolved WC forecasts{}\n",
        rows.len(),
        if dry_run {
            " (DRY RUN — no writes)"
        } else {
            ""
        }
    );

    let mut results: Vec<(String, f64, f64)> = Vec::new(); // (question, old, new)

    for row in &rows {
        let fid: String = row.get("id");
        let question: String = row.get("question_text");
        let workspace_id: Option<uuid::Uuid> = row.try_get("workspace_id").ok();

        let Some(ws) = workspace_id else {
            println!("  SKIP  {question} — no workspace_id");
            continue;
        };

        // Fetch the params object (key='params') for this workspace.
        let params_row = sqlx::query(
            "SELECT value FROM public.workspace_outputs
              WHERE workspace_id = $1 AND key = 'params'",
        )
        .bind(ws)
        .fetch_optional(&db)
        .await
        .expect("fetch params");

        let Some(params_row) = params_row else {
            println!("  SKIP  {question} — no params row");
            continue;
        };
        let params: Value = params_row.get("value");

        // Bind exactly like cockpit run_simulation: numbers → set_params,
        // bools → 0/1, objects/arrays → set_json_params (BayesOps fits),
        // strings → skipped.
        let mut numeric: HashMap<String, f64> = HashMap::new();
        let mut json_params: HashMap<String, Value> = HashMap::new();
        if let Some(obj) = params.as_object() {
            for (k, v) in obj {
                match v {
                    Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            numeric.insert(k.clone(), f);
                        }
                    }
                    Value::Bool(b) => {
                        numeric.insert(k.clone(), if *b { 1.0 } else { 0.0 });
                    }
                    Value::Object(_) | Value::Array(_) => {
                        json_params.insert(k.clone(), v.clone());
                    }
                    _ => {}
                }
            }
        }

        let program = Parser::new(Lexer::new(&template).tokenize().expect("tokenize"))
            .parse()
            .expect("parse");
        let mut exec = Executor::new(ITERATIONS);
        exec.set_params(numeric);
        exec.set_json_params(json_params);

        let sim = match exec.execute(&program) {
            Ok(r) => r,
            Err(e) => {
                println!("  FAIL  {question} — executor error: {e}");
                continue;
            }
        };

        // Option-2 contract: take the mean, clamp to the display range.
        let new_p = sim.mean.clamp(0.01, 0.99);

        let old_p: f32 = sqlx::query_scalar(
            "SELECT predicted_probability FROM public.fermi_forecasts WHERE id = $1",
        )
        .bind(&fid)
        .fetch_one(&db)
        .await
        .unwrap_or(0.0);

        results.push((question.clone(), old_p as f64, new_p));

        let sim_json = json!({
            "mean": sim.mean,
            "median": sim.median,
            "p5": sim.p5,
            "p95": sim.p95,
            "std_dev": sim.std_dev,
        });

        if !dry_run {
            // fermi_forecasts is the source the catalogue/cockpit read.
            sqlx::query(
                "UPDATE public.fermi_forecasts
                    SET predicted_probability = $1,
                        simulation_results = $2,
                        updated_at = NOW()
                  WHERE id = $3",
            )
            .bind(new_p as f32)
            .bind(&sim_json)
            .bind(&fid)
            .execute(&db)
            .await
            .expect("update forecast");

            // Mirror into workspace_outputs so the cockpit's hydrated view
            // and the trajectory cards stay consistent.
            for (key, val) in [
                ("predicted_probability", json!(new_p)),
                ("simulation_results", sim_json.clone()),
            ] {
                sqlx::query(
                    "INSERT INTO public.workspace_outputs (workspace_id, key, value, updated_by)
                     VALUES ($1, $2, $3, 'resim_wc')
                     ON CONFLICT (workspace_id, key)
                     DO UPDATE SET value = EXCLUDED.value, updated_at = NOW(), updated_by = 'resim_wc'",
                )
                .bind(ws)
                .bind(key)
                .bind(&val)
                .execute(&db)
                .await
                .expect("upsert workspace_output");
            }
        }

        println!(
            "  {:>6.2}% → {:>6.2}%   {}",
            old_p * 100.0,
            new_p * 100.0,
            question
        );
    }

    // Summary: ranked + field sum.
    results.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    let sum: f64 = results.iter().map(|r| r.2).sum();
    println!("\n── Ranked (new) ──────────────────────────────");
    for (i, (q, _old, new_p)) in results.iter().enumerate() {
        let name = q
            .trim_start_matches("Will ")
            .trim_end_matches(" win the 2026 FIFA World Cup?");
        println!("  {:>2}. {:>6.2}%  {}", i + 1, new_p * 100.0, name);
    }
    println!(
        "\nField sum over {} unresolved teams: {:.3} (informational — mutex \
         normalization is the cascade path's job, not the per-team sim)",
        results.len(),
        sum
    );
}
