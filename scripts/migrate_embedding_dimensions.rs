//! Embedding Dimension Migration Script
//!
//! This script migrates the PostgreSQL schema to support different embedding dimensions.
//!
//! Usage:
//!   cargo run --bin migrate-embedding-dimensions -- \
//!     --database-url postgresql://... \
//!     --old-dimensions 1024 \
//!     --new-dimensions 1536
//!
//! WARNING: This is a destructive operation. Backup your database first!

use anyhow::{bail, Result};
use clap::Parser;
use sqlx::PgPool;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Database URL
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Current embedding dimensions (e.g., 1024)
    #[arg(long)]
    old_dimensions: usize,

    /// New embedding dimensions (e.g., 1536)
    #[arg(long)]
    new_dimensions: usize,

    /// Confirm the migration (required to prevent accidents)
    #[arg(long)]
    confirm: bool,

    /// Dry run - show what would be done without making changes
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("🔄 Embedding Dimension Migration Tool");
    println!("=====================================");
    println!();
    println!("Database: {}", args.database_url);
    println!(
        "Migration: {}d → {}d",
        args.old_dimensions, args.new_dimensions
    );
    println!();

    if !args.confirm && !args.dry_run {
        bail!(
            "⚠️  SAFETY CHECK FAILED\n\n\
            This is a DESTRUCTIVE operation that will modify your database schema.\n\
            All existing embeddings will be DELETED and need to be regenerated.\n\n\
            To proceed, add --confirm flag:\n\
            cargo run --bin migrate_embedding_dimensions -- \\\n  \
              --database-url $DATABASE_URL \\\n  \
              --old-dimensions {} \\\n  \
              --new-dimensions {} \\\n  \
              --confirm\n\n\
            Or use --dry-run to see what would happen without making changes.",
            args.old_dimensions,
            args.new_dimensions
        );
    }

    // Connect to database
    println!("📡 Connecting to database...");
    let pool = PgPool::connect(&args.database_url).await?;
    println!("✅ Connected!");
    println!();

    // Check current schema
    println!("🔍 Checking current schema...");
    let current_dims = check_current_dimensions(&pool).await?;

    if current_dims != args.old_dimensions {
        bail!(
            "Schema dimension mismatch!\n\
            You specified --old-dimensions {}, but database has {}d vectors.\n\
            Please correct the --old-dimensions parameter.",
            args.old_dimensions,
            current_dims
        );
    }

    println!("✅ Schema has {}d vectors as expected", current_dims);
    println!();

    // Count existing data
    println!("📊 Analyzing existing data...");
    let stats = get_embedding_stats(&pool).await?;
    println!(
        "   Episodes with embeddings: {}",
        stats.episodes_with_embeddings
    );
    println!("   Rules with embeddings: {}", stats.rules_with_embeddings);
    println!(
        "   Entities with embeddings: {}",
        stats.entities_with_embeddings
    );
    println!(
        "   Communities with embeddings: {}",
        stats.communities_with_embeddings
    );
    println!(
        "   Total embeddings to migrate: {}",
        stats.total_embeddings()
    );
    println!();

    if args.dry_run {
        println!("🧪 DRY RUN MODE - No changes will be made");
        println!();
        println!("Migration plan:");
        println!("1. Backup current embeddings (recommended)");
        println!(
            "2. Alter episodes.embedding from vector({}) to vector({})",
            current_dims, args.new_dimensions
        );
        println!(
            "3. Alter semantic_rules.embedding from vector({}) to vector({})",
            current_dims, args.new_dimensions
        );
        println!(
            "4. Alter entities.embedding from vector({}) to vector({})",
            current_dims, args.new_dimensions
        );
        println!(
            "5. Alter communities.embedding from vector({}) to vector({})",
            current_dims, args.new_dimensions
        );
        println!("6. Set all embeddings to NULL (must be regenerated)");
        println!("7. Rebuild vector indexes");
        println!();
        println!("⚠️  After migration:");
        println!("   - All embeddings will be NULL");
        println!("   - You must re-embed all data with the new model");
        println!("   - Use fermi-consolidate with new --embedding-dimensions flag");
        println!();
        println!("To execute this migration, run again with --confirm flag");
        return Ok(());
    }

    // Confirmation
    println!("⚠️  WARNING: This will DELETE all existing embeddings!");
    println!("⚠️  You will need to regenerate all embeddings after migration.");
    println!();
    println!("Proceeding with migration...");
    println!();

    // Step 1: Backup recommendation
    println!("📦 Step 1: Database backup");
    println!("   ⚠️  Did you backup your database? (This script doesn't create backups)");
    println!("   Recommended: pg_dump -h $HOST -U $USER -d fermi > backup.sql");
    println!();

    // Step 2: Alter tables
    println!("🔧 Step 2: Altering table schemas...");

    // We need to drop indexes first, alter columns, then recreate indexes
    println!("   Dropping vector indexes...");
    sqlx::query("DROP INDEX IF EXISTS idx_episodes_embedding")
        .execute(&pool)
        .await?;
    sqlx::query("DROP INDEX IF EXISTS idx_semantic_rules_embedding")
        .execute(&pool)
        .await?;
    sqlx::query("DROP INDEX IF EXISTS idx_entities_embedding")
        .execute(&pool)
        .await?;
    sqlx::query("DROP INDEX IF EXISTS idx_communities_embedding")
        .execute(&pool)
        .await?;
    println!("   ✅ Indexes dropped");

    println!("   Altering episodes.embedding...");
    sqlx::query(&format!(
        "ALTER TABLE episodes ALTER COLUMN embedding TYPE vector({})",
        args.new_dimensions
    ))
    .execute(&pool)
    .await?;

    println!("   Altering semantic_rules.embedding...");
    sqlx::query(&format!(
        "ALTER TABLE semantic_rules ALTER COLUMN embedding TYPE vector({})",
        args.new_dimensions
    ))
    .execute(&pool)
    .await?;

    println!("   Altering entities.embedding...");
    sqlx::query(&format!(
        "ALTER TABLE entities ALTER COLUMN embedding TYPE vector({})",
        args.new_dimensions
    ))
    .execute(&pool)
    .await?;

    println!("   Altering communities.embedding...");
    sqlx::query(&format!(
        "ALTER TABLE communities ALTER COLUMN embedding TYPE vector({})",
        args.new_dimensions
    ))
    .execute(&pool)
    .await?;

    println!("   ✅ Schema altered to {}d", args.new_dimensions);
    println!();

    // Step 3: Clear embeddings (they're invalid now)
    println!("🗑️  Step 3: Clearing old embeddings...");
    sqlx::query("UPDATE episodes SET embedding = NULL WHERE embedding IS NOT NULL")
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE semantic_rules SET embedding = NULL WHERE embedding IS NOT NULL")
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE entities SET embedding = NULL WHERE embedding IS NOT NULL")
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE communities SET embedding = NULL WHERE embedding IS NOT NULL")
        .execute(&pool)
        .await?;
    println!("   ✅ Embeddings cleared");
    println!();

    // Step 4: Recreate indexes
    println!("🔨 Step 4: Recreating vector indexes...");
    sqlx::query("CREATE INDEX idx_episodes_embedding ON episodes USING ivfflat (embedding vector_cosine_ops)")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE INDEX idx_semantic_rules_embedding ON semantic_rules USING ivfflat (embedding vector_cosine_ops)")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE INDEX idx_entities_embedding ON entities USING ivfflat (embedding vector_cosine_ops)")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE INDEX idx_communities_embedding ON communities USING ivfflat (embedding vector_cosine_ops)")
        .execute(&pool)
        .await?;
    println!("   ✅ Indexes recreated");
    println!();

    // Done
    println!("✅ Migration complete!");
    println!();
    println!("📋 Next steps:");
    println!("1. Update your agent configurations to use new embedding dimensions");
    println!("2. Re-embed all episodes using your new embedding model:");
    println!(
        "   cargo run --bin re_embed_episodes -- --agent-id <UUID> --dimensions {}",
        args.new_dimensions
    );
    println!("3. Run consolidation with new dimensions:");
    println!(
        "   fermi-consolidate --embedding-dimensions {} --embedding-provider <provider>",
        args.new_dimensions
    );
    println!();
    println!("⚠️  Until you re-embed, similarity search will not work!");

    Ok(())
}

async fn check_current_dimensions(pool: &PgPool) -> Result<usize> {
    let row: (i32,) = sqlx::query_as(
        "SELECT atttypmod - 4 as dims
         FROM pg_attribute
         WHERE attrelid = 'episodes'::regclass
         AND attname = 'embedding'",
    )
    .fetch_one(pool)
    .await?;

    Ok(row.0 as usize)
}

#[derive(Debug)]
struct EmbeddingStats {
    episodes_with_embeddings: i64,
    rules_with_embeddings: i64,
    entities_with_embeddings: i64,
    communities_with_embeddings: i64,
}

impl EmbeddingStats {
    fn total_embeddings(&self) -> i64 {
        self.episodes_with_embeddings
            + self.rules_with_embeddings
            + self.entities_with_embeddings
            + self.communities_with_embeddings
    }
}

async fn get_embedding_stats(pool: &PgPool) -> Result<EmbeddingStats> {
    let episodes: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM episodes WHERE embedding IS NOT NULL")
            .fetch_one(pool)
            .await?;

    let rules: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM semantic_rules WHERE embedding IS NOT NULL")
            .fetch_one(pool)
            .await?;

    let entities: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM entities WHERE embedding IS NOT NULL")
            .fetch_one(pool)
            .await?;

    let communities: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM communities WHERE embedding IS NOT NULL")
            .fetch_one(pool)
            .await?;

    Ok(EmbeddingStats {
        episodes_with_embeddings: episodes.0,
        rules_with_embeddings: rules.0,
        entities_with_embeddings: entities.0,
        communities_with_embeddings: communities.0,
    })
}
