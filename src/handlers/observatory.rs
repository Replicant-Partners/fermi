//! Observatory handlers — Plane D JSON API.
//!
//! Routes (registered on `api-server`):
//!
//! ```text
//!   GET  /api/observatory/agents/:id/timeline?window=N
//!   GET  /api/observatory/agents/:id/dyads
//!   GET  /api/observatory/agents/:id/anomalies?limit=N
//!   POST /api/observatory/agents/:id/scan
//!   GET  /api/observatory/hitl                           [admin or owner-of-any]
//!   POST /api/observatory/hitl/:event_id/action          [approve | relabel | intervene]
//!   POST /api/observatory/hitl/consensus/:request_id     [second reviewer confirm/reject]
//! ```
//!
//! ## Phase 5 — Intervention feedback loop
//!
//! The `intervene` action now executes the full five-step flow from the
//! architecture doc (Plane D):
//!
//! 1. Reviewer acts (this handler)
//! 2. `InterventionEncoder::encode` → `EncodedIntervention`
//! 3. `CoherenceGate::check` → blocks `AgentWide` when Γ(C) < 0.5
//! 4. `AgentWide` scope: create `two_reviewer_requests` row, return 202
//!    (awaiting second reviewer).  `Episode`/`Dyad` scope: proceed immediately.
//! 5. `TwoWriteMemory::execute` → annotation + synthetic episode + optional
//!    persona_version bump
//!
//! See:
//! - `docs/architecture/social_agent_observability_architecture.html` (Plane D)
//! - `docs/architecture/OBSERVABILITY_IMPL.md` (Phase 5)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use agent_bestiary_coherence_gate::{
    encoder::{InterventionEncoder, InterventionRequest},
    gate::CoherenceGate,
    two_write::TwoWriteMemory,
    GateError,
};
use agent_bestiary_memory::{
    CorrectionClassification, CorrectionScope, HitlAction, ReviewerAction, TwoReviewerRequest,
};
use agent_bestiary_observability::{ObservabilityWorker, TrendAnalyzer, TrendWindow};
use fermi_auth::AuthPrincipal;

use crate::{resolve_agent, AppState};

// ─── Permission helpers ──────────────────────────────────────────────────────

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
        return Err((StatusCode::FORBIDDEN, "Owner or admin access required".into()));
    }
    Ok(db_agent)
}

// ─── GET /api/observatory/agents/:id/timeline?window=N ──────────────────────

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

// ─── GET /api/observatory/agents/:id/dyads ──────────────────────────────────

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

// ─── GET /api/observatory/agents/:id/anomalies?limit=N ──────────────────────

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

// ─── POST /api/observatory/agents/:id/scan ──────────────────────────────────

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

// ─── GET /api/observatory/hitl ──────────────────────────────────────────────

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
        use std::collections::{HashMap, HashSet};
        let mut owned: HashMap<Uuid, bool> = HashMap::new();
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

// ─── POST /api/observatory/hitl/:event_id/action ────────────────────────────
//
// Phase 5: `intervene` action now executes the full feedback loop.
//
// For `episode` / `dyad` scope:
//   - InterventionEncoder → CoherenceGate (settler mode) → TwoWriteMemory
//   - Returns 200 with correction_id + synthetic_episode_id
//
// For `agent_wide` scope:
//   - InterventionEncoder → CoherenceGate (synchronous gate)
//   - If gate blocks: returns 422 with tensions
//   - If gate approves: creates a `two_reviewer_requests` row, returns 202
//     (awaiting second reviewer at POST /api/observatory/hitl/consensus/:id)

#[derive(Deserialize)]
pub struct HitlActionRequest {
    /// "approve" | "relabel" | "intervene"
    pub action: String,
    pub notes: Option<String>,
    #[serde(default)]
    pub score_overrides: serde_json::Value,

    // ── intervene-specific fields ────────────────────────────────────
    /// "episode" | "dyad" | "agent_wide"
    pub scope: Option<String>,
    /// "belief" | "behaviour"
    pub classification: Option<String>,
    /// The evaluator dimension being corrected.
    pub dimension: Option<String>,
    /// Free-text description of the corrected response.
    pub correction_text: Option<String>,
    /// Human-readable justification.
    pub justification: Option<String>,
}

pub async fn record_hitl_action_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(event_id_s): Path<String>,
    Json(body): Json<HitlActionRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let event_id: Uuid = event_id_s
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid event_id".into()))?;

    let event = state
        .memory_store
        .get_anomaly_event(event_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Anomaly event not found".into()))?;

    // Auth: owner of the agent or admin.
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
        return Err((StatusCode::FORBIDDEN, "Owner or admin access required".into()));
    }

    let action: ReviewerAction = body
        .action
        .parse()
        .map_err(|e: String| (StatusCode::BAD_REQUEST, e))?;

    // ── approve / relabel path (unchanged from Phase 4) ─────────────────

    if !matches!(action, ReviewerAction::Intervene) {
        let hitl = HitlAction {
            action_id: Uuid::new_v4(),
            anomaly_event_id: event_id,
            agent_id: event.agent_id,
            reviewer_id: user_id.clone(),
            action,
            notes: body.notes,
            score_overrides: body.score_overrides,
            correction_id: None,
            created_at: Utc::now(),
        };
        let action_id = state
            .memory_store
            .create_hitl_action(&hitl)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        state
            .memory_store
            .resolve_anomaly_event(event_id, &user_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        return Ok(Json(json!({
            "action_id": action_id,
            "anomaly_event_id": event_id,
            "resolved": true,
        })));
    }

    // ── intervene path — Phase 5 ─────────────────────────────────────────

    let scope: CorrectionScope = body
        .scope
        .as_deref()
        .unwrap_or("episode")
        .parse()
        .map_err(|e: String| (StatusCode::BAD_REQUEST, e))?;

    let classification: Option<CorrectionClassification> = body
        .classification
        .as_deref()
        .map(|s| s.parse().map_err(|e: String| (StatusCode::BAD_REQUEST, e)))
        .transpose()?;

    // Step 2 — Encode.
    let req = InterventionRequest {
        anomaly_event_id: event_id,
        agent_id: event.agent_id,
        episode_id: event.episode_id,
        reviewer_id: user_id.clone(),
        scope,
        classification,
        dimension: body.dimension.clone(),
        correction_text: body.correction_text.clone(),
        score_overrides: body.score_overrides.clone(),
        justification: body.justification.clone(),
    };

    let encoded = InterventionEncoder::encode(req).map_err(|e| match e {
        GateError::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    })?;

    // Step 3 — Coherence gate.
    let gate = CoherenceGate::default();
    let gate_outcome = gate.check(&encoded).map_err(|e| match e {
        GateError::Blocked {
            gamma,
            threshold,
            ref tensions,
        } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "coherence gate blocked: gamma={:.3} < threshold={:.3}, tensions={:?}",
                gamma, threshold, tensions
            ),
        ),
        other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    })?;

    // Step 4a — AgentWide: two-reviewer consensus required.
    if scope == CorrectionScope::AgentWide {
        let encoded_json = serde_json::to_value(&encoded)
            .unwrap_or(serde_json::Value::Null);

        let two_review_req = TwoReviewerRequest {
            request_id: Uuid::new_v4(),
            anomaly_event_id: event_id,
            agent_id: event.agent_id,
            encoded_intervention: encoded_json,
            first_reviewer_id: user_id.clone(),
            first_reviewed_at: Utc::now(),
            second_reviewer_id: None,
            second_reviewed_at: None,
            second_approved: None,
            status: "pending".to_string(),
            correction_id: None,
            synthetic_episode_id: None,
            notes: body.notes.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let request_id = state
            .memory_store
            .create_two_reviewer_request(&two_review_req)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Record HITL action referencing the pending consensus request.
        let hitl = HitlAction {
            action_id: Uuid::new_v4(),
            anomaly_event_id: event_id,
            agent_id: event.agent_id,
            reviewer_id: user_id.clone(),
            action: ReviewerAction::Intervene,
            notes: body.notes,
            score_overrides: body.score_overrides,
            correction_id: None,
            created_at: Utc::now(),
        };
        state
            .memory_store
            .create_hitl_action(&hitl)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        return Ok(Json(json!({
            "status": "awaiting_second_reviewer",
            "request_id": request_id,
            "anomaly_event_id": event_id,
            "gate": gate_outcome,
            "message": "agent_wide intervention recorded; a second reviewer must confirm at POST /api/observatory/hitl/consensus/:request_id",
        })));
    }

    // Step 4b — Episode/Dyad: execute immediately.
    let original_episode = if let Some(ep_id) = event.episode_id {
        state
            .memory_store
            .get_episode(ep_id)
            .await
            .ok()
    } else {
        None
    };

    let two_write = TwoWriteMemory::new(Arc::clone(&state.memory_store));
    let receipt = two_write
        .execute(&encoded, &gate_outcome, original_episode)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Record HITL action with correction_id back-link.
    let hitl = HitlAction {
        action_id: Uuid::new_v4(),
        anomaly_event_id: event_id,
        agent_id: event.agent_id,
        reviewer_id: user_id.clone(),
        action: ReviewerAction::Intervene,
        notes: body.notes,
        score_overrides: body.score_overrides,
        correction_id: Some(receipt.correction_id),
        created_at: Utc::now(),
    };
    let action_id = state
        .memory_store
        .create_hitl_action(&hitl)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Mark the anomaly resolved.
    state
        .memory_store
        .resolve_anomaly_event(event_id, &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "intervention_complete",
        "action_id": action_id,
        "anomaly_event_id": event_id,
        "correction_id": receipt.correction_id,
        "synthetic_episode_id": receipt.synthetic_episode_id,
        "gate": gate_outcome,
        "persona_version_bumped": receipt.persona_version_bumped,
        "new_persona_version": receipt.new_persona_version,
        "resolved": true,
    })))
}

// ─── POST /api/observatory/hitl/consensus/:request_id ───────────────────────
//
// Second reviewer confirms or rejects a pending `agent_wide` intervention.
//
// Rules:
//   - Second reviewer must be a different user from the first reviewer.
//   - `approved = true` → gate already passed (stored in the request row);
//     execute TwoWriteMemory and mark the anomaly resolved.
//   - `approved = false` → mark the request rejected; anomaly remains open.

#[derive(Deserialize)]
pub struct ConsensusRequest {
    pub approved: bool,
    pub notes: Option<String>,
}

pub async fn confirm_two_reviewer_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(request_id_s): Path<String>,
    Json(body): Json<ConsensusRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let request_id: Uuid = request_id_s
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid request_id".into()))?;

    let two_req = state
        .memory_store
        .get_two_reviewer_request(request_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Two-reviewer request not found".into()))?;

    if two_req.status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            format!("Request is already in status '{}'", two_req.status),
        ));
    }

    let user_id = principal.user_id();
    if two_req.first_reviewer_id == user_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Second reviewer must be a different user from the first reviewer".into(),
        ));
    }

    // Auth: owner or admin on the agent.
    let agent = state
        .memory_store
        .get_agent(two_req.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".into()))?;
    let is_owner = agent.owner_id.as_deref() == Some(&user_id);
    let is_admin = principal.can_admin();
    if !is_owner && !is_admin {
        return Err((StatusCode::FORBIDDEN, "Owner or admin access required".into()));
    }

    if !body.approved {
        // Rejected — mark request and return.
        state
            .memory_store
            .confirm_two_reviewer_request(request_id, &user_id, false, None, None)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        return Ok(Json(json!({
            "status": "rejected",
            "request_id": request_id,
            "message": "intervention rejected by second reviewer",
        })));
    }

    // Approved — deserialize the stored EncodedIntervention and run the
    // two-write pattern now.
    let encoded: agent_bestiary_coherence_gate::EncodedIntervention =
        serde_json::from_value(two_req.encoded_intervention.clone()).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to deserialize stored intervention: {}", e),
            )
        })?;

    // Re-run the gate to get a fresh GateOutcome for the receipt
    // (the gate already approved when the first reviewer submitted; we
    // run it again to populate the gate_outcome fields in the response).
    let gate = CoherenceGate::default();
    let gate_outcome = gate.check(&encoded).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Gate re-check failed: {}", e),
        )
    })?;

    let original_episode = if let Some(ep_id) = encoded.episode_id {
        state.memory_store.get_episode(ep_id).await.ok()
    } else {
        None
    };

    let two_write = TwoWriteMemory::new(Arc::clone(&state.memory_store));
    let receipt = two_write
        .execute(&encoded, &gate_outcome, original_episode)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update the consensus request row.
    state
        .memory_store
        .confirm_two_reviewer_request(
            request_id,
            &user_id,
            true,
            Some(receipt.correction_id),
            Some(receipt.synthetic_episode_id),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Mark anomaly resolved.
    state
        .memory_store
        .resolve_anomaly_event(two_req.anomaly_event_id, &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "intervention_complete",
        "request_id": request_id,
        "correction_id": receipt.correction_id,
        "synthetic_episode_id": receipt.synthetic_episode_id,
        "gate": gate_outcome,
        "persona_version_bumped": receipt.persona_version_bumped,
        "new_persona_version": receipt.new_persona_version,
        "resolved": true,
    })))
}
