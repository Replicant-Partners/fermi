//! Swarm (rabble) handlers — create, list, get, my-rabbles.

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

use super::helpers::compute_h3_cell;

// ─── My Rabbles (hosted vs participating, with creature placement) ──

/// GET /api/my/rabbles — lists all rabbles the caller hosts or participates in,
/// split into `hosting` and `participating` sections. Each entry includes which
/// of the user's creatures are currently in that rabble.
///
/// Addresses UX requirements:
///   - Distinction of rabbles I host vs rabbles I'm a member of
///   - Understanding which creatures I have in which rabbles
///   - Latest activity ordering
pub async fn my_rabbles_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // 1. Rabbles I host (I am the creator)
    let hosted_rows = sqlx::query(
        "SELECT s.swarm_id, s.name, s.description, s.status, s.location_name,
                s.center_lat, s.center_lng, s.h3_cell,
                s.creature_count, s.participant_count, s.visibility,
                s.funding_mode, s.walk_in_price, s.walk_in_budget_remaining,
                s.invite_pool_remaining, s.radius_meters,
                s.starts_at, s.ends_at, s.created_at,
                s.anchor_creature_id,
                ac.specimen_name AS anchor_creature_name,
                ac.asset_path AS anchor_creature_image,
                -- Latest activity timestamp for ordering
                GREATEST(s.created_at, COALESCE(
                    (SELECT MAX(ae.created_at) FROM activity_events ae WHERE ae.rabble_id = s.swarm_id),
                    s.created_at
                )) AS last_activity_at
         FROM swarm_events s
         LEFT JOIN creatures ac ON ac.creature_id = s.anchor_creature_id
         WHERE s.creator_id = $1
           AND s.status IN ('scheduled', 'active', 'completed')
         ORDER BY last_activity_at DESC",
    )
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let hosted_ids: Vec<Uuid> = hosted_rows
        .iter()
        .map(|r| r.get::<Uuid, _>("swarm_id"))
        .collect();

    // 2. Rabbles I participate in (my creature is flying there, but I'm NOT the creator)
    let participating_rows = sqlx::query(
        "SELECT DISTINCT ON (s.swarm_id)
                s.swarm_id, s.name, s.description, s.status, s.location_name,
                s.center_lat, s.center_lng, s.h3_cell,
                s.creature_count, s.participant_count, s.visibility,
                s.creator_id, s.radius_meters,
                s.starts_at, s.ends_at, s.created_at,
                s.anchor_creature_id,
                ac.specimen_name AS anchor_creature_name,
                ac.asset_path AS anchor_creature_image,
                u_creator.display_name AS host_display_name,
                GREATEST(s.created_at, COALESCE(
                    (SELECT MAX(ae.created_at) FROM activity_events ae WHERE ae.rabble_id = s.swarm_id),
                    s.created_at
                )) AS last_activity_at
         FROM creature_flights cf
         JOIN creatures c ON c.creature_id = cf.creature_id AND c.owner_id = $1
         JOIN swarm_events s ON s.swarm_id = cf.swarm_id
         LEFT JOIN creatures ac ON ac.creature_id = s.anchor_creature_id
         LEFT JOIN users u_creator ON u_creator.user_id = s.creator_id
         WHERE cf.ended_at IS NULL
           AND s.creator_id != $1
           AND s.status IN ('scheduled', 'active')
         ORDER BY s.swarm_id, cf.started_at DESC",
    )
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 3. For all rabbles (hosted + participating), find which of MY creatures are in each
    let all_swarm_ids: Vec<Uuid> = hosted_ids
        .iter()
        .copied()
        .chain(
            participating_rows
                .iter()
                .map(|r| r.get::<Uuid, _>("swarm_id")),
        )
        .collect();

    let mut my_creatures_in_rabbles: std::collections::HashMap<Uuid, Vec<serde_json::Value>> =
        std::collections::HashMap::new();

    if !all_swarm_ids.is_empty() {
        let placeholders: Vec<String> = (1..=all_swarm_ids.len())
            .map(|i| format!("${}", i + 1))
            .collect();
        let creature_sql = format!(
            "SELECT cf.swarm_id, c.creature_id, c.specimen_name, c.species_group,
                    c.asset_path, cf.data_source,
                    cs.state AS creature_state,
                    -- One creature = one host. Anchor creature IS the host; user is proxy.
                    (CASE WHEN sw.anchor_creature_id = c.creature_id THEN true ELSE false END) AS is_anchor
             FROM creature_flights cf
             JOIN creatures c ON c.creature_id = cf.creature_id
             LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
             LEFT JOIN swarm_events sw ON sw.swarm_id = cf.swarm_id
             WHERE c.owner_id = $1
               AND cf.swarm_id IN ({})
               AND cf.ended_at IS NULL
             ORDER BY cf.started_at DESC",
            placeholders.join(", ")
        );
        let mut q = sqlx::query(&creature_sql).bind(&user_id);
        for sid in &all_swarm_ids {
            q = q.bind(sid);
        }
        if let Ok(rows) = q.fetch_all(pool).await {
            for r in &rows {
                let sid: Uuid = r.get("swarm_id");
                let entry = my_creatures_in_rabbles.entry(sid).or_default();
                let is_anchor = r.try_get::<bool, _>("is_anchor").unwrap_or(false);
                entry.push(json!({
                    "creature_id": r.get::<Uuid, _>("creature_id"),
                    "specimen_name": r.try_get::<Option<String>, _>("specimen_name").unwrap_or(None),
                    "species_group": r.try_get::<Option<String>, _>("species_group").unwrap_or(None),
                    "asset_path": r.try_get::<Option<String>, _>("asset_path").unwrap_or(None),
                    "data_source": r.try_get::<String, _>("data_source").unwrap_or_else(|_| "synthetic".into()),
                    "creature_state": r.try_get::<Option<String>, _>("creature_state").unwrap_or(None),
                    "is_anchor": is_anchor,
                    "role": if is_anchor { "host" } else { "participant" },
                }));
            }
        }
    }

    // 4. Build hosted response
    let hosting: Vec<serde_json::Value> = hosted_rows
        .iter()
        .map(|row| {
            let sid = row.get::<Uuid, _>("swarm_id");
            json!({
                "swarm_id": sid,
                "name": row.get::<String, _>("name"),
                "description": row.try_get::<Option<String>, _>("description").unwrap_or(None),
                "status": row.get::<String, _>("status"),
                "location_name": row.try_get::<Option<String>, _>("location_name").unwrap_or(None),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "creature_count": row.get::<i32, _>("creature_count"),
                "participant_count": row.get::<i32, _>("participant_count"),
                "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".into()),
                "funding_mode": row.try_get::<String, _>("funding_mode").unwrap_or_else(|_| "hosted".into()),
                "walk_in_price": row.try_get::<Option<i32>, _>("walk_in_price").unwrap_or(None),
                "walk_in_budget_remaining": row.try_get::<Option<i32>, _>("walk_in_budget_remaining").unwrap_or(None),
                "invite_pool_remaining": row.try_get::<Option<i32>, _>("invite_pool_remaining").unwrap_or(None),
                "radius_meters": row.try_get::<i32, _>("radius_meters").unwrap_or(100),
                "anchor_creature_id": row.try_get::<Option<Uuid>, _>("anchor_creature_id").ok().flatten(),
                "anchor_creature_name": row.try_get::<Option<String>, _>("anchor_creature_name").unwrap_or(None),
                "anchor_creature_image": row.try_get::<Option<String>, _>("anchor_creature_image").unwrap_or(None),
                "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "last_activity_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("last_activity_at")
                    .map(|t| t.to_rfc3339()).ok(),
                "my_creatures": my_creatures_in_rabbles.get(&sid).cloned().unwrap_or_default(),
                "role": "host",
            })
        })
        .collect();

    // 5. Build participating response
    let participating: Vec<serde_json::Value> = participating_rows
        .iter()
        .map(|row| {
            let sid = row.get::<Uuid, _>("swarm_id");
            json!({
                "swarm_id": sid,
                "name": row.get::<String, _>("name"),
                "description": row.try_get::<Option<String>, _>("description").unwrap_or(None),
                "status": row.get::<String, _>("status"),
                "location_name": row.try_get::<Option<String>, _>("location_name").unwrap_or(None),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "creature_count": row.get::<i32, _>("creature_count"),
                "participant_count": row.get::<i32, _>("participant_count"),
                "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".into()),
                "host_id": row.get::<String, _>("creator_id"),
                "host_display_name": row.try_get::<Option<String>, _>("host_display_name").unwrap_or(None),
                "radius_meters": row.try_get::<i32, _>("radius_meters").unwrap_or(100),
                "anchor_creature_id": row.try_get::<Option<Uuid>, _>("anchor_creature_id").ok().flatten(),
                "anchor_creature_name": row.try_get::<Option<String>, _>("anchor_creature_name").unwrap_or(None),
                "anchor_creature_image": row.try_get::<Option<String>, _>("anchor_creature_image").unwrap_or(None),
                "starts_at": row.get::<chrono::DateTime<chrono::Utc>, _>("starts_at").to_rfc3339(),
                "ends_at": row.get::<chrono::DateTime<chrono::Utc>, _>("ends_at").to_rfc3339(),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "last_activity_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("last_activity_at")
                    .map(|t| t.to_rfc3339()).ok(),
                "my_creatures": my_creatures_in_rabbles.get(&sid).cloned().unwrap_or_default(),
                "role": "participant",
            })
        })
        .collect();

    Ok(Json(json!({
        "hosting": hosting,
        "hosting_count": hosting.len(),
        "participating": participating,
        "participating_count": participating.len(),
        "total": hosting.len() + participating.len(),
    })))
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
    let limit = q.limit.unwrap_or(20).min(200);
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

    // Show rabbles even with 0 creatures — they may be newly created or between joins
    // sql.push_str(" AND creature_count > 0"); // Removed: was hiding valid rabbles from search

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

// ─── Update swarm (rabble editing) ─────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateSwarmRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub walk_in_price: Option<i32>,
    pub visibility: Option<String>,
    pub radius_meters: Option<i32>,
    pub center_lat: Option<f64>,
    pub center_lng: Option<f64>,
    pub location_name: Option<String>,
}

/// PATCH /api/swarms/:swarm_id — update swarm settings (creator only, no gas)
pub async fn update_swarm_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<Uuid>,
    Json(req): Json<UpdateSwarmRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creator
    let creator: String = sqlx::query("SELECT creator_id FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Swarm not found".to_string()))?
        .get("creator_id");

    if creator != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the creator can edit this rabble".to_string(),
        ));
    }

    // Validate visibility if provided
    if let Some(ref vis) = req.visibility {
        if vis != "public" && vis != "shared" && vis != "private" {
            return Err((
                StatusCode::BAD_REQUEST,
                "visibility must be 'public', 'shared', or 'private'".into(),
            ));
        }
    }

    // Build dynamic UPDATE
    let mut set_clauses: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    let mut int_binds: Vec<(usize, i32)> = Vec::new();
    let mut bind_idx = 1u32; // $1 = swarm_id

    if let Some(ref name) = req.name {
        bind_idx += 1;
        set_clauses.push(format!("name = ${bind_idx}"));
        binds.push(name.clone());
    }
    if let Some(ref desc) = req.description {
        bind_idx += 1;
        set_clauses.push(format!("description = ${bind_idx}"));
        binds.push(desc.clone());
    }
    if let Some(ref vis) = req.visibility {
        bind_idx += 1;
        set_clauses.push(format!("visibility = ${bind_idx}"));
        binds.push(vis.clone());
    }
    if let Some(price) = req.walk_in_price {
        bind_idx += 1;
        set_clauses.push(format!("walk_in_price = ${bind_idx}"));
        int_binds.push((bind_idx as usize, price));
    }
    if let Some(radius) = req.radius_meters {
        bind_idx += 1;
        set_clauses.push(format!("radius_meters = ${bind_idx}"));
        int_binds.push((bind_idx as usize, radius));
    }

    // Location move fields
    let is_move = req.center_lat.is_some() && req.center_lng.is_some();
    let h3_cell = if is_move {
        let lat = req.center_lat.unwrap();
        let lng = req.center_lng.unwrap();
        let h3 = compute_h3_cell(lat, lng);
        bind_idx += 1;
        set_clauses.push(format!("center_lat = ${bind_idx}"));
        bind_idx += 1;
        set_clauses.push(format!("center_lng = ${bind_idx}"));
        bind_idx += 1;
        set_clauses.push(format!("h3_cell = ${bind_idx}"));
        if req.location_name.is_some() {
            bind_idx += 1;
            set_clauses.push(format!("location_name = ${bind_idx}"));
        }
        Some(h3)
    } else {
        None
    };

    if set_clauses.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    let update_sql = format!(
        "UPDATE swarm_events SET {} WHERE swarm_id = $1 \
         RETURNING swarm_id, name, description, visibility, walk_in_price, radius_meters, \
                  center_lat, center_lng, h3_cell, location_name",
        set_clauses.join(", ")
    );

    // We need a single approach for mixed types — use serde_json::Value as intermediary
    // Build with raw query and manual bind
    let mut query = sqlx::query(&update_sql).bind(swarm_id);

    // Track which bind indices are ints vs strings
    let mut current_idx = 2u32;
    if req.name.is_some() {
        query = query.bind(req.name.as_ref().unwrap().as_str());
        current_idx += 1;
    }
    if req.description.is_some() {
        query = query.bind(req.description.as_ref().unwrap().as_str());
        current_idx += 1;
    }
    if req.visibility.is_some() {
        query = query.bind(req.visibility.as_ref().unwrap().as_str());
        current_idx += 1;
    }
    if let Some(price) = req.walk_in_price {
        query = query.bind(price);
        current_idx += 1;
    }
    if let Some(radius) = req.radius_meters {
        query = query.bind(radius);
        current_idx += 1;
    }
    if is_move {
        query = query.bind(req.center_lat.unwrap());
        current_idx += 1;
        query = query.bind(req.center_lng.unwrap());
        current_idx += 1;
        query = query.bind(h3_cell.as_deref().unwrap_or(""));
        current_idx += 1;
        if let Some(ref loc) = req.location_name {
            query = query.bind(loc.as_str());
            current_idx += 1;
        }
    }
    let _ = current_idx; // suppress unused warning

    let row = query
        .fetch_one(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let returned_lat: f64 = row.try_get("center_lat").unwrap_or(0.0);
    let returned_lng: f64 = row.try_get("center_lng").unwrap_or(0.0);
    let returned_loc: Option<String> = row.try_get("location_name").ok();

    // Broadcast rabble_moved event if location changed
    if is_move {
        let _ = state.rabble_broadcast.send(crate::RabbleEvent {
            swarm_id,
            message: json!({
                "type": "rabble_moved",
                "swarm_id": swarm_id,
                "center_lat": returned_lat,
                "center_lng": returned_lng,
                "location_name": returned_loc,
                "h3_cell": row.try_get::<String, _>("h3_cell").unwrap_or_default(),
            }),
        });

        // Notify followers about the move
        let pool_bg = state.memory_store.pool().clone();
        let swarm_name: String = row.try_get("name").unwrap_or_else(|_| "Rabble".into());
        let loc_display = returned_loc
            .clone()
            .unwrap_or_else(|| format!("{:.4}, {:.4}", returned_lat, returned_lng));
        let uid = user_id.clone();
        tokio::spawn(async move {
            crate::handlers::social::notify_rabble_followers(
                &pool_bg,
                swarm_id,
                "start", // reuse "start" notification type for moves
                &format!("{} moved to {}", swarm_name, loc_display),
                Some("The rabble has a new location"),
                Some(uid.as_str()),
            )
            .await;
        });
    }

    Ok(Json(json!({
        "swarm_id": row.get::<Uuid, _>("swarm_id"),
        "name": row.get::<String, _>("name"),
        "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
        "visibility": row.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".into()),
        "walk_in_price": row.try_get::<Option<i32>, _>("walk_in_price").ok().flatten(),
        "radius_meters": row.try_get::<i32, _>("radius_meters").unwrap_or(100),
        "center_lat": returned_lat,
        "center_lng": returned_lng,
        "location_name": returned_loc,
        "moved": is_move,
    })))
}

// ─── End Rabble ────────────────────────────────────────────────────
//
// POST /api/rabble/:id/end
//
// Two triggers:
//   1. Host explicitly closes the rabble (this endpoint).
//   2. Timed rabble — `ends_at` countdown expires (handled by a scheduled
//      worker or client-side timer that calls this same endpoint).
//
// What happens:
//   - Status → "completed"
//   - All creature flights attached to this swarm are ended
//   - Creature counts decremented
//   - Followers notified
//   - System message posted in chat

pub async fn end_rabble_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify rabble exists and caller owns the anchor creature (host-by-proxy).
    // One creature = one host. The user manages the rabble through their anchor creature.
    let swarm = sqlx::query(
        "SELECT s.creator_id, s.status, s.name, s.anchor_creature_id,
                c.owner_id AS anchor_owner_id
         FROM swarm_events s
         LEFT JOIN creatures c ON c.creature_id = s.anchor_creature_id
         WHERE s.swarm_id = $1",
    )
    .bind(swarm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Rabble not found".to_string()))?;

    let creator_id: String = swarm.get("creator_id");
    let status: String = swarm.get("status");
    let swarm_name: String = swarm.try_get("name").unwrap_or_else(|_| "Rabble".into());
    let anchor_owner: Option<String> = swarm
        .try_get::<Option<String>, _>("anchor_owner_id")
        .unwrap_or(None);

    // Auth: user must own the anchor creature OR be the original creator (fallback
    // for edge cases where anchor_creature_id is NULL or creature was transferred).
    let is_host = anchor_owner.as_deref() == Some(&user_id) || creator_id == user_id;
    if !is_host {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the host creature's owner can end this rabble.".into(),
        ));
    }

    if status == "completed" || status == "cancelled" {
        return Ok(Json(json!({
            "message": "Rabble already ended",
            "swarm_id": swarm_id,
            "status": status,
        })));
    }

    // End all active creature flights in this rabble
    let ended_flights = sqlx::query(
        "UPDATE creature_flights
         SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
         WHERE swarm_id = $1 AND ended_at IS NULL
         RETURNING creature_id",
    )
    .bind(swarm_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let creatures_removed = ended_flights.len();

    // Clear creature_state for all creatures that were in this rabble
    // Sets them back to 'idle' with no rabble association.
    sqlx::query(
        "UPDATE creature_state SET state = 'idle', rabble_id = NULL, updated_at = NOW()
         WHERE rabble_id = $1",
    )
    .bind(swarm_id)
    .execute(pool)
    .await
    .ok();

    // Also emit SSE events so creature cards update in real-time
    for row in &ended_flights {
        let cid: Uuid = row.get("creature_id");
        crate::handlers::streams::emit_creature_event(
            &state,
            cid,
            "left_rabble",
            json!({
                "swarm_id": swarm_id,
                "creature_id": cid,
                "state": "idle",
                "reason": "rabble_ended",
            }),
        );
    }

    // Mark rabble as completed
    sqlx::query(
        "UPDATE swarm_events
         SET status = 'completed', creature_count = 0, participant_count = 0
         WHERE swarm_id = $1",
    )
    .bind(swarm_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Post system message in chat
    let msg_id = Uuid::new_v4();
    let _ = sqlx::query(
        "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, content, message_type, created_at)
         VALUES ($1, $2, 'system', $3, 'system', NOW())",
    )
    .bind(msg_id)
    .bind(swarm_id)
    .bind(format!("The host has ended this rabble. {} creature{} released.",
        creatures_removed, if creatures_removed == 1 { "" } else { "s" }))
    .execute(pool)
    .await;

    // Broadcast end event to connected clients
    let _ = state.rabble_broadcast.send(crate::RabbleEvent {
        swarm_id,
        message: json!({
            "type": "rabble_ended",
            "swarm_id": swarm_id,
            "ended_by": user_id,
            "creatures_removed": creatures_removed,
        }),
    });

    // Notify followers in background
    let pool_bg = pool.clone();
    let name_clone = swarm_name.clone();
    let uid = user_id.clone();
    tokio::spawn(async move {
        crate::handlers::social::notify_rabble_followers(
            &pool_bg,
            swarm_id,
            "end",
            &format!("{} has ended", name_clone),
            Some("The host closed this rabble"),
            Some(uid.as_str()),
        )
        .await;
    });

    Ok(Json(json!({
        "message": "Rabble ended",
        "swarm_id": swarm_id,
        "swarm_name": swarm_name,
        "creatures_removed": creatures_removed,
        "status": "completed",
    })))
}

// ─── Leave Rabble ──────────────────────────────────────────────────
//
// POST /api/rabble/:id/leave
//
// Explicit leave: removes a creature from a rabble without ending it.
// If the creature is the anchor → reject with guidance to end/transfer.
//
// What happens:
//   - Clears swarm_id on the creature's active flight
//   - Decrements creature_count on the swarm
//   - Updates creature_state to "perched" (still at location, just not in the gathering)
//   - Posts system message in chat ("[creature] has left")

#[derive(Deserialize)]
pub struct LeaveRabbleRequest {
    pub creature_id: Uuid,
}

pub async fn leave_rabble_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<Uuid>,
    Json(req): Json<LeaveRabbleRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creature ownership
    let creature =
        super::helpers::verify_creature_ownership(pool, req.creature_id, &user_id).await?;
    let creature_name: String = creature
        .try_get("specimen_name")
        .unwrap_or_else(|_| "A creature".into());

    // Check if creature is actually in this rabble
    let flight = sqlx::query(
        "SELECT flight_id, data_source FROM creature_flights
         WHERE creature_id = $1 AND swarm_id = $2 AND ended_at IS NULL
         LIMIT 1",
    )
    .bind(req.creature_id)
    .bind(swarm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::NOT_FOUND,
        "This creature is not in this rabble".into(),
    ))?;

    let flight_id: Uuid = flight.get("flight_id");

    // Block if creature is the anchor — must end or transfer rabble instead
    let is_anchor = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM swarm_events
            WHERE swarm_id = $1 AND anchor_creature_id = $2
            AND status IN ('active', 'scheduled')
        )",
    )
    .bind(swarm_id)
    .bind(req.creature_id)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_anchor {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "{} is the anchor of this rabble. End the rabble or transfer the anchor first.",
                creature_name
            ),
        ));
    }

    // Clear swarm_id on the flight — creature stays at its location but leaves the gathering
    sqlx::query("UPDATE creature_flights SET swarm_id = NULL WHERE flight_id = $1")
        .bind(flight_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Decrement creature count
    sqlx::query(
        "UPDATE swarm_events SET creature_count = GREATEST(creature_count - 1, 0)
         WHERE swarm_id = $1",
    )
    .bind(swarm_id)
    .execute(pool)
    .await
    .ok();

    // Update creature_state to "perched" — still at location, just not in the rabble
    sqlx::query(
        "UPDATE creature_state SET state = 'perched', rabble_id = NULL, updated_at = NOW()
         WHERE creature_id = $1",
    )
    .bind(req.creature_id)
    .execute(pool)
    .await
    .ok();

    // Post system message in chat
    let msg_id = Uuid::new_v4();
    let _ = sqlx::query(
        "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, content, message_type, created_at)
         VALUES ($1, $2, 'system', $3, 'system', NOW())",
    )
    .bind(msg_id)
    .bind(swarm_id)
    .bind(format!("{} has left the rabble", creature_name))
    .execute(pool)
    .await;

    // Broadcast leave event
    let _ = state.rabble_broadcast.send(crate::RabbleEvent {
        swarm_id,
        message: json!({
            "type": "creature_left",
            "swarm_id": swarm_id,
            "creature_id": req.creature_id,
            "creature_name": creature_name,
        }),
    });

    // Emit creature SSE event so the creature card updates
    crate::handlers::streams::emit_creature_event(
        &state,
        req.creature_id,
        "left_rabble",
        json!({
            "swarm_id": swarm_id,
            "creature_id": req.creature_id,
            "state": "perched",
        }),
    );

    Ok(Json(json!({
        "message": format!("{} left the rabble", creature_name),
        "swarm_id": swarm_id,
        "creature_id": req.creature_id,
        "state": "perched",
    })))
}
