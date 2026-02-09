//! Fork logic — copy an agent's contract (and optionally ontology/embeddings).

use super::types::{AgentLifecycleStatus, ForkPricing};
use crate::gas::GasFees;
use fermi_auth::{credit_charge, credit_grant, get_or_create_wallet};
use sqlx::PgPool;
use uuid::Uuid;

/// Fork a published agent. Returns the new agent's ID.
pub async fn fork_agent(
    pool: &PgPool,
    source_id: Uuid,
    forker_id: &str,
    include_ontology: bool,
    include_embeddings: bool,
    gas_fees: &GasFees,
) -> Result<ForkResult, String> {
    // 1. Load source agent
    let source = sqlx::query_as::<_, SourceAgent>(
        "SELECT agent_id, agent_name, owner_id, status, description, system_prompt, \
         agent_type, model, temperature, executor_type, tags, fork_pricing, fork_count, \
         version, visibility, llm_provider, embedding_provider, embedding_model, embedding_dimension \
         FROM agents WHERE agent_id = $1"
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| "Source agent not found".to_string())?;

    // 2. Must be published
    if source.status != "published" {
        return Err("Can only fork published agents".into());
    }

    // 3. Parse fork pricing
    let pricing: ForkPricing = source
        .fork_pricing
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // 4. Calculate total cost
    let mut total_cost = gas_fees.fork_base + pricing.base_price;
    if include_ontology {
        total_cost += pricing.ontology_price.unwrap_or(0);
    }
    if include_embeddings {
        total_cost += pricing.embedding_price.unwrap_or(0);
    }

    // 5. Charge forker
    let forker_wallet = get_or_create_wallet(pool, "user", forker_id)
        .await
        .map_err(|e| format!("Wallet error: {}", e))?;

    if total_cost > 0 {
        credit_charge(
            pool,
            forker_wallet.wallet_id,
            total_cost,
            "fork_fee",
            &format!(
                "Fork {} (base:{} + author:{})",
                source.agent_name,
                gas_fees.fork_base,
                total_cost - gas_fees.fork_base
            ),
            Some(&source_id.to_string()),
        )
        .await
        .map_err(|e| format!("Insufficient credits: {}", e))?;
    }

    // 6. Pay royalty to source author (author's portion only, not platform gas)
    let author_royalty = total_cost - gas_fees.fork_base;
    if author_royalty > 0 {
        if let Some(ref owner_id) = source.owner_id {
            let author_wallet = get_or_create_wallet(pool, "user", owner_id)
                .await
                .map_err(|e| format!("Author wallet error: {}", e))?;
            credit_grant(
                pool,
                author_wallet.wallet_id,
                author_royalty,
                &format!(
                    "Fork royalty: {} forked by {}",
                    source.agent_name, forker_id
                ),
            )
            .await
            .map_err(|e| format!("Royalty grant error: {}", e))?;
        }
    }

    // 7. Create new agent (fork)
    let new_id = Uuid::new_v4();
    let fork_name = format!("{}_fork_{}", source.agent_name, source.fork_count + 1);
    let fork_desc = source.description.as_deref().unwrap_or("").to_string();
    let fork_prompt = source.system_prompt.clone();
    let tags = source.tags.clone();

    sqlx::query(
        "INSERT INTO agents (agent_id, agent_name, owner_id, description, system_prompt, \
         agent_type, model, temperature, executor_type, tags, status, visibility, tier, \
         forked_from, version, llm_provider, embedding_provider, embedding_model, embedding_dimension) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'draft', 'private', 'community', \
         $11, '1.0.0', $12, $13, $14, $15)"
    )
    .bind(new_id)
    .bind(&fork_name)
    .bind(forker_id)
    .bind(&fork_desc)
    .bind(&fork_prompt)
    .bind(&source.agent_type)
    .bind(&source.model)
    .bind(source.temperature)
    .bind(&source.executor_type)
    .bind(&tags)
    .bind(source_id)
    .bind(&source.llm_provider)
    .bind(&source.embedding_provider)
    .bind(&source.embedding_model)
    .bind(source.embedding_dimension)
    .execute(pool)
    .await
    .map_err(|e| format!("Create fork error: {}", e))?;

    // 8. Increment source fork_count
    sqlx::query("UPDATE agents SET fork_count = fork_count + 1 WHERE agent_id = $1")
        .bind(source_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Fork count error: {}", e))?;

    // 9. Optionally copy ontology (entities, rules, facts)
    if include_ontology {
        // Copy entities
        sqlx::query(
            "INSERT INTO entities (id, agent_id, name, entity_type, description, confidence, created_at) \
             SELECT gen_random_uuid(), $2, name, entity_type, description, confidence, NOW() \
             FROM entities WHERE agent_id = (SELECT agent_name FROM agents WHERE agent_id = $1)"
        )
        .bind(source_id)
        .bind(&fork_name)
        .execute(pool)
        .await
        .ok();

        // Copy rules
        sqlx::query(
            "INSERT INTO rules (id, agent_id, rule_text, confidence, source_episodes, created_at) \
             SELECT gen_random_uuid(), $2, rule_text, confidence, source_episodes, NOW() \
             FROM rules WHERE agent_id = (SELECT agent_name FROM agents WHERE agent_id = $1)",
        )
        .bind(source_id)
        .bind(&fork_name)
        .execute(pool)
        .await
        .ok();

        // Copy facts
        sqlx::query(
            "INSERT INTO facts (id, agent_id, subject_entity, predicate, object_entity, confidence, created_at) \
             SELECT gen_random_uuid(), $2, subject_entity, predicate, object_entity, confidence, NOW() \
             FROM facts WHERE agent_id = (SELECT agent_name FROM agents WHERE agent_id = $1)"
        )
        .bind(source_id)
        .bind(&fork_name)
        .execute(pool)
        .await
        .ok();
    }

    // 10. Optionally copy embeddings (episodes with embeddings)
    if include_embeddings {
        sqlx::query(
            "INSERT INTO episodes (id, agent_id, content, embedding, role, source, created_at) \
             SELECT gen_random_uuid(), $2, content, embedding, role, 'fork', NOW() \
             FROM episodes WHERE agent_id = (SELECT agent_name FROM agents WHERE agent_id = $1) \
             AND embedding IS NOT NULL",
        )
        .bind(source_id)
        .bind(&fork_name)
        .execute(pool)
        .await
        .ok();
    }

    Ok(ForkResult {
        agent_id: new_id,
        agent_name: fork_name,
        total_cost,
        author_royalty,
    })
}

/// Result of a fork operation
#[derive(Debug, serde::Serialize)]
pub struct ForkResult {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub total_cost: i32,
    pub author_royalty: i32,
}

/// Minimal agent data needed for fork source
#[derive(sqlx::FromRow)]
struct SourceAgent {
    agent_id: Uuid,
    agent_name: String,
    owner_id: Option<String>,
    status: String,
    description: Option<String>,
    system_prompt: Option<String>,
    agent_type: String,
    model: String,
    temperature: f64,
    executor_type: String,
    tags: Option<Vec<String>>,
    fork_pricing: Option<serde_json::Value>,
    fork_count: i32,
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    visibility: String,
    llm_provider: Option<String>,
    embedding_provider: Option<String>,
    embedding_model: Option<String>,
    embedding_dimension: Option<i32>,
}
