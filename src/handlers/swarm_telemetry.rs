//! Swarm telemetry ingestion API — external drone platforms POST telemetry here.
//!
//! Schema follows Onto4MAT ontology (arxiv 2203.12955) data properties:
//! Agent (speed, heading, energy, position), Team (alignment, cohesion, separation),
//! Formation (arc, echelon, line, v, wedge), Actions, Temperament.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::super::AppState;
use crate::agent_output_to_episode;
use fermi::agent_backend::ExecutionContext;
use fermi::ast;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

// ─── Request types ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub name: String,
    pub description: Option<String>,
    pub agent_count: Option<i32>,
    pub formation_type: Option<String>,
    pub mission_type: Option<String>,
    pub environment: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct TelemetryBatch {
    pub samples: Vec<TelemetrySample>,
}

#[derive(Deserialize)]
pub struct TelemetrySample {
    pub agent_label: String,
    pub agent_type: Option<String>,
    pub timestamp_ms: i64,
    pub x_location: f64,
    pub y_location: f64,
    pub z_location: Option<f64>,
    pub heading: Option<f64>,
    pub speed: Option<f64>,
    pub energy: Option<f64>,
    pub distance_to_goal: Option<f64>,
    pub team_alignment: Option<f64>,
    pub team_cohesion: Option<f64>,
    pub team_separation: Option<f64>,
    pub influence: Option<f64>,
    pub action: Option<String>,
    pub temperament: Option<String>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct TelemetryQuery {
    pub agent_label: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct SessionListQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

// ─── POST /api/swarm/sessions ──────────────────────────────────────

pub async fn create_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.swarm_session_create,
        "swarm_session_create",
        &format!("Create swarm session: {}", req.name),
        None,
    )
    .await?;

    let session_id = Uuid::new_v4();
    let agent_count = req.agent_count.unwrap_or(0);
    let environment = req.environment.unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO swarm_sessions (session_id, owner_id, name, description, agent_count, formation_type, mission_type, environment)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    )
    .bind(session_id)
    .bind(&user_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(agent_count)
    .bind(&req.formation_type)
    .bind(&req.mission_type)
    .bind(&environment)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "session_id": session_id,
        "name": req.name,
        "agent_count": agent_count,
        "status": "active",
        "started_at": chrono::Utc::now()
    })))
}

// ─── GET /api/swarm/sessions ───────────────────────────────────────

pub async fn list_sessions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<SessionListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let limit = q.limit.unwrap_or(50).min(200);

    let rows = if let Some(status) = &q.status {
        sqlx::query(
            "SELECT session_id, name, description, agent_count, formation_type, mission_type, status, started_at, ended_at, metadata
             FROM swarm_sessions WHERE owner_id = $1 AND status = $2
             ORDER BY started_at DESC LIMIT $3"
        )
        .bind(&user_id)
        .bind(status)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT session_id, name, description, agent_count, formation_type, mission_type, status, started_at, ended_at, metadata
             FROM swarm_sessions WHERE owner_id = $1
             ORDER BY started_at DESC LIMIT $2"
        )
        .bind(&user_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sessions: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "session_id": r.get::<Uuid, _>("session_id"),
                "name": r.get::<String, _>("name"),
                "description": r.get::<Option<String>, _>("description"),
                "agent_count": r.get::<i32, _>("agent_count"),
                "formation_type": r.get::<Option<String>, _>("formation_type"),
                "mission_type": r.get::<Option<String>, _>("mission_type"),
                "status": r.get::<String, _>("status"),
                "started_at": r.get::<chrono::DateTime<chrono::Utc>, _>("started_at"),
                "ended_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "sessions": sessions })))
}

// ─── GET /api/swarm/sessions/:id ───────────────────────────────────

pub async fn get_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let row = sqlx::query(
        "SELECT session_id, owner_id, name, description, agent_count, formation_type, mission_type, environment, status, started_at, ended_at, metadata
         FROM swarm_sessions WHERE session_id = $1"
    )
    .bind(session_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let owner: String = row.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your session".to_string()));
    }

    Ok(Json(json!({
        "session_id": row.get::<Uuid, _>("session_id"),
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
        "agent_count": row.get::<i32, _>("agent_count"),
        "formation_type": row.get::<Option<String>, _>("formation_type"),
        "mission_type": row.get::<Option<String>, _>("mission_type"),
        "environment": row.get::<serde_json::Value, _>("environment"),
        "status": row.get::<String, _>("status"),
        "started_at": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at"),
        "ended_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at"),
        "metadata": row.get::<serde_json::Value, _>("metadata"),
    })))
}

// ─── PUT /api/swarm/sessions/:id/end ───────────────────────────────

pub async fn end_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let result = sqlx::query(
        "UPDATE swarm_sessions SET status = 'completed', ended_at = NOW()
         WHERE session_id = $1 AND owner_id = $2 AND status = 'active'
         RETURNING session_id, ended_at",
    )
    .bind(session_id)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match result {
        Some(row) => Ok(Json(json!({
            "session_id": row.get::<Uuid, _>("session_id"),
            "status": "completed",
            "ended_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ended_at"),
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            "Session not found or already ended".to_string(),
        )),
    }
}

// ─── POST /api/swarm/sessions/:id/telemetry ────────────────────────

pub async fn ingest_telemetry_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
    Json(batch): Json<TelemetryBatch>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    if batch.samples.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Empty batch".to_string()));
    }

    if batch.samples.len() > 10_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Batch too large (max 10000 samples)".to_string(),
        ));
    }

    // Verify session ownership and active status
    let session = sqlx::query("SELECT owner_id, status FROM swarm_sessions WHERE session_id = $1")
        .bind(session_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let owner: String = session.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your session".to_string()));
    }

    let status: String = session.get("status");
    if status != "active" {
        return Err((StatusCode::CONFLICT, "Session is not active".to_string()));
    }

    // Charge gas
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.swarm_telemetry_ingest,
        "swarm_telemetry_ingest",
        &format!(
            "Ingest {} telemetry samples to session {}",
            batch.samples.len(),
            session_id
        ),
        Some(&session_id.to_string()),
    )
    .await?;

    // Batch insert — build multi-row VALUES
    let mut query = String::from(
        "INSERT INTO swarm_telemetry (telemetry_id, session_id, agent_label, agent_type, timestamp_ms, x_location, y_location, z_location, heading, speed, energy, distance_to_goal, team_alignment, team_cohesion, team_separation, influence, action, temperament, extra) VALUES "
    );

    let mut param_idx = 1u32;
    let sample_count = batch.samples.len();

    for (i, _) in batch.samples.iter().enumerate() {
        if i > 0 {
            query.push_str(", ");
        }
        query.push('(');
        for j in 0..19u32 {
            if j > 0 {
                query.push_str(", ");
            }
            query.push_str(&format!("${}", param_idx));
            param_idx += 1;
        }
        query.push(')');
    }

    // Pre-compute derived values so they live long enough for bind references
    let derived: Vec<(Uuid, String, f64, serde_json::Value)> = batch
        .samples
        .iter()
        .map(|s| {
            (
                Uuid::new_v4(),
                s.agent_type
                    .clone()
                    .unwrap_or_else(|| "artificial".to_string()),
                s.z_location.unwrap_or(0.0),
                s.extra.clone().unwrap_or(json!({})),
            )
        })
        .collect();

    let mut q = sqlx::query(&query);

    for (i, sample) in batch.samples.iter().enumerate() {
        let (ref tid, ref atype, z, ref extra) = derived[i];

        q = q
            .bind(*tid)
            .bind(session_id)
            .bind(&sample.agent_label)
            .bind(atype.as_str())
            .bind(sample.timestamp_ms)
            .bind(sample.x_location)
            .bind(sample.y_location)
            .bind(z)
            .bind(sample.heading)
            .bind(sample.speed)
            .bind(sample.energy)
            .bind(sample.distance_to_goal)
            .bind(sample.team_alignment)
            .bind(sample.team_cohesion)
            .bind(sample.team_separation)
            .bind(sample.influence)
            .bind(&sample.action)
            .bind(&sample.temperament)
            .bind(extra);
    }

    q.execute(&state.db).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Insert failed: {}", e),
        )
    })?;

    // Update agent_count on session if we see new agents
    let distinct_agents: Vec<String> = batch
        .samples
        .iter()
        .map(|s| s.agent_label.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    sqlx::query(
        "UPDATE swarm_sessions SET agent_count = GREATEST(agent_count, $1) WHERE session_id = $2",
    )
    .bind(distinct_agents.len() as i32)
    .bind(session_id)
    .execute(&state.db)
    .await
    .ok(); // Non-critical update

    // Auto-execute swarm_coordinator to analyze the batch (fire-and-forget)
    {
        let spawn_state = state.clone();
        let agents = distinct_agents.clone();
        let n = sample_count;
        tokio::spawn(async move {
            let coordinator_id = "swarm_coordinator";
            let card = match spawn_state.registry.get(coordinator_id) {
                Ok(c) => c,
                Err(_) => return, // coordinator not available
            };

            let summary_prompt = format!(
                "Telemetry batch ingested for session {}.\n\
                 Samples: {}, Distinct agents: {:?}.\n\
                 Analyze the latest telemetry. Query the session summary for aggregate metrics, \
                 identify formation patterns, flag anomalies (energy < 0.2, high separation), \
                 and recommend corrective actions using Onto4MAT terminology.",
                session_id, n, agents
            );

            let agent_stmt = ast::AgentStmt {
                name: coordinator_id.to_string(),
                agent_type: Some(card.agent_type.clone()),
                query: summary_prompt.clone(),
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
                creature_id: None,
                cognition_tier: None,
            };

            match spawn_state
                .registry
                .execute_agent(&agent_stmt, &context)
                .await
            {
                Ok(output) => {
                    // Look up agent DB id for episode storage
                    let db_id = sqlx::query(
                        "SELECT agent_id FROM agents WHERE LOWER(name) = LOWER($1) OR agent_id::TEXT = $1 LIMIT 1"
                    )
                    .bind(coordinator_id)
                    .fetch_optional(&spawn_state.db)
                    .await
                    .ok()
                    .flatten();

                    if let Some(row) = db_id {
                        let agent_uuid: Uuid = row.get("agent_id");
                        let mut episode =
                            agent_output_to_episode(agent_uuid, &summary_prompt, &output);

                        // Generate embedding for the analysis
                        let embed_text = format!(
                            "{} {}",
                            summary_prompt,
                            output.metadata.reasoning.as_deref().unwrap_or("")
                        );
                        if let Ok(embedding) = spawn_state.embedder.generate(&embed_text).await {
                            episode.embedding = Some(embedding);
                        }

                        let _ = spawn_state.memory_store.store_episode(episode).await;
                    }

                    eprintln!(
                        "Swarm coordinator: analyzed batch for session {} ({} samples)",
                        session_id, n
                    );
                }
                Err(e) => {
                    eprintln!(
                        "Swarm coordinator failed for session {}: {:?}",
                        session_id, e
                    );
                }
            }
        });
    }

    Ok(Json(json!({
        "session_id": session_id,
        "samples_ingested": sample_count,
        "distinct_agents": distinct_agents,
    })))
}

// ─── GET /api/swarm/sessions/:id/telemetry ─────────────────────────

pub async fn query_telemetry_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
    Query(q): Query<TelemetryQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Verify ownership
    let session = sqlx::query("SELECT owner_id FROM swarm_sessions WHERE session_id = $1")
        .bind(session_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let owner: String = session.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your session".to_string()));
    }

    let limit = q.limit.unwrap_or(1000).min(10_000);
    let offset = q.offset.unwrap_or(0);

    // Build dynamic query with optional filters
    let mut sql = String::from(
        "SELECT telemetry_id, agent_label, agent_type, timestamp_ms, x_location, y_location, z_location, heading, speed, energy, distance_to_goal, team_alignment, team_cohesion, team_separation, influence, action, temperament, extra
         FROM swarm_telemetry WHERE session_id = $1"
    );
    let mut param_idx = 2u32;
    let mut conditions = Vec::new();

    if q.agent_label.is_some() {
        conditions.push(format!(" AND agent_label = ${}", param_idx));
        param_idx += 1;
    }
    if q.from_ms.is_some() {
        conditions.push(format!(" AND timestamp_ms >= ${}", param_idx));
        param_idx += 1;
    }
    if q.to_ms.is_some() {
        conditions.push(format!(" AND timestamp_ms <= ${}", param_idx));
        param_idx += 1;
    }

    for c in &conditions {
        sql.push_str(c);
    }
    sql.push_str(&format!(
        " ORDER BY timestamp_ms ASC LIMIT ${} OFFSET ${}",
        param_idx,
        param_idx + 1
    ));

    let mut query = sqlx::query(&sql).bind(session_id);

    if let Some(ref label) = q.agent_label {
        query = query.bind(label);
    }
    if let Some(from) = q.from_ms {
        query = query.bind(from);
    }
    if let Some(to) = q.to_ms {
        query = query.bind(to);
    }
    query = query.bind(limit).bind(offset);

    let rows = query
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let samples: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "telemetry_id": r.get::<Uuid, _>("telemetry_id"),
                "agent_label": r.get::<String, _>("agent_label"),
                "agent_type": r.get::<String, _>("agent_type"),
                "timestamp_ms": r.get::<i64, _>("timestamp_ms"),
                "x_location": r.get::<f64, _>("x_location"),
                "y_location": r.get::<f64, _>("y_location"),
                "z_location": r.get::<Option<f64>, _>("z_location"),
                "heading": r.get::<Option<f64>, _>("heading"),
                "speed": r.get::<Option<f64>, _>("speed"),
                "energy": r.get::<Option<f64>, _>("energy"),
                "distance_to_goal": r.get::<Option<f64>, _>("distance_to_goal"),
                "team_alignment": r.get::<Option<f64>, _>("team_alignment"),
                "team_cohesion": r.get::<Option<f64>, _>("team_cohesion"),
                "team_separation": r.get::<Option<f64>, _>("team_separation"),
                "influence": r.get::<Option<f64>, _>("influence"),
                "action": r.get::<Option<String>, _>("action"),
                "temperament": r.get::<Option<String>, _>("temperament"),
                "extra": r.get::<serde_json::Value, _>("extra"),
            })
        })
        .collect();

    Ok(Json(json!({
        "session_id": session_id,
        "count": samples.len(),
        "samples": samples,
    })))
}

// ─── GET /api/swarm/sessions/:id/summary ───────────────────────────

pub async fn session_summary_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Get session info
    let session = sqlx::query(
        "SELECT owner_id, name, agent_count, formation_type, status, started_at, ended_at
         FROM swarm_sessions WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let owner: String = session.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your session".to_string()));
    }

    // Aggregated metrics
    let stats = sqlx::query(
        "SELECT
            COUNT(*)::BIGINT as sample_count,
            COUNT(DISTINCT agent_label)::INT as distinct_agents,
            MIN(timestamp_ms) as min_ts,
            MAX(timestamp_ms) as max_ts,
            AVG(speed) as avg_speed,
            AVG(energy) as avg_energy,
            AVG(team_alignment) as avg_team_alignment,
            AVG(team_cohesion) as avg_team_cohesion,
            AVG(team_separation) as avg_team_separation
         FROM swarm_telemetry WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Distinct agent labels
    let agent_rows = sqlx::query(
        "SELECT DISTINCT agent_label FROM swarm_telemetry WHERE session_id = $1 ORDER BY agent_label"
    )
    .bind(session_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agents: Vec<String> = agent_rows.iter().map(|r| r.get("agent_label")).collect();

    // Low energy anomalies
    let low_energy_rows = sqlx::query(
        "SELECT DISTINCT agent_label FROM swarm_telemetry
         WHERE session_id = $1 AND energy IS NOT NULL AND energy < 0.2",
    )
    .bind(session_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let low_energy: Vec<String> = low_energy_rows
        .iter()
        .map(|r| r.get("agent_label"))
        .collect();

    let sample_count: i64 = stats.get("sample_count");
    let min_ts: Option<i64> = stats.get("min_ts");
    let max_ts: Option<i64> = stats.get("max_ts");
    let duration_ms = match (min_ts, max_ts) {
        (Some(a), Some(b)) => b - a,
        _ => 0,
    };

    Ok(Json(json!({
        "session_id": session_id,
        "name": session.get::<String, _>("name"),
        "status": session.get::<String, _>("status"),
        "formation_type": session.get::<Option<String>, _>("formation_type"),
        "agent_count": stats.get::<Option<i32>, _>("distinct_agents"),
        "sample_count": sample_count,
        "duration_ms": duration_ms,
        "agents": agents,
        "avg_speed": stats.get::<Option<f64>, _>("avg_speed"),
        "avg_energy": stats.get::<Option<f64>, _>("avg_energy"),
        "avg_team_alignment": stats.get::<Option<f64>, _>("avg_team_alignment"),
        "avg_team_cohesion": stats.get::<Option<f64>, _>("avg_team_cohesion"),
        "avg_team_separation": stats.get::<Option<f64>, _>("avg_team_separation"),
        "anomalies": {
            "low_energy": low_energy,
        },
    })))
}

// ─── GET /api/swarm/sessions/:id/experience ────────────────────────
//
// Returns the consolidated experience lookup table:
// embedding vectors from swarm_coordinator episodes paired with
// the telemetry context (state) and recommended actions.
//
// A drone downloads this and does nearest-neighbor at runtime:
//   current_state → embed → cosine_similarity → best_action
//
// No ML training needed — each flight adds experiences, the table
// grows, and nearest-neighbor coverage improves automatically.

#[derive(Deserialize)]
pub struct ExperienceQuery {
    pub limit: Option<i64>,
}

pub async fn experience_export_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
    Query(q): Query<ExperienceQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Verify session ownership
    let session = sqlx::query("SELECT owner_id FROM swarm_sessions WHERE session_id = $1")
        .bind(session_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let owner: String = session.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your session".to_string()));
    }

    // Find swarm_coordinator agent DB id
    let coordinator_row =
        sqlx::query("SELECT agent_id FROM agents WHERE LOWER(name) = 'swarm_coordinator' LIMIT 1")
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let coordinator_id: Uuid = match coordinator_row {
        Some(row) => row.get("agent_id"),
        None => {
            return Ok(Json(json!({
                "session_id": session_id,
                "experiences": [],
                "note": "swarm_coordinator agent not found — no episodes to export"
            })));
        }
    };

    let limit = q.limit.unwrap_or(500).min(5000);

    // Fetch episodes with embeddings from swarm_coordinator that reference this session
    let rows = sqlx::query(
        "SELECT episode_id, query, context, embedding::TEXT as embedding_text,
                execution_status, timestamp_created
         FROM episodes
         WHERE agent_id = $1
           AND embedding IS NOT NULL
           AND query LIKE '%' || $2 || '%'
         ORDER BY timestamp_created DESC
         LIMIT $3",
    )
    .bind(coordinator_id)
    .bind(session_id.to_string())
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let experiences: Vec<serde_json::Value> = rows
        .iter()
        .filter_map(|r| {
            let embedding_text: Option<String> = r.get("embedding_text");
            let embedding_vec: Vec<f64> = embedding_text
                .as_deref()
                .unwrap_or("")
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .collect();

            if embedding_vec.is_empty() {
                return None;
            }

            let context: serde_json::Value = r.get("context");

            // Extract reasoning/output from context as the "action recommendation"
            let action = context
                .get("reasoning")
                .and_then(|v| v.as_str())
                .or_else(|| context.get("output").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();

            Some(json!({
                "episode_id": r.get::<Uuid, _>("episode_id"),
                "embedding": embedding_vec,
                "query": r.get::<String, _>("query"),
                "action": action,
                "status": r.get::<String, _>("execution_status"),
                "timestamp": r.get::<chrono::DateTime<chrono::Utc>, _>("timestamp_created"),
            }))
        })
        .collect();

    Ok(Json(json!({
        "session_id": session_id,
        "coordinator_agent_id": coordinator_id,
        "experience_count": experiences.len(),
        "embedding_dim": 1024,
        "usage": "Nearest-neighbor lookup: embed current drone state, find closest experience, adopt its action.",
        "experiences": experiences,
    })))
}
