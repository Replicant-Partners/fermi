//! Creature location and rabble handlers — perch, host rabble, join swarm, favourites.

use axum::{
    extract::{Path, State},
    http::StatusCode,
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

use super::helpers::{compute_h3_cell, get_current_state, record_transition};

#[derive(Deserialize)]
pub struct PerchRequest {
    pub h3_cell: String,
    pub center_lat: f64,
    pub center_lng: f64,
    pub location_name: Option<String>,
    /// Display name for the perch (default: "{creature_name}'s perch")
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct HostRabbleRequest {
    /// Display name for the rabble (default: "{creature_name}'s rabble")
    pub name: Option<String>,
    /// NULL = private (contacts/invitees only), 0 = free open, 2+ = paid walk-in
    pub walk_in_price: Option<i32>,
    /// Credits to pre-fund for invited/contact joins (default 0)
    pub invite_pool: Option<i32>,
    /// Spending cap for free walk-ins (walk_in_price=0). Host pays per join. (default 0)
    pub walk_in_budget: Option<i32>,
    /// Operational radius in meters (default 100). Defines bounded area for flock dynamics.
    pub radius_meters: Option<i32>,
    /// Location — used when creature has no active flight (auto-perch + host)
    pub h3_cell: Option<String>,
    pub center_lat: Option<f64>,
    pub center_lng: Option<f64>,
    pub location_name: Option<String>,
}

/// POST /api/creatures/:creature_id/perch — place creature at a location (2cr)
///
/// Location-only placement. Creates a flight record but NO rabble/swarm.
/// Use POST /api/creatures/:creature_id/host to create a rabble at the perch location.
pub async fn perch_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PerchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate creature ownership
    let creature = sqlx::query(
        "SELECT c.owner_id, c.specimen_name, c.scientific_name, c.species_group,
                COALESCE(cc.visibility, 'public') AS visibility
         FROM creatures c
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
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

    // Auto-end any existing flight — creature can always change state
    let active_flight = sqlx::query(
        "SELECT flight_id, swarm_id FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = active_flight {
        let old_fid: Uuid = row.get("flight_id");
        let old_sid: Option<Uuid> = row.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();

        sqlx::query(
            "UPDATE creature_flights SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
             WHERE flight_id = $1",
        )
        .bind(old_fid)
        .execute(pool)
        .await
        .ok();

        if let Some(sid) = old_sid {
            sqlx::query(
                "UPDATE swarm_events SET creature_count = GREATEST(creature_count - 1, 0)
                 WHERE swarm_id = $1",
            )
            .bind(sid)
            .execute(pool)
            .await
            .ok();
        }
    }

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        2,
        "perch",
        &format!("Perch creature {} (2cr)", creature_id),
        Some(&creature_id.to_string()),
    )
    .await?;

    let creature_name: String = creature.try_get("specimen_name").unwrap_or_else(|_| {
        creature
            .try_get("scientific_name")
            .unwrap_or("creature".into())
    });
    let perch_name = req
        .name
        .unwrap_or_else(|| format!("{}'s perch", creature_name));

    // Compute h3_cell server-side if client sent empty string
    let h3_cell = if req.h3_cell.is_empty() {
        compute_h3_cell(req.center_lat, req.center_lng)
    } else {
        req.h3_cell.clone()
    };

    let now = chrono::Utc::now();
    let flight_id = Uuid::new_v4();
    let creature_visibility: String = creature
        .try_get("visibility")
        .unwrap_or_else(|_| "public".to_string());

    // Create flight record — no swarm_id (perch is location-only)
    sqlx::query(
        "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
         h3_cell, h3_resolution, center_lat, center_lng, location_name,
         flight_pattern, visibility, started_at, data_source)
         VALUES ($1, $2, $3, $4, 12, $5, $6, $7, 'perch', $8, $9, 'synthetic')",
    )
    .bind(flight_id)
    .bind(creature_id)
    .bind(&user_id)
    .bind(&h3_cell)
    .bind(req.center_lat)
    .bind(req.center_lng)
    .bind(&req.location_name)
    .bind(&creature_visibility)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let response = json!({
        "flight_id": flight_id,
        "creature_id": creature_id,
        "name": perch_name,
        "total_cost": 2,
    });

    // Record versioned state in background — not critical for response
    {
        let pool_bg = state.memory_store.pool().clone();
        let user_bg = user_id.clone();
        let h3_bg = h3_cell.clone();
        let lat = req.center_lat;
        let lng = req.center_lng;
        let name_bg = perch_name.clone();
        tokio::spawn(async move {
            if let Err(e) = record_transition(
                &pool_bg,
                creature_id,
                "perched",
                None,
                "perch",
                &user_bg,
                lat,
                lng,
                &h3_bg,
                None,
                None,
                &json!({
                    "flight_id": flight_id,
                    "perch_name": name_bg,
                }),
            )
            .await
            {
                eprintln!(
                    "[perch] record_transition failed for creature {}: {}",
                    creature_id, e
                );
            }
        });
    }

    // Defer non-critical stats to background
    {
        let pool_bg = state.db.clone();
        tokio::spawn(async move {
            sqlx::query(
                "UPDATE creatures SET total_flights = total_flights + 1, updated_at = NOW()
                 WHERE creature_id = $1",
            )
            .bind(creature_id)
            .execute(&pool_bg)
            .await
            .ok();
        });
    }

    // Emit activity event (fire-and-forget)
    {
        let _pool_ae = state.memory_store.pool().clone();
        let _uid_ae = user_id.clone();
        let _name_ae = perch_name.clone();
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &_pool_ae,
                &_uid_ae,
                Some(creature_id),
                "creature_perched",
                None,
                None,
                &format!("{} perched at a new location", _name_ae),
                None,
                None,
            )
            .await;
        });
    }

    // Broadcast creature SSE event
    crate::handlers::streams::emit_creature_event(
        &state,
        creature_id,
        "state_changed",
        json!({
            "state": "perched",
            "creature_id": creature_id,
            "location_name": perch_name,
        }),
    );

    Ok(Json(response))
}

/// POST /api/creatures/:creature_id/host — create a rabble at creature's location (3cr + pools)
///
/// Works from any state: perched, flying, or unplaced (with location in request).
/// If creature is flying, ends the flight and perches at current location.
/// If creature has no flight, auto-creates a perch from request location.
pub async fn host_rabble_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<HostRabbleRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate creature ownership
    let creature = sqlx::query(
        "SELECT c.owner_id, c.specimen_name, c.scientific_name, c.species_group,
                COALESCE(cc.visibility, 'public') AS visibility
         FROM creatures c
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
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

    // End any existing flight — creature moves on, no state gating
    let existing_flight = sqlx::query(
        "SELECT flight_id, swarm_id FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ef) = existing_flight {
        let old_fid: Uuid = ef.get("flight_id");
        let old_sid: Option<Uuid> = ef.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
        sqlx::query(
            "UPDATE creature_flights SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
             WHERE flight_id = $1",
        )
        .bind(old_fid)
        .execute(pool)
        .await
        .ok();
        if let Some(sid) = old_sid {
            sqlx::query(
                "UPDATE swarm_events SET creature_count = GREATEST(creature_count - 1, 0)
                 WHERE swarm_id = $1",
            )
            .bind(sid)
            .execute(pool)
            .await
            .ok();
        }
    }

    // Location from request (client always sends it)
    let center_lat = req.center_lat.unwrap_or(0.0);
    let center_lng = req.center_lng.unwrap_or(0.0);
    let location_name = req.location_name.clone();
    let h3_cell = {
        let cell = req.h3_cell.clone().unwrap_or_default();
        if cell.is_empty() {
            compute_h3_cell(center_lat, center_lng)
        } else {
            cell
        }
    };

    // Create perch flight at this location
    let flight_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO creature_flights (flight_id, creature_id, owner_id, h3_cell,
         center_lat, center_lng, location_name, flight_pattern, started_at, data_source)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'perch', NOW(), 'app')",
    )
    .bind(flight_id)
    .bind(creature_id)
    .bind(&user_id)
    .bind(&h3_cell)
    .bind(center_lat)
    .bind(center_lng)
    .bind(&location_name)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let invite_pool = req.invite_pool.unwrap_or(0).max(0);
    let walk_in_budget = req.walk_in_budget.unwrap_or(0).max(0);

    // Validate: free walk-in (price=0) requires a budget cap
    if req.walk_in_price == Some(0) && walk_in_budget == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Free walk-in requires a spending cap (walk_in_budget > 0)".to_string(),
        ));
    }

    let total_cost = state.gas_fees.host_rabble + invite_pool + walk_in_budget;

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        total_cost,
        "host_rabble",
        &format!(
            "Host rabble for creature {} ({}cr + {}cr invite + {}cr walk-in)",
            creature_id, state.gas_fees.host_rabble, invite_pool, walk_in_budget
        ),
        Some(&creature_id.to_string()),
    )
    .await?;

    let creature_name: String = creature.try_get("specimen_name").unwrap_or_else(|_| {
        creature
            .try_get("scientific_name")
            .unwrap_or("creature".into())
    });
    let rabble_name = req
        .name
        .unwrap_or_else(|| format!("{}'s rabble", creature_name));

    let visibility = if req.walk_in_price.is_none() {
        "private"
    } else {
        "public"
    };

    let swarm_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let ends_at = now + chrono::Duration::days(3650);
    let qr_token = super::generate_qr_token();

    // Create swarm_events row
    sqlx::query(
        "INSERT INTO swarm_events (swarm_id, creator_id, h3_cell, h3_resolution,
         center_lat, center_lng, location_name,
         name, starts_at, ends_at, status, created_at,
         funding_mode, invite_pool, invite_pool_remaining,
         qr_token, visibility, anchor_creature_id, walk_in_price,
         walk_in_budget, walk_in_budget_remaining,
         radius_meters, participant_count, creature_count)
         VALUES ($1, $2, $3, 12, $4, $5, $6, $7, $8, $9, 'active', $8,
                 'hosted', $10, $10, $11, $12, $13, $14,
                 $15, $15, $16, 1, 1)",
    )
    .bind(swarm_id)
    .bind(&user_id)
    .bind(&h3_cell)
    .bind(center_lat)
    .bind(center_lng)
    .bind(&location_name)
    .bind(&rabble_name)
    .bind(now)
    .bind(ends_at)
    .bind(invite_pool)
    .bind(&qr_token)
    .bind(visibility)
    .bind(creature_id)
    .bind(req.walk_in_price)
    .bind(walk_in_budget)
    .bind(req.radius_meters.unwrap_or(100).max(10).min(10000))
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Link creature's current flight to the new swarm
    sqlx::query("UPDATE creature_flights SET swarm_id = $1 WHERE flight_id = $2")
        .bind(swarm_id)
        .bind(flight_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Create workspace in background
    {
        let state_ws = state.clone();
        let user_ws = user_id.clone();
        let name_ws = rabble_name.clone();
        tokio::spawn(async move {
            match rabble_workspace::create_rabble_workspace(
                &state_ws,
                &user_ws,
                &name_ws,
                Some(swarm_id),
            )
            .await
            {
                Ok(ws_id) => {
                    eprintln!("[host] Created workspace {} for swarm {}", ws_id, swarm_id)
                }
                Err((_status, msg)) => eprintln!(
                    "[host] Workspace creation failed for swarm {}: {}",
                    swarm_id, msg
                ),
            }
        });
    }

    let response = json!({
        "swarm_id": swarm_id,
        "flight_id": flight_id,
        "creature_id": creature_id,
        "qr_token": qr_token,
        "name": rabble_name,
        "walk_in_price": req.walk_in_price,
        "invite_pool": invite_pool,
        "walk_in_budget": walk_in_budget,
        "visibility": visibility,
        "total_cost": total_cost,
    });

    // Record versioned state in background — not critical for response
    {
        let pool_bg = state.memory_store.pool().clone();
        let user_bg = user_id.clone();
        let h3_bg = h3_cell.clone();
        let name_bg = rabble_name.clone();
        let walk_in_bg = req.walk_in_price;
        tokio::spawn(async move {
            if let Err(e) = record_transition(
                &pool_bg,
                creature_id,
                "hosting",
                Some("perched"),
                "host",
                &user_bg,
                center_lat,
                center_lng,
                &h3_bg,
                Some(swarm_id),
                None,
                &json!({
                    "flight_id": flight_id,
                    "swarm_id": swarm_id,
                    "rabble_name": name_bg,
                    "walk_in_price": walk_in_bg,
                }),
            )
            .await
            {
                eprintln!(
                    "[host] record_transition failed for creature {}: {}",
                    creature_id, e
                );
            }
        });
    }

    // Record co-presence for post-rabble recap (host is first creature present)
    {
        let pool_cp = state.memory_store.pool().clone();
        let uid_cp = user_id.clone();
        tokio::spawn(async move {
            crate::handlers::social::record_co_presence(&pool_cp, swarm_id, creature_id, &uid_cp)
                .await;
        });
    }

    // Emit activity event (lightweight, fire-and-forget)
    {
        let pool_ae = state.memory_store.pool().clone();
        let uid_ae = user_id.clone();
        let rabble_name_ae: String = rabble_name.clone();
        let c_name_ae: String = creature_name.clone();
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &pool_ae,
                &uid_ae,
                Some(creature_id),
                "rabble_created",
                Some(swarm_id),
                None,
                &format!("{} created rabble {}", c_name_ae, rabble_name_ae),
                None,
                None,
            )
            .await;
        });
    }

    Ok(Json(response))
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
        "SELECT status, name, h3_cell, center_lat, center_lng, creator_id, visibility,
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

    // Verify creature ownership
    let creature = sqlx::query(
        "SELECT owner_id, specimen_name, scientific_name AS species_name, species_group
         FROM creatures WHERE creature_id = $1",
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

    // Check existing flight — tether flights are preserved, others are ended
    let active_flight = sqlx::query(
        "SELECT flight_id, swarm_id, data_source FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(req.creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut tether_preserved = false;

    if let Some(row) = active_flight {
        let old_fid: Uuid = row.get("flight_id");
        let old_sid: Option<Uuid> = row.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
        let data_source: String = row
            .try_get("data_source")
            .unwrap_or_else(|_| "synthetic".into());

        // Already in this rabble — idempotent, just return success
        if old_sid == Some(swarm_id) {
            return Ok(Json(json!({
                "message": "Already in this rabble",
                "swarm_id": swarm_id,
                "creature_id": req.creature_id,
            })));
        }

        if data_source == "device" {
            // Tether flight: preserve it, just attach to the rabble.
            // The creature stays tethered AND joins the swarm.
            sqlx::query("UPDATE creature_flights SET swarm_id = $1 WHERE flight_id = $2")
                .bind(swarm_id)
                .bind(old_fid)
                .execute(pool)
                .await
                .ok();
            tether_preserved = true;

            // Decrement old swarm if it was in a different one
            if let Some(sid) = old_sid {
                sqlx::query(
                    "UPDATE swarm_events SET creature_count = GREATEST(creature_count - 1, 0)
                     WHERE swarm_id = $1",
                )
                .bind(sid)
                .execute(pool)
                .await
                .ok();
            }
        } else {
            // Non-tether flight: end it as before
            sqlx::query(
                "UPDATE creature_flights SET ended_at = NOW(),
                 duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
                 WHERE flight_id = $1",
            )
            .bind(old_fid)
            .execute(pool)
            .await
            .ok();

            if let Some(sid) = old_sid {
                sqlx::query(
                    "UPDATE swarm_events SET creature_count = GREATEST(creature_count - 1, 0)
                     WHERE swarm_id = $1",
                )
                .bind(sid)
                .execute(pool)
                .await
                .ok();
            }
        }
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
        // Create workspace in background — don't block the join response.
        // Agent dispatches below re-query workspace_id; if not yet ready,
        // they fall back to trigger_swarm_host_welcome (fire-and-forget).
        let state_ws = state.clone();
        let creator_ws = creator_id.clone();
        let swarm_name: String = swarm
            .try_get::<String, _>("name")
            .unwrap_or_else(|_| "rabble".into());
        tokio::spawn(async move {
            match rabble_workspace::create_rabble_workspace(
                &state_ws,
                &creator_ws,
                &swarm_name,
                Some(swarm_id),
            )
            .await
            {
                Ok(ws_id) => {
                    eprintln!(
                        "[perch] First join — created workspace {} for swarm {}",
                        ws_id, swarm_id
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[perch] Failed to create workspace for swarm {}: {:?}",
                        swarm_id, e
                    );
                }
            }
        });
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

    // Record the flight at the swarm location (skip if tether was preserved)
    let h3_cell: String = swarm.get("h3_cell");
    let lat: f64 = swarm.get("center_lat");
    let lng: f64 = swarm.get("center_lng");
    let flight_id = if tether_preserved {
        // Tether flight already has swarm_id set — reuse its flight_id
        sqlx::query("SELECT flight_id FROM creature_flights WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device' LIMIT 1")
            .bind(req.creature_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<Uuid, _>("flight_id"))
            .unwrap_or_else(Uuid::new_v4)
    } else {
        let fid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
             h3_cell, h3_resolution, center_lat, center_lng,
             flight_pattern, swarm_id, started_at)
             VALUES ($1, $2, $3, $4, 12, $5, $6, 'swarm', $7, NOW())",
        )
        .bind(fid)
        .bind(req.creature_id)
        .bind(&user_id)
        .bind(&h3_cell)
        .bind(lat)
        .bind(lng)
        .bind(swarm_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        fid
    };

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

    // Defer versioned state + system messages + agent welcomes to background
    {
        let state_bg = state.clone();
        let uid_bg = user_id.clone();
        let cid_bg = req.creature_id;
        let h3_bg = h3_cell.clone();
        let c_name_bg = creature_name.clone();
        let s_name_bg = species_name.clone();
        let s_group_bg = species_group.clone();
        tokio::spawn(async move {
            let pool_bg = &state_bg.db;

            // Record join transition (versioned model)
            let prev = get_current_state(pool_bg, cid_bg).await;
            let prev_state = prev.as_ref().map(|(s, _)| s.as_str());
            let _ = record_transition(
                pool_bg, cid_bg, "in_rabble", prev_state, "join", &uid_bg,
                lat, lng, &h3_bg, Some(swarm_id), existing_ws,
                &json!({ "flight_id": flight_id, "swarm_id": swarm_id, "first_join": is_first_join }),
            )
            .await;

            // System messages
            let display_name = c_name_bg.as_deref().unwrap_or("A creature");
            let species_display = s_name_bg.as_deref().unwrap_or("unknown species");

            if is_first_join {
                let _ = crate::handlers::rabble_chat::insert_system_message(
                    &state_bg,
                    swarm_id,
                    "We have a rabble!!",
                )
                .await;
            }
            let _ = crate::handlers::rabble_chat::insert_system_message(
                &state_bg,
                swarm_id,
                &format!(
                    "{} ({}) has joined the rabble!",
                    display_name, species_display
                ),
            )
            .await;

            // Route through workspace agents
            let swarm_ws_id: Option<Uuid> =
                sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
                    .bind(swarm_id)
                    .fetch_optional(pool_bg)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten());

            if let Some(ws_id) = swarm_ws_id {
                let c_name = c_name_bg.unwrap_or_else(|| "creature".to_string());
                let s_name = s_name_bg.unwrap_or_else(|| "unknown".to_string());
                let s_group = s_group_bg.unwrap_or_else(|| "unknown".to_string());

                // Dispatch swarm_host welcome
                let query = format!(
                    "Welcome {} ({}, {}) to the rabble! Share a fun taxonomic fact.",
                    c_name, s_name, s_group
                );
                if let Ok(welcome) = rabble_workspace::dispatch_rabble_action(
                    &state_bg,
                    ws_id,
                    "swarm_host",
                    "swarm_join",
                    &query,
                    &uid_bg,
                )
                .await
                {
                    let _ = sqlx::query(
                        "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, creature_id, creature_name, content, message_type)
                         VALUES ($1, $2, 'system', NULL, 'Swarm Host', $3, 'narrator')"
                    )
                    .bind(Uuid::new_v4())
                    .bind(swarm_id)
                    .bind(&welcome)
                    .execute(pool_bg)
                    .await;
                }

                // Lifecycle coordinator
                let query2 = format!(
                    "participant_joined: {} ({}) has joined the rabble.",
                    c_name, s_name
                );
                let _ = rabble_workspace::dispatch_rabble_action(
                    &state_bg,
                    ws_id,
                    "rabble_lifecycle_coordinator",
                    "participant_joined",
                    &query2,
                    &uid_bg,
                )
                .await;
            } else {
                // Legacy fallback
                super::trigger_swarm_host_welcome(
                    &state_bg,
                    swarm_id,
                    c_name_bg.as_deref().unwrap_or("creature"),
                    s_name_bg.as_deref().unwrap_or("unknown"),
                    s_group_bg.as_deref().unwrap_or("unknown"),
                )
                .await;
            }
        });
    }

    // Record co-presence for post-rabble recap ("You met these creatures")
    {
        let pool_cp = state.memory_store.pool().clone();
        let uid_cp = user_id.clone();
        let cid_cp = req.creature_id;
        tokio::spawn(async move {
            crate::handlers::social::record_co_presence(&pool_cp, swarm_id, cid_cp, &uid_cp).await;
        });
    }

    // Emit activity event (lightweight, fire-and-forget)
    {
        let pool_ae = state.memory_store.pool().clone();
        let uid_ae = user_id.clone();
        let cid_ae = req.creature_id;
        let swarm_name: String = swarm.try_get("name").unwrap_or_else(|_| "Rabble".into());
        let c_name_ae = creature_name.clone().unwrap_or_else(|| "Creature".into());
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &pool_ae,
                &uid_ae,
                Some(cid_ae),
                "rabble_joined",
                Some(swarm_id),
                None,
                &format!("{} joined {}", c_name_ae, swarm_name),
                None,
                None,
            )
            .await;
        });
    }

    // Broadcast creature SSE event
    crate::handlers::streams::emit_creature_event(
        &state,
        req.creature_id,
        "entered_rabble",
        json!({
            "swarm_id": swarm_id,
            "flight_id": flight_id,
            "funding_mode": funding_mode,
            "first_join": is_first_join,
        }),
    );

    Ok(Json(json!({
        "swarm_id": swarm_id,
        "flight_id": flight_id,
        "creature_id": req.creature_id,
        "joined": true,
        "funding_mode": funding_mode,
        "first_join": is_first_join,
        "tether_preserved": tether_preserved,
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

// ═══════════════════════════════════════════════════════════════════
// Creature favourites (star/follow)
// ═══════════════════════════════════════════════════════════════════

/// POST /api/creatures/:creature_id/favourite — star a creature
pub async fn favourite_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creature exists
    sqlx::query("SELECT 1 FROM creatures WHERE creature_id = $1")
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    sqlx::query(
        "INSERT INTO creature_favourites (user_id, creature_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "creature_id": creature_id, "starred": true })))
}

/// DELETE /api/creatures/:creature_id/favourite — unstar a creature
pub async fn unfavourite_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    sqlx::query("DELETE FROM creature_favourites WHERE user_id = $1 AND creature_id = $2")
        .bind(&user_id)
        .bind(creature_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(
        json!({ "creature_id": creature_id, "starred": false }),
    ))
}

