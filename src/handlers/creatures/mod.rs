//! Creature handlers — decomposed by domain.
//!
//! Re-exports every public handler so `handlers::creatures::foo_handler` works
//! unchanged in api_server.rs route registrations.

mod agent_modules;
mod collections;
mod devices;
mod flights;
mod goals;
pub(crate) mod helpers;
mod identity;
mod query;
mod state;
mod swarms;
mod tethering;

// ─── Re-exports: query ─────────────────────────────────────────────
pub use query::{
    creature_activity_handler, creature_animation_layer_handler, creature_animation_status_handler,
    creature_cognition_handler, creature_flight_path_handler, creature_flights_handler,
    creature_image_handler, creature_versions_handler, feed_handler, get_creature_handler,
    list_creatures_handler, list_visible_flights_handler,
};

// ─── Re-exports: flights ───────────────────────────────────────────
pub use flights::{
    append_telemetry_handler, end_flight_handler, export_flight_handler, fly_handler,
    import_flight_handler, record_flight_handler,
};

// ─── Re-exports: state (location + rabble) ─────────────────────────
pub use state::{
    favourite_creature_handler, host_rabble_handler, join_by_qr_token_handler, join_swarm_handler,
    perch_handler, unfavourite_creature_handler,
};

// ─── Re-exports: tethering ─────────────────────────────────────────
pub use tethering::{
    get_track_handler, push_telemetry_handler, tether_handler, untether_handler,
    update_creature_presence_handler,
};

// ─── Re-exports: agent_modules ─────────────────────────────────────
pub use agent_modules::{
    creature_dream_handler, creature_level_handler, enemy_sensor_handler, forage_handler,
    genome_profiler_handler, prey_locator_handler,
};

// ─── Re-exports: goals ─────────────────────────────────────────────
pub use goals::{create_goal_handler, list_goals_handler, update_goal_handler};

// ─── Re-exports: identity ──────────────────────────────────────────
pub use identity::{
    animate_creature_handler, generate_art_batch_handler, generate_art_handler,
    mint_creature_handler, sosa_opt_in_handler, transfer_creature_handler, update_creature_handler,
    update_creature_status_handler, update_creature_visibility_handler,
};

// ─── Re-exports: swarms ────────────────────────────────────────────
pub use swarms::{
    create_swarm_handler, end_rabble_handler, get_swarm_handler, leave_rabble_handler,
    list_swarms_handler, my_rabbles_handler, update_swarm_handler,
};

// ─── Re-exports: collections ───────────────────────────────────────
pub use collections::{
    create_collection_handler, list_collections_handler, update_collection_handler,
};

// ─── Re-exports: devices ───────────────────────────────────────────
pub use devices::{
    list_devices_handler, pair_device_handler, report_device_location_handler,
    unpair_device_handler, update_device_handler,
};

// ─── Shared helpers (used across submodules) ────────────────────────

/// Generate a short alphanumeric QR token (8 chars).
pub(crate) fn generate_qr_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Try to extract a JSON object from an agent response that may contain markdown fences
#[allow(dead_code)]
pub(crate) fn extract_json_from_response(text: &str) -> Option<serde_json::Value> {
    // Try direct parse first
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
        if val.is_object() {
            return Some(val);
        }
    }
    // Try extracting from json fences
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                if val.is_object() {
                    return Some(val);
                }
            }
        }
    }
    // Try extracting from bare fences
    if let Some(start) = text.find("```\n") {
        let after = &text[start + 4..];
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                if val.is_object() {
                    return Some(val);
                }
            }
        }
    }
    // Try finding first { ... last }
    let first_brace = text.find('{')?;
    let last_brace = text.rfind('}')?;
    if last_brace > first_brace {
        let json_str = &text[first_brace..=last_brace];
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            if val.is_object() {
                return Some(val);
            }
        }
    }
    None
}
pub(crate) async fn generate_creature_image(
    pool: &sqlx::PgPool,
    creature_id: uuid::Uuid,
    scientific_name: &str,
    common_name: Option<&str>,
    species_group: &str,
    gbif_key: Option<i64>,
    style: &str,
) -> Result<String, String> {
    let api_key =
        std::env::var("GEMINI_API_KEY").map_err(|_| "GEMINI_API_KEY not set".to_string())?;

    // Fetch GBIF reference
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

    let display_name = common_name
        .map(|c| format!("{} ({})", c, scientific_name))
        .unwrap_or_else(|| scientific_name.to_string());

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
         Art style: {}\n\
         Anatomical details: {}\n\
         Composition: Single specimen, centered, anatomically accurate. No text or labels. Square format, dark background (#1A2E20). \
         The style should be STRONGLY distinct from a photograph — this should unmistakably look like the specified art style.{}",
        display_name, species_group, style_instruction, group_detail, reference_desc,
    );

    let gemini_body = serde_json::json!({
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
        .map_err(|e| format!("Gemini request failed: {}", e))?;

    if !response.status().is_success() {
        let err = response.text().await.unwrap_or_default();
        return Err(format!("Gemini error: {}", err));
    }

    let gemini_resp: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    let inline_data = gemini_resp
        .pointer("/candidates/0/content/parts/0/inlineData")
        .ok_or("No image in response")?;
    let mime_type = inline_data
        .get("mimeType")
        .and_then(|v| v.as_str())
        .unwrap_or("image/png");
    let b64_data = inline_data
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or("No image data")?;

    use base64::Engine;
    let decoder = base64::engine::general_purpose::STANDARD;
    let bytes = decoder
        .decode(b64_data)
        .map_err(|e| format!("Decode error: {}", e))?;

    let ext = if mime_type.contains("png") {
        "png"
    } else if mime_type.contains("webp") {
        "webp"
    } else {
        "jpg"
    };
    let filename = format!("{}.{}", creature_id, ext);
    let fs_path = format!("static/creatures/{}", filename);

    std::fs::create_dir_all("static/creatures").map_err(|e| format!("mkdir error: {}", e))?;
    std::fs::write(&fs_path, &bytes).map_err(|e| format!("write error: {}", e))?;

    // Persist to database for cross-deploy durability
    identity::persist_creature_image(pool, creature_id, &bytes, mime_type).await;

    // Use API endpoint as asset_path (survives redeploys)
    let api_path = format!("/api/creatures/{}/image", creature_id);

    let gen_params = serde_json::json!({
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
    .map_err(|e| format!("DB update error: {}", e))?;

    Ok(api_path)
}

/// Trigger swarm host agent to generate a welcome message for a joining creature.
pub(crate) async fn trigger_swarm_host_welcome(
    state: &crate::AppState,
    swarm_id: uuid::Uuid,
    creature_name: &str,
    species_name: &str,
    species_group: &str,
) {
    use crate::{resolve_agent, resolve_agent_card};
    use fermi::agent_backend::executor::AgentExecutor;
    use fermi::agent_backend::tool_executor::ToolAwareExecutor;
    use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
    use fermi::agent_backend::ExecutionContext;
    use fermi::ast;
    use std::sync::Arc;

    let db_agent = match resolve_agent(state, "swarm_host").await {
        Ok(a) => a,
        Err(_) => return,
    };
    let card = resolve_agent_card(state, &db_agent);

    let query = format!(
        "Welcome {} ({}, {}) to the rabble! Share a fun taxonomic fact about this species.",
        creature_name, species_name, species_group
    );

    let agent_stmt = ast::AgentStmt {
        name: "swarm_host".to_string(),
        agent_type: Some(card.agent_type.clone()),
        query,
        executor: Some(ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let program = ast::Program {
        statements: vec![ast::Statement::Agent(agent_stmt.clone())],
    };

    // SPEC_28 — credentials resolved before the card is moved into the
    // context below.
    let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;

    let context = ExecutionContext {
        program,
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
        credentials: credentials.clone(),
    };

    let tool_context = Arc::new(ToolContext {
        // This path persists no episode of its own (mig-198), so there is
        // nothing for a child to point at: anything delegated from here is
        // recorded as a root. Its cost is still captured, just not linked
        // into a delegation tree.
        parent_episode_id: None,
        credentials,
        memory_store: state.memory_store.clone(),
        embedder: state.embedder.clone(),
        registry: state.registry.clone(),
        current_agent_id: Some(db_agent.agent_id),
        workspace_id: None,
        workspace_slug: None,
        workspace_git: None,
        db: Some(state.db.clone()),
        gas_fees: Some(state.gas_fees.clone()),
        user_id: None,
        user_secrets: None,
        eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
            state: state.clone(),
        })),
        remote_mcp: None,
    });

    let tool_executor = ToolAwareExecutor::new(
        state.registry.executor_arc(),
        ToolRegistry::standard(),
        tool_context,
    );

    match tool_executor.execute(&agent_stmt, &context).await {
        Ok(output) => {
            let narrative = if let Some(reasoning) = &output.metadata.reasoning {
                reasoning.trim().to_string()
            } else {
                output
                    .evidence
                    .first()
                    .and_then(|e| e.summary.clone())
                    .unwrap_or_default()
            };
            if !narrative.is_empty() {
                let _ = crate::handlers::rabble_chat::insert_narrator_message(
                    state, swarm_id, &narrative,
                )
                .await;
            }
        }
        Err(e) => {
            eprintln!("Swarm host welcome failed: {}", e);
        }
    }
}
