//! Creature SSE stream — real-time push of creature lifecycle events.
//!
//! `GET /api/creatures/:creature_id/stream` opens a Server-Sent Events
//! connection that delivers state changes, location updates, flight events,
//! friendship changes, and rabble enter/leave notifications for a single
//! creature.
//!
//! Auth: caller must own the creature **or** have a creature in the same
//! rabble (co-presence). Unauthenticated requests are rejected.
//!
//! Reconnection: pass `?since=<RFC3339>` to receive a backfill of events
//! that occurred while the client was disconnected.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
};
use futures_core::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::convert::Infallible;
use uuid::Uuid;

use crate::{AppState, CreatureEvent};
use fermi_auth::AuthPrincipal;

// ═══════════════════════════════════════════════════════════════════════════
// QUERY PARAMS
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct CreatureStreamQuery {
    /// RFC 3339 timestamp — if provided, events since this time are back-filled
    /// before switching to live push.
    pub since: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// SSE HANDLER
// ═══════════════════════════════════════════════════════════════════════════

/// `GET /api/creatures/:creature_id/stream`
///
/// Opens an SSE connection scoped to a single creature.  The caller must
/// either own the creature or be co-present in the same rabble.
pub async fn creature_stream_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(creature_id): Path<Uuid>,
    Query(params): Query<CreatureStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // ── Auth: ownership or co-presence ─────────────────────────────
    let allowed = check_creature_access(&state.db, &user_id, creature_id).await?;
    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            "You must own this creature or share a rabble with it".to_string(),
        ));
    }

    // ── Backfill (reconnection catch-up) ───────────────────────────
    let backfill: Vec<Value> = if let Some(ref since_str) = params.since {
        backfill_events(&state.db, creature_id, since_str).await
    } else {
        vec![]
    };

    // ── Subscribe to broadcast ─────────────────────────────────────
    let mut rx = state.creature_broadcast.subscribe();

    let stream = async_stream::stream! {
        // 1. Emit backfill events first so the client catches up.
        for evt in backfill {
            let data = serde_json::to_string(&evt).unwrap_or_default();
            yield Ok(Event::default().event("backfill").data(data));
        }

        // 2. Keepalive interval — prevents proxy/LB idle-timeout kills.
        let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30));
        keepalive.tick().await; // discard first immediate tick

        // 3. Live event loop.
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if event.creature_id == creature_id {
                                let data = serde_json::to_string(&event.payload)
                                    .unwrap_or_default();
                                yield Ok(
                                    Event::default()
                                        .event(&event.event_type)
                                        .data(data)
                                );
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            // Buffer overflow — tell client how many it missed so
                            // it can decide whether to do a full refetch.
                            let msg = json!({ "missed": n }).to_string();
                            yield Ok(Event::default().event("lagged").data(msg));
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

// ═══════════════════════════════════════════════════════════════════════════
// ACCESS CHECK
// ═══════════════════════════════════════════════════════════════════════════

/// Returns `true` when the caller either:
///   1. Owns the creature, OR
///   2. Owns any creature that is currently in the same rabble.
async fn check_creature_access(
    db: &sqlx::PgPool,
    user_id: &str,
    creature_id: Uuid,
) -> Result<bool, (StatusCode, String)> {
    // Fast path: ownership check.
    let owns = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM creatures
            WHERE creature_id = $1 AND owner_id = $2
        )",
    )
    .bind(creature_id)
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
    })?;

    if owns {
        return Ok(true);
    }

    // Slow path: co-presence — does the caller have *any* creature in the
    // same rabble as the target creature?
    let co_present = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1
            FROM swarm_participants sp1
            JOIN swarm_participants sp2 ON sp1.swarm_id = sp2.swarm_id
            JOIN creatures c ON sp2.creature_id = c.creature_id
            WHERE sp1.creature_id = $1
              AND c.owner_id = $2
              AND sp1.left_at IS NULL
              AND sp2.left_at IS NULL
        )",
    )
    .bind(creature_id)
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
    })?;

    Ok(co_present)
}

// ═══════════════════════════════════════════════════════════════════════════
// BACKFILL
// ═══════════════════════════════════════════════════════════════════════════

/// Fetch activity events for this creature that happened after `since`.
/// Returns at most 200 events (newest last) so a reconnecting client can
/// catch up without a full reload.
async fn backfill_events(db: &sqlx::PgPool, creature_id: Uuid, since_str: &str) -> Vec<Value> {
    let since = match chrono::DateTime::parse_from_rfc3339(since_str) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => return vec![],
    };

    // Pull from activity_events — the table that `emit_activity_event` writes to.
    let rows = sqlx::query(
        r#"
        SELECT event_type, title, body, metadata, created_at
        FROM activity_events
        WHERE creature_id = $1
          AND created_at > $2
        ORDER BY created_at ASC
        LIMIT 200
        "#,
    )
    .bind(creature_id)
    .bind(since)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    rows.iter()
        .map(|row| {
            json!({
                "event_type": row.get::<String, _>("event_type"),
                "title": row.get::<String, _>("title"),
                "body": row.get::<Option<String>, _>("body"),
                "metadata": row.get::<Option<Value>, _>("metadata"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .to_rfc3339(),
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// BROADCAST HELPER — called from mutation handlers across the codebase
// ═══════════════════════════════════════════════════════════════════════════

/// Fire-and-forget broadcast of a creature event.
///
/// Safe to call even when no subscribers are listening (the send simply
/// returns `Err` which we ignore).
///
/// # Event types
///
/// | `event_type`       | When                                    |
/// |--------------------|-----------------------------------------|
/// | `state_changed`    | fly, perch, tether, untether             |
/// | `location_update`  | push_telemetry                           |
/// | `flight_started`   | record_flight, fly                       |
/// | `flight_ended`     | end_flight                               |
/// | `friend_request`   | send_friendship_request                  |
/// | `friend_accepted`  | accept_friendship                        |
/// | `entered_rabble`   | join_swarm                               |
/// | `left_rabble`      | end_flight (when leaving), leave_swarm   |
/// | `transferred`      | transfer_creature                        |
/// | `presence_changed` | update_creature_presence                 |
pub(crate) fn emit_creature_event(
    state: &AppState,
    creature_id: Uuid,
    event_type: &str,
    payload: Value,
) {
    let event = CreatureEvent {
        creature_id,
        event_type: event_type.to_string(),
        payload,
    };
    // Ignore send errors — they just mean nobody is subscribed right now.
    let _ = state.creature_broadcast.send(event);
}
