//! Rabble chat handlers — POST/GET messages + SSE stream for creature-attributed chat.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::convert::Infallible;

use super::rabble_workspace;
use crate::{AppState, RabbleEvent};

// ─── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PostMessageRequest {
    pub creature_id: Option<uuid::Uuid>,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct RabbleMessage {
    pub message_id: uuid::Uuid,
    pub swarm_id: uuid::Uuid,
    pub sender_id: String,
    pub creature_id: Option<uuid::Uuid>,
    pub creature_name: Option<String>,
    pub species_name: Option<String>,
    pub species_group: Option<String>,
    pub content: String,
    pub message_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct MessagesQuery {
    pub limit: Option<i64>,
    pub before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub since: Option<String>,
}

// ─── POST /api/rabble/:id/messages ─────────────────────────────────

pub async fn post_rabble_message(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<uuid::Uuid>,
    Json(body): Json<PostMessageRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    if body.content.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Message content is required".into(),
        ));
    }

    // Verify swarm exists and is active
    let swarm = sqlx::query("SELECT status FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Rabble not found".into()))?;

    let status: String = swarm.try_get("status").unwrap_or_default();
    if status != "active" && status != "scheduled" {
        return Err((StatusCode::CONFLICT, format!("Rabble is {}", status)));
    }

    // Resolve which creature is speaking.
    // If client specifies creature_id, use that (validated); else auto-detect from flights.
    let (creature_id, creature_name, species_name, species_group) = if let Some(cid) =
        body.creature_id
    {
        // Client specified a creature — verify ownership + membership in this rabble
        let row = sqlx::query(
            "SELECT c.specimen_name, c.scientific_name AS species_name, c.species_group
             FROM creature_flights cf
             JOIN creatures c ON c.creature_id = cf.creature_id
             WHERE cf.swarm_id = $1 AND cf.creature_id = $2 AND c.owner_id = $3
             LIMIT 1",
        )
        .bind(swarm_id)
        .bind(cid)
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((
            StatusCode::FORBIDDEN,
            "That creature is not yours or not in this rabble".into(),
        ))?;
        (
            Some(cid),
            row.try_get::<Option<String>, _>("specimen_name")
                .ok()
                .flatten(),
            row.try_get::<Option<String>, _>("species_name")
                .ok()
                .flatten(),
            row.try_get::<Option<String>, _>("species_group")
                .ok()
                .flatten(),
        )
    } else {
        // Auto-detect: most recently joined creature in this rabble
        let has_creature = sqlx::query(
            "SELECT cf.creature_id, c.specimen_name, c.scientific_name AS species_name, c.species_group
             FROM creature_flights cf
             JOIN creatures c ON c.creature_id = cf.creature_id
             WHERE cf.swarm_id = $1 AND c.owner_id = $2 AND cf.ended_at IS NULL
             ORDER BY cf.started_at DESC
             LIMIT 1",
        )
        .bind(swarm_id)
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(row) = &has_creature {
            (
                Some(
                    row.try_get::<uuid::Uuid, _>("creature_id")
                        .ok()
                        .unwrap_or_default(),
                ),
                row.try_get::<Option<String>, _>("specimen_name")
                    .ok()
                    .flatten(),
                row.try_get::<Option<String>, _>("species_name")
                    .ok()
                    .flatten(),
                row.try_get::<Option<String>, _>("species_group")
                    .ok()
                    .flatten(),
            )
        } else {
            // Allow swarm creator to chat without a creature in flight
            let is_creator =
                sqlx::query("SELECT 1 FROM swarm_events WHERE swarm_id = $1 AND creator_id = $2")
                    .bind(swarm_id)
                    .bind(&user_id)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if is_creator.is_none() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "You have no creature in this rabble".into(),
                ));
            }
            (None, None, None, None)
        }
    };

    // Fetch sender display name
    let sender_display_name: Option<String> =
        sqlx::query("SELECT display_name FROM users WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|r| {
                r.try_get::<Option<String>, _>("display_name")
                    .ok()
                    .flatten()
            });

    // Charge gas
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.rabble_chat,
        "rabble_chat",
        &format!("Chat in rabble {}", swarm_id),
        Some(&swarm_id.to_string()),
    )
    .await?;

    // Insert message
    let message_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, creature_id, creature_name, species_name, species_group, content, message_type, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'chat', $9)"
    )
    .bind(message_id)
    .bind(swarm_id)
    .bind(&user_id)
    .bind(creature_id)
    .bind(&creature_name)
    .bind(&species_name)
    .bind(&species_group)
    .bind(&body.content)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let msg_json = json!({
        "message_id": message_id,
        "swarm_id": swarm_id,
        "sender_id": user_id,
        "sender_display_name": sender_display_name,
        "creature_id": creature_id,
        "creature_name": creature_name,
        "species_name": species_name,
        "species_group": species_group,
        "content": body.content,
        "message_type": "chat",
        "created_at": now.to_rfc3339(),
    });

    // Broadcast
    let _ = state.rabble_broadcast.send(RabbleEvent {
        swarm_id,
        message: msg_json.clone(),
    });

    // Push notification for new chat messages — notify all rabble members
    // except the sender. 5-minute cooldown per user per rabble to avoid
    // spam during active conversations.
    {
        let pool_push = state.db.clone();
        let sender_uid = user_id.clone();
        let sender_creature_name = creature_name
            .clone()
            .unwrap_or_else(|| "Someone".to_string());
        let msg_content = body.content.clone();
        let swarm_name: String =
            sqlx::query_scalar("SELECT name FROM swarm_events WHERE swarm_id = $1")
                .bind(swarm_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "a rabble".into());

        tokio::spawn(async move {
            // Get all member owners except the sender
            let member_owners: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT c.owner_id
                 FROM creature_state cs
                 JOIN creatures c ON c.creature_id = cs.creature_id
                 WHERE cs.rabble_id = $1
                   AND cs.state IN ('hosting', 'in_rabble')
                   AND c.owner_id != $2",
            )
            .bind(swarm_id)
            .bind(&sender_uid)
            .fetch_all(&pool_push)
            .await
            .unwrap_or_default();

            for owner_id in &member_owners {
                // 5-minute cooldown per user per rabble
                let recently_notified = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(
                        SELECT 1 FROM notifications
                        WHERE user_id = $1 AND type = 'chat_message'
                        AND metadata->>'swarm_id' = $2
                        AND created_at > NOW() - INTERVAL '5 minutes'
                    )",
                )
                .bind(owner_id)
                .bind(swarm_id.to_string())
                .fetch_one(&pool_push)
                .await
                .unwrap_or(false);

                if recently_notified {
                    continue;
                }

                // Truncate message for notification body
                let body_text = if msg_content.len() > 100 {
                    format!("{}...", &msg_content[..100])
                } else {
                    msg_content.clone()
                };

                crate::handlers::push::notify_user(
                    &pool_push,
                    owner_id,
                    "chat_message",
                    &format!("{} in {}", sender_creature_name, swarm_name),
                    Some(&body_text),
                    Some(&serde_json::json!({
                        "swarm_id": swarm_id,
                        "creature_name": sender_creature_name,
                        "swarm_name": swarm_name,
                    })),
                    None,
                )
                .await;
            }
        });
    }

    // @mention notifications — parse @creatureName from content,
    // find the creature's owner, and send a push notification.
    // Runs in background (fire-and-forget, never blocks the response).
    if body.content.contains('@') {
        let pool_mention = state.db.clone();
        let content_mention = body.content.clone();
        let sender_name = creature_name
            .clone()
            .unwrap_or_else(|| "Someone".to_string());
        let swarm_id_mention = swarm_id;
        let sender_user_id = user_id.clone();

        tokio::spawn(async move {
            // Extract all @mentions — match @word or @"multi word"
            let re = regex::Regex::new(r"@(\S+)").unwrap();
            for cap in re.captures_iter(&content_mention) {
                let mentioned_name = &cap[1];

                // Look up creature by specimen_name (case-insensitive)
                let creature_row = sqlx::query(
                    "SELECT c.creature_id, c.owner_id, c.specimen_name
                     FROM creatures c
                     WHERE LOWER(c.specimen_name) = LOWER($1)
                     LIMIT 1",
                )
                .bind(mentioned_name)
                .fetch_optional(&pool_mention)
                .await
                .ok()
                .flatten();

                if let Some(row) = creature_row {
                    let mentioned_owner: String = row.get("owner_id");
                    let mentioned_creature_name: String = row
                        .try_get("specimen_name")
                        .unwrap_or_else(|_| mentioned_name.to_string());
                    let mentioned_creature_id: uuid::Uuid = row.get("creature_id");

                    // Don't notify yourself
                    if mentioned_owner == sender_user_id {
                        continue;
                    }

                    crate::handlers::push::notify_user(
                        &pool_mention,
                        &mentioned_owner,
                        "chat_mention",
                        &format!("{} mentioned {}!", sender_name, mentioned_creature_name),
                        Some(&content_mention),
                        Some(&serde_json::json!({
                            "swarm_id": swarm_id_mention,
                            "creature_id": mentioned_creature_id,
                            "creature_name": mentioned_creature_name,
                            "sender_name": sender_name,
                        })),
                        None,
                    )
                    .await;
                }
            }
        });
    }

    // Every Nth message, dispatch swarm_host narrator (non-blocking)
    let narrator_interval: i64 = std::env::var("RABBLE_NARRATOR_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let msg_count: i64 = sqlx::query(
        "SELECT COUNT(*) as cnt FROM rabble_messages WHERE swarm_id = $1 AND message_type = 'chat'",
    )
    .bind(swarm_id)
    .fetch_one(&state.db)
    .await
    .map(|r| r.try_get("cnt").unwrap_or(0))
    .unwrap_or(0);

    if msg_count > 0 && msg_count % narrator_interval == 0 {
        let swarm_ws_id: Option<uuid::Uuid> =
            sqlx::query("SELECT workspace_id FROM swarm_events WHERE swarm_id = $1")
                .bind(swarm_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .and_then(|r| {
                    r.try_get::<Option<uuid::Uuid>, _>("workspace_id")
                        .ok()
                        .flatten()
                });

        if let Some(ws_id) = swarm_ws_id {
            let state2 = state.clone();
            let user_id2 = user_id.clone();
            tokio::spawn(async move {
                let query = "Narrate what's happening in this rabble based on the recent conversation. Keep it brief and fun.".to_string();
                if let Ok(narration) = rabble_workspace::dispatch_rabble_action(
                    &state2,
                    ws_id,
                    "swarm_host",
                    "rabble_narration",
                    &query,
                    &user_id2,
                    None,
                )
                .await
                {
                    let _ = insert_narrator_message(&state2, swarm_id, &narration).await;
                }
            });
        }
    }

    Ok(Json(msg_json))
}

// ─── GET /api/rabble/:id/messages ──────────────────────────────────

pub async fn get_rabble_messages(
    State(state): State<AppState>,
    Path(swarm_id): Path<uuid::Uuid>,
    Query(params): Query<MessagesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(50).min(200);

    let rows = if let Some(ref before_str) = params.before {
        let before = chrono::DateTime::parse_from_rfc3339(before_str)
            .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid 'before' timestamp".into()))?
            .with_timezone(&chrono::Utc);

        sqlx::query(
            "SELECT m.message_id, m.swarm_id, m.sender_id, m.creature_id, m.creature_name, m.species_name, m.species_group, m.content, m.message_type, m.created_at,
                    u.display_name AS sender_display_name
             FROM rabble_messages m
             LEFT JOIN users u ON u.user_id = m.sender_id
             WHERE m.swarm_id = $1 AND m.created_at < $2
             ORDER BY m.created_at DESC
             LIMIT $3"
        )
        .bind(swarm_id)
        .bind(before)
        .bind(limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        sqlx::query(
            "SELECT m.message_id, m.swarm_id, m.sender_id, m.creature_id, m.creature_name, m.species_name, m.species_group, m.content, m.message_type, m.created_at,
                    u.display_name AS sender_display_name
             FROM rabble_messages m
             LEFT JOIN users u ON u.user_id = m.sender_id
             WHERE m.swarm_id = $1
             ORDER BY m.created_at DESC
             LIMIT $2"
        )
        .bind(swarm_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let messages: Vec<Value> = rows.iter().map(|row| {
        json!({
            "message_id": row.try_get::<uuid::Uuid, _>("message_id").ok(),
            "swarm_id": row.try_get::<uuid::Uuid, _>("swarm_id").ok(),
            "sender_id": row.try_get::<String, _>("sender_id").ok(),
            "sender_display_name": row.try_get::<Option<String>, _>("sender_display_name").ok().flatten(),
            "creature_id": row.try_get::<Option<uuid::Uuid>, _>("creature_id").ok().flatten(),
            "creature_name": row.try_get::<Option<String>, _>("creature_name").ok().flatten(),
            "species_name": row.try_get::<Option<String>, _>("species_name").ok().flatten(),
            "species_group": row.try_get::<Option<String>, _>("species_group").ok().flatten(),
            "content": row.try_get::<String, _>("content").ok(),
            "message_type": row.try_get::<String, _>("message_type").ok(),
            "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
        })
    }).collect();

    Ok(Json(json!({ "messages": messages })))
}

// ─── GET /api/rabble/:id/stream (SSE) ──────────────────────────────

pub async fn rabble_stream(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(swarm_id): Path<uuid::Uuid>,
    Query(params): Query<StreamQuery>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let _user_id = principal.user_id();

    // Verify swarm exists
    sqlx::query("SELECT 1 FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Rabble not found".into()))?;

    // Backfill messages since reconnection timestamp
    let backfill: Vec<Value> = if let Some(ref since_str) = params.since {
        if let Ok(since) = chrono::DateTime::parse_from_rfc3339(since_str) {
            let since_utc = since.with_timezone(&chrono::Utc);
            let rows = sqlx::query(
                "SELECT m.message_id, m.swarm_id, m.sender_id, m.creature_id, m.creature_name, m.species_name, m.species_group, m.content, m.message_type, m.created_at,
                        u.display_name AS sender_display_name
                 FROM rabble_messages m
                 LEFT JOIN users u ON u.user_id = m.sender_id
                 WHERE m.swarm_id = $1 AND m.created_at > $2
                 ORDER BY m.created_at ASC
                 LIMIT 200"
            )
            .bind(swarm_id)
            .bind(since_utc)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

            rows.iter().map(|row| {
                json!({
                    "message_id": row.try_get::<uuid::Uuid, _>("message_id").ok(),
                    "swarm_id": row.try_get::<uuid::Uuid, _>("swarm_id").ok(),
                    "sender_id": row.try_get::<String, _>("sender_id").ok(),
                    "sender_display_name": row.try_get::<Option<String>, _>("sender_display_name").ok().flatten(),
                    "creature_id": row.try_get::<Option<uuid::Uuid>, _>("creature_id").ok().flatten(),
                    "creature_name": row.try_get::<Option<String>, _>("creature_name").ok().flatten(),
                    "species_name": row.try_get::<Option<String>, _>("species_name").ok().flatten(),
                    "species_group": row.try_get::<Option<String>, _>("species_group").ok().flatten(),
                    "content": row.try_get::<String, _>("content").ok(),
                    "message_type": row.try_get::<String, _>("message_type").ok(),
                    "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
                })
            }).collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let mut rx = state.rabble_broadcast.subscribe();

    let stream = async_stream::stream! {
        // Send backfill messages first
        for msg_json in backfill {
            let data = serde_json::to_string(&msg_json).unwrap_or_default();
            yield Ok(Event::default().data(data));
        }

        let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30));
        keepalive.tick().await; // skip immediate tick

        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if event.swarm_id == swarm_id {
                                let data = serde_json::to_string(&event.message).unwrap_or_default();
                                yield Ok(Event::default().data(data));
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            yield Ok(Event::default().event("lagged").data("refetch"));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                _ = keepalive.tick() => {
                    yield Ok(Event::default().comment("keepalive"));
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("keepalive"),
    ))
}

// ─── Helper: insert narrator message ───────────────────────────────

pub async fn insert_narrator_message(
    state: &AppState,
    swarm_id: uuid::Uuid,
    content: &str,
) -> Result<(), String> {
    let message_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, content, message_type, created_at)
         VALUES ($1, $2, 'swarm_host', $3, 'narrator', $4)"
    )
    .bind(message_id)
    .bind(swarm_id)
    .bind(content)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    let msg_json = json!({
        "message_id": message_id,
        "swarm_id": swarm_id,
        "sender_id": "swarm_host",
        "content": content,
        "message_type": "narrator",
        "created_at": now.to_rfc3339(),
    });

    let _ = state.rabble_broadcast.send(RabbleEvent {
        swarm_id,
        message: msg_json,
    });

    // Emit activity event for narrator messages (fire-and-forget)
    {
        let _pool_ae = state.db.clone();
        let _swarm_ae = swarm_id;
        let _content_ae = content.to_string();
        tokio::spawn(async move {
            crate::handlers::social::emit_activity_event(
                &_pool_ae,
                "system",
                None,
                "chat_message",
                Some(_swarm_ae),
                None,
                &_content_ae,
                None,
                None,
            )
            .await;
        });
    }

    Ok(())
}

// ─── Invite endpoints (private rabbles) ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InviteRequest {
    pub user_id: Option<String>,
    pub team_id: Option<String>,
}

/// POST /api/rabble/:id/invite — invite a user or team to a private rabble.
/// Only the rabble creator can invite.
pub async fn invite_to_rabble(
    State(state): State<AppState>,
    principal: fermi_auth::AuthPrincipal,
    Path(swarm_id): Path<uuid::Uuid>,
    Json(body): Json<InviteRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller = principal.user_id();

    // Verify caller is creator
    let row = sqlx::query("SELECT creator_id, name FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Rabble not found".into()))?;

    let creator_id: String = row.get("creator_id");
    if creator_id != caller {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the rabble creator can invite".into(),
        ));
    }
    let rabble_name: String = row.try_get("name").unwrap_or_default();

    let (share_type, share_target) = if let Some(ref uid) = body.user_id {
        ("user", uid.clone())
    } else if let Some(ref tid) = body.team_id {
        ("team", tid.clone())
    } else {
        return Err((StatusCode::BAD_REQUEST, "Provide user_id or team_id".into()));
    };

    // Check for existing invite (idempotent)
    let existing = sqlx::query(
        "SELECT id FROM object_shares
         WHERE object_type = 'rabble' AND object_id = $1 AND share_target = $2
         LIMIT 1",
    )
    .bind(swarm_id.to_string())
    .bind(&share_target)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_some() {
        return Ok(Json(
            json!({ "status": "already_invited", "share_target": share_target }),
        ));
    }

    let share_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO object_shares (id, object_type, object_id, share_type, share_target, permission, granted_by, created_at)
         VALUES ($1, 'rabble', $2, $3, $4, 'edit', $5, NOW())",
    )
    .bind(share_id)
    .bind(swarm_id.to_string())
    .bind(share_type)
    .bind(&share_target)
    .bind(&caller)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Create notification for invitee (only for user invites)
    if share_type == "user" {
        crate::handlers::push::notify_user(
            &state.db,
            &share_target,
            "rabble_invite",
            &format!("Invited to {}", rabble_name),
            Some(&format!(
                "You've been invited to the rabble '{}'",
                rabble_name
            )),
            Some(&serde_json::json!({
                "swarm_id": swarm_id,
                "rabble_name": rabble_name,
            })),
            None,
        )
        .await;
    }

    Ok(Json(json!({
        "status": "invited",
        "share_id": share_id,
        "share_type": share_type,
        "share_target": share_target,
    })))
}

/// GET /api/rabble/:id/members — list rabble members (creator only).
/// GET /api/rabble/:id/members — list actual creatures currently in this rabble.
///
/// Any authenticated user can view members (needed for creature tray, chat context,
/// member list). Returns creatures with active flights in this swarm, not object_shares.
pub async fn list_rabble_members(
    State(state): State<AppState>,
    _principal: fermi_auth::AuthPrincipal,
    Path(swarm_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify rabble exists
    let swarm =
        sqlx::query("SELECT creator_id, anchor_creature_id FROM swarm_events WHERE swarm_id = $1")
            .bind(swarm_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Rabble not found".into()))?;

    let creator_id: String = swarm.get("creator_id");
    let anchor_creature_id: Option<uuid::Uuid> = swarm
        .try_get::<Option<uuid::Uuid>, _>("anchor_creature_id")
        .unwrap_or(None);

    // Get all creatures currently in this rabble.
    // Source of truth: creature_state.rabble_id (always up to date).
    // Falls back to creature_flights for creatures that joined before
    // creature_state was introduced.
    let rows = sqlx::query(
        "SELECT DISTINCT ON (c.creature_id)
                c.creature_id, c.specimen_name, c.scientific_name,
                c.species_group, c.asset_path, c.owner_id,
                COALESCE(cf.data_source, 'synthetic') AS data_source,
                COALESCE(cf.started_at, cs.updated_at) AS started_at,
                u.display_name AS owner_display_name
         FROM creature_state cs
         JOIN creatures c ON c.creature_id = cs.creature_id
         LEFT JOIN users u ON u.user_id = c.owner_id
         LEFT JOIN creature_flights cf ON cf.creature_id = c.creature_id
              AND cf.ended_at IS NULL
         WHERE cs.rabble_id = $1
           AND cs.state IN ('hosting', 'in_rabble')
         ORDER BY c.creature_id, cf.started_at DESC NULLS LAST",
    )
    .bind(swarm_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let members: Vec<Value> = rows
        .iter()
        .map(|r| {
            let cid: uuid::Uuid = r.get("creature_id");
            let is_anchor = anchor_creature_id == Some(cid);
            json!({
                "creature_id": cid,
                "creature_name": r.try_get::<Option<String>, _>("specimen_name").unwrap_or(None),
                "specimen_name": r.try_get::<Option<String>, _>("specimen_name").unwrap_or(None),
                "scientific_name": r.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
                "species_group": r.try_get::<Option<String>, _>("species_group").unwrap_or(None),
                "asset_path": r.try_get::<Option<String>, _>("asset_path").unwrap_or(None),
                "creature_image": r.try_get::<Option<String>, _>("asset_path").unwrap_or(None),
                "owner_id": r.get::<String, _>("owner_id"),
                "owner_display_name": r.try_get::<Option<String>, _>("owner_display_name").unwrap_or(None),
                "data_source": r.try_get::<String, _>("data_source").unwrap_or_else(|_| "synthetic".into()),
                "is_anchor": is_anchor,
                "joined_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("started_at").ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "creator_id": creator_id,
        "anchor_creature_id": anchor_creature_id,
        "members": members,
        "count": members.len(),
    })))
}

/// DELETE /api/rabble/:id/invite/:user_id — revoke a rabble invite (creator only).
pub async fn revoke_rabble_invite(
    State(state): State<AppState>,
    principal: fermi_auth::AuthPrincipal,
    Path((swarm_id, target_user_id)): Path<(uuid::Uuid, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller = principal.user_id();

    // Verify caller is creator
    let row = sqlx::query("SELECT creator_id FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Rabble not found".into()))?;

    let creator_id: String = row.get("creator_id");
    if creator_id != caller {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the rabble creator can revoke invites".into(),
        ));
    }

    let result = sqlx::query(
        "DELETE FROM object_shares
         WHERE object_type = 'rabble' AND object_id = $1 AND share_target = $2",
    )
    .bind(swarm_id.to_string())
    .bind(&target_user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Invite not found".into()));
    }

    Ok(Json(json!({
        "status": "revoked",
        "share_target": target_user_id,
    })))
}

// ─── Helper: insert system message (join/leave) ────────────────────

pub async fn insert_system_message(
    state: &AppState,
    swarm_id: uuid::Uuid,
    content: &str,
) -> Result<(), String> {
    let message_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO rabble_messages (message_id, swarm_id, sender_id, content, message_type, created_at)
         VALUES ($1, $2, 'system', $3, 'system', $4)"
    )
    .bind(message_id)
    .bind(swarm_id)
    .bind(content)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    let msg_json = json!({
        "message_id": message_id,
        "swarm_id": swarm_id,
        "sender_id": "system",
        "content": content,
        "message_type": "system",
        "created_at": now.to_rfc3339(),
    });

    let _ = state.rabble_broadcast.send(RabbleEvent {
        swarm_id,
        message: msg_json,
    });

    Ok(())
}
