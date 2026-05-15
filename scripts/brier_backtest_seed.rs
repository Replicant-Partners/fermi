//! Brier Calibration Backtest Seed
//!
//! Seeds `fermi_forecasts` with historical resolved questions to bootstrap
//! the calibration signal for forecasting agents (Loop 5).
//!
//! Each agent with a `fermi_contract` needs calibration data before the
//! moe_router_strategist can route based on measured accuracy rather than
//! capability declarations alone. This script populates resolved forecasts
//! from a YAML fixture file of known-outcome questions.
//!
//! Usage:
//!   cargo run --bin brier-backtest-seed -- \
//!     --database-url "postgresql://..." \
//!     --questions scripts/brier_backtest_questions.yaml \
//!     --owner-id "your-user-id" \
//!     [--dry-run]
//!
//! The script is idempotent: it skips questions whose `id` already exists
//! in fermi_forecasts (checked via the `metadata.backtest_id` field).
//!
//! After running, verify with:
//!   SELECT a.agent_name, COUNT(*) as n_resolved, AVG(f.brier_score) as avg_brier
//!   FROM fermi_forecasts f
//!   CROSS JOIN LATERAL jsonb_array_elements(f.agents_used) au
//!   JOIN agents a ON a.agent_id::text = au->>'agent_id'
//!   WHERE f.status = 'resolved' AND f.metadata->>'backtest_id' IS NOT NULL
//!   GROUP BY a.agent_name ORDER BY avg_brier;

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Deserialize;
use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "brier-backtest-seed",
    about = "Seed fermi_forecasts with historical resolved questions for Brier calibration"
)]
struct Args {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    #[arg(long, default_value = "scripts/brier_backtest_questions.yaml")]
    questions: String,

    /// User ID to own the seeded forecasts. Use your own user_id or a system user.
    #[arg(long, env = "BACKTEST_OWNER_ID")]
    owner_id: String,

    /// Print what would be inserted without writing to DB.
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Force re-seed even if backtest_id already exists.
    #[arg(long, default_value_t = false)]
    force: bool,
}

// ── YAML fixture types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct QuestionsFile {
    questions: Vec<Question>,
}

#[derive(Debug, Deserialize)]
struct Question {
    id: String,
    question: String,
    domain: String,
    actual_outcome: bool,
    predicted_probability: f64,
    resolved_date: String,
    agents: Vec<String>,    // agent_name strings — resolved to UUIDs at runtime
    tags: Vec<String>,
}

// ── Main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();

    println!("Brier Backtest Seed");
    println!("  Questions file: {}", args.questions);
    println!("  Owner ID:       {}", args.owner_id);
    println!("  Dry run:        {}", args.dry_run);
    println!();

    // Load questions
    let yaml = std::fs::read_to_string(&args.questions)
        .with_context(|| format!("Failed to read {}", args.questions))?;
    let fixture: QuestionsFile = serde_yaml::from_str(&yaml)
        .with_context(|| "Failed to parse questions YAML")?;

    println!("Loaded {} questions", fixture.questions.len());

    // Connect
    let opts = PgConnectOptions::from_str(&args.database_url)
        .context("Invalid DATABASE_URL")?
        .statement_cache_capacity(0);
    let pool = PgPool::connect_with(opts).await.context("DB connection failed")?;

    // Pre-load agent name → UUID mapping
    let agent_rows = sqlx::query("SELECT agent_id, agent_name FROM agents")
        .fetch_all(&pool)
        .await
        .context("Failed to fetch agents")?;
    let agent_map: std::collections::HashMap<String, Uuid> = agent_rows
        .iter()
        .filter_map(|r| {
            let name: String = sqlx::Row::try_get(r, "agent_name").ok()?;
            let id: Uuid = sqlx::Row::try_get(r, "agent_id").ok()?;
            Some((name, id))
        })
        .collect();

    println!("Loaded {} agents from DB\n", agent_map.len());

    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    for q in &fixture.questions {
        // Check if already seeded
        if !args.force {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM fermi_forecasts WHERE metadata->>'backtest_id' = $1)",
            )
            .bind(&q.id)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);

            if exists {
                println!("  SKIP  {} (already seeded)", q.id);
                skipped += 1;
                continue;
            }
        }

        // Validate probability
        if q.predicted_probability < 0.0 || q.predicted_probability > 1.0 {
            eprintln!("  ERROR {} — predicted_probability out of range: {}", q.id, q.predicted_probability);
            errors += 1;
            continue;
        }

        // Resolve agent names → UUIDs
        let agents_used: Vec<serde_json::Value> = q.agents.iter()
            .filter_map(|name| {
                agent_map.get(name).map(|id| serde_json::json!({
                    "agent_id": id.to_string(),
                    "agent_name": name,
                    "source": "backtest_seed",
                }))
            })
            .collect();

        let unknown_agents: Vec<&str> = q.agents.iter()
            .filter(|name| !agent_map.contains_key(*name))
            .map(|s| s.as_str())
            .collect();

        if !unknown_agents.is_empty() {
            eprintln!("  WARN  {} — unknown agents (will skip them): {:?}", q.id, unknown_agents);
        }

        if agents_used.is_empty() {
            eprintln!("  ERROR {} — no valid agents resolved, skipping", q.id);
            errors += 1;
            continue;
        }

        // Parse resolved_date as target_date
        let target_date = chrono::NaiveDate::parse_from_str(&q.resolved_date, "%Y-%m-%d")
            .ok()
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            .map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc));

        // Compute Brier score directly: (predicted - actual)^2
        let actual_f: f64 = if q.actual_outcome { 1.0 } else { 0.0 };
        let brier = (q.predicted_probability - actual_f).powi(2);

        let forecast_id = Uuid::new_v4().to_string();
        let metadata = serde_json::json!({
            "backtest_id": q.id,
            "source": "brier_backtest_seed",
            "seeded_at": chrono::Utc::now().to_rfc3339(),
        });

        println!(
            "  {}  {} [{}] → {} agents, brier={:.4}, actual={}",
            if args.dry_run { "DRY" } else { "INS" },
            q.id,
            q.domain,
            agents_used.len(),
            brier,
            q.actual_outcome,
        );

        if args.dry_run {
            inserted += 1;
            continue;
        }

        // Insert as already-resolved (bypass active→resolved flow to avoid credit charge)
        let result = sqlx::query(
            "INSERT INTO fermi_forecasts
             (id, owner_id, question_text, domain, target_date,
              predicted_probability, agents_used, status,
              actual_outcome, brier_score,
              resolved_at, resolved_by, resolution_notes,
              visibility, tags, metadata, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5,
                     $6, $7, 'resolved',
                     $8, $9,
                     $10, $11, 'Historical backtest seed',
                     'private', $12, $13, $10, $10)
             ON CONFLICT DO NOTHING",
        )
        .bind(&forecast_id)
        .bind(&args.owner_id)
        .bind(&q.question)
        .bind(&q.domain)
        .bind(target_date)
        .bind(q.predicted_probability)
        .bind(serde_json::Value::Array(agents_used))
        .bind(q.actual_outcome)
        .bind(brier)
        .bind(target_date.unwrap_or_else(chrono::Utc::now))
        .bind(&args.owner_id)
        .bind(q.tags.clone())
        .bind(metadata)
        .execute(&pool)
        .await;

        match result {
            Ok(r) if r.rows_affected() > 0 => inserted += 1,
            Ok(_) => {
                println!("    (conflict — row already exists with this id)");
                skipped += 1;
            }
            Err(e) => {
                eprintln!("    ERROR inserting {}: {}", q.id, e);
                errors += 1;
            }
        }
    }

    println!();
    println!("═══════════════════════════════════════");
    if args.dry_run {
        println!("DRY RUN — no changes written");
        println!("  Would insert: {}", inserted);
    } else {
        println!("Inserted: {}", inserted);
        println!("Skipped:  {} (already seeded)", skipped);
        println!("Errors:   {}", errors);
    }
    println!("═══════════════════════════════════════");

    if !args.dry_run && inserted > 0 {
        println!();
        println!("Verify with:");
        println!("  SELECT a.agent_name, COUNT(*) as n_resolved, ROUND(AVG(f.brier_score)::numeric, 4) as avg_brier");
        println!("  FROM fermi_forecasts f");
        println!("  CROSS JOIN LATERAL jsonb_array_elements(f.agents_used) au");
        println!("  JOIN agents a ON a.agent_id::text = (au->>'agent_id')");
        println!("  WHERE f.status = 'resolved' AND f.metadata->>'backtest_id' IS NOT NULL");
        println!("  GROUP BY a.agent_name ORDER BY avg_brier;");
        println!();
        println!("Then check calibration via:");
        println!("  GET /api/agents/<agent_id>/calibration");
    }

    if errors > 0 {
        bail!("{} errors occurred", errors);
    }

    Ok(())
}
