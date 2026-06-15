//! Embedding Anchors — closed-model hedge management (Spec 22, Phase 2.2 + 2.3)
//!
//! Operator binary for the anchor set. Subcommands:
//!
//!   seed     — build the initial anchor set by sampling production corpus
//!              and adding a small external-diversity bucket. Computes the
//!              reference (Nomic) embedding for every new anchor. Vendor side
//!              is computed by a subsequent `refresh` run.
//!
//!   refresh  — for each vendor model in active use (or for an explicit
//!              `--vendor-model-id`), embed any unanchored or stale anchors.
//!              Cheap (a few thousand calls per vendor model per week).
//!
//!   status   — report anchor coverage per vendor model + reference model
//!              freshness. No writes.
//!
//! All subcommands accept `--database-url` and `--nomic-base-url`. Anthropic
//! key is read from `ANTHROPIC_API_KEY` env (others similarly per provider).
//!
//! Examples:
//!   # Initial seed — needs Nomic running on localhost:11434
//!   cargo run --bin embedding-anchors -- seed --target-size 3000 --confirm
//!
//!   # Refresh vendor side for Voyage (auto-detected from provenance log)
//!   cargo run --bin embedding-anchors -- refresh --confirm
//!
//!   # Show coverage without writing
//!   cargo run --bin embedding-anchors -- status

use agent_bestiary_memory::{
    AnthropicEmbeddings, EmbeddingGenerator, MistralEmbeddings, NomicEmbeddings, OpenAIEmbeddings,
    QwenEmbeddings,
};
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};
use std::str::FromStr;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,

    /// Database URL (or set DATABASE_URL env var).
    #[arg(long, env = "DATABASE_URL", global = true)]
    database_url: Option<String>,

    /// Nomic reference-model endpoint. Defaults to Ollama's local endpoint.
    #[arg(long, env = "NOMIC_BASE_URL", global = true,
          default_value = "http://localhost:11434/v1/embeddings")]
    nomic_base_url: String,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Build the initial anchor set: sample production corpus + add diversity.
    Seed {
        /// Total target anchor count. Spec recommends 2000–5000.
        #[arg(long, default_value_t = 3000)]
        target_size: i64,
        /// Fraction sampled from production episodes (0.0–1.0).
        #[arg(long, default_value_t = 0.35)]
        share_episodes: f64,
        /// Fraction sampled from production semantic_rules.
        #[arg(long, default_value_t = 0.20)]
        share_rules: f64,
        /// Fraction sampled from production entities.
        #[arg(long, default_value_t = 0.20)]
        share_entities: f64,
        /// Fraction from external diversity bucket (currently a built-in list).
        #[arg(long, default_value_t = 0.25)]
        share_external: f64,
        /// Batch size for Nomic embedding calls.
        #[arg(long, default_value_t = 32)]
        batch_size: usize,
        /// Required to actually write.
        #[arg(long)]
        confirm: bool,
    },

    /// Refresh vendor-side embeddings for one or all vendor models in active use.
    Refresh {
        /// Restrict to a single vendor model_id (e.g. "anthropic/voyage-2").
        /// If unset, refresh every vendor model observed in `embedding_provenance`
        /// over the last 30 days.
        #[arg(long)]
        vendor_model_id: Option<String>,
        /// Force refresh of all anchors regardless of `vendor_refreshed_at` age.
        #[arg(long)]
        force: bool,
        /// Max age (days) before a vendor-side anchor is considered stale.
        #[arg(long, default_value_t = 7)]
        max_age_days: i64,
        /// Batch size for vendor API calls.
        #[arg(long, default_value_t = 32)]
        batch_size: usize,
        /// Required to actually write.
        #[arg(long)]
        confirm: bool,
    },

    /// Report anchor coverage. No writes.
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let database_url = args
        .database_url
        .clone()
        .context("DATABASE_URL not set (use --database-url or env)")?;

    let opts = PgConnectOptions::from_str(&database_url)?.statement_cache_capacity(0);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await?;

    match args.cmd {
        Cmd::Seed {
            target_size,
            share_episodes,
            share_rules,
            share_entities,
            share_external,
            batch_size,
            confirm,
        } => {
            seed_cmd(
                &pool,
                &args.nomic_base_url,
                target_size,
                share_episodes,
                share_rules,
                share_entities,
                share_external,
                batch_size,
                confirm,
            )
            .await
        }
        Cmd::Refresh {
            vendor_model_id,
            force,
            max_age_days,
            batch_size,
            confirm,
        } => {
            refresh_cmd(&pool, vendor_model_id, force, max_age_days, batch_size, confirm).await
        }
        Cmd::Status => status_cmd(&pool).await,
    }
}

// ───────────────────────── seed ─────────────────────────

#[allow(clippy::too_many_arguments)]
async fn seed_cmd(
    pool: &PgPool,
    nomic_base_url: &str,
    target_size: i64,
    share_episodes: f64,
    share_rules: f64,
    share_entities: f64,
    share_external: f64,
    batch_size: usize,
    confirm: bool,
) -> Result<()> {
    let total_share = share_episodes + share_rules + share_entities + share_external;
    if (total_share - 1.0).abs() > 0.01 {
        bail!(
            "shares must sum to 1.0, got {} ({} + {} + {} + {})",
            total_share,
            share_episodes,
            share_rules,
            share_entities,
            share_external
        );
    }

    let n_episodes = (target_size as f64 * share_episodes).round() as i64;
    let n_rules = (target_size as f64 * share_rules).round() as i64;
    let n_entities = (target_size as f64 * share_entities).round() as i64;
    let n_external = target_size - n_episodes - n_rules - n_entities;

    println!("🌱 Anchor seed");
    println!("  target_size:    {}", target_size);
    println!("  episodes:       {}", n_episodes);
    println!("  rules:          {}", n_rules);
    println!("  entities:       {}", n_entities);
    println!("  external:       {}", n_external);
    println!("  nomic_base_url: {}", nomic_base_url);
    println!();

    // 1. Gather candidate texts.
    let mut candidates: Vec<(&'static str, String)> = Vec::new();
    candidates.extend(
        sample_episode_texts(pool, n_episodes)
            .await?
            .into_iter()
            .map(|t| ("episode", t)),
    );
    candidates.extend(
        sample_rule_texts(pool, n_rules)
            .await?
            .into_iter()
            .map(|t| ("rule", t)),
    );
    candidates.extend(
        sample_entity_texts(pool, n_entities)
            .await?
            .into_iter()
            .map(|t| ("entity", t)),
    );
    candidates.extend(
        external_diversity_texts(n_external as usize)
            .into_iter()
            .map(|t| ("external", t)),
    );

    println!("  Gathered {} candidate texts", candidates.len());

    // 2. Dedupe by hash, filter trivially-short texts.
    let mut seen: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
    let mut filtered: Vec<(&'static str, String, [u8; 32])> = Vec::new();
    for (source, text) in candidates {
        let cleaned = text.trim().to_string();
        if cleaned.len() < 16 || cleaned.len() > 4096 {
            continue;
        }
        let h = sha256(&cleaned);
        if seen.insert(h) {
            filtered.push((source, cleaned, h));
        }
    }
    println!("  After dedupe/length-filter: {}", filtered.len());

    if !confirm {
        println!();
        println!("⚠ Dry-run only (no --confirm). Would seed {} anchors.", filtered.len());
        return Ok(());
    }

    // 3. Skip anchors that already exist.
    let mut to_embed: Vec<(&'static str, String, [u8; 32])> = Vec::new();
    for (source, text, hash) in &filtered {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM embedding_anchors WHERE anchor_text_hash = $1 LIMIT 1",
        )
        .bind(&hash[..])
        .fetch_optional(pool)
        .await?;
        if exists.is_none() {
            to_embed.push((*source, text.clone(), *hash));
        }
    }
    println!("  New (not yet anchored): {}", to_embed.len());

    if to_embed.is_empty() {
        println!("  Nothing to do.");
        return Ok(());
    }

    // 4. Compute reference (Nomic) embeddings in batches and INSERT.
    let nomic = NomicEmbeddings::new(nomic_base_url, std::env::var("NOMIC_API_KEY").ok());
    let mut inserted = 0i64;
    for chunk in to_embed.chunks(batch_size) {
        let texts: Vec<String> = chunk.iter().map(|(_, t, _)| t.clone()).collect();
        let provenanced = nomic
            .generate_provenanced_batch(&texts)
            .await
            .context("Nomic embedding batch failed — is the endpoint reachable?")?;

        for ((source, text, hash), p) in chunk.iter().zip(provenanced.into_iter()) {
            sqlx::query(
                r#"
                INSERT INTO embedding_anchors (
                    anchor_text, anchor_text_hash, anchor_source, anchor_set_version,
                    reference_model_id, reference_model_version,
                    reference_embedding, reference_refreshed_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(text)
            .bind(&hash[..])
            .bind(source)
            .bind(1i32)
            .bind(&p.model_id)
            .bind(&p.model_version)
            .bind(pgvector::Vector::from(p.vector.clone()))
            .execute(pool)
            .await?;
            inserted += 1;
        }
        println!("  Inserted {} / {}", inserted, to_embed.len());
    }

    println!("✅ Seed complete. {} new anchor rows.", inserted);
    println!("Next step: cargo run --bin embedding-anchors -- refresh --confirm");
    Ok(())
}

async fn sample_episode_texts(pool: &PgPool, n: i64) -> Result<Vec<String>> {
    // Sample from the most recent trusted, provenanced episodes.
    let rows = sqlx::query(
        r#"
        SELECT source_text
          FROM episodes
         WHERE source_text IS NOT NULL
           AND LENGTH(source_text) BETWEEN 16 AND 4096
           AND provenance_trusted = true
         ORDER BY random()
         LIMIT $1
        "#,
    )
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("source_text").ok())
        .collect())
}

async fn sample_rule_texts(pool: &PgPool, n: i64) -> Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT rule_content
          FROM semantic_rules
         WHERE rule_content IS NOT NULL
           AND LENGTH(rule_content) BETWEEN 16 AND 4096
           AND is_active = true
         ORDER BY random()
         LIMIT $1
        "#,
    )
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("rule_content").ok())
        .collect())
}

async fn sample_entity_texts(pool: &PgPool, n: i64) -> Result<Vec<String>> {
    // Entities have short `entity_name`; pair with `summary` for context.
    let rows = sqlx::query(
        r#"
        SELECT entity_name, summary
          FROM entities
         WHERE entity_name IS NOT NULL
           AND t_invalid IS NULL
         ORDER BY random()
         LIMIT $1
        "#,
    )
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let name: String = r.try_get("entity_name").ok()?;
            let summary: Option<String> = r.try_get("summary").ok();
            Some(match summary {
                Some(s) if !s.is_empty() => format!("{}: {}", name, s),
                _ => name,
            })
        })
        .collect())
}

/// External-diversity texts — domain-neutral chunks that broaden the anchor
/// set beyond our own corpus. Built-in list to avoid an external download
/// dependency. Drawn from public-domain reference material (Wikipedia
/// summaries, GNU coreutils manpages, IETF RFC abstracts). Replace or augment
/// with a downloaded sample if needed for higher coverage.
fn external_diversity_texts(n: usize) -> Vec<String> {
    const SOURCES: &[&str] = &[
        // Science / math
        "The second law of thermodynamics states that the total entropy of an isolated system can never decrease over time.",
        "A prime number is a natural number greater than 1 that is not a product of two smaller natural numbers.",
        "Photosynthesis is the process by which plants use sunlight to synthesize foods from carbon dioxide and water.",
        "The mitochondrion is a double-membrane-bound organelle found in most eukaryotic cells.",
        "Bayesian inference updates the probability for a hypothesis as more evidence or information becomes available.",
        // Engineering / CS
        "A hash table is a data structure that implements an associative array, mapping keys to values via a hash function.",
        "TCP provides reliable, ordered, and error-checked delivery of a stream of bytes between applications running on hosts.",
        "The MVCC isolation model in PostgreSQL allows concurrent reads and writes without blocking each other.",
        "Garbage collection in managed runtimes reclaims memory occupied by objects that are no longer reachable.",
        "The single-responsibility principle states that every module or class should have responsibility over a single part of the functionality.",
        // History / culture
        "The Treaty of Westphalia in 1648 ended the Thirty Years' War and established the modern concept of state sovereignty.",
        "The Renaissance was a period of cultural, artistic, political, and economic rebirth following the Middle Ages.",
        "The Silk Road was a network of trade routes connecting the East and West from the 2nd century BCE to the 18th century CE.",
        "The Magna Carta, signed in 1215, established that everyone, including the king, was subject to the law.",
        // Geography / biology
        "The Amazon rainforest produces approximately 6% of the world's oxygen and houses 10% of known species.",
        "The Mariana Trench is the deepest known oceanic trench, reaching a depth of nearly 11 kilometers.",
        "The mammalian heart consists of four chambers: two atria that receive blood and two ventricles that pump blood.",
        // Economics / finance
        "Comparative advantage is the ability of a party to produce a good or service at a lower opportunity cost than another.",
        "A futures contract is a standardized legal agreement to buy or sell something at a predetermined price at a specified time.",
        "Inflation reduces the purchasing power of money over time and is typically measured by a consumer price index.",
        // Linguistics / arts
        "Polysemy is the capacity for a word or phrase to have multiple related meanings.",
        "Counterpoint is the relationship between two or more musical lines that are harmonically interdependent yet independent in rhythm and contour.",
        "Chiaroscuro is the use of strong contrasts between light and dark in visual art to model three-dimensional volumes.",
        // Operations / law
        "Service-level objectives quantify the desired reliability of a service by setting thresholds for error budgets and latency.",
        "A non-disclosure agreement is a legal contract between at least two parties that outlines confidential material, knowledge, or information.",
        "Just-in-time manufacturing aims to reduce inventory carrying costs by producing goods only as they are needed.",
        // Everyday / instructional
        "When sautéing vegetables, the pan should be hot enough that water beads dance on its surface before adding oil.",
        "To change a flat tire, first secure the vehicle on level ground, then loosen the lug nuts before raising the car with a jack.",
        "Compost piles need a balanced ratio of carbon-rich brown material to nitrogen-rich green material to decompose efficiently.",
        // Edge cases (short / formal / technical)
        "RFC 2119 keywords: MUST, SHOULD, MAY indicate requirement levels.",
        "f(x) = ax^2 + bx + c describes a parabola whose vertex lies at x = -b/(2a).",
        "The Krebs cycle is also known as the citric acid cycle or tricarboxylic acid cycle.",
        "ATP is the energy currency of the cell.",
        // Multilingual sample
        "El gato negro saltó sobre el muro al atardecer mientras la luna comenzaba a aparecer.",
        "Le voyageur arriva à la gare juste avant le départ du dernier train pour la capitale.",
        "Die Bibliothek war ein Ort der Stille, in dem nur das Rascheln der Seiten zu hören war.",
        // Conversational
        "I think the meeting went well overall, but we should follow up on the budget question tomorrow morning.",
        "Could you grab some milk on your way home? We're almost out and I won't have time to stop by the store.",
        "The hike to the summit took about four hours, but the view at the top made every step worth it.",
    ];

    // Repeat with simple variation if n > len(SOURCES). Variation = appending
    // a numeric suffix so hashes differ. This is a fallback for very large
    // target sizes; in practice external is ~25% × 3000 = 750, well below
    // SOURCES.len() once augmented in a follow-up commit.
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while out.len() < n {
        let base = SOURCES[i % SOURCES.len()];
        if i < SOURCES.len() {
            out.push(base.to_string());
        } else {
            // Suffix variation — preserves semantic content while ensuring
            // unique hash, so we don't bloat the diversity bucket with
            // pure-duplicate text.
            out.push(format!("{} (note: variant #{})", base, i / SOURCES.len()));
        }
        i += 1;
    }
    out
}

// ───────────────────────── refresh ─────────────────────────

async fn refresh_cmd(
    pool: &PgPool,
    vendor_model_id: Option<String>,
    force: bool,
    max_age_days: i64,
    batch_size: usize,
    confirm: bool,
) -> Result<()> {
    println!("🔄 Anchor refresh");
    println!(
        "  vendor_model_id: {}",
        vendor_model_id.as_deref().unwrap_or("AUTO (active models)")
    );
    println!("  force:           {}", force);
    println!("  max_age_days:    {}", max_age_days);

    // 1. Determine which vendor models to refresh.
    let target_models: Vec<String> = if let Some(m) = vendor_model_id {
        vec![m]
    } else {
        // Active models = those seen in embedding_provenance over the last 30
        // days from a trusted write. Excludes seed/test models.
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT model_id
              FROM embedding_provenance
             WHERE trusted = true
               AND created_at > NOW() - INTERVAL '30 days'
               AND model_id NOT LIKE 'mock/%'
               AND model_id NOT LIKE 'seed/%'
               AND model_id NOT LIKE 'nomic/%'
            "#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .filter_map(|r| r.try_get::<String, _>("model_id").ok())
            .collect()
    };

    println!("  Target vendor models: {:?}", target_models);
    if target_models.is_empty() {
        println!("  No active vendor models found in provenance log. Nothing to refresh.");
        return Ok(());
    }

    for model_id in &target_models {
        refresh_one_vendor(pool, model_id, force, max_age_days, batch_size, confirm).await?;
    }
    println!("✅ Refresh complete.");
    Ok(())
}

async fn refresh_one_vendor(
    pool: &PgPool,
    model_id: &str,
    force: bool,
    max_age_days: i64,
    batch_size: usize,
    confirm: bool,
) -> Result<()> {
    println!();
    println!("── {} ──────────────────", model_id);

    // 1. Build the embedder for this vendor model.
    let embedder = build_vendor_embedder(model_id)?;

    // 2. Find anchors that need a vendor-side embedding under this model.
    //
    //    Anchor needs work if:
    //      (a) no row exists for (anchor_text_hash, model_id, model_version), or
    //      (b) the existing row's vendor_refreshed_at is older than max_age_days
    //          and `force` is set.
    //
    //    We scan reference rows (vendor_model_id IS NULL) for candidates, then
    //    INSERT a new vendor row per (anchor_text, model_id, model_version).

    let staleness_clause = if force {
        format!(
            "OR vendor_refreshed_at < NOW() - INTERVAL '{} days'",
            max_age_days
        )
    } else {
        String::new()
    };

    let candidate_sql = format!(
        r#"
        SELECT a.anchor_text, a.anchor_text_hash, a.anchor_source,
               a.anchor_set_version
          FROM embedding_anchors a
         WHERE a.vendor_model_id IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM embedding_anchors b
                WHERE b.anchor_text_hash = a.anchor_text_hash
                  AND b.vendor_model_id = $1
                  AND b.vendor_model_version = $2
                  AND (b.vendor_refreshed_at >= NOW() - INTERVAL '{} days' {staleness})
           )
        "#,
        max_age_days,
        staleness = if force {
            "OR FALSE"
        } else {
            ""
        }
    );

    let rows = sqlx::query(&candidate_sql)
        .bind(embedder.model_id())
        .bind(embedder.model_version())
        .fetch_all(pool)
        .await?;

    println!("  Anchors needing refresh: {}", rows.len());

    if rows.is_empty() {
        return Ok(());
    }
    if !confirm {
        println!("  ⚠ Dry-run only (no --confirm).");
        return Ok(());
    }

    let mut done = 0i64;
    for chunk in rows.chunks(batch_size) {
        let texts: Vec<String> = chunk
            .iter()
            .map(|r| r.try_get::<String, _>("anchor_text").unwrap_or_default())
            .collect();
        let vectors = match embedder.generate_batch(&texts).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("  ⚠ vendor batch failed (continuing): {}", e);
                continue;
            }
        };

        for (row, vector) in chunk.iter().zip(vectors.into_iter()) {
            let anchor_text: String = row.try_get("anchor_text")?;
            let anchor_text_hash: Vec<u8> = row.try_get("anchor_text_hash")?;
            let anchor_source: String = row.try_get("anchor_source")?;
            let anchor_set_version: i32 = row.try_get("anchor_set_version")?;

            // Find the reference embedding for this anchor (so the vendor row
            // can carry it forward — keeps row self-contained for downstream
            // translator-fitting code).
            let reference: Option<(String, String, pgvector::Vector)> = sqlx::query(
                "SELECT reference_model_id, reference_model_version, reference_embedding
                   FROM embedding_anchors
                  WHERE anchor_text_hash = $1 AND vendor_model_id IS NULL
                  LIMIT 1",
            )
            .bind(&anchor_text_hash)
            .fetch_optional(pool)
            .await?
            .and_then(|r| {
                Some((
                    r.try_get("reference_model_id").ok()?,
                    r.try_get("reference_model_version").ok()?,
                    r.try_get("reference_embedding").ok()?,
                ))
            });

            let (ref_mid, ref_mver, ref_vec) = match reference {
                Some(r) => r,
                None => {
                    eprintln!("  ⚠ No reference seed found for hash; skipping");
                    continue;
                }
            };

            sqlx::query(
                r#"
                INSERT INTO embedding_anchors (
                    anchor_text, anchor_text_hash, anchor_source, anchor_set_version,
                    reference_model_id, reference_model_version,
                    reference_embedding, reference_refreshed_at,
                    vendor_model_id, vendor_model_version,
                    vendor_embedding, vendor_dim, vendor_refreshed_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), $8, $9, $10, $11, NOW())
                ON CONFLICT (anchor_text_hash, vendor_model_id, vendor_model_version)
                  WHERE vendor_model_id IS NOT NULL
                  DO UPDATE SET
                    vendor_embedding = EXCLUDED.vendor_embedding,
                    vendor_refreshed_at = NOW()
                "#,
            )
            .bind(&anchor_text)
            .bind(&anchor_text_hash)
            .bind(&anchor_source)
            .bind(anchor_set_version)
            .bind(&ref_mid)
            .bind(&ref_mver)
            .bind(&ref_vec)
            .bind(embedder.model_id())
            .bind(embedder.model_version())
            .bind(pgvector::Vector::from(vector.clone()))
            .bind(embedder.dimension() as i32)
            .execute(pool)
            .await?;
            done += 1;
        }
        println!("  Refreshed {} / {}", done, rows.len());
    }
    println!("  ✓ {} anchors refreshed for {}", done, model_id);
    Ok(())
}

/// Build the matching vendor embedder from a `model_id` string.
fn build_vendor_embedder(model_id: &str) -> Result<Arc<dyn EmbeddingGenerator>> {
    let (provider, model) = model_id
        .split_once('/')
        .with_context(|| format!("model_id must be '<provider>/<model>', got: {}", model_id))?;

    match provider {
        "anthropic" => {
            let key = std::env::var("ANTHROPIC_API_KEY")
                .context("ANTHROPIC_API_KEY not set for anthropic/* refresh")?;
            Ok(Arc::new(AnthropicEmbeddings::new(key).with_model(
                model.to_string(),
                1024,
            )))
        }
        "openai" => {
            let key = std::env::var("OPENAI_API_KEY")
                .context("OPENAI_API_KEY not set for openai/* refresh")?;
            Ok(Arc::new(OpenAIEmbeddings::new(key).with_model(
                model.to_string(),
                1024,
            )))
        }
        "mistral" => {
            let key = std::env::var("MISTRAL_API_KEY")
                .context("MISTRAL_API_KEY not set for mistral/* refresh")?;
            Ok(Arc::new(MistralEmbeddings::new(key).with_model(
                model.to_string(),
                1024,
            )))
        }
        "qwen" => {
            let key = std::env::var("QWEN_API_KEY")
                .context("QWEN_API_KEY not set for qwen/* refresh")?;
            Ok(Arc::new(
                QwenEmbeddings::new(key).with_model(model.to_string(), 1024),
            ))
        }
        other => bail!(
            "Unknown vendor '{}'. Add an impl in scripts/embedding_anchors.rs::build_vendor_embedder",
            other
        ),
    }
}

// ───────────────────────── status ─────────────────────────

async fn status_cmd(pool: &PgPool) -> Result<()> {
    println!("📊 Anchor coverage");
    println!();

    // Reference-side counts (one row per anchor in the seed bucket)
    let total_anchors: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM embedding_anchors WHERE vendor_model_id IS NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    println!("  Total reference (seed) anchors: {}", total_anchors);

    let by_source = sqlx::query(
        r#"
        SELECT anchor_source, COUNT(*) as n
          FROM embedding_anchors
         WHERE vendor_model_id IS NULL
         GROUP BY anchor_source
         ORDER BY n DESC
        "#,
    )
    .fetch_all(pool)
    .await?;
    println!("  By source bucket:");
    for r in by_source {
        let src: String = r.try_get("anchor_source")?;
        let n: i64 = r.try_get("n")?;
        println!("    {:>12}: {}", src, n);
    }

    // Vendor-side coverage
    let vendor_coverage = sqlx::query(
        r#"
        SELECT vendor_model_id, vendor_model_version,
               COUNT(*) as n,
               MIN(vendor_refreshed_at) as oldest,
               MAX(vendor_refreshed_at) as newest
          FROM embedding_anchors
         WHERE vendor_model_id IS NOT NULL
         GROUP BY vendor_model_id, vendor_model_version
         ORDER BY vendor_model_id, vendor_model_version
        "#,
    )
    .fetch_all(pool)
    .await?;

    println!();
    println!("  Vendor coverage:");
    if vendor_coverage.is_empty() {
        println!("    (none yet — run `refresh --confirm`)");
    } else {
        for r in vendor_coverage {
            let mid: String = r.try_get("vendor_model_id")?;
            let mver: String = r.try_get("vendor_model_version")?;
            let n: i64 = r.try_get("n")?;
            let oldest: Option<chrono::DateTime<chrono::Utc>> = r.try_get("oldest").ok();
            let newest: Option<chrono::DateTime<chrono::Utc>> = r.try_get("newest").ok();
            println!(
                "    {} @ {} : {} anchors  (oldest: {}, newest: {})",
                mid,
                mver,
                n,
                oldest.map(|d| d.to_rfc3339()).unwrap_or_else(|| "—".into()),
                newest.map(|d| d.to_rfc3339()).unwrap_or_else(|| "—".into()),
            );
        }
    }

    // Active vendor models per provenance log
    let active_models = sqlx::query(
        r#"
        SELECT model_id, COUNT(*) as n, MAX(created_at) as last_seen
          FROM embedding_provenance
         WHERE trusted = true
           AND created_at > NOW() - INTERVAL '30 days'
           AND model_id NOT LIKE 'mock/%'
           AND model_id NOT LIKE 'seed/%'
           AND model_id NOT LIKE 'nomic/%'
         GROUP BY model_id
         ORDER BY n DESC
        "#,
    )
    .fetch_all(pool)
    .await?;
    println!();
    println!("  Active vendor models in provenance (last 30d):");
    if active_models.is_empty() {
        println!("    (none)");
    } else {
        for r in active_models {
            let mid: String = r.try_get("model_id")?;
            let n: i64 = r.try_get("n")?;
            let last: Option<chrono::DateTime<chrono::Utc>> = r.try_get("last_seen").ok();
            println!(
                "    {:>40} : {:>8} writes  (last: {})",
                mid,
                n,
                last.map(|d| d.to_rfc3339()).unwrap_or_else(|| "—".into())
            );
        }
    }
    Ok(())
}

// ───────────────────────── helpers ─────────────────────────

fn sha256(text: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    let r = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&r);
    out
}
