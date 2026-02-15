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
use super::creature_state;
use super::rabble_workspace;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

// ─── Public endpoints ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreatureQuery {
    pub species_group: Option<String>,
    pub scientific_name: Option<String>,
    pub owner_id: Option<String>,
    pub status: Option<String>,
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
         total_flights, unique_locations, status, animation_status, visibility, presence, created_at,
         (SELECT location_name FROM creature_flights WHERE creature_id = creatures.creature_id
          ORDER BY started_at DESC LIMIT 1) as last_location_name
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

    // Status filter: default to 'active', use 'all' to see everything
    match q.status.as_deref() {
        Some("all") => {} // no filter
        Some(status) => {
            bind_idx += 1;
            sql.push_str(&format!(" AND status = ${}", bind_idx));
            binds_str.push(status.to_string());
        }
        None => {
            bind_idx += 1;
            sql.push_str(&format!(" AND status = ${}", bind_idx));
            binds_str.push("active".to_string());
        }
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
                        "status": row.try_get::<String, _>("status").unwrap_or_else(|_| "active".to_string()),
                        "animation_status": row.try_get::<Option<String>, _>("animation_status").unwrap_or(None),
                        "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".to_string()),
                        "presence": row.try_get::<String, _>("presence").unwrap_or_else(|_| "active".to_string()),
                        "last_location_name": row.try_get::<Option<String>, _>("last_location_name").unwrap_or(None),
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
         data_card, sosa_opt_in, animation_status, visibility, presence, created_at, updated_at
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
                "animation_status": row.try_get::<Option<String>, _>("animation_status").unwrap_or(None),
                "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".to_string()),
                "presence": row.try_get::<String, _>("presence").unwrap_or_else(|_| "active".to_string()),
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
         path_samples, environment, data_source
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
                        "environment": row.try_get::<Option<serde_json::Value>, _>("environment").unwrap_or(None),
                        "data_source": row.try_get::<String, _>("data_source").unwrap_or_else(|_| "synthetic".to_string()),
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
         visibility, funding_mode, qr_token, created_at,
         anchor_creature_id, anchor_transferred_at,
         walk_in_price, walk_in_budget, walk_in_budget_remaining
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
             (SELECT team_id::text FROM team_members WHERE member_id = ${bind_idx}))))",
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
            // Collect swarm IDs for participation lookup
            let swarm_ids: Vec<Uuid> = rows.iter().map(|r| r.get::<Uuid, _>("swarm_id")).collect();

            // Look up caller's creature in each swarm (if authenticated)
            let mut my_creatures: std::collections::HashMap<Uuid, (Uuid, String)> =
                std::collections::HashMap::new();
            if let Some(ref uid) = caller_id {
                if !swarm_ids.is_empty() {
                    let placeholders: Vec<String> = (1..=swarm_ids.len())
                        .map(|i| format!("${}", i + 1))
                        .collect();
                    let my_sql = format!(
                        "SELECT DISTINCT ON (cf.swarm_id) cf.swarm_id, cf.creature_id, c.specimen_name, c.scientific_name AS species_name
                         FROM creature_flights cf
                         JOIN creatures c ON c.creature_id = cf.creature_id
                         WHERE cf.swarm_id IN ({}) AND c.owner_id = $1
                         ORDER BY cf.swarm_id, cf.started_at DESC",
                        placeholders.join(", ")
                    );
                    let mut my_query = sqlx::query(&my_sql).bind(uid);
                    for sid in &swarm_ids {
                        my_query = my_query.bind(sid);
                    }
                    if let Ok(my_rows) = my_query.fetch_all(pool).await {
                        for r in &my_rows {
                            let sid: Uuid = r.get("swarm_id");
                            let cid: Uuid = r.get("creature_id");
                            let cname: String = r
                                .try_get::<Option<String>, _>("specimen_name")
                                .ok()
                                .flatten()
                                .or_else(|| {
                                    r.try_get::<Option<String>, _>("species_name")
                                        .ok()
                                        .flatten()
                                })
                                .unwrap_or_else(|| "Creature".into());
                            my_creatures.insert(sid, (cid, cname));
                        }
                    }
                }
            }

            // Batch-lookup creator display names
            let creator_ids: Vec<String> = rows
                .iter()
                .map(|r| r.get::<String, _>("creator_id"))
                .collect();
            let mut creator_names: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            if !creator_ids.is_empty() {
                let unique_ids: Vec<&String> = creator_ids
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                let placeholders: Vec<String> =
                    (1..=unique_ids.len()).map(|i| format!("${}", i)).collect();
                let name_sql = format!(
                    "SELECT user_id, display_name FROM users WHERE user_id IN ({})",
                    placeholders.join(", ")
                );
                let mut name_query = sqlx::query(&name_sql);
                for uid in &unique_ids {
                    name_query = name_query.bind(uid.as_str());
                }
                if let Ok(name_rows) = name_query.fetch_all(pool).await {
                    for r in &name_rows {
                        if let Ok(Some(name)) = r.try_get::<Option<String>, _>("display_name") {
                            creator_names.insert(r.get::<String, _>("user_id"), name);
                        }
                    }
                }
            }

            // Batch-lookup anchor creature images
            let anchor_ids: Vec<Uuid> = rows
                .iter()
                .filter_map(|r| {
                    r.try_get::<Option<Uuid>, _>("anchor_creature_id")
                        .ok()
                        .flatten()
                })
                .collect();
            let mut anchor_images: std::collections::HashMap<Uuid, (String, String)> =
                std::collections::HashMap::new();
            if !anchor_ids.is_empty() {
                let unique_anchors: Vec<&Uuid> = anchor_ids
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                let placeholders: Vec<String> = (1..=unique_anchors.len())
                    .map(|i| format!("${}", i))
                    .collect();
                let img_sql = format!(
                    "SELECT creature_id, asset_path, COALESCE(specimen_name, common_name, scientific_name) as creature_name FROM creatures WHERE creature_id IN ({})",
                    placeholders.join(", ")
                );
                let mut img_query = sqlx::query(&img_sql);
                for cid in &unique_anchors {
                    img_query = img_query.bind(cid);
                }
                if let Ok(img_rows) = img_query.fetch_all(pool).await {
                    for r in &img_rows {
                        let cid: Uuid = r.get("creature_id");
                        let path: String = r.get("asset_path");
                        let name: String =
                            r.try_get::<String, _>("creature_name").unwrap_or_default();
                        anchor_images.insert(cid, (path, name));
                    }
                }
            }

            // Batch-lookup member creature images (top 4 per swarm)
            let mut member_images: std::collections::HashMap<Uuid, Vec<String>> =
                std::collections::HashMap::new();
            if !swarm_ids.is_empty() {
                let placeholders: Vec<String> =
                    (1..=swarm_ids.len()).map(|i| format!("${}", i)).collect();
                let mem_sql = format!(
                    "SELECT cf.swarm_id, c.asset_path
                     FROM creature_flights cf
                     JOIN creatures c ON c.creature_id = cf.creature_id
                     WHERE cf.swarm_id IN ({})
                       AND cf.ended_at IS NULL
                     ORDER BY cf.swarm_id, cf.started_at ASC",
                    placeholders.join(", ")
                );
                let mut mem_query = sqlx::query(&mem_sql);
                for sid in &swarm_ids {
                    mem_query = mem_query.bind(sid);
                }
                if let Ok(mem_rows) = mem_query.fetch_all(pool).await {
                    for r in &mem_rows {
                        let sid: Uuid = r.get("swarm_id");
                        let path: String = r.get("asset_path");
                        let entry = member_images.entry(sid).or_default();
                        if entry.len() < 4 {
                            entry.push(path);
                        }
                    }
                }
            }

            let swarms: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    let sid = row.get::<Uuid, _>("swarm_id");
                    let creator_id = row.get::<String, _>("creator_id");
                    let creator_display_name = creator_names.get(&creator_id).cloned();
                    let anchor_cid = row.try_get::<Option<Uuid>, _>("anchor_creature_id").ok().flatten();
                    let (anchor_image, anchor_name) = anchor_cid
                        .and_then(|cid| anchor_images.get(&cid).cloned())
                        .map(|(img, name)| (Some(img), Some(name)))
                        .unwrap_or((None, None));
                    let (my_cid, my_cname) = my_creatures.get(&sid).map(|(c, n)| (Some(*c), Some(n.clone()))).unwrap_or((None, None));
                    json!({
                        "swarm_id": sid,
                        "creator_id": creator_id,
                        "creator_display_name": creator_display_name,
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
                        "my_creature_id": my_cid,
                        "my_creature_name": my_cname,
                        "anchor_creature_id": anchor_cid,
                        "anchor_creature_image": anchor_image,
                        "anchor_creature_name": anchor_name,
                        "anchor_transferred_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("anchor_transferred_at").ok().flatten().map(|t| t.to_rfc3339()),
                        "walk_in_price": row.try_get::<Option<i32>, _>("walk_in_price").unwrap_or(None),
                        "walk_in_budget": row.try_get::<Option<i32>, _>("walk_in_budget").unwrap_or(None),
                        "walk_in_budget_remaining": row.try_get::<Option<i32>, _>("walk_in_budget_remaining").unwrap_or(None),
                        "member_images": member_images.get(&sid).cloned().unwrap_or_default(),
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
         invite_pool_remaining, suggested_contribution, total_contributions, qr_token,
         anchor_creature_id, anchor_transferred_at
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
                "anchor_creature_id": row.try_get::<Option<Uuid>, _>("anchor_creature_id").ok().flatten(),
                "anchor_transferred_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("anchor_transferred_at").ok().flatten().map(|t| t.to_rfc3339()),
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
    pub environment: Option<serde_json::Value>,
}

/// POST /api/flights — record a creature flight (3 credits)
pub async fn record_flight_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<RecordFlightRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Verify creature ownership and presence
    let pool = state.memory_store.pool();
    let creature =
        sqlx::query("SELECT owner_id, presence, visibility FROM creatures WHERE creature_id = $1")
            .bind(req.creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let creature_visibility: String = creature
        .try_get("visibility")
        .unwrap_or_else(|_| "public".to_string());

    let presence: String = creature
        .try_get("presence")
        .unwrap_or_else(|_| "active".to_string());
    if presence != "active" {
        return Err((
            StatusCode::CONFLICT,
            format!("Creature is {} — wake it first", presence),
        ));
    }

    // Enforce: one active flight per creature (no teleportation!)
    let active_flight = sqlx::query(
        "SELECT flight_id, location_name, swarm_id FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(req.creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = active_flight {
        let loc: Option<String> = row.try_get("location_name").unwrap_or(None);
        let in_swarm: bool = row
            .try_get::<Option<Uuid>, _>("swarm_id")
            .ok()
            .flatten()
            .is_some();
        let msg = if in_swarm {
            format!(
                "Creature is already in a rabble{}",
                loc.map(|l| format!(" at {}", l)).unwrap_or_default()
            )
        } else {
            format!(
                "Creature is already flying{}",
                loc.map(|l| format!(" at {}", l)).unwrap_or_default()
            )
        };
        return Err((StatusCode::CONFLICT, msg));
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

    // Auto-detect data_source: if creature has an active paired device, it's real telemetry
    let data_source = if req.beacon_id.is_some() {
        "device"
    } else {
        let has_device = sqlx::query(
            "SELECT 1 FROM creature_devices WHERE creature_id = $1 AND is_active = true LIMIT 1",
        )
        .bind(req.creature_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some();
        if has_device {
            "device"
        } else {
            "synthetic"
        }
    };

    sqlx::query(
        "INSERT INTO creature_flights (flight_id, creature_id, beacon_id, owner_id,
         h3_cell, h3_resolution, center_lat, center_lng, location_name, country_code,
         flight_pattern, swarm_id, visibility, started_at, environment, data_source)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
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
    .bind(&creature_visibility)
    .bind(now)
    .bind(&req.environment)
    .bind(data_source)
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

    // Dispatch navigator agent for flight narration (non-blocking)
    // Use the swarm's workspace if flying to a swarm, else personal workspace
    let dispatch_ws_id = if let Some(sid) = req.swarm_id {
        sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
            .bind(sid)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten())
    } else {
        sqlx::query("SELECT personal_workspace_id FROM users WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| {
                r.try_get::<Option<Uuid>, _>("personal_workspace_id")
                    .ok()
                    .flatten()
            })
    };

    if let Some(ws_id) = dispatch_ws_id {
        let state2 = state.clone();
        let user_id2 = user_id.clone();
        let loc = req
            .location_name
            .clone()
            .unwrap_or_else(|| format!("({}, {})", req.center_lat, req.center_lng));
        let cid = req.creature_id;
        tokio::spawn(async move {
            let query = format!(
                "Creature is flying from {}. Describe the habitat and what it might observe.",
                loc
            );
            match rabble_workspace::dispatch_rabble_action(
                &state2,
                ws_id,
                "navigator",
                "creature_flight",
                &query,
                &user_id2,
            )
            .await
            {
                Ok(_) => eprintln!("[rabble] Navigator described flight for creature {}", cid),
                Err(e) => eprintln!("[rabble] Navigator dispatch failed: {}", e),
            }
        });
    }

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

    // ── Dual-write: record land transition (new versioned model) ──
    {
        let flight_info = sqlx::query(
            "SELECT creature_id, center_lat, center_lng, h3_cell, swarm_id
             FROM creature_flights WHERE flight_id = $1",
        )
        .bind(flight_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        if let Some(fi) = flight_info {
            let cid: Uuid = fi.get("creature_id");
            let lat: f64 = fi.get("center_lat");
            let lng: f64 = fi.get("center_lng");
            let h3: String = fi.get("h3_cell");
            let sid: Option<Uuid> = fi.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
            let new_state = if sid.is_some() {
                "perch_rabble"
            } else {
                "perch_solo"
            };
            let transition = if sid.is_some() { "join" } else { "land" };

            let _ = creature_state::record_transition(
                pool,
                cid,
                new_state,
                Some("fly"),
                transition,
                &principal.user_id(),
                lat,
                lng,
                &h3,
                sid,
                None,
                &json!({
                    "flight_id": flight_id,
                    "duration_seconds": req.duration_seconds,
                }),
            )
            .await;
        }
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

    // Check if this creature is the anchor for any active rabble
    {
        let spawn_db = state.db.clone();
        let spawn_state = state.clone();
        let spawn_user_id = principal.user_id();
        let fid = flight_id;
        tokio::spawn(async move {
            // Get the creature_id and swarm_id for this flight
            let flight_row = sqlx::query(
                "SELECT creature_id, swarm_id FROM creature_flights WHERE flight_id = $1",
            )
            .bind(fid)
            .fetch_optional(&spawn_db)
            .await
            .ok()
            .flatten();

            let flight_row = match flight_row {
                Some(r) => r,
                None => return,
            };

            let creature_id: Uuid = match flight_row.try_get("creature_id") {
                Ok(id) => id,
                Err(_) => return,
            };
            let swarm_id: Option<Uuid> = flight_row
                .try_get::<Option<Uuid>, _>("swarm_id")
                .ok()
                .flatten();

            let swarm_id = match swarm_id {
                Some(id) => id,
                None => return,
            };

            // Check if this creature is the anchor for this swarm
            let anchor_row = sqlx::query(
                "SELECT anchor_creature_id, creator_id FROM swarm_events
                 WHERE swarm_id = $1 AND status IN ('scheduled', 'active')",
            )
            .bind(swarm_id)
            .fetch_optional(&spawn_db)
            .await
            .ok()
            .flatten();

            let anchor_row = match anchor_row {
                Some(r) => r,
                None => return,
            };

            let anchor_id: Option<Uuid> = anchor_row
                .try_get::<Option<Uuid>, _>("anchor_creature_id")
                .ok()
                .flatten();

            if anchor_id != Some(creature_id) {
                return; // Not the anchor creature, no warning needed
            }

            // Get creature name for the warning message
            let creature_name: String =
                sqlx::query("SELECT specimen_name FROM creatures WHERE creature_id = $1")
                    .bind(creature_id)
                    .fetch_optional(&spawn_db)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|r| r.try_get("specimen_name").ok())
                    .unwrap_or_else(|| "Unknown creature".to_string());

            // Post warning system message
            let _ = sqlx::query(
                "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, content, message_type)
                 VALUES ($1, $2, 'system', $3, 'system')",
            )
            .bind(Uuid::new_v4())
            .bind(swarm_id)
            .bind(format!(
                "Anchor creature {} is leaving! The rabble will dissipate unless the anchor is transferred to another creature.",
                creature_name
            ))
            .execute(&spawn_db)
            .await;

            // Set anchor_departing flag in metadata
            let _ = sqlx::query(
                "UPDATE swarm_events SET metadata = COALESCE(metadata, '{}'::jsonb) || '{\"anchor_departing\": true}'::jsonb
                 WHERE swarm_id = $1",
            )
            .bind(swarm_id)
            .execute(&spawn_db)
            .await;

            // Dispatch anchor_departing to compound agents
            let ws_id: Option<Uuid> =
                sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
                    .bind(swarm_id)
                    .fetch_optional(&spawn_db)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten());

            if let Some(ws_id) = ws_id {
                let query = format!(
                    "anchor_departing: Anchor creature {} is leaving the rabble! List eligible transfer targets.",
                    creature_name
                );
                let _ = rabble_workspace::dispatch_rabble_action(
                    &spawn_state,
                    ws_id,
                    "rabble_anchor_manager",
                    "anchor_departing",
                    &query,
                    &spawn_user_id,
                )
                .await;
            }
        });
    }

    Ok(Json(json!({
        "flight_id": flight_id,
        "ended_at": now.to_rfc3339(),
        "duration_seconds": req.duration_seconds,
        "has_path": req.path_samples.is_some(),
    })))
}

// ─── Flight planning (agentic) ──────────────────────────────────────

#[derive(Deserialize)]
pub struct PlanFlightRequest {
    pub creature_id: Uuid,
    pub origin: serde_json::Value,      // { lat, lng, name? }
    pub destination: serde_json::Value, // { lat, lng, name? }
    pub prompt: Option<String>,         // optional creative route description
    pub swarm_id: Option<Uuid>,         // if planning for a rabble (tiered pricing)
}

/// POST /api/flights/plan — generate an agentic flight plan via flight_coordinator
pub async fn plan_flight_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<PlanFlightRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creature ownership
    let creature = sqlx::query(
        "SELECT owner_id, species_group, specimen_name, scientific_name FROM creatures WHERE creature_id = $1",
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

    let species: String = creature.get("species_group");
    let specimen_name: Option<String> = creature.try_get("specimen_name").unwrap_or(None);
    let scientific_name: Option<String> = creature.try_get("scientific_name").unwrap_or(None);

    // Tiered pricing: solo = 5cr, rabble = 5cr base + 1cr per creature
    let (total_cost, creature_count) = if let Some(swarm_id) = req.swarm_id {
        let count: i64 = sqlx::query("SELECT creature_count FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map(|r| r.try_get::<i32, _>("creature_count").unwrap_or(1) as i64)
            .unwrap_or(1);
        (
            state.gas_fees.flight_plan + count.max(1) as i32,
            count as i32,
        )
    } else {
        (state.gas_fees.flight_plan, 1)
    };

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        total_cost,
        "flight_plan",
        &format!(
            "Flight plan for {} ({} creature{})",
            req.swarm_id
                .map(|s| format!("rabble {}", s))
                .unwrap_or_else(|| format!("creature {}", req.creature_id)),
            creature_count,
            if creature_count != 1 { "s" } else { "" },
        ),
        Some(&req.creature_id.to_string()),
    )
    .await?;

    // Use rabble workspace if swarm_id provided, else personal workspace
    let ws_id = if let Some(swarm_id) = req.swarm_id {
        sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten())
    } else {
        None
    };

    let ws_id = match ws_id {
        Some(id) => id,
        None => sqlx::query("SELECT personal_workspace_id FROM users WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Uuid>, _>("personal_workspace_id")
                    .ok()
                    .flatten()
            })
            .ok_or((
                StatusCode::BAD_REQUEST,
                "No personal workspace — mint a creature first".to_string(),
            ))?,
    };

    // Build agent query with origin/destination and creature context
    let origin_name = req
        .origin
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("origin");
    let dest_name = req
        .destination
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("destination");
    let origin_lat = req
        .origin
        .get("lat")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let origin_lng = req
        .origin
        .get("lng")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let dest_lat = req
        .destination
        .get("lat")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let dest_lng = req
        .destination
        .get("lng")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let creature_label =
        specimen_name.unwrap_or_else(|| scientific_name.unwrap_or_else(|| species.clone()));

    let query =
        if let Some(ref prompt) = req.prompt {
            format!(
            "Plan a flight for {} (species: {}) from {} ({},{}) to {} ({},{}). Creative route: {}",
            creature_label, species, origin_name, origin_lat, origin_lng,
            dest_name, dest_lat, dest_lng, prompt,
        )
        } else {
            format!(
                "Plan a flight for {} (species: {}) from {} ({},{}) to {} ({},{}).",
                creature_label,
                species,
                origin_name,
                origin_lat,
                origin_lng,
                dest_name,
                dest_lat,
                dest_lng,
            )
        };

    // Dispatch to flight_coordinator compound agent
    let agent_result = rabble_workspace::dispatch_rabble_action(
        &state,
        ws_id,
        "flight_coordinator",
        "flight_plan",
        &query,
        &user_id,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Agent dispatch failed: {}", e),
        )
    })?;

    // Try to extract structured JSON from agent response
    let plan: serde_json::Value = extract_json_from_response(&agent_result).unwrap_or_else(|| {
        // Fallback: return the raw text as narrative with minimal structure
        json!({
            "version": 1,
            "creature_id": req.creature_id,
            "species": species,
            "origin": req.origin,
            "destination": req.destination,
            "narrative": agent_result,
            "waypoints": [],
            "segments": [],
        })
    });

    Ok(Json(json!({
        "plan": plan,
        "gas_charged": total_cost,
        "creature_count": creature_count,
        "pricing": if req.swarm_id.is_some() {
            format!("{}cr base + {}cr ({} creatures)", state.gas_fees.flight_plan, creature_count, creature_count)
        } else {
            format!("{}cr", state.gas_fees.flight_plan)
        },
    })))
}

/// Try to extract a JSON object from an agent response that may contain markdown fences
fn extract_json_from_response(text: &str) -> Option<serde_json::Value> {
    // Try direct parse first
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
        if val.is_object() {
            return Some(val);
        }
    }
    // Try extracting from ```json ... ``` fences
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
    // Try extracting from ``` ... ``` fences
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

// ─── Flight telemetry ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AppendTelemetryRequest {
    pub samples: Vec<serde_json::Value>,
}

/// POST /api/flights/:flight_id/telemetry — append path samples to an active flight
pub async fn append_telemetry_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(flight_id): Path<Uuid>,
    Json(req): Json<AppendTelemetryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if req.samples.is_empty() {
        return Ok(Json(json!({ "appended": 0 })));
    }
    if req.samples.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Maximum 100 samples per request".to_string(),
        ));
    }

    let pool = state.memory_store.pool();
    let samples_json = serde_json::Value::Array(req.samples.clone());

    let result = sqlx::query(
        "UPDATE creature_flights
         SET path_samples = COALESCE(path_samples, '[]'::jsonb) || $1::jsonb
         WHERE flight_id = $2 AND owner_id = $3 AND ended_at IS NULL",
    )
    .bind(&samples_json)
    .bind(flight_id)
    .bind(principal.user_id())
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Flight not found, already ended, or not owned by you".to_string(),
        ));
    }

    Ok(Json(json!({ "appended": req.samples.len() })))
}

/// GET /api/flights/:flight_id/export — export a flight as downloadable JSON
pub async fn export_flight_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(flight_id): Path<Uuid>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let user_id = principal.user_id();

    let row = sqlx::query(
        "SELECT f.flight_id, f.creature_id, f.center_lat, f.center_lng, f.location_name,
                f.flight_pattern, f.started_at, f.ended_at, f.duration_seconds, f.path_samples,
                f.environment, f.data_source, c.species_group, c.specimen_name
         FROM creature_flights f
         JOIN creatures c ON f.creature_id = c.creature_id
         WHERE f.flight_id = $1 AND (f.owner_id = $2 OR f.visibility = 'public')",
    )
    .bind(flight_id)
    .bind(&user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = row.ok_or((StatusCode::NOT_FOUND, "Flight not found".to_string()))?;

    let export = json!({
        "version": 1,
        "flight_id": row.get::<Uuid, _>("flight_id").to_string(),
        "creature_id": row.get::<Uuid, _>("creature_id").to_string(),
        "species": row.get::<String, _>("species_group"),
        "specimen_name": row.get::<Option<String>, _>("specimen_name"),
        "location": {
            "lat": row.get::<f64, _>("center_lat"),
            "lng": row.get::<f64, _>("center_lng"),
            "name": row.get::<Option<String>, _>("location_name"),
        },
        "flight_pattern": row.get::<String, _>("flight_pattern"),
        "started_at": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at").to_rfc3339(),
        "ended_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at").map(|t| t.to_rfc3339()),
        "duration_seconds": row.get::<Option<i32>, _>("duration_seconds"),
        "path_samples": row.get::<Option<serde_json::Value>, _>("path_samples"),
        "environment": row.try_get::<Option<serde_json::Value>, _>("environment").unwrap_or(None),
        "data_source": row.try_get::<String, _>("data_source").unwrap_or_else(|_| "synthetic".to_string()),
    });

    let body = serde_json::to_string_pretty(&export)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let filename = format!("flight-{}.json", flight_id);
    Ok(axum::response::Response::builder()
        .header("Content-Type", "application/json")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(axum::body::Body::from(body))
        .unwrap())
}

#[derive(Deserialize)]
pub struct ImportFlightRequest {
    pub creature_id: Uuid,
    pub location: Option<serde_json::Value>,
    pub flight_pattern: Option<String>,
    pub duration_seconds: Option<i32>,
    pub path_samples: serde_json::Value,
}

/// POST /api/flights/import — import a recorded flight (replay)
pub async fn import_flight_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ImportFlightRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let user_id = principal.user_id();

    // Verify creature belongs to caller
    let creature =
        sqlx::query("SELECT creature_id FROM creatures WHERE creature_id = $1 AND owner_id = $2")
            .bind(req.creature_id)
            .bind(&user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if creature.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature not found or not owned by you".to_string(),
        ));
    }

    let flight_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let lat = req
        .location
        .as_ref()
        .and_then(|l| l.get("lat"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let lng = req
        .location
        .as_ref()
        .and_then(|l| l.get("lng"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let loc_name = req
        .location
        .as_ref()
        .and_then(|l| l.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let pattern = req.flight_pattern.as_deref().unwrap_or("replay");

    sqlx::query(
        "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
         center_lat, center_lng, location_name,
         flight_pattern, started_at, ended_at, duration_seconds, path_samples)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(flight_id)
    .bind(req.creature_id)
    .bind(&user_id)
    .bind(lat)
    .bind(lng)
    .bind(&loc_name)
    .bind(pattern)
    .bind(now)
    .bind(now) // ended_at = now (it's a completed replay)
    .bind(req.duration_seconds)
    .bind(&req.path_samples)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "flight_id": flight_id,
        "creature_id": req.creature_id,
        "flight_pattern": pattern,
        "imported_at": now.to_rfc3339(),
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
    creature_state::init_conditions(pool, creature_id, "public", false).await;

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
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub funding_mode: Option<String>,
    pub invite_pool: Option<i32>,
    pub suggested_contribution: Option<i32>,
    pub visibility: Option<String>,
    pub anchor_creature_id: Option<Uuid>,
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
    let starts_at = if let Some(ref s) = req.starts_at {
        chrono::DateTime::parse_from_rfc3339(s)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid starts_at: {}", e)))?
            .with_timezone(&chrono::Utc)
    } else {
        now
    };
    // No ends_at = persistent rabble (10 years out)
    let ends_at = if let Some(ref s) = req.ends_at {
        chrono::DateTime::parse_from_rfc3339(s)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid ends_at: {}", e)))?
            .with_timezone(&chrono::Utc)
    } else {
        now + chrono::Duration::days(3650)
    };
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
         suggested_contribution, qr_token, visibility, anchor_creature_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'scheduled', $15,
                 $16, $17, $17, $18, $19, $20, $21)",
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
    .bind(req.anchor_creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Auto-create workspace for this rabble + hire system agents
    let workspace_id =
        rabble_workspace::create_rabble_workspace(&state, &user_id, &req.name, Some(swarm_id))
            .await
            .ok();

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
        "workspace_id": workspace_id,
        "anchor_creature_id": req.anchor_creature_id,
    })))
}

// ── Perch + Fly model ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PerchRequest {
    pub h3_cell: String,
    pub center_lat: f64,
    pub center_lng: f64,
    pub location_name: Option<String>,
    /// NULL = private (contacts/invitees only), 0 = free open, 2+ = paid walk-in
    pub walk_in_price: Option<i32>,
    /// Credits to pre-fund for invited/contact joins (default 0)
    pub invite_pool: Option<i32>,
    /// Spending cap for free walk-ins (walk_in_price=0). Host pays per join. (default 0)
    pub walk_in_budget: Option<i32>,
    /// Display name for the perch (default: "{creature_name}'s perch")
    pub name: Option<String>,
}

/// POST /api/creatures/:creature_id/perch — place creature at a location (2cr + invite pool)
///
/// Creates a swarm_events row (no workspace yet — created on first join) and a flight record.
/// The creature is now discoverable on the map with idle animations.
pub async fn perch_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PerchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate creature ownership + presence
    let creature = sqlx::query(
        "SELECT owner_id, specimen_name, scientific_name, species_group, presence, visibility
         FROM creatures WHERE creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let presence: String = creature
        .try_get("presence")
        .unwrap_or_else(|_| "active".to_string());
    if presence != "active" {
        return Err((
            StatusCode::CONFLICT,
            format!("Creature is {} — wake it first", presence),
        ));
    }

    // Enforce: one active flight per creature
    let active_flight = sqlx::query(
        "SELECT flight_id, location_name, swarm_id FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = active_flight {
        let loc: Option<String> = row.try_get("location_name").unwrap_or(None);
        let in_swarm: bool = row
            .try_get::<Option<Uuid>, _>("swarm_id")
            .ok()
            .flatten()
            .is_some();
        let msg = if in_swarm {
            format!(
                "Creature is already in a rabble{}",
                loc.map(|l| format!(" at {}", l)).unwrap_or_default()
            )
        } else {
            format!(
                "Creature is already flying{}",
                loc.map(|l| format!(" at {}", l)).unwrap_or_default()
            )
        };
        return Err((StatusCode::CONFLICT, msg));
    }

    let invite_pool = req.invite_pool.unwrap_or(0).max(0);
    let walk_in_budget = req.walk_in_budget.unwrap_or(0).max(0);

    // Validate: free walk-in (price=0) requires a budget cap
    if req.walk_in_price == Some(0) && walk_in_budget == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Free walk-in requires a spending cap (walk_in_budget > 0)".to_string(),
        ));
    }

    let total_cost = 2 + invite_pool + walk_in_budget; // 2cr base + pools

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        total_cost,
        "perch",
        &format!(
            "Perch creature {} ({}cr + {}cr invite + {}cr walk-in)",
            creature_id, 2, invite_pool, walk_in_budget
        ),
        Some(&creature_id.to_string()),
    )
    .await?;

    // Derive perch name
    let creature_name: String = creature.try_get("specimen_name").unwrap_or_else(|_| {
        creature
            .try_get("scientific_name")
            .unwrap_or("creature".into())
    });
    let perch_name = req
        .name
        .unwrap_or_else(|| format!("{}'s perch", creature_name));

    // Visibility: NULL walk_in_price = private, anything else = public
    let visibility = if req.walk_in_price.is_none() {
        "private"
    } else {
        "public"
    };

    let swarm_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let ends_at = now + chrono::Duration::days(3650); // persistent
    let qr_token = generate_qr_token();

    // Create swarm_events row
    sqlx::query(
        "INSERT INTO swarm_events (swarm_id, creator_id, h3_cell, h3_resolution,
         center_lat, center_lng, location_name,
         name, starts_at, ends_at, status, created_at,
         funding_mode, invite_pool, invite_pool_remaining,
         qr_token, visibility, anchor_creature_id, walk_in_price,
         walk_in_budget, walk_in_budget_remaining,
         participant_count, creature_count)
         VALUES ($1, $2, $3, 12, $4, $5, $6, $7, $8, $9, 'active', $8,
                 'hosted', $10, $10, $11, $12, $13, $14,
                 $15, $15, 1, 1)",
    )
    .bind(swarm_id)
    .bind(&user_id)
    .bind(&req.h3_cell)
    .bind(req.center_lat)
    .bind(req.center_lng)
    .bind(&req.location_name)
    .bind(&perch_name)
    .bind(now)
    .bind(ends_at)
    .bind(invite_pool)
    .bind(&qr_token)
    .bind(visibility)
    .bind(creature_id)
    .bind(req.walk_in_price)
    .bind(walk_in_budget)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Create workspace immediately so solo flights have agents for flight plans
    if let Ok(ws_id) =
        rabble_workspace::create_rabble_workspace(&state, &user_id, &perch_name, Some(swarm_id))
            .await
    {
        eprintln!("[perch] Created workspace {} for swarm {}", ws_id, swarm_id);
    }

    // Create flight record (pattern = 'perch' — grounded with idle animations)
    let flight_id = Uuid::new_v4();
    let creature_visibility: String = creature
        .try_get("visibility")
        .unwrap_or_else(|_| "public".to_string());

    sqlx::query(
        "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
         h3_cell, h3_resolution, center_lat, center_lng, location_name,
         flight_pattern, swarm_id, visibility, started_at, data_source)
         VALUES ($1, $2, $3, $4, 12, $5, $6, $7, 'perch', $8, $9, $10, 'synthetic')",
    )
    .bind(flight_id)
    .bind(creature_id)
    .bind(&user_id)
    .bind(&req.h3_cell)
    .bind(req.center_lat)
    .bind(req.center_lng)
    .bind(&req.location_name)
    .bind(swarm_id)
    .bind(&creature_visibility)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update creature stats
    sqlx::query(
        "UPDATE creatures SET total_flights = total_flights + 1, updated_at = NOW()
         WHERE creature_id = $1",
    )
    .bind(creature_id)
    .execute(pool)
    .await
    .ok();

    // ── Dual-write: record perch transition (new versioned model) ──
    let _ = creature_state::record_transition(
        pool,
        creature_id,
        "perch_solo",
        None, // initial placement — no previous state
        "perch",
        &user_id,
        req.center_lat,
        req.center_lng,
        &req.h3_cell,
        Some(swarm_id),
        None, // workspace_id filled when workspace is created
        &json!({
            "flight_id": flight_id,
            "swarm_id": swarm_id,
            "perch_name": perch_name,
            "walk_in_price": req.walk_in_price,
        }),
    )
    .await;

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "flight_id": flight_id,
        "creature_id": creature_id,
        "qr_token": qr_token,
        "name": perch_name,
        "walk_in_price": req.walk_in_price,
        "invite_pool": invite_pool,
        "walk_in_budget": walk_in_budget,
        "visibility": visibility,
        "total_cost": total_cost,
    })))
}

#[derive(Deserialize)]
pub struct FlyRequest {
    /// Optional destination — omit for free-form wander
    pub destination: Option<serde_json::Value>, // { lat, lng, name? }
    /// Optional creative route prompt for agent
    pub prompt: Option<String>,
}

/// POST /api/creatures/:creature_id/fly — activate flight dynamics on an active perch (1cr + agent pass-through)
///
/// Creature must already be perched (active flight with swarm_id, pattern='perch').
/// Updates flight_pattern to 'fly' and optionally dispatches flight_coordinator agent.
pub async fn fly_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<FlyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate creature ownership
    let creature = sqlx::query(
        "SELECT owner_id, species_group, specimen_name, scientific_name, presence
         FROM creatures WHERE creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let presence: String = creature
        .try_get("presence")
        .unwrap_or_else(|_| "active".to_string());
    if presence != "active" {
        return Err((
            StatusCode::CONFLICT,
            format!("Creature is {} — wake it first", presence),
        ));
    }

    // Must have an active perch (flight with swarm_id and ended_at IS NULL)
    let active_flight = sqlx::query(
        "SELECT flight_id, swarm_id, flight_pattern, location_name
         FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::CONFLICT,
        "Creature is not perched — perch first".to_string(),
    ))?;

    let flight_id: Uuid = active_flight.get("flight_id");
    let swarm_id: Option<Uuid> = active_flight
        .try_get::<Option<Uuid>, _>("swarm_id")
        .ok()
        .flatten();

    if swarm_id.is_none() {
        return Err((
            StatusCode::CONFLICT,
            "Creature is on a solo flight without a perch — end it first".to_string(),
        ));
    }
    let swarm_id = swarm_id.unwrap();

    // Charge 1cr for fly activation
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        1,
        "fly",
        &format!("Fly creature {} from perch", creature_id),
        Some(&creature_id.to_string()),
    )
    .await?;

    // Update flight pattern from 'perch' to 'fly'
    sqlx::query("UPDATE creature_flights SET flight_pattern = 'fly' WHERE flight_id = $1")
        .bind(flight_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Dual-write: record fly transition (new versioned model) ──
    {
        // Get current state to record previous_state
        let prev = creature_state::get_current_state(pool, creature_id).await;
        let prev_state = prev.as_ref().map(|(s, _)| s.as_str());
        let _ = creature_state::record_transition(
            pool,
            creature_id,
            "fly",
            prev_state,
            "fly",
            &user_id,
            0.0, // in transit — no fixed location
            0.0,
            "",
            None, // leaving rabble
            None,
            &json!({
                "flight_id": flight_id,
                "from_swarm_id": swarm_id,
                "destination": req.destination,
            }),
        )
        .await;
    }

    // Store destination in flight metadata if provided
    if let Some(ref dest) = req.destination {
        sqlx::query(
            "UPDATE creature_flights SET environment = COALESCE(environment, '{}'::jsonb) || jsonb_build_object('destination', $1::jsonb)
             WHERE flight_id = $2",
        )
        .bind(dest)
        .bind(flight_id)
        .execute(pool)
        .await
        .ok();
    }

    // Determine mode: solo (no other creatures in swarm) or rabble
    let creature_count: i64 =
        sqlx::query("SELECT creature_count FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map(|r| r.try_get::<i32, _>("creature_count").unwrap_or(1) as i64)
            .unwrap_or(1);

    let mode = if creature_count > 1 { "rabble" } else { "solo" };

    // If destination or prompt provided, dispatch flight_coordinator agent (async, non-blocking)
    if req.destination.is_some() || req.prompt.is_some() {
        // Find workspace: swarm's workspace or personal
        let ws_id = sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten());

        let ws_id = match ws_id {
            Some(id) => id,
            None => sqlx::query("SELECT personal_workspace_id FROM users WHERE user_id = $1")
                .bind(&user_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .and_then(|r| {
                    r.try_get::<Option<Uuid>, _>("personal_workspace_id")
                        .ok()
                        .flatten()
                })
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "No workspace available — mint a creature first".to_string(),
                ))?,
        };

        let species: String = creature.get("species_group");
        let specimen_name: Option<String> = creature.try_get("specimen_name").unwrap_or(None);
        let scientific_name: Option<String> = creature.try_get("scientific_name").unwrap_or(None);
        let creature_label =
            specimen_name.unwrap_or_else(|| scientific_name.unwrap_or_else(|| species.clone()));

        let loc_name: String = active_flight
            .try_get("location_name")
            .unwrap_or_else(|_| "current location".to_string());

        let query = if let Some(ref dest) = req.destination {
            let dest_name = dest
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("destination");
            if let Some(ref prompt) = req.prompt {
                format!(
                    "Plan a flight for {} ({}) from {} to {}. Creative route: {}",
                    creature_label, species, loc_name, dest_name, prompt,
                )
            } else {
                format!(
                    "Plan a flight for {} ({}) from {} to {}.",
                    creature_label, species, loc_name, dest_name,
                )
            }
        } else {
            format!(
                "Describe a wandering flight for {} ({}) around {}. {}",
                creature_label,
                species,
                loc_name,
                req.prompt.as_deref().unwrap_or(""),
            )
        };

        // Dispatch flight_coordinator async — don't block the HTTP response.
        // Flight plan arrives as a workspace message (same pattern as dream_narrator).
        let spawn_state = state.clone();
        let spawn_user = user_id.clone();
        let spawn_creature = creature_id;
        let spawn_flight = flight_id;
        tokio::spawn(async move {
            match rabble_workspace::dispatch_rabble_action(
                &spawn_state,
                ws_id,
                "flight_coordinator",
                "fly",
                &query,
                &spawn_user,
            )
            .await
            {
                Ok(result) => {
                    // Store flight plan reference on the flight record
                    let _ = sqlx::query(
                        "UPDATE creature_flights SET metadata = jsonb_set(
                            COALESCE(metadata, '{}'::jsonb), '{flight_plan}', $1::jsonb
                        ) WHERE flight_id = $2",
                    )
                    .bind(serde_json::to_string(&result).unwrap_or_default())
                    .bind(spawn_flight)
                    .execute(spawn_state.memory_store.pool())
                    .await
                    .ok();
                    eprintln!(
                        "[fly] flight_coordinator completed for creature {}",
                        spawn_creature
                    );
                }
                Err(e) => {
                    eprintln!("[fly] flight_coordinator dispatch failed: {}", e);
                }
            }
        });
    }

    Ok(Json(json!({
        "flight_id": flight_id,
        "swarm_id": swarm_id,
        "creature_id": creature_id,
        "mode": mode,
        "pattern": "fly",
        "plan": "generating",
        "gas_charged": 1,
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
         funding_mode, invite_pool_remaining, suggested_contribution,
         walk_in_price, walk_in_budget_remaining, workspace_id, participant_count
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

    // Verify creature ownership and presence
    let creature = sqlx::query(
        "SELECT owner_id, specimen_name, scientific_name AS species_name, species_group, presence FROM creatures WHERE creature_id = $1"
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

    let presence: String = creature
        .try_get("presence")
        .unwrap_or_else(|_| "active".to_string());
    if presence != "active" {
        return Err((
            StatusCode::CONFLICT,
            format!("Creature is {} — wake it first", presence),
        ));
    }

    let creature_name: Option<String> = creature.try_get("specimen_name").ok();
    let species_name: Option<String> = creature.try_get("species_name").ok();
    let species_group: Option<String> = creature.try_get("species_group").ok();

    // Enforce: one active flight per creature (no being in two places at once)
    let active_flight = sqlx::query(
        "SELECT flight_id, swarm_id, location_name FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(req.creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = active_flight {
        let existing_swarm: Option<Uuid> =
            row.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
        if existing_swarm == Some(swarm_id) {
            return Err((StatusCode::CONFLICT, "Already in this rabble".to_string()));
        }
        let msg = if existing_swarm.is_some() {
            "Creature is in another rabble — leave first"
        } else {
            "Creature is on a flight — end it first"
        };
        return Err((StatusCode::CONFLICT, msg.to_string()));
    }

    // Two-doors access model
    let walk_in_price: Option<i32> = swarm.try_get("walk_in_price").unwrap_or(None);

    if creator_id != user_id {
        // Check if joiner is a contact of the host OR has an explicit invite (object_shares)
        let is_contact_or_invited = sqlx::query(
            "SELECT 1 FROM contacts WHERE user_id = $1 AND contact_id = $2
             UNION ALL
             SELECT 1 FROM object_shares
             WHERE object_type = 'rabble' AND object_id = $3::text
             AND (share_target = $2 OR share_target IN
                  (SELECT team_id::text FROM team_members WHERE user_id = $2))
             LIMIT 1",
        )
        .bind(&creator_id)
        .bind(&user_id)
        .bind(swarm_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_some();

        if is_contact_or_invited {
            // Invited door: use invite pool
            let remaining: i32 = swarm.try_get("invite_pool_remaining").unwrap_or(0);
            if remaining > 0 {
                sqlx::query("UPDATE swarm_events SET invite_pool_remaining = invite_pool_remaining - 1 WHERE swarm_id = $1")
                    .bind(swarm_id)
                    .execute(pool)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            } else if let Some(price) = walk_in_price {
                // Pool exhausted but walk-in door exists — charge walk-in price
                if price > 0 {
                    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                        .await
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    charge_gas(
                        &state.db,
                        wallet.wallet_id,
                        price,
                        "walk_in_fee",
                        &format!("Walk-in fee for rabble {} ({}cr)", swarm_id, price),
                        Some(&swarm_id.to_string()),
                    )
                    .await?;

                    // Revenue to host (90%, platform keeps 10%)
                    let host_revenue = (price as f64 * 0.9).round() as i32;
                    if host_revenue > 0 {
                        if let Ok(host_wallet) =
                            get_or_create_wallet(&state.db, "user", &creator_id).await
                        {
                            let _ = fermi_auth::credit_deposit_typed(
                                &state.db,
                                host_wallet.wallet_id,
                                host_revenue,
                                "walk_in_revenue",
                                &format!("Walk-in revenue from rabble {}", swarm_id),
                            )
                            .await;
                        }
                    }
                }
                // price == 0: free walk-in — host pays from walk_in_budget
                if price == 0 {
                    let budget_left: i32 = swarm.try_get("walk_in_budget_remaining").unwrap_or(0);
                    if budget_left > 0 {
                        let host_wallet_for_free =
                            get_or_create_wallet(&state.db, "user", &creator_id)
                                .await
                                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                        charge_gas(
                            &state.db,
                            host_wallet_for_free.wallet_id,
                            1,
                            "walk_in_fee",
                            &format!(
                                "Free walk-in fee (host-paid, invite overflow) for rabble {}",
                                swarm_id
                            ),
                            Some(&swarm_id.to_string()),
                        )
                        .await?;
                        sqlx::query("UPDATE swarm_events SET walk_in_budget_remaining = walk_in_budget_remaining - 1 WHERE swarm_id = $1 AND walk_in_budget_remaining > 0")
                            .bind(swarm_id)
                            .execute(pool)
                            .await
                            .ok();
                    }
                    // If budget exhausted, contact still gets in free (invite pool already exhausted is a soft limit for contacts)
                }
            } else {
                // walk_in_price is NULL (private) and pool exhausted
                return Err((StatusCode::PAYMENT_REQUIRED, "Invite pool exhausted".into()));
            }
        } else {
            // Stranger — walk-in door only
            match walk_in_price {
                None => {
                    return Err((
                        StatusCode::FORBIDDEN,
                        "Private perch — need an invite to join".into(),
                    ));
                }
                Some(0) => {
                    // Free walk-in — host pays from walk_in_budget
                    let budget_left: i32 = swarm.try_get("walk_in_budget_remaining").unwrap_or(0);
                    if budget_left <= 0 {
                        return Err((
                            StatusCode::PAYMENT_REQUIRED,
                            "Host's walk-in budget is exhausted".into(),
                        ));
                    }
                    // Charge host 1cr per free walk-in
                    let host_wallet = get_or_create_wallet(&state.db, "user", &creator_id)
                        .await
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    charge_gas(
                        &state.db,
                        host_wallet.wallet_id,
                        1,
                        "walk_in_fee",
                        &format!("Free walk-in fee (host-paid) for rabble {}", swarm_id),
                        Some(&swarm_id.to_string()),
                    )
                    .await?;
                    // Decrement budget
                    sqlx::query("UPDATE swarm_events SET walk_in_budget_remaining = walk_in_budget_remaining - 1 WHERE swarm_id = $1 AND walk_in_budget_remaining > 0")
                        .bind(swarm_id)
                        .execute(pool)
                        .await
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                }
                Some(price) => {
                    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                        .await
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    charge_gas(
                        &state.db,
                        wallet.wallet_id,
                        price,
                        "walk_in_fee",
                        &format!("Walk-in fee for rabble {} ({}cr)", swarm_id, price),
                        Some(&swarm_id.to_string()),
                    )
                    .await?;

                    // Revenue to host (90%, platform keeps 10%)
                    let host_revenue = (price as f64 * 0.9).round() as i32;
                    if host_revenue > 0 {
                        if let Ok(host_wallet) =
                            get_or_create_wallet(&state.db, "user", &creator_id).await
                        {
                            let _ = fermi_auth::credit_deposit_typed(
                                &state.db,
                                host_wallet.wallet_id,
                                host_revenue,
                                "walk_in_revenue",
                                &format!("Walk-in revenue from rabble {}", swarm_id),
                            )
                            .await;
                        }
                    }
                }
            }
        }
    }
    // else: creator joins own perch for free

    // First non-host join: create workspace + "We have a rabble!!" moment
    let existing_ws: Option<Uuid> = swarm
        .try_get::<Option<Uuid>, _>("workspace_id")
        .ok()
        .flatten();
    let participant_count: i32 = swarm.try_get("participant_count").unwrap_or(0);
    let is_first_join = existing_ws.is_none() && creator_id != user_id;

    if is_first_join {
        // Create workspace now (deferred from perch time)
        let swarm_name: String = sqlx::query("SELECT name FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .map(|r| r.try_get::<String, _>("name").unwrap_or("rabble".into()))
            .unwrap_or("rabble".into());

        if let Ok(ws_id) = rabble_workspace::create_rabble_workspace(
            &state,
            &creator_id,
            &swarm_name,
            Some(swarm_id),
        )
        .await
        {
            eprintln!(
                "[perch] First join — created workspace {} for swarm {}",
                ws_id, swarm_id
            );
        }
    }

    // Legacy support: handle old swarms with funding_mode = 'support' that don't have walk_in_price
    if walk_in_price.is_none() && funding_mode == "support" && creator_id != user_id {
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

    // ── Dual-write: record join transition (new versioned model) ──
    {
        let prev = creature_state::get_current_state(pool, req.creature_id).await;
        let prev_state = prev.as_ref().map(|(s, _)| s.as_str());
        let _ = creature_state::record_transition(
            pool,
            req.creature_id,
            "perch_rabble",
            prev_state,
            "join",
            &user_id,
            lat,
            lng,
            &h3_cell,
            Some(swarm_id),
            existing_ws,
            &json!({
                "flight_id": flight_id,
                "swarm_id": swarm_id,
                "first_join": is_first_join,
            }),
        )
        .await;
    }

    // Post system message — special message for first join (perch → rabble transition)
    let display_name = creature_name.as_deref().unwrap_or("A creature");
    let species_display = species_name.as_deref().unwrap_or("unknown species");

    if is_first_join {
        let _ =
            super::rabble_chat::insert_system_message(&state, swarm_id, "We have a rabble!!").await;
    }

    let _ = super::rabble_chat::insert_system_message(
        &state,
        swarm_id,
        &format!(
            "{} ({}) has joined the rabble!",
            display_name, species_display
        ),
    )
    .await;

    // Route through workspace agents (swarm_host welcome + keeper log)
    let swarm_ws_id: Option<Uuid> =
        sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten());

    if let Some(ws_id) = swarm_ws_id {
        // Dispatch swarm_host welcome via workspace
        let state2 = state.clone();
        let user_id2 = user_id.clone();
        let c_name = creature_name
            .clone()
            .unwrap_or_else(|| "creature".to_string());
        let s_name = species_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let s_group = species_group
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        tokio::spawn(async move {
            let query = format!(
                "Welcome {} ({}, {}) to the rabble! Share a fun taxonomic fact.",
                c_name, s_name, s_group
            );
            if let Ok(welcome) = rabble_workspace::dispatch_rabble_action(
                &state2,
                ws_id,
                "swarm_host",
                "swarm_join",
                &query,
                &user_id2,
            )
            .await
            {
                // Insert welcome as narrator message in rabble chat
                let _ = sqlx::query(
                    "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, creature_id, creature_name, content, message_type)
                     VALUES ($1, $2, 'system', NULL, 'Swarm Host', $3, 'narrator')"
                )
                .bind(Uuid::new_v4())
                .bind(swarm_id)
                .bind(&welcome)
                .execute(&state2.db)
                .await;
            }
        });

        // Also dispatch lifecycle coordinator (fire-and-forget)
        let state3 = state.clone();
        let user_id3 = user_id.clone();
        let c_name2 = creature_name
            .clone()
            .unwrap_or_else(|| "creature".to_string());
        let s_name2 = species_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        tokio::spawn(async move {
            let query = format!(
                "participant_joined: {} ({}) has joined the rabble.",
                c_name2, s_name2
            );
            let _ = rabble_workspace::dispatch_rabble_action(
                &state3,
                ws_id,
                "rabble_lifecycle_coordinator",
                "participant_joined",
                &query,
                &user_id3,
            )
            .await;
        });
    } else {
        // Fallback: legacy swarm host welcome (no workspace yet)
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
    }

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "flight_id": flight_id,
        "creature_id": req.creature_id,
        "joined": true,
        "funding_mode": funding_mode,
        "first_join": is_first_join,
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

// ─── Creature presence (active/sleeping/parked) ───────────────────

#[derive(Deserialize)]
pub struct UpdatePresenceRequest {
    pub presence: String,
}

/// PUT /api/creatures/:creature_id/presence — set creature presence state.
/// Owner only. Dispatches keeper agent to log the transition.
pub async fn update_creature_presence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<UpdatePresenceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    if !["active", "sleeping", "parked"].contains(&req.presence.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Presence must be 'active', 'sleeping', or 'parked'".to_string(),
        ));
    }

    let creature = sqlx::query(
        "SELECT owner_id, specimen_name, personal_workspace_id FROM creatures c
         JOIN users u ON u.user_id = c.owner_id
         WHERE c.creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let specimen_name: String = creature.try_get("specimen_name").unwrap_or_default();
    let personal_ws: Option<Uuid> = creature
        .try_get::<Option<Uuid>, _>("personal_workspace_id")
        .ok()
        .flatten();

    let result = sqlx::query(
        "UPDATE creatures SET presence = $1, presence_changed_at = NOW(), updated_at = NOW()
         WHERE creature_id = $2 AND owner_id = $3",
    )
    .bind(&req.presence)
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

    // Dispatch keeper agent to log the transition (non-blocking)
    if let Some(ws_id) = personal_ws {
        let state2 = state.clone();
        let user_id2 = user_id.clone();
        let presence2 = req.presence.clone();
        let name2 = specimen_name.clone();
        tokio::spawn(async move {
            let query = format!(
                "Creature {} is now {}. Log the transition.",
                name2, presence2
            );
            let _ = rabble_workspace::dispatch_rabble_action(
                &state2,
                ws_id,
                "keeper",
                "presence_change",
                &query,
                &user_id2,
            )
            .await;
        });
    }

    Ok(Json(json!({
        "creature_id": creature_id,
        "presence": req.presence,
    })))
}

// ─── Device pairing handlers ──────────────────────────────────────

#[derive(Deserialize)]
pub struct PairDeviceRequest {
    pub device_type: String,
    pub device_identifier: String,
    pub device_name: Option<String>,
}

/// GET /api/creatures/:id/devices — list paired devices
pub async fn list_devices_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let rows = sqlx::query(
        "SELECT device_id, creature_id, device_type, device_identifier, device_name,
         is_active, last_lat, last_lng, last_seen_at, created_at
         FROM creature_devices WHERE creature_id = $1 AND owner_id = $2
         ORDER BY created_at DESC",
    )
    .bind(creature_id)
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let devices: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "device_id": row.get::<Uuid, _>("device_id"),
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "device_type": row.get::<String, _>("device_type"),
                "device_identifier": row.get::<String, _>("device_identifier"),
                "device_name": row.get::<Option<String>, _>("device_name"),
                "is_active": row.get::<bool, _>("is_active"),
                "last_lat": row.get::<Option<f64>, _>("last_lat"),
                "last_lng": row.get::<Option<f64>, _>("last_lng"),
                "last_seen_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_seen_at").map(|dt| dt.to_rfc3339()),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "devices": devices })))
}

/// POST /api/creatures/:id/devices — pair a device
pub async fn pair_device_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PairDeviceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creature ownership
    let owns = sqlx::query("SELECT 1 FROM creatures WHERE creature_id = $1 AND owner_id = $2")
        .bind(creature_id)
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if owns.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature not found or not owned by you".to_string(),
        ));
    }

    let device_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO creature_devices (device_id, creature_id, owner_id, device_type, device_identifier, device_name)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (owner_id, device_identifier) DO UPDATE SET creature_id = $2, device_type = $4, device_name = $6",
    )
    .bind(device_id)
    .bind(creature_id)
    .bind(&user_id)
    .bind(&req.device_type)
    .bind(&req.device_identifier)
    .bind(&req.device_name)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "device_id": device_id,
        "creature_id": creature_id,
        "device_type": req.device_type,
        "paired": true,
    })))
}

#[derive(Deserialize)]
pub struct UpdateDeviceRequest {
    pub device_name: Option<String>,
    pub is_active: Option<bool>,
}

/// PUT /api/devices/:device_id — update device name/active
pub async fn update_device_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(device_id): Path<Uuid>,
    Json(req): Json<UpdateDeviceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let mut sets = Vec::new();
    let device_name = req.device_name;
    let is_active = req.is_active;
    let mut bind_idx = 0u32;

    if device_name.is_some() {
        bind_idx += 1;
        sets.push(format!("device_name = ${}", bind_idx));
    }
    if is_active.is_some() {
        bind_idx += 1;
        sets.push(format!("is_active = ${}", bind_idx));
    }

    if sets.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    let device_bind = bind_idx + 1;
    let owner_bind = bind_idx + 2;
    let sql = format!(
        "UPDATE creature_devices SET {} WHERE device_id = ${} AND owner_id = ${}",
        sets.join(", "),
        device_bind,
        owner_bind
    );

    let mut query = sqlx::query(&sql);
    if let Some(ref name) = device_name {
        query = query.bind(name);
    }
    if let Some(active) = is_active {
        query = query.bind(active);
    }
    query = query.bind(device_id).bind(&user_id);

    let result = query
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Device not found".to_string()));
    }

    Ok(Json(json!({ "updated": true })))
}

/// DELETE /api/devices/:device_id — unpair device
pub async fn unpair_device_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(device_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = sqlx::query("DELETE FROM creature_devices WHERE device_id = $1 AND owner_id = $2")
        .bind(device_id)
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Device not found".to_string()));
    }

    Ok(Json(json!({ "unpaired": true })))
}

#[derive(Deserialize)]
pub struct ReportLocationRequest {
    pub lat: f64,
    pub lng: f64,
}

/// POST /api/devices/:device_id/location — report device location
pub async fn report_device_location_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(device_id): Path<Uuid>,
    Json(req): Json<ReportLocationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = sqlx::query(
        "UPDATE creature_devices SET last_lat = $1, last_lng = $2, last_seen_at = NOW()
         WHERE device_id = $3 AND owner_id = $4",
    )
    .bind(req.lat)
    .bind(req.lng)
    .bind(device_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Device not found".to_string()));
    }

    Ok(Json(json!({
        "device_id": device_id,
        "lat": req.lat,
        "lng": req.lng,
        "synced": true,
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
    let _ = sqlx::query(
        "INSERT INTO notifications (id, user_id, notification_type, title, body, created_at)
         VALUES ($1, $2, 'creature_gift', $3, $4, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(&body.recipient_id)
    .bind(format!("You received {}!", creature_name))
    .bind(format!(
        "Someone gifted you the creature '{}'",
        creature_name
    ))
    .execute(&state.db)
    .await;

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

/// GET /api/creatures/:creature_id/animation/:layer_name — serve an animation layer from DB.
pub async fn creature_animation_layer_handler(
    State(state): State<AppState>,
    Path((creature_id, layer_name)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();

    // Validate layer name
    if !["body", "left_wing", "right_wing"].contains(&layer_name.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            [
                (axum::http::header::CONTENT_TYPE, "text/plain".to_string()),
                (axum::http::header::CACHE_CONTROL, "no-cache".to_string()),
            ],
            b"Invalid layer name. Must be: body, left_wing, right_wing".to_vec(),
        )
            .into_response();
    }

    if let Ok(Some(row)) = sqlx::query(
        "SELECT image_bytes, mime_type FROM creature_animation_layers WHERE creature_id = $1 AND layer_name = $2",
    )
    .bind(creature_id)
    .bind(&layer_name)
    .fetch_optional(pool)
    .await
    {
        let bytes: Vec<u8> = row.get("image_bytes");
        let mime: String = row.get("mime_type");
        return (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (axum::http::header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            ],
            bytes,
        ).into_response();
    }

    (
        StatusCode::NOT_FOUND,
        [
            (axum::http::header::CONTENT_TYPE, "text/plain".to_string()),
            (axum::http::header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        b"Animation layer not found".to_vec(),
    )
        .into_response()
}

/// GET /api/creatures/:creature_id/animation-status — check animation readiness.
pub async fn creature_animation_status_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();

    let row =
        sqlx::query("SELECT animation_status, species_group FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let status: Option<String> = row.try_get("animation_status").unwrap_or(None);
    let species_group: String = row.get("species_group");

    let mut result = json!({
        "creature_id": creature_id,
        "species_group": species_group,
        "animation_status": status,
    });

    if status.as_deref() == Some("ready") {
        result["layers"] = json!({
            "body": format!("/api/creatures/{}/animation/body", creature_id),
            "left_wing": format!("/api/creatures/{}/animation/left_wing", creature_id),
            "right_wing": format!("/api/creatures/{}/animation/right_wing", creature_id),
        });
    }

    Ok(Json(result))
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
    let visibility = req.visibility.trim().to_lowercase();
    if !["public", "contacts", "private"].contains(&visibility.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "visibility must be 'public', 'contacts', or 'private'".to_string(),
        ));
    }

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

    sqlx::query("UPDATE creatures SET visibility = $1, updated_at = NOW() WHERE creature_id = $2")
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

// ─── Visible flights endpoint ───────────────────────────────────────

#[derive(Deserialize)]
pub struct VisibleFlightsQuery {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub radius: Option<f64>, // km, unused for now but reserved
}

/// GET /api/flights/visible — active flights visible to the current user
/// Returns public flights + contacts-only flights where viewer is a contact.
pub async fn list_visible_flights_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<VisibleFlightsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Get active flights that the viewer can see:
    // - public flights (anyone)
    // - contacts-only flights where the viewer is in the owner's contacts
    // - the viewer's own flights (always visible to self)
    // Excludes private flights from others.
    let rows = sqlx::query(
        "SELECT f.flight_id, f.creature_id, f.owner_id, f.center_lat, f.center_lng,
                f.location_name, f.flight_pattern, f.visibility, f.started_at, f.swarm_id,
                c.scientific_name, c.common_name, c.specimen_name, c.species_group,
                c.asset_path
         FROM creature_flights f
         JOIN creatures c ON c.creature_id = f.creature_id
         WHERE f.ended_at IS NULL
           AND (
             f.owner_id = $1
             OR f.visibility = 'public'
             OR (f.visibility = 'contacts'
                 AND EXISTS (
                   SELECT 1 FROM contacts
                   WHERE user_id = f.owner_id AND contact_id = $1
                 ))
           )
         ORDER BY f.started_at DESC
         LIMIT 100",
    )
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let flights: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let owner_id: String = row.get("owner_id");
            json!({
                "flight_id": row.get::<Uuid, _>("flight_id"),
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "owner_id": &owner_id,
                "is_mine": owner_id == user_id,
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "location_name": row.try_get::<Option<String>, _>("location_name").unwrap_or(None),
                "flight_pattern": row.get::<String, _>("flight_pattern"),
                "visibility": row.get::<String, _>("visibility"),
                "swarm_id": row.try_get::<Option<Uuid>, _>("swarm_id").unwrap_or(None),
                "started_at": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at").to_rfc3339(),
                "scientific_name": row.get::<String, _>("scientific_name"),
                "common_name": row.try_get::<Option<String>, _>("common_name").unwrap_or(None),
                "specimen_name": row.try_get::<Option<String>, _>("specimen_name").unwrap_or(None),
                "species_group": row.get::<String, _>("species_group"),
                "asset_path": row.get::<String, _>("asset_path"),
            })
        })
        .collect();

    Ok(Json(json!({
        "flights": flights,
        "count": flights.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Tethering — link creature to live GPS/sensor for real-time tracking
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct TetherRequest {
    pub tether_type: Option<String>, // phone_gps, meshtastic, gps_tracker, fixed_sensor
    pub device_label: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
}

/// POST /api/creatures/:creature_id/tether — tether creature to a signal source (1cr)
pub async fn tether_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<TetherRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let creature = sqlx::query(
        "SELECT owner_id, presence, specimen_name FROM creatures WHERE creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let presence: String = creature
        .try_get("presence")
        .unwrap_or_else(|_| "active".to_string());
    if presence == "sleeping" || presence == "parked" {
        return Err((
            StatusCode::CONFLICT,
            format!("Creature is {} — wake it first", presence),
        ));
    }

    // Check not already tethered
    let existing = sqlx::query(
        "SELECT tether_id FROM creature_tethers WHERE creature_id = $1 AND active = true LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_some() {
        return Err((StatusCode::CONFLICT, "Creature is already tethered".into()));
    }

    // End non-perch active flights (fly, solo). Keep perch flight alive so
    // creature retains its location after untether. Perch = "creature is here."
    sqlx::query(
        "UPDATE creature_flights SET ended_at = NOW(),
         duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
         WHERE creature_id = $1 AND ended_at IS NULL AND flight_pattern != 'perch'",
    )
    .bind(creature_id)
    .execute(pool)
    .await
    .ok();

    // Charge 1cr tether fee
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        1,
        "tether",
        &format!(
            "Tether creature {} to {}",
            creature_id,
            req.tether_type.as_deref().unwrap_or("phone_gps")
        ),
        Some(&creature_id.to_string()),
    )
    .await?;

    let tether_type = req.tether_type.as_deref().unwrap_or("phone_gps");
    let tether_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO creature_tethers (tether_id, creature_id, owner_id, tether_type, device_label, config)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tether_id)
    .bind(creature_id)
    .bind(&user_id)
    .bind(tether_type)
    .bind(&req.device_label)
    .bind(&req.config)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Set presence to tracking
    sqlx::query("UPDATE creatures SET presence = 'tracking', presence_changed_at = NOW() WHERE creature_id = $1")
        .bind(creature_id)
        .execute(pool)
        .await
        .ok();

    Ok(Json(json!({
        "tether_id": tether_id,
        "creature_id": creature_id,
        "tether_type": tether_type,
        "device_label": req.device_label,
        "status": "active",
    })))
}

/// DELETE /api/creatures/:creature_id/tether — untether creature
pub async fn untether_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let owner: String = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?
        .get("owner_id");

    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
    }

    let result = sqlx::query(
        "UPDATE creature_tethers SET active = false, deactivated_at = NOW()
         WHERE creature_id = $1 AND active = true",
    )
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "No active tether".into()));
    }

    // End any tracking flights created during tether
    sqlx::query(
        "UPDATE creature_flights SET ended_at = NOW(),
         duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
         WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device'",
    )
    .bind(creature_id)
    .execute(pool)
    .await
    .ok();

    // Set presence back to active
    sqlx::query("UPDATE creatures SET presence = 'active', presence_changed_at = NOW() WHERE creature_id = $1")
        .bind(creature_id)
        .execute(pool)
        .await
        .ok();

    Ok(Json(json!({
        "creature_id": creature_id,
        "status": "untethered",
    })))
}

#[derive(Debug, Deserialize)]
pub struct TelemetryPoint {
    pub lat: f64,
    pub lng: f64,
    pub altitude: Option<f64>,
    pub accuracy: Option<f64>,
    pub speed: Option<f64>,
    pub heading: Option<f64>,
    pub recorded_at: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct PushTelemetryRequest {
    pub points: Vec<TelemetryPoint>,
}

/// POST /api/creatures/:creature_id/telemetry — push position points from tethered device
pub async fn push_telemetry_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PushTelemetryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Get active tether
    let tether = sqlx::query(
        "SELECT tether_id, owner_id FROM creature_tethers
         WHERE creature_id = $1 AND active = true LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::NOT_FOUND,
        "No active tether for this creature".into(),
    ))?;

    let tether_owner: String = tether.get("owner_id");
    if tether_owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your tether".into()));
    }

    let tether_id: Uuid = tether.get("tether_id");
    let mut inserted = 0;

    for point in &req.points {
        let recorded_at = point
            .recorded_at
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        sqlx::query(
            "INSERT INTO telemetry_points
             (tether_id, creature_id, lat, lng, altitude, accuracy, speed, heading, metadata, recorded_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(tether_id)
        .bind(creature_id)
        .bind(point.lat)
        .bind(point.lng)
        .bind(point.altitude)
        .bind(point.accuracy)
        .bind(point.speed)
        .bind(point.heading)
        .bind(&point.metadata)
        .bind(recorded_at)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        inserted += 1;
    }

    // Update creature's flight record with latest position (for map display)
    if let Some(last) = req.points.last() {
        // Upsert: update existing tracking flight or create one
        let existing_flight = sqlx::query(
            "SELECT flight_id FROM creature_flights
             WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device' LIMIT 1",
        )
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        if let Some(row) = existing_flight {
            let flight_id: Uuid = row.get("flight_id");
            sqlx::query(
                "UPDATE creature_flights SET center_lat = $1, center_lng = $2 WHERE flight_id = $3",
            )
            .bind(last.lat)
            .bind(last.lng)
            .bind(flight_id)
            .execute(pool)
            .await
            .ok();
        } else {
            // Create a tracking flight record
            sqlx::query(
                "INSERT INTO creature_flights
                 (flight_id, creature_id, owner_id, h3_cell, h3_resolution,
                  center_lat, center_lng, flight_pattern, data_source, started_at)
                 VALUES ($1, $2, $3, '', 12, $4, $5, 'tracking', 'device', NOW())",
            )
            .bind(Uuid::new_v4())
            .bind(creature_id)
            .bind(&user_id)
            .bind(last.lat)
            .bind(last.lng)
            .execute(pool)
            .await
            .ok();
        }
    }

    Ok(Json(json!({
        "inserted": inserted,
        "creature_id": creature_id,
    })))
}

#[derive(Debug, Deserialize)]
pub struct TrackQuery {
    pub since: Option<String>, // ISO 8601, defaults to last 24h
    pub limit: Option<i64>,    // max points, defaults to 1000
}

/// GET /api/creatures/:creature_id/track — get telemetry track for visualization
pub async fn get_track_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
    Query(q): Query<TrackQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();

    let since = q
        .since
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::hours(24));

    let limit = q.limit.unwrap_or(1000).min(5000);

    let rows = sqlx::query(
        "SELECT lat, lng, altitude, accuracy, speed, heading, metadata, recorded_at
         FROM telemetry_points
         WHERE creature_id = $1 AND recorded_at >= $2
         ORDER BY recorded_at ASC
         LIMIT $3",
    )
    .bind(creature_id)
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let points: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "lat": r.get::<f64, _>("lat"),
                "lng": r.get::<f64, _>("lng"),
                "altitude": r.try_get::<Option<f64>, _>("altitude").unwrap_or(None),
                "accuracy": r.try_get::<Option<f64>, _>("accuracy").unwrap_or(None),
                "speed": r.try_get::<Option<f64>, _>("speed").unwrap_or(None),
                "heading": r.try_get::<Option<f64>, _>("heading").unwrap_or(None),
                "metadata": r.try_get::<serde_json::Value, _>("metadata").unwrap_or(json!({})),
                "recorded_at": r.get::<chrono::DateTime<chrono::Utc>, _>("recorded_at").to_rfc3339(),
            })
        })
        .collect();

    // Get active tether info
    let tether = sqlx::query(
        "SELECT tether_id, tether_type, device_label, created_at
         FROM creature_tethers WHERE creature_id = $1 AND active = true LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let tether_info = tether.map(|t| {
        json!({
            "tether_id": t.get::<Uuid, _>("tether_id"),
            "tether_type": t.get::<String, _>("tether_type"),
            "device_label": t.try_get::<Option<String>, _>("device_label").unwrap_or(None),
            "since": t.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        })
    });

    Ok(Json(json!({
        "creature_id": creature_id,
        "points": points,
        "count": points.len(),
        "tether": tether_info,
    })))
}
