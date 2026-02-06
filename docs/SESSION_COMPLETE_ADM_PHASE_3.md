# ADM Phase 3 Complete: Semantic Memory

**Date**: 2026-02-06  
**Status**: ✅ Complete  
**Tests**: 15 passing (2 new in Phase 3)

## Overview

Phase 3 implements the semantic memory layer of ADM, enabling agents to store and query consolidated knowledge extracted from episodic memory. This includes semantic rules (learned patterns), entities (knowledge graph nodes), and facts (relationships between entities) with full bi-temporal tracking.

## What Was Built

### 1. Semantic Rule Storage (`store.rs`)

**SemanticRule Operations**:
- `store_semantic_rule(rule)` - Store new semantic rule with embeddings
- `get_semantic_rule(rule_id)` - Retrieve rule by ID
- `get_agent_semantic_rules(agent_id)` - Get all active rules for an agent
- `update_rule_verification(rule_id, status, method)` - Update verification status
- `deactivate_rule(rule_id)` - Soft delete (set is_active = false)

**Features**:
- Confidence scoring (0.0-1.0)
- Verification status tracking (pending, verified, rejected)
- Source episode cluster linkage
- Episode count statistics
- Optional embeddings for semantic similarity search

### 2. Entity Storage (Knowledge Graph Nodes) (`store.rs`)

**Entity Operations**:
- `store_entity(entity)` - Store new entity with bi-temporal tracking
- `get_entity(entity_id)` - Retrieve active entity by ID
- `get_agent_entities(agent_id)` - Get all active entities for an agent
- `invalidate_entity(entity_id)` - Soft delete with t_invalid timestamp

**Bi-Temporal Tracking**:
```rust
pub struct Entity {
    pub t_valid: DateTime<Utc>,      // When entity became valid
    pub t_invalid: Option<DateTime<Utc>>,  // When entity became invalid
    // ...
}
```

**Features**:
- Entity types (Company, Market, Product, etc.)
- Optional summary text
- Source episode tracing
- Extraction confidence scoring
- Optional embeddings

### 3. Fact Storage (Knowledge Graph Edges) (`store.rs`)

**Fact Operations**:
- `store_fact(fact)` - Store relationship between entities
- `get_fact(fact_id)` - Retrieve active fact by ID
- `get_agent_facts(agent_id)` - Get all active facts for an agent
- `get_entity_facts(entity_id)` - Get all facts involving a specific entity
- `invalidate_fact(fact_id)` - Soft delete with t_invalid timestamp

**Relationship Cardinality** (Mermaid ER notation):
```rust
pub enum Cardinality {
    OneToOne,   // ||--||
    OneToMany,  // ||--o{
    ManyToOne,  // }o--||
    ManyToMany, // }o--o{
}
```

**Features**:
- Named relation types (operates_in, competes_with, etc.)
- Confidence scoring
- Optional reasoning/justification
- Source episode tracing
- Bi-temporal validity tracking

### 4. Type System Enhancements (`types.rs`)

**VerificationStatus** - String conversion support:
```rust
impl std::str::FromStr for VerificationStatus {
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(VerificationStatus::Pending),
            "verified" => Ok(VerificationStatus::Verified),
            "rejected" => Ok(VerificationStatus::Rejected),
            _ => Err(format!("Invalid verification status: {}", s)),
        }
    }
}
```

**Cardinality** - Mermaid notation parsing:
```rust
impl std::str::FromStr for Cardinality {
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "||--||" => Ok(Cardinality::OneToOne),
            "||--o{" => Ok(Cardinality::OneToMany),
            "}o--||" => Ok(Cardinality::ManyToOne),
            "}o--o{" => Ok(Cardinality::ManyToMany),
            _ => Err(format!("Invalid cardinality: {}", s)),
        }
    }
}
```

## Tests Added

1. **`test_semantic_rule_lifecycle`** - Full CRUD for semantic rules
   - Store rule with confidence and episode cluster
   - Retrieve and verify fields
   - Update verification status
   - Query agent rules
   - Deactivate rule

2. **`test_entity_and_fact_storage`** - Knowledge graph operations
   - Create two entities (AMD, Datacenter)
   - Store and retrieve entities
   - Create fact (relationship) between entities
   - Query facts by entity
   - Invalidate fact and entity
   - Verify bi-temporal tracking

## Test Results

```
test clustering::tests::test_cosine_distance ... ok
test clustering::tests::test_dbscan_clustering ... ok
test embeddings::tests::test_mock_batch_embeddings ... ok
test embeddings::tests::test_mock_embeddings ... ok
test locking::tests::test_cleanup_expired_locks ... ok
test locking::tests::test_lock_acquire_and_release ... ok
test locking::tests::test_lock_expiry ... ok
test locking::tests::test_lock_prevents_concurrent_access ... ok
test store::tests::test_consolidation_job_lifecycle ... ok
test store::tests::test_database_connection ... ok
test store::tests::test_entity_and_fact_storage ... ok ✨ NEW
test store::tests::test_mark_episodes_consolidated ... ok
test store::tests::test_semantic_rule_lifecycle ... ok ✨ NEW
test store::tests::test_store_and_retrieve_episode ... ok
test store::tests::test_vector_similarity_search ... ok

test result: ok. 15 passed; 0 failed
```

## Database Schema Integration

All features use existing PostgreSQL schema:

### semantic_rules table
- Stores consolidated patterns learned from episode clusters
- Tracks verification status and method
- Links to source episode clusters via UUID array
- Supports embeddings for similarity search

### entities table
- Knowledge graph nodes with bi-temporal validity
- Soft delete via t_invalid timestamp
- Links to source episodes for traceability
- Type categorization (Company, Market, Product, etc.)

### facts table
- Knowledge graph edges with named relations
- Cardinality stored as Mermaid notation strings
- Bi-temporal tracking like entities
- Confidence scoring for relationship strength

## Key Design Patterns

### 1. Bi-Temporal Validity Tracking

Entities and facts use bi-temporal tracking for knowledge evolution:

```sql
-- Query only currently valid entities
WHERE t_invalid IS NULL OR t_invalid > NOW()

-- Invalidate (soft delete) entity
UPDATE entities SET t_invalid = NOW() WHERE entity_id = $1
```

This allows:
- Historical queries ("what did the agent know on date X?")
- Knowledge evolution tracking
- Non-destructive updates

### 2. Source Episode Tracing

All semantic memory (rules, entities, facts) links back to source episodes:

```rust
pub source_episodes: Vec<Uuid>,  // Episode IDs that support this knowledge
```

Benefits:
- Traceability and explainability
- Confidence calibration based on episode count
- Ability to invalidate derived knowledge when episodes are discarded

### 3. Verification Workflow

Semantic rules support a verification workflow:

```
Pending → Verified/Rejected
```

Verification methods tracked:
- "unit_test" - Automated test validation
- "llm_generated_test" - LLM-created verification test
- "manual" - Human review
- "cross_validation" - Tested against held-out episodes

### 4. Confidence Scoring

All semantic memory has confidence scores (0.0-1.0):
- **Rules**: Based on cluster cohesion and episode count
- **Entities**: Extraction confidence from NER/LLM
- **Facts**: Relationship confidence from text analysis

Used for:
- Ranking query results
- Filtering low-confidence knowledge
- Calibrating agent decisions

## Semantic Memory Query Patterns

### Pattern 1: Get Agent's Knowledge Base
```rust
let rules = store.get_agent_semantic_rules(agent_id).await?;
let entities = store.get_agent_entities(agent_id).await?;
let facts = store.get_agent_facts(agent_id).await?;
```

### Pattern 2: Entity-Centric Knowledge Graph
```rust
let entity = store.get_entity(entity_id).await?;
let related_facts = store.get_entity_facts(entity_id).await?;
// Follow edges to get related entities
```

### Pattern 3: Rule-Based Reasoning
```rust
let rules = store.get_agent_semantic_rules(agent_id).await?;
let relevant_rules = rules.into_iter()
    .filter(|r| r.confidence_score > 0.8)
    .filter(|r| matches!(r.verification_status, VerificationStatus::Verified))
    .collect();
```

## Files Modified

- `fermi-memory/src/store.rs` - Added 246 lines (semantic memory operations)
- `fermi-memory/src/types.rs` - Added FromStr implementations for VerificationStatus and Cardinality
- Tests: Added 179 lines (2 comprehensive test functions)

## Performance Characteristics

- **Semantic rule queries**: Sorted by confidence_score DESC
- **Entity queries**: Filtered by bi-temporal validity, sorted by name
- **Fact queries**: Filtered by bi-temporal validity, sorted by confidence DESC
- **Entity fact lookup**: Efficient index on (source_entity_id, target_entity_id)

## Integration with Consolidation Workflow

Phase 3 completes the consolidation data model:

1. **Episodes** (Phase 0) - Raw experiences stored
2. **Clustering** (Phase 1) - Failure patterns identified
3. **Locking & Jobs** (Phase 2) - Safe concurrent consolidation
4. **Semantic Memory** (Phase 3) - Consolidated knowledge stored

**Next steps** will implement the actual consolidation logic:
- LLM-based rule extraction from episode clusters
- Entity/fact extraction from episode contexts
- Verification test generation
- Ontology snapshot creation

## Usage Example

```rust
// Store semantic rule learned from consolidation
let rule = SemanticRule {
    rule_id: Uuid::new_v4(),
    agent_id: my_agent_id,
    rule_content: "When AMD releases datacenter products, stock price increases".to_string(),
    confidence_score: 0.85,
    verification_status: VerificationStatus::Pending,
    source_episode_cluster: cluster_episode_ids,
    episode_count: 3,
    is_active: true,
    // ...
};
store.store_semantic_rule(rule).await?;

// Store entities
let amd = Entity {
    entity_name: "AMD".to_string(),
    entity_type: "Company".to_string(),
    t_valid: Utc::now(),
    // ...
};
store.store_entity(amd).await?;

// Create relationship
let fact = Fact {
    source_entity_id: amd_id,
    target_entity_id: datacenter_market_id,
    relation_type: "operates_in".to_string(),
    relation_cardinality: Cardinality::ManyToMany,
    confidence: 0.92,
    // ...
};
store.store_fact(fact).await?;
```

## What's Not Included (Future Work)

Phase 3 provides storage and retrieval, but **not yet implemented**:

1. **LLM-based extraction** - Actually generating rules from episodes
2. **Verification tests** - Automated test generation and execution
3. **Ontology snapshots** - Versioned Mermaid ER diagrams
4. **Git integration** - Committing ontologies to version control
5. **Rule similarity search** - Vector-based rule deduplication
6. **Knowledge graph queries** - Path finding, subgraph matching

These will be implemented in subsequent phases.

---

**Phase 3 Complete** - Semantic memory storage infrastructure ready for consolidation workflow
