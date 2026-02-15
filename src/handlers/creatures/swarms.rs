//! Swarm (rabble) handlers — create, list, get.

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

use crate::handlers::rabble_workspace;
use crate::AppState;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

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
         walk_in_price, walk_in_budget, walk_in_budget_remaining,
         radius_meters
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
                        "radius_meters": row.try_get::<i32, _>("radius_meters").unwrap_or(100),
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
         anchor_creature_id, anchor_transferred_at, radius_meters
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
                "radius_meters": row.try_get::<i32, _>("radius_meters").unwrap_or(100),
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
    let qr_token = super::generate_qr_token();

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
