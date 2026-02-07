# Fermi Migration Scripts

This directory contains migration and maintenance scripts for Fermi ADM.

## migrate_embedding_dimensions.rs

Migrates the PostgreSQL schema to support different embedding dimensions.

### Purpose

The default Fermi schema uses 1024-dimensional vectors for all embeddings. If you want to use embedding models with different native dimensions (e.g., 1536d or 3072d), you need to migrate the schema.

### When to Use

- Switching from 1024d to 1536d (e.g., OpenAI text-embedding-3-small native, Anthropic voyage-large-2)
- Switching from 1024d to 3072d (e.g., OpenAI text-embedding-3-large native)
- Any other dimension change

### Safety

⚠️ **WARNING**: This is a DESTRUCTIVE operation!

- All existing embeddings will be deleted
- You must re-embed all data after migration
- Always backup your database first

### Usage

```bash
# 1. Backup your database
pg_dump -h $DB_HOST -U $DB_USER -d fermi > backup_$(date +%Y%m%d).sql

# 2. Dry run to see what will happen
cargo run --bin migrate-embedding-dimensions -- \
  --database-url $DATABASE_URL \
  --old-dimensions 1024 \
  --new-dimensions 1536 \
  --dry-run

# 3. Execute the migration (requires --confirm flag)
cargo run --bin migrate-embedding-dimensions -- \
  --database-url $DATABASE_URL \
  --old-dimensions 1024 \
  --new-dimensions 1536 \
  --confirm

# 4. Re-embed all data with new dimensions
fermi-consolidate \
  --agent-id <agent-id> \
  --embedding-provider openai \
  --embedding-model text-embedding-3-small \
  --embedding-dimensions 1536 \
  --openai-api-key $OPENAI_API_KEY \
  --anthropic-api-key $ANTHROPIC_API_KEY
```

### What It Does

1. **Validates** current schema dimensions match `--old-dimensions`
2. **Counts** existing embeddings to show migration scope
3. **Drops** vector indexes (required before altering vector columns)
4. **Alters** all embedding columns to new dimensions:
   - `episodes.embedding`
   - `semantic_rules.embedding`
   - `entities.embedding`
   - `communities.embedding`
5. **Clears** all embeddings (set to NULL)
6. **Recreates** vector indexes with new dimensions

### Example Output

```
🔄 Embedding Dimension Migration Tool
=====================================

Database: postgresql://localhost/fermi
Migration: 1024d → 1536d

📡 Connecting to database...
✅ Connected!

🔍 Checking current schema...
✅ Schema has 1024d vectors as expected

📊 Analyzing existing data...
   Episodes with embeddings: 1250
   Rules with embeddings: 45
   Entities with embeddings: 120
   Communities with embeddings: 8
   Total embeddings to migrate: 1423

⚠️  WARNING: This will DELETE all existing embeddings!
⚠️  You will need to regenerate all embeddings after migration.

Proceeding with migration...

📦 Step 1: Database backup
   ⚠️  Did you backup your database?

🔧 Step 2: Altering table schemas...
   Dropping vector indexes...
   ✅ Indexes dropped
   Altering episodes.embedding...
   Altering semantic_rules.embedding...
   Altering entities.embedding...
   Altering communities.embedding...
   ✅ Schema altered to 1536d

🗑️  Step 3: Clearing old embeddings...
   ✅ Embeddings cleared

🔨 Step 4: Recreating vector indexes...
   ✅ Indexes recreated

✅ Migration complete!

📋 Next steps:
1. Update your agent configurations to use new embedding dimensions
2. Re-embed all episodes using your new embedding model
3. Run consolidation with new dimensions
```

### Troubleshooting

#### Error: "Schema dimension mismatch"

```
Schema dimension mismatch!
You specified --old-dimensions 1024, but database has 1536d vectors.
```

**Solution**: Check your current schema dimensions and specify the correct `--old-dimensions`.

Query to check current dimensions:
```sql
SELECT atttypmod - 4 as dims 
FROM pg_attribute 
WHERE attrelid = 'episodes'::regclass 
AND attname = 'embedding';
```

#### Error: "Safety check failed"

```
⚠️  SAFETY CHECK FAILED
This is a DESTRUCTIVE operation...
```

**Solution**: This is intentional. Add `--confirm` flag to proceed, or use `--dry-run` to preview changes.

#### Error: "Index already exists"

If the script fails partway through, you may need to manually clean up:

```sql
-- Drop any partially created indexes
DROP INDEX IF EXISTS idx_episodes_embedding;
DROP INDEX IF EXISTS idx_semantic_rules_embedding;
DROP INDEX IF EXISTS idx_entities_embedding;
DROP INDEX IF EXISTS idx_communities_embedding;

-- Then re-run the migration script
```

### See Also

- [Embedding Migration Guide](../docs/guides/EMBEDDING_MIGRATION.md) - Complete migration documentation
- [Agent Cards - Embedding Configuration](../docs/api/agent-cards.md#embedding-configuration) - Choosing embedding providers
- [Design Checklist - Step 5](../agents/templates/DESIGN_CHECKLIST.md#step-5-embedding-configuration) - Planning embedding configuration
