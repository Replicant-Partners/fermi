# Embedding Provider Migration Guide

**Version**: 1.0.0  
**Date**: 2026-02-07

---

## Overview

This guide covers migrating Fermi agents from one embedding provider to another. Embeddings are critical to Fermi's Active Dreaming Memory (ADM), so migration requires careful planning and execution.

⚠️ **Warning**: Embedding migration is a significant operation that requires re-embedding all agent memories. Plan for downtime and resource usage.

---

## When to Migrate

Consider migrating embedding providers when:

1. **Cost Optimization**: Switching to a more cost-effective provider
2. **Performance**: Upgrading to a higher quality model
3. **Compliance**: Meeting data residency requirements (e.g., GDPR)
4. **Language Support**: Supporting multilingual content (e.g., switching to Qwen for Chinese)
5. **Technical Requirements**: Changing dimensionality or model capabilities

---

## Migration Impact

### What Changes

✅ **Preserved**:
- Agent configuration
- Episodic memory (raw episodes)
- Semantic memory (rules, entities, facts)
- Ontology structure
- Git history

❌ **Re-embedded**:
- Episode embeddings (all episodes)
- Rule embeddings (semantic memory)
- Entity embeddings (if applicable)
- Vector similarity scores

### Downtime

- **Agent unavailable**: During re-embedding process (minutes to hours depending on data volume)
- **Memory queries**: Similarity search unavailable during migration
- **Consolidation**: Should be paused during migration

### Resource Usage

- **API Costs**: All memories will be re-embedded (estimate: N episodes × embedding cost)
- **Database I/O**: Heavy read/write during migration
- **Compute**: CPU/memory for batch processing

---

## Migration Process

### Phase 1: Pre-Migration Assessment

#### 1. Calculate Scope

```sql
-- Count episodes per agent
SELECT agent_id, COUNT(*) as episode_count
FROM episodes
GROUP BY agent_id;

-- Count semantic rules per agent
SELECT agent_id, COUNT(*) as rule_count
FROM semantic_rules
GROUP BY agent_id;

-- Check current embedding dimensions
SELECT dimension
FROM episodes
LIMIT 1;
```

#### 2. Estimate Costs

```
Total embeddings needed = Episodes + Rules + Entities

Example calculation:
- 10,000 episodes
- 500 rules
- 100 entities
= 10,600 total embeddings

Cost estimates (approximate):
- OpenAI text-embedding-3-small: $0.02 per 1M tokens ≈ $0.02-0.20
- Anthropic Voyage-2: $0.10 per 1M tokens ≈ $0.10-1.00
- Mistral: Similar to OpenAI
- Qwen: Regional pricing varies

Actual cost depends on average text length.
```

#### 3. Backup Database

```bash
# PostgreSQL backup
pg_dump -h $DB_HOST -U $DB_USER -d fermi > fermi_backup_$(date +%Y%m%d).sql

# Or use your cloud provider's backup system
```

### Phase 2: Configuration Update

#### 1. Update Agent Card

```toml
# agents/your-agent/config.toml

[knowledge]
# OLD:
# embeddings_provider = "openai"
# embeddings_model = "text-embedding-3-large"
# dimensions = 3072

# NEW:
embeddings_provider = "anthropic"
embeddings_model = "voyage-2"
dimensions = 1024
```

#### 2. Database Schema Changes (if dimensions change)

If your new embedding model uses different dimensions, you'll need to update the database schema:

```sql
-- Check current dimension
SELECT dimension FROM episodes LIMIT 1;

-- If dimensions match, skip this step
-- If dimensions differ, you'll need to alter the table

-- Option 1: Add new column, migrate, drop old column
ALTER TABLE episodes ADD COLUMN embedding_new vector(1024);
-- (Populate via migration script)
-- ALTER TABLE episodes DROP COLUMN embedding;
-- ALTER TABLE episodes RENAME COLUMN embedding_new TO embedding;

-- Option 2: Recreate table (requires downtime)
-- See migration script below
```

### Phase 3: Re-Embedding

#### Option A: Using Automated Migration Script (Recommended for Dimension Changes)

If you're changing embedding dimensions, use the automated migration script:

```bash
# Dry run first to see what will happen
cargo run --bin migrate-embedding-dimensions -- \
  --database-url $DATABASE_URL \
  --old-dimensions 1024 \
  --new-dimensions 1536 \
  --dry-run

# Execute the migration
cargo run --bin migrate-embedding-dimensions -- \
  --database-url $DATABASE_URL \
  --old-dimensions 1024 \
  --new-dimensions 1536 \
  --confirm
```

This script will:
1. Validate current schema dimensions
2. Drop vector indexes
3. Alter all embedding columns to new dimensions
4. Clear all embeddings (set to NULL)
5. Recreate indexes

See [scripts/README.md](../../scripts/README.md) for detailed documentation.

After running the script, proceed to re-embedding (Option B below).

#### Option B: Manual Re-Embedding Script

Create a migration script `scripts/migrate_embeddings.rs`:

```rust
use anyhow::Result;
use fermi_memory::{AnthropicEmbeddings, EmbeddingGenerator, MemoryStore};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = std::env::var("DATABASE_URL")?;
    let api_key = std::env::var("ANTHROPIC_API_KEY")?;
    let agent_id: Uuid = std::env::var("AGENT_ID")?.parse()?;

    println!("🔄 Starting embedding migration...");
    println!("   Agent: {}", agent_id);
    println!("   New provider: Anthropic Voyage-2");

    let pool = PgPool::connect(&database_url).await?;
    let embedder = AnthropicEmbeddings::new(api_key)
        .with_model("voyage-2".to_string(), 1024);

    // Migrate episodes
    println!("\n📝 Migrating episodes...");
    let episodes = sqlx::query!(
        "SELECT episode_id, observation FROM episodes WHERE agent_id = $1",
        agent_id
    )
    .fetch_all(&pool)
    .await?;

    println!("   Found {} episodes", episodes.len());

    for (i, episode) in episodes.iter().enumerate() {
        let embedding = embedder.generate(&episode.observation).await?;
        
        sqlx::query!(
            "UPDATE episodes SET embedding = $1 WHERE episode_id = $2",
            embedding.as_slice(),
            episode.episode_id
        )
        .execute(&pool)
        .await?;

        if (i + 1) % 100 == 0 {
            println!("   Progress: {}/{} episodes", i + 1, episodes.len());
        }
    }

    println!("✅ Episodes migrated!");

    // Migrate semantic rules
    println!("\n🧠 Migrating semantic rules...");
    let rules = sqlx::query!(
        "SELECT rule_id, rule_text FROM semantic_rules WHERE agent_id = $1",
        agent_id
    )
    .fetch_all(&pool)
    .await?;

    println!("   Found {} rules", rules.len());

    for (i, rule) in rules.iter().enumerate() {
        let embedding = embedder.generate(&rule.rule_text).await?;
        
        sqlx::query!(
            "UPDATE semantic_rules SET embedding = $1 WHERE rule_id = $2",
            embedding.as_slice(),
            rule.rule_id
        )
        .execute(&pool)
        .await?;

        if (i + 1) % 10 == 0 {
            println!("   Progress: {}/{} rules", i + 1, rules.len());
        }
    }

    println!("✅ Rules migrated!");

    // Migrate entities (if they have embeddings)
    println!("\n🏷️  Migrating entities...");
    let entities = sqlx::query!(
        "SELECT entity_id, entity_name, description 
         FROM entities 
         WHERE agent_id = $1",
        agent_id
    )
    .fetch_all(&pool)
    .await?;

    println!("   Found {} entities", entities.len());

    for (i, entity) in entities.iter().enumerate() {
        let text = format!(
            "{}: {}",
            entity.entity_name,
            entity.description.as_deref().unwrap_or("")
        );
        let embedding = embedder.generate(&text).await?;
        
        sqlx::query!(
            "UPDATE entities SET embedding = $1 WHERE entity_id = $2",
            embedding.as_slice(),
            entity.entity_id
        )
        .execute(&pool)
        .await?;

        if (i + 1) % 10 == 0 {
            println!("   Progress: {}/{} entities", i + 1, entities.len());
        }
    }

    println!("✅ Entities migrated!");

    println!("\n🎉 Migration complete!");
    Ok(())
}
```

**Run the migration:**

```bash
# Set environment variables
export DATABASE_URL="postgresql://..."
export ANTHROPIC_API_KEY="sk-..."
export AGENT_ID="uuid-here"

# Run migration
cargo run --bin migrate_embeddings
```

#### Option C: Using fermi-consolidate

If you're changing providers but keeping the same dimensions (1024d), you can just re-run consolidation:

```bash
# 1. Clear semantic memory (optional - forces full reconsolidation)
psql $DATABASE_URL -c "DELETE FROM semantic_rules WHERE agent_id = 'your-agent-id';"
psql $DATABASE_URL -c "DELETE FROM entities WHERE agent_id = 'your-agent-id';"
psql $DATABASE_URL -c "DELETE FROM facts WHERE agent_id = 'your-agent-id';"

# 2. Re-embed episodes (custom script needed - see Option A)

# 3. Run consolidation with new embeddings
fermi-consolidate \
  --agent-id your-agent-id \
  --database-url $DATABASE_URL \
  --embedding-provider anthropic \
  --anthropic-api-key $ANTHROPIC_API_KEY
```

### Phase 4: Verification

#### 1. Verify Embedding Dimensions

```sql
-- Check that all episodes have correct dimensions
SELECT 
  COUNT(*) as total,
  COUNT(CASE WHEN array_length(embedding::float[], 1) = 1024 THEN 1 END) as correct_dims
FROM episodes
WHERE agent_id = 'your-agent-id';
```

#### 2. Test Similarity Search

```sql
-- Test vector similarity search
SELECT episode_id, observation
FROM episodes
WHERE agent_id = 'your-agent-id'
ORDER BY embedding <=> (SELECT embedding FROM episodes WHERE episode_id = 'test-id')
LIMIT 5;
```

#### 3. Run Test Queries

```bash
# Test agent execution with new embeddings
# Verify output quality and relevance
```

### Phase 5: Cleanup

```bash
# Remove backup columns if you added them
psql $DATABASE_URL -c "ALTER TABLE episodes DROP COLUMN IF EXISTS embedding_old;"

# Update documentation
# Mark migration as complete in agent README
```

---

## Dimension Change Migrations

If your new embedding model uses different dimensions (e.g., 3072 → 1024), you need additional steps:

### PostgreSQL Vector Column Update

```sql
-- Option 1: In-place update (CAREFUL - test in staging first)
ALTER TABLE episodes ALTER COLUMN embedding TYPE vector(1024);
ALTER TABLE semantic_rules ALTER COLUMN embedding TYPE vector(1024);
ALTER TABLE entities ALTER COLUMN embedding TYPE vector(1024);

-- Option 2: Safe migration with new column
ALTER TABLE episodes ADD COLUMN embedding_new vector(1024);
-- Populate via migration script
-- Then swap columns

-- Option 3: Recreate table (requires downtime)
CREATE TABLE episodes_new AS 
SELECT episode_id, agent_id, observation, action, reward, 
       created_at, valid_from, valid_to, NULL::vector(1024) as embedding
FROM episodes;
-- Re-embed all episodes
-- Swap tables
```

---

## Rollback Plan

If migration fails:

### 1. Restore from Backup

```bash
# Stop agent
# Restore database
psql $DATABASE_URL < fermi_backup_20260207.sql

# Revert agent config
git checkout HEAD~1 agents/your-agent/config.toml
```

### 2. Keep Old Provider Available

During migration, keep both API keys available:

```bash
export OPENAI_API_KEY="old-key"
export ANTHROPIC_API_KEY="new-key"
```

This allows quick rollback if needed.

---

## Best Practices

### 1. Test in Staging First

Always test migration on a staging environment with production data before migrating production.

### 2. Migrate During Low Traffic

Schedule migration during maintenance windows or low-usage periods.

### 3. Monitor Costs

Set up billing alerts before starting migration to avoid unexpected costs.

### 4. Batch Processing

Process embeddings in batches (e.g., 100 episodes at a time) to handle rate limits and reduce memory usage.

```rust
// Batch processing example
for chunk in episodes.chunks(100) {
    let texts: Vec<String> = chunk.iter().map(|e| e.observation.clone()).collect();
    let embeddings = embedder.generate_batch(&texts).await?;
    
    // Update database
    for (episode, embedding) in chunk.iter().zip(embeddings.iter()) {
        // ... update query
    }
    
    // Rate limiting
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

### 5. Document Migration

Update agent README with:
- Migration date
- Old provider → New provider
- Reason for migration
- Any issues encountered

---

## FAQ

### Q: Can I migrate only some agents?

**A**: Yes, each agent has independent embedding configuration. You can migrate agents one at a time.

### Q: Do I need to re-train anything?

**A**: No. Embeddings are purely for vector search. The LLM used for consolidation is separate from the embedding model.

### Q: Will similarity scores be comparable?

**A**: No. Different embedding models produce different vector spaces. Similarity scores before and after migration are not directly comparable.

### Q: Can I run multiple embedding providers simultaneously?

**A**: Not per agent. Each agent must use a single embedding provider. However, different agents can use different providers.

### Q: What happens to git ontology history?

**A**: Ontology git history is preserved. Migration only affects vector embeddings in the database, not the Mermaid diagrams in git.

### Q: How long does migration take?

**A**: Depends on volume:
- 1,000 episodes: ~5-10 minutes
- 10,000 episodes: ~30-60 minutes  
- 100,000 episodes: ~4-8 hours

Actual time varies by API rate limits and network speed.

---

## Troubleshooting

### Rate Limit Errors

```
Error: API rate limit exceeded
```

**Solution**: Add rate limiting to your migration script:

```rust
use tokio::time::{sleep, Duration};

// Add delay between batches
sleep(Duration::from_millis(200)).await;
```

### Dimension Mismatch

```
Error: expected vector of dimension 1024, got 1536
```

**Solution**: Ensure database schema matches new embedding dimensions. See "Dimension Change Migrations" section above.

### Out of Memory

```
Error: OOM killed
```

**Solution**: Process in smaller batches:

```rust
// Reduce batch size
for chunk in episodes.chunks(50) { // Was 100
    // ...
}
```

### API Authentication Failed

```
Error: Invalid API key
```

**Solution**: Verify environment variables:

```bash
echo $ANTHROPIC_API_KEY  # Should show key
# Re-export if needed
export ANTHROPIC_API_KEY="sk-ant-..."
```

---

## Support

For migration assistance:

1. Check existing issues: `github.com/yourorg/fermi/issues`
2. Review agent examples: `agents/templates/examples/`
3. Contact Fermi team with:
   - Agent ID
   - Migration plan (old → new provider)
   - Data volume (episode count)
   - Any errors encountered

---

**Remember**: Embedding migration is irreversible without backup. Always test in staging first and maintain backups before production migration.
