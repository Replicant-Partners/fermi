//! Flight lifecycle handlers — record, end, plan, fly, export, import, append telemetry.

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

use super::helpers::{
    compute_h3_cell, get_current_state, record_transition, verify_creature_ownership,
};

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
    let creature = verify_creature_ownership(pool, req.creature_id, &user_id).await?;

    let creature_visibility: String = creature
        .try_get("visibility")
        .unwrap_or_else(|_| "public".to_string());

    // Auto-end any existing flight — creature can always change state
    let active_flight = sqlx::query(
        "SELECT flight_id, location_name, swarm_id FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(req.creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Auto-end any existing flight — creature can always change state
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

        // Decrement swarm creature count if leaving a rabble
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

    // Compute h3_cell server-side if client sent empty string
    let h3_cell = if req.h3_cell.is_empty() {
        compute_h3_cell(req.center_lat, req.center_lng)
    } else {
        req.h3_cell.clone()
    };

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
    .bind(&h3_cell)
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

    // Record state transition to "fly" so creature_state is populated
    // This is critical for scan_nearby_creatures and prey_locator to find creature location
    {
        let pool_bg = state.db.clone();
        let uid_bg = user_id.clone();
        let cid = req.creature_id;
        let lat = req.center_lat;
        let lng = req.center_lng;
        let h3 = h3_cell.clone();
        let sid = req.swarm_id;
        let fid = flight_id;
        tokio::spawn(async move {
            let new_state = if sid.is_some() { "in_rabble" } else { "fly" };
            let _ = record_transition(
                &pool_bg,
                cid,
                new_state,
                Some("perched"),
                "launch",
                &uid_bg,
                lat,
                lng,
                &h3,
                sid,
                None,
                &json!({ "flight_id": fid }),
            )
            .await;
        });
    }

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

    // Fire enemy sensor check if module is active (fire-and-forget, 1cr)
    {
        let modules: Vec<String> =
            sqlx::query("SELECT active_modules FROM creature_conditions WHERE creature_id = $1")
                .bind(req.creature_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .and_then(|r| {
                    r.try_get::<Option<Vec<String>>, _>("active_modules")
                        .ok()
                        .flatten()
                })
                .unwrap_or_default();

        if modules.contains(&"enemy_sensor".to_string()) {
            if let Some(ws_id) = dispatch_ws_id {
                let state3 = state.clone();
                let user_id3 = user_id.clone();
                let cid = req.creature_id;
                let gas = state.gas_fees.clone();
                tokio::spawn(async move {
                    // Charge 1cr check fee
                    if let Ok(wallet) = get_or_create_wallet(&state3.db, "user", &user_id3).await {
                        if charge_gas(
                            &state3.db,
                            wallet.wallet_id,
                            gas.enemy_sensor_check,
                            "enemy_sensor_check",
                            &format!("Auto enemy sensor scan for creature {}", cid),
                            Some(&cid.to_string()),
                        )
                        .await
                        .is_ok()
                        {
                            let query = format!(
                                "Auto-scan for natural enemies near creature {}. \
                                 Use scan_nearby_creatures with creature_id \"{}\".",
                                cid, cid
                            );
                            match rabble_workspace::dispatch_rabble_action(
                                &state3,
                                ws_id,
                                "enemy_sensor",
                                "threat_scan",
                                &query,
                                &user_id3,
                            )
                            .await
                            {
                                Ok(_) => {
                                    eprintln!("[rabble] Enemy sensor scanned creature {}", cid)
                                }
                                Err(e) => eprintln!("[rabble] Enemy sensor dispatch failed: {}", e),
                            }
                        }
                    }
                });
            }
        }
    }

    // Emit activity event (fire-and-forget)
    {
        let _pool_ae = state.memory_store.pool().clone();
        let _uid_ae = user_id.clone();
        let _cid_ae = req.creature_id;
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &_pool_ae,
                &_uid_ae,
                Some(_cid_ae),
                "creature_flew",
                None,
                None,
                &format!("Creature took flight"),
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
        "flight_started",
        json!({
            "flight_id": flight_id,
            "h3_cell": h3_cell,
            "location_name": req.location_name,
            "started_at": now.to_rfc3339(),
        }),
    );

    Ok(Json(json!({
        "flight_id": flight_id,
        "creature_id": req.creature_id,
        "h3_cell": h3_cell,
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

    // Fetch creature_id for this flight (needed for SSE broadcast + background work)
    let creature_id: Uuid =
        sqlx::query_scalar("SELECT creature_id FROM creature_flights WHERE flight_id = $1")
            .bind(flight_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to resolve creature: {}", e),
                )
            })?;

    // Defer versioned state recording to background
    {
        let pool_bg = state.db.clone();
        let uid_bg = principal.user_id();
        let dur = req.duration_seconds;
        tokio::spawn(async move {
            let flight_info = sqlx::query(
                "SELECT creature_id, center_lat, center_lng, h3_cell, swarm_id
                 FROM creature_flights WHERE flight_id = $1",
            )
            .bind(flight_id)
            .fetch_optional(&pool_bg)
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
                    "in_rabble"
                } else {
                    "perched"
                };
                let transition = if sid.is_some() { "join" } else { "land" };

                let _ = record_transition(
                    &pool_bg,
                    cid,
                    new_state,
                    Some("fly"),
                    transition,
                    &uid_bg,
                    lat,
                    lng,
                    &h3,
                    sid,
                    None,
                    &json!({ "flight_id": flight_id, "duration_seconds": dur }),
                )
                .await;
            }
        });
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
                        "SELECT f.creature_id, f.swarm_id, c.species_group, c.scientific_name,
                                COALESCE(cc.sosa_opt_in, false) AS sosa_opt_in
                         FROM creature_flights f
                         JOIN creatures c ON f.creature_id = c.creature_id
                         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
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

    // Emit activity event (fire-and-forget)
    {
        let _pool_ae = state.memory_store.pool().clone();
        let _uid_ae = principal.user_id();
        let _fid_ae = flight_id;
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &_pool_ae,
                &_uid_ae,
                None,
                "creature_landed",
                None,
                None,
                "Creature landed",
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
        "flight_ended",
        json!({
            "flight_id": flight_id,
            "ended_at": now.to_rfc3339(),
            "duration_seconds": req.duration_seconds,
        }),
    );

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
#[allow(dead_code)]
pub async fn plan_flight_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<PlanFlightRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify creature ownership
    let creature = verify_creature_ownership(pool, req.creature_id, &user_id).await?;

    let species: String = creature.get("species_group");
    let specimen_name: Option<String> = creature.try_get("specimen_name").unwrap_or(None);
    let specimen_name_for_activity = specimen_name.clone();
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

    let creature_label = specimen_name
        .clone()
        .unwrap_or_else(|| scientific_name.unwrap_or_else(|| species.clone()));

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

    // Fire-and-forget: spawn flight plan agent dispatch, return immediately
    let spawn_state = state.clone();
    let spawn_user = user_id.clone();
    let creature_id = req.creature_id;
    let species_clone = species.clone();
    let origin_clone = req.origin.clone();
    let dest_clone = req.destination.clone();
    tokio::spawn(async move {
        let pool_bg = spawn_state.memory_store.pool();
        match rabble_workspace::dispatch_rabble_action(
            &spawn_state,
            ws_id,
            "flight_coordinator",
            "flight_plan",
            &query,
            &spawn_user,
        )
        .await
        {
            Ok(agent_result) => {
                let plan: serde_json::Value = super::extract_json_from_response(&agent_result)
                    .unwrap_or_else(|| {
                        json!({
                            "version": 1, "creature_id": creature_id,
                            "species": species_clone, "origin": origin_clone,
                            "destination": dest_clone, "narrative": agent_result,
                            "waypoints": [], "segments": [],
                        })
                    });
                let _ = record_transition(
                    pool_bg,
                    creature_id,
                    "active",
                    None,
                    "flight_plan",
                    "flight_coordinator",
                    0.0,
                    0.0,
                    "",
                    None,
                    None,
                    &json!({ "plan": plan }),
                )
                .await;
                eprintln!("[flight_plan] completed for creature {}", creature_id);
            }
            Err(e) => eprintln!("[flight_plan] failed for creature {}: {}", creature_id, e),
        }
    });

    // Emit activity event (fire-and-forget)
    {
        let _pool_ae = state.memory_store.pool().clone();
        let _uid_ae = user_id.clone();
        let _cid_ae = req.creature_id;
        let _specimen_ae = specimen_name.clone().unwrap_or_else(|| "Creature".into());
        let _swarm_ae = req.swarm_id;
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &_pool_ae,
                &_uid_ae,
                Some(_cid_ae),
                "flight_planned",
                _swarm_ae,
                None,
                &format!("Flight plan dispatched for {}", _specimen_ae),
                None,
                None,
            )
            .await;
        });
    }

    Ok(Json(json!({
        "status": "processing",
        "message": "Flight plan dispatched — check creature log for results",
        "gas_charged": total_cost,
        "creature_count": creature_count,
        "pricing": if req.swarm_id.is_some() {
            format!("{}cr base + {}cr ({} creatures)", state.gas_fees.flight_plan, creature_count, creature_count)
        } else {
            format!("{}cr", state.gas_fees.flight_plan)
        },
    })))
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

// ── Perch + Fly model ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FlyRequest {
    /// Optional destination — omit for free-form wander
    pub destination: Option<serde_json::Value>, // { lat, lng, name? }
    /// Optional creative route prompt for agent
    pub prompt: Option<String>,
}

/// POST /api/creatures/:creature_id/fly — start a flight (1cr hop, 5cr expedition)
///
/// Works from any state: perched, hosting, in rabble, flying, or unplaced.
/// Auto-ends any existing flight. No state gating — creature can always fly.
pub async fn fly_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<FlyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate creature ownership
    let creature = verify_creature_ownership(pool, creature_id, &user_id).await?;

    let specimen_name: Option<String> = creature.try_get("specimen_name").unwrap_or(None);
    let specimen_name_for_activity = specimen_name.clone();

    // Check for active flight — auto-end if needed, creature can always change state
    let active_flight = sqlx::query(
        "SELECT flight_id, swarm_id, flight_pattern, location_name
         FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let prev_location_name: String = active_flight
        .as_ref()
        .and_then(|r| r.try_get("location_name").ok())
        .unwrap_or_else(|| "current location".to_string());

    let (flight_id, swarm_id) = if let Some(af) = active_flight {
        let fid: Uuid = af.get("flight_id");
        let sid: Option<Uuid> = af.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
        let pattern: String = af.try_get("flight_pattern").unwrap_or_default();

        if pattern != "perch" {
            // Already in non-perch flight — auto-end it so creature can start fresh
            sqlx::query(
                "UPDATE creature_flights SET ended_at = NOW(),
                 duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
                 WHERE flight_id = $1",
            )
            .bind(fid)
            .execute(pool)
            .await
            .ok();

            // Decrement swarm creature count if leaving a rabble
            if let Some(sid) = sid {
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

        (fid, sid)
    } else {
        // No active flight — that's fine, we'll create one
        (Uuid::nil(), None)
    };

    let swarm_id = swarm_id.unwrap_or(Uuid::nil());

    // Tiered fly pricing: hop (no prompt) = 1cr, expedition (with prompt) = 5cr
    let is_expedition = req.prompt.is_some();
    let fly_cost = if is_expedition {
        state.gas_fees.flight_plan
    } else {
        1
    };
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        fly_cost,
        if is_expedition { "expedition" } else { "fly" },
        &format!(
            "{} creature {} from perch",
            if is_expedition {
                "Expedition for"
            } else {
                "Hop"
            },
            creature_id
        ),
        Some(&creature_id.to_string()),
    )
    .await?;

    // Start or update the flight
    let pattern = if is_expedition { "expedition" } else { "fly" };
    let flight_id = if flight_id.is_nil() {
        // No active flight — create a new fly flight
        let new_fid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO creature_flights (flight_id, creature_id, owner_id, h3_cell, center_lat, center_lng,
             flight_pattern, started_at, data_source)
             VALUES ($1, $2, $3, '', 0, 0, $4, NOW(), 'app')",
        )
        .bind(new_fid)
        .bind(creature_id)
        .bind(&user_id)
        .bind(pattern)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        new_fid
    } else {
        // Active perch flight — update pattern
        sqlx::query("UPDATE creature_flights SET flight_pattern = $1 WHERE flight_id = $2")
            .bind(pattern)
            .bind(flight_id)
            .execute(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        flight_id
    };

    // Defer dual-write + destination metadata to background
    {
        let pool_bg = state.db.clone();
        let uid_bg = user_id.clone();
        let dest_bg = req.destination.clone();
        let transition_type = if is_expedition { "expedition" } else { "fly" };
        tokio::spawn(async move {
            // Record fly transition (new versioned model)
            let prev = get_current_state(&pool_bg, creature_id).await;
            let prev_state = prev.as_ref().map(|(s, _)| s.as_str());
            let _ = record_transition(
                &pool_bg,
                creature_id,
                transition_type,
                prev_state,
                "fly",
                &uid_bg,
                0.0,
                0.0,
                "",
                None,
                None,
                &json!({
                    "flight_id": flight_id,
                    "from_swarm_id": swarm_id,
                    "destination": dest_bg,
                    "is_expedition": is_expedition,
                    "cost": fly_cost,
                }),
            )
            .await;

            // Store destination in flight metadata if provided
            if let Some(ref dest) = dest_bg {
                sqlx::query(
                    "UPDATE creature_flights SET environment = COALESCE(environment, '{}'::jsonb) || jsonb_build_object('destination', $1::jsonb)
                     WHERE flight_id = $2",
                )
                .bind(dest)
                .bind(flight_id)
                .execute(&pool_bg)
                .await
                .ok();
            }
        });
    }

    // Return immediately — defer workspace lookup + flight_coordinator to background
    let has_plan_request = req.destination.is_some() || req.prompt.is_some();

    if has_plan_request {
        let spawn_state = state.clone();
        let spawn_user = user_id.clone();
        let spawn_creature = creature_id;
        let spawn_flight = flight_id;
        let species: String = creature.get("species_group");
        let specimen_name: Option<String> = creature.try_get("specimen_name").unwrap_or(None);
        let scientific_name: Option<String> = creature.try_get("scientific_name").unwrap_or(None);
        let creature_label =
            specimen_name.unwrap_or_else(|| scientific_name.unwrap_or_else(|| species.clone()));
        let loc_name = prev_location_name.clone();
        let dest_clone = req.destination.clone();
        let prompt_clone = req.prompt.clone();

        tokio::spawn(async move {
            let pool_bg = &spawn_state.db;

            // Find workspace
            let ws_id = sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
                .bind(swarm_id)
                .fetch_optional(pool_bg)
                .await
                .ok()
                .flatten()
                .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten());

            let ws_id = match ws_id {
                Some(id) => id,
                None => {
                    match sqlx::query("SELECT personal_workspace_id FROM users WHERE user_id = $1")
                        .bind(&spawn_user)
                        .fetch_optional(pool_bg)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|r| {
                            r.try_get::<Option<Uuid>, _>("personal_workspace_id")
                                .ok()
                                .flatten()
                        }) {
                        Some(id) => id,
                        None => {
                            eprintln!("[fly] No workspace for flight_coordinator dispatch");
                            return;
                        }
                    }
                }
            };

            let query = if let Some(ref dest) = dest_clone {
                let dest_name = dest
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("destination");
                if let Some(ref prompt) = prompt_clone {
                    format!(
                        "Plan a flight for {} ({}) from {} to {}. Creative route: {}",
                        creature_label, species, loc_name, dest_name, prompt
                    )
                } else {
                    format!(
                        "Plan a flight for {} ({}) from {} to {}.",
                        creature_label, species, loc_name, dest_name
                    )
                }
            } else {
                format!(
                    "Describe a wandering flight for {} ({}) around {}. {}",
                    creature_label,
                    species,
                    loc_name,
                    prompt_clone.as_deref().unwrap_or("")
                )
            };

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
                    // Store raw plan in metadata
                    let _ = sqlx::query(
                        "UPDATE creature_flights SET metadata = jsonb_set(
                            COALESCE(metadata, '{}'::jsonb), '{flight_plan}', $1::jsonb
                        ) WHERE flight_id = $2",
                    )
                    .bind(serde_json::to_string(&result).unwrap_or_default())
                    .bind(spawn_flight)
                    .execute(pool_bg)
                    .await
                    .ok();

                    // Parse flight plan JSON and populate path_samples + environment
                    // The agent returns JSON with "waypoints" array
                    let plan_text = result
                        .trim()
                        .trim_start_matches("```json")
                        .trim_end_matches("```")
                        .trim();
                    if let Ok(plan) = serde_json::from_str::<serde_json::Value>(plan_text) {
                        if let Some(waypoints) = plan.get("waypoints").and_then(|w| w.as_array()) {
                            // path_samples: [{lat, lng, heading, speed, t}, ...]
                            let path_samples: Vec<serde_json::Value> = waypoints.iter().map(|wp| {
                                json!({
                                    "lat": wp.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    "lng": wp.get("lng").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    "heading": wp.get("heading").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    "t": wp.get("t_offset_s").and_then(|v| v.as_f64()).unwrap_or(0.0) * 1000.0,
                                })
                            }).collect();

                            // environment: [{wind_speed, wind_direction, temperature, terrain_type, elevation, t_offset_s}, ...]
                            let environment: Vec<serde_json::Value> = waypoints.iter().map(|wp| {
                                json!({
                                    "wind_speed": wp.get("wind_speed").and_then(|v| v.as_f64()),
                                    "wind_direction": wp.get("wind_direction").and_then(|v| v.as_f64()),
                                    "temperature": wp.get("temperature").and_then(|v| v.as_f64()),
                                    "terrain_type": wp.get("terrain_type").and_then(|v| v.as_str()),
                                    "elevation": wp.get("elevation").and_then(|v| v.as_f64()),
                                    "t_offset_s": wp.get("t_offset_s").and_then(|v| v.as_f64()),
                                })
                            }).collect();

                            // Also extract destination coords for flight endpoint
                            let last_wp = waypoints.last();
                            let dest_lat = last_wp
                                .and_then(|w| w.get("lat"))
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            let dest_lng = last_wp
                                .and_then(|w| w.get("lng"))
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);

                            // Store narrative + species_notes in metadata
                            let narrative =
                                plan.get("narrative").and_then(|v| v.as_str()).unwrap_or("");
                            let species_notes = plan
                                .get("species_notes")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let total_distance_km =
                                plan.get("total_distance_km").and_then(|v| v.as_f64());

                            let _ = sqlx::query(
                                "UPDATE creature_flights SET
                                    path_samples = $1::jsonb,
                                    environment = $2::jsonb,
                                    metadata = COALESCE(metadata, '{}'::jsonb)
                                        || jsonb_build_object(
                                            'narrative', $3::text,
                                            'species_notes', $4::text,
                                            'total_distance_km', $5::float8,
                                            'segments', $6::jsonb,
                                            'dest_lat', $7::float8,
                                            'dest_lng', $8::float8
                                        )
                                WHERE flight_id = $9",
                            )
                            .bind(serde_json::to_string(&path_samples).unwrap_or_default())
                            .bind(serde_json::to_string(&environment).unwrap_or_default())
                            .bind(narrative)
                            .bind(species_notes)
                            .bind(total_distance_km)
                            .bind(serde_json::to_string(&plan.get("segments")).unwrap_or_default())
                            .bind(dest_lat)
                            .bind(dest_lng)
                            .bind(spawn_flight)
                            .execute(pool_bg)
                            .await
                            .ok();

                            eprintln!(
                                "[fly] flight plan populated: {} waypoints, dest ({}, {})",
                                path_samples.len(),
                                dest_lat,
                                dest_lng
                            );
                        }
                    }

                    eprintln!(
                        "[fly] flight_coordinator completed for creature {}",
                        spawn_creature
                    );
                }
                Err(e) => eprintln!("[fly] flight_coordinator dispatch failed: {}", e),
            }
        });
    }

    // Emit activity event (fire-and-forget)
    {
        let _pool_ae = state.memory_store.pool().clone();
        let _uid_ae = user_id.clone();
        let _specimen_ae = specimen_name_for_activity
            .clone()
            .unwrap_or_else(|| "Creature".into());
        // Convert Uuid to Option<Uuid> - nil UUID means no swarm
        let _swarm_ae = if swarm_id == Uuid::nil() {
            None
        } else {
            Some(swarm_id)
        };
        let _is_exp = is_expedition;
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &_pool_ae,
                &_uid_ae,
                Some(creature_id),
                "creature_flew",
                _swarm_ae,
                None,
                &format!(
                    "{} {}",
                    _specimen_ae,
                    if _is_exp {
                        "launched an expedition"
                    } else {
                        "took flight"
                    }
                ),
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
        "flight_started",
        json!({
            "flight_id": flight_id,
            "swarm_id": swarm_id,
            "pattern": if is_expedition { "expedition" } else { "fly" },
        }),
    );

    Ok(Json(json!({
        "flight_id": flight_id,
        "swarm_id": swarm_id,
        "creature_id": creature_id,
        "pattern": if is_expedition { "expedition" } else { "fly" },
        "plan": if has_plan_request { "generating" } else { "none" },
        "gas_charged": fly_cost,
    })))
}
