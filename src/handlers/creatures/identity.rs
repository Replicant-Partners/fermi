//! Creature identity & lifecycle — mint, update, transfer, art, animation.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::handlers::rabble_workspace;
use crate::AppState;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

// ─── Creature minting ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MintCreatureRequest {
    pub scientific_name: String,
    pub common_name: Option<String>,
    pub species_group: String,
    pub gbif_key: Option<i64>,
    pub taxonomy: Option<serde_json::Value>,
    pub specimen_name: Option<String>,
    pub variation_notes: Option<String>,
    pub generate_art: Option<bool>,
    pub art_style: Option<String>,
}

/// POST /api/creatures/mint — mint a new creature from a GBIF species.
/// Costs creature_mint credits (default 3). Optionally triggers art generation (+5cr).
pub async fn mint_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<MintCreatureRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate species_group
    let valid_groups = [
        "butterfly",
        "dragonfly",
        "beetle",
        "bee",
        "locust",
        "fly",
        "bug",
        "insect",
    ];
    if !valid_groups.contains(&req.species_group.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid species_group '{}'. Must be one of: {}",
                req.species_group,
                valid_groups.join(", ")
            ),
        ));
    }

    let generate_art = req.generate_art.unwrap_or(true);
    let art_style = req.art_style.as_deref().unwrap_or("naturalist");

    // Calculate total cost
    let mint_cost = state.gas_fees.creature_mint;
    let art_cost = if generate_art {
        state.gas_fees.creature_art
    } else {
        0
    };
    let total_cost = mint_cost + art_cost;

    // Charge upfront
    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        pool,
        wallet.wallet_id,
        total_cost,
        "creature_mint",
        &format!(
            "Mint {} ({}cr mint + {}cr art)",
            req.scientific_name, mint_cost, art_cost
        ),
        None,
    )
    .await?;

    // Ensure user has a personal workspace (menagerie)
    let personal_ws_id = rabble_workspace::ensure_personal_workspace(&state, &user_id)
        .await
        .ok();

    // Auto-generate specimen name if not provided
    let specimen_name = if let Some(ref name) = req.specimen_name {
        name.clone()
    } else {
        let display = req.common_name.as_deref().unwrap_or(&req.scientific_name);
        // Count user's existing creatures of this species
        let count: i64 = sqlx::query(
            "SELECT COUNT(*) as cnt FROM creatures WHERE owner_id = $1 AND scientific_name = $2",
        )
        .bind(&user_id)
        .bind(&req.scientific_name)
        .fetch_one(pool)
        .await
        .map(|r| r.try_get("cnt").unwrap_or(0))
        .unwrap_or(0);
        format!("{} #{}", display, count + 1)
    };

    // Global mint number for this species
    let mint_number: i64 =
        sqlx::query("SELECT COUNT(*) as cnt FROM creatures WHERE scientific_name = $1")
            .bind(&req.scientific_name)
            .fetch_one(pool)
            .await
            .map(|r| r.try_get("cnt").unwrap_or(0))
            .unwrap_or(0);

    let creature_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let taxonomy = req.taxonomy.unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO creatures (creature_id, owner_id, scientific_name, common_name,
         species_group, gbif_key, taxonomy, specimen_name, variation_notes,
         asset_path, mint_number, data_card, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                 '/static/creatures/placeholder.svg', $10, '{}', $11, $11)",
    )
    .bind(creature_id)
    .bind(&user_id)
    .bind(&req.scientific_name)
    .bind(&req.common_name)
    .bind(&req.species_group)
    .bind(req.gbif_key)
    .bind(&taxonomy)
    .bind(&specimen_name)
    .bind(&req.variation_notes)
    .bind((mint_number + 1) as i32)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Dual-write: init creature_conditions (new versioned model) ──
    super::helpers::init_conditions(pool, creature_id, "public", false).await;

    // Spawn async art generation if requested
    let art_generating = if generate_art {
        let pool_clone = pool.clone();
        let sci_name = req.scientific_name.clone();
        let common = req.common_name.clone();
        let group = req.species_group.clone();
        let gbif = req.gbif_key;
        let style = art_style.to_string();
        tokio::spawn(async move {
            match super::generate_creature_image(
                &pool_clone,
                creature_id,
                &sci_name,
                common.as_deref(),
                &group,
                gbif,
                &style,
            )
            .await
            {
                Ok(path) => eprintln!("[rabble] Art generated for {}: {}", sci_name, path),
                Err(e) => eprintln!("[rabble] Art generation failed for {}: {}", sci_name, e),
            }
        });
        true
    } else {
        false
    };

    // Dispatch naturalist agent to generate specimen description (non-blocking)
    if let Some(ws_id) = personal_ws_id {
        let state2 = state.clone();
        let user_id2 = user_id.clone();
        let spec_name = specimen_name.clone();
        let sci_name2 = req.scientific_name.clone();
        let group2 = req.species_group.clone();
        tokio::spawn(async move {
            let query = format!(
                "New creature minted: {} ({}, {}). Generate a specimen description and a fun taxonomic fact.",
                spec_name, sci_name2, group2
            );
            match rabble_workspace::dispatch_rabble_action(
                &state2,
                ws_id,
                "naturalist",
                "creature_mint",
                &query,
                &user_id2,
            )
            .await
            {
                Ok(desc) => {
                    // Store description in variation_notes
                    let _ = sqlx::query(
                        "UPDATE creatures SET variation_notes = $1 WHERE creature_id = $2",
                    )
                    .bind(&desc)
                    .bind(creature_id)
                    .execute(&state2.db)
                    .await;
                    eprintln!(
                        "[rabble] Naturalist described {}: {}...",
                        spec_name,
                        &desc[..desc.len().min(80)]
                    );
                }
                Err(e) => eprintln!(
                    "[rabble] Naturalist dispatch failed for {}: {}",
                    spec_name, e
                ),
            }
        });
    }

    // Emit activity event (lightweight, fire-and-forget)
    {
        let pool_ae = state.memory_store.pool().clone();
        let uid_ae = user_id.clone();
        let specimen_ae = specimen_name.clone();
        let species_ae = req.species_group.clone();
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &pool_ae,
                &uid_ae,
                Some(creature_id),
                "creature_minted",
                None,
                None,
                &format!(
                    "{} minted a new {} creature: {}",
                    uid_ae, species_ae, specimen_ae
                ),
                None,
                None,
            )
            .await;
        });
    }

    Ok(Json(json!({
        "creature_id": creature_id,
        "owner_id": user_id,
        "scientific_name": req.scientific_name,
        "common_name": req.common_name,
        "species_group": req.species_group,
        "specimen_name": specimen_name,
        "mint_number": mint_number + 1,
        "asset_path": "/static/creatures/placeholder.svg",
        "art_generating": art_generating,
        "art_style": art_style,
        "credits_charged": total_cost,
        "personal_workspace_id": personal_ws_id,
        "created_at": now.to_rfc3339(),
    })))
}

// ─── Art generation endpoints ──────────────────────────────────────

/// POST /api/creatures/:id/generate-art — generate unique illustration for a creature
///
/// Charges 5 credits. Calls Gemini image generation with a naturalist prompt
/// informed by GBIF species data. Updates creature asset_path from placeholder.
pub async fn generate_art_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<GenerateArtRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creature exists
    let row = sqlx::query(
        "SELECT creature_id, scientific_name, common_name, species_group, gbif_key, asset_path
         FROM creatures WHERE creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let current_path = row.get::<String, _>("asset_path");
    let scientific_name = row.get::<String, _>("scientific_name");
    let common_name = row.get::<Option<String>, _>("common_name");
    let species_group = row.get::<String, _>("species_group");
    let gbif_key = row.get::<Option<i64>, _>("gbif_key");

    // Skip if already generated (unless force=true)
    if !current_path.contains("placeholder") && !req.force.unwrap_or(false) {
        return Ok(Json(json!({
            "status": "already_generated",
            "creature_id": creature_id,
            "asset_path": current_path,
            "message": "Art already exists. Use force=true to regenerate."
        })));
    }

    // Charge credits
    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        pool,
        wallet.wallet_id,
        5,
        "execution_fee",
        &format!("Generate art for creature {}", creature_id),
        Some(&creature_id.to_string()),
    )
    .await?;

    let style = req.style.as_deref().unwrap_or("naturalist");

    // Build GBIF reference
    let mut reference_desc = String::new();
    if let Some(key) = gbif_key {
        let client = reqwest::Client::new();
        let media_url = format!("https://api.gbif.org/v1/species/{}/media", key);
        if let Ok(resp) = client
            .get(&media_url)
            .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
            .send()
            .await
        {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(results) = body.get("results").and_then(|v| v.as_array()) {
                    let descs: Vec<&str> = results
                        .iter()
                        .take(3)
                        .filter_map(|m| {
                            m.get("description")
                                .or(m.get("title"))
                                .and_then(|v| v.as_str())
                        })
                        .collect();
                    if !descs.is_empty() {
                        reference_desc = format!(" Reference: {}", descs.join("; "));
                    }
                }
            }
        }
    }

    // Build prompt
    let display_name = common_name
        .as_deref()
        .map(|c| format!("{} ({})", c, scientific_name))
        .unwrap_or_else(|| scientific_name.clone());

    let style_instruction = match style {
        "watercolor" => "Loose, flowing watercolor painting. Visible wet-on-wet brush strokes, soft bleeding edges where colors meet. Natural color blending on rough textured watercolor paper. Delicate translucent washes layered for depth. Paper texture visible through thin areas. Warm natural palette.",
        "botanical" => "Precise botanical field guide plate in the tradition of Redouté. Fine ink line work with subtle color wash. Specimen shown from multiple angles (dorsal, ventral, lateral). Cream parchment paper background. Labeled-feeling composition with careful attention to morphological detail. Muted, scholarly palette.",
        "field-guide" => "Peterson-style field guide illustration. Clean side profile with wings spread. Key identifying features emphasized with high contrast. Crisp white background. Proportions accurate for species identification. Bold diagnostic markings highlighted. Clear, educational style with no artistic embellishment.",
        "ukiyo-e" => "Japanese woodblock print (ukiyo-e) in the style of Kitagawa Utamaro's insect studies. Bold black outlines, flat color planes with bokashi gradation. Washi paper texture with subtle fiber. Decorative natural background: cherry blossoms, chrysanthemums, or bamboo. Traditional palette: indigo, ochre, vermillion, grey. Red hanko seal in corner.",
        _ => "Detailed scientific illustration in the style of Maria Sibylla Merian. Precise anatomical rendering with rich, luminous colors on aged vellum. Fine cross-hatching for texture. Specimen plate layout showing the creature in naturalistic pose. Warm golden undertones from the vellum showing through.",
    };

    let group_detail = if species_group == "dragonfly" {
        "Emphasize: iridescent wing venation patterns, elongated segmented abdomen, large compound eyes with metallic sheen, translucent wings with pterostigma visible, thorax coloration and markings."
    } else if species_group == "locust" {
        "Emphasize: powerful hind legs with tibial spurs, tegmina texture, compound eyes, mandible structure, wing membrane patterns when spread, body segmentation and pronotum shape."
    } else {
        "Emphasize: intricate wing scale patterns and coloration, coiled proboscis, clubbed antennae, body fur texture, eyespot details if present, upper and lower wing surfaces distinct."
    };

    let prompt = format!(
        "Create a high-quality scientific illustration of a {} ({}).\n\
         Art style: {}\nAnatomical details: {}\n\
         Composition: single specimen, centered, anatomically accurate, \
         no text/labels/watermarks, square format, dark background (#1A2E20).{}",
        display_name, species_group, style_instruction, group_detail, reference_desc,
    );

    // Call Gemini
    let api_key = std::env::var("GEMINI_API_KEY").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Image generation unavailable".to_string(),
        )
    })?;

    let gemini_body = json!({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {"responseModalities": ["IMAGE"]}
    });

    let client = reqwest::Client::new();
    let response = client
        .post("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent")
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&gemini_body)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Gemini request failed: {}", e)))?;

    if !response.status().is_success() {
        let err = response.text().await.unwrap_or_default();
        return Err((StatusCode::BAD_GATEWAY, format!("Gemini error: {}", err)));
    }

    let gemini_resp: serde_json::Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

    // Extract image
    let inline_data = gemini_resp
        .pointer("/candidates/0/content/parts/0/inlineData")
        .ok_or((
            StatusCode::BAD_GATEWAY,
            "No image in Gemini response".to_string(),
        ))?;
    let mime_type = inline_data
        .get("mimeType")
        .and_then(|v| v.as_str())
        .unwrap_or("image/png");
    let b64_data = inline_data
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_GATEWAY, "No image data".to_string()))?;

    // Decode and save
    use base64::Engine;
    let decoder = base64::engine::general_purpose::STANDARD;
    let bytes = decoder.decode(b64_data).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Decode error: {}", e),
        )
    })?;

    let ext = if mime_type.contains("png") {
        "png"
    } else if mime_type.contains("webp") {
        "webp"
    } else {
        "jpg"
    };
    let filename = format!("{}.{}", creature_id, ext);
    let relative_path = format!("/static/creatures/{}", filename);
    let fs_path = format!("static/creatures/{}", filename);

    std::fs::create_dir_all("static/creatures")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    std::fs::write(&fs_path, &bytes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Persist to database for cross-deploy durability
    persist_creature_image(pool, creature_id, &bytes, mime_type).await;

    // Use API endpoint as asset_path (survives redeploys, unlike static files)
    let api_path = format!("/api/creatures/{}/image", creature_id);

    // Update DB
    let gen_params = json!({
        "style": style,
        "prompt": prompt,
        "mime_type": mime_type,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "gbif_key": gbif_key,
        "file_size_bytes": bytes.len(),
    });

    sqlx::query(
        "UPDATE creatures SET asset_path = $1, generation_params = $2, updated_at = NOW()
         WHERE creature_id = $3",
    )
    .bind(&api_path)
    .bind(&gen_params)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "generated",
        "creature_id": creature_id,
        "asset_path": api_path,
        "mime_type": mime_type,
        "file_size_bytes": bytes.len(),
        "style": style,
    })))
}

#[derive(Deserialize)]
pub struct GenerateArtRequest {
    pub style: Option<String>,
    pub force: Option<bool>,
}

/// POST /api/creatures/generate-art-batch — generate art for all placeholder creatures
///
/// Admin-only (owner_id = 'system' creatures). Spawns background tasks.
/// Returns immediately with count of creatures queued.
pub async fn generate_art_batch_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<BatchArtRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let style = req.style.unwrap_or_else(|| "naturalist".to_string());
    let limit = req.limit.unwrap_or(5).min(20); // max 20 per batch

    // Find creatures still on placeholder
    let rows = sqlx::query(
        "SELECT creature_id, scientific_name, common_name, species_group, gbif_key
         FROM creatures
         WHERE asset_path LIKE '%placeholder%'
         ORDER BY created_at ASC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if rows.is_empty() {
        return Ok(Json(json!({
            "status": "complete",
            "message": "All creatures already have art",
            "queued": 0,
        })));
    }

    let queued_count = rows.len();

    // Charge per creature
    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_cost = queued_count as i32 * 5;
    charge_gas(
        pool,
        wallet.wallet_id,
        total_cost,
        "execution_fee",
        &format!("Batch art generation for {} creatures", queued_count),
        None,
    )
    .await?;

    // Spawn background generation for each creature
    let pool_clone = state.memory_store.pool().clone();
    let style_clone = style.clone();
    tokio::spawn(async move {
        for row in rows {
            let creature_id: Uuid = row.get("creature_id");
            let scientific_name: String = row.get("scientific_name");
            let common_name: Option<String> = row.get("common_name");
            let species_group: String = row.get("species_group");
            let gbif_key: Option<i64> = row.get("gbif_key");

            match super::generate_creature_image(
                &pool_clone,
                creature_id,
                &scientific_name,
                common_name.as_deref(),
                &species_group,
                gbif_key,
                &style_clone,
            )
            .await
            {
                Ok(path) => {
                    eprintln!(
                        "[rabble] Generated art for {} ({}): {}",
                        scientific_name, creature_id, path,
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[rabble] Art generation failed for {} ({}): {}",
                        scientific_name, creature_id, e,
                    );
                }
            }

            // Small delay between Gemini calls to respect rate limits
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        eprintln!("[rabble] Batch art generation complete");
    });

    Ok(Json(json!({
        "status": "queued",
        "queued": queued_count,
        "style": style,
        "credits_charged": total_cost,
        "message": format!("{} creatures queued for art generation", queued_count),
    })))
}

#[derive(Deserialize)]
pub struct BatchArtRequest {
    pub style: Option<String>,
    pub limit: Option<i64>,
}

// ─── SOSA opt-in toggle ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SosaOptInRequest {
    pub opt_in: bool,
}

/// PUT /api/creatures/:creature_id/sosa-opt-in — toggle SOSA data sharing for a creature.
/// AKP consent: creature owner must explicitly opt in before flight data is bridged to SOSA.
pub async fn sosa_opt_in_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<SosaOptInRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let owner: Option<String> =
        sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map(|r| r.get("owner_id"));
    match owner {
        None => return Err((StatusCode::NOT_FOUND, "Creature not found".to_string())),
        Some(o) if o != user_id => {
            return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()))
        }
        _ => {}
    }

    let result = sqlx::query(
        "UPDATE creature_conditions SET sosa_opt_in = $1, updated_at = NOW()
         WHERE creature_id = $2",
    )
    .bind(req.opt_in)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature conditions not initialized".to_string(),
        ));
    }

    Ok(Json(json!({
        "creature_id": creature_id,
        "sosa_opt_in": req.opt_in,
        "message": if req.opt_in {
            "SOSA data sharing enabled — future flights will generate universal sensor observations"
        } else {
            "SOSA data sharing disabled — flight data stays private"
        },
    })))
}

// ─── Creature update handlers ──────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateCreatureRequest {
    pub specimen_name: Option<String>,
    pub variation_notes: Option<String>,
}

/// PUT /api/creatures/:id — update mutable fields (owner only)
pub async fn update_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<UpdateCreatureRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let mut sets = Vec::new();
    let mut bind_idx = 0u32;

    // Collect SET clauses
    let specimen_name = req.specimen_name;
    let variation_notes = req.variation_notes;

    if specimen_name.is_some() {
        bind_idx += 1;
        sets.push(format!("specimen_name = ${}", bind_idx));
    }
    if variation_notes.is_some() {
        bind_idx += 1;
        sets.push(format!("variation_notes = ${}", bind_idx));
    }

    if sets.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    sets.push("updated_at = NOW()".to_string());
    let creature_bind = bind_idx + 1;
    let owner_bind = bind_idx + 2;

    let sql = format!(
        "UPDATE creatures SET {} WHERE creature_id = ${} AND owner_id = ${}",
        sets.join(", "),
        creature_bind,
        owner_bind
    );

    let mut query = sqlx::query(&sql);
    if let Some(ref name) = specimen_name {
        query = query.bind(name);
    }
    if let Some(ref notes) = variation_notes {
        query = query.bind(notes);
    }
    query = query.bind(creature_id).bind(&user_id);

    let result = query
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature not found or not owned by you".to_string(),
        ));
    }

    Ok(Json(json!({
        "creature_id": creature_id,
        "updated": true,
    })))
}

#[derive(Deserialize)]
pub struct UpdateCreatureStatusRequest {
    pub status: String,
}

/// PUT /api/creatures/:id/status — archive/restore/retire (owner only)
pub async fn update_creature_status_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<UpdateCreatureStatusRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate status value
    if !["active", "archived", "retired"].contains(&req.status.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Status must be 'active', 'archived', or 'retired'".to_string(),
        ));
    }

    let result = sqlx::query(
        "UPDATE creatures SET status = $1, updated_at = NOW()
         WHERE creature_id = $2 AND owner_id = $3",
    )
    .bind(&req.status)
    .bind(creature_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature not found or not owned by you".to_string(),
        ));
    }

    Ok(Json(json!({
        "creature_id": creature_id,
        "status": req.status,
    })))
}

/// Helper: persist image bytes to creature_images table
pub async fn persist_creature_image(
    pool: &sqlx::PgPool,
    creature_id: Uuid,
    bytes: &[u8],
    mime_type: &str,
) {
    let _ = sqlx::query(
        "INSERT INTO creature_images (creature_id, image_bytes, mime_type, file_size)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (creature_id) DO UPDATE
         SET image_bytes = $2, mime_type = $3, file_size = $4, updated_at = NOW()",
    )
    .bind(creature_id)
    .bind(bytes)
    .bind(mime_type)
    .bind(bytes.len() as i32)
    .execute(pool)
    .await;
}

// ─── Creature Transfer (Gift) ──────────────────────────────────────

#[derive(Deserialize)]
pub struct TransferCreatureRequest {
    pub recipient_id: String,
}

/// POST /api/creatures/:creature_id/transfer — gift a creature to another user.
pub async fn transfer_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<String>,
    Json(body): Json<TransferCreatureRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let owner_id = principal.user_id();
    let cid = Uuid::parse_str(&creature_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid creature ID".into()))?;

    if body.recipient_id == owner_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Cannot transfer to yourself".into(),
        ));
    }

    // Verify ownership
    let current_owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id FROM creatures WHERE creature_id = $1")
            .bind(cid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match current_owner {
        None => return Err((StatusCode::NOT_FOUND, "Creature not found".into())),
        Some(ref oid) if oid != &owner_id => {
            return Err((StatusCode::FORBIDDEN, "You don't own this creature".into()));
        }
        _ => {}
    }

    // Verify recipient exists
    let recipient_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE user_id = $1)")
            .bind(&body.recipient_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !recipient_exists {
        return Err((StatusCode::NOT_FOUND, "Recipient not found".into()));
    }

    // Transfer ownership
    sqlx::query("UPDATE creatures SET owner_id = $1 WHERE creature_id = $2")
        .bind(&body.recipient_id)
        .bind(cid)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get creature name for notification
    let creature_name: String = sqlx::query_scalar(
        "SELECT COALESCE(specimen_name, common_name, scientific_name) FROM creatures WHERE creature_id = $1",
    )
    .bind(cid)
    .fetch_one(&state.db)
    .await
    .unwrap_or_else(|_| "a creature".to_string());

    // Notify recipient
    crate::handlers::push::notify_user(
        &state.db,
        &body.recipient_id,
        "creature_gift",
        &format!("You received {}!", creature_name),
        Some(&format!(
            "Someone gifted you the creature '{}'",
            creature_name
        )),
        Some(&json!({
            "creature_id": cid,
            "creature_name": creature_name,
        })),
        None,
    )
    .await;

    // Emit activity event (fire-and-forget)
    {
        let _pool_ae = state.memory_store.pool().clone();
        let _uid_ae = owner_id.clone();
        let _cid_ae = cid;
        let _creature_name_ae = creature_name.clone();
        let _recipient_ae = body.recipient_id.clone();
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &_pool_ae,
                &_uid_ae,
                Some(_cid_ae),
                "creature_gifted",
                None,
                None,
                &format!("{} was gifted to a new owner", _creature_name_ae),
                None,
                None,
            )
            .await;
        });
    }

    // Broadcast creature SSE event
    crate::handlers::streams::emit_creature_event(
        &state,
        cid,
        "transferred",
        json!({
            "creature_id": cid,
            "new_owner": body.recipient_id,
            "creature_name": creature_name,
        }),
    );

    Ok(Json(json!({
        "status": "transferred",
        "creature_id": creature_id,
        "new_owner": body.recipient_id,
    })))
}

// ─── Wing Animation (Make It Alive) ────────────────────────────────

/// Helper: persist a single animation layer to the database.
pub async fn persist_animation_layer(
    pool: &sqlx::PgPool,
    creature_id: Uuid,
    layer_name: &str,
    bytes: &[u8],
    mime_type: &str,
) {
    let _ = sqlx::query(
        "INSERT INTO creature_animation_layers (creature_id, layer_name, image_bytes, mime_type, file_size)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (creature_id, layer_name) DO UPDATE
         SET image_bytes = $3, mime_type = $4, file_size = $5, updated_at = NOW()",
    )
    .bind(creature_id)
    .bind(layer_name)
    .bind(bytes)
    .bind(mime_type)
    .bind(bytes.len() as i32)
    .execute(pool)
    .await;
}

/// POST /api/creatures/:creature_id/animate — trigger wing segmentation.
/// Charges creature_animate credits and spawns background Gemini segmentation.
pub async fn animate_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // 1. Verify creature exists, is owned, and is a butterfly
    let row = sqlx::query(
        "SELECT owner_id, species_group, animation_status FROM creatures WHERE creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner_id: String = row.get("owner_id");
    if owner_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            "You don't own this creature".to_string(),
        ));
    }

    let species_group: String = row.get("species_group");
    if species_group != "butterfly" {
        return Err((StatusCode::BAD_REQUEST, "Wing animation is currently only available for butterflies. Other species coming soon!".to_string()));
    }

    let status: Option<String> = row.try_get("animation_status").unwrap_or(None);
    if status.as_deref() == Some("ready") {
        return Ok(Json(json!({
            "status": "ready",
            "creature_id": creature_id,
            "message": "This creature already has animation layers.",
            "layers": {
                "body": format!("/api/creatures/{}/animation/body", creature_id),
                "left_wing": format!("/api/creatures/{}/animation/left_wing", creature_id),
                "right_wing": format!("/api/creatures/{}/animation/right_wing", creature_id),
            }
        })));
    }
    if status.as_deref() == Some("processing") {
        return Ok(Json(json!({
            "status": "processing",
            "creature_id": creature_id,
            "message": "Animation is already being generated. Please poll /animation-status."
        })));
    }

    // 2. Charge credits
    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    let gas_fees = &state.gas_fees;
    charge_gas(
        pool,
        wallet.wallet_id,
        gas_fees.creature_animate,
        "creature_animate",
        &format!("Wing animation for creature {}", creature_id),
        Some(&creature_id.to_string()),
    )
    .await?;

    // 3. Set status to processing
    sqlx::query("UPDATE creatures SET animation_status = 'processing', updated_at = NOW() WHERE creature_id = $1")
        .bind(creature_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 4. Spawn background task for Gemini segmentation
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = run_wing_segmentation(&pool_clone, creature_id).await {
            tracing::error!("Wing segmentation failed for {}: {}", creature_id, e);
            let _ = sqlx::query(
                "UPDATE creatures SET animation_status = 'failed', updated_at = NOW() WHERE creature_id = $1",
            )
            .bind(creature_id)
            .execute(&pool_clone)
            .await;
        }
    });

    Ok(Json(json!({
        "status": "processing",
        "creature_id": creature_id,
        "message": "Wing segmentation started. Poll /animation-status for progress."
    })))
}

/// Background task: segment creature image into 3 layers via Gemini edit_image.
async fn run_wing_segmentation(pool: &sqlx::PgPool, creature_id: Uuid) -> Result<(), String> {
    let api_key =
        std::env::var("GEMINI_API_KEY").map_err(|_| "GEMINI_API_KEY not set".to_string())?;

    // Fetch source image from creature_images
    let img_row =
        sqlx::query("SELECT image_bytes, mime_type FROM creature_images WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("DB error fetching image: {}", e))?
            .ok_or_else(|| "No image found for creature. Generate art first.".to_string())?;

    let image_bytes: Vec<u8> = img_row.get("image_bytes");
    let source_mime: String = img_row.get("mime_type");

    use base64::Engine;
    let encoder = base64::engine::general_purpose::STANDARD;
    let img_base64 = encoder.encode(&image_bytes);

    // Segmentation prompts for each layer
    let layers = [
        ("left_wing", "Isolate ONLY the left wing (viewer's left) of this butterfly specimen. Remove the body, right wing, antennae, and all other parts completely. Output ONLY the left wing on a fully transparent background (PNG with alpha). Preserve the exact wing shape, coloration, scale patterns, and venation. The wing should be positioned exactly where it appears in the original image. Do not add any artistic effects, shadows, or modifications."),
        ("right_wing", "Isolate ONLY the right wing (viewer's right) of this butterfly specimen. Remove the body, left wing, antennae, and all other parts completely. Output ONLY the right wing on a fully transparent background (PNG with alpha). Preserve the exact wing shape, coloration, scale patterns, and venation. The wing should be positioned exactly where it appears in the original image. Do not add any artistic effects, shadows, or modifications."),
        ("body", "Isolate ONLY the body (thorax, abdomen, head, antennae, legs) of this butterfly specimen. Remove both wings completely, leaving only the central body structure. Output on a fully transparent background (PNG with alpha). Preserve exact body position, coloration, and detail from the original image. The body should be positioned exactly where it appears in the original."),
    ];

    let client = reqwest::Client::new();
    let gemini_url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent";

    for (layer_name, prompt) in &layers {
        tracing::info!("Segmenting {} for creature {}", layer_name, creature_id);

        let body = json!({
            "contents": [{
                "parts": [
                    { "text": prompt },
                    {
                        "inlineData": {
                            "mimeType": source_mime,
                            "data": img_base64
                        }
                    }
                ]
            }],
            "generationConfig": {
                "responseModalities": ["TEXT", "IMAGE"]
            }
        });

        let response = client
            .post(gemini_url)
            .header("x-goog-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini request failed for {}: {}", layer_name, e))?;

        if !response.status().is_success() {
            let err = response.text().await.unwrap_or_default();
            return Err(format!("Gemini error for {}: {}", layer_name, err));
        }

        let gemini_resp: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Parse error for {}: {}", layer_name, e))?;

        // Extract image from response
        let inline_data = gemini_resp
            .pointer("/candidates/0/content/parts")
            .and_then(|parts| parts.as_array())
            .and_then(|parts| parts.iter().find_map(|p| p.get("inlineData")))
            .ok_or_else(|| format!("No image in Gemini response for {}", layer_name))?;

        let mime_type = inline_data
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");
        let b64_data = inline_data
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("No image data for {}", layer_name))?;

        let decoded = encoder
            .decode(b64_data)
            .map_err(|e| format!("Decode error for {}: {}", layer_name, e))?;

        // Basic validation: layer should have some data
        if decoded.len() < 100 {
            return Err(format!(
                "Layer {} too small ({} bytes), likely failed",
                layer_name,
                decoded.len()
            ));
        }

        persist_animation_layer(pool, creature_id, layer_name, &decoded, mime_type).await;

        // Rate limit: 2 second delay between calls
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // All layers done — mark as ready
    sqlx::query(
        "UPDATE creatures SET animation_status = 'ready', updated_at = NOW() WHERE creature_id = $1",
    )
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update animation_status: {}", e))?;

    tracing::info!("Wing segmentation complete for creature {}", creature_id);
    Ok(())
}

// ─── Creature visibility ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateVisibilityRequest {
    pub visibility: String,
}

/// PUT /api/creatures/:creature_id/visibility — set creature visibility
pub async fn update_creature_visibility_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<UpdateVisibilityRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate visibility value
    let visibility = match req.visibility.trim().to_lowercase().as_str() {
        "public" => "public".to_string(),
        "contacts" | "contacts_only" => "contacts".to_string(),
        "private" => "private".to_string(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "visibility must be 'public', 'contacts', or 'private'".to_string(),
            ))
        }
    };

    // Verify ownership
    let creature = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    sqlx::query(
        "UPDATE creature_conditions SET visibility = $1, updated_at = NOW() WHERE creature_id = $2",
    )
    .bind(&visibility)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "creature_id": creature_id,
        "visibility": visibility,
    })))
}
