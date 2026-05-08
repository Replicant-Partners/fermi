//! Observatory handlers — Plane D JSON API for the Phase 4 dashboard
//! and HITL review surfaces.
//!
//! Routes (registered on `api-server`):
//!
//! ```text
//!   GET  /api/observatory/agents/:id/timeline?window=N
//!   GET  /api/observatory/agents/:id/dyads
//!   GET  /api/observatory/agents/:id/anomalies?limit=N
//!   POST /api/observatory/agents/:id/scan
//!   GET  /api/observatory/hitl                      [admin or owner-of-any]
//!   POST /api/observatory/hitl/:event_id/action
//! ```
//!
//! See:
//! - `docs/architecture/social_agent_observability_architecture.html` (Plane D)
//! - `docs/architecture/OBSERVABILITY_IMPL.md` (Phase 4)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use agent_bestiary_memory::{HitlAction, ReviewerAction};
use agent_bestiary_observability::{
    ObservabilityWorker, TrendAnalyzer, TrendWindow,
};
use fermi_auth::AuthPrincipal;

use crate::{resolve_agent, AppState};

// ─── Permissions helpers ────────────────────────────────────────────

/// Q1 (a + admin override) — owner of the agent OR platform admin.
/// `curated` agents fall through to admin since they have no owner.
async fn require_owner_or_admin(
    state: &AppState,
    principal: &AuthPrincipal,
    agent_id: &str,
) -> Result<agent_bestiary_memory::Agent, (StatusCode, String)> {
    let db_agent = resolve_agent(state, agent_id).await?;
    let user_id = principal.user_id();
    let is_owner = db_agent.owner_id.as_deref() == Some(&user_id);
    let is_admin = principal.can_admin();
    if !is_owner && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            "Owner or admin access required".into(),
        ));
    }
    Ok(db_agent)
}

// ─── GET /api/observatory/agents/:id/timeline?window=N ──────────────

#[derive(Deserialize)]
pub struct TimelineQuery {
    pub window: Option<usize>,
}

pub async fn get_agent_timeline_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Query(q): Query<TimelineQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;

    let window = TrendWindow {
        size: q.window.unwrap_or(50).clamp(1, 500),
    };

    let analyzer = TrendAnalyzer::new(state.memory_store.clone());
    let report = analyzer
        .compute(db_agent.agent_id, window)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries = state
        .memory_store
        .list_timeline_entries(db_agent.agent_id, window.size as i64)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "agent_id": db_agent.agent_id,
        "agent_name": db_agent.agent_name,
        "persona_version": db_agent.persona_version,
        "window": window.size,
        "trend": report,
        "entries": entries,
    })))
}

// ─── GET /api/observatory/agents/:id/dyads ──────────────────────────

pub async fn list_agent_dyads_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;
    let dyads = state
        .memory_store
        .list_dyads_for_agent(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "dyads": dyads })))
}

// ─── GET /api/observatory/agents/:id/anomalies?limit=N ──────────────

#[derive(Deserialize)]
pub struct AnomaliesQuery {
    pub limit: Option<i64>,
}

pub async fn list_agent_anomalies_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Query(q): Query<AnomaliesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let events = state
        .memory_store
        .list_anomaly_events_for_agent(db_agent.agent_id, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "anomalies": events })))
}

// ─── POST /api/observatory/agents/:id/scan ──────────────────────────
//
// Q6 (a) — manual scan trigger. Owner+admin only. Best-effort —
// returns the ScanReport synchronously so operators see the result.

pub async fn trigger_agent_scan_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;

    let worker = ObservabilityWorker::new(Arc::clone(&state.memory_store));
    let report = worker
        .scan_agent(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "report": report })))
}

// ─── GET /api/observatory/hitl ──────────────────────────────────────
//
// Pending HITL queue. Two read modes:
//   - admin: sees all pending events
//   - owner: sees only events on agents they own
//
// Phase 4 implements the simpler "all-pending then filter by ownership"
// path. Once we have many agents this should push the filter into SQL.

#[derive(Deserialize)]
pub struct HitlQueueQuery {
    pub limit: Option<i64>,
}

pub async fn list_hitl_queue_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<HitlQueueQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let is_admin = principal.can_admin();

    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let pending = state
        .memory_store
        .list_pending_anomaly_events(limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let visible = if is_admin {
        pending
    } else {
        // Filter to agents this user owns. One DB lookup per distinct
        // agent_id — fine at Phase 4 scale; revisit when the queue
        // grows.
        use std::collections::{HashMap, HashSet};
        let mut owned: HashMap<uuid::Uuid, bool> = HashMap::new();
        let agent_ids: HashSet<_> = pending.iter().map(|e| e.agent_id).collect();
        for aid in &agent_ids {
            let owner = state
                .memory_store
                .get_agent(*aid)
                .await
                .ok()
                .flatten()
                .and_then(|a| a.owner_id);
            owned.insert(*aid, owner.as_deref() == Some(&user_id));
        }
        pending
            .into_iter()
            .filter(|e| *owned.get(&e.agent_id).unwrap_or(&false))
            .collect()
    };

    Ok(Json(json!({ "queue": visible })))
}

// ─── POST /api/observatory/hitl/:event_id/action ────────────────────

#[derive(Deserialize)]
pub struct HitlActionRequest {
    pub action: String, // "approve" | "relabel" | "intervene"
    pub notes: Option<String>,
    #[serde(default)]
    pub score_overrides: serde_json::Value,
}

pub async fn record_hitl_action_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(event_id_s): Path<String>,
    Json(body): Json<HitlActionRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let event_id: uuid::Uuid = event_id_s
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid event_id".into()))?;

    let event = state
        .memory_store
        .get_anomaly_event(event_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Anomaly event not found".into()))?;

    // Auth check: owner of the agent or admin.
    let user_id = principal.user_id();
    let is_admin = principal.can_admin();
    let agent = state
        .memory_store
        .get_agent(event.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".into()))?;
    let is_owner = agent.owner_id.as_deref() == Some(&user_id);
    if !is_owner && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            "Owner or admin access required".into(),
        ));
    }

    let action: ReviewerAction = body
        .action
        .parse()
        .map_err(|e: String| (StatusCode::BAD_REQUEST, e))?;

    // Q3 / Phase 5 gating — `intervene` triggers the full feedback
    // flow which is implemented in Phase 5. Phase 4 returns 501 so
    // the UI can disable the button without breaking the wire.
    if matches!(action, ReviewerAction::Intervene) {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            "intervene action requires Phase 5 (coherence gate + two-write memory pattern)".into(),
        ));
    }

    let hitl = HitlAction {
        action_id: uuid::Uuid::new_v4(),
        anomaly_event_id: event_id,
        agent_id: event.agent_id,
        reviewer_id: user_id.clone(),
        action,
        notes: body.notes,
        score_overrides: body.score_overrides,
        correction_id: None,
        created_at: chrono::Utc::now(),
    };
    let action_id = state
        .memory_store
        .create_hitl_action(&hitl)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Mark the anomaly resolved so the queue stops surfacing it.
    state
        .memory_store
        .resolve_anomaly_event(event_id, &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "action_id": action_id,
        "anomaly_event_id": event_id,
        "resolved": true,
    })))
}
