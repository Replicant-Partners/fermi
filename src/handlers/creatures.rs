//! Rabble.world creature API handlers — public discovery + authenticated management.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::super::AppState;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

// ─── Public endpoints ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreatureQuery {
    pub species_group: Option<String>,
    pub scientific_name: Option<String>,
    pub owner_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /api/creatures — browse minted creatures
pub async fn list_creatures_handler(
    State(state): State<AppState>,
    Query(q): Query<CreatureQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(20).min(100);
    let offset = q.offset.unwrap_or(0);

    let mut sql = String::from(
        "SELECT creature_id, owner_id, scientific_name, common_name, species_group,
         gbif_key, specimen_name, variation_notes, asset_path, flight_silhouette_path,
         total_flights, unique_locations, created_at
         FROM creatures WHERE 1=1",
    );
    let mut bind_idx = 0u32;
    let mut binds_str: Vec<String> = Vec::new();

    if let Some(ref group) = q.species_group {
        bind_idx += 1;
        sql.push_str(&format!(" AND species_group = ${}", bind_idx));
        binds_str.push(group.clone());
    }
    if let Some(ref name) = q.scientific_name {
        bind_idx += 1;
        sql.push_str(&format!(" AND scientific_name ILIKE ${}", bind_idx));
        binds_str.push(format!("%{}%", name));
    }
    if let Some(ref owner) = q.owner_id {
        bind_idx += 1;
        sql.push_str(&format!(" AND owner_id = ${}", bind_idx));
        binds_str.push(owner.clone());
    }

    sql.push_str(" ORDER BY created_at DESC");
    sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    // Build query with dynamic binds
    let mut query = sqlx::query(&sql);
    for s in &binds_str {
        query = query.bind(s);
    }

    let pool = state.memory_store.pool();
    match query.fetch_all(pool).await {
        Ok(rows) => {
            let creatures: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "creature_id": row.get::<Uuid, _>("creature_id"),
                        "owner_id": row.get::<String, _>("owner_id"),
                        "scientific_name": row.get::<String, _>("scientific_name"),
                        "common_name": row.get::<Option<String>, _>("common_name"),
                        "species_group": row.get::<String, _>("species_group"),
                        "gbif_key": row.get::<Option<i64>, _>("gbif_key"),
                        "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                        "variation_notes": row.get::<Option<String>, _>("variation_notes"),
                        "asset_path": row.get::<String, _>("asset_path"),
                        "flight_silhouette_path": row.get::<Option<String>, _>("flight_silhouette_path"),
                        "total_flights": row.get::<i32, _>("total_flights"),
                        "unique_locations": row.get::<i32, _>("unique_locations"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({ "creatures": creatures, "count": creatures.len() })),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Failed to list creatures: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to list creatures"})),
            )
                .into_response()
        }
    }
}

/// GET /api/creatures/:id — single creature with full data card
pub async fn get_creature_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT creature_id, owner_id, workspace_id, scientific_name, common_name,
         species_group, gbif_key, taxonomy, specimen_name, variation_notes,
         asset_path, flight_silhouette_path, generation_params,
         mint_number, total_flights, total_flight_time_seconds, unique_locations,
         data_card, sosa_opt_in, created_at, updated_at
         FROM creatures WHERE creature_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => {
            let creature = json!({
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "owner_id": row.get::<String, _>("owner_id"),
                "workspace_id": row.get::<Option<Uuid>, _>("workspace_id"),
                "scientific_name": row.get::<String, _>("scientific_name"),
                "common_name": row.get::<Option<String>, _>("common_name"),
                "species_group": row.get::<String, _>("species_group"),
                "gbif_key": row.get::<Option<i64>, _>("gbif_key"),
                "taxonomy": row.get::<serde_json::Value, _>("taxonomy"),
                "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                "variation_notes": row.get::<Option<String>, _>("variation_notes"),
                "asset_path": row.get::<String, _>("asset_path"),
                "flight_silhouette_path": row.get::<Option<String>, _>("flight_silhouette_path"),
                "generation_params": row.get::<serde_json::Value, _>("generation_params"),
                "mint_number": row.get::<i32, _>("mint_number"),
                "total_flights": row.get::<i32, _>("total_flights"),
                "total_flight_time_seconds": row.get::<i64, _>("total_flight_time_seconds"),
                "unique_locations": row.get::<i32, _>("unique_locations"),
                "data_card": row.get::<serde_json::Value, _>("data_card"),
                "sosa_opt_in": row.try_get::<bool, _>("sosa_opt_in").unwrap_or(false),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            });
            (StatusCode::OK, Json(creature)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Creature not found"})),
        )
            .into_response(),
        Err(e) => {
            eprintln!("Failed to get creature: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to get creature"})),
            )
                .into_response()
        }
    }
}

/// GET /api/creatures/:id/flights — flight history for a creature
pub async fn creature_flights_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT flight_id, creature_id, beacon_id, owner_id,
         h3_cell, center_lat, center_lng, location_name, country_code,
         flight_pattern, swarm_id, started_at, ended_at, duration_seconds,
         path_samples
         FROM creature_flights
         WHERE creature_id = $1
         ORDER BY started_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            let flights: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "flight_id": row.get::<Uuid, _>("flight_id"),
                        "creature_id": row.get::<Uuid, _>("creature_id"),
                        "beacon_id": row.get::<Option<Uuid>, _>("beacon_id"),
                        "owner_id": row.get::<String, _>("owner_id"),
                        "h3_cell": row.get::<String, _>("h3_cell"),
                        "center_lat": row.get::<f64, _>("center_lat"),
                        "center_lng": row.get::<f64, _>("center_lng"),
                        "location_name": row.get::<Option<String>, _>("location_name"),
                        "country_code": row.get::<Option<String>, _>("country_code"),
                        "flight_pattern": row.get::<String, _>("flight_pattern"),
                        "swarm_id": row.get::<Option<Uuid>, _>("swarm_id"),
                        "started_at": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at").to_rfc3339(),
                        "ended_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at").map(|t| t.to_rfc3339()),
                        "duration_seconds": row.get::<Option<i32>, _>("duration_seconds"),
                        "path_samples": row.get::<Option<serde_json::Value>, _>("path_samples"),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({ "flights": flights, "count": flights.len() })),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Failed to get flights: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to get flights"})),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ─── Swarm endpoints (public read) ─────────────────────────────────

#[derive(Deserialize)]
pub struct SwarmQuery {
    pub h3_cell: Option<String>,
    pub status: Option<String>,
    pub species_filter: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api/swarms — browse upcoming/active swarm events.
/// Visibility rules: public always shown; shared/private shown only to creator or invited users.
pub async fn list_swarms_handler(
    State(state): State<AppState>,
    caller: Option<axum::extract::Extension<AuthPrincipal>>,
    Query(q): Query<SwarmQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(20).min(50);
    let caller_id = caller.map(|c| c.0.user_id());

    let mut sql = String::from(
        "SELECT swarm_id, creator_id, h3_cell, center_lat, center_lng,
         location_name, name, description, species_filter, max_participants,
         starts_at, ends_at, status, participant_count, creature_count,
         visibility, funding_mode, qr_token, created_at
         FROM swarm_events WHERE 1=1",
    );

    let mut binds: Vec<String> = Vec::new();
    let mut bind_idx = 0u32;

    if let Some(ref status) = q.status {
        bind_idx += 1;
        sql.push_str(&format!(" AND status = ${}", bind_idx));
        binds.push(status.clone());
    } else {
        sql.push_str(" AND status IN ('scheduled', 'active')");
    }

    if let Some(ref h3) = q.h3_cell {
        bind_idx += 1;
        sql.push_str(&format!(" AND h3_cell = ${}", bind_idx));
        binds.push(h3.clone());
    }

    if let Some(ref species) = q.species_filter {
        bind_idx += 1;
        sql.push_str(&format!(" AND species_filter = ${}", bind_idx));
        binds.push(species.clone());
    }

    // Visibility filter: public always; shared+private only for creator or invited
    if let Some(ref uid) = caller_id {
        bind_idx += 1;
        sql.push_str(&format!(
            " AND (visibility = 'public' OR creator_id = ${bind_idx} \
             OR swarm_id::text IN (SELECT object_id FROM object_shares \
             WHERE object_type = 'rabble' AND (share_target = ${bind_idx} OR share_target IN \
             (SELECT team_id::text FROM team_members WHERE user_id = ${bind_idx}))))",
        ));
        binds.push(uid.clone());
    } else {
        sql.push_str(" AND visibility = 'public'");
    }

    sql.push_str(&format!(" ORDER BY starts_at ASC LIMIT {}", limit));

    let mut query = sqlx::query(&sql);
    for s in &binds {
        query = query.bind(s);
    }

    let pool = state.memory_store.pool();
    match query.fetch_all(pool).await {
        Ok(rows) => {
            let swarms: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "swarm_id": row.get::<Uuid, _>("swarm_id"),
                        "creator_id": row.get::<String, _>("creator_id"),
                        "h3_cell": row.get::<String, _>("h3_cell"),
                        "center_lat": row.get::<f64, _>("center_lat"),
                        "center_lng": row.get::<f64, _>("center_lng"),
                        "location_name": row.get::<Option<String>, _>("location_name"),
                        "name": row.get::<String, _>("name"),
                        "description": row.get::<Option<String>, _>("description"),
                        "species_filter": row.get::<Option<String>, _>("species_filter"),
                        "max_participants": row.get::<Option<i32>, _>("max_participants"),
                        "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                        "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                        "status": row.get::<String, _>("status"),
                        "participant_count": row.get::<i32, _>("participant_count"),
                        "creature_count": row.get::<i32, _>("creature_count"),
                        "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".into()),
                        "funding_mode": row.try_get::<String, _>("funding_mode").unwrap_or_else(|_| "hosted".into()),
                        "qr_token": row.try_get::<Option<String>, _>("qr_token").unwrap_or(None),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({ "swarms": swarms, "count": swarms.len() })),
            )
                .into_response()
        }
        Err(e) => {
            eprintln!("Failed to list swarms: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to list swarms"})),
            )
                .into_response()
        }
    }
}

/// GET /api/swarms/:id — single swarm with details
pub async fn get_swarm_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT swarm_id, creator_id, workspace_id, h3_cell, h3_resolution,
         center_lat, center_lng, location_name, grid_map_id,
         name, description, species_filter, max_participants,
         starts_at, ends_at, status, participant_count, creature_count,
         metadata, created_at, visibility, funding_mode, invite_pool,
         invite_pool_remaining, suggested_contribution, total_contributions, qr_token
         FROM swarm_events WHERE swarm_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => {
            let swarm = json!({
                "swarm_id": row.get::<Uuid, _>("swarm_id"),
                "creator_id": row.get::<String, _>("creator_id"),
                "workspace_id": row.get::<Option<Uuid>, _>("workspace_id"),
                "h3_cell": row.get::<String, _>("h3_cell"),
                "h3_resolution": row.get::<i32, _>("h3_resolution"),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "location_name": row.get::<Option<String>, _>("location_name"),
                "grid_map_id": row.get::<Option<Uuid>, _>("grid_map_id"),
                "name": row.get::<String, _>("name"),
                "description": row.get::<Option<String>, _>("description"),
                "species_filter": row.get::<Option<String>, _>("species_filter"),
                "max_participants": row.get::<Option<i32>, _>("max_participants"),
                "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                "status": row.get::<String, _>("status"),
                "participant_count": row.get::<i32, _>("participant_count"),
                "creature_count": row.get::<i32, _>("creature_count"),
                "metadata": row.get::<serde_json::Value, _>("metadata"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".into()),
                "funding_mode": row.try_get::<String, _>("funding_mode").unwrap_or_else(|_| "hosted".into()),
                "invite_pool": row.try_get::<i32, _>("invite_pool").unwrap_or(0),
                "invite_pool_remaining": row.try_get::<i32, _>("invite_pool_remaining").unwrap_or(0),
                "suggested_contribution": row.try_get::<i32, _>("suggested_contribution").unwrap_or(1),
                "total_contributions": row.try_get::<i32, _>("total_contributions").unwrap_or(0),
                "qr_token": row.try_get::<Option<String>, _>("qr_token").unwrap_or(None),
            });
            (StatusCode::OK, Json(swarm)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Swarm not found"})),
        )
            .into_response(),
        Err(e) => {
            eprintln!("Failed to get swarm: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to get swarm"})),
            )
                .into_response()
        }
    }
}

// ─── Collections (authenticated) ───────────────────────────────────

/// GET /api/collections — user's creature collections
pub async fn list_collections_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT collection_id, owner_id, name, description, creature_ids, created_at, updated_at
         FROM creature_collections WHERE owner_id = $1
         ORDER BY updated_at DESC",
    )
    .bind(principal.user_id())
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            let collections: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "collection_id": row.get::<Uuid, _>("collection_id"),
                        "name": row.get::<String, _>("name"),
                        "description": row.get::<Option<String>, _>("description"),
                        "creature_ids": row.get::<serde_json::Value, _>("creature_ids"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "collections": collections }))).into_response()
        }
        Err(e) => {
            eprintln!("Failed to list collections: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to list collections"})),
            )
                .into_response()
        }
    }
}

// ─── Write endpoints (authenticated) ───────────────────────────────

#[derive(Deserialize)]
pub struct RecordFlightRequest {
    pub creature_id: Uuid,
    pub h3_cell: String,
    pub h3_resolution: Option<i32>,
    pub center_lat: f64,
    pub center_lng: f64,
    pub location_name: Option<String>,
    pub country_code: Option<String>,
    pub flight_pattern: Option<String>,
    pub beacon_id: Option<Uuid>,
    pub swarm_id: Option<Uuid>,
}

/// POST /api/flights — record a creature flight (3 credits)
pub async fn record_flight_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<RecordFlightRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Verify creature ownership
    let pool = state.memory_store.pool();
    let creature = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(req.creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    // Charge 3 credits
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        3,
        "creature_flight",
        &format!("Fly creature {}", req.creature_id),
        Some(&req.creature_id.to_string()),
    )
    .await?;

    let flight_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let pattern = req.flight_pattern.as_deref().unwrap_or("wander");
    let resolution = req.h3_resolution.unwrap_or(12);

    sqlx::query(
        "INSERT INTO creature_flights (flight_id, creature_id, beacon_id, owner_id,
         h3_cell, h3_resolution, center_lat, center_lng, location_name, country_code,
         flight_pattern, swarm_id, started_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(flight_id)
    .bind(req.creature_id)
    .bind(req.beacon_id)
    .bind(&user_id)
    .bind(&req.h3_cell)
    .bind(resolution)
    .bind(req.center_lat)
    .bind(req.center_lng)
    .bind(&req.location_name)
    .bind(&req.country_code)
    .bind(pattern)
    .bind(req.swarm_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update creature stats
    sqlx::query(
        "UPDATE creatures SET total_flights = total_flights + 1, updated_at = NOW()
         WHERE creature_id = $1",
    )
    .bind(req.creature_id)
    .execute(pool)
    .await
    .ok(); // best-effort

    Ok(Json(json!({
        "flight_id": flight_id,
        "creature_id": req.creature_id,
        "h3_cell": req.h3_cell,
        "location_name": req.location_name,
        "started_at": now.to_rfc3339(),
    })))
}

#[derive(Deserialize)]
pub struct EndFlightRequest {
    pub duration_seconds: Option<i32>,
    /// GPS breadcrumbs from swarm simulation: [{lat, lng, heading, t}]
    pub path_samples: Option<serde_json::Value>,
}

/// PUT /api/flights/:flight_id/end — end a flight
pub async fn end_flight_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(flight_id): Path<Uuid>,
    Json(req): Json<EndFlightRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let now = chrono::Utc::now();

    let result = sqlx::query(
        "UPDATE creature_flights SET ended_at = $1, duration_seconds = $2, path_samples = $3
         WHERE flight_id = $4 AND owner_id = $5 AND ended_at IS NULL",
    )
    .bind(now)
    .bind(req.duration_seconds)
    .bind(&req.path_samples)
    .bind(flight_id)
    .bind(principal.user_id())
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Flight not found or already ended".to_string(),
        ));
    }

    // Bridge to SOSA: convert path_samples into universal observations (fire-and-forget).
    // Only fires if creature.sosa_opt_in = true (AKP consent model).
    if let Some(ref samples) = req.path_samples {
        if let Some(arr) = samples.as_array() {
            if !arr.is_empty() {
                let spawn_db = state.db.clone();
                let spawn_pool = state.memory_store.pool().clone();
                let user_id = principal.user_id();
                let path_data = arr.clone();
                let fid = flight_id;

                tokio::spawn(async move {
                    // Look up flight details + AKP consent flag
                    let flight_row = sqlx::query(
                        "SELECT f.creature_id, f.swarm_id, c.species_group, c.scientific_name, c.sosa_opt_in
                         FROM creature_flights f
                         JOIN creatures c ON f.creature_id = c.creature_id
                         WHERE f.flight_id = $1",
                    )
                    .bind(fid)
                    .fetch_optional(&spawn_pool)
                    .await
                    .ok()
                    .flatten();

                    let flight_row = match flight_row {
                        Some(r) => r,
                        None => return,
                    };

                    // Respect AKP consent: only emit SOSA if creature owner opted in
                    let opt_in: bool = flight_row.get("sosa_opt_in");
                    if !opt_in {
                        return;
                    }

                    let creature_id: Uuid = flight_row.get("creature_id");
                    let species: String = flight_row.get("species_group");
                    let sci_name: Option<String> = flight_row.get("scientific_name");
                    let feature = sci_name.unwrap_or_else(|| species.clone());

                    // Get or create platform for this creature
                    let platform_id = match sqlx::query(
                        "SELECT platform_id FROM sosa_platforms
                         WHERE owner_id = $1 AND name = $2 LIMIT 1",
                    )
                    .bind(&user_id)
                    .bind(&format!("creature-{}", creature_id))
                    .fetch_optional(&spawn_db)
                    .await
                    .ok()
                    .flatten()
                    {
                        Some(row) => row.get::<Uuid, _>("platform_id"),
                        None => {
                            let pid = Uuid::new_v4();
                            let _ = sqlx::query(
                                "INSERT INTO sosa_platforms (platform_id, owner_id, name, platform_type, description)
                                 VALUES ($1, $2, $3, $4, $5)"
                            )
                            .bind(pid)
                            .bind(&user_id)
                            .bind(&format!("creature-{}", creature_id))
                            .bind("ar_creature")
                            .bind(&format!("Rabble AR {} ({})", species, feature))
                            .execute(&spawn_db)
                            .await;
                            pid
                        }
                    };

                    // Get or create observation session for this flight
                    let session_id = Uuid::new_v4();
                    let _ = sqlx::query(
                        "INSERT INTO observation_sessions (session_id, owner_id, platform_id, name, description)
                         VALUES ($1, $2, $3, $4, $5)"
                    )
                    .bind(session_id)
                    .bind(&user_id)
                    .bind(platform_id)
                    .bind(&format!("flight-{}", fid))
                    .bind(&format!("AR {} flight path → SOSA", species))
                    .execute(&spawn_db)
                    .await;

                    // Convert path_samples [{lat, lng, heading, t}] → SOSA observations
                    // Each sample produces 2-3 observations: position, heading, (speed if derivable)
                    let mut obs_count = 0u32;
                    for (i, sample) in path_data.iter().enumerate() {
                        let t = sample.get("t").and_then(|v| v.as_i64()).unwrap_or(0);
                        let lat = sample.get("lat").and_then(|v| v.as_f64());
                        let lng = sample.get("lng").and_then(|v| v.as_f64());
                        let heading = sample.get("heading").and_then(|v| v.as_f64());

                        if let (Some(lat_v), Some(lng_v)) = (lat, lng) {
                            // Position X (longitude as proxy)
                            let _ = sqlx::query(
                                "INSERT INTO sosa_observations (observation_id, session_id, platform_id,
                                 observable_property, feature_of_interest, result_value, result_unit,
                                 phenomenon_time) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
                            )
                            .bind(Uuid::new_v4()).bind(session_id).bind(platform_id)
                            .bind("onto4mat:xLocation").bind(&feature)
                            .bind(lng_v).bind("deg").bind(t)
                            .execute(&spawn_db).await;

                            // Position Y (latitude)
                            let _ = sqlx::query(
                                "INSERT INTO sosa_observations (observation_id, session_id, platform_id,
                                 observable_property, feature_of_interest, result_value, result_unit,
                                 phenomenon_time) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
                            )
                            .bind(Uuid::new_v4()).bind(session_id).bind(platform_id)
                            .bind("onto4mat:yLocation").bind(&feature)
                            .bind(lat_v).bind("deg").bind(t)
                            .execute(&spawn_db).await;

                            obs_count += 2;
                        }

                        if let Some(h) = heading {
                            let _ = sqlx::query(
                                "INSERT INTO sosa_observations (observation_id, session_id, platform_id,
                                 observable_property, feature_of_interest, result_value, result_unit,
                                 phenomenon_time) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
                            )
                            .bind(Uuid::new_v4()).bind(session_id).bind(platform_id)
                            .bind("onto4mat:hasHeading").bind(&feature)
                            .bind(h).bind("deg").bind(t)
                            .execute(&spawn_db).await;
                            obs_count += 1;
                        }

                        // Derive speed from consecutive samples
                        if i > 0 {
                            let prev = &path_data[i - 1];
                            let prev_t = prev.get("t").and_then(|v| v.as_i64()).unwrap_or(0);
                            let dt = (t - prev_t) as f64 / 1000.0; // seconds
                            if dt > 0.0 {
                                if let (Some(plat), Some(plng), Some(clat), Some(clng)) = (
                                    prev.get("lat").and_then(|v| v.as_f64()),
                                    prev.get("lng").and_then(|v| v.as_f64()),
                                    lat,
                                    lng,
                                ) {
                                    // Haversine-lite: approximate m/s at small scales
                                    let dlat = (clat - plat).to_radians();
                                    let dlng = (clng - plng).to_radians();
                                    let a = (dlat / 2.0).sin().powi(2)
                                        + clat.to_radians().cos()
                                            * plat.to_radians().cos()
                                            * (dlng / 2.0).sin().powi(2);
                                    let dist_m = 2.0 * 6_371_000.0 * a.sqrt().asin();
                                    let speed = dist_m / dt;

                                    let _ = sqlx::query(
                                        "INSERT INTO sosa_observations (observation_id, session_id, platform_id,
                                         observable_property, feature_of_interest, result_value, result_unit,
                                         phenomenon_time) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
                                    )
                                    .bind(Uuid::new_v4()).bind(session_id).bind(platform_id)
                                    .bind("onto4mat:hasSpeed").bind(&feature)
                                    .bind(speed).bind("m/s").bind(t)
                                    .execute(&spawn_db).await;
                                    obs_count += 1;
                                }
                            }
                        }
                    }

                    // Close the session
                    let _ = sqlx::query(
                        "UPDATE observation_sessions SET status = 'completed', ended_at = NOW()
                         WHERE session_id = $1",
                    )
                    .bind(session_id)
                    .execute(&spawn_db)
                    .await;

                    eprintln!(
                        "SOSA bridge: flight {} → {} observations ({} path samples, species: {})",
                        fid,
                        obs_count,
                        path_data.len(),
                        species
                    );
                });
            }
        }
    }

    Ok(Json(json!({
        "flight_id": flight_id,
        "ended_at": now.to_rfc3339(),
        "duration_seconds": req.duration_seconds,
        "has_path": req.path_samples.is_some(),
    })))
}

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
    if req.species_group != "butterfly" && req.species_group != "dragonfly" {
        return Err((
            StatusCode::BAD_REQUEST,
            "species_group must be 'butterfly' or 'dragonfly'".into(),
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

    // Spawn async art generation if requested
    let art_generating = if generate_art {
        let pool_clone = pool.clone();
        let sci_name = req.scientific_name.clone();
        let common = req.common_name.clone();
        let group = req.species_group.clone();
        let gbif = req.gbif_key;
        let style = art_style.to_string();
        tokio::spawn(async move {
            match generate_creature_image(
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
        "created_at": now.to_rfc3339(),
    })))
}

#[derive(Deserialize)]
pub struct CreateSwarmRequest {
    pub h3_cell: String,
    pub h3_resolution: Option<i32>,
    pub center_lat: f64,
    pub center_lng: f64,
    pub location_name: Option<String>,
    pub grid_map_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub species_filter: Option<String>,
    pub max_participants: Option<i32>,
    pub starts_at: String,
    pub ends_at: String,
    pub funding_mode: Option<String>,
    pub invite_pool: Option<i32>,
    pub suggested_contribution: Option<i32>,
    pub visibility: Option<String>,
}

/// POST /api/swarms — create a swarm event (5 credits)
pub async fn create_swarm_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateSwarmRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let visibility = req.visibility.as_deref().unwrap_or("public");
    if visibility != "public" && visibility != "shared" && visibility != "private" {
        return Err((
            StatusCode::BAD_REQUEST,
            "visibility must be 'public', 'shared', or 'private'".into(),
        ));
    }

    let funding_mode = req.funding_mode.as_deref().unwrap_or("hosted");
    if funding_mode != "hosted" && funding_mode != "support" {
        return Err((
            StatusCode::BAD_REQUEST,
            "funding_mode must be 'hosted' or 'support'".into(),
        ));
    }
    let invite_pool = req.invite_pool.unwrap_or(0).max(0);
    let suggested_contribution = req.suggested_contribution.unwrap_or(1).max(1);

    // Calculate total cost: 5 (create fee) + invite_pool (if hosted mode)
    let total_cost = if funding_mode == "hosted" {
        5 + invite_pool
    } else {
        5
    };

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        total_cost,
        "swarm_create",
        &format!("Create rabble: {} ({})", req.name, funding_mode),
        None,
    )
    .await?;

    let swarm_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let starts_at = chrono::DateTime::parse_from_rfc3339(&req.starts_at)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid starts_at: {}", e)))?;
    let ends_at = chrono::DateTime::parse_from_rfc3339(&req.ends_at)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid ends_at: {}", e)))?;
    let resolution = req.h3_resolution.unwrap_or(12);

    // Generate QR token
    let qr_token = generate_qr_token();

    let pool = state.memory_store.pool();
    sqlx::query(
        "INSERT INTO swarm_events (swarm_id, creator_id, h3_cell, h3_resolution,
         center_lat, center_lng, location_name, grid_map_id,
         name, description, species_filter, max_participants,
         starts_at, ends_at, status, created_at,
         funding_mode, invite_pool, invite_pool_remaining,
         suggested_contribution, qr_token, visibility)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'scheduled', $15,
                 $16, $17, $17, $18, $19, $20)",
    )
    .bind(swarm_id)
    .bind(&user_id)
    .bind(&req.h3_cell)
    .bind(resolution)
    .bind(req.center_lat)
    .bind(req.center_lng)
    .bind(&req.location_name)
    .bind(req.grid_map_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.species_filter)
    .bind(req.max_participants)
    .bind(starts_at)
    .bind(ends_at)
    .bind(now)
    .bind(funding_mode)
    .bind(invite_pool)
    .bind(suggested_contribution)
    .bind(&qr_token)
    .bind(visibility)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "name": req.name,
        "status": "scheduled",
        "starts_at": starts_at.to_rfc3339(),
        "ends_at": ends_at.to_rfc3339(),
        "funding_mode": funding_mode,
        "invite_pool": invite_pool,
        "qr_token": qr_token,
        "visibility": visibility,
    })))
}

#[derive(Deserialize)]
pub struct JoinSwarmRequest {
    pub creature_id: Uuid,
    pub contribution: Option<i32>,
}

/// POST /api/swarms/:swarm_id/join — join a rabble with a creature.
/// Cost depends on funding_mode: hosted = free (pool pays), support = joiner contributes.
pub async fn join_swarm_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<Uuid>,
    Json(req): Json<JoinSwarmRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify swarm exists and is joinable
    let swarm = sqlx::query(
        "SELECT status, h3_cell, center_lat, center_lng, creator_id, visibility,
         funding_mode, invite_pool_remaining, suggested_contribution
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

    // Visibility access check
    let visibility: String = swarm
        .try_get("visibility")
        .unwrap_or_else(|_| "public".into());
    let creator_id: String = swarm.get("creator_id");
    if visibility == "private" && creator_id != user_id {
        // Check object_shares for direct user or team membership
        let has_share = sqlx::query(
            "SELECT 1 FROM object_shares
             WHERE object_type = 'rabble' AND object_id = $1::text
             AND (share_target = $2 OR share_target IN
                  (SELECT team_id::text FROM team_members WHERE user_id = $2))
             LIMIT 1",
        )
        .bind(swarm_id)
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if has_share.is_none() {
            return Err((
                StatusCode::FORBIDDEN,
                "This rabble is private. You need an invite to join.".into(),
            ));
        }
    }
    // shared: having the swarm_id means they have the link/QR — allow
    // public: always allow

    let funding_mode: String = swarm.try_get("funding_mode").unwrap_or("hosted".into());

    // Verify creature ownership
    let creature = sqlx::query(
        "SELECT owner_id, specimen_name, species_name, species_group FROM creatures WHERE creature_id = $1"
    )
    .bind(req.creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let creature_name: Option<String> = creature.try_get("specimen_name").ok();
    let species_name: Option<String> = creature.try_get("species_name").ok();
    let species_group: Option<String> = creature.try_get("species_group").ok();

    // Handle funding mode
    if funding_mode == "hosted" {
        let remaining: i32 = swarm.try_get("invite_pool_remaining").unwrap_or(0);
        if remaining <= 0 {
            return Err((StatusCode::PAYMENT_REQUIRED, "Invite pool exhausted".into()));
        }
        sqlx::query("UPDATE swarm_events SET invite_pool_remaining = invite_pool_remaining - 1 WHERE swarm_id = $1")
            .bind(swarm_id)
            .execute(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        // Support mode: joiner contributes
        let suggested: i32 = swarm.try_get("suggested_contribution").unwrap_or(1);
        let contribution = req.contribution.unwrap_or(suggested).max(1);

        let wallet = get_or_create_wallet(&state.db, "user", &user_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        charge_gas(
            &state.db,
            wallet.wallet_id,
            contribution,
            "swarm_join",
            &format!("Support rabble {} ({} credits)", swarm_id, contribution),
            Some(&swarm_id.to_string()),
        )
        .await?;

        sqlx::query("UPDATE swarm_events SET total_contributions = total_contributions + $1 WHERE swarm_id = $2")
            .bind(contribution)
            .bind(swarm_id)
            .execute(pool)
            .await
            .ok();
    }

    // Record the flight at the swarm location
    let flight_id = Uuid::new_v4();
    let h3_cell: String = swarm.get("h3_cell");
    let lat: f64 = swarm.get("center_lat");
    let lng: f64 = swarm.get("center_lng");

    sqlx::query(
        "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
         h3_cell, h3_resolution, center_lat, center_lng,
         flight_pattern, swarm_id, started_at)
         VALUES ($1, $2, $3, $4, 12, $5, $6, 'swarm', $7, NOW())",
    )
    .bind(flight_id)
    .bind(req.creature_id)
    .bind(&user_id)
    .bind(&h3_cell)
    .bind(lat)
    .bind(lng)
    .bind(swarm_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Increment swarm counters
    sqlx::query(
        "UPDATE swarm_events SET participant_count = participant_count + 1,
         creature_count = creature_count + 1
         WHERE swarm_id = $1",
    )
    .bind(swarm_id)
    .execute(pool)
    .await
    .ok();

    // Post system message + trigger swarm host narrator welcome
    let display_name = creature_name.as_deref().unwrap_or("A creature");
    let species_display = species_name.as_deref().unwrap_or("unknown species");
    let _ = super::rabble_chat::insert_system_message(
        &state,
        swarm_id,
        &format!(
            "{} ({}) has joined the rabble!",
            display_name, species_display
        ),
    )
    .await;

    // Trigger swarm host agent welcome (async, non-blocking)
    let state_clone = state.clone();
    let creature_name_c = creature_name.clone();
    let species_name_c = species_name.clone();
    let species_group_c = species_group.clone();
    tokio::spawn(async move {
        trigger_swarm_host_welcome(
            &state_clone,
            swarm_id,
            creature_name_c.as_deref().unwrap_or("creature"),
            species_name_c.as_deref().unwrap_or("unknown"),
            species_group_c.as_deref().unwrap_or("unknown"),
        )
        .await;
    });

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "flight_id": flight_id,
        "creature_id": req.creature_id,
        "joined": true,
        "funding_mode": funding_mode,
    })))
}

/// POST /api/rabble/join/:qr_token — join a rabble via QR code scan.
/// Resolves the token to a swarm_id, then delegates to the standard join logic.
pub async fn join_by_qr_token_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(qr_token): Path<String>,
    Json(req): Json<JoinSwarmRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let row = sqlx::query("SELECT swarm_id FROM swarm_events WHERE qr_token = $1")
        .bind(&qr_token)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Invalid QR code".into()))?;

    let swarm_id: Uuid = row.try_get("swarm_id").unwrap_or_default();

    // Delegate to the standard join handler
    join_swarm_handler(State(state), principal, Path(swarm_id), Json(req)).await
}

#[derive(Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
    pub creature_ids: Option<Vec<Uuid>>,
}

/// POST /api/collections — create a collection
pub async fn create_collection_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateCollectionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let collection_id = Uuid::new_v4();
    let creature_ids = req.creature_ids.unwrap_or_default();
    let now = chrono::Utc::now();

    let pool = state.memory_store.pool();
    sqlx::query(
        "INSERT INTO creature_collections (collection_id, owner_id, name, description, creature_ids, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $6)",
    )
    .bind(collection_id)
    .bind(&user_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(json!(creature_ids))
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "collection_id": collection_id,
        "name": req.name,
        "creature_count": creature_ids.len(),
    })))
}

#[derive(Deserialize)]
pub struct UpdateCollectionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub creature_ids: Option<Vec<Uuid>>,
}

/// PUT /api/collections/:collection_id — update a collection
pub async fn update_collection_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(collection_id): Path<Uuid>,
    Json(req): Json<UpdateCollectionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let existing =
        sqlx::query("SELECT owner_id FROM creature_collections WHERE collection_id = $1")
            .bind(collection_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Collection not found".to_string()))?;

    let owner: String = existing.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your collection".to_string()));
    }

    // Build dynamic update
    let mut sets = vec!["updated_at = NOW()".to_string()];
    let mut bind_idx = 1u32;
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref name) = req.name {
        bind_idx += 1;
        sets.push(format!("name = ${}", bind_idx));
        binds.push(name.clone());
    }
    if let Some(ref desc) = req.description {
        bind_idx += 1;
        sets.push(format!("description = ${}", bind_idx));
        binds.push(desc.clone());
    }

    let creature_ids_json = req.creature_ids.as_ref().map(|ids| json!(ids));
    if creature_ids_json.is_some() {
        bind_idx += 1;
        sets.push(format!("creature_ids = ${}", bind_idx));
    }

    let sql = format!(
        "UPDATE creature_collections SET {} WHERE collection_id = $1 AND owner_id = ${}",
        sets.join(", "),
        bind_idx + 1,
    );

    let mut query = sqlx::query(&sql).bind(collection_id);
    for s in &binds {
        query = query.bind(s);
    }
    if let Some(ref ids_json) = creature_ids_json {
        query = query.bind(ids_json);
    }
    query = query.bind(&user_id);

    query
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "collection_id": collection_id,
        "updated": true,
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
        "watercolor" => "Soft watercolor painting style with visible brush strokes and subtle color bleeding.",
        "botanical" => "Precise botanical illustration on cream parchment. Fine ink linework with hand-tinted washes.",
        "field-guide" => "Clean field guide illustration. Crisp outlines, accurate proportions, white background, wings spread.",
        "ukiyo-e" => "Japanese woodblock print (ukiyo-e) style. Bold black outlines, flat color planes, bokashi gradation on wings. Warm washi paper background. Small red hanko seal in corner. Indigo, ochre, grey tones. Multiple views at different scales.",
        _ => "Detailed naturalist scientific illustration in the style of Maria Sibylla Merian. Rich colors on aged vellum.",
    };

    let group_detail = if species_group == "dragonfly" {
        "Show wing venation, elongated abdomen, compound eyes. Translucent wings with visible cells."
    } else {
        "Show wing scale patterns, proboscis, antennae. Upper and lower wing surfaces visible."
    };

    let prompt = format!(
        "Create a beautiful scientific illustration of a {} ({}).\n\
         Style: {}\nDetails: {}\n\
         Requirements: single specimen, centered, anatomically accurate, \
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

            match generate_creature_image(
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

/// Shared image generation logic used by both single and batch endpoints.
async fn generate_creature_image(
    pool: &sqlx::PgPool,
    creature_id: Uuid,
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
        "watercolor" => "Soft watercolor with visible brush strokes.",
        "botanical" => "Precise botanical illustration on cream parchment.",
        "field-guide" => "Clean field guide illustration, white background, wings spread.",
        "ukiyo-e" => "Japanese woodblock print (ukiyo-e). Bold outlines, flat color planes, bokashi gradation. Washi paper background, red hanko seal in corner. Indigo, ochre, grey. Multiple views at different scales.",
        _ => "Naturalist scientific illustration in the style of Maria Sibylla Merian. Rich colors on aged vellum.",
    };

    let group_detail = if species_group == "dragonfly" {
        "Wing venation, elongated abdomen, compound eyes, translucent wings."
    } else {
        "Wing scale patterns, proboscis, antennae, upper and lower surfaces."
    };

    let prompt = format!(
        "Create a scientific illustration of a {} ({}).\n\
         Style: {}\nDetails: {}\n\
         Single specimen, centered, anatomically accurate, no text, square, dark background (#1A2E20).{}",
        display_name, species_group, style_instruction, group_detail, reference_desc,
    );

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
    let relative_path = format!("/static/creatures/{}", filename);
    let fs_path = format!("static/creatures/{}", filename);

    std::fs::create_dir_all("static/creatures").map_err(|e| format!("mkdir error: {}", e))?;
    std::fs::write(&fs_path, &bytes).map_err(|e| format!("write error: {}", e))?;

    // Persist to database for cross-deploy durability
    persist_creature_image(pool, creature_id, &bytes, mime_type).await;

    // Use API endpoint as asset_path (survives redeploys)
    let api_path = format!("/api/creatures/{}/image", creature_id);

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
    .map_err(|e| format!("DB update error: {}", e))?;

    Ok(api_path)
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

    let result = sqlx::query(
        "UPDATE creatures SET sosa_opt_in = $1, updated_at = NOW()
         WHERE creature_id = $2 AND owner_id = $3",
    )
    .bind(req.opt_in)
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
        "sosa_opt_in": req.opt_in,
        "message": if req.opt_in {
            "SOSA data sharing enabled — future flights will generate universal sensor observations"
        } else {
            "SOSA data sharing disabled — flight data stays private"
        },
    })))
}

// ─── Rabble helpers ────────────────────────────────────────────────

/// Generate a short alphanumeric QR token (8 chars).
fn generate_qr_token() -> String {
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

/// Trigger swarm host agent to generate a welcome message for a joining creature.
async fn trigger_swarm_host_welcome(
    state: &AppState,
    swarm_id: Uuid,
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

    let context = ExecutionContext {
        program,
        agent_card: card,
    };

    let tool_context = Arc::new(ToolContext {
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
                let _ =
                    super::rabble_chat::insert_narrator_message(state, swarm_id, &narrative).await;
            }
        }
        Err(e) => {
            eprintln!("Swarm host welcome failed: {}", e);
        }
    }
}

// ─── Image serving (persistent, from DB) ───────────────────────────

/// GET /api/creatures/:creature_id/image — serve creature art from database
///
/// Falls back to filesystem, then placeholder SVG.
/// Sets Cache-Control for browser caching.
pub async fn creature_image_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();

    // Try DB first
    if let Ok(Some(row)) =
        sqlx::query("SELECT image_bytes, mime_type FROM creature_images WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
    {
        let bytes: Vec<u8> = row.get("image_bytes");
        let mime: String = row.get("mime_type");
        return (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=86400".to_string(),
                ),
            ],
            bytes,
        )
            .into_response();
    }

    // Fallback: try filesystem (works during same deploy that generated it)
    let fs_path = format!("static/creatures/{}.png", creature_id);
    if let Ok(bytes) = std::fs::read(&fs_path) {
        // Also persist to DB for next deploy
        let _ = sqlx::query(
            "INSERT INTO creature_images (creature_id, image_bytes, mime_type, file_size)
             VALUES ($1, $2, 'image/png', $3)
             ON CONFLICT (creature_id) DO UPDATE
             SET image_bytes = $2, mime_type = 'image/png', file_size = $3, updated_at = NOW()",
        )
        .bind(creature_id)
        .bind(&bytes)
        .bind(bytes.len() as i32)
        .execute(pool)
        .await;

        return (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "image/png".to_string()),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=86400".to_string(),
                ),
            ],
            bytes,
        )
            .into_response();
    }

    // Final fallback: placeholder SVG
    let placeholder = std::fs::read("static/creatures/placeholder.svg")
        .unwrap_or_else(|_| b"<svg></svg>".to_vec());
    (
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "image/svg+xml".to_string(),
            ),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=60".to_string(),
            ),
        ],
        placeholder,
    )
        .into_response()
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
