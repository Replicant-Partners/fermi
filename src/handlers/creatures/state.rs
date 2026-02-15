//! Creature state transitions and flight management.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::handlers::rabble_workspace;
use crate::AppState;
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

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
    let creature = sqlx::query(
        "SELECT c.owner_id, COALESCE(cc.visibility, 'public') AS visibility
         FROM creatures c
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         WHERE c.creature_id = $1",
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

            let _ = record_transition(
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
    let plan: serde_json::Value =
        super::extract_json_from_response(&agent_result).unwrap_or_else(|| {
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
    /// Operational radius in meters (default 100). Defines bounded area for flock dynamics.
    pub radius_meters: Option<i32>,
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
    .bind(req.radius_meters.unwrap_or(100).max(10).min(10000))
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Create workspace in background — don't block the HTTP response
    {
        let state_ws = state.clone();
        let user_ws = user_id.clone();
        let name_ws = perch_name.clone();
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
                    eprintln!("[perch] Created workspace {} for swarm {}", ws_id, swarm_id)
                }
                Err((_status, msg)) => eprintln!(
                    "[perch] Workspace creation failed for swarm {}: {}",
                    swarm_id, msg
                ),
            }
        });
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
    let _ = record_transition(
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
    let fly_start = std::time::Instant::now();
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    eprintln!(
        "[fly] handler entered for creature {} user {}",
        creature_id, user_id
    );

    // Validate creature ownership
    let creature = sqlx::query(
        "SELECT owner_id, species_group, specimen_name, scientific_name
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

    eprintln!("[fly] charged gas in {:?}", fly_start.elapsed());

    // Start or update the flight
    let flight_id = if flight_id.is_nil() {
        // No active flight — create a new fly flight
        let new_fid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO creature_flights (flight_id, creature_id, owner_id, h3_cell, center_lat, center_lng,
             flight_pattern, started_at, data_source)
             VALUES ($1, $2, $3, '', 0, 0, 'fly', NOW(), 'app')",
        )
        .bind(new_fid)
        .bind(creature_id)
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        new_fid
    } else {
        // Active perch flight — update pattern to 'fly'
        sqlx::query("UPDATE creature_flights SET flight_pattern = 'fly' WHERE flight_id = $1")
            .bind(flight_id)
            .execute(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        flight_id
    };

    // ── Dual-write: record fly transition (new versioned model) ──
    {
        // Get current state to record previous_state
        let prev = get_current_state(pool, creature_id).await;
        let prev_state = prev.as_ref().map(|(s, _)| s.as_str());
        let _ = record_transition(
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

    eprintln!("[fly] transition recorded in {:?}", fly_start.elapsed());

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

        let loc_name = prev_location_name.clone();

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

    eprintln!("[fly] returning response in {:?}", fly_start.elapsed());

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

    // ── Dual-write: record join transition (new versioned model) ──
    {
        let prev = get_current_state(pool, req.creature_id).await;
        let prev_state = prev.as_ref().map(|(s, _)| s.as_str());
        let _ = record_transition(
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
        let _ = crate::handlers::rabble_chat::insert_system_message(
            &state,
            swarm_id,
            "We have a rabble!!",
        )
        .await;
    }

    let _ = crate::handlers::rabble_chat::insert_system_message(
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
            super::trigger_swarm_host_welcome(
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
        "UPDATE creature_conditions SET presence = $1, updated_at = NOW()
         WHERE creature_id = $2",
    )
    .bind(&req.presence)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Creature not found or conditions not initialized".to_string(),
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
    let creature =
        sqlx::query("SELECT owner_id, specimen_name FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Creature not found".to_string()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".to_string()));
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
    sqlx::query("UPDATE creature_conditions SET presence = 'tracking', updated_at = NOW() WHERE creature_id = $1")
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

    // End tracking flights — but if the creature is in a rabble, keep it there.
    // Find the tether flight first to check if it's attached to a swarm.
    let tether_flight = sqlx::query(
        "SELECT flight_id, swarm_id, center_lat, center_lng, h3_cell
         FROM creature_flights
         WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device' LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let stayed_in_rabble = if let Some(ref tf) = tether_flight {
        let swarm_id: Option<Uuid> = tf.try_get::<Option<Uuid>, _>("swarm_id").ok().flatten();
        let tf_id: Uuid = tf.get("flight_id");

        // End the tether flight
        sqlx::query(
            "UPDATE creature_flights SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
             WHERE flight_id = $1",
        )
        .bind(tf_id)
        .execute(pool)
        .await
        .ok();

        // If it was in a rabble, create a new static swarm flight at current position.
        // The rabble freezes at the last anchor location.
        if let Some(sid) = swarm_id {
            let lat: f64 = tf.try_get("center_lat").unwrap_or(0.0);
            let lng: f64 = tf.try_get("center_lng").unwrap_or(0.0);
            let h3: String = tf.try_get("h3_cell").unwrap_or_else(|_| String::new());
            sqlx::query(
                "INSERT INTO creature_flights (flight_id, creature_id, owner_id,
                 h3_cell, h3_resolution, center_lat, center_lng,
                 flight_pattern, swarm_id, started_at)
                 VALUES ($1, $2, $3, $4, 12, $5, $6, 'swarm', $7, NOW())",
            )
            .bind(Uuid::new_v4())
            .bind(creature_id)
            .bind(&user_id)
            .bind(&h3)
            .bind(lat)
            .bind(lng)
            .bind(sid)
            .execute(pool)
            .await
            .ok();
            true
        } else {
            false
        }
    } else {
        // No tether flight found — end any other device flights
        sqlx::query(
            "UPDATE creature_flights SET ended_at = NOW(),
             duration_seconds = EXTRACT(EPOCH FROM (NOW() - started_at))::int
             WHERE creature_id = $1 AND ended_at IS NULL AND data_source = 'device'",
        )
        .bind(creature_id)
        .execute(pool)
        .await
        .ok();
        false
    };

    // Set presence back to active
    sqlx::query("UPDATE creature_conditions SET presence = 'active', updated_at = NOW() WHERE creature_id = $1")
        .bind(creature_id)
        .execute(pool)
        .await
        .ok();

    Ok(Json(json!({
        "creature_id": creature_id,
        "status": "untethered",
        "stayed_in_rabble": stayed_in_rabble,
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

        // Anchor propagation: if this creature is a rabble's anchor, move the rabble
        let h3 = h3o::LatLng::new(last.lat, last.lng)
            .ok()
            .map(|ll| ll.to_cell(h3o::Resolution::Twelve).to_string())
            .unwrap_or_default();

        let updated = sqlx::query(
            "UPDATE swarm_events SET center_lat = $1, center_lng = $2, h3_cell = $3
             WHERE anchor_creature_id = $4 AND status IN ('scheduled', 'active')",
        )
        .bind(last.lat)
        .bind(last.lng)
        .bind(&h3)
        .bind(creature_id)
        .execute(pool)
        .await
        .ok()
        .map(|r| r.rows_affected())
        .unwrap_or(0);

        if updated > 0 {
            eprintln!(
                "[tether] Anchor creature {} moved rabble to ({}, {})",
                creature_id, last.lat, last.lng
            );
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

// ═══════════════════════════════════════════════════════════════════
// Enemy Sensor — enable / disable / check
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct EnemySensorRequest {
    pub action: String, // "enable" | "disable" | "check"
}

/// POST /api/creatures/:creature_id/enemy-sensor
pub async fn enemy_sensor_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<EnemySensorRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let gas = &state.gas_fees;

    // Verify ownership
    let creature = sqlx::query(
        "SELECT c.owner_id, c.scientific_name, c.common_name, c.species_group, c.workspace_id,
                c.taxonomy
         FROM creatures c WHERE c.creature_id = $1",
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

    match req.action.as_str() {
        "enable" => {
            // One-time unlock fee — check ledger for prior purchase
            let already_paid: bool = sqlx::query(
                "SELECT 1 FROM credit_ledger WHERE tx_type = 'enemy_sensor_enable' AND related_id = $1 LIMIT 1",
            )
            .bind(creature_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false);

            let cost = if already_paid {
                0
            } else {
                let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                charge_gas(
                    &state.db,
                    wallet.wallet_id,
                    gas.enemy_sensor_enable,
                    "enemy_sensor_enable",
                    &format!("Unlock enemy sensor for creature {}", creature_id),
                    Some(&creature_id.to_string()),
                )
                .await?;
                gas.enemy_sensor_enable
            };

            toggle_module(pool, creature_id, "enemy_sensor", true).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "enemy_sensor": "enabled",
                "cost": cost,
            })))
        }
        "disable" => {
            toggle_module(pool, creature_id, "enemy_sensor", false).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "enemy_sensor": "disabled",
            })))
        }
        "check" => {
            // Verify module is enabled
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

            if !modules.contains(&"enemy_sensor".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Enemy sensor not enabled — enable it first (5cr)".to_string(),
                ));
            }

            // Charge check fee
            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.enemy_sensor_check,
                "enemy_sensor_check",
                &format!("Enemy sensor scan for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            // Build query for the agent
            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let species_group: String = creature
                .try_get("species_group")
                .unwrap_or_else(|_| "insect".to_string());
            let taxonomy: Option<serde_json::Value> = creature.try_get("taxonomy").ok().flatten();
            let order = taxonomy
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let query = format!(
                "Scan for natural enemies near creature {} ({}, order {}). \
                 Use scan_nearby_creatures with creature_id \"{}\" to find who is nearby, \
                 then assess predation risk.",
                creature
                    .try_get::<Option<String>, _>("common_name")
                    .unwrap_or(None)
                    .unwrap_or_else(|| scientific_name.clone()),
                scientific_name,
                order,
                creature_id,
            );

            // Get workspace_id — look up via swarm (workspace is created per-rabble)
            let workspace_id: Uuid = sqlx::query(
                "SELECT se.workspace_id
                 FROM creature_flights cf
                 JOIN swarm_events se ON se.swarm_id = cf.swarm_id
                 WHERE cf.creature_id = $1 AND cf.ended_at IS NULL AND se.workspace_id IS NOT NULL
                 LIMIT 1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten())
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Creature has no workspace — perch first, then wait a moment for workspace setup"
                    .to_string(),
            ))?;

            // Dispatch to enemy_sensor agent
            let assessment = rabble_workspace::dispatch_rabble_action(
                &state,
                workspace_id,
                "enemy_sensor",
                "threat_scan",
                &query,
                &user_id,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Enemy sensor scan failed: {}", e),
                )
            })?;

            // Try to parse agent response as JSON for structured output
            let parsed: serde_json::Value =
                serde_json::from_str(&assessment).unwrap_or_else(|_| {
                    json!({
                        "threat_level": "unknown",
                        "summary": assessment,
                        "threats": [],
                    })
                });

            Ok(Json(json!({
                "creature_id": creature_id,
                "species_group": species_group,
                "cost": gas.enemy_sensor_check,
                "assessment": parsed,
            })))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown action '{}' — use 'enable', 'disable', or 'check'",
                other
            ),
        )),
    }
}

// ── Genome Profiler ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GenomeProfilerRequest {
    pub action: String, // "enable", "disable", "check"
}

/// POST /api/creatures/:creature_id/genome-profiler
pub async fn genome_profiler_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<GenomeProfilerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let gas = &state.gas_fees;

    let creature = sqlx::query(
        "SELECT c.owner_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy, c.gbif_key
         FROM creatures c WHERE c.creature_id = $1",
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

    match req.action.as_str() {
        "enable" => {
            // One-time unlock fee — check ledger for prior purchase
            let already_paid: bool = sqlx::query(
                "SELECT 1 FROM credit_ledger WHERE tx_type = 'genome_profiler_enable' AND related_id = $1 LIMIT 1",
            )
            .bind(creature_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false);

            let cost = if already_paid {
                0
            } else {
                let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                charge_gas(
                    &state.db,
                    wallet.wallet_id,
                    gas.genome_profiler_enable,
                    "genome_profiler_enable",
                    &format!("Unlock genome profiler for creature {}", creature_id),
                    Some(&creature_id.to_string()),
                )
                .await?;
                gas.genome_profiler_enable
            };

            toggle_module(pool, creature_id, "genome_profiler", true).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "genome_profiler": "enabled",
                "cost": cost,
            })))
        }
        "disable" => {
            toggle_module(pool, creature_id, "genome_profiler", false).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "genome_profiler": "disabled",
            })))
        }
        "check" => {
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

            if !modules.contains(&"genome_profiler".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Genome profiler not enabled — enable it first (5cr)".to_string(),
                ));
            }

            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.genome_profiler_check,
                "genome_profiler_check",
                &format!("Genome profile for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let gbif_key: Option<i64> = creature.try_get("gbif_key").ok().flatten();
            let taxonomy: Option<serde_json::Value> = creature.try_get("taxonomy").ok().flatten();
            let order = taxonomy
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let query = format!(
                "Build a phylogenetic profile for {} (order {}). \
                 Use gbif_taxonomy_tree with {} to get the full taxonomy hierarchy, \
                 then analyze its genomic context and evolutionary relationships.",
                scientific_name,
                order,
                if let Some(key) = gbif_key {
                    format!("gbif_key {}", key)
                } else {
                    format!("scientific_name \"{}\"", scientific_name)
                },
            );

            // Get workspace from swarm
            let workspace_id: Uuid = sqlx::query(
                "SELECT se.workspace_id
                 FROM creature_flights cf
                 JOIN swarm_events se ON se.swarm_id = cf.swarm_id
                 WHERE cf.creature_id = $1 AND cf.ended_at IS NULL AND se.workspace_id IS NOT NULL
                 LIMIT 1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten())
            .ok_or((
                StatusCode::BAD_REQUEST,
                "Creature has no workspace yet — perch first".to_string(),
            ))?;

            let profile = rabble_workspace::dispatch_rabble_action(
                &state,
                workspace_id,
                "genome_profiler",
                "phylogenetic_profile",
                &query,
                &user_id,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Genome profiler failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value = serde_json::from_str(&profile).unwrap_or_else(|_| {
                json!({
                    "summary": profile,
                    "taxonomy": {},
                    "genome": {},
                    "phylogeny": {},
                })
            });

            Ok(Json(json!({
                "creature_id": creature_id,
                "cost": gas.genome_profiler_check,
                "profile": parsed,
            })))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown action '{}' — use 'enable', 'disable', or 'check'",
                other
            ),
        )),
    }
}

// ── Prey Locator (premium hunting) ──────────────────────────────

#[derive(Deserialize)]
pub struct PreyLocatorRequest {
    pub action: String,                   // "enable", "disable", "scan", "stalk"
    pub target_creature_id: Option<Uuid>, // required for "stalk"
}

/// POST /api/creatures/:creature_id/prey-locator
///
/// Premium tactical hunting: scan for prey (2cr), then stalk with flight plan (5cr).
/// One-time unlock: 5cr.
pub async fn prey_locator_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Json(req): Json<PreyLocatorRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let gas = &state.gas_fees;

    let creature = sqlx::query(
        "SELECT c.owner_id, c.scientific_name, c.common_name, c.species_group, c.taxonomy
         FROM creatures c WHERE c.creature_id = $1",
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

    match req.action.as_str() {
        "enable" => {
            let already_paid: bool = sqlx::query(
                "SELECT 1 FROM credit_ledger WHERE tx_type = 'prey_locator_enable' AND related_id = $1 LIMIT 1",
            )
            .bind(creature_id.to_string())
            .fetch_optional(pool)
            .await
            .map(|r| r.is_some())
            .unwrap_or(false);

            let cost = if already_paid {
                0
            } else {
                let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                charge_gas(
                    &state.db,
                    wallet.wallet_id,
                    gas.prey_locator_enable,
                    "prey_locator_enable",
                    &format!("Unlock prey locator for creature {}", creature_id),
                    Some(&creature_id.to_string()),
                )
                .await?;
                gas.prey_locator_enable
            };

            toggle_module(pool, creature_id, "prey_locator", true).await;

            Ok(Json(json!({
                "creature_id": creature_id,
                "prey_locator": "enabled",
                "cost": cost,
            })))
        }
        "disable" => {
            toggle_module(pool, creature_id, "prey_locator", false).await;
            Ok(Json(
                json!({ "creature_id": creature_id, "prey_locator": "disabled" }),
            ))
        }
        "scan" => {
            // Verify module is enabled
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

            if !modules.contains(&"prey_locator".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Prey locator not enabled".to_string(),
                ));
            }

            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.prey_locator_scan,
                "prey_locator_scan",
                &format!("Prey scan for creature {}", creature_id),
                Some(&creature_id.to_string()),
            )
            .await?;

            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let taxonomy: Option<serde_json::Value> = creature.try_get("taxonomy").ok().flatten();
            let order = taxonomy
                .as_ref()
                .and_then(|t| t.get("order"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let common_name: Option<String> = creature
                .try_get::<Option<String>, _>("common_name")
                .unwrap_or(None);
            let label = common_name.unwrap_or_else(|| scientific_name.clone());

            let query = format!(
                "SCAN MODE: Identify viable prey near {} ({}, order {}). \
                 Use scan_nearby_creatures with creature_id \"{}\" to find who is nearby, \
                 then assess which creatures this predator could hunt. \
                 Rank by vulnerability and accessibility.",
                label, scientific_name, order, creature_id,
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            let result = rabble_workspace::dispatch_rabble_action(
                &state,
                workspace_id,
                "prey_locator",
                "prey_scan",
                &query,
                &user_id,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Prey scan failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value = serde_json::from_str(&result)
                .unwrap_or_else(|_| json!({ "prey_targets": [], "hunting_summary": result }));

            Ok(Json(json!({
                "creature_id": creature_id,
                "cost": gas.prey_locator_scan,
                "scan": parsed,
            })))
        }
        "stalk" => {
            let target_id = req.target_creature_id.ok_or((
                StatusCode::BAD_REQUEST,
                "target_creature_id required for stalk".to_string(),
            ))?;

            // Verify module enabled
            let modules: Vec<String> = sqlx::query(
                "SELECT active_modules FROM creature_conditions WHERE creature_id = $1",
            )
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .and_then(|r| {
                r.try_get::<Option<Vec<String>>, _>("active_modules")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

            if !modules.contains(&"prey_locator".to_string()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Prey locator not enabled".to_string(),
                ));
            }

            // Look up target creature
            let target = sqlx::query(
                "SELECT c.scientific_name, c.common_name, cs.location_lat, cs.location_lng, cs.h3_cell
                 FROM creatures c LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
                 WHERE c.creature_id = $1",
            )
            .bind(target_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Target creature not found".to_string()))?;

            let wallet = get_or_create_wallet(&state.db, "user", &user_id)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            charge_gas(
                &state.db,
                wallet.wallet_id,
                gas.prey_locator_stalk,
                "prey_locator_stalk",
                &format!(
                    "Stalk flight plan for creature {} → {}",
                    creature_id, target_id
                ),
                Some(&creature_id.to_string()),
            )
            .await?;

            let scientific_name: String = creature
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let target_name: String = target
                .try_get("scientific_name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let target_lat: f64 = target.try_get("location_lat").unwrap_or(0.0);
            let target_lng: f64 = target.try_get("location_lng").unwrap_or(0.0);

            let query = format!(
                "STALK MODE: Generate a tactical intercept flight plan for {} to hunt {} \
                 at position ({}, {}). Plan approach vectors, ambush positions, \
                 and waypoints. Consider prey escape behaviors and optimal intercept strategy.",
                scientific_name, target_name, target_lat, target_lng,
            );

            let workspace_id = find_creature_workspace(pool, creature_id).await?;

            let plan = rabble_workspace::dispatch_rabble_action(
                &state,
                workspace_id,
                "prey_locator",
                "stalk_plan",
                &query,
                &user_id,
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Stalk plan failed: {}", e),
                )
            })?;

            let parsed: serde_json::Value = serde_json::from_str(&plan).unwrap_or_else(
                |_| json!({ "flight_plan": { "approach": plan }, "tactical_notes": "" }),
            );

            Ok(Json(json!({
                "creature_id": creature_id,
                "target_creature_id": target_id,
                "cost": gas.prey_locator_stalk,
                "stalk": parsed,
            })))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown action '{}' — use 'enable', 'disable', 'scan', or 'stalk'",
                other
            ),
        )),
    }
}

/// Helper: find workspace for a creature via its active flight's swarm
async fn find_creature_workspace(
    pool: &PgPool,
    creature_id: Uuid,
) -> Result<Uuid, (StatusCode, String)> {
    sqlx::query(
        "SELECT se.workspace_id
         FROM creature_flights cf
         JOIN swarm_events se ON se.swarm_id = cf.swarm_id
         WHERE cf.creature_id = $1 AND cf.ended_at IS NULL AND se.workspace_id IS NOT NULL
         LIMIT 1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .and_then(|r| r.try_get::<Option<Uuid>, _>("workspace_id").ok().flatten())
    .ok_or((
        StatusCode::BAD_REQUEST,
        "Creature has no workspace — perch first".to_string(),
    ))
}

// ═══════════════════════════════════════════════════════════════════
// Creature versioned state — dual-write helpers
// (previously in handlers/creature_state.rs)
// ═══════════════════════════════════════════════════════════════════

/// Record a state transition: insert a creature_version and update creature_state.
///
/// Returns the new version_id.
pub(crate) async fn record_transition(
    pool: &PgPool,
    creature_id: Uuid,
    state: &str,
    previous_state: Option<&str>,
    transition_type: &str,
    triggered_by: &str,
    location_lat: f64,
    location_lng: f64,
    h3_cell: &str,
    rabble_id: Option<Uuid>,
    workspace_id: Option<Uuid>,
    metadata: &serde_json::Value,
) -> Result<Uuid, String> {
    // Get next version number for this creature
    let next_vn: i64 = sqlx::query(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM creature_versions WHERE creature_id = $1",
    )
    .bind(creature_id)
    .fetch_one(pool)
    .await
    .map(|r| r.get::<i64, _>(0))
    .unwrap_or(1);

    // Insert version (immutable)
    let version_id = Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO creature_versions (
            version_id, creature_id, version_number, state, previous_state,
            location_lat, location_lng, h3_cell, rabble_id,
            transition_type, triggered_by, workspace_id,
            valid_from, recorded_at, metadata
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW(), $13)",
    )
    .bind(version_id)
    .bind(creature_id)
    .bind(next_vn)
    .bind(state)
    .bind(previous_state)
    .bind(location_lat)
    .bind(location_lng)
    .bind(h3_cell)
    .bind(rabble_id)
    .bind(transition_type)
    .bind(triggered_by)
    .bind(workspace_id)
    .bind(metadata)
    .execute(pool)
    .await;

    if let Err(e) = result {
        eprintln!("[creature_state] version insert failed: {}", e);
        return Err(e.to_string());
    }

    // Upsert creature_state (mutable pointer)
    let result = sqlx::query(
        "INSERT INTO creature_state (
            creature_id, state, location_lat, location_lng, h3_cell,
            rabble_id, workspace_id, version_id, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
        ON CONFLICT (creature_id) DO UPDATE SET
            state = $2, location_lat = $3, location_lng = $4, h3_cell = $5,
            rabble_id = $6, workspace_id = $7, version_id = $8, updated_at = NOW()",
    )
    .bind(creature_id)
    .bind(state)
    .bind(location_lat)
    .bind(location_lng)
    .bind(h3_cell)
    .bind(rabble_id)
    .bind(workspace_id)
    .bind(version_id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        eprintln!("[creature_state] state upsert failed: {}", e);
        return Err(e.to_string());
    }

    Ok(version_id)
}

/// Initialize creature_conditions on mint.
pub(crate) async fn init_conditions(
    pool: &PgPool,
    creature_id: Uuid,
    visibility: &str,
    sosa_opt_in: bool,
) {
    let _ = sqlx::query(
        "INSERT INTO creature_conditions (creature_id, visibility, sosa_opt_in, presence, active_modules, updated_at)
         VALUES ($1, $2, $3, 'active', '{}', NOW())
         ON CONFLICT (creature_id) DO NOTHING",
    )
    .bind(creature_id)
    .bind(visibility)
    .bind(sosa_opt_in)
    .execute(pool)
    .await
    .map_err(|e| eprintln!("[creature_state] conditions init failed: {}", e));
}

/// Update a specific condition.
pub(crate) async fn update_condition_visibility(
    pool: &PgPool,
    creature_id: Uuid,
    visibility: &str,
) {
    let _ = sqlx::query(
        "UPDATE creature_conditions SET visibility = $1, updated_at = NOW() WHERE creature_id = $2",
    )
    .bind(visibility)
    .bind(creature_id)
    .execute(pool)
    .await
    .map_err(|e| eprintln!("[creature_state] visibility update failed: {}", e));
}

/// Toggle a module in active_modules array.
pub(crate) async fn toggle_module(pool: &PgPool, creature_id: Uuid, module: &str, active: bool) {
    let sql = if active {
        "UPDATE creature_conditions SET active_modules = array_append(
            array_remove(active_modules, $1), $1
         ), updated_at = NOW() WHERE creature_id = $2"
    } else {
        "UPDATE creature_conditions SET active_modules = array_remove(active_modules, $1),
         updated_at = NOW() WHERE creature_id = $2"
    };
    let _ = sqlx::query(sql)
        .bind(module)
        .bind(creature_id)
        .execute(pool)
        .await
        .map_err(|e| eprintln!("[creature_state] toggle_module failed: {}", e));
}

/// Get the current state for a creature. Returns (state, previous version_id) or None.
pub(crate) async fn get_current_state(
    pool: &PgPool,
    creature_id: Uuid,
) -> Option<(String, Option<Uuid>)> {
    sqlx::query("SELECT state, version_id FROM creature_state WHERE creature_id = $1")
        .bind(creature_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|r| {
            (
                r.get::<String, _>("state"),
                r.try_get::<Option<Uuid>, _>("version_id").unwrap_or(None),
            )
        })
}
