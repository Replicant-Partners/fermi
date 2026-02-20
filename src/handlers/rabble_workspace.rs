//! Rabble ↔ Workspace integration — auto-workspace creation, agent dispatch, fee distribution.
//!
//! Every rabble (swarm) gets its own workspace with 4 system agents.
//! Every user gets a personal workspace (menagerie) on first mint.
//! Actions route through agents, who earn fractional fees and build knowledge.

use axum::{extract::State, http::StatusCode, Json};
use fermi_auth::{get_or_create_wallet, teams, AuthPrincipal};
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use agent_bestiary_memory::WorkspaceMessage;
use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;
use fermi::gas::{charge_and_distribute, charge_gas, get_workspace_agent_ids};

use crate::{agent_output_to_episode, resolve_agent, resolve_agent_card, AppState};

/// The system agents auto-hired into every rabble workspace.
const RABBLE_SYSTEM_AGENTS: &[&str] = &[
    "naturalist",
    "navigator",
    "swarm_host",
    "keeper",
    "rabble_anchor_manager",
    "rabble_lifecycle_coordinator",
    "flight_coordinator",
    "enemy_sensor",
    "genome_profiler",
    "prey_locator",
    "reynolds_flock",
];

/// Create a workspace for a rabble (swarm) and hire the 4 system agents.
///
/// Returns the workspace UUID (team.id).
pub async fn create_rabble_workspace(
    state: &AppState,
    creator_id: &str,
    name: &str,
    swarm_id: Option<Uuid>,
) -> Result<Uuid, (StatusCode, String)> {
    // Generate a URL-safe slug from the name
    let slug = format!(
        "rabble-{}",
        name.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    );
    // Append short UUID suffix to guarantee uniqueness
    let slug = format!("{}-{}", slug, &Uuid::new_v4().to_string()[..8]);

    // Create the team (workspace) — auto-adds owner to team_members
    let team = teams::create_team(
        &state.db,
        name,
        &slug,
        Some("Auto-created workspace for rabble"),
        creator_id,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create rabble workspace: {}", e),
        )
    })?;

    let ws_id = team.id;

    // No workspace seed — all costs are pass-through. Users fund explicitly.

    // Auto-hire the system agents
    for agent_name in RABBLE_SYSTEM_AGENTS {
        // Look up agent UUID from agents table
        let agent_row =
            sqlx::query("SELECT agent_id FROM agents WHERE agent_name = $1 AND status IN ('active', 'published')")
                .bind(agent_name)
                .fetch_optional(&state.db)
                .await;

        if let Ok(Some(row)) = agent_row {
            if let Ok(agent_id) = row.try_get::<Uuid, _>("agent_id") {
                // Insert into workspace_agents with 'system' relationship
                let _ = sqlx::query(
                    "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship)
                     VALUES ($1, $2, $3, 'system')
                     ON CONFLICT (workspace_id, agent_id) DO NOTHING",
                )
                .bind(ws_id)
                .bind(agent_id)
                .bind(creator_id)
                .execute(&state.db)
                .await;
            }
        }
    }

    // Link swarm to workspace if provided
    if let Some(sid) = swarm_id {
        let _ = sqlx::query("UPDATE swarm_events SET workspace_id = $1 WHERE swarm_id = $2")
            .bind(ws_id)
            .bind(sid)
            .execute(&state.db)
            .await;
    }

    // Initialize workspace git repo
    let _ = state.workspace_git.init_or_open(&slug);

    tracing::info!(
        workspace_id = %ws_id,
        swarm_id = ?swarm_id,
        "Created rabble workspace with {} system agents",
        RABBLE_SYSTEM_AGENTS.len()
    );

    Ok(ws_id)
}

/// Ensure a user has a personal workspace (menagerie). Creates one if missing.
///
/// Returns the personal workspace UUID.
pub async fn ensure_personal_workspace(
    state: &AppState,
    user_id: &str,
) -> Result<Uuid, (StatusCode, String)> {
    // Check if user already has a personal workspace
    let existing = sqlx::query("SELECT personal_workspace_id FROM users WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {}", e),
            )
        })?;

    if let Some(row) = existing {
        if let Ok(Some(ws_id)) = row.try_get::<Option<Uuid>, _>("personal_workspace_id") {
            return Ok(ws_id);
        }
    }

    // Get display name for workspace title
    let display_name: String = sqlx::query("SELECT display_name FROM users WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|r| {
            r.try_get::<Option<String>, _>("display_name")
                .ok()
                .flatten()
        })
        .unwrap_or_else(|| "My".to_string());

    let ws_name = format!("{}'s Menagerie", display_name);

    // Create the workspace
    let ws_id = create_rabble_workspace(state, user_id, &ws_name, None).await?;

    // Link to user
    let _ = sqlx::query("UPDATE users SET personal_workspace_id = $1 WHERE user_id = $2")
        .bind(ws_id)
        .bind(user_id)
        .execute(&state.db)
        .await;

    tracing::info!(user_id = %user_id, workspace_id = %ws_id, "Created personal menagerie workspace");

    Ok(ws_id)
}

/// Dispatch a Rabble action to a workspace agent for execution.
///
/// Mirrors the workspace message execution flow:
/// 1. Resolve agent from workspace_agents
/// 2. Build ExecutionContext + ToolContext
/// 3. Execute via ToolAwareExecutor
/// 4. Record episode with embedding
/// 5. Post result as workspace message
/// 6. Return the agent's text response
///
/// This is fire-and-forget safe — callers should wrap in tokio::spawn.
pub async fn dispatch_rabble_action(
    state: &AppState,
    workspace_id: Uuid,
    agent_name: &str,
    action_type: &str,
    query: &str,
    user_id: &str,
) -> Result<String, String> {
    // Resolve agent
    let db_agent = resolve_agent(state, agent_name)
        .await
        .map_err(|(_code, msg)| msg)?;
    let card = resolve_agent_card(state, &db_agent);

    // Build AST for execution
    let agent_stmt = ast::AgentStmt {
        name: agent_name.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: query.to_string(),
        executor: Some(ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };
    let program = ast::Program {
        statements: vec![ast::Statement::Agent(agent_stmt.clone())],
    };
    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
    };

    // Get workspace slug for git context
    let slug: String = sqlx::query("SELECT slug FROM teams WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("slug").ok())
        .unwrap_or_default();

    // Build ToolContext
    let tool_context = Arc::new(ToolContext {
        memory_store: state.memory_store.clone(),
        embedder: state.embedder.clone(),
        registry: state.registry.clone(),
        current_agent_id: Some(db_agent.agent_id),
        workspace_id: Some(workspace_id),
        workspace_slug: Some(slug.clone()),
        workspace_git: Some(state.workspace_git.clone()),
        db: Some(state.db.clone()),
        gas_fees: Some(state.gas_fees.clone()),
        user_id: Some(user_id.to_string()),
        user_secrets: None,
    });

    // Execute with tool-aware executor
    let tool_executor = ToolAwareExecutor::new(
        state.registry.executor_arc(),
        ToolRegistry::with_workspace(),
        tool_context,
    );
    let output = tool_executor
        .execute(&agent_stmt, &context)
        .await
        .map_err(|e| format!("Agent execution failed: {:?}", e))?;

    // Extract response text
    let response_text = output
        .metadata
        .reasoning
        .clone()
        .unwrap_or_else(|| "(no response)".to_string());

    // Record episode with embedding
    let mut episode = agent_output_to_episode(db_agent.agent_id, query, &output);
    let embed_text = format!("{} {}", query, &response_text);
    if let Ok(embedding) = state.embedder.generate(&embed_text).await {
        episode.embedding = Some(embedding);
    }
    let _ = state.memory_store.store_episode(episode).await;

    // Store as workspace message
    let msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id,
        sender_type: "agent".to_string(),
        sender_id: db_agent.agent_id.to_string(),
        sender_name: Some(agent_name.to_string()),
        content: response_text.clone(),
        message_type: action_type.to_string(),
        metadata: json!({
            "action_type": action_type,
            "tokens_used": output.tokens_used,
            "confidence": output.confidence,
        }),
        created_at: chrono::Utc::now(),
    };
    let _ = state.memory_store.store_workspace_message(&msg).await;

    // Charge execution gas from workspace wallet + distribute to agents
    let tokens = output.tokens_used.unwrap_or(0) as i32;
    let (exec_fee, gas_fee) = state.gas_fees.execution_fee(tokens);
    let total = exec_fee + gas_fee;
    let agent_ids = get_workspace_agent_ids(&state.db, workspace_id).await;
    let ws_id_str = workspace_id.to_string();
    if let Ok(ws_wallet) = get_or_create_wallet(&state.db, "workspace", &ws_id_str).await {
        let _ = charge_and_distribute(
            &state.db,
            ws_wallet.wallet_id,
            total,
            "execution_fee",
            &format!("@{} {} ({}tk)", agent_name, action_type, tokens),
            &agent_ids,
            None,
            Some(workspace_id),
        )
        .await;
    }

    tracing::info!(
        agent = %agent_name,
        action = %action_type,
        workspace = %workspace_id,
        tokens = tokens,
        "Dispatched rabble action through agent"
    );

    Ok(response_text)
}

/// GET /api/me/workspace — return the caller's personal workspace (menagerie) details.
pub async fn get_personal_workspace_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let ws_id = sqlx::query("SELECT personal_workspace_id FROM users WHERE user_id = $1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .and_then(|r| {
            r.try_get::<Option<Uuid>, _>("personal_workspace_id")
                .ok()
                .flatten()
        });

    let ws_id = match ws_id {
        Some(id) => id,
        None => {
            return Ok(Json(json!({
                "workspace_id": null,
                "exists": false,
                "message": "No personal workspace yet. Mint a creature to create one."
            })));
        }
    };

    // Get workspace details
    let team = sqlx::query("SELECT id, name, slug, workspace_budget FROM teams WHERE id = $1")
        .bind(ws_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (name, slug, budget) = if let Some(t) = team {
        (
            t.try_get::<String, _>("name").unwrap_or_default(),
            t.try_get::<String, _>("slug").unwrap_or_default(),
            t.try_get::<i32, _>("workspace_budget").unwrap_or(0),
        )
    } else {
        return Err((StatusCode::NOT_FOUND, "Workspace not found".to_string()));
    };

    // Get workspace agents
    let agents = sqlx::query(
        "SELECT wa.agent_id, a.agent_name, a.display_alias, wa.relationship
         FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1",
    )
    .bind(ws_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let agent_list: Vec<Value> = agents
        .iter()
        .map(|r| {
            json!({
                "agent_id": r.try_get::<Uuid, _>("agent_id").ok(),
                "agent_name": r.try_get::<String, _>("agent_name").ok(),
                "display_alias": r.try_get::<Option<String>, _>("display_alias").ok().flatten(),
                "relationship": r.try_get::<String, _>("relationship").ok(),
            })
        })
        .collect();

    // Count episodes for workspace agents
    let episode_count: i64 = sqlx::query(
        "SELECT COUNT(*) as cnt FROM episodes WHERE agent_id = ANY(
            SELECT agent_id FROM workspace_agents WHERE workspace_id = $1
        )",
    )
    .bind(ws_id)
    .fetch_one(&state.db)
    .await
    .map(|r| r.try_get("cnt").unwrap_or(0))
    .unwrap_or(0);

    Ok(Json(json!({
        "workspace_id": ws_id,
        "exists": true,
        "name": name,
        "slug": slug,
        "budget": budget,
        "agents": agent_list,
        "episode_count": episode_count,
    })))
}

/// POST /api/rabble/:id/flock — compute one Reynolds flocking tick for all creatures in a rabble.
///
/// Gathers current creature positions from active flights, dispatches reynolds_flock agent,
/// returns updated positions + narration. Charges gas via workspace.
pub async fn flock_tick_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Path(swarm_id): axum::extract::Path<Uuid>,
    Json(params): Json<Option<FlockParams>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify swarm exists and get its workspace
    let swarm = sqlx::query("SELECT workspace_id, status FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Rabble not found".to_string()))?;

    let ws_id: Uuid = swarm
        .try_get::<Option<Uuid>, _>("workspace_id")
        .ok()
        .flatten()
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Rabble has no workspace — create one first".to_string(),
        ))?;

    // Check reynolds_flock agent is in this workspace
    let has_flock_agent = sqlx::query(
        "SELECT 1 FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1 AND a.agent_name = 'reynolds_flock'",
    )
    .bind(ws_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if has_flock_agent.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "reynolds_flock agent not hired in this rabble workspace. Hire it first.".to_string(),
        ));
    }

    // Gather all active creature flights in this swarm
    let flights = sqlx::query(
        "SELECT cf.creature_id, cf.center_lat, cf.center_lng, cf.flight_pattern,
                c.specimen_name, c.species_group,
                cf.path_samples
         FROM creature_flights cf
         JOIN creatures c ON c.creature_id = cf.creature_id
         WHERE cf.swarm_id = $1 AND cf.ended_at IS NULL
         ORDER BY cf.started_at ASC",
    )
    .bind(swarm_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if flights.is_empty() {
        return Ok(Json(json!({
            "swarm_id": swarm_id,
            "creatures": [],
            "message": "No active flights in this rabble"
        })));
    }

    // Build creature position array for the agent
    let creatures: Vec<Value> = flights
        .iter()
        .map(|f| {
            let path_samples: Option<serde_json::Value> = f.try_get("path_samples").ok();

            // Use last path sample if available, otherwise flight origin
            let (lat, lng, heading, speed) = if let Some(ref samples) = path_samples {
                if let Some(arr) = samples.as_array() {
                    if let Some(last) = arr.last() {
                        (
                            last.get("lat")
                                .and_then(|v| v.as_f64())
                                .unwrap_or_else(|| f.try_get("center_lat").unwrap_or(0.0)),
                            last.get("lng")
                                .and_then(|v| v.as_f64())
                                .unwrap_or_else(|| f.try_get("center_lng").unwrap_or(0.0)),
                            last.get("heading").and_then(|v| v.as_f64()).unwrap_or(0.0),
                            last.get("speed").and_then(|v| v.as_f64()).unwrap_or(1.0),
                        )
                    } else {
                        (
                            f.try_get("center_lat").unwrap_or(0.0),
                            f.try_get("center_lng").unwrap_or(0.0),
                            0.0,
                            1.0,
                        )
                    }
                } else {
                    (
                        f.try_get("center_lat").unwrap_or(0.0),
                        f.try_get("center_lng").unwrap_or(0.0),
                        0.0,
                        1.0,
                    )
                }
            } else {
                (
                    f.try_get("center_lat").unwrap_or(0.0),
                    f.try_get("center_lng").unwrap_or(0.0),
                    0.0,
                    1.0,
                )
            };

            json!({
                "id": f.try_get::<Uuid, _>("creature_id").ok(),
                "name": f.try_get::<String, _>("specimen_name").ok().unwrap_or_default(),
                "species": f.try_get::<String, _>("species_group").ok().unwrap_or_default(),
                "lat": lat,
                "lng": lng,
                "heading": heading,
                "speed": speed,
            })
        })
        .collect();

    // Build flocking params
    let flock_params = params.unwrap_or_default();
    let query = json!({
        "creatures": creatures,
        "params": {
            "separation_radius": flock_params.separation_radius,
            "alignment_radius": flock_params.alignment_radius,
            "cohesion_radius": flock_params.cohesion_radius,
            "max_speed": flock_params.max_speed,
            "separation_weight": flock_params.separation_weight,
            "alignment_weight": flock_params.alignment_weight,
            "cohesion_weight": flock_params.cohesion_weight,
        }
    });

    let query_str = format!(
        "Compute one Reynolds flocking tick for these creatures:\n{}",
        serde_json::to_string_pretty(&query).unwrap_or_default()
    );

    // Dispatch to reynolds_flock agent
    let response = dispatch_rabble_action(
        &state,
        ws_id,
        "reynolds_flock",
        "flock_tick",
        &query_str,
        &user_id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Try to parse agent response as JSON for structured output
    let parsed: Value = serde_json::from_str(&response).unwrap_or_else(|_| {
        json!({
            "raw_response": response,
            "note": "Agent response was not valid JSON"
        })
    });

    // If we got updated positions, store them as path samples on the flights
    if let Some(updated) = parsed.get("updated").and_then(|u| u.as_array()) {
        for update in updated {
            let creature_id = update
                .get("id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());
            let lat = update.get("lat").and_then(|v| v.as_f64());
            let lng = update.get("lng").and_then(|v| v.as_f64());
            let heading = update.get("heading").and_then(|v| v.as_f64());

            if let (Some(cid), Some(lat), Some(lng)) = (creature_id, lat, lng) {
                let sample = json!({
                    "lat": lat,
                    "lng": lng,
                    "heading": heading.unwrap_or(0.0),
                    "t": chrono::Utc::now().timestamp_millis(),
                });

                // Append to path_samples JSONB array
                let _ = sqlx::query(
                    "UPDATE creature_flights
                     SET path_samples = COALESCE(path_samples, '[]'::jsonb) || $1::jsonb
                     WHERE creature_id = $2 AND swarm_id = $3 AND ended_at IS NULL",
                )
                .bind(json!([sample]))
                .bind(cid)
                .bind(swarm_id)
                .execute(pool)
                .await;
            }
        }
    }

    // Broadcast flock update to rabble SSE
    let flock_event = json!({
        "type": "flock_tick",
        "swarm_id": swarm_id,
        "data": parsed,
    });
    let _ = state.rabble_broadcast.send(crate::RabbleEvent {
        swarm_id,
        message: flock_event,
    });

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "creature_count": creatures.len(),
        "result": parsed,
    })))
}

#[derive(serde::Deserialize)]
pub struct FlockParams {
    pub separation_radius: Option<f64>,
    pub alignment_radius: Option<f64>,
    pub cohesion_radius: Option<f64>,
    pub max_speed: Option<f64>,
    pub separation_weight: Option<f64>,
    pub alignment_weight: Option<f64>,
    pub cohesion_weight: Option<f64>,
}

impl Default for FlockParams {
    fn default() -> Self {
        Self {
            separation_radius: Some(0.0001), // ~11m
            alignment_radius: Some(0.0005),  // ~55m
            cohesion_radius: Some(0.001),    // ~111m
            max_speed: Some(2.0),
            separation_weight: Some(1.5),
            alignment_weight: Some(1.0),
            cohesion_weight: Some(1.0),
        }
    }
}

/// POST /api/rabble/:id/transfer-anchor — transfer the anchor to a different creature.
///
/// Only the rabble creator can transfer. The new creature must be actively flying at this rabble.
pub async fn transfer_anchor_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Path(swarm_id): axum::extract::Path<Uuid>,
    Json(req): Json<TransferAnchorRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify swarm exists and caller is creator
    let swarm = sqlx::query("SELECT creator_id, status FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Rabble not found".to_string()))?;

    let creator: String = swarm.get("creator_id");
    if creator != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the rabble creator can transfer the anchor".to_string(),
        ));
    }

    let status: String = swarm.get("status");
    if status != "scheduled" && status != "active" {
        return Err((
            StatusCode::CONFLICT,
            format!("Rabble is {} — cannot transfer anchor", status),
        ));
    }

    // Verify the new anchor creature is actively flying at this swarm
    let flight = sqlx::query(
        "SELECT cf.creature_id, c.specimen_name FROM creature_flights cf
         JOIN creatures c ON c.creature_id = cf.creature_id
         WHERE cf.creature_id = $1 AND cf.swarm_id = $2 AND cf.ended_at IS NULL",
    )
    .bind(req.new_anchor_creature_id)
    .bind(swarm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::BAD_REQUEST,
        "That creature is not actively flying at this rabble".to_string(),
    ))?;

    let creature_name: String = flight
        .try_get("specimen_name")
        .unwrap_or_else(|_| "Unknown".to_string());

    // Update the anchor
    sqlx::query(
        "UPDATE swarm_events SET anchor_creature_id = $1, anchor_transferred_at = NOW(),
         metadata = COALESCE(metadata, '{}'::jsonb) - 'anchor_departing'
         WHERE swarm_id = $2",
    )
    .bind(req.new_anchor_creature_id)
    .bind(swarm_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Post system message
    let _ = super::rabble_chat::insert_system_message(
        &state,
        swarm_id,
        &format!("Anchor transferred to {}!", creature_name),
    )
    .await;

    tracing::info!(
        swarm_id = %swarm_id,
        new_anchor = %req.new_anchor_creature_id,
        "Anchor creature transferred"
    );

    // Dispatch anchor_transferred to compound agents (fire-and-forget)
    let ws_id: Option<Uuid> =
        sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten());

    if let Some(ws_id) = ws_id {
        let state2 = state.clone();
        let user_id2 = user_id.clone();
        let c_name = creature_name.clone();
        tokio::spawn(async move {
            let query = format!(
                "anchor_transferred: Anchor has been transferred to {}. Update beacon placement at new position.",
                c_name
            );
            let _ = dispatch_rabble_action(
                &state2,
                ws_id,
                "rabble_anchor_manager",
                "anchor_transferred",
                &query,
                &user_id2,
            )
            .await;
            let _ = dispatch_rabble_action(
                &state2,
                ws_id,
                "rabble_lifecycle_coordinator",
                "anchor_transferred",
                &query,
                &user_id2,
            )
            .await;
        });
    }

    Ok(Json(json!({
        "success": true,
        "new_anchor_creature_id": req.new_anchor_creature_id,
        "creature_name": creature_name,
    })))
}

#[derive(serde::Deserialize)]
pub struct TransferAnchorRequest {
    pub new_anchor_creature_id: Uuid,
}

/// POST /api/rabble/:id/update-anchor-position — update the rabble's location from the anchor creature's GPS.
///
/// Called periodically by the client when the anchor creature is moving.
pub async fn update_anchor_position_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Path(swarm_id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateAnchorPositionRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Verify swarm exists and is active, and caller owns the anchor creature
    let swarm =
        sqlx::query("SELECT anchor_creature_id, status FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Rabble not found".to_string()))?;

    let status: String = swarm.get("status");
    if status != "active" && status != "scheduled" {
        return Err((
            StatusCode::CONFLICT,
            format!("Rabble is {} — cannot update position", status),
        ));
    }

    let anchor_id: Option<Uuid> = swarm
        .try_get::<Option<Uuid>, _>("anchor_creature_id")
        .ok()
        .flatten();

    let anchor_id = anchor_id.ok_or((
        StatusCode::BAD_REQUEST,
        "Rabble has no anchor creature".to_string(),
    ))?;

    // Verify caller owns the anchor creature
    let owner_check = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(anchor_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Anchor creature not found".to_string(),
        ))?;

    let owner: String = owner_check.get("owner_id");
    if owner != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the anchor creature's owner can update position".to_string(),
        ));
    }

    // Update the swarm's location
    sqlx::query(
        "UPDATE swarm_events SET center_lat = $1, center_lng = $2, h3_cell = $3
         WHERE swarm_id = $4",
    )
    .bind(req.lat)
    .bind(req.lng)
    .bind(&req.h3_cell)
    .bind(swarm_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Broadcast position update to connected clients
    let _ = state.rabble_broadcast.send(crate::RabbleEvent {
        swarm_id,
        message: json!({
            "type": "anchor_position_update",
            "swarm_id": swarm_id,
            "lat": req.lat,
            "lng": req.lng,
            "h3_cell": req.h3_cell,
        }),
    });

    Ok(Json(json!({
        "success": true,
        "lat": req.lat,
        "lng": req.lng,
    })))
}

#[derive(serde::Deserialize)]
pub struct UpdateAnchorPositionRequest {
    pub lat: f64,
    pub lng: f64,
    pub h3_cell: String,
}

/// POST /api/swarms/:swarm_id/join-batch — join a rabble with multiple creatures as a sub-flock.
///
/// Creates a sub-flock group and joins all specified creatures in one operation.
/// Gas fee charged once for the group, not per creature.
pub async fn join_batch_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Path(swarm_id): axum::extract::Path<Uuid>,
    Json(req): Json<JoinBatchRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    if req.creature_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No creatures specified".to_string(),
        ));
    }

    // Verify swarm exists and is joinable
    let swarm = sqlx::query(
        "SELECT status, h3_cell, center_lat, center_lng, creator_id, visibility,
         funding_mode, invite_pool_remaining, anchor_creature_id
         FROM swarm_events WHERE swarm_id = $1",
    )
    .bind(swarm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Rabble not found".to_string()))?;

    let status: String = swarm.get("status");
    if status != "scheduled" && status != "active" {
        return Err((StatusCode::CONFLICT, format!("Rabble is {}", status)));
    }

    // Verify all creatures belong to the user and are active
    for cid in &req.creature_ids {
        let creature = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
            .bind(cid)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, format!("Creature {} not found", cid)))?;

        let owner: String = creature.get("owner_id");
        if owner != user_id {
            return Err((
                StatusCode::FORBIDDEN,
                format!("Creature {} is not yours", cid),
            ));
        }

        // Check not already flying at this swarm
        let existing = sqlx::query(
            "SELECT 1 FROM creature_flights WHERE creature_id = $1 AND swarm_id = $2 AND ended_at IS NULL",
        )
        .bind(cid)
        .bind(swarm_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if existing.is_some() {
            return Err((
                StatusCode::CONFLICT,
                format!("Creature {} already in this rabble", cid),
            ));
        }
    }

    // Handle funding: charge once for the group
    let funding_mode: String = swarm.try_get("funding_mode").unwrap_or("hosted".into());
    let count = req.creature_ids.len() as i32;

    if funding_mode == "hosted" {
        let remaining: i32 = swarm.try_get("invite_pool_remaining").unwrap_or(0);
        if remaining < count {
            return Err((
                StatusCode::PAYMENT_REQUIRED,
                "Invite pool insufficient for batch".into(),
            ));
        }
        sqlx::query("UPDATE swarm_events SET invite_pool_remaining = invite_pool_remaining - $1 WHERE swarm_id = $2")
            .bind(count)
            .bind(swarm_id)
            .execute(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        let contribution = req.contribution.unwrap_or(1).max(1);
        let wallet = fermi_auth::get_or_create_wallet(pool, "user", &user_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        charge_gas(
            pool,
            wallet.wallet_id,
            contribution,
            "swarm_join",
            &format!("Batch join rabble {} ({} creatures)", swarm_id, count),
            Some(&swarm_id.to_string()),
        )
        .await?;
    }

    // Create sub-flock if name provided
    let sub_flock_id = if let Some(ref name) = req.sub_flock_name {
        let sf_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO swarm_sub_flocks (sub_flock_id, swarm_id, owner_id, name, species_filter)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(sf_id)
        .bind(swarm_id)
        .bind(&user_id)
        .bind(name)
        .bind(&req.species_filter)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Some(sf_id)
    } else {
        None
    };

    // Get swarm location for flight records
    let h3_cell: String = swarm.get("h3_cell");
    let lat: f64 = swarm.get("center_lat");
    let lng: f64 = swarm.get("center_lng");
    let anchor_id: Option<Uuid> = swarm
        .try_get::<Option<Uuid>, _>("anchor_creature_id")
        .ok()
        .flatten();

    // Join each creature
    for cid in &req.creature_ids {
        let flight_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
             h3_cell, h3_resolution, center_lat, center_lng,
             flight_pattern, swarm_id, sub_flock_id, attracted_by_creature_id, started_at)
             VALUES ($1, $2, $3, $4, 12, $5, $6, 'swarm', $7, $8, $9, NOW())",
        )
        .bind(flight_id)
        .bind(cid)
        .bind(&user_id)
        .bind(&h3_cell)
        .bind(lat)
        .bind(lng)
        .bind(swarm_id)
        .bind(sub_flock_id)
        .bind(req.attracted_by_creature_id.or(anchor_id))
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Increment swarm counters
    sqlx::query(
        "UPDATE swarm_events SET participant_count = participant_count + 1,
         creature_count = creature_count + $1
         WHERE swarm_id = $2",
    )
    .bind(count)
    .bind(swarm_id)
    .execute(pool)
    .await
    .ok();

    // Attraction reward: credit the attractor's owner
    let attractor_id = req.attracted_by_creature_id.or(anchor_id);
    if let Some(attr_id) = attractor_id {
        let attr_owner = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
            .bind(attr_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

        if let Some(row) = attr_owner {
            let attr_owner_id: String = row.get("owner_id");
            if attr_owner_id != user_id {
                // Credit the attractor's owner: 1 credit per creature joined
                if let Ok(attr_wallet) =
                    fermi_auth::get_or_create_wallet(pool, "user", &attr_owner_id).await
                {
                    let _ = fermi_auth::credit_deposit_typed(
                        pool,
                        attr_wallet.wallet_id,
                        count,
                        "attraction_reward",
                        &format!("Attraction: {} creatures joined via your creature", count),
                    )
                    .await;
                }
                // Increment attraction score
                let _ = sqlx::query(
                    "UPDATE creatures SET attraction_score = attraction_score + $1 WHERE creature_id = $2",
                )
                .bind(count)
                .bind(attr_id)
                .execute(pool)
                .await;
            }
        }
    }

    // Post system message
    let _ = super::rabble_chat::insert_system_message(
        &state,
        swarm_id,
        &format!("{} creatures have joined the rabble as a flock!", count),
    )
    .await;

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "sub_flock_id": sub_flock_id,
        "creatures_joined": count,
    })))
}

#[derive(serde::Deserialize)]
pub struct JoinBatchRequest {
    pub creature_ids: Vec<Uuid>,
    pub sub_flock_name: Option<String>,
    pub species_filter: Option<String>,
    pub contribution: Option<i32>,
    pub attracted_by_creature_id: Option<Uuid>,
}

/// GET /api/rabble/:id/attraction-leaderboard — which creatures attracted the most participants.
pub async fn attraction_leaderboard_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    axum::extract::Path(swarm_id): axum::extract::Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let pool = &state.db;

    let rows = sqlx::query(
        "SELECT cf.attracted_by_creature_id, c.specimen_name, c.species_group, c.owner_id,
                COUNT(*) as attracted_count
         FROM creature_flights cf
         JOIN creatures c ON c.creature_id = cf.attracted_by_creature_id
         WHERE cf.swarm_id = $1 AND cf.attracted_by_creature_id IS NOT NULL
         GROUP BY cf.attracted_by_creature_id, c.specimen_name, c.species_group, c.owner_id
         ORDER BY attracted_count DESC
         LIMIT 20",
    )
    .bind(swarm_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let leaderboard: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "creature_id": r.try_get::<Uuid, _>("attracted_by_creature_id").ok(),
                "creature_name": r.try_get::<String, _>("specimen_name").ok(),
                "species_group": r.try_get::<String, _>("species_group").ok(),
                "owner_id": r.try_get::<String, _>("owner_id").ok(),
                "attracted_count": r.try_get::<i64, _>("attracted_count").unwrap_or(0),
            })
        })
        .collect();

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "leaderboard": leaderboard,
    })))
}

/// GET /api/rabble/:id/flock-history — returns all creature path data normalized to XY for visualization.
///
/// Takes lat/lng path_samples from all creature flights in the swarm, normalizes them
/// relative to the swarm centroid so they fit in a [0,1] x [0,1] viewport. Each creature
/// gets a color and a trail of (x, y, t) points.
///
/// Charges platform_read gas (1 credit). Agents already got paid when they processed
/// the flights — this just covers infrastructure cost of serving the data.
pub async fn flock_history_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Path(swarm_id): axum::extract::Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Charge platform read gas — no agent payout, just infrastructure
    let user_wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;
    charge_gas(
        pool,
        user_wallet.wallet_id,
        state.gas_fees.platform_read,
        "platform_read",
        "Flock visualization data",
        Some(&swarm_id.to_string()),
    )
    .await?;

    // Get swarm center as reference point
    let swarm =
        sqlx::query("SELECT center_lat, center_lng, name FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Rabble not found".to_string()))?;

    let center_lat: f64 = swarm.try_get("center_lat").unwrap_or(0.0);
    let center_lng: f64 = swarm.try_get("center_lng").unwrap_or(0.0);
    let swarm_name: String = swarm.try_get("name").unwrap_or_default();

    // Get all flights in this swarm with their path_samples
    let flights = sqlx::query(
        "SELECT cf.creature_id, cf.center_lat, cf.center_lng, cf.path_samples, cf.started_at,
                c.specimen_name, c.scientific_name, c.species_group, c.owner_id,
                c.asset_path,
                cf.sub_flock_id, sf.name AS sub_flock_name,
                c.attraction_score
         FROM creature_flights cf
         JOIN creatures c ON c.creature_id = cf.creature_id
         LEFT JOIN swarm_sub_flocks sf ON sf.sub_flock_id = cf.sub_flock_id
         WHERE cf.swarm_id = $1 AND cf.ended_at IS NULL
         ORDER BY cf.started_at ASC",
    )
    .bind(swarm_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if flights.is_empty() {
        return Ok(Json(json!({
            "swarm_id": swarm_id,
            "swarm_name": swarm_name,
            "creatures": [],
            "bounds": { "min_x": 0, "max_x": 1, "min_y": 0, "max_y": 1 },
        })));
    }

    // Collect all lat/lng points to compute bounds for normalization
    let mut all_lats: Vec<f64> = vec![center_lat];
    let mut all_lngs: Vec<f64> = vec![center_lng];

    let colors = [
        "#FF6B6B", "#4ECDC4", "#45B7D1", "#96CEB4", "#FFEAA7", "#DDA0DD", "#98D8C8", "#F7DC6F",
        "#BB8FCE", "#85C1E9",
    ];

    let mut creature_data: Vec<Value> = Vec::new();

    for (i, flight) in flights.iter().enumerate() {
        let creature_id: Uuid = flight.try_get("creature_id").unwrap_or_default();
        let name: String = flight.try_get("specimen_name").unwrap_or_default();
        let scientific_name: String = flight.try_get("scientific_name").unwrap_or_default();
        let species: String = flight.try_get("species_group").unwrap_or_default();
        let owner_id: String = flight.try_get("owner_id").unwrap_or_default();
        let asset_path: String = flight.try_get("asset_path").unwrap_or_default();
        let origin_lat: f64 = flight.try_get("center_lat").unwrap_or(center_lat);
        let origin_lng: f64 = flight.try_get("center_lng").unwrap_or(center_lng);
        let color = colors[i % colors.len()];

        // Build points array: origin + path_samples
        let mut points: Vec<Value> = vec![json!({
            "lat": origin_lat, "lng": origin_lng, "t": 0,
        })];

        all_lats.push(origin_lat);
        all_lngs.push(origin_lng);

        if let Ok(Some(samples)) = flight.try_get::<Option<serde_json::Value>, _>("path_samples") {
            if let Some(arr) = samples.as_array() {
                for s in arr {
                    let lat = s.get("lat").and_then(|v| v.as_f64()).unwrap_or(origin_lat);
                    let lng = s.get("lng").and_then(|v| v.as_f64()).unwrap_or(origin_lng);
                    let t = s.get("t").and_then(|v| v.as_i64()).unwrap_or(0);
                    points.push(json!({ "lat": lat, "lng": lng, "t": t }));
                    all_lats.push(lat);
                    all_lngs.push(lng);
                }
            }
        }

        let sub_flock_id: Option<Uuid> = flight.try_get("sub_flock_id").ok();
        let sub_flock_name: Option<String> = flight.try_get("sub_flock_name").ok().flatten();
        let attraction_score: i32 = flight.try_get("attraction_score").unwrap_or(0);

        creature_data.push(json!({
            "creature_id": creature_id,
            "owner_id": owner_id,
            "name": name,
            "scientific_name": scientific_name,
            "species": species,
            "color": color,
            "image_url": if asset_path.is_empty() {
                format!("/api/creatures/{}/image", creature_id)
            } else {
                asset_path.clone()
            },
            "points": points,
            "sub_flock_id": sub_flock_id,
            "sub_flock_name": sub_flock_name,
            "attraction_score": attraction_score,
        }));
    }

    // Compute bounds for normalization
    let min_lat = all_lats.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_lat = all_lats.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_lng = all_lngs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_lng = all_lngs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // Add padding (10%) so dots aren't on edges
    let lat_range = (max_lat - min_lat).max(0.0001); // min range to avoid division by zero
    let lng_range = (max_lng - min_lng).max(0.0001);
    let pad_lat = lat_range * 0.1;
    let pad_lng = lng_range * 0.1;
    let norm_min_lat = min_lat - pad_lat;
    let norm_max_lat = max_lat + pad_lat;
    let norm_min_lng = min_lng - pad_lng;
    let norm_max_lng = max_lng + pad_lng;
    let norm_lat_range = norm_max_lat - norm_min_lat;
    let norm_lng_range = norm_max_lng - norm_min_lng;

    // Normalize all points to [0, 1] x [0, 1]
    for creature in &mut creature_data {
        if let Some(points) = creature.get_mut("points").and_then(|p| p.as_array_mut()) {
            for point in points.iter_mut() {
                let lat = point.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let lng = point.get("lng").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let x = (lng - norm_min_lng) / norm_lng_range;
                let y = 1.0 - (lat - norm_min_lat) / norm_lat_range; // invert Y so north is up
                point.as_object_mut().map(|m| {
                    m.insert("x".to_string(), json!(x));
                    m.insert("y".to_string(), json!(y));
                });
            }
        }
    }

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "swarm_name": swarm_name,
        "center": { "lat": center_lat, "lng": center_lng },
        "creatures": creature_data,
        "bounds": {
            "min_lat": min_lat, "max_lat": max_lat,
            "min_lng": min_lng, "max_lng": max_lng,
            "lat_range_m": lat_range * 111_000.0,
            "lng_range_m": lng_range * 111_000.0 * (center_lat.to_radians().cos()),
        },
    })))
}
