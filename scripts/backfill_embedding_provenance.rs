//! Embedding Provenance Backfill (Spec 22, Phase 1b)
//!
//! One-shot script to stamp pre-Spec-22 vectors with synthetic provenance.
//!
//! For each of the five vector-bearing tables (`episodes`, `semantic_rules`,
//! `entities`, `communities`, `shopping_profiles`), this script populates the
//! Spec 22 provenance columns AND inserts a row into `embedding_provenance`
//! with `notes='backfill'`.
//!
//! Honesty discipline (per spec §1b):
//!   - episodes:         provenance_trusted=false (lossy source_text reconstruction)
//!   - semantic_rules:   provenance_trusted=true  (rule_content IS what was embedded)
//!   - entities:         provenance_trusted=true  (entity_name IS what was embedded)
//!   - communities:      provenance_trusted=false (centroid; no source text)
//!   - shopping_profiles: provenance_trusted=false (centroid; no source text)
//!
//! Usage:
//!   cargo run --bin backfill-embedding-provenance -- \
//!     --database-url postgresql://... \
//!     --model-id "anthropic/voyage-2" \
//!     --model-version "unknown_pre_provenance" \
//!     --dim 1024 \
//!     --batch-size 500 \
//!     --confirm
//!
//! Idempotent: rows already stamped (where embedding_model_id IS NOT NULL)
//! are skipped automatically. Re-run is safe.

use anyhow::{bail, Result};
use clap::Parser;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Database URL (or set DATABASE_URL env var).
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Model identifier to stamp on backfilled rows. Should match what was
    /// actually producing embeddings at the time (e.g. "anthropic/voyage-2").
    #[arg(long, default_value = "anthropic/voyage-2")]
    model_id: String,

    /// Model version to stamp. Use "unknown_pre_provenance" for vectors
    /// written before Spec 22.
    #[arg(long, default_value = "unknown_pre_provenance")]
    model_version: String,

    /// Output dimensionality of the historical embeddings.
    #[arg(long, default_value_t = 1024)]
    dim: i32,

    /// Batch size for UPDATE + INSERT chunks. Tune based on DB load.
    #[arg(long, default_value_t = 500)]
    batch_size: i64,

    /// Limit to a single agent (for staged rollout). If unset, processes all
    /// agents.
    #[arg(long)]
    agent_id: Option<uuid::Uuid>,

    /// Required to actually perform writes (prevents accidents).
    #[arg(long)]
    confirm: bool,

    /// Dry-run mode — print row counts that would be backfilled, write nothing.
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("🔧 Embedding Provenance Backfill (Spec 22 §1b)");
    println!("==============================================");
    println!("Database:       {}", redact_url(&args.database_url));
    println!("model_id:       {}", args.model_id);
    println!("model_version:  {}", args.model_version);
    println!("dim:            {}", args.dim);
    println!("batch_size:     {}", args.batch_size);
    println!(
        "agent_filter:   {}",
        args.agent_id.map(|u| u.to_string()).unwrap_or("ALL".into())
    );
    println!("dry_run:        {}", args.dry_run);
    println!();

    if !args.dry_run && !args.confirm {
        bail!("Refusing to write without --confirm (or use --dry-run)");
    }

    // Match the api-server's Neon-compatible pool config: PgBouncer in
    // transaction mode needs prepared statement cache disabled and DISCARD ALL
    // after connect to reset state. Use connect_lazy_with so Neon compute can
    // wake up on first query rather than during pool init.
    let opts = PgConnectOptions::from_str(&args.database_url)?.statement_cache_capacity(0);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .min_connections(0)
        .acquire_timeout(std::time::Duration::from_secs(120))
        .test_before_acquire(false)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::Executor::execute(conn, "DISCARD ALL")
                    .await
                    .map(|_| ())
            })
        })
        .connect_lazy_with(opts);

    let table_jobs: Vec<TableJob> = vec![
        TableJob {
            label: "episodes",
            target_table: "episodes",
            id_col: "episode_id",
            embed_col: "embedding",
            source_text_expr:
                "(query || ' ' || COALESCE(context->>'reasoning', ''))",
            source_ref_extra: "jsonb_build_object('original_query', query)",
            trusted: false,
            has_user_id: true,
        },
        TableJob {
            label: "semantic_rules",
            target_table: "semantic_rules",
            id_col: "rule_id",
            embed_col: "embedding",
            source_text_expr: "rule_content",
            source_ref_extra: "'{}'::jsonb",
            trusted: true,
            has_user_id: true,
        },
        TableJob {
            label: "entities",
            target_table: "entities",
            id_col: "entity_id",
            embed_col: "embedding",
            source_text_expr: "entity_name",
            source_ref_extra:
                "jsonb_build_object('source_episodes', source_episodes)",
            trusted: true,
            has_user_id: false,
        },
        TableJob {
            label: "communities",
            target_table: "communities",
            id_col: "community_id",
            embed_col: "embedding",
            // Centroid — no clean source_text. NULL is honest.
            source_text_expr: "NULL",
            source_ref_extra:
                "jsonb_build_object('member_entity_ids', member_entity_ids, 'centroid', true)",
            trusted: false,
            has_user_id: false,
        },
        TableJob {
            label: "shopping_profiles",
            target_table: "shopping_profiles",
            id_col: "profile_id",
            embed_col: "composite_embedding",
            source_text_expr: "NULL",
            source_ref_extra: "jsonb_build_object('centroid', true)",
            trusted: false,
            has_user_id: true,
        },
    ];

    let mut grand_total_backfilled: i64 = 0;
    for job in &table_jobs {
        let n = process_table(&pool, &args, job).await?;
        grand_total_backfilled += n;
    }

    println!();
    println!(
        "✅ Done. {} row{} {}.",
        grand_total_backfilled,
        if grand_total_backfilled == 1 { "" } else { "s" },
        if args.dry_run { "WOULD be backfilled (dry-run)" } else { "backfilled" }
    );
    Ok(())
}

struct TableJob {
    label: &'static str,
    target_table: &'static str,
    id_col: &'static str,
    embed_col: &'static str,
    /// SQL expression that produces the source_text from the row's columns.
    source_text_expr: &'static str,
    /// SQL expression for additional source_ref fields. Merged with
    /// `{"kind":"backfill"}`.
    source_ref_extra: &'static str,
    trusted: bool,
    has_user_id: bool,
}

async fn process_table(pool: &PgPool, args: &Args, job: &TableJob) -> Result<i64> {
    println!("── {} ──────────────────", job.label);

    let agent_filter_sql = if args.agent_id.is_some() {
        " AND agent_id = $1"
    } else {
        ""
    };

    // Count unstamped rows.
    let count_sql = format!(
        "SELECT COUNT(*) FROM {tbl}
         WHERE {embed} IS NOT NULL
           AND embedding_model_id IS NULL
           {agent_filter}",
        tbl = job.target_table,
        embed = job.embed_col,
        agent_filter = agent_filter_sql,
    );
    let unstamped: i64 = match args.agent_id {
        Some(aid) => {
            sqlx::query_scalar(&count_sql)
                .bind(aid)
                .fetch_one(pool)
                .await?
        }
        None => sqlx::query_scalar(&count_sql).fetch_one(pool).await?,
    };

    println!("  Unstamped rows: {}", unstamped);
    if unstamped == 0 {
        println!("  Nothing to do.");
        return Ok(0);
    }

    if args.dry_run {
        return Ok(unstamped);
    }

    // Loop until exhausted, in batches.
    let user_id_select = if job.has_user_id { "user_id" } else { "NULL::text" };
    let mut total_done = 0i64;
    loop {
        // Stamp a batch — update per-row provenance columns AND insert the
        // sidecar event row using a CTE.
        let stamp_sql = format!(
            r#"
            WITH targets AS (
                SELECT {id_col} AS tid, agent_id, {user_id_sel} AS uid, {embed_col} AS emb,
                       {source_text_expr} AS src_text
                  FROM {tbl}
                 WHERE {embed_col} IS NOT NULL
                   AND embedding_model_id IS NULL
                   {agent_filter}
                 ORDER BY {id_col}
                 LIMIT {batch}
            ),
            updated AS (
                UPDATE {tbl} t
                   SET embedding_model_id      = $1,
                       embedding_model_version = $2,
                       embedding_dim           = $3,
                       source_text             = targets.src_text,
                       source_ref              = jsonb_build_object('kind', 'backfill')
                                                  || ({source_ref_extra}),
                       provenance_trusted      = $4
                  FROM targets
                 WHERE t.{id_col} = targets.tid
              RETURNING t.{id_col} AS tid, t.agent_id, targets.uid AS uid,
                        targets.src_text AS src_text, t.{embed_col} AS emb
            ),
            inserted AS (
                INSERT INTO embedding_provenance (
                    target_table, target_id, agent_id, user_id,
                    source_text, source_ref,
                    model_id, model_version, dim, embedding,
                    trusted, notes
                )
                SELECT $5, updated.tid, updated.agent_id, updated.uid,
                       updated.src_text,
                       jsonb_build_object('kind', 'backfill'),
                       $1, $2, $3, updated.emb,
                       $4, 'backfill'
                  FROM updated
              RETURNING 1
            )
            SELECT COUNT(*) FROM updated
            "#,
            id_col = job.id_col,
            user_id_sel = user_id_select,
            embed_col = job.embed_col,
            source_text_expr = job.source_text_expr,
            source_ref_extra = job.source_ref_extra,
            tbl = job.target_table,
            agent_filter = if args.agent_id.is_some() { "AND agent_id = $6" } else { "" },
            batch = args.batch_size,
        );

        let count: i64 = match args.agent_id {
            Some(aid) => {
                sqlx::query_scalar(&stamp_sql)
                    .bind(&args.model_id)
                    .bind(&args.model_version)
                    .bind(args.dim)
                    .bind(job.trusted)
                    .bind(job.target_table)
                    .bind(aid)
                    .fetch_one(pool)
                    .await?
            }
            None => {
                sqlx::query_scalar(&stamp_sql)
                    .bind(&args.model_id)
                    .bind(&args.model_version)
                    .bind(args.dim)
                    .bind(job.trusted)
                    .bind(job.target_table)
                    .fetch_one(pool)
                    .await?
            }
        };

        if count == 0 {
            break;
        }
        total_done += count;
        println!("  Stamped {} rows (total: {})", count, total_done);
        if count < args.batch_size {
            break;
        }
    }

    println!("  ✓ {} backfilled.", total_done);
    Ok(total_done)
}

fn redact_url(url: &str) -> String {
    // Strip password from "postgresql://user:pass@host/db" for logs.
    if let Some(at_idx) = url.find('@') {
        if let Some(scheme_idx) = url.find("://") {
            let prefix = &url[..scheme_idx + 3];
            let after = &url[at_idx..];
            return format!("{}***{}", prefix, after);
        }
    }
    url.to_string()
}
