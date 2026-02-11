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
    if status != "active" {
        return Err((StatusCode::CONFLICT, "Rabble is not active".into()));
    }

    // Verify sender has a creature in this rabble (via creature_flights)
    let has_creature = sqlx::query(
        "SELECT cf.creature_id, c.specimen_name, c.species_name, c.species_group
         FROM creature_flights cf
         JOIN creatures c ON c.creature_id = cf.creature_id
         WHERE cf.swarm_id = $1 AND c.owner_id = $2 AND cf.ended_at IS NULL
         LIMIT 1",
    )
    .bind(swarm_id)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Use the creature from the flight, or the one specified in the request
    let (creature_id, creature_name, species_name, species_group) = if let Some(row) = &has_creature
    {
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
    };

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
            "SELECT message_id, swarm_id, sender_id, creature_id, creature_name, species_name, species_group, content, message_type, created_at
             FROM rabble_messages
             WHERE swarm_id = $1 AND created_at < $2
             ORDER BY created_at DESC
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
            "SELECT message_id, swarm_id, sender_id, creature_id, creature_name, species_name, species_group, content, message_type, created_at
             FROM rabble_messages
             WHERE swarm_id = $1
             ORDER BY created_at DESC
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
                "SELECT message_id, swarm_id, sender_id, creature_id, creature_name, species_name, species_group, content, message_type, created_at
                 FROM rabble_messages
                 WHERE swarm_id = $1 AND created_at > $2
                 ORDER BY created_at ASC
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

    Ok(())
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
