//! Social layer handlers — contacts, creature friendships, creature invites,
//! rabble recap, social visibility, and activity feed.
//!
//! Contacts are user-to-user (asymmetric follow model, Layer 1).
//! Friendships are creature-to-creature (symmetric, Layer 2).
//! Creature invites are "come fly with me" (creature-to-creature, Layer 2).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures_core::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::convert::Infallible;
use uuid::Uuid;

use super::super::AppState;
use fermi_auth::AuthPrincipal;

// ═══════════════════════════════════════════════════════════════════════════
// CONTACTS (existing — asymmetric user-to-user follow)
// ═══════════════════════════════════════════════════════════════════════════

/// GET /api/contacts — list my contacts with profile info
pub async fn list_contacts_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let rows = sqlx::query(
        "SELECT c.id, c.contact_id, c.nickname, c.created_at,
         u.display_name, u.avatar_url, u.bio
         FROM contacts c
         LEFT JOIN users u ON u.user_id = c.contact_id
         WHERE c.user_id = $1
         ORDER BY c.created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let contacts: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "contact_id": row.get::<String, _>("contact_id"),
                "nickname": row.get::<Option<String>, _>("nickname"),
                "display_name": row.get::<Option<String>, _>("display_name"),
                "avatar_url": row.get::<Option<String>, _>("avatar_url"),
                "bio": row.get::<Option<String>, _>("bio"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "contacts": contacts })))
}

#[derive(Deserialize)]
pub struct AddContactRequest {
    pub contact_id: String,
    pub nickname: Option<String>,
}

/// POST /api/contacts — add a contact
pub async fn add_contact_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<AddContactRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    if user_id == req.contact_id {
        return Err((StatusCode::BAD_REQUEST, "Cannot add yourself".to_string()));
    }

    // Check contact exists as a user
    let exists = sqlx::query("SELECT 1 FROM users WHERE user_id = $1")
        .bind(&req.contact_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if exists.is_none() {
        return Err((StatusCode::NOT_FOUND, "User not found".to_string()));
    }

    let id = Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO contacts (id, user_id, contact_id, nickname)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, contact_id) DO NOTHING",
    )
    .bind(id)
    .bind(&user_id)
    .bind(&req.contact_id)
    .bind(&req.nickname)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::CONFLICT, "Contact already added".to_string()));
    }

    Ok(Json(json!({
        "id": id,
        "contact_id": req.contact_id,
        "added": true,
    })))
}

/// DELETE /api/contacts/:id — remove a contact by row id or contact_id
pub async fn remove_contact_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id_or_contact): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Support both row UUID and contact user_id for backwards compatibility
    let result = if let Ok(row_uuid) = id_or_contact.parse::<Uuid>() {
        sqlx::query("DELETE FROM contacts WHERE id = $1 AND user_id = $2")
            .bind(row_uuid)
            .bind(&user_id)
            .execute(pool)
            .await
    } else {
        sqlx::query("DELETE FROM contacts WHERE user_id = $1 AND contact_id = $2")
            .bind(&user_id)
            .bind(&id_or_contact)
            .execute(pool)
            .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Contact not found".to_string()));
    }

    Ok(Json(json!({ "removed": true })))
}

#[derive(Deserialize)]
pub struct UpdateContactRequest {
    pub nickname: Option<String>,
}

/// PUT /api/contacts/:id — update nickname by row id or contact_id
pub async fn update_contact_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id_or_contact): Path<String>,
    Json(req): Json<UpdateContactRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = if let Ok(row_uuid) = id_or_contact.parse::<Uuid>() {
        sqlx::query("UPDATE contacts SET nickname = $1 WHERE id = $2 AND user_id = $3")
            .bind(&req.nickname)
            .bind(row_uuid)
            .bind(&user_id)
            .execute(pool)
            .await
    } else {
        sqlx::query("UPDATE contacts SET nickname = $1 WHERE user_id = $2 AND contact_id = $3")
            .bind(&req.nickname)
            .bind(&user_id)
            .bind(&id_or_contact)
            .execute(pool)
            .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Contact not found".to_string()));
    }

    Ok(Json(json!({ "updated": true })))
}

// ═══════════════════════════════════════════════════════════════════════════
// CREATURE FRIENDSHIPS (symmetric, creature-to-creature)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct BefriendRequest {
    /// The creature sending the friend request (must be owned by caller)
    pub from_creature_id: Uuid,
    /// The creature to befriend
    pub to_creature_id: Uuid,
    /// Rabble where they met (optional, for context)
    pub met_in_rabble: Option<Uuid>,
}

/// POST /api/creature-friendships — send a friendship request.
/// Creature A befriends Creature B. Caller must own Creature A.
pub async fn send_friendship_request_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<BefriendRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    if req.from_creature_id == req.to_creature_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "A creature cannot befriend itself".into(),
        ));
    }

    // Verify caller owns the from_creature
    let from_creature = sqlx::query(
        "SELECT owner_id, specimen_name, species_group FROM creatures WHERE creature_id = $1",
    )
    .bind(req.from_creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "From creature not found".into()))?;

    let owner: String = from_creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "You don't own this creature".into()));
    }

    // Verify target creature exists
    let to_creature =
        sqlx::query("SELECT owner_id, specimen_name FROM creatures WHERE creature_id = $1")
            .bind(req.to_creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Target creature not found".into()))?;

    let to_owner: String = to_creature.get("owner_id");

    // Don't allow befriending your own creatures
    if to_owner == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "Cannot befriend your own creature".into(),
        ));
    }

    // Canonical ordering: creature_a < creature_b
    let (creature_a, creature_b) = if req.from_creature_id < req.to_creature_id {
        (req.from_creature_id, req.to_creature_id)
    } else {
        (req.to_creature_id, req.from_creature_id)
    };

    // Check for existing friendship
    let existing = sqlx::query(
        "SELECT id, status FROM creature_friendships
         WHERE creature_a = $1 AND creature_b = $2",
    )
    .bind(creature_a)
    .bind(creature_b)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = existing {
        let status: String = row.get("status");
        let id: Uuid = row.get("id");
        return match status.as_str() {
            "accepted" => Ok(Json(json!({
                "status": "already_friends",
                "friendship_id": id,
            }))),
            "pending" => Ok(Json(json!({
                "status": "already_pending",
                "friendship_id": id,
            }))),
            "blocked" => Err((StatusCode::FORBIDDEN, "This friendship is blocked".into())),
            "declined" => {
                // Allow re-requesting after decline
                sqlx::query(
                    "UPDATE creature_friendships
                     SET status = 'pending', initiated_by = $1, updated_at = NOW()
                     WHERE id = $2",
                )
                .bind(req.from_creature_id)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                Ok(Json(json!({
                    "status": "re_requested",
                    "friendship_id": id,
                })))
            }
            _ => Err((StatusCode::INTERNAL_SERVER_ERROR, "Unknown status".into())),
        };
    }

    // Create new friendship request
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO creature_friendships (id, creature_a, creature_b, initiated_by, status, met_in_rabble, met_at)
         VALUES ($1, $2, $3, $4, 'pending', $5, NOW())",
    )
    .bind(id)
    .bind(creature_a)
    .bind(creature_b)
    .bind(req.from_creature_id)
    .bind(req.met_in_rabble)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Write activity event
    let from_name: String = from_creature.get("specimen_name");
    let to_name: String = to_creature.get("specimen_name");
    emit_activity_event(
        pool,
        &user_id,
        Some(req.from_creature_id),
        "friendship_requested",
        req.met_in_rabble,
        Some(req.to_creature_id),
        &format!("{} wants to befriend {}", from_name, to_name),
        None,
        None,
    )
    .await;

    // Create notification for target creature's owner
    let _ = sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, message, created_at)
         VALUES ($1, $2, 'friendship_request', $3, $4, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(&to_owner)
    .bind(format!("{} wants to be friends!", from_name))
    .bind(format!(
        "{} met your creature {} and wants to befriend it",
        from_name, to_name
    ))
    .execute(pool)
    .await;

    Ok(Json(json!({
        "status": "requested",
        "friendship_id": id,
        "creature_a": creature_a,
        "creature_b": creature_b,
    })))
}

/// POST /api/creature-friendships/:id/accept — accept a pending friendship.
/// Caller must own the creature that DIDN'T initiate.
pub async fn accept_friendship_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(friendship_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let row = sqlx::query(
        "SELECT cf.id, cf.creature_a, cf.creature_b, cf.initiated_by, cf.status,
                ca.owner_id AS owner_a, cb.owner_id AS owner_b,
                ca.specimen_name AS name_a, cb.specimen_name AS name_b
         FROM creature_friendships cf
         JOIN creatures ca ON ca.creature_id = cf.creature_a
         JOIN creatures cb ON cb.creature_id = cf.creature_b
         WHERE cf.id = $1",
    )
    .bind(friendship_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Friendship not found".into()))?;

    let status: String = row.get("status");
    if status != "pending" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Friendship is {}, not pending", status),
        ));
    }

    let initiated_by: Uuid = row.get("initiated_by");
    let creature_a: Uuid = row.get("creature_a");
    let creature_b: Uuid = row.get("creature_b");
    let owner_a: String = row.get("owner_a");
    let owner_b: String = row.get("owner_b");

    // The accepting user must own the creature that didn't initiate
    let accepting_creature = if initiated_by == creature_a {
        // Creature A initiated → user must own creature B
        if owner_b != user_id {
            return Err((
                StatusCode::FORBIDDEN,
                "You don't own the receiving creature".into(),
            ));
        }
        creature_b
    } else {
        // Creature B initiated → user must own creature A
        if owner_a != user_id {
            return Err((
                StatusCode::FORBIDDEN,
                "You don't own the receiving creature".into(),
            ));
        }
        creature_a
    };

    sqlx::query(
        "UPDATE creature_friendships SET status = 'accepted', updated_at = NOW() WHERE id = $1",
    )
    .bind(friendship_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Emit activity event
    let name_a: String = row.get("name_a");
    let name_b: String = row.get("name_b");
    emit_activity_event(
        pool,
        &user_id,
        Some(accepting_creature),
        "friendship_accepted",
        None,
        Some(initiated_by),
        &format!("{} and {} are now friends!", name_a, name_b),
        None,
        None,
    )
    .await;

    // Notify the initiator's owner
    let initiator_owner = if initiated_by == creature_a {
        &owner_a
    } else {
        &owner_b
    };
    let _ = sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, message, created_at)
         VALUES ($1, $2, 'friendship_accepted', $3, $4, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(initiator_owner)
    .bind(format!("{} and {} are now friends!", name_a, name_b))
    .bind("Your friendship request was accepted")
    .execute(pool)
    .await;

    // Broadcast creature SSE events — both creatures get notified
    crate::handlers::streams::emit_creature_event(
        &state,
        creature_a,
        "friend_accepted",
        json!({
            "friendship_id": friendship_id,
            "friend_creature_id": creature_b,
            "friend_name": name_b,
        }),
    );
    crate::handlers::streams::emit_creature_event(
        &state,
        creature_b,
        "friend_accepted",
        json!({
            "friendship_id": friendship_id,
            "friend_creature_id": creature_a,
            "friend_name": name_a,
        }),
    );

    Ok(Json(json!({
        "status": "accepted",
        "friendship_id": friendship_id,
        "creature_a": creature_a,
        "creature_b": creature_b,
    })))
}

/// POST /api/creature-friendships/:id/decline — decline a pending friendship.
pub async fn decline_friendship_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(friendship_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let row = sqlx::query(
        "SELECT cf.creature_a, cf.creature_b, cf.initiated_by, cf.status,
                ca.owner_id AS owner_a, cb.owner_id AS owner_b
         FROM creature_friendships cf
         JOIN creatures ca ON ca.creature_id = cf.creature_a
         JOIN creatures cb ON cb.creature_id = cf.creature_b
         WHERE cf.id = $1",
    )
    .bind(friendship_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Friendship not found".into()))?;

    let status: String = row.get("status");
    if status != "pending" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Friendship is {}, not pending", status),
        ));
    }

    let initiated_by: Uuid = row.get("initiated_by");
    let creature_a: Uuid = row.get("creature_a");
    let owner_a: String = row.get("owner_a");
    let owner_b: String = row.get("owner_b");

    // The declining user must own the creature that didn't initiate
    let is_receiver = if initiated_by == creature_a {
        owner_b == user_id
    } else {
        owner_a == user_id
    };

    if !is_receiver {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the receiving creature's owner can decline".into(),
        ));
    }

    sqlx::query(
        "UPDATE creature_friendships SET status = 'declined', updated_at = NOW() WHERE id = $1",
    )
    .bind(friendship_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "declined",
        "friendship_id": friendship_id,
    })))
}

/// DELETE /api/creature-friendships/:id — remove an accepted friendship (unfriend).
/// Either side can unfriend.
pub async fn remove_friendship_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(friendship_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = sqlx::query(
        "DELETE FROM creature_friendships
         WHERE id = $1
           AND (
               creature_a IN (SELECT creature_id FROM creatures WHERE owner_id = $2)
               OR creature_b IN (SELECT creature_id FROM creatures WHERE owner_id = $2)
           )",
    )
    .bind(friendship_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Friendship not found".into()));
    }

    Ok(Json(json!({ "removed": true })))
}

/// GET /api/creatures/:id/friends — list a creature's accepted friends.
pub async fn list_creature_friends_handler(
    State(state): State<AppState>,
    Path(creature_id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let limit = q.limit.unwrap_or(50).min(200) as i32;
    let offset = q.offset.unwrap_or(0) as i32;

    let rows = sqlx::query("SELECT * FROM get_creature_friends($1, $2, $3)")
        .bind(creature_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let friends: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "friendship_id": row.get::<Uuid, _>("friendship_id"),
                "creature_id": row.get::<Uuid, _>("friend_creature_id"),
                "specimen_name": row.get::<Option<String>, _>("friend_name"),
                "species_group": row.get::<Option<String>, _>("friend_species_group"),
                "asset_path": row.get::<Option<String>, _>("friend_asset_path"),
                "owner_id": row.get::<Option<String>, _>("friend_owner_id"),
                "owner_display_name": row.get::<Option<String>, _>("friend_owner_name"),
                "social_visibility": row.get::<Option<String>, _>("friend_social_visibility"),
                "met_in_rabble": row.get::<Option<Uuid>, _>("met_in_rabble"),
                "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                "friends_since": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("friends_since")
                    .map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "creature_id": creature_id,
        "friends": friends,
        "count": friends.len(),
    })))
}

/// GET /api/creature-friendships/pending — list pending friendship requests for my creatures.
pub async fn pending_friendships_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let rows = sqlx::query("SELECT * FROM get_pending_friendship_requests($1)")
        .bind(&user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let requests: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "friendship_id": row.get::<Uuid, _>("friendship_id"),
                "from_creature_id": row.get::<Uuid, _>("from_creature_id"),
                "from_creature_name": row.get::<Option<String>, _>("from_creature_name"),
                "from_species_group": row.get::<Option<String>, _>("from_species_group"),
                "from_asset_path": row.get::<Option<String>, _>("from_asset_path"),
                "from_owner_id": row.get::<Option<String>, _>("from_owner_id"),
                "from_owner_name": row.get::<Option<String>, _>("from_owner_name"),
                "to_creature_id": row.get::<Uuid, _>("to_creature_id"),
                "to_creature_name": row.get::<Option<String>, _>("to_creature_name"),
                "met_in_rabble": row.get::<Option<Uuid>, _>("met_in_rabble"),
                "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "pending_requests": requests,
        "count": requests.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// CREATURE INVITES ("come fly with me" — creature-to-creature, Layer 2)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct CreatureInviteRequest {
    /// The inviting creature (must be in a rabble, owned by caller)
    pub from_creature_id: Uuid,
    /// The creature being invited
    pub to_creature_id: Uuid,
    /// Optional message
    pub message: Option<String>,
}

/// POST /api/creature-invites — invite another creature to your rabble.
/// "Come fly with me." The from_creature must be actively in a rabble.
pub async fn send_creature_invite_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreatureInviteRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    if req.from_creature_id == req.to_creature_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "A creature cannot invite itself".into(),
        ));
    }

    // Verify caller owns the from_creature and it's in a rabble
    let from_row = sqlx::query(
        "SELECT c.owner_id, c.specimen_name, c.species_group,
                cs.rabble_id, cs.state
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         WHERE c.creature_id = $1",
    )
    .bind(req.from_creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "From creature not found".into()))?;

    let owner: String = from_row.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "You don't own this creature".into()));
    }

    let rabble_id: Option<Uuid> = from_row.get("rabble_id");
    let rabble_id = rabble_id.ok_or((
        StatusCode::BAD_REQUEST,
        "Creature must be in a rabble to send an invite".into(),
    ))?;

    // Verify target creature exists and is not owned by caller
    let to_row =
        sqlx::query("SELECT owner_id, specimen_name FROM creatures WHERE creature_id = $1")
            .bind(req.to_creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Target creature not found".into()))?;

    let to_owner: String = to_row.get("owner_id");

    // Get rabble info
    let rabble_row = sqlx::query("SELECT name FROM swarm_events WHERE swarm_id = $1")
        .bind(rabble_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rabble_name: String = rabble_row
        .as_ref()
        .and_then(|r| r.try_get("name").ok())
        .unwrap_or_else(|| "Unknown Rabble".to_string());

    // Expire old invites first
    let _ = sqlx::query(
        "UPDATE creature_invites SET status = 'expired'
         WHERE status = 'pending' AND expires_at < NOW()",
    )
    .execute(pool)
    .await;

    // Check for existing pending invite
    let existing = sqlx::query(
        "SELECT id FROM creature_invites
         WHERE from_creature_id = $1 AND to_creature_id = $2
           AND rabble_id = $3 AND status = 'pending'",
    )
    .bind(req.from_creature_id)
    .bind(req.to_creature_id)
    .bind(rabble_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_some() {
        return Ok(Json(json!({ "status": "already_invited" })));
    }

    // Create the invite
    let invite_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO creature_invites (id, from_creature_id, to_creature_id, rabble_id, message)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(invite_id)
    .bind(req.from_creature_id)
    .bind(req.to_creature_id)
    .bind(rabble_id)
    .bind(&req.message)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let from_name: String = from_row.get("specimen_name");
    let to_name: String = to_row.get("specimen_name");

    // Emit activity event
    emit_activity_event(
        pool,
        &user_id,
        Some(req.from_creature_id),
        "creature_invited",
        Some(rabble_id),
        Some(req.to_creature_id),
        &format!("{} invited {} to {}", from_name, to_name, rabble_name),
        req.message.as_deref(),
        None,
    )
    .await;

    // Notify target creature's owner
    let _ = sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, message, created_at)
         VALUES ($1, $2, 'creature_invite', $3, $4, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(&to_owner)
    .bind(format!("{} says: come fly with me!", from_name))
    .bind(format!(
        "{} invites {} to join {} — {}",
        from_name,
        to_name,
        rabble_name,
        req.message.as_deref().unwrap_or("No message")
    ))
    .execute(pool)
    .await;

    // Also grant rabble visibility via object_shares so the target can see it
    let _ = sqlx::query(
        "INSERT INTO object_shares (id, object_type, object_id, share_type, share_target, permission, granted_by, created_at)
         VALUES ($1, 'rabble', $2, 'user', $3, 'edit', $4, NOW())
         ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(rabble_id.to_string())
    .bind(&to_owner)
    .bind(&user_id)
    .execute(pool)
    .await;

    Ok(Json(json!({
        "status": "invited",
        "invite_id": invite_id,
        "rabble_id": rabble_id,
        "rabble_name": rabble_name,
    })))
}

/// POST /api/creature-invites/:id/accept — accept a creature invite (join the rabble).
pub async fn accept_creature_invite_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let row = sqlx::query(
        "SELECT ci.id, ci.from_creature_id, ci.to_creature_id, ci.rabble_id, ci.status,
                ci.expires_at,
                c_to.owner_id AS to_owner,
                c_to.specimen_name AS to_name,
                c_from.specimen_name AS from_name,
                s.name AS rabble_name
         FROM creature_invites ci
         JOIN creatures c_to ON c_to.creature_id = ci.to_creature_id
         JOIN creatures c_from ON c_from.creature_id = ci.from_creature_id
         LEFT JOIN swarm_events s ON s.swarm_id = ci.rabble_id
         WHERE ci.id = $1",
    )
    .bind(invite_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Invite not found".into()))?;

    let to_owner: String = row.get("to_owner");
    if to_owner != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            "You don't own the invited creature".into(),
        ));
    }

    let status: String = row.get("status");
    if status != "pending" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invite is {}, not pending", status),
        ));
    }

    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
    if expires_at < chrono::Utc::now() {
        let _ = sqlx::query("UPDATE creature_invites SET status = 'expired' WHERE id = $1")
            .bind(invite_id)
            .execute(pool)
            .await;
        return Err((StatusCode::GONE, "Invite has expired".into()));
    }

    sqlx::query(
        "UPDATE creature_invites SET status = 'accepted', responded_at = NOW() WHERE id = $1",
    )
    .bind(invite_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rabble_id: Uuid = row.get("rabble_id");
    let to_creature_id: Uuid = row.get("to_creature_id");
    let to_name: String = row.get("to_name");
    let from_name: String = row.get("from_name");
    let rabble_name: String = row.try_get("rabble_name").unwrap_or_default();

    // Emit activity event
    emit_activity_event(
        pool,
        &user_id,
        Some(to_creature_id),
        "creature_invite_accepted",
        Some(rabble_id),
        Some(row.get("from_creature_id")),
        &format!(
            "{} accepted {}'s invite to {}",
            to_name, from_name, rabble_name
        ),
        None,
        None,
    )
    .await;

    // The actual join is handled by the client calling POST /api/swarms/:id/join
    // with the creature_id. This handler just marks the invite as accepted.
    Ok(Json(json!({
        "status": "accepted",
        "invite_id": invite_id,
        "rabble_id": rabble_id,
        "creature_id": to_creature_id,
        "message": "Now call POST /api/swarms/:rabble_id/join with your creature to complete the join",
    })))
}

/// POST /api/creature-invites/:id/decline — decline a creature invite.
pub async fn decline_creature_invite_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = sqlx::query(
        "UPDATE creature_invites ci SET status = 'declined', responded_at = NOW()
         FROM creatures c
         WHERE ci.id = $1
           AND ci.to_creature_id = c.creature_id
           AND c.owner_id = $2
           AND ci.status = 'pending'",
    )
    .bind(invite_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Invite not found or already responded".into(),
        ));
    }

    Ok(Json(
        json!({ "status": "declined", "invite_id": invite_id }),
    ))
}

/// GET /api/creature-invites/pending — list pending creature invites for my creatures.
pub async fn list_pending_creature_invites_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Expire old invites first
    let _ = sqlx::query(
        "UPDATE creature_invites SET status = 'expired'
         WHERE status = 'pending' AND expires_at < NOW()",
    )
    .execute(pool)
    .await;

    let rows = sqlx::query("SELECT * FROM get_pending_creature_invites($1)")
        .bind(&user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let invites: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "invite_id": row.get::<Uuid, _>("invite_id"),
                "from_creature_id": row.get::<Uuid, _>("from_creature_id"),
                "from_creature_name": row.get::<Option<String>, _>("from_creature_name"),
                "from_species_group": row.get::<Option<String>, _>("from_species_group"),
                "from_asset_path": row.get::<Option<String>, _>("from_asset_path"),
                "from_owner_name": row.get::<Option<String>, _>("from_owner_name"),
                "to_creature_id": row.get::<Uuid, _>("to_creature_id"),
                "to_creature_name": row.get::<Option<String>, _>("to_creature_name"),
                "rabble_id": row.get::<Uuid, _>("rabble_id"),
                "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                "message": row.get::<Option<String>, _>("message"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "expires_at": row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "invites": invites,
        "count": invites.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// RABBLE RECAP ("You met these creatures")
// ═══════════════════════════════════════════════════════════════════════════

/// GET /api/rabble/:id/recap/:creature_id — "You met these creatures" post-rabble screen.
/// Shows all creatures that were co-present in the rabble with friendship status.
pub async fn rabble_recap_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((swarm_id, creature_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify caller owns the creature
    let creature =
        sqlx::query("SELECT owner_id, specimen_name FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Creature not found".into()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "You don't own this creature".into()));
    }

    // Get rabble info
    let rabble = sqlx::query(
        "SELECT name, status, starts_at, ends_at, creature_count
         FROM swarm_events WHERE swarm_id = $1",
    )
    .bind(swarm_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Rabble not found".into()))?;

    let rabble_name: String = rabble.try_get("name").unwrap_or_default();
    let rabble_status: String = rabble.try_get("status").unwrap_or_default();

    // Get co-present creatures via the helper function
    let rows = sqlx::query("SELECT * FROM get_creatures_met_in_rabble($1, $2)")
        .bind(swarm_id)
        .bind(creature_id)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let met_creatures: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "creature_id": row.get::<Uuid, _>("creature_id"),
                "specimen_name": row.get::<Option<String>, _>("specimen_name"),
                "scientific_name": row.get::<Option<String>, _>("scientific_name"),
                "species_group": row.get::<Option<String>, _>("species_group"),
                "asset_path": row.get::<Option<String>, _>("asset_path"),
                "owner_id": row.get::<Option<String>, _>("owner_id"),
                "owner_display_name": row.get::<Option<String>, _>("owner_display_name"),
                "owner_social_visibility": row.get::<Option<String>, _>("owner_social_visibility"),
                "overlap_seconds": row.get::<Option<i32>, _>("overlap_seconds"),
                "already_friends": row.get::<Option<bool>, _>("already_friends"),
                "friendship_status": row.get::<Option<String>, _>("friendship_status"),
            })
        })
        .collect();

    Ok(Json(json!({
        "rabble_id": swarm_id,
        "rabble_name": rabble_name,
        "rabble_status": rabble_status,
        "your_creature": creature_id,
        "creatures_met": met_creatures,
        "count": met_creatures.len(),
    })))
}

/// POST /api/rabble/:id/co-presence — record creature co-presence (called on join).
/// Internal: called by join_swarm_handler to track who was present.
pub async fn record_co_presence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<Uuid>,
    Json(body): Json<CoPresenceRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify caller owns the creature
    let creature = sqlx::query("SELECT owner_id FROM creatures WHERE creature_id = $1")
        .bind(body.creature_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Creature not found".into()))?;

    let owner: String = creature.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "You don't own this creature".into()));
    }

    sqlx::query(
        "INSERT INTO rabble_co_presence (id, rabble_id, creature_id, owner_id, joined_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (rabble_id, creature_id) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(swarm_id)
    .bind(body.creature_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "recorded": true })))
}

#[derive(Deserialize)]
pub struct CoPresenceRequest {
    pub creature_id: Uuid,
}

// ═══════════════════════════════════════════════════════════════════════════
// SOCIAL VISIBILITY
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct UpdateSocialVisibilityRequest {
    pub social_visibility: String,
}

/// PUT /api/users/social-visibility — update user social visibility preference.
pub async fn update_social_visibility_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<UpdateSocialVisibilityRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let valid = ["public", "creature-only", "private"];
    if !valid.contains(&req.social_visibility.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid visibility: {}. Must be one of: {}",
                req.social_visibility,
                valid.join(", ")
            ),
        ));
    }

    sqlx::query("UPDATE users SET social_visibility = $1 WHERE user_id = $2")
        .bind(&req.social_visibility)
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "social_visibility": req.social_visibility,
        "updated": true,
    })))
}

// ═══════════════════════════════════════════════════════════════════════════
// ACTIVITY FEED (SSE)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct FeedQuery {
    pub before: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api/feed/events — paginated activity feed with relationship context.
pub async fn activity_feed_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<FeedQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let limit = q.limit.unwrap_or(50).min(200) as i32;

    let before = if let Some(ref before_str) = q.before {
        chrono::DateTime::parse_from_rfc3339(before_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now())
    } else {
        chrono::Utc::now()
    };

    let rows = sqlx::query("SELECT * FROM get_activity_feed($1, $2, $3)")
        .bind(&user_id)
        .bind(before)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let events: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "event_id": row.get::<Uuid, _>("event_id"),
                "event_type": row.get::<String, _>("event_type"),
                "actor_user_id": row.get::<String, _>("actor_user_id"),
                "actor_creature_id": row.get::<Option<Uuid>, _>("actor_creature_id"),
                "actor_creature_name": row.get::<Option<String>, _>("actor_creature_name"),
                "actor_species_group": row.get::<Option<String>, _>("actor_species_group"),
                "rabble_id": row.get::<Option<Uuid>, _>("rabble_id"),
                "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                "target_creature_id": row.get::<Option<Uuid>, _>("target_creature_id"),
                "target_creature_name": row.get::<Option<String>, _>("target_creature_name"),
                "title": row.get::<String, _>("title"),
                "body": row.get::<Option<String>, _>("body"),
                "metadata": row.get::<Option<Value>, _>("metadata"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "is_own_creature": row.get::<Option<bool>, _>("is_own_creature"),
                "is_contact": row.get::<Option<bool>, _>("is_contact"),
                "is_friend_creature": row.get::<Option<bool>, _>("is_friend_creature"),
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events,
        "count": events.len(),
    })))
}

#[derive(Deserialize)]
pub struct FeedStreamQuery {
    pub since: Option<String>,
}

/// GET /api/feed/stream — SSE activity feed stream.
/// Pushes new events as they arrive, with relationship context annotations.
pub async fn activity_feed_stream_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<FeedStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool().clone();

    // Backfill: get recent events since the reconnection timestamp
    let backfill: Vec<Value> = if let Some(ref since_str) = params.since {
        if let Ok(since) = chrono::DateTime::parse_from_rfc3339(since_str) {
            let since_utc = since.with_timezone(&chrono::Utc);
            let rows = sqlx::query("SELECT * FROM get_activity_feed($1, $2, $3)")
                .bind(&user_id)
                .bind(chrono::Utc::now())
                .bind(200i32)
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

            rows.iter()
                .filter(|row| {
                    row.get::<chrono::DateTime<chrono::Utc>, _>("created_at") > since_utc
                })
                .map(|row| {
                    json!({
                        "event_id": row.get::<Uuid, _>("event_id"),
                        "event_type": row.get::<String, _>("event_type"),
                        "actor_creature_name": row.get::<Option<String>, _>("actor_creature_name"),
                        "actor_species_group": row.get::<Option<String>, _>("actor_species_group"),
                        "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                        "title": row.get::<String, _>("title"),
                        "body": row.get::<Option<String>, _>("body"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                        "is_own_creature": row.get::<Option<bool>, _>("is_own_creature"),
                        "is_contact": row.get::<Option<bool>, _>("is_contact"),
                        "is_friend_creature": row.get::<Option<bool>, _>("is_friend_creature"),
                    })
                })
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Poll-based SSE stream: check for new events every 5 seconds
    // (will upgrade to broadcast channel when activity_events volume justifies it)
    let user_id_owned = user_id.to_string();
    let stream = async_stream::stream! {
        // Send backfill events first
        for event_json in backfill {
            let data = serde_json::to_string(&event_json).unwrap_or_default();
            yield Ok(Event::default().data(data));
        }

        let mut last_check = chrono::Utc::now();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.tick().await; // skip immediate tick

        loop {
            interval.tick().await;

            // Poll for new events since last check
            let rows = sqlx::query("SELECT * FROM get_activity_feed($1, $2, $3)")
                .bind(&user_id_owned)
                .bind(chrono::Utc::now())
                .bind(20i32)
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

            for row in &rows {
                let created: chrono::DateTime<chrono::Utc> = row.get("created_at");
                if created > last_check {
                    let event_json = json!({
                        "event_id": row.get::<Uuid, _>("event_id"),
                        "event_type": row.get::<String, _>("event_type"),
                        "actor_creature_name": row.get::<Option<String>, _>("actor_creature_name"),
                        "actor_species_group": row.get::<Option<String>, _>("actor_species_group"),
                        "rabble_name": row.get::<Option<String>, _>("rabble_name"),
                        "title": row.get::<String, _>("title"),
                        "body": row.get::<Option<String>, _>("body"),
                        "created_at": created.to_rfc3339(),
                        "is_own_creature": row.get::<Option<bool>, _>("is_own_creature"),
                        "is_contact": row.get::<Option<bool>, _>("is_contact"),
                        "is_friend_creature": row.get::<Option<bool>, _>("is_friend_creature"),
                    });
                    let data = serde_json::to_string(&event_json).unwrap_or_default();
                    yield Ok(Event::default().data(data));
                }
            }

            last_check = chrono::Utc::now();
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("keepalive"),
    ))
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPER TYPES AND FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Emit an activity event into the activity_events table.
/// Fire-and-forget — errors are logged but don't propagate.
pub(crate) async fn emit_activity_event(
    pool: &sqlx::PgPool,
    actor_user_id: &str,
    actor_creature_id: Option<Uuid>,
    event_type: &str,
    rabble_id: Option<Uuid>,
    target_creature_id: Option<Uuid>,
    title: &str,
    body: Option<&str>,
    metadata: Option<&Value>,
) {
    let default_meta = json!({});
    let meta = metadata.unwrap_or(&default_meta);

    let result = sqlx::query(
        "INSERT INTO activity_events
         (id, actor_user_id, actor_creature_id, event_type, rabble_id, target_creature_id, title, body, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(actor_user_id)
    .bind(actor_creature_id)
    .bind(event_type)
    .bind(rabble_id)
    .bind(target_creature_id)
    .bind(title)
    .bind(body)
    .bind(meta)
    .execute(pool)
    .await;

    if let Err(e) = result {
        eprintln!("Failed to emit activity event '{}': {}", event_type, e);
    }
}

/// Helper: record co-presence when a creature joins a rabble.
/// Called from join_swarm_handler or similar.
pub(crate) async fn record_co_presence(
    pool: &sqlx::PgPool,
    rabble_id: Uuid,
    creature_id: Uuid,
    owner_id: &str,
) {
    let result = sqlx::query(
        "INSERT INTO rabble_co_presence (id, rabble_id, creature_id, owner_id, joined_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (rabble_id, creature_id) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(rabble_id)
    .bind(creature_id)
    .bind(owner_id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        eprintln!(
            "Failed to record co-presence for creature {} in rabble {}: {}",
            creature_id, rabble_id, e
        );
    }
}

/// Helper: update co-presence left_at when a creature leaves a rabble.
pub(crate) async fn update_co_presence_departure(
    pool: &sqlx::PgPool,
    rabble_id: Uuid,
    creature_id: Uuid,
) {
    let _ = sqlx::query(
        "UPDATE rabble_co_presence
         SET left_at = NOW(),
             overlap_seconds = EXTRACT(EPOCH FROM (NOW() - joined_at))::INTEGER
         WHERE rabble_id = $1 AND creature_id = $2 AND left_at IS NULL",
    )
    .bind(rabble_id)
    .bind(creature_id)
    .execute(pool)
    .await;
}
