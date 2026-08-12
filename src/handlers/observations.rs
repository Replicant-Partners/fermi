//! Universal sensor observation API — W3C SSN/SOSA vocabulary.
//!
//! Domain-agnostic telemetry ingestion. Any sensor platform (drone, weather
//! station, greenhouse, wearable, vehicle) can POST observations here using
//! the SOSA property-value pattern. The observation_analyst agent auto-analyzes
//! each batch, producing episodes with embeddings that feed the experience
//! lookup table — same learning loop as swarm telemetry, but universal.
//!
//! See: https://www.w3.org/TR/vocab-ssn/

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
pub struct CreatePlatformRequest {
    pub name: String,
    pub platform_type: String,
    pub description: Option<String>,
    pub location: Option<serde_json::Value>,
    pub sensors: Option<Vec<SensorDef>>,
}

#[derive(Deserialize)]
pub struct SensorDef {
    pub name: String,
    pub observable_property: String,
    pub unit: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateObservationSessionRequest {
    pub platform_id: Uuid,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct ObservationBatch {
    pub observations: Vec<SosaObservation>,
}

#[derive(Deserialize, Clone)]
pub struct SosaObservation {
    pub sensor_id: Option<Uuid>,
    pub observable_property: String,
    pub feature_of_interest: Option<String>,
    pub result_value: f64,
    pub result_unit: Option<String>,
    pub phenomenon_time: i64,
    pub result_time: Option<i64>,
    pub extra: Option<serde_json::Value>,
    /// Doc 12 § Capability 1 — optional provenance: which agent (and which
    /// version of that agent) produced this observation. The server resolves
    /// the current version if only `agent_id` is supplied. NULL when the
    /// observation is typed directly by a user / streamed from a sensor.
    #[serde(default)]
    pub produced_by: Option<ProducedByAgent>,
}

/// Doc 12 § Capability 1 — agent provenance attached to an observation.
///
/// Two shapes the platform accepts:
///   - `{ "agent_id": "<slug-or-uuid>" }` — server resolves current version.
///   - `{ "agent_id": "...", "version_id": "...", "version_number": N }` —
///     client knows the version explicitly and passes it through verbatim.
///
/// Apps that don't track versions client-side (the dominant pattern; kask
/// does this) should pass only `agent_id`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProducedByAgent {
    pub agent_id: String,
    #[serde(default)]
    pub version_id: Option<Uuid>,
    #[serde(default)]
    pub version_number: Option<i32>,
}

/// Resolved provenance the INSERT path actually binds against the row.
#[derive(Debug, Clone, Default)]
struct ResolvedProducedBy {
    agent_id: Option<String>,
    version_id: Option<Uuid>,
    version_number: Option<i32>,
}

#[derive(Deserialize)]
pub struct ObservationQuery {
    pub observable_property: Option<String>,
    pub feature_of_interest: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Doc 12 § Capability 1 — filter by agent provenance. Both accept the
    /// agent's slug (the human-readable name) verbatim; the produced_by_agent_id
    /// column stores whatever the writer passed in.
    pub produced_by_agent_id: Option<String>,
    pub produced_by_version_number: Option<i32>,
}

#[derive(Deserialize)]
pub struct PlatformListQuery {
    pub platform_type: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct SessionListQuery {
    pub limit: Option<i64>,
}

// ─── POST /api/observe/platforms ────────────────────────────────────

pub async fn create_platform_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreatePlatformRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let platform_id = Uuid::new_v4();
    let location = req.location.unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO sosa_platforms (platform_id, owner_id, name, platform_type, description, location)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(platform_id)
    .bind(&user_id)
    .bind(&req.name)
    .bind(&req.platform_type)
    .bind(&req.description)
    .bind(&location)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Auto-register sensors if provided
    let mut sensor_ids = Vec::new();
    if let Some(sensors) = &req.sensors {
        for s in sensors {
            let sid = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO sosa_sensors (sensor_id, platform_id, name, observable_property, unit, description)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(sid)
            .bind(platform_id)
            .bind(&s.name)
            .bind(&s.observable_property)
            .bind(&s.unit)
            .bind(&s.description)
            .execute(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            sensor_ids
                .push(json!({"sensor_id": sid, "name": s.name, "property": s.observable_property}));
        }
    }

    Ok(Json(json!({
        "platform_id": platform_id,
        "name": req.name,
        "platform_type": req.platform_type,
        "sensors": sensor_ids,
    })))
}

// ─── GET /api/observe/platforms ─────────────────────────────────────

pub async fn list_platforms_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<PlatformListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let limit = q.limit.unwrap_or(50).min(200);

    let rows = if let Some(ref ptype) = q.platform_type {
        sqlx::query(
            "SELECT platform_id, name, platform_type, description, location, created_at
             FROM sosa_platforms WHERE owner_id = $1 AND platform_type = $2
             ORDER BY created_at DESC LIMIT $3",
        )
        .bind(&user_id)
        .bind(ptype)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT platform_id, name, platform_type, description, location, created_at
             FROM sosa_platforms WHERE owner_id = $1
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(&user_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let platforms: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "platform_id": r.get::<Uuid, _>("platform_id"),
                "name": r.get::<String, _>("name"),
                "platform_type": r.get::<String, _>("platform_type"),
                "description": r.get::<Option<String>, _>("description"),
                "location": r.get::<serde_json::Value, _>("location"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "platforms": platforms })))
}

// ─── POST /api/observe/sessions ────────────────────────────────────

pub async fn create_observation_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateObservationSessionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Verify platform ownership
    let platform = sqlx::query("SELECT owner_id FROM sosa_platforms WHERE platform_id = $1")
        .bind(req.platform_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Platform not found".to_string()))?;

    let owner: String = platform.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your platform".to_string()));
    }

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.observation_session_create,
        "observation_session_create",
        &format!("Create observation session: {}", req.name),
        None,
    )
    .await?;

    let session_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO observation_sessions (session_id, owner_id, platform_id, name, description)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(session_id)
    .bind(&user_id)
    .bind(req.platform_id)
    .bind(&req.name)
    .bind(&req.description)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "session_id": session_id,
        "platform_id": req.platform_id,
        "name": req.name,
        "status": "active",
    })))
}

// ─── GET /api/observe/sessions ─────────────────────────────────────

pub async fn list_observation_sessions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<SessionListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let limit = q.limit.unwrap_or(50).min(200);

    let rows = sqlx::query(
        "SELECT s.session_id, s.platform_id, s.name, s.description, s.status,
                s.started_at, s.ended_at, p.name as platform_name, p.platform_type
         FROM observation_sessions s
         JOIN sosa_platforms p ON s.platform_id = p.platform_id
         WHERE s.owner_id = $1
         ORDER BY s.started_at DESC LIMIT $2",
    )
    .bind(&user_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sessions: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "session_id": r.get::<Uuid, _>("session_id"),
                "platform_id": r.get::<Uuid, _>("platform_id"),
                "platform_name": r.get::<String, _>("platform_name"),
                "platform_type": r.get::<String, _>("platform_type"),
                "name": r.get::<String, _>("name"),
                "description": r.get::<Option<String>, _>("description"),
                "status": r.get::<String, _>("status"),
                "started_at": r.get::<chrono::DateTime<chrono::Utc>, _>("started_at"),
                "ended_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "sessions": sessions })))
}

// ─── PUT /api/observe/sessions/:id/end ─────────────────────────────

pub async fn end_observation_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let result = sqlx::query(
        "UPDATE observation_sessions SET status = 'completed', ended_at = NOW()
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

// ─── POST /api/observe/sessions/:id/observations ───────────────────

pub async fn ingest_observations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
    Json(batch): Json<ObservationBatch>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    if batch.observations.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Empty batch".to_string()));
    }
    if batch.observations.len() > 10_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Batch too large (max 10000 observations)".to_string(),
        ));
    }

    // Verify session ownership and active status
    let session = sqlx::query(
        "SELECT owner_id, platform_id, status FROM observation_sessions WHERE session_id = $1",
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
    let status: String = session.get("status");
    if status != "active" {
        return Err((StatusCode::CONFLICT, "Session is not active".to_string()));
    }
    let platform_id: Uuid = session.get("platform_id");

    // Charge gas
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.observation_ingest,
        "observation_ingest",
        &format!(
            "Ingest {} observations to session {}",
            batch.observations.len(),
            session_id
        ),
        Some(&session_id.to_string()),
    )
    .await?;

    // Doc 12 § Capability 1 — resolve agent provenance per observation.
    //
    // Multiple observations in the same batch frequently share an agent, so
    // cache resolutions by agent_id to avoid hammering `agent_versions` once
    // per row. Resolution is best-effort: if the agent isn't found or the
    // version lookup fails, the columns are left NULL and the batch still
    // ingests successfully — this is observability, not auth.
    let mut resolved: Vec<ResolvedProducedBy> = Vec::with_capacity(batch.observations.len());
    let mut agent_cache: std::collections::HashMap<String, ResolvedProducedBy> =
        std::collections::HashMap::new();

    for obs in &batch.observations {
        match &obs.produced_by {
            None => resolved.push(ResolvedProducedBy::default()),
            Some(pb) => {
                // Cache key is (agent_id, supplied_version_number). When the
                // client provides an explicit version we use it verbatim and
                // never resolve; otherwise we cache the server-resolved
                // current version per agent.
                let cache_key = format!("{}|{:?}", pb.agent_id, pb.version_number);
                if let Some(existing) = agent_cache.get(&cache_key) {
                    resolved.push(existing.clone());
                    continue;
                }

                // Resolve the agent_id (which may be the slug or UUID) into
                // the canonical agent_id UUID. Best-effort: NULL out the
                // provenance if lookup fails.
                let agent_uuid_opt: Option<Uuid> = match sqlx::query(
                    "SELECT agent_id FROM agents WHERE LOWER(name) = LOWER($1) \
                     OR agent_id::text = $1 LIMIT 1",
                )
                .bind(&pb.agent_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                {
                    Some(row) => row.try_get("agent_id").ok(),
                    None => None,
                };

                let resolved_one = match agent_uuid_opt {
                    None => ResolvedProducedBy {
                        agent_id: Some(pb.agent_id.clone()),
                        version_id: pb.version_id,
                        version_number: pb.version_number,
                    },
                    Some(agent_uuid) => {
                        // If caller supplied an explicit version_id/number,
                        // trust it; otherwise resolve the current version.
                        if pb.version_id.is_some() || pb.version_number.is_some() {
                            ResolvedProducedBy {
                                agent_id: Some(pb.agent_id.clone()),
                                version_id: pb.version_id,
                                version_number: pb.version_number,
                            }
                        } else {
                            let cur = state
                                .memory_store
                                .get_current_agent_version(agent_uuid)
                                .await
                                .ok()
                                .flatten();
                            ResolvedProducedBy {
                                agent_id: Some(pb.agent_id.clone()),
                                version_id: cur.as_ref().map(|v| v.version_id),
                                version_number: cur.as_ref().map(|v| v.version_number),
                            }
                        }
                    }
                };

                agent_cache.insert(cache_key, resolved_one.clone());
                resolved.push(resolved_one);
            }
        }
    }

    // Build multi-row INSERT
    let obs_count = batch.observations.len();
    let col_count = 14u32; // Doc 12 § Capability 1: +3 produced_by columns.
    let mut query = String::from(
        "INSERT INTO sosa_observations (observation_id, session_id, sensor_id, platform_id, \
         observable_property, feature_of_interest, result_value, result_unit, \
         phenomenon_time, result_time, extra, \
         produced_by_agent_id, produced_by_version_id, produced_by_version_number) VALUES ",
    );

    let mut param_idx = 1u32;
    for (i, _) in batch.observations.iter().enumerate() {
        if i > 0 {
            query.push_str(", ");
        }
        query.push('(');
        for j in 0..col_count {
            if j > 0 {
                query.push_str(", ");
            }
            query.push_str(&format!("${}", param_idx));
            param_idx += 1;
        }
        query.push(')');
    }

    // Pre-compute derived values
    let derived: Vec<(Uuid, serde_json::Value)> = batch
        .observations
        .iter()
        .map(|o| (Uuid::new_v4(), o.extra.clone().unwrap_or(json!({}))))
        .collect();

    let mut q = sqlx::query(&query);

    for (i, obs) in batch.observations.iter().enumerate() {
        let (ref oid, ref extra) = derived[i];
        let prov = &resolved[i];
        q = q
            .bind(*oid)
            .bind(session_id)
            .bind(obs.sensor_id)
            .bind(platform_id)
            .bind(&obs.observable_property)
            .bind(&obs.feature_of_interest)
            .bind(obs.result_value)
            .bind(&obs.result_unit)
            .bind(obs.phenomenon_time)
            .bind(obs.result_time)
            .bind(extra)
            .bind(&prov.agent_id)
            .bind(prov.version_id)
            .bind(prov.version_number);
    }

    q.execute(&state.db).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Insert failed: {}", e),
        )
    })?;

    // Collect distinct properties for the response
    let properties: Vec<String> = batch
        .observations
        .iter()
        .map(|o| o.observable_property.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Hook 2: resolve real readings against SimOps predictions (fire-and-forget).
    // For each real observation (not simops_simulation), check if a committed
    // synthetic prediction exists and write process_spacetime rows.
    {
        let pool_bg = state.db.clone();
        let obs_copy = batch.observations.clone();
        let derived_copy = derived.clone();
        let sid = session_id;
        let wid: Option<uuid::Uuid> = None; // workspace_id not available here; enriched later

        tokio::spawn(async move {
            for (i, obs) in obs_copy.iter().enumerate() {
                // Skip synthetic observations — only process real sensor readings
                let source = obs
                    .extra
                    .as_ref()
                    .and_then(|e| e.get("source"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if source == "simops_simulation" {
                    continue;
                }

                let (oid, _) = &derived_copy[i];
                let conditions = obs.extra.clone();

                let reading = crate::handlers::simops_benchmark::RealReading {
                    observation_id: *oid,
                    session_id: sid,
                    workspace_id: wid,
                    observable_property: obs.observable_property.clone(),
                    feature_of_interest: obs.feature_of_interest.clone(),
                    actual_value: obs.result_value,
                    measured_at: chrono::Utc::now(),
                    conditions,
                };
                let _ = crate::handlers::simops_benchmark::resolve_against_projection(
                    &pool_bg, &reading,
                )
                .await;
            }
        });
    }

    // Auto-execute observation_analyst (fire-and-forget)
    {
        let spawn_state = state.clone();
        let props = properties.clone();
        let n = obs_count;
        tokio::spawn(async move {
            let analyst_id = "observation_analyst";
            let card = match spawn_state.registry.get(analyst_id) {
                Ok(c) => c,
                Err(_) => return,
            };

            let prompt = format!(
                "Observation batch ingested for session {} on platform {}.\n\
                 Observations: {}, Properties: {:?}.\n\
                 Analyze the observations using SSN/SOSA vocabulary. \
                 Identify temporal patterns, anomalies, and correlations across properties. \
                 Recommend any actuations if warranted.",
                session_id, platform_id, n, props
            );

            let agent_stmt = ast::AgentStmt {
                name: analyst_id.to_string(),
                agent_type: Some(card.agent_type.clone()),
                query: prompt.clone(),
                executor: Some(ast::ExecutorType::LLM),
                schedule: None,
                driver_refs: vec![],
                depends_on: vec![],
                confidence_threshold: None,
            };
            let program = ast::Program {
                statements: vec![ast::Statement::Agent(agent_stmt.clone())],
            };
            // SPEC_28 — platform-service agent; funded from the
            // `abw-system` principal's credential store.
            let credentials = match crate::resolve_agent(&spawn_state, analyst_id).await {
                Ok(db_agent) => {
                    crate::build_execution_credentials(&spawn_state, &db_agent, &card).await
                }
                Err(_) => fermi::agent_backend::credentials::ResolvedCredentials::unfunded_arc(),
            };

            let context = ExecutionContext {
                program,
                agent_card: card.clone(),
                creature_id: None,
                cognition_tier: None,
                credentials,
            };

            match spawn_state
                .registry
                .execute_agent(&agent_stmt, &context)
                .await
            {
                Ok(output) => {
                    let db_id = sqlx::query(
                        "SELECT agent_id FROM agents WHERE LOWER(name) = LOWER($1) LIMIT 1",
                    )
                    .bind(analyst_id)
                    .fetch_optional(&spawn_state.db)
                    .await
                    .ok()
                    .flatten();

                    if let Some(row) = db_id {
                        let agent_uuid: Uuid = row.get("agent_id");
                        // No dyad_id: this is a system-spawned platform agent
                        // with no human counterpart, so it must not accrue
                        // relationship state.
                        let episode = agent_output_to_episode(agent_uuid, &prompt, &output);

                        let embed_text = format!(
                            "{} {}",
                            prompt,
                            output.metadata.reasoning.as_deref().unwrap_or("")
                        );
                        let t_embed = tokio::time::Instant::now();
                        let provenance =
                            match spawn_state.embedder.generate_provenanced(&embed_text).await {
                                Ok(p) => {
                                    tracing::info!(
                                        elapsed_ms = t_embed.elapsed().as_millis() as u64,
                                        model = %p.model_id,
                                        site = "observation_analyst",
                                        "embed_call"
                                    );
                                    Some(p)
                                }
                                Err(_) => None,
                            };
                        let source_ref = serde_json::json!({
                            "kind": "observation_analyst",
                            "agent_id": agent_uuid,
                            "session_id": session_id,
                        });
                        let _ = spawn_state
                            .memory_store
                            .store_episode_with_provenance(
                                episode,
                                provenance.as_ref(),
                                Some(source_ref),
                            )
                            .await;
                    }
                    eprintln!(
                        "Observation analyst: analyzed {} observations for session {}",
                        n, session_id
                    );
                }
                Err(e) => {
                    eprintln!(
                        "Observation analyst failed for session {}: {:?}",
                        session_id, e
                    );
                }
            }
        });
    }

    Ok(Json(json!({
        "session_id": session_id,
        "observations_ingested": obs_count,
        "properties": properties,
    })))
}

// ─── GET /api/observe/sessions/:id/observations ────────────────────

pub async fn query_observations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
    Query(q): Query<ObservationQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Verify ownership
    let session = sqlx::query("SELECT owner_id FROM observation_sessions WHERE session_id = $1")
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

    // Build dynamic query — note the three new produced_by_* columns from
    // Doc 12 § Capability 1, always returned.
    let mut sql = String::from(
        "SELECT observation_id, sensor_id, observable_property, feature_of_interest,
                result_value, result_unit, phenomenon_time, result_time, procedure, extra,
                produced_by_agent_id, produced_by_version_id, produced_by_version_number
         FROM sosa_observations WHERE session_id = $1",
    );
    let mut param_idx = 2u32;
    let mut conditions = Vec::new();

    if q.observable_property.is_some() {
        conditions.push(format!(" AND observable_property = ${}", param_idx));
        param_idx += 1;
    }
    if q.feature_of_interest.is_some() {
        conditions.push(format!(" AND feature_of_interest = ${}", param_idx));
        param_idx += 1;
    }
    if q.from_ms.is_some() {
        conditions.push(format!(" AND phenomenon_time >= ${}", param_idx));
        param_idx += 1;
    }
    if q.to_ms.is_some() {
        conditions.push(format!(" AND phenomenon_time <= ${}", param_idx));
        param_idx += 1;
    }
    if q.produced_by_agent_id.is_some() {
        conditions.push(format!(" AND produced_by_agent_id = ${}", param_idx));
        param_idx += 1;
    }
    if q.produced_by_version_number.is_some() {
        conditions.push(format!(" AND produced_by_version_number = ${}", param_idx));
        param_idx += 1;
    }

    for c in &conditions {
        sql.push_str(c);
    }
    sql.push_str(&format!(
        " ORDER BY phenomenon_time ASC LIMIT ${} OFFSET ${}",
        param_idx,
        param_idx + 1
    ));

    let mut query = sqlx::query(&sql).bind(session_id);
    if let Some(ref prop) = q.observable_property {
        query = query.bind(prop);
    }
    if let Some(ref foi) = q.feature_of_interest {
        query = query.bind(foi);
    }
    if let Some(from) = q.from_ms {
        query = query.bind(from);
    }
    if let Some(to) = q.to_ms {
        query = query.bind(to);
    }
    if let Some(ref pa) = q.produced_by_agent_id {
        query = query.bind(pa);
    }
    if let Some(pv) = q.produced_by_version_number {
        query = query.bind(pv);
    }
    query = query.bind(limit).bind(offset);

    let rows = query
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let observations: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "observation_id": r.get::<Uuid, _>("observation_id"),
                "sensor_id": r.get::<Option<Uuid>, _>("sensor_id"),
                "observable_property": r.get::<String, _>("observable_property"),
                "feature_of_interest": r.get::<Option<String>, _>("feature_of_interest"),
                "result_value": r.get::<f64, _>("result_value"),
                "result_unit": r.get::<Option<String>, _>("result_unit"),
                "phenomenon_time": r.get::<i64, _>("phenomenon_time"),
                "result_time": r.get::<Option<i64>, _>("result_time"),
                "procedure": r.get::<Option<String>, _>("procedure"),
                "extra": r.get::<serde_json::Value, _>("extra"),
                // Doc 12 § Capability 1
                "produced_by_agent_id": r.get::<Option<String>, _>("produced_by_agent_id"),
                "produced_by_version_id": r.get::<Option<Uuid>, _>("produced_by_version_id"),
                "produced_by_version_number": r.get::<Option<i32>, _>("produced_by_version_number"),
            })
        })
        .collect();

    Ok(Json(json!({
        "session_id": session_id,
        "count": observations.len(),
        "observations": observations,
    })))
}

// ─── GET /api/observe/sessions/:id/summary ─────────────────────────

pub async fn observation_summary_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let session = sqlx::query(
        "SELECT s.owner_id, s.name, s.status, s.started_at, s.ended_at,
                p.name as platform_name, p.platform_type
         FROM observation_sessions s
         JOIN sosa_platforms p ON s.platform_id = p.platform_id
         WHERE s.session_id = $1",
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

    // Per-property aggregates
    let prop_stats = sqlx::query(
        "SELECT observable_property, result_unit,
                COUNT(*)::BIGINT as count,
                AVG(result_value) as avg_value,
                MIN(result_value) as min_value,
                MAX(result_value) as max_value,
                MIN(phenomenon_time) as first_time,
                MAX(phenomenon_time) as last_time
         FROM sosa_observations WHERE session_id = $1
         GROUP BY observable_property, result_unit
         ORDER BY observable_property",
    )
    .bind(session_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let properties: Vec<serde_json::Value> = prop_stats
        .iter()
        .map(|r| {
            json!({
                "property": r.get::<String, _>("observable_property"),
                "unit": r.get::<Option<String>, _>("result_unit"),
                "count": r.get::<i64, _>("count"),
                "avg": r.get::<Option<f64>, _>("avg_value"),
                "min": r.get::<Option<f64>, _>("min_value"),
                "max": r.get::<Option<f64>, _>("max_value"),
                "first_time": r.get::<Option<i64>, _>("first_time"),
                "last_time": r.get::<Option<i64>, _>("last_time"),
            })
        })
        .collect();

    let total: i64 = properties
        .iter()
        .filter_map(|p| p.get("count").and_then(|v| v.as_i64()))
        .sum();

    Ok(Json(json!({
        "session_id": session_id,
        "platform_name": session.get::<String, _>("platform_name"),
        "platform_type": session.get::<String, _>("platform_type"),
        "status": session.get::<String, _>("status"),
        "total_observations": total,
        "property_count": properties.len(),
        "properties": properties,
    })))
}

// ─── GET /api/observe/sessions/:id/experience ──────────────────────

pub async fn observation_experience_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
    Query(q): Query<ExperienceQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let session = sqlx::query("SELECT owner_id FROM observation_sessions WHERE session_id = $1")
        .bind(session_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    let owner: String = session.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your session".to_string()));
    }

    let analyst_row = sqlx::query(
        "SELECT agent_id FROM agents WHERE LOWER(name) = 'observation_analyst' LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let analyst_id: Uuid = match analyst_row {
        Some(row) => row.get("agent_id"),
        None => {
            return Ok(Json(json!({
                "session_id": session_id,
                "experiences": [],
                "note": "observation_analyst agent not found"
            })));
        }
    };

    let limit = q.limit.unwrap_or(500).min(5000);

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
    .bind(analyst_id)
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
        "analyst_agent_id": analyst_id,
        "experience_count": experiences.len(),
        "embedding_dim": 1024,
        "usage": "Nearest-neighbor lookup: embed current sensor state, find closest experience, adopt its recommendation.",
        "experiences": experiences,
    })))
}

#[derive(Deserialize)]
pub struct ExperienceQuery {
    pub limit: Option<i64>,
}
