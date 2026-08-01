//! Fork logic — copy an agent's contract (and optionally ontology/embeddings).

use super::types::ForkPricing;
use crate::gas::GasFees;
use fermi_auth::{credit_charge, credit_deposit_typed, get_or_create_wallet};
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
    // 1. Load source agent.
    //
    // v0.10.16: `agents.owner_id` has never existed — the owner
    // column is `agents.user_id` (mig-006). The prior SELECT
    // referenced `owner_id` directly, so this query 500'd on every
    // fork attempt with `column "owner_id" does not exist`. Alias
    // `user_id AS owner_id` keeps the Rust struct field name stable
    // so downstream royalty-payment logic on `source.owner_id`
    // doesn't need to change. Sibling fix: eval_brier.rs (v0.10.15).
    let source = sqlx::query_as::<_, SourceAgent>(
        "SELECT agent_id, agent_name, user_id AS owner_id, status, description, system_prompt, \
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
            credit_deposit_typed(
                pool,
                author_wallet.wallet_id,
                author_royalty,
                "fork_royalty",
                &format!(
                    "Fork royalty: {} forked by {}",
                    source.agent_name, forker_id
                ),
            )
            .await
            .map_err(|e| format!("Royalty deposit error: {}", e))?;
        }
    }

    // 7. Create new agent (fork).
    //
    // v0.10.16: derive the fork name and validate its shape against
    // the platform-wide slug rule. If the source has a legacy name
    // (contains `-` or `/` — predates d0f94e8, 2026-05-23), the
    // derived name inherits the bad shape and would land another
    // un-routable agent in the DB. Refuse with a detailed 400 so
    // the forker knows exactly why and what to ask the source
    // owner to do first. See the deferred rename-migration audit
    // in RELEASE_NOTES_v0.10.15.md.
    let new_id = Uuid::new_v4();
    let fork_name = format!("{}_fork_{}", source.agent_name, source.fork_count + 1);
    if let Err(msg) = crate::slug::validate(&fork_name) {
        return Err(format!(
            "Cannot fork `{source_name}`: the derived fork name `{fork_name}` fails the \
             platform slug rule ({msg}). This happens when the source agent has a legacy \
             name that predates the URL-safety rule enforced since 2026-05-23 (commit \
             d0f94e8). Legacy names contain characters (`-` or `/`) that would produce \
             un-routable URLs on the fork. Ask an admin to rename `{source_name}` to a \
             snake_case name first, then retry the fork.",
            source_name = source.agent_name,
            fork_name = fork_name,
            msg = msg,
        ));
    }
    let fork_desc = source.description.as_deref().unwrap_or("").to_string();
    let fork_prompt = source.system_prompt.clone();
    let tags = source.tags.clone();

    // v0.10.16: `agents.owner_id` → `agents.user_id`, same fix as
    // the SELECT above. Was: INSERT INTO agents (..., owner_id, ...).
    sqlx::query(
        "INSERT INTO agents (agent_id, agent_name, user_id, description, system_prompt, \
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

    // 9. Optionally copy ontology (entities, semantic_rules, facts).
    //
    // Spec 22 §1.7c fix: the previous SQL referenced a non-existent table
    // `rules` and used wrong column names on `episodes`. This version uses
    // the actual schema (see migrations/010) and preserves embedding-provenance
    // columns so forked vectors carry their original model identity forward.
    if include_ontology {
        // Copy entities (including embedding + Spec 22 provenance columns)
        sqlx::query(
            "INSERT INTO entities (
                entity_id, agent_id, entity_name, entity_type, summary,
                t_valid, t_invalid, source_episodes, extraction_confidence,
                embedding, properties,
                embedding_model_id, embedding_model_version, embedding_dim,
                source_text, source_ref, provenance_trusted
            )
            SELECT gen_random_uuid(), $2, entity_name, entity_type, summary,
                   t_valid, t_invalid, source_episodes, extraction_confidence,
                   embedding, properties,
                   embedding_model_id, embedding_model_version, embedding_dim,
                   source_text,
                   COALESCE(source_ref, '{}'::jsonb)
                       || jsonb_build_object('forked_from', $1::text),
                   provenance_trusted
              FROM entities
             WHERE agent_id = $1 AND t_invalid IS NULL",
        )
        .bind(source_id)
        .bind(new_id)
        .execute(pool)
        .await
        .ok();

        // Copy semantic_rules (real table name)
        sqlx::query(
            "INSERT INTO semantic_rules (
                rule_id, agent_id, rule_content, rule_description, confidence_score,
                verification_status, verification_method, source_episode_cluster,
                episode_count, embedding, is_active,
                embedding_model_id, embedding_model_version, embedding_dim,
                source_text, source_ref, provenance_trusted
            )
            SELECT gen_random_uuid(), $2, rule_content, rule_description, confidence_score,
                   verification_status, verification_method, source_episode_cluster,
                   episode_count, embedding, is_active,
                   embedding_model_id, embedding_model_version, embedding_dim,
                   source_text,
                   COALESCE(source_ref, '{}'::jsonb)
                       || jsonb_build_object('forked_from', $1::text),
                   provenance_trusted
              FROM semantic_rules
             WHERE agent_id = $1 AND is_active = true",
        )
        .bind(source_id)
        .bind(new_id)
        .execute(pool)
        .await
        .ok();

        // Copy facts (no embedding column on facts; see migration 010)
        sqlx::query(
            "INSERT INTO facts (
                fact_id, agent_id, subject_entity_id, predicate, object_entity_id,
                confidence_score, source_episodes,
                t_valid, t_invalid, t_created
            )
            SELECT gen_random_uuid(), $2, subject_entity_id, predicate, object_entity_id,
                   confidence_score, source_episodes,
                   t_valid, t_invalid, NOW()
              FROM facts
             WHERE agent_id = $1 AND t_invalid IS NULL",
        )
        .bind(source_id)
        .bind(new_id)
        .execute(pool)
        .await
        .ok();
    }

    // 10. Optionally copy embeddings (episodes with embeddings).
    //
    // Spec 22 §1.7c fix: corrected column names and preserved provenance.
    // Forked episodes carry the original model identity forward; source_ref is
    // annotated with `{"forked_from": <source_agent_id>}` so the audit trail
    // is preserved.
    if include_embeddings {
        sqlx::query(
            "INSERT INTO episodes (
                episode_id, agent_id, timestamp_ref, query, context,
                execution_status, error_details, execution_time_ms,
                tokens_used, cost_usd, embedding, consolidated, tags,
                embedding_model_id, embedding_model_version, embedding_dim,
                source_text, source_ref, provenance_trusted,
                created_at
            )
            SELECT gen_random_uuid(), $2, timestamp_ref, query, context,
                   execution_status, error_details, execution_time_ms,
                   tokens_used, cost_usd, embedding, consolidated, tags,
                   embedding_model_id, embedding_model_version, embedding_dim,
                   source_text,
                   COALESCE(source_ref, '{}'::jsonb)
                       || jsonb_build_object('forked_from', $1::text),
                   provenance_trusted,
                   NOW()
              FROM episodes
             WHERE agent_id = $1 AND embedding IS NOT NULL",
        )
        .bind(source_id)
        .bind(new_id)
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
