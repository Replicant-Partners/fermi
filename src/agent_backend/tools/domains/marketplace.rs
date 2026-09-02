// src/agent_backend/tools/domains/marketplace.rs
//
// Phase 4 domain migration: Marketplace tools.
//
// Four tools, all requires_workspace: true:
//   get_shopping_profile
//   update_shopping_profile
//   list_marketplace
//   create_listing
//
// Each is a zero-size struct implementing PlatformTool. execute() calls
// a private function defined in this module.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Marketplace-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(GetShoppingProfile),
        Arc::new(UpdateShoppingProfile),
        Arc::new(ListMarketplace),
        Arc::new(CreateListing),
    ]
}

// ─── get_shopping_profile ─────────────────────────────────────────────────────

struct GetShoppingProfile;

#[async_trait]
impl PlatformTool for GetShoppingProfile {
    fn name(&self) -> &'static str {
        "get_shopping_profile"
    }

    fn description(&self) -> &'static str {
        "Retrieve the current user's shopping preference profile for a given agent. Returns metadata, category tags, brand affinities, price sensitivity, and quality bias. Never exposes raw embeddings."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "profile_name": {
                    "type": "string",
                    "description": "Name of the shopping profile (e.g. 'electronics', 'fitness'). Default: 'default'",
                    "default": "default"
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Marketplace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_get_shopping_profile(input, ctx).await
    }
}

// ─── update_shopping_profile ──────────────────────────────────────────────────

struct UpdateShoppingProfile;

#[async_trait]
impl PlatformTool for UpdateShoppingProfile {
    fn name(&self) -> &'static str {
        "update_shopping_profile"
    }

    fn description(&self) -> &'static str {
        "Recompute the composite shopping embedding from recent episodes and update profile metadata (brand affinities, price sensitivity, quality bias, category tags). The embedding is computed server-side as a weighted centroid of episode embeddings."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "profile_name": {
                    "type": "string",
                    "description": "Name of the shopping profile to update. Default: 'default'",
                    "default": "default"
                },
                "category_tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Category tags for the profile (e.g. ['electronics', 'espresso', 'kitchen'])"
                },
                "price_sensitivity": {
                    "type": "number",
                    "description": "Price sensitivity score 0.0 (price insensitive) to 1.0 (very price sensitive)"
                },
                "quality_bias": {
                    "type": "number",
                    "description": "Quality bias score 0.0 (value-focused) to 1.0 (premium-focused)"
                },
                "brand_affinities": {
                    "type": "object",
                    "description": "Brand affinity scores, e.g. {\"nike\": 0.85, \"breville\": 0.72}"
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Marketplace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_update_shopping_profile(input, ctx).await
    }
}

// ─── list_marketplace ─────────────────────────────────────────────────────────

struct ListMarketplace;

#[async_trait]
impl PlatformTool for ListMarketplace {
    fn name(&self) -> &'static str {
        "list_marketplace"
    }

    fn description(&self) -> &'static str {
        "Browse active marketplace listings where consumers have listed their shopping profiles for advertiser queries. Filter by category. Returns listing metadata and pricing — never raw embeddings."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Comma-separated category filter (e.g. 'electronics,kitchen')"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum listings to return (default: 20)",
                    "default": 20
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Marketplace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_list_marketplace(input, ctx).await
    }
}

// ─── create_listing ───────────────────────────────────────────────────────────

struct CreateListing;

#[async_trait]
impl PlatformTool for CreateListing {
    fn name(&self) -> &'static str {
        "create_listing"
    }

    fn description(&self) -> &'static str {
        "List a shopping profile on the embedding marketplace so advertisers can run similarity queries against it. The consumer sets the price per query and can delist at any time. Costs a one-time listing fee."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "profile_name": {
                    "type": "string",
                    "description": "Name of the shopping profile to list. Default: 'default'",
                    "default": "default"
                },
                "price_credits": {
                    "type": "integer",
                    "description": "Credits to charge per advertiser query (min 1)"
                },
                "max_queries_per_buyer": {
                    "type": "integer",
                    "description": "Optional cap on queries per buyer (privacy control)"
                },
                "category_tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Category tags for marketplace discovery"
                },
                "description": {
                    "type": "string",
                    "description": "Public description of this listing"
                }
            },
            "required": ["price_credits"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Marketplace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_create_listing(input, ctx).await
    }
}

// ─── Private execute functions ────────────────────────────────────────────────

async fn execute_get_shopping_profile(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for get_shopping_profile")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for get_shopping_profile")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let profile = ctx
        .memory_store
        .get_shopping_profile(user_id, agent_id, profile_name)
        .await
        .map_err(|e| format!("Profile lookup failed: {}", e))?;

    match profile {
        Some(p) => {
            let result = json!({
                "profile_id": p.profile_id,
                "profile_name": p.profile_name,
                "embedding_version": p.embedding_version,
                "episode_count": p.episode_count,
                "category_tags": p.category_tags,
                "price_sensitivity": p.price_sensitivity,
                "quality_bias": p.quality_bias,
                "brand_affinities": p.brand_affinities,
                "is_listed": p.is_listed,
                "updated_at": p.updated_at.to_rfc3339(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        None => Ok(json!({
            "status": "not_found",
            "message": format!("No shopping profile '{}' found. Use update_shopping_profile to create one.", profile_name)
        })
        .to_string()),
    }
}

async fn execute_update_shopping_profile(
    input: &Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for update_shopping_profile")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for update_shopping_profile")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    // Extract metadata from input
    let category_tags: Vec<String> = input
        .get("category_tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let price_sensitivity = input.get("price_sensitivity").and_then(|v| v.as_f64());
    let quality_bias = input.get("quality_bias").and_then(|v| v.as_f64());
    let brand_affinities = input.get("brand_affinities").cloned().unwrap_or(json!({}));

    // Compute composite embedding from episodes (weighted centroid)
    let episodes = ctx
        .memory_store
        .get_all_episodes_with_embeddings(agent_id)
        .await
        .map_err(|e| format!("Episode fetch failed: {}", e))?;

    let now = chrono::Utc::now();
    let mut weighted_sum: Option<Vec<f64>> = None;
    let mut total_weight = 0.0f64;
    let mut episode_count = 0i32;

    for episode in &episodes {
        if let Some(ref emb) = episode.embedding {
            let age_days = (now - episode.timestamp_ref).num_hours() as f64 / 24.0;
            let recency_weight = (-0.1 * age_days).exp();
            let success_weight = match episode.execution_status {
                agent_bestiary_memory::ExecutionStatus::Success => 1.0,
                _ => 0.3,
            };
            let w = recency_weight * success_weight;

            match &mut weighted_sum {
                Some(sum) => {
                    for (i, &val) in emb.iter().enumerate() {
                        if i < sum.len() {
                            sum[i] += w * val as f64;
                        }
                    }
                }
                None => {
                    weighted_sum = Some(emb.iter().map(|&v| w * v as f64).collect());
                }
            }
            total_weight += w;
            episode_count += 1;
        }
    }

    // L2 normalize the composite embedding
    let composite: Option<Vec<f32>> = weighted_sum.map(|sum| {
        let norm: f64 = sum.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm > 1e-10 {
            sum.iter().map(|&v| (v / norm) as f32).collect()
        } else {
            sum.iter().map(|&v| v as f32).collect()
        }
    });

    let episode_ids_for_centroid: Vec<uuid::Uuid> = episodes
        .iter()
        .filter(|e| e.embedding.is_some())
        .map(|e| e.episode_id)
        .collect();

    let profile_id = if let Some(ref composite_vec) = composite {
        // Centroid was computed — record full Spec 22 provenance. The centroid
        // inherits the model identity of the constituent episode embeddings,
        // which all come from `ctx.embedder` (single shared embedder per
        // server).
        let source_ref = json!({
            "kind": "shopping_profile_centroid",
            "member_episode_ids": episode_ids_for_centroid,
            "episode_count": episode_count,
            "total_weight": total_weight,
        });
        ctx.memory_store
            .upsert_shopping_profile_with_provenance(
                user_id,
                agent_id,
                profile_name,
                composite_vec,
                episode_count,
                &category_tags,
                price_sensitivity,
                quality_bias,
                &brand_affinities,
                ctx.embedder.model_id(),
                ctx.embedder.model_version(),
                ctx.embedder.dimension() as i32,
                source_ref,
            )
            .await
            .map_err(|e| format!("Profile upsert failed: {}", e))?
    } else {
        // No episodes had embeddings → no centroid to compute. Fall back to
        // the legacy upsert path; the row is created without an embedding.
        #[allow(deprecated)]
        ctx.memory_store
            .upsert_shopping_profile(
                user_id,
                agent_id,
                profile_name,
                None,
                episode_count,
                &category_tags,
                price_sensitivity,
                quality_bias,
                &brand_affinities,
            )
            .await
            .map_err(|e| format!("Profile upsert failed: {}", e))?
    };

    let result = json!({
        "profile_id": profile_id,
        "profile_name": profile_name,
        "episode_count": episode_count,
        "embedding_computed": composite.is_some(),
        "category_tags": category_tags,
        "price_sensitivity": price_sensitivity,
        "quality_bias": quality_bias,
        "brand_affinities": brand_affinities,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_list_marketplace(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let cat_str = input.get("category").and_then(|v| v.as_str());
    let cat_filter: Option<Vec<String>> =
        cat_str.map(|s| s.split(',').map(|t| t.trim().to_string()).collect());
    let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

    let listings = ctx
        .memory_store
        .get_active_listings(cat_filter.as_deref(), limit)
        .await
        .map_err(|e| format!("Marketplace query failed: {}", e))?;

    let items: Vec<serde_json::Value> = listings
        .iter()
        .map(|l| {
            json!({
                "listing_id": l.listing_id,
                "seller_id": l.seller_id,
                "price_credits": l.price_credits,
                "total_queries": l.total_queries,
                "category_tags": l.category_tags,
                "description": l.description,
            })
        })
        .collect();

    let result = json!({ "listings": items, "count": items.len() });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_create_listing(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for create_listing")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for create_listing")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let price_credits = input
        .get("price_credits")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1) as i32;
    let max_queries = input
        .get("max_queries_per_buyer")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    let category_tags: Vec<String> = input
        .get("category_tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let description = input.get("description").and_then(|v| v.as_str());

    // Look up the profile
    let profile = ctx
        .memory_store
        .get_shopping_profile(user_id, agent_id, profile_name)
        .await
        .map_err(|e| format!("Profile lookup failed: {}", e))?
        .ok_or_else(|| {
            format!(
                "No shopping profile '{}' found. Create one with update_shopping_profile first.",
                profile_name
            )
        })?;

    // Charge listing fee if pool is available
    if let (Some(db), Some(gas)) = (&ctx.db, &ctx.gas_fees) {
        let wallet = fermi_auth::get_or_create_wallet(db, "user", user_id)
            .await
            .map_err(|e| format!("Wallet error: {}", e))?;
        fermi_auth::credit_charge(
            db,
            wallet.wallet_id,
            gas.marketplace_listing_fee,
            "marketplace_listing_fee",
            "Marketplace listing creation",
            Some(&profile.profile_id.to_string()),
        )
        .await
        .map_err(|e| format!("Insufficient credits for listing fee: {}", e))?;
    }

    let listing_id = ctx
        .memory_store
        .create_marketplace_listing(
            profile.profile_id,
            user_id,
            price_credits,
            max_queries,
            &category_tags,
            description,
        )
        .await
        .map_err(|e| format!("Listing creation failed: {}", e))?;

    let result = json!({
        "listing_id": listing_id,
        "profile_id": profile.profile_id,
        "status": "active",
        "price_credits": price_credits,
        "message": format!("Profile '{}' is now listed on the marketplace at {} credits per query.", profile_name, price_credits),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_marketplace() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Marketplace,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_four() {
        assert_eq!(tools().len(), 4);
    }

    #[test]
    fn all_tools_require_workspace() {
        for tool in tools() {
            assert!(
                tool.requires_workspace(),
                "tool `{}` should require workspace",
                tool.name()
            );
        }
    }
}
