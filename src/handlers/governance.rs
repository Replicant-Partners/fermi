//! Governance handlers — Block, Eject, Report.
//!
//! Three primitives for social safety:
//!   1. Block: creature-level + user-level escalation (private, blocked party never knows)
//!   2. Eject: host removes creature from their rabble (24h cooldown or permanent)
//!   3. Report: flag content/behavior for admin review (with context snapshot)
//!
//! Design: docs/DESIGN_GOVERNANCE.md

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use fermi_auth::AuthPrincipal;

// ═══════════════════════════════════════════════════════════════════════════
// BLOCK — Creature Level
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct BlockCreatureRequest {
    pub blocked_creature_id: Uuid,
}

/// POST /api/creatures/:creature_id/block — block another creature.
///
/// The blocked creature's owner is NOT notified (privacy).
/// If a friendship exists between the two creatures, it is ended.
pub async fn block_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(blocker_creature_id): Path<Uuid>,
    Json(req): Json<BlockCreatureRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify the caller owns the blocking creature
    let creature = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(blocker_creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".into()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your creature".into()));
    }

    // Get the blocked creature's owner for denormalized column
    let blocked_creature = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(req.blocked_creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Target creature not found".into()))?;

    let blocked_user_id: String = blocked_creature.get("owner_id");

    // Can't block your own creature
    if blocker_creature_id == req.blocked_creature_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Cannot block your own creature".into(),
        ));
    }

    // Insert block (ON CONFLICT = idempotent)
    sqlx::query(
        "INSERT INTO creature_blocks (blocker_creature_id, blocked_creature_id, blocker_user_id, blocked_user_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (blocker_creature_id, blocked_creature_id) DO NOTHING",
    )
    .bind(blocker_creature_id)
    .bind(req.blocked_creature_id)
    .bind(&user_id)
    .bind(&blocked_user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // End any existing friendship between these two creatures
    sqlx::query(
        "UPDATE creature_friendships SET status = 'ended_by_block'
         WHERE status = 'accepted'
           AND ((creature_a = $1 AND creature_b = $2)
             OR (creature_a = $2 AND creature_b = $1))",
    )
    .bind(blocker_creature_id)
    .bind(req.blocked_creature_id)
    .execute(pool)
    .await
    .ok();

    // Also cancel any pending friendship requests
    sqlx::query(
        "UPDATE creature_friendships SET status = 'ended_by_block'
         WHERE status = 'pending'
           AND ((creature_a = $1 AND creature_b = $2)
             OR (creature_a = $2 AND creature_b = $1))",
    )
    .bind(blocker_creature_id)
    .bind(req.blocked_creature_id)
    .execute(pool)
    .await
    .ok();

    Ok(Json(json!({
        "status": "blocked",
        "blocker_creature_id": blocker_creature_id,
        "blocked_creature_id": req.blocked_creature_id,
    })))
}

/// DELETE /api/creatures/:creature_id/block/:blocked_creature_id — unblock a creature.
pub async fn unblock_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((blocker_creature_id, blocked_creature_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id FROM creatures WHERE creature_id = $1")
            .bind(blocker_creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if owner.as_deref() != Some(&user_id) {
        return Err((StatusCode::FORBIDDEN, "Not your creature".into()));
    }

    sqlx::query(
        "DELETE FROM creature_blocks
         WHERE blocker_creature_id = $1 AND blocked_creature_id = $2",
    )
    .bind(blocker_creature_id)
    .bind(blocked_creature_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "unblocked",
        "blocker_creature_id": blocker_creature_id,
        "blocked_creature_id": blocked_creature_id,
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOCK — User Level (Escalation)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct BlockUserRequest {
    pub blocked_user_id: String,
}

/// POST /api/users/block — block a user entirely.
///
/// All creatures owned by the blocked user become invisible to all of the
/// blocker's creatures. Messages hidden in shared rabbles. The blocked user
/// does NOT know they are blocked.
pub async fn block_user_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<BlockUserRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    if user_id == req.blocked_user_id {
        return Err((StatusCode::BAD_REQUEST, "Cannot block yourself".into()));
    }

    // Verify the target user exists
    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE user_id = $1)")
            .bind(&req.blocked_user_id)
            .fetch_one(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !exists {
        return Err((StatusCode::NOT_FOUND, "User not found".into()));
    }

    // Insert user block (idempotent)
    sqlx::query(
        "INSERT INTO user_blocks (blocker_user_id, blocked_user_id)
         VALUES ($1, $2)
         ON CONFLICT (blocker_user_id, blocked_user_id) DO NOTHING",
    )
    .bind(&user_id)
    .bind(&req.blocked_user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // End ALL friendships between any creatures of these two users
    sqlx::query(
        "UPDATE creature_friendships SET status = 'ended_by_block'
         WHERE status IN ('accepted', 'pending')
           AND ((creature_a IN (SELECT creature_id FROM creatures WHERE owner_id = $1)
             AND creature_b IN (SELECT creature_id FROM creatures WHERE owner_id = $2))
            OR (creature_a IN (SELECT creature_id FROM creatures WHERE owner_id = $2)
             AND creature_b IN (SELECT creature_id FROM creatures WHERE owner_id = $1)))",
    )
    .bind(&user_id)
    .bind(&req.blocked_user_id)
    .execute(pool)
    .await
    .ok();

    Ok(Json(json!({
        "status": "blocked",
        "blocked_user_id": req.blocked_user_id,
    })))
}

/// DELETE /api/users/block/:blocked_user_id — unblock a user.
pub async fn unblock_user_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(blocked_user_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    sqlx::query("DELETE FROM user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2")
        .bind(&user_id)
        .bind(&blocked_user_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "unblocked",
        "blocked_user_id": blocked_user_id,
    })))
}

/// GET /api/my/blocks — list all blocks (creature-level + user-level).
pub async fn list_blocks_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let rows = sqlx::query("SELECT * FROM get_user_blocks($1)")
        .bind(&user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let blocks: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "block_id": r.get::<Uuid, _>("block_id"),
                "block_level": r.get::<String, _>("block_level"),
                "blocked_entity_id": r.get::<String, _>("blocked_entity_id"),
                "blocked_name": r.try_get::<Option<String>, _>("blocked_name").unwrap_or(None),
                "blocked_image": r.try_get::<Option<String>, _>("blocked_image").unwrap_or(None),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "blocks": blocks,
        "count": blocks.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// EJECT — Host removes creature from rabble
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct EjectRequest {
    pub creature_id: Uuid,
    pub reason: Option<String>,
    #[serde(default)]
    pub permanent: bool,
}

/// POST /api/rabble/:id/eject — host removes a creature from their rabble.
///
/// The ejected creature faces a 24h cooldown before rejoining (or permanent ban).
/// A system message is posted in chat. The ejected user is notified.
pub async fn eject_creature_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<Uuid>,
    Json(req): Json<EjectRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify caller is the rabble host (owns the anchor creature or is creator)
    let swarm = sqlx::query(
        "SELECT s.creator_id, s.name, s.anchor_creature_id, c.owner_id AS anchor_owner_id
         FROM swarm_events s
         LEFT JOIN creatures c ON c.creature_id = s.anchor_creature_id
         WHERE s.swarm_id = $1 AND s.status IN ('active', 'scheduled')",
    )
    .bind(swarm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Active rabble not found".into()))?;

    let creator_id: String = swarm.get("creator_id");
    let swarm_name: String = swarm.try_get("name").unwrap_or_else(|_| "Rabble".into());
    let anchor_owner: Option<String> = swarm
        .try_get::<Option<String>, _>("anchor_owner_id")
        .unwrap_or(None);
    let anchor_creature_id: Option<Uuid> = swarm
        .try_get::<Option<Uuid>, _>("anchor_creature_id")
        .unwrap_or(None);

    let is_host = anchor_owner.as_deref() == Some(&user_id) || creator_id == user_id;
    if !is_host {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the rabble host can eject creatures".into(),
        ));
    }

    // Can't eject the anchor creature — use End Rabble instead
    if anchor_creature_id == Some(req.creature_id) {
        return Err((
            StatusCode::CONFLICT,
            "Cannot eject the anchor creature. Use End Rabble or Transfer Anchor instead.".into(),
        ));
    }

    // Verify the creature is actually in this rabble
    let in_rabble = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM creature_state
            WHERE creature_id = $1 AND rabble_id = $2
            AND state IN ('hosting', 'in_rabble')
        )",
    )
    .bind(req.creature_id)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !in_rabble {
        return Err((
            StatusCode::NOT_FOUND,
            "This creature is not in this rabble".into(),
        ));
    }

    // Get creature info for messages
    let creature_row =
        sqlx::query("SELECT specimen_name, owner_id FROM creatures WHERE creature_id = $1")
            .bind(req.creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Creature not found".into()))?;

    let creature_name: String = creature_row
        .try_get("specimen_name")
        .unwrap_or_else(|_| "A creature".into());
    let ejected_user_id: String = creature_row.get("owner_id");

    // Clear swarm_id on the creature's active flight
    sqlx::query(
        "UPDATE creature_flights SET swarm_id = NULL
         WHERE creature_id = $1 AND swarm_id = $2 AND ended_at IS NULL",
    )
    .bind(req.creature_id)
    .bind(swarm_id)
    .execute(pool)
    .await
    .ok();

    // Update creature_state — back to perched, no rabble
    sqlx::query(
        "UPDATE creature_state SET state = 'perched', rabble_id = NULL, updated_at = NOW()
         WHERE creature_id = $1",
    )
    .bind(req.creature_id)
    .execute(pool)
    .await
    .ok();

    // Decrement creature count
    sqlx::query(
        "UPDATE swarm_events SET creature_count = GREATEST(creature_count - 1, 0)
         WHERE swarm_id = $1",
    )
    .bind(swarm_id)
    .execute(pool)
    .await
    .ok();

    // Record the ejection (for cooldown/ban enforcement)
    let cooldown_until = if req.permanent {
        None
    } else {
        Some(chrono::Utc::now() + chrono::Duration::hours(24))
    };

    sqlx::query(
        "INSERT INTO rabble_ejections
         (swarm_id, ejected_creature_id, ejected_user_id, ejected_by_user, reason, permanent, cooldown_until)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(swarm_id)
    .bind(req.creature_id)
    .bind(&ejected_user_id)
    .bind(&user_id)
    .bind(&req.reason)
    .bind(req.permanent)
    .bind(cooldown_until)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Post system message in chat
    let msg = if req.permanent {
        format!("{} has been removed from the rabble", creature_name)
    } else {
        format!(
            "{} has been removed from the rabble (24h cooldown)",
            creature_name
        )
    };

    let _ = sqlx::query(
        "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, content, message_type, created_at)
         VALUES ($1, $2, 'system', $3, 'system', NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(swarm_id)
    .bind(&msg)
    .execute(pool)
    .await;

    // Notify the ejected user
    crate::handlers::push::notify_user(
        pool,
        &ejected_user_id,
        "rabble_eject",
        &format!("{} was removed from {}", creature_name, swarm_name),
        Some(if req.permanent {
            "Your creature has been permanently removed from this rabble."
        } else {
            "Your creature was removed. You can rejoin after 24 hours."
        }),
        Some(&json!({
            "swarm_id": swarm_id,
            "creature_id": req.creature_id,
            "permanent": req.permanent,
        })),
        None,
    )
    .await;

    // Broadcast event so chat UI updates
    let _ = state.rabble_broadcast.send(crate::RabbleEvent {
        swarm_id,
        message: json!({
            "type": "creature_ejected",
            "swarm_id": swarm_id,
            "creature_id": req.creature_id,
            "creature_name": creature_name,
            "permanent": req.permanent,
        }),
    });

    // SSE event so creature card updates
    crate::handlers::streams::emit_creature_event(
        &state,
        req.creature_id,
        "left_rabble",
        json!({
            "swarm_id": swarm_id,
            "creature_id": req.creature_id,
            "state": "perched",
            "reason": "ejected",
        }),
    );

    Ok(Json(json!({
        "status": "ejected",
        "swarm_id": swarm_id,
        "creature_id": req.creature_id,
        "creature_name": creature_name,
        "permanent": req.permanent,
        "cooldown_until": cooldown_until.map(|t| t.to_rfc3339()),
    })))
}

/// DELETE /api/rabble/:id/eject/:creature_id — lift ejection/ban (host only).
pub async fn lift_ejection_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((swarm_id, creature_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify host
    let is_host = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM swarm_events s
            LEFT JOIN creatures c ON c.creature_id = s.anchor_creature_id
            WHERE s.swarm_id = $1
              AND (s.creator_id = $2 OR c.owner_id = $2)
        )",
    )
    .bind(swarm_id)
    .bind(&user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !is_host {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the rabble host can lift bans".into(),
        ));
    }

    sqlx::query("DELETE FROM rabble_ejections WHERE swarm_id = $1 AND ejected_creature_id = $2")
        .bind(swarm_id)
        .bind(creature_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "ejection_lifted",
        "swarm_id": swarm_id,
        "creature_id": creature_id,
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// REPORT — Flag content/behavior for review
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct ReportRequest {
    pub report_type: String, // 'creature', 'message', 'user', 'rabble'
    pub target_id: String,   // UUID as string (polymorphic)
    pub reason: String, // 'inappropriate_content', 'harassment', 'spam', 'impersonation', 'other'
    pub description: Option<String>,
}

/// POST /api/reports — file a report.
///
/// Captures a snapshot of the reported content. Always returns 200 OK
/// to avoid revealing whether action will be taken.
pub async fn create_report_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ReportRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Validate report_type
    let valid_types = ["creature", "message", "user", "rabble"];
    if !valid_types.contains(&req.report_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid report_type. Must be one of: {:?}", valid_types),
        ));
    }

    // Validate reason
    let valid_reasons = [
        "inappropriate_content",
        "harassment",
        "spam",
        "impersonation",
        "other",
    ];
    if !valid_reasons.contains(&req.reason.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid reason. Must be one of: {:?}", valid_reasons),
        ));
    }

    // Capture context snapshot based on report type
    let context = match req.report_type.as_str() {
        "message" => {
            if let Ok(target_uuid) = req.target_id.parse::<Uuid>() {
                sqlx::query(
                    "SELECT content, sender_id, creature_id, creature_name, swarm_id, created_at
                     FROM rabble_messages WHERE message_id = $1",
                )
                .bind(target_uuid)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .map(|r| {
                    json!({
                        "content": r.try_get::<Option<String>, _>("content").unwrap_or(None),
                        "sender_id": r.try_get::<Option<String>, _>("sender_id").unwrap_or(None),
                        "creature_id": r.try_get::<Option<Uuid>, _>("creature_id").ok().flatten(),
                        "creature_name": r.try_get::<Option<String>, _>("creature_name").unwrap_or(None),
                        "swarm_id": r.try_get::<Option<Uuid>, _>("swarm_id").ok(),
                        "created_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at").ok(),
                    })
                })
                .unwrap_or_else(|| json!({}))
            } else {
                json!({})
            }
        }
        "creature" => {
            if let Ok(target_uuid) = req.target_id.parse::<Uuid>() {
                sqlx::query(
                    "SELECT specimen_name, scientific_name, species_group, owner_id, asset_path
                     FROM creatures WHERE creature_id = $1",
                )
                .bind(target_uuid)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .map(|r| {
                    json!({
                        "specimen_name": r.try_get::<Option<String>, _>("specimen_name").unwrap_or(None),
                        "scientific_name": r.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
                        "species_group": r.try_get::<Option<String>, _>("species_group").unwrap_or(None),
                        "owner_id": r.try_get::<Option<String>, _>("owner_id").unwrap_or(None),
                        "asset_path": r.try_get::<Option<String>, _>("asset_path").unwrap_or(None),
                    })
                })
                .unwrap_or_else(|| json!({}))
            } else {
                json!({})
            }
        }
        "user" => {
            sqlx::query(
                "SELECT display_name, email, github_username, avatar_url
                 FROM users WHERE user_id = $1",
            )
            .bind(&req.target_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .map(|r| {
                json!({
                    "display_name": r.try_get::<Option<String>, _>("display_name").unwrap_or(None),
                    "email": r.try_get::<Option<String>, _>("email").unwrap_or(None),
                    "github_username": r.try_get::<Option<String>, _>("github_username").unwrap_or(None),
                })
            })
            .unwrap_or_else(|| json!({}))
        }
        "rabble" => {
            if let Ok(target_uuid) = req.target_id.parse::<Uuid>() {
                sqlx::query(
                    "SELECT name, description, creator_id, location_name, status
                     FROM swarm_events WHERE swarm_id = $1",
                )
                .bind(target_uuid)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .map(|r| {
                    json!({
                        "name": r.try_get::<Option<String>, _>("name").unwrap_or(None),
                        "description": r.try_get::<Option<String>, _>("description").unwrap_or(None),
                        "creator_id": r.try_get::<Option<String>, _>("creator_id").unwrap_or(None),
                        "location_name": r.try_get::<Option<String>, _>("location_name").unwrap_or(None),
                        "status": r.try_get::<Option<String>, _>("status").unwrap_or(None),
                    })
                })
                .unwrap_or_else(|| json!({}))
            } else {
                json!({})
            }
        }
        _ => json!({}),
    };

    let report_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO reports (id, reporter_user_id, report_type, target_id, target_type, reason, description, context)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(report_id)
    .bind(&user_id)
    .bind(&req.report_type)
    .bind(&req.target_id)
    .bind(&req.report_type)
    .bind(&req.reason)
    .bind(&req.description)
    .bind(&context)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Always return success — don't reveal whether action will be taken
    Ok(Json(json!({
        "status": "received",
        "report_id": report_id,
        "message": "Thank you for your report. We'll review it.",
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOCK CHECK HELPER — used by other handlers
// ═══════════════════════════════════════════════════════════════════════════

/// Check if any block exists between two creatures (either direction, either level).
/// Used by friendship, invite, and chat handlers.
pub async fn is_blocked(pool: &sqlx::PgPool, creature_a: Uuid, creature_b: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT is_blocked($1, $2)")
        .bind(creature_a)
        .bind(creature_b)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

/// Check if user_a has blocked user_b (or vice versa) at the user level.
pub async fn is_user_blocked(pool: &sqlx::PgPool, user_a: &str, user_b: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT is_user_blocked($1, $2)")
        .bind(user_a)
        .bind(user_b)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

/// Check if a creature is ejected from a rabble (cooldown or permanent ban).
pub async fn is_ejected(pool: &sqlx::PgPool, swarm_id: Uuid, creature_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT is_ejected($1, $2)")
        .bind(swarm_id)
        .bind(creature_id)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

// ═══════════════════════════════════════════════════════════════════════════
// EJECTION CHECK — added to join_swarm flow
// ═══════════════════════════════════════════════════════════════════════════

/// Check ejection status and return appropriate error if ejected.
/// Call this from join_swarm_handler before allowing a creature to join.
pub async fn check_ejection(
    pool: &sqlx::PgPool,
    swarm_id: Uuid,
    creature_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT permanent, cooldown_until FROM rabble_ejections
         WHERE swarm_id = $1 AND ejected_creature_id = $2
         ORDER BY ejected_at DESC LIMIT 1",
    )
    .bind(swarm_id)
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = row {
        let permanent: bool = row.get("permanent");
        let cooldown: Option<chrono::DateTime<chrono::Utc>> = row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("cooldown_until")
            .unwrap_or(None);

        if permanent {
            return Err((
                StatusCode::FORBIDDEN,
                "This creature has been permanently removed from this rabble.".into(),
            ));
        }

        if let Some(until) = cooldown {
            if chrono::Utc::now() < until {
                let remaining = until - chrono::Utc::now();
                let hours = remaining.num_hours();
                let mins = remaining.num_minutes() % 60;
                return Err((
                    StatusCode::FORBIDDEN,
                    format!(
                        "This creature was removed from this rabble. You can rejoin in {}h {}m.",
                        hours, mins
                    ),
                ));
            }
        }
    }

    Ok(())
}
