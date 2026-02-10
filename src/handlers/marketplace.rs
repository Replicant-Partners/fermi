//! Embedding marketplace handlers — consumer-controlled similarity matching.
//!
//! Consumers build shopping profiles via the ADM cycle, then list them
//! on the marketplace. Advertisers run similarity queries against listed
//! profiles, paying credits. Raw embeddings are never exposed.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{credit_charge, credit_deposit, get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppState;
use fermi::gas::charge_gas;

// ─── Request / query types ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct MatchRequest {
    pub product_description: String,
    pub category_filter: Option<Vec<String>>,
    #[serde(default = "default_min_similarity")]
    pub min_similarity: f64,
    #[serde(default = "default_match_limit")]
    pub max_results: i64,
}

fn default_min_similarity() -> f64 {
    0.3
}
fn default_match_limit() -> i64 {
    10
}

#[derive(Deserialize)]
pub struct ListingsQuery {
    pub category: Option<String>,
    #[serde(default = "default_listings_limit")]
    pub limit: i64,
}

fn default_listings_limit() -> i64 {
    50
}

#[derive(Deserialize)]
pub struct CreateListingRequest {
    pub profile_id: Uuid,
    pub price_credits: i32,
    pub max_queries_per_buyer: Option<i32>,
    pub category_tags: Option<Vec<String>>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateListingRequest {
    pub status: Option<String>,
    pub price_credits: Option<i32>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_history_limit")]
    pub limit: i64,
}

fn default_history_limit() -> i64 {
    50
}

// ─── POST /api/marketplace/match ───────────────────────────────────
//
// Core endpoint: generate embedding from product description, run
// pgvector cosine similarity, charge buyer, pay seller, return scores.

pub async fn marketplace_match_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<MatchRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let buyer_id = principal.user_id();

    // 1. Generate embedding from product description
    let product_embedding = state
        .embedder
        .generate(&req.product_description)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Embedding generation failed: {}", e),
            )
        })?;

    // 2. Hash the product embedding for audit trail
    let mut hasher = Sha256::new();
    for f in &product_embedding {
        hasher.update(f.to_le_bytes());
    }
    let embedding_hash = format!("{:x}", hasher.finalize());

    // 3. Run pgvector similarity query
    let cat_filter = req.category_filter.as_deref();
    let matches = state
        .memory_store
        .match_marketplace_profiles(
            &product_embedding,
            cat_filter,
            req.min_similarity,
            req.max_results,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Marketplace match failed: {}", e),
            )
        })?;

    if matches.is_empty() {
        return Ok(Json(json!({
            "matches": [],
            "total_cost": 0,
            "message": "No profiles matched your query"
        })));
    }

    // 4. Get buyer wallet
    let buyer_wallet = get_or_create_wallet(&state.db, "user", &buyer_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    // 5. Process each match: charge buyer, pay seller, record transaction
    let gas = &state.gas_fees;
    let mut results = Vec::new();
    let mut total_cost = 0i32;

    for (listing, similarity, price_sensitivity, quality_bias) in &matches {
        let price = listing.price_credits;
        let platform_fee =
            gas.marketplace_match_base + (price as f64 * gas.marketplace_platform_pct) as i32;
        let seller_payout = price - (price as f64 * gas.marketplace_platform_pct) as i32;
        let buyer_total = price + gas.marketplace_match_base;

        // Charge buyer
        if let Err(e) = credit_charge(
            &state.db,
            buyer_wallet.wallet_id,
            buyer_total,
            "marketplace_match_purchase",
            &format!("Marketplace match: listing {}", listing.listing_id),
            Some(&listing.listing_id.to_string()),
        )
        .await
        {
            // If buyer can't afford, stop processing
            if results.is_empty() {
                return Err((
                    StatusCode::PAYMENT_REQUIRED,
                    format!("Insufficient credits for match: {}", e),
                ));
            }
            break; // Partial results — buyer ran out mid-batch
        }

        // Pay seller
        let seller_wallet = get_or_create_wallet(&state.db, "user", &listing.seller_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Seller wallet error: {}", e),
                )
            })?;

        let _ = credit_deposit(
            &state.db,
            seller_wallet.wallet_id,
            seller_payout,
            &format!(
                "Marketplace payout: listing {} (buyer: {})",
                listing.listing_id, buyer_id
            ),
        )
        .await;

        // Record transaction
        let _ = state
            .memory_store
            .record_marketplace_transaction(
                listing.listing_id,
                &buyer_id,
                &listing.seller_id,
                *similarity,
                Some(&embedding_hash),
                buyer_total,
                seller_payout,
                platform_fee,
            )
            .await;

        total_cost += buyer_total;

        results.push(json!({
            "listing_id": listing.listing_id,
            "seller_id": listing.seller_id,
            "similarity_score": similarity,
            "price_charged": buyer_total,
            "category_tags": listing.category_tags,
            "description": listing.description,
            "price_sensitivity": price_sensitivity,
            "quality_bias": quality_bias,
        }));
    }

    Ok(Json(json!({
        "matches": results,
        "total_cost": total_cost,
        "match_count": results.len(),
    })))
}

// ─── GET /api/marketplace/listings ─────────────────────────────────

pub async fn list_marketplace_listings_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Query(params): Query<ListingsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let cat_filter: Option<Vec<String>> = params
        .category
        .map(|c| c.split(',').map(|s| s.trim().to_string()).collect());

    let listings = state
        .memory_store
        .get_active_listings(cat_filter.as_deref(), params.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Listing query failed: {}", e),
            )
        })?;

    let items: Vec<Value> = listings
        .iter()
        .map(|l| {
            json!({
                "listing_id": l.listing_id,
                "profile_id": l.profile_id,
                "seller_id": l.seller_id,
                "price_credits": l.price_credits,
                "total_queries": l.total_queries,
                "category_tags": l.category_tags,
                "description": l.description,
                "created_at": l.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "listings": items, "count": items.len() })))
}

// ─── GET /api/marketplace/history ──────────────────────────────────

pub async fn marketplace_history_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let buyer_id = principal.user_id();

    let txs = state
        .memory_store
        .get_match_history(&buyer_id, params.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("History query failed: {}", e),
            )
        })?;

    let items: Vec<Value> = txs
        .iter()
        .map(|t| {
            json!({
                "tx_id": t.tx_id,
                "listing_id": t.listing_id,
                "seller_id": t.seller_id,
                "similarity_score": t.similarity_score,
                "credits_charged": t.credits_charged,
                "credits_to_seller": t.credits_to_seller,
                "platform_fee": t.platform_fee,
                "created_at": t.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "transactions": items, "count": items.len() })))
}

// ─── POST /api/marketplace/listings ────────────────────────────────

pub async fn create_marketplace_listing_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateListingRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let seller_id = principal.user_id();

    // Charge listing fee
    let wallet = get_or_create_wallet(&state.db, "user", &seller_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.marketplace_listing_fee,
        "marketplace_listing_fee",
        "Marketplace listing creation",
        Some(&req.profile_id.to_string()),
    )
    .await?;

    let listing_id = state
        .memory_store
        .create_marketplace_listing(
            req.profile_id,
            &seller_id,
            req.price_credits.max(1),
            req.max_queries_per_buyer,
            req.category_tags.as_deref().unwrap_or(&[]),
            req.description.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Listing creation failed: {}", e),
            )
        })?;

    Ok(Json(json!({
        "listing_id": listing_id,
        "status": "active",
        "price_credits": req.price_credits.max(1),
    })))
}

// ─── GET /api/shopping/profile ─────────────────────────────────────

pub async fn get_shopping_profiles_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let profiles = state
        .memory_store
        .get_user_shopping_profiles(&user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Profile query failed: {}", e),
            )
        })?;

    let items: Vec<Value> = profiles
        .iter()
        .map(|p| {
            json!({
                "profile_id": p.profile_id,
                "agent_id": p.agent_id,
                "profile_name": p.profile_name,
                "embedding_version": p.embedding_version,
                "episode_count": p.episode_count,
                "category_tags": p.category_tags,
                "price_sensitivity": p.price_sensitivity,
                "quality_bias": p.quality_bias,
                "brand_affinities": p.brand_affinities,
                "is_listed": p.is_listed,
                "created_at": p.created_at,
                "updated_at": p.updated_at,
            })
        })
        .collect();

    Ok(Json(json!({ "profiles": items, "count": items.len() })))
}

// ─── PUT /api/shopping/profile/:id/listing ─────────────────────────

pub async fn update_listing_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(listing_id): Path<Uuid>,
    Json(req): Json<UpdateListingRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let seller_id = principal.user_id();

    state
        .memory_store
        .update_marketplace_listing(
            listing_id,
            &seller_id,
            req.status.as_deref(),
            req.price_credits,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Listing update failed: {}", e),
            )
        })?;

    Ok(Json(json!({
        "listing_id": listing_id,
        "updated": true,
    })))
}
