//! Observatory handlers — Plane D JSON API.
//!
//! Routes (registered on `api-server`):
//!
//! ```text
//!   GET  /api/observatory/agents/:id/timeline?window=N
//!   GET  /api/observatory/agents/:id/dyads
//!   GET  /api/observatory/agents/:id/relationships
//!   GET  /api/observatory/agents/:id/anomalies?limit=N
//!   GET  /api/observatory/agents/:id/loops
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
use sqlx::Row;
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
use fermi_auth::{rbac, AuthPrincipal, ObjectType, Visibility};

use crate::{resolve_agent, AppState};

// ─── Permission helpers ──────────────────────────────────────────────────────

async fn require_owner_or_admin(
    state: &AppState,
    principal: &AuthPrincipal,
    agent_id: &str,
) -> Result<agent_bestiary_memory::Agent, (StatusCode, String)> {
    let db_agent = resolve_agent(state, agent_id).await?;
    // Curated agents (owner_id = NULL, tier = "curated") are
    // observable by any authenticated user — they are platform-level
    // agents. Everyone else goes through substrate RBAC.
    let is_curated = db_agent.owner_id.is_none() && db_agent.tier == "curated";
    if is_curated {
        return Ok(db_agent);
    }
    // v0.10.5: substrate RBAC. Observatory is read-only but the
    // signals it exposes (episode counts, timelines) can leak
    // execution history — gate on Admin (owner + platform admin).
    rbac::require_admin_on(
        &state.db,
        principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        Visibility::Private,
    )
    .await?;
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

// ─── POST /api/observatory/agents/:id/backfill-social ──────────────────────

/// Replay the social pass over an agent's full timeline.
///
/// Needed once per agent whose history predates the social pass — those
/// entries sit behind the scan checkpoint and would otherwise never be
/// scored. Idempotent; safe to re-run.
pub async fn backfill_social_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;
    let worker = ObservabilityWorker::new(Arc::clone(&state.memory_store));
    let report = worker
        .backfill_social(db_agent.agent_id)
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
        return Err((
            StatusCode::FORBIDDEN,
            "Owner or admin access required".into(),
        ));
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
        let encoded_json = serde_json::to_value(&encoded).unwrap_or(serde_json::Value::Null);

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
        state.memory_store.get_episode(ep_id).await.ok()
    } else {
        None
    };

    let two_write = TwoWriteMemory::new(Arc::clone(&state.memory_store))
        .with_embedder(Arc::clone(&state.embedder));
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
        .ok_or((
            StatusCode::NOT_FOUND,
            "Two-reviewer request not found".into(),
        ))?;

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
        return Err((
            StatusCode::FORBIDDEN,
            "Owner or admin access required".into(),
        ));
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

    let two_write = TwoWriteMemory::new(Arc::clone(&state.memory_store))
        .with_embedder(Arc::clone(&state.embedder));
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

// ─── Fleet endpoints ─────────────────────────────────────────────────────────

pub async fn fleet_summary_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let agents: Vec<_> = state
        .memory_store
        .list_agents_for_owner(&user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .into_iter()
        .filter(|a| !crate::handlers::is_test_cruft(&a.agent_name))
        .collect();
    let agent_ids: Vec<Uuid> = agents.iter().map(|a| a.agent_id).collect();

    // The eval-run tally that used to drive staging is gone with
    // `maturity_stage`: counting attempts is exactly what made the old ladder a
    // vanity metric, and nothing else in this handler needed the number.

    let open_anom_map: std::collections::HashMap<Uuid, i64> = if !agent_ids.is_empty() {
        sqlx::query("SELECT agent_id, COUNT(*) as cnt FROM anomaly_events WHERE agent_id = ANY($1) AND resolved_at IS NULL GROUP BY agent_id")
            .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
            .iter().map(|r| (r.get("agent_id"), r.get::<i64,_>("cnt"))).collect()
    } else {
        std::collections::HashMap::new()
    };

    let mut buckets = std::collections::BTreeMap::<String, i32>::new();
    let mut provider_health: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();

    // Outcome-based staging, one round trip for the whole fleet.
    let fleet_ev = crate::handlers::evolution::fleet_evolution(&state.db).await;

    for ag in &agents {
        let anom = open_anom_map.get(&ag.agent_id).copied().unwrap_or(0);
        *buckets
            .entry(maturity_from_evolution(fleet_ev.get(&ag.agent_id), anom))
            .or_insert(0) += 1;
        let p = ag.llm_provider.clone();
        let entry = provider_health
            .entry(p)
            .or_insert(serde_json::json!({"agent_count":0i32,"open_anomalies":0i32,"agents":[]}));
        if let Some(obj) = entry.as_object_mut() {
            *obj.entry("agent_count").or_insert(serde_json::json!(0)) =
                serde_json::json!(obj["agent_count"].as_i64().unwrap_or(0) + 1);
            *obj.entry("open_anomalies").or_insert(serde_json::json!(0)) =
                serde_json::json!(obj["open_anomalies"].as_i64().unwrap_or(0) + anom);
            if let Some(arr) = obj.get_mut("agents").and_then(|v| v.as_array_mut()) {
                arr.push(serde_json::json!(ag.agent_name));
            }
        }
    }
    let curated_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agents WHERE user_id IS NULL AND tier = 'curated'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    let total_open: i64 = open_anom_map.values().sum();
    Ok(Json(serde_json::json!({
        "total_agents": agents.len() as i64 + curated_count,
        "owned_agents": agents.len(), "curated_agents": curated_count,
        "open_anomalies": total_open, "maturity_buckets": buckets, "provider_health": provider_health,
    })))
}

pub async fn fleet_scan_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let since = Utc::now() - chrono::Duration::days(7);
    let rows = sqlx::query(
        r#"SELECT ate.agent_id, ate.dim_scores,
                  COALESCE(ate.provider_used, a.llm_provider, 'unknown') as provider,
                  a.agent_name, ate.created_at
           FROM agent_timeline_entries ate
           JOIN agents a ON a.agent_id = ate.agent_id
           WHERE a.user_id = $1 AND ate.created_at >= $2
             AND ate.dim_scores IS NOT NULL AND ate.dim_scores != '{}'::jsonb
           ORDER BY ate.agent_id, ate.created_at DESC"#,
    )
    .bind(&user_id)
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agent_dim: std::collections::HashMap<(Uuid, String), (String, String, Vec<f64>)> =
        std::collections::HashMap::new();
    for row in &rows {
        let aid: Uuid = row.get("agent_id");
        let name: String = row.get("agent_name");
        let provider: String = row.try_get("provider").unwrap_or_default();
        if let Some(obj) = row
            .try_get::<serde_json::Value, _>("dim_scores")
            .ok()
            .and_then(|v| v.as_object().cloned())
        {
            for (dim, val) in obj {
                if let Some(s) = val.as_f64() {
                    agent_dim
                        .entry((aid, dim))
                        .or_insert((name.clone(), provider.clone(), vec![]))
                        .2
                        .push(s);
                }
            }
        }
    }

    let mut signals: std::collections::HashMap<(String, String), Vec<(String, f64)>> =
        std::collections::HashMap::new();
    for ((_aid, dim), (name, provider, scores)) in &agent_dim {
        if scores.len() < 2 {
            continue;
        }
        let decline = scores.last().unwrap() - scores[0];
        if decline > 0.10 {
            signals
                .entry((provider.clone(), dim.clone()))
                .or_default()
                .push((name.clone(), decline));
        }
    }

    let mut fleet_anomalies: Vec<Value> = signals
        .iter()
        .filter(|(_, aff)| aff.len() >= 3)
        .map(|((provider, dim), aff)| {
            let avg = aff.iter().map(|(_, d)| d).sum::<f64>() / aff.len() as f64;
            serde_json::json!({
                "suspected_provider": provider, "dimension": dim,
                "affected_agent_count": aff.len(),
                "affected_agents": aff.iter().map(|(n,_)| n).collect::<Vec<_>>(),
                "avg_decline": (avg*100.0).round()/100.0,
                "severity": if avg>0.25{"high"} else if avg>0.15{"medium"} else {"low"},
            })
        })
        .collect();
    fleet_anomalies.sort_by_key(|a| match a["severity"].as_str().unwrap_or("") {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    });

    Ok(Json(serde_json::json!({
        "scanned_at": Utc::now(), "window_days": 7,
        "fleet_anomalies": fleet_anomalies,
        "threshold": {"min_agents":3,"min_decline_pct":10},
    })))
}

pub async fn fleet_agents_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let agents: Vec<_> = state
        .memory_store
        .list_agents_for_owner(&user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .into_iter()
        .filter(|a| !crate::handlers::is_test_cruft(&a.agent_name))
        .collect();
    let agent_ids: Vec<Uuid> = agents.iter().map(|a| a.agent_id).collect();
    if agent_ids.is_empty() {
        return Ok(Json(serde_json::json!({"agents":[]})));
    }

    let run_c: std::collections::HashMap<Uuid, i64> = sqlx::query(
        "SELECT agent_id,COUNT(*) as cnt FROM eval_runs WHERE agent_id=ANY($1) GROUP BY agent_id",
    )
    .bind(&agent_ids)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .iter()
    .map(|r| (r.get("agent_id"), r.get::<i64, _>("cnt")))
    .collect();
    let anom_c: std::collections::HashMap<Uuid,i64> =
        sqlx::query("SELECT agent_id,COUNT(*) as cnt FROM anomaly_events WHERE agent_id=ANY($1) AND resolved_at IS NULL GROUP BY agent_id")
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get("agent_id"),r.get::<i64,_>("cnt"))).collect();
    let scores_c: std::collections::HashMap<Uuid,serde_json::Value> =
        sqlx::query(r#"SELECT DISTINCT ON (agent_id) agent_id,dim_scores FROM agent_timeline_entries WHERE agent_id=ANY($1) AND dim_scores IS NOT NULL AND dim_scores!='{}'::jsonb ORDER BY agent_id,created_at DESC"#)
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get::<Uuid,_>("agent_id"),r.try_get::<serde_json::Value,_>("dim_scores").unwrap_or(serde_json::json!({})))).collect();
    // Outcome-based staging for the whole fleet in one round trip; see
    // `maturity_from_evolution` for why run counts no longer decide this.
    let fleet_ev = crate::handlers::evolution::fleet_evolution(&state.db).await;
    let dyad_c: std::collections::HashMap<Uuid, i64> = sqlx::query(
        "SELECT agent_id,COUNT(*) as cnt FROM dyad_state WHERE agent_id=ANY($1) GROUP BY agent_id",
    )
    .bind(&agent_ids)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .iter()
    .map(|r| (r.get("agent_id"), r.get::<i64, _>("cnt")))
    .collect();
    let tc_c: std::collections::HashMap<Uuid,(i64,i64)> =
        sqlx::query("SELECT agent_id,COUNT(*) as tot,COUNT(*) FILTER (WHERE rubric IS NULL) as nor FROM eval_test_cases WHERE agent_id=ANY($1) AND is_active=true GROUP BY agent_id")
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get("agent_id"),(r.get::<i64,_>("tot"),r.get::<i64,_>("nor")))).collect();
    // Lifetime run counts measured from `episodes` via
    // `agent_execution_rollup`. `a.total_executions` — what this used to
    // report — is never written by any code path, so the fleet table showed
    // 0 executions for every agent the clinician owns, including ones with
    // hundreds of real runs. Note this is a different number from
    // `eval_runs` below: executions are production runs, eval runs are
    // deliberate tests. See migrations/192 and src/rollup_trust.rs.
    //
    // An agent with no episodes is absent from the rollup, so it falls
    // through to `MeasuredExecStats::default()` — 0 runs, which is the
    // truth rather than an unmeasured zero.
    let exec_stats = crate::agent_economics::measured_exec_stats(&state.db, &agent_ids).await;

    let result: Vec<Value> = agents
        .iter()
        .map(|a| {
            let runs = run_c.get(&a.agent_id).copied().unwrap_or(0);
            let anom = anom_c.get(&a.agent_id).copied().unwrap_or(0);
            let maturity = maturity_from_evolution(fleet_ev.get(&a.agent_id), anom);
            let sc = scores_c
                .get(&a.agent_id)
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let dyads = dyad_c.get(&a.agent_id).copied().unwrap_or(0);
            let (tct, tcnr) = tc_c.get(&a.agent_id).copied().unwrap_or((0, 0));
            let measured = exec_stats.get(&a.agent_id).copied().unwrap_or_default();
            let health: Option<f64> = sc.as_object().and_then(|obj| {
                let v: Vec<f64> = obj.values().filter_map(|x| x.as_f64()).collect();
                if v.is_empty() {
                    None
                } else {
                    Some(v.iter().sum::<f64>() / v.len() as f64)
                }
            });
            serde_json::json!({
                "agent_id": a.agent_name, "agent_name": a.agent_name,
                "agent_type": a.agent_type, "provider": a.llm_provider,
                "persona_version": a.persona_version, "total_executions": measured.executions,
                "eval_runs": runs, "open_anomalies": anom, "dyad_count": dyads,
                "maturity": maturity, "overall_health": health, "latest_scores": sc,
                "care_plan": build_care_plan(runs,tct,tcnr,anom,&maturity,health),
                "last_consolidated_at": a.last_consolidated_at,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({"agents":result})))
}

/// RETIRED: staged an agent by **eval run count**, so 50 runs made it
/// `established` regardless of whether any of them scored well.
///
/// That was a vanity ladder. It rewarded pressing a button, and it is the exact
/// pattern that let 91 consolidation cycles which extracted nothing read as
/// healthy activity. Running two ladders with contradictory rules — one
/// rewarding effort, one rewarding outcomes — is worse than either alone.
///
/// Superseded by [`maturity_from_evolution`], which keeps this clinical
/// vocabulary (the Observatory's badge, buckets, chart, filter and sort all
/// speak it) but derives the stage from earned evolution level instead of
/// attempt count.
#[deprecated(
    note = "staged agents by eval-run count; use maturity_from_evolution, which is outcome-based"
)]
#[allow(dead_code)]
fn maturity_stage(eval_runs: i64, open_anomalies: i64) -> String {
    if open_anomalies > 0 {
        return "flagged".into();
    }
    match eval_runs {
        0 => "newborn",
        1..=4 => "intake",
        5..=19 => "learning",
        20..=49 => "functioning",
        _ => "established",
    }
    .into()
}

/// Clinical stage, derived from the earned evolution level.
///
/// Same vocabulary as the retired `maturity_stage` so every Observatory surface
/// keeps working unchanged, but the input is now what the agent has
/// demonstrated rather than how many times it was poked:
///
/// | stage | means |
/// |---|---|
/// | `flagged` | open unreviewed anomalies — overrides everything |
/// | `newborn` | unranked: no usage data to grade yet |
/// | `intake` | level 1, a first measured outcome |
/// | `learning` | level 2 |
/// | `functioning` | level 3 |
/// | `established` | level 4–5 |
///
/// `flagged` still wins outright: an agent with unreviewed anomalies is a
/// clinical concern whatever it has earned elsewhere.
fn maturity_from_evolution(
    ev: Option<&crate::handlers::evolution::FleetEvolution>,
    open_anomalies: i64,
) -> String {
    if open_anomalies > 0 {
        return "flagged".into();
    }
    let Some(f) = ev else {
        return "newborn".into();
    };
    let badge = crate::handlers::evolution::compute_evolution(f.inputs, f.peak_level);
    if !badge.ranked {
        return "newborn".into();
    }
    match badge.level {
        0 | 1 => "intake",
        2 => "learning",
        3 => "functioning",
        _ => "established",
    }
    .into()
}

fn build_care_plan(
    runs: i64,
    tc_total: i64,
    tc_no_rubric: i64,
    anomalies: i64,
    maturity: &str,
    health: Option<f64>,
) -> String {
    if anomalies > 0 {
        return format!("{} open flag(s) — review in HITL queue.", anomalies);
    }
    if runs == 0 {
        return if tc_total == 0 {
            "No test cases yet. Seed from sample_queries then run baseline eval.".into()
        } else {
            "Test cases ready. Run a baseline eval to activate all evaluators.".into()
        };
    }
    if tc_no_rubric > 0 {
        return format!(
            "{} test case(s) missing rubrics — Sotopia inapplicable. Click ✦ Generate rubrics.",
            tc_no_rubric
        );
    }
    if runs < 5 {
        return format!(
            "{} more eval run(s) to activate LifelongBench (needs ≥5).",
            5 - runs
        );
    }
    if let Some(h) = health {
        if h < 0.5 {
            return "Health below 50% — run a scan to check for drift.".into();
        }
    }
    match maturity {
        "established" => "Well-established. Continue regular evals to maintain calibration.".into(),
        "functioning" => "Functioning well. Consider reviewing dyad relationships.".into(),
        _ => "Continue running evals to build calibration history.".into(),
    }
}

// ─── Dyad endpoints ──────────────────────────────────────────────────────────

pub async fn auto_form_dyads_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    // Check dyad_profiles table exists (migration 133 may not have run yet)
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='dyad_profiles')",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);
    if !table_exists {
        return Ok(Json(serde_json::json!({
            "formed": 0,
            "message": "dyad_profiles table not yet created — migration 133 pending. Will resolve on next deploy.",
        })));
    }
    // Every eligible pair, whether or not a profile already exists. The
    // upsert below refreshes interaction counts on existing profiles, and
    // counting both lets us tell "nothing to do" apart from "already done"
    // in the response — the previous version filtered out existing profiles
    // and then reported "Auto-formed 0", which read as a failure.
    let pairs = sqlx::query(
        r#"SELECT e.agent_id, e.dyad_id, COUNT(*) as cnt,
                  MIN(e.timestamp_ref) as first_at, MAX(e.timestamp_ref) as last_at,
                  EXISTS(SELECT 1 FROM dyad_profiles dp WHERE dp.dyad_id=e.dyad_id) AS existing
           FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
           WHERE a.user_id=$1 AND e.dyad_id IS NOT NULL
           GROUP BY e.agent_id, e.dyad_id HAVING COUNT(*)>=3"#,
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut formed = 0usize;
    let mut refreshed = 0usize;
    let mut skipped_malformed = 0usize;
    for row in &pairs {
        let aid: Uuid = row.get("agent_id");
        let did: String = row.get("dyad_id");
        let cnt: i64 = row.get("cnt");
        let fa: chrono::DateTime<Utc> = row.get("first_at");
        let la: chrono::DateTime<Utc> = row.get("last_at");
        let existing: bool = row.try_get("existing").unwrap_or(false);
        // Skip ids we cannot parse a human out of rather than storing the
        // whole dyad_id as the human_id, which is what the old fallback did.
        let Some(human_id) = agent_bestiary_memory::human_id_from_dyad(&did) else {
            skipped_malformed += 1;
            continue;
        };
        let human_id = human_id.to_string();
        let _ = sqlx::query(
            r#"INSERT INTO dyad_profiles(dyad_id,agent_id,human_id,auto_formed,formed_at,
                first_interaction_at,last_interaction_at,total_interactions)
               VALUES($1,$2,$3,true,NOW(),$4,$5,$6)
               ON CONFLICT(dyad_id) DO UPDATE SET
                 last_interaction_at=EXCLUDED.last_interaction_at,
                 total_interactions=EXCLUDED.total_interactions,updated_at=NOW()"#,
        )
        .bind(&did)
        .bind(aid)
        .bind(&human_id)
        .bind(fa)
        .bind(la)
        .bind(cnt as i32)
        .execute(&state.db)
        .await;
        if existing {
            refreshed += 1;
        } else {
            formed += 1;
        }
    }

    let message = if formed > 0 && refreshed > 0 {
        format!(
            "Auto-formed {} new dyad profile(s); refreshed {}.",
            formed, refreshed
        )
    } else if formed > 0 {
        format!("Auto-formed {} new dyad profile(s).", formed)
    } else if refreshed > 0 {
        format!(
            "No new dyads — {} existing profile(s) already cover every pair with 3+ interactions.",
            refreshed
        )
    } else {
        "No dyads eligible yet — a pair needs 3+ interactions before a profile forms.".to_string()
    };

    Ok(Json(serde_json::json!({
        "formed": formed,
        "refreshed": refreshed,
        "skipped_malformed": skipped_malformed,
        "message": message,
    })))
}

pub async fn agent_relationships_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;

    // A dyad is visible if ANY of three sources knows about it:
    //   1. `dyad_state`   — scored relationship (written by the social pass)
    //   2. `episodes`     — conversations happened, scan may not have run yet
    //   3. `dyad_profiles`— operator named it
    //
    // Previously this handler listed only (1), so a dyad with real
    // conversation history and an operator-assigned name still rendered as
    // "No dyads formed yet" until the social pass had run. Sourcing the id
    // set from the union means history shows up immediately and gains its
    // rapport/trust scores once scanned.
    let dyads = state
        .memory_store
        .list_dyads_for_agent(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let state_by_id: std::collections::HashMap<String, _> =
        dyads.iter().map(|d| (d.dyad_id.clone(), d)).collect();

    // Episode-derived ground truth: how many exchanges actually happened.
    let ep_rows = sqlx::query(
        r#"SELECT dyad_id, COUNT(*)::int AS ep_count,
                  MIN(timestamp_ref) AS first_at, MAX(timestamp_ref) AS last_at
           FROM episodes
           WHERE agent_id = $1 AND dyad_id IS NOT NULL
           GROUP BY dyad_id"#,
    )
    .bind(db_agent.agent_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let episode_stats: std::collections::HashMap<
        String,
        (
            i32,
            Option<chrono::DateTime<Utc>>,
            Option<chrono::DateTime<Utc>>,
        ),
    > = ep_rows
        .iter()
        .map(|r| {
            let did: String = r.get("dyad_id");
            (
                did,
                (
                    r.try_get::<i32, _>("ep_count").unwrap_or(0),
                    r.try_get::<Option<chrono::DateTime<Utc>>, _>("first_at")
                        .ok()
                        .flatten(),
                    r.try_get::<Option<chrono::DateTime<Utc>>, _>("last_at")
                        .ok()
                        .flatten(),
                ),
            )
        })
        .collect();

    let mut dyad_ids: Vec<String> = state_by_id.keys().cloned().collect();
    for k in episode_stats.keys() {
        if !state_by_id.contains_key(k) {
            dyad_ids.push(k.clone());
        }
    }

    // ── Resolve the human half of each dyad to a readable identity ───────────
    //
    // The third segment of a dyad_id is a `users.user_id`, which for every
    // production auth provider is an opaque Zitadel id or an Ethereum address.
    // This handler used to return it as `display_name` whenever the operator
    // had not named the dyad, so the social graph rendered as a wall of
    // `2e644008-f5c7-47c5-854c-3801df9879cc` — technically the truth and
    // practically unreadable, because two cards for the same person look
    // identical until you diff 36 characters by eye.
    //
    // Resolved here rather than in the template because the fallback ladder
    // (display_name > @github > email local-part) already exists on the users
    // handler and must not fork. An id with no matching row is reported as
    // unresolved rather than being dressed up as a name — a deleted or
    // cross-tenant user is a real state and the operator should see it.
    let human_ids: Vec<String> = dyad_ids
        .iter()
        .filter_map(|d| agent_bestiary_memory::human_id_from_dyad(d))
        .map(|s| s.to_string())
        .collect();
    let humans: std::collections::HashMap<String, Value> = if human_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        sqlx::query(
            "SELECT user_id, display_name, github_username, email, avatar_url
               FROM users WHERE user_id = ANY($1)",
        )
        .bind(&human_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| {
            let uid: String = r.get("user_id");
            let display: Option<String> = r.try_get("display_name").ok().flatten();
            let github: Option<String> = r.try_get("github_username").ok().flatten();
            let email: Option<String> = r.try_get("email").ok().flatten();
            // Same ladder as `handlers::users::search_users_handler`.
            let label = display
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| github.clone().map(|g| format!("@{}", g)))
                .or_else(|| {
                    email
                        .as_deref()
                        .and_then(|e| e.split('@').next())
                        .map(|s| s.to_string())
                })
                .filter(|s| !s.trim().is_empty());
            (
                uid.clone(),
                json!({
                    "user_id": uid,
                    "label": label,
                    "github_username": github,
                    "avatar_url": r.try_get::<Option<String>, _>("avatar_url").ok().flatten(),
                }),
            )
        })
        .collect()
    };

    // dyad_profiles may not exist yet (migration 133 pending) — fall back gracefully
    let profiles: std::collections::HashMap<String, serde_json::Value> = if !dyad_ids.is_empty() {
        sqlx::query("SELECT dyad_id,display_name,notes,tags,total_interactions,first_interaction_at,last_interaction_at FROM dyad_profiles WHERE dyad_id=ANY($1)")
        .bind(&dyad_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|{
            let did:String=r.get("dyad_id");
            (did,serde_json::json!({
                "display_name":r.try_get::<Option<String>,_>("display_name").ok().flatten(),
                "notes":r.try_get::<Option<String>,_>("notes").ok().flatten(),
                "tags":r.try_get::<Vec<String>,_>("tags").unwrap_or_default(),
                "total_interactions":r.try_get::<i32,_>("total_interactions").unwrap_or(0),
                "first_interaction_at":r.try_get::<Option<chrono::DateTime<Utc>>,_>("first_interaction_at").ok().flatten(),
                "last_interaction_at":r.try_get::<Option<chrono::DateTime<Utc>>,_>("last_interaction_at").ok().flatten(),
            }))
        }).collect()
    } else {
        std::collections::HashMap::new()
    };

    let mut relationships: Vec<Value> = dyad_ids
        .iter()
        .map(|did| {
            let profile = profiles.get(did).cloned().unwrap_or(serde_json::json!({}));
            let scored = state_by_id.get(did);
            let (ep_count, first_at, last_at) =
                episode_stats.get(did).cloned().unwrap_or((0, None, None));

            let human_id = agent_bestiary_memory::human_id_from_dyad(did)
                .unwrap_or(did.as_str())
                .to_string();
            let human = humans.get(&human_id);
            let human_label = human
                .and_then(|h| h["label"].as_str())
                .map(|s| s.to_string());

            // Precedence: what the operator named it > who the platform says
            // it is > an explicit admission that we do not know. The last case
            // keeps a short id fragment so two unknowns stay distinguishable
            // without pretending the id is a name.
            let display_name = profile["display_name"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string())
                .or_else(|| human_label.clone())
                .unwrap_or_else(|| {
                    format!(
                        "unknown user · {}",
                        human_id.chars().take(8).collect::<String>()
                    )
                });

            // Prefer the episode count — it is ground truth. `dyad_state`
            // only counts episodes the social pass has folded in.
            let episode_count = ep_count.max(scored.map(|d| d.episode_count).unwrap_or(0));

            // Days since the last exchange. The three stored axes only move
            // when an episode arrives, so a relationship that has gone silent
            // keeps whatever scores it last earned — reciprocity in
            // particular would read ~1.0 ("always comes back") for someone
            // who has not been seen in months. Absence is a first-class
            // relationship signal, so it is reported explicitly here rather
            // than being invisible.
            let days_since = last_at.map(|t| (Utc::now() - t).num_seconds() as f64 / 86_400.0);
            let stale = days_since.map(|d| d > 30.0).unwrap_or(false);

            // Only report health when the relationship has actually been
            // scored. Emitting a default 0.5 for unscanned dyads would be
            // indistinguishable from a genuinely neutral relationship.
            let (rapport, trust, reciprocity, health, status) = match scored {
                Some(d) => {
                    let health = (d.rapport + d.trust + d.reciprocity) / 3.0;
                    let status = if stale {
                        // Deliberately outranks "strong": a warm relationship
                        // nobody has touched in a month needs attention more
                        // than a lukewarm active one.
                        "cooling"
                    } else if health >= 0.75 {
                        "strong"
                    } else if health >= 0.5 {
                        "developing"
                    } else if d.episode_count < 3 {
                        "new"
                    } else {
                        "needs_attention"
                    };
                    (
                        Some(d.rapport),
                        Some(d.trust),
                        Some(d.reciprocity),
                        Some((health * 100.0).round() / 100.0),
                        status,
                    )
                }
                None => (None, None, None, None, "pending_scan"),
            };

            serde_json::json!({
                "dyad_id": did,
                "display_name": display_name,
                "human_id": human_id,
                // The resolved identity, kept separate from `display_name` so a
                // UI can show "Operator's label (real person)" without having to
                // guess which of the two it is looking at.
                "human": human.cloned(),
                "human_label": human_label,
                "human_resolved": human.is_some(),
                // True when the only thing we can show is the raw id. Surfaced
                // so the UI can style it as a gap rather than as data.
                "human_unresolved": human.is_none(),
                // `eval` dyads are synthetic history from the regression
                // pipeline; `dyad` dyads are real conversations.
                "origin": if agent_bestiary_memory::is_eval_dyad(did) { "eval" } else { "dyad" },
                "rapport": rapport,
                "trust": trust,
                "reciprocity": reciprocity,
                "episode_count": episode_count,
                "health": health,
                "relationship_status": status,
                "days_since_last_interaction": days_since.map(|d| (d * 10.0).round() / 10.0),
                "stale": stale,
                "first_interaction_at": first_at,
                "last_interaction_at": last_at,
                "last_updated_at": scored.map(|d| d.last_updated_at),
                "profile": profile,
            })
        })
        .collect();

    // Most-recently-active first.
    relationships.sort_by(|a, b| {
        let key = |v: &Value| {
            v["last_interaction_at"]
                .as_str()
                .or_else(|| v["last_updated_at"].as_str())
                .unwrap_or("")
                .to_string()
        };
        key(b).cmp(&key(a))
    });

    let scored_count = relationships
        .iter()
        .filter(|r| r["health"].is_number())
        .count();
    let pending_count = relationships.len() - scored_count;
    let unresolved_humans = relationships
        .iter()
        .filter(|r| r["human_unresolved"] == Value::Bool(true))
        .count();

    Ok(Json(serde_json::json!({
        "agent_id": db_agent.agent_name,
        "relationship_count": relationships.len(),
        "scored_count": scored_count,
        // Surfaced so the UI can say "N awaiting scan" instead of implying
        // the relationships do not exist.
        "pending_scan_count": pending_count,
        // Dyads whose human half matched no `users` row. Non-zero is worth
        // knowing: it means either a deleted account or a dyad written under an
        // id that never was a user.
        "unresolved_human_count": unresolved_humans,
        "relationships": relationships,
    })))
}

pub async fn patch_dyad_profile_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(dyad_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Identity comes from the dyad_id itself, not from a `dyad_profiles` row.
    //
    // This used to authorise against `EXISTS(dyad_profiles JOIN agents ON
    // a.user_id = caller)`, which made the endpoint unusable in exactly the
    // case the UI offers it. `dyad_profiles` rows are only created by
    // `auto_form_dyads_handler`, which filters on `a.user_id = $1` — so a
    // curated agent (owner_id NULL) never gets profile rows at all, and every
    // "Save name" button on a curated agent's relationship card returned
    // "Not the owner of this dyad" no matter who clicked it. Naming a dyad was
    // the one write the social graph offered and it was unreachable for the
    // agents most likely to have a social graph.
    let Some(agent_uuid) = agent_bestiary_memory::agent_id_from_dyad(&dyad_id) else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Malformed dyad_id: expected <origin>:<agent_uuid>:<human_id>".into(),
        ));
    };
    let human_id = agent_bestiary_memory::human_id_from_dyad(&dyad_id).unwrap_or_default();

    // Three ways to have standing: you own the agent, you ARE the human in the
    // relationship, or you are a platform admin.
    let owns_agent: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE agent_id=$1 AND user_id=$2)")
            .bind(agent_uuid)
            .bind(&user_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(false);
    let is_the_human = !human_id.is_empty() && human_id == user_id;
    if !(owns_agent || is_the_human || principal.can_admin()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Naming a dyad requires owning the agent, being the person in the relationship, \
             or platform admin."
                .into(),
        ));
    }

    // Upsert: a dyad with conversation history but no profile row is the normal
    // state, so naming one has to be able to create it. `COALESCE` on update so
    // a PATCH of only `notes` does not wipe the name.
    //
    // An explicit `null` display_name is the UI's "clear the name" and is left
    // to COALESCE (a no-op) deliberately — clearing is not offered, and
    // silently blanking on every notes-only PATCH would be worse.
    sqlx::query(
        r#"INSERT INTO dyad_profiles(dyad_id, agent_id, human_id, display_name, notes,
                                     auto_formed, formed_at)
           VALUES($1, $2, $3, $4, $5, false, NOW())
           ON CONFLICT(dyad_id) DO UPDATE SET
             display_name = COALESCE($4, dyad_profiles.display_name),
             notes        = COALESCE($5, dyad_profiles.notes),
             updated_at   = NOW()"#,
    )
    .bind(&dyad_id)
    .bind(agent_uuid)
    .bind(human_id)
    .bind(body.get("display_name").and_then(|v| v.as_str()))
    .bind(body.get("notes").and_then(|v| v.as_str()))
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("dyad_profiles unavailable (migration pending?): {}", e),
        )
    })?;
    Ok(Json(
        serde_json::json!({"updated": true, "dyad_id": dyad_id}),
    ))
}

// ─── Loop 5a mechanism probe (GET /api/observatory/loops/brier/mechanism) ────
//
// The in-product twin of `scripts/loop5_brier_mechanical_check.sql`. Same nine
// MECHANISM checks, same SQL, same IDs — see that file for the full rationale
// behind each one.
//
// WHY THIS EXISTS SEPARATELY FROM /api/agents/:id/calibration
//
// Calibration answers "what is this agent's score". This answers "is the
// machinery that produced that score actually working". They fail
// independently, and conflating them is what let Loop 5a report itself closed
// while the BrierEvaluator could not see a single forecast.
//
// The distinction that makes this usable on a young loop: MECHANISM checks are
// sample-size independent. They must be clean at n=1. A thin loop is expected
// to produce weak *numbers* (that is `evidence_class` on the calibration
// endpoint) but it must never produce a dropped, duplicated, mis-attributed or
// mis-transformed signal. So a green verdict here means "sound, just needs
// volume" and a red verdict means "do not trust any Loop 5 number yet".
//
// Every check is a bare COUNT with no parameters, so they live in one table and
// run in a loop. Keep the IDs and SQL identical to the .sql probe; if they
// diverge, the two tools will disagree and neither can be trusted.

/// One MECHANISM check: `(id, severity, description, count_sql)`.
/// A non-zero count is a violation.
const LOOP5_MECHANISM_CHECKS: &[(&str, &str, &str, &str)] = &[
    (
        "L5-M01",
        "CRITICAL",
        "Resolved forecasts with an outcome but no brier_score — resolution never scored them, so Loop 5 can never receive a signal for them.",
        "SELECT count(*) FROM fermi_forecasts
          WHERE status='resolved' AND actual_outcome IS NOT NULL AND brier_score IS NULL",
    ),
    (
        "L5-M02",
        "CRITICAL",
        "brier_score not reproducible from the frozen (scored_probability, actual_outcome) pair — mig-174's audit anchor is broken, so no downstream number is verifiable.",
        "SELECT count(*) FROM fermi_forecasts
          WHERE status='resolved' AND brier_score IS NOT NULL AND scored_probability IS NOT NULL
            AND abs(brier_score::float8
                    - power(scored_probability::float8
                            - (CASE WHEN actual_outcome THEN 1.0 ELSE 0.0 END), 2)) > 1e-4",
    ),
    (
        "L5-M03",
        "HIGH",
        "Scored forecasts attributable to no agent at all — the Brier exists but no agent's calibration can ever include it.",
        "SELECT count(*) FROM fermi_forecasts f
          WHERE f.status='resolved' AND f.brier_score IS NOT NULL
            AND NOT EXISTS (
              SELECT 1 FROM jsonb_array_elements(
                       CASE WHEN jsonb_typeof(f.agents_used)='array'
                            THEN f.agents_used ELSE '[]'::jsonb END) e
                JOIN agents a ON a.agent_id::text = e->>'agent_id'
                              OR a.agent_name     = e->>'agent_name'
                              OR a.agent_name     = e->>'name')",
    ),
    (
        "L5-M04",
        "HIGH",
        "Roster entries naming an agent that does not exist — partial credit loss, usually a rename that skipped the agents_used backfill.",
        "SELECT count(*) FROM fermi_forecasts f
          CROSS JOIN LATERAL jsonb_array_elements(
                 CASE WHEN jsonb_typeof(f.agents_used)='array'
                      THEN f.agents_used ELSE '[]'::jsonb END) e
          WHERE f.status='resolved' AND f.brier_score IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM agents a
                             WHERE a.agent_id::text = e->>'agent_id'
                                OR a.agent_name     = e->>'agent_name'
                                OR a.agent_name     = e->>'name')",
    ),
    (
        "L5-M05",
        "CRITICAL",
        "PARTIAL emission: the emitter demonstrably ran for a forecast (some roster agents have signals) but skipped others. Not explainable by backfill history — a genuine drop.",
        "WITH pairs AS (
           SELECT DISTINCT f.id AS forecast_id, a.agent_id
             FROM fermi_forecasts f
             CROSS JOIN LATERAL jsonb_array_elements(
                    CASE WHEN jsonb_typeof(f.agents_used)='array'
                         THEN f.agents_used ELSE '[]'::jsonb END) e
             JOIN agents a ON a.agent_id::text = e->>'agent_id'
                           OR a.agent_name     = e->>'agent_name'
                           OR a.agent_name     = e->>'name'
            WHERE f.status='resolved' AND f.brier_score IS NOT NULL
         ), emitted AS (
           SELECT p.*, EXISTS (
                    SELECT 1 FROM eval_signals s
                     WHERE s.agent_id = p.agent_id
                       AND s.dimension = 'forecast_calibration'
                       AND s.rationale LIKE 'forecast ' || p.forecast_id || ' resolved%'
                  ) AS has_signal
             FROM pairs p
         )
         SELECT count(*) FROM emitted e
          WHERE NOT e.has_signal
            AND EXISTS (SELECT 1 FROM emitted o
                         WHERE o.forecast_id = e.forecast_id AND o.has_signal)",
    ),
    (
        "L5-M06",
        "HIGH",
        "Stored signal score does not equal 1 - clamp(brier) — the inversion, clamp or forecast->signal binding is wrong, and every derived calibration_score is wrong with it.",
        "SELECT count(*)
           FROM eval_signals s
           JOIN fermi_forecasts f ON s.rationale LIKE 'forecast ' || f.id || ' resolved%'
          WHERE s.dimension='forecast_calibration'
            AND f.brier_score IS NOT NULL
            AND abs(s.score - (1.0 - least(greatest(f.brier_score::float8,0.0),1.0))) > 1e-3",
    ),
    (
        "L5-M07",
        "MEDIUM",
        "Signals citing a forecast that is not resolved-and-scored — a signal outlived an un-resolve, void or delete, so the mean averages over evidence that no longer exists.",
        "SELECT count(*) FROM eval_signals s
          WHERE s.dimension='forecast_calibration'
            AND s.evaluator_name='brier_forecast_resolver'
            AND substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})') IS NOT NULL
            AND NOT EXISTS (
              SELECT 1 FROM fermi_forecasts f
               WHERE f.id = substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})')
                 AND f.status='resolved' AND f.brier_score IS NOT NULL)",
    ),
    (
        "L5-M08",
        "MEDIUM",
        "Duplicate (agent, forecast) signals — the emitter's NOT EXISTS guard was bypassed, and each duplicate double-weights one forecast in the calibration mean.",
        "SELECT count(*) FROM (
           SELECT s.agent_id, s.rationale
             FROM eval_signals s
            WHERE s.dimension='forecast_calibration' AND s.rationale LIKE 'forecast %'
            GROUP BY s.agent_id, s.rationale
           HAVING count(*) > 1
         ) x",
    ),
    (
        "L5-M09",
        "HIGH",
        "Forecasts reachable by agent name but not by agent_id — invisible to any reader matching agent_id alone. Non-zero means mig-170's one-shot backfill is stale and grows with every new forecast until the write path stamps agent_id at creation.",
        "WITH per_agent AS (
           SELECT a.agent_id,
                  count(*) FILTER (WHERE f.agents_used @> jsonb_build_array(
                            jsonb_build_object('agent_id', a.agent_id::text))) AS via_id,
                  count(*) AS via_any
             FROM agents a
             JOIN fermi_forecasts f
               ON f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
               OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
               OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name))
            WHERE f.status='resolved' AND f.brier_score IS NOT NULL
            GROUP BY a.agent_id
         )
         SELECT COALESCE(sum(via_any - via_id), 0)::bigint FROM per_agent",
    ),
];

/// INFORMATIONAL context: never a failure, only a measure of how much the
/// signal currently means. Expected to look weak on a young loop.
const LOOP5_INFO_CHECKS: &[(&str, &str, &str)] = &[
    (
        "L5-I01",
        "Scored forecasts with no calibration signal for ANY agent — the backfill backlog (resolved before the emitter shipped), not a bug. Re-emitting these is the cheapest way to thicken Loop 5.",
        "SELECT count(*) FROM fermi_forecasts f
          WHERE f.status='resolved' AND f.brier_score IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM eval_signals s
                             WHERE s.dimension='forecast_calibration'
                               AND s.rationale LIKE 'forecast ' || f.id || ' resolved%')",
    ),
    (
        "L5-I02",
        "Total scored forecasts — the ceiling on all Loop 5 evidence. Per-agent confidence saturates at n=20.",
        "SELECT count(*) FROM fermi_forecasts WHERE status='resolved' AND brier_score IS NOT NULL",
    ),
    (
        "L5-I03",
        "Agents whose attributed forecast set is identical to another agent's. These can never be ranked against each other by Loop 5, at any sample size — a composition that cites every member on every forecast cannot discriminate between its members.",
        "WITH pairs AS (
           SELECT DISTINCT f.id AS forecast_id, a.agent_id
             FROM fermi_forecasts f
             CROSS JOIN LATERAL jsonb_array_elements(
                    CASE WHEN jsonb_typeof(f.agents_used)='array'
                         THEN f.agents_used ELSE '[]'::jsonb END) e
             JOIN agents a ON a.agent_id::text = e->>'agent_id'
                           OR a.agent_name     = e->>'agent_name'
                           OR a.agent_name     = e->>'name'
            WHERE f.status='resolved' AND f.brier_score IS NOT NULL
         ), fingerprints AS (
           SELECT agent_id, md5(string_agg(forecast_id, ',' ORDER BY forecast_id)) AS fp
             FROM pairs GROUP BY agent_id
         )
         SELECT count(*) FROM fingerprints f
          WHERE EXISTS (SELECT 1 FROM fingerprints o
                         WHERE o.fp = f.fp AND o.agent_id <> f.agent_id)",
    ),
    (
        "L5-I04",
        "Agents whose outcome set is so one-sided that the base-rate baseline b(1-b) is under 0.01. On such a set a zero-knowledge forecaster still scores ~99%, so only brier_skill_score is meaningful.",
        "WITH pairs AS (
           SELECT DISTINCT f.id AS forecast_id, f.actual_outcome, a.agent_id
             FROM fermi_forecasts f
             CROSS JOIN LATERAL jsonb_array_elements(
                    CASE WHEN jsonb_typeof(f.agents_used)='array'
                         THEN f.agents_used ELSE '[]'::jsonb END) e
             JOIN agents a ON a.agent_id::text = e->>'agent_id'
                           OR a.agent_name     = e->>'agent_name'
                           OR a.agent_name     = e->>'name'
            WHERE f.status='resolved' AND f.brier_score IS NOT NULL
         )
         SELECT count(*) FROM (
           SELECT agent_id, avg(CASE WHEN actual_outcome THEN 1.0 ELSE 0.0 END) AS b
             FROM pairs GROUP BY agent_id
         ) x WHERE b * (1.0 - b) < 0.01",
    ),
];

// ─── Agent-scoped MECHANISM checks ───────────────────────────────────────────
//
// The fleet probe above aggregates across every tenant's forecasts, which makes
// it admin-only and makes its verdict a poor answer to the question an agent
// owner actually has: "is MY loop broken, or just young?" A fleet `broken`
// caused by someone else's orphaned forecast would have shown up on this
// agent's row, which is exactly the kind of unattributable claim the loops
// endpoint exists to eliminate.
//
// THE MAINTENANCE CONTRACT — read before editing either table
//
// These are the SAME nine checks with the SAME ids and severities, restricted
// to one agent. `scripts/loop5_brier_mechanical_check.sql` is the third copy.
// If the three disagree, none of them can be trusted, so:
//
//   * ids and severities must match `LOOP5_MECHANISM_CHECKS` exactly. Enforced
//     by `agent_and_fleet_checks_declare_the_same_ids` below, not by discipline.
//   * the roster predicate appears ONCE, as `ROSTER_PREDICATE`, and is spliced
//     in via `{ROSTER}`. Eight checks need it; writing it eight times is how it
//     drifts from `eval_brier.rs::latest_for_agent` and `/calibration`.
//   * a check that cannot be attributed to an agent is declared unscopable
//     rather than quietly dropped. Silently omitting it would make a scoped
//     "all clean" mean less than it appears to.
//
// The three-shape join (`agent_id` | `agent_name` | `name`) mirrors
// `src/handlers/eval_brier.rs::latest_for_agent` and
// `src/handlers/agents.rs::get_agent_calibration_handler`. Change one, change
// all of them.
const ROSTER_PREDICATE: &str = "EXISTS (
           SELECT 1 FROM jsonb_array_elements(
                    CASE WHEN jsonb_typeof(f.agents_used)='array'
                         THEN f.agents_used ELSE '[]'::jsonb END) re
             JOIN agents ra ON ra.agent_id::text = re->>'agent_id'
                            OR ra.agent_name     = re->>'agent_name'
                            OR ra.agent_name     = re->>'name'
            WHERE ra.agent_id = $1)";

/// One agent-scoped check: `(id, severity, count_sql)`.
///
/// `count_sql` takes `$1` = agent uuid and may contain `{ROSTER}`. A non-zero
/// count is a violation *attributable to this agent*.
type AgentCheck = (&'static str, &'static str, &'static str);

/// Checks that cannot be scoped to an agent, with the reason. Declared so a
/// scoped verdict can say what it did not cover instead of overclaiming.
const LOOP5_UNSCOPABLE: &[(&str, &str)] = &[(
    "L5-M03",
    "Counts scored forecasts attributable to NO agent at all. Being unattributable is \
     the definition of the fault, so by construction it cannot be filed under any \
     agent — it is only visible in the fleet probe.",
)];

const LOOP5_AGENT_CHECKS: &[AgentCheck] = &[
    (
        "L5-M01",
        "CRITICAL",
        "SELECT count(*) FROM fermi_forecasts f
          WHERE f.status='resolved' AND f.actual_outcome IS NOT NULL AND f.brier_score IS NULL
            AND {ROSTER}",
    ),
    (
        "L5-M02",
        "CRITICAL",
        "SELECT count(*) FROM fermi_forecasts f
          WHERE f.status='resolved' AND f.brier_score IS NOT NULL
            AND f.scored_probability IS NOT NULL
            AND abs(f.brier_score::float8
                    - power(f.scored_probability::float8
                            - (CASE WHEN f.actual_outcome THEN 1.0 ELSE 0.0 END), 2)) > 1e-4
            AND {ROSTER}",
    ),
    // L5-M03 is fleet-only — see LOOP5_UNSCOPABLE.
    (
        "L5-M04",
        "HIGH",
        // This agent's own forecasts whose roster ALSO names an agent that does
        // not exist. The credit lost is lost from a forecast it participated in.
        "SELECT count(*) FROM fermi_forecasts f
          CROSS JOIN LATERAL jsonb_array_elements(
                 CASE WHEN jsonb_typeof(f.agents_used)='array'
                      THEN f.agents_used ELSE '[]'::jsonb END) e
          WHERE f.status='resolved' AND f.brier_score IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM agents a
                             WHERE a.agent_id::text = e->>'agent_id'
                                OR a.agent_name     = e->>'agent_name'
                                OR a.agent_name     = e->>'name')
            AND {ROSTER}",
    ),
    (
        "L5-M05",
        "CRITICAL",
        // The CTE stays fleet-wide on purpose: "did the emitter run for this
        // forecast at all" can only be answered by looking at the other roster
        // members. Only the counted row is scoped to $1.
        "WITH pairs AS (
           SELECT DISTINCT f.id AS forecast_id, a.agent_id
             FROM fermi_forecasts f
             CROSS JOIN LATERAL jsonb_array_elements(
                    CASE WHEN jsonb_typeof(f.agents_used)='array'
                         THEN f.agents_used ELSE '[]'::jsonb END) e
             JOIN agents a ON a.agent_id::text = e->>'agent_id'
                           OR a.agent_name     = e->>'agent_name'
                           OR a.agent_name     = e->>'name'
            WHERE f.status='resolved' AND f.brier_score IS NOT NULL
         ), emitted AS (
           SELECT p.*, EXISTS (
                    SELECT 1 FROM eval_signals s
                     WHERE s.agent_id = p.agent_id
                       AND s.dimension = 'forecast_calibration'
                       AND s.rationale LIKE 'forecast ' || p.forecast_id || ' resolved%'
                  ) AS has_signal
             FROM pairs p
         )
         SELECT count(*) FROM emitted e
          WHERE NOT e.has_signal AND e.agent_id = $1
            AND EXISTS (SELECT 1 FROM emitted o
                         WHERE o.forecast_id = e.forecast_id AND o.has_signal)",
    ),
    (
        "L5-M06",
        "HIGH",
        "SELECT count(*)
           FROM eval_signals s
           JOIN fermi_forecasts f ON s.rationale LIKE 'forecast ' || f.id || ' resolved%'
          WHERE s.dimension='forecast_calibration' AND s.agent_id = $1
            AND f.brier_score IS NOT NULL
            AND abs(s.score - (1.0 - least(greatest(f.brier_score::float8,0.0),1.0))) > 1e-3",
    ),
    (
        "L5-M07",
        "MEDIUM",
        "SELECT count(*) FROM eval_signals s
          WHERE s.dimension='forecast_calibration' AND s.agent_id = $1
            AND s.evaluator_name='brier_forecast_resolver'
            AND substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})') IS NOT NULL
            AND NOT EXISTS (
              SELECT 1 FROM fermi_forecasts f
               WHERE f.id = substring(s.rationale from 'forecast ([0-9a-fA-F-]{36})')
                 AND f.status='resolved' AND f.brier_score IS NOT NULL)",
    ),
    (
        "L5-M08",
        "MEDIUM",
        "SELECT count(*) FROM (
           SELECT s.rationale
             FROM eval_signals s
            WHERE s.dimension='forecast_calibration' AND s.agent_id = $1
              AND s.rationale LIKE 'forecast %'
            GROUP BY s.rationale
           HAVING count(*) > 1
         ) x",
    ),
    (
        "L5-M09",
        "HIGH",
        // The fleet check sums (via_any - via_id) over every agent; restricted
        // to one agent that is just the count of its own name-only forecasts.
        "SELECT count(*) FROM fermi_forecasts f, agents a
          WHERE a.agent_id = $1
            AND f.status='resolved' AND f.brier_score IS NOT NULL
            AND (f.agents_used @> jsonb_build_array(
                     jsonb_build_object('agent_name', a.agent_name))
              OR f.agents_used @> jsonb_build_array(
                     jsonb_build_object('name', a.agent_name)))
            AND NOT (f.agents_used @> jsonb_build_array(
                     jsonb_build_object('agent_id', a.agent_id::text)))",
    ),
];

/// Outcome of the MECHANISM probe: whether the Loop 5a chain moves a signal
/// correctly, independent of whether the resulting numbers are impressive.
///
/// Extracted from the handler so the per-agent loops endpoint can reach the same
/// verdict from the same SQL. A second implementation would let the fleet probe
/// and the per-agent row disagree about whether the wiring works, and then
/// neither could be trusted — the failure this module's header warns about.
pub struct Loop5Mechanism {
    /// `sound` | `broken` | `inconclusive`
    pub verdict: &'static str,
    pub violations: usize,
    pub ok: usize,
    pub errored: usize,
    pub checks: Vec<Value>,
}

impl Loop5Mechanism {
    /// Only the MECHANISM checks that actually failed, for callers that want to
    /// name the fault without carrying every row.
    ///
    /// `NOT_SCOPABLE` is excluded deliberately: a check that cannot be filed
    /// under this agent is not a finding against it. It is reported separately
    /// by [`Loop5Mechanism::not_scopable`] so it stays visible without being
    /// counted as a fault.
    pub fn failing(&self) -> Vec<Value> {
        self.checks
            .iter()
            .filter(|c| {
                c["class"] == "MECHANISM" && c["status"] != "OK" && c["status"] != "NOT_SCOPABLE"
            })
            .cloned()
            .collect()
    }

    /// Checks this probe could not attribute to the subject, with the reason.
    pub fn not_scopable(&self) -> Vec<Value> {
        self.checks
            .iter()
            .filter(|c| c["status"] == "NOT_SCOPABLE")
            .cloned()
            .collect()
    }

    pub fn verdict_detail(&self) -> &'static str {
        match self.verdict {
            "sound" => {
                "The chain moves signals correctly. Any weakness in the numbers is thin data \
                 or skew, not wiring."
            }
            "broken" => {
                "Do not trust any Loop 5 number until the MECHANISM violations are resolved."
            }
            _ => "A probe query errored — fix it before drawing conclusions either way.",
        }
    }
}

/// Run every MECHANISM + INFO check and return the structured verdict.
pub async fn probe_loop5_mechanism(db: &sqlx::PgPool) -> Loop5Mechanism {
    let mut checks: Vec<Value> = Vec::new();
    let mut violations = 0usize;
    let mut ok = 0usize;
    let mut errored = 0usize;

    for (id, severity, description, sql) in LOOP5_MECHANISM_CHECKS {
        // A failing probe query must never 500 the probe: an errored check is
        // itself a finding, and reporting it as INCONCLUSIVE is more honest
        // than either hiding it or pretending the loop is sound.
        match sqlx::query_scalar::<_, i64>(sql).fetch_one(db).await {
            Ok(n) => {
                let status = if n == 0 { "OK" } else { "VIOLATION" };
                if n == 0 {
                    ok += 1;
                } else {
                    violations += 1;
                }
                checks.push(json!({
                    "id": id, "class": "MECHANISM", "severity": severity,
                    "status": status, "count": n, "description": description,
                }));
            }
            Err(e) => {
                errored += 1;
                checks.push(json!({
                    "id": id, "class": "MECHANISM", "severity": severity,
                    "status": "ERROR", "count": null, "description": description,
                    "error": e.to_string(),
                }));
            }
        }
    }

    for (id, description, sql) in LOOP5_INFO_CHECKS {
        match sqlx::query_scalar::<_, i64>(sql).fetch_one(db).await {
            Ok(n) => checks.push(json!({
                "id": id, "class": "INFO", "severity": "INFO",
                "status": "OK", "count": n, "description": description,
            })),
            Err(e) => {
                errored += 1;
                checks.push(json!({
                    "id": id, "class": "INFO", "severity": "INFO",
                    "status": "ERROR", "count": null, "description": description,
                    "error": e.to_string(),
                }));
            }
        }
    }

    let verdict = if errored > 0 {
        "inconclusive"
    } else if violations == 0 {
        "sound"
    } else {
        "broken"
    };

    Loop5Mechanism {
        verdict,
        violations,
        ok,
        errored,
        checks,
    }
}

/// Run the MECHANISM checks restricted to one agent.
///
/// Owner-safe by construction: every count is filtered to forecasts this agent
/// is on the roster of, or to signals carrying its `agent_id`. That is what lets
/// the per-agent loops endpoint answer "is MY wiring broken" without the
/// admin gate the fleet probe needs.
///
/// The description text is reused from `LOOP5_MECHANISM_CHECKS` by id, so the
/// two tables cannot describe the same check differently.
pub async fn probe_loop5_mechanism_for_agent(db: &sqlx::PgPool, agent_id: Uuid) -> Loop5Mechanism {
    let describe = |id: &str| -> &'static str {
        LOOP5_MECHANISM_CHECKS
            .iter()
            .find(|(cid, ..)| *cid == id)
            .map(|(_, _, d, _)| *d)
            .unwrap_or("(no description — id missing from LOOP5_MECHANISM_CHECKS)")
    };

    let mut checks: Vec<Value> = Vec::new();
    let (mut violations, mut ok, mut errored) = (0usize, 0usize, 0usize);

    for (id, severity, sql) in LOOP5_AGENT_CHECKS {
        let scoped = sql.replace("{ROSTER}", ROSTER_PREDICATE);
        match sqlx::query_scalar::<_, i64>(&scoped)
            .bind(agent_id)
            .fetch_one(db)
            .await
        {
            Ok(n) => {
                if n == 0 {
                    ok += 1;
                } else {
                    violations += 1;
                }
                checks.push(json!({
                    "id": id, "class": "MECHANISM", "severity": severity,
                    "status": if n == 0 { "OK" } else { "VIOLATION" },
                    "count": n, "scope": "agent",
                    "description": describe(id),
                }));
            }
            Err(e) => {
                errored += 1;
                checks.push(json!({
                    "id": id, "class": "MECHANISM", "severity": severity,
                    "status": "ERROR", "count": null, "scope": "agent",
                    "description": describe(id), "error": e.to_string(),
                }));
            }
        }
    }

    // Declared, not dropped. A scoped "all clean" that silently omitted a check
    // would claim more than it measured.
    for (id, why) in LOOP5_UNSCOPABLE {
        checks.push(json!({
            "id": id, "class": "MECHANISM", "severity": "HIGH",
            "status": "NOT_SCOPABLE", "count": null, "scope": "fleet_only",
            "description": describe(id), "why_unscopable": why,
        }));
    }

    let verdict = if errored > 0 {
        "inconclusive"
    } else if violations == 0 {
        "sound"
    } else {
        "broken"
    };

    Loop5Mechanism {
        verdict,
        violations,
        ok,
        errored,
        checks,
    }
}

/// GET /api/observatory/loops/brier/mechanism
///
/// Structured verdict on whether the Loop 5a chain moves a signal correctly,
/// independent of whether the resulting numbers are impressive.
///
/// Admin-only: the checks aggregate across every tenant's forecasts, so the
/// counts are not owner-scoped and must not leak to a normal caller.
pub async fn loop5_mechanism_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    if !principal.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Loop 5 mechanism probe is admin-only (counts span all tenants)".into(),
        ));
    }

    let m = probe_loop5_mechanism(&state.db).await;

    Ok(Json(json!({
        "loop": "5a",
        "label": "Brier calibration",
        "verdict": m.verdict,
        "verdict_detail": m.verdict_detail(),
        "mechanism_violations": m.violations,
        "mechanism_ok": m.ok,
        "errored": m.errored,
        "checks": m.checks,
        "note": "MECHANISM checks are sample-size independent and must be clean at n=1. INFO checks measure how much the signal currently means and are never failures. Full rationale per check: scripts/loop5_brier_mechanical_check.sql",
    })))
}

// ─── The two axes of Loop 5 health ───────────────────────────────────
//
// MECHANISM and EVIDENCE fail independently and have opposite remedies, so they
// must never be collapsed into one status:
//
//   thin   — the chain is correct, there is just not much data through it yet.
//            Remedy: wait. Nothing is wrong. More volume fixes it by itself.
//   broken — the chain drops, duplicates, mis-attributes or mis-transforms
//            signals. Remedy: repair the wiring. More volume makes it WORSE,
//            because every new forecast is scored through the same fault.
//
// Reporting both as `partial` — which is what a single status forces — tells an
// operator to be patient when they should be debugging.

/// Evidence sufficiency, i.e. how much the number currently means.
fn evidence_band(n_resolved: i64, evidence_class: &str) -> &'static str {
    // Prefer the calibration endpoint's own classification; fall back to n so
    // this never reports `usable` for an empty set.
    match evidence_class {
        "usable" if n_resolved > 0 => "usable",
        "thin" | "provisional" | "none" => evidence_class_static(evidence_class),
        _ if n_resolved == 0 => "none",
        _ if n_resolved < 5 => "provisional",
        _ if n_resolved < 20 => "thin",
        _ => "usable",
    }
}

fn evidence_class_static(s: &str) -> &'static str {
    match s {
        "usable" => "usable",
        "thin" => "thin",
        "provisional" => "provisional",
        _ => "none",
    }
}

/// The sentence an operator needs: is this thin, or is it broken?
///
/// `mechanism` is one of `sound` | `broken` | `inconclusive`, as returned by
/// [`probe_loop5_mechanism_for_agent`]. There is deliberately no "could not
/// check" arm any more: the agent-scoped probe needs no admin gate, so every
/// caller who can see the agent can see whether its wiring works.
fn loop5_interpretation(mechanism: &str, evidence: &str) -> &'static str {
    match (mechanism, evidence) {
        ("broken", _) => {
            "BROKEN, not thin. The chain that produces this number drops, duplicates or \
             mis-transforms signals, so the score is not a measurement of calibration at all. \
             More forecasts will not help — each new one is scored through the same fault. \
             Repair the wiring first."
        }
        ("inconclusive", _) => {
            "UNKNOWN. A mechanism probe query errored, so soundness could not be established \
             either way. Fix the probe before reading anything into the number."
        }
        ("sound", "none") => {
            "SOUND but empty. The wiring is verified correct and no forecast has resolved \
             through it yet. Nothing to fix; nothing to read."
        }
        ("sound", "provisional") | ("sound", "thin") => {
            "THIN, not broken. The wiring is verified correct — signals are emitted once, \
             attributed to the right agent and transformed correctly — there is simply not \
             enough resolved history for the number to mean much yet. This improves with \
             volume alone. Nothing needs fixing."
        }
        ("sound", _) => {
            "SOUND and sufficient. The wiring is verified correct and enough forecasts have \
             resolved for the number to be treated as a real measurement."
        }
        _ => "Mechanism state unrecognised.",
    }
}

// ─── GET /api/observatory/agents/:agent_id/loops ──────────────────────────
//
// Per-agent RSI feedback-loop health.
//
// WHY THIS IS A SERVER ENDPOINT AND NOT TEMPLATE JAVASCRIPT
//
// The observatory's Loops tab used to assemble these six verdicts client-side
// from whatever payloads the page happened to have already fetched, and two of
// them were not derived from anything at all:
//
//   3a Coherence (inner)     status: "partial"  — hardcoded, every agent
//   4  Composition evolution status: "open"     — hardcoded, every agent
//
// Those two rendered identically for an agent in six actively-evaluated
// workspaces and for an agent that has never been in one. A constant presented
// in a column headed by a live status glyph is worse than an empty column,
// because it is indistinguishable from a measurement.
//
// Loop 1a was worse in a subtler way: it reported `closed` on
// `eval_runs > 0`. Eval runs are the *signal* half of Loop 1. The correction
// half is consolidation — the loop is only closed when something was learned
// and written back to the ontology. So an agent with 140 eval runs and zero
// dreaming cycles reported a closed learning loop while having never learned
// anything, which is the exact failure the dreaming-maturity work was built to
// expose.
//
// THE FOURTH STATUS
//
// `open` is a measurement: the loop is not turning. It is not the right word
// for "this surface cannot tell". Conflating them is how a missing table
// becomes a confident negative verdict, so a query that fails returns
// `unmeasured` naming the failure, and every loop carries the `source` it was
// derived from so the operator can go check it directly.

/// Inputs to the Loop 1 verdict, split along the line that matters: what the
/// agent was *told* about itself versus what it *did* with that.
///
/// A struct with a pure classifier rather than inline logic, for the same reason
/// `dreaming_maturity::classify_maturity` is one: the interesting cases are
/// combinations that are awkward to reach through a live database, and the
/// regression this fixes (many eval runs, no consolidation, reported `closed`)
/// is exactly such a case.
#[derive(Debug, Clone, Copy, Default)]
pub struct Loop1aInputs {
    /// Signal half — the agent was scored.
    pub eval_runs: i64,
    pub eval_signals: i64,
    pub dimensions: i64,
    /// Correction half — the scores were consolidated into durable knowledge.
    pub completed_cycles: i64,
    pub failed_cycles: i64,
    pub entities: i64,
    pub facts: i64,
    pub rules: i64,
    /// Context, so "nothing happened" can be told from "nothing could happen".
    pub episodes: i64,
    pub backlog: i64,
    /// This agent is invoked as a service rather than executed as an agent, so
    /// an empty `episodes` count does not mean it has never run.
    ///
    /// The ontologist is the case that forced this. It is handed to the
    /// consolidation worker as a bare `LLMProvider` — its card supplies a
    /// model and a credential, and no agent execution ever happens — while
    /// everything it extracts is stamped with the SUBJECT agent's id. So it
    /// powered Loop 1 for the entire fleet and reported, correctly from the
    /// tables and absurdly in substance, "no episodes — execute it first".
    pub off_ledger_service: bool,
}

impl Loop1aInputs {
    pub fn ontology_rows(&self) -> i64 {
        self.entities + self.facts + self.rules
    }
    /// The agent has been measured.
    pub fn has_signal(&self) -> bool {
        self.eval_runs > 0 || self.eval_signals > 0
    }
    /// Something was written back. Requires *yield*, not just a cycle: a
    /// consolidation run that extracted nothing corrected nothing, and 91 such
    /// cycles on this deployment reported success while leaving 62 agents with
    /// an empty ontology.
    pub fn has_correction(&self) -> bool {
        self.completed_cycles > 0 && self.ontology_rows() > 0
    }
}

/// Classify Loop 1, and say which half is missing.
///
/// The rule the old client-side version got wrong: eval runs are the SIGNAL
/// half. A loop with a signal and no correction is half a loop, and calling it
/// `closed` is how an agent with 140 eval runs and no ontology reported a
/// closed learning loop.
pub fn classify_loop1a(i: Loop1aInputs) -> (&'static str, String) {
    let (runs, dims, cycles) = (i.eval_runs, i.dimensions, i.completed_cycles);
    let onto = i.ontology_rows();

    match (i.has_signal(), i.has_correction()) {
        (true, true) => (
            "closed",
            format!(
                "{runs} eval run(s) · {dims} dimension(s) scored, and {cycles} dreaming \
                 cycle(s) wrote back {onto} ontology row(s). Both halves turning."
            ),
        ),
        (true, false) if cycles == 0 => (
            "partial",
            format!(
                "Signal half only: {runs} eval run(s) over {dims} dimension(s), but the agent \
                 has never dreamt{}. Nothing has been written back, so no correction has \
                 occurred — run a consolidation cycle to close it.",
                if i.backlog > 0 {
                    format!(" ({} episode(s) waiting)", i.backlog)
                } else {
                    String::new()
                }
            ),
        ),
        (true, false) => (
            "partial",
            format!(
                "Signal half only: {runs} eval run(s), and {cycles} dreaming cycle(s) completed \
                 but extracted nothing — 0 entities, facts or rules. The loop is running on \
                 real material and learning nothing."
            ),
        ),
        (false, true) => (
            "partial",
            format!(
                "Correction half only: {cycles} dreaming cycle(s) produced {onto} ontology \
                 row(s), but no eval run has ever scored this agent — it is consolidating \
                 without any measure of whether it improved."
            ),
        ),
        // An off-ledger service with no episodes is not idle — the ledger is
        // incomplete. `unmeasured`, not `open`: `open` asserts the loop is not
        // turning, and here we simply cannot see whether it is.
        (false, false) if i.episodes == 0 && i.off_ledger_service => (
            "unmeasured",
            "This agent runs as a dreaming-pipeline service, not as an ordinary agent — the \
             consolidation worker calls it directly and files everything it produces under \
             the agent being dreamt. Its work before the dream ledger was never recorded, so \
             its learning cannot be read from these tables. Episodes accumulate from its next \
             cycle onward."
                .to_string(),
        ),
        (false, false) if i.episodes == 0 => (
            "open",
            "Neither half. The agent has produced no episodes, so there is nothing to score \
             and nothing to consolidate — execute it first."
                .to_string(),
        ),
        (false, false) => (
            "open",
            format!(
                "Neither half. {} episode(s) exist, no eval runs and no completed dreaming \
                 cycles{}.",
                i.episodes,
                if i.failed_cycles > 0 {
                    format!(" ({} cycle(s) failed)", i.failed_cycles)
                } else {
                    String::new()
                }
            ),
        ),
    }
}

/// Assemble one loop verdict. `source` names the tables or endpoint the verdict
/// was derived from — the point is that no row on this tab is unattributable.
fn loop_verdict(
    id: &str,
    name: &str,
    scope: &str,
    status: &str,
    detail: String,
    source: &str,
    evidence: Value,
) -> Value {
    json!({
        "id": id,
        "name": name,
        // agent | workspace | composition — which object the loop actually runs
        // on. A workspace-scoped loop reported against an agent is reported
        // across every workspace the agent belongs to, and saying so stops the
        // number being read as the agent's own.
        "scope": scope,
        // closed | partial | open | unmeasured
        "status": status,
        "detail": detail,
        "source": source,
        "evidence": evidence,
    })
}

pub async fn agent_loops_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;
    let aid = db_agent.agent_id;
    let db = &state.db;

    let mut loops: Vec<Value> = Vec::new();

    // ── Loop 1 — individual agent learning ──────────────────────────────
    //
    // Both halves in one query so they cannot be reported out of step with
    // each other: `signal_*` is what the agent was told about itself,
    // `cycles`/`ontology_size` is what it did with that.
    let l1 = sqlx::query(
        "SELECT (SELECT COUNT(*) FROM eval_runs WHERE agent_id=$1)                  AS runs,
                (SELECT COUNT(*) FROM eval_signals WHERE agent_id=$1)               AS signals,
                (SELECT COUNT(DISTINCT dimension) FROM eval_signals WHERE agent_id=$1) AS dims,
                (SELECT COUNT(*) FROM consolidation_jobs
                  WHERE agent_id=$1 AND status='completed')                         AS cycles,
                (SELECT COUNT(*) FROM consolidation_jobs
                  WHERE agent_id=$1 AND status='failed')                            AS failed_cycles,
                (SELECT COUNT(*) FROM entities WHERE agent_id=$1)                   AS entities,
                (SELECT COUNT(*) FROM facts WHERE agent_id=$1)                      AS facts,
                (SELECT COUNT(*) FROM semantic_rules WHERE agent_id=$1)             AS rules,
                (SELECT COUNT(*) FROM episodes WHERE agent_id=$1)                   AS episodes,
                (SELECT COUNT(*) FROM episodes
                  WHERE agent_id=$1 AND NOT consolidated)                           AS backlog",
    )
    .bind(aid)
    .fetch_one(db)
    .await;

    match l1 {
        Ok(r) => {
            let n = |k: &str| -> i64 { r.try_get::<i64, _>(k).unwrap_or(0) };
            let i = Loop1aInputs {
                eval_runs: n("runs"),
                eval_signals: n("signals"),
                dimensions: n("dims"),
                completed_cycles: n("cycles"),
                failed_cycles: n("failed_cycles"),
                entities: n("entities"),
                facts: n("facts"),
                rules: n("rules"),
                episodes: n("episodes"),
                backlog: n("backlog"),
                off_ledger_service: crate::handlers::consolidation::is_dream_pipeline_member(
                    &state,
                    &db_agent.agent_name,
                ),
            };
            let (status, detail) = classify_loop1a(i);

            loops.push(loop_verdict(
                "1a",
                "Individual learning",
                "agent",
                status,
                detail,
                "eval_runs + eval_signals (signal) · consolidation_jobs + entities/facts/semantic_rules (correction)",
                json!({
                    "eval_runs": i.eval_runs, "eval_signals": i.eval_signals,
                    "dimensions": i.dimensions,
                    "completed_cycles": i.completed_cycles, "failed_cycles": i.failed_cycles,
                    "entities": i.entities, "facts": i.facts, "semantic_rules": i.rules,
                    "ontology_rows": i.ontology_rows(),
                    "episodes": i.episodes, "unconsolidated_episodes": i.backlog,
                    "has_signal_half": i.has_signal(),
                    "has_correction_half": i.has_correction(),
                    "off_ledger_service": i.off_ledger_service,
                }),
            ));
        }
        Err(e) => loops.push(loop_verdict(
            "1a",
            "Individual learning",
            "agent",
            "unmeasured",
            format!("Could not read the learning tables: {e}"),
            "eval_runs + eval_signals + consolidation_jobs",
            Value::Null,
        )),
    }

    // ── Loop 2 — HITL behavioural correction ─────────────────────────────
    //
    // "No anomalies detected" is genuinely ambiguous — a well-behaved agent and
    // an agent nothing has ever scanned look the same from this table — so the
    // scan count disambiguates it instead of leaving the operator to guess.
    let l2 = sqlx::query(
        "SELECT COUNT(*)                                                        AS total,
                COUNT(*) FILTER (WHERE requires_review AND resolved_at IS NULL)  AS pending,
                COUNT(*) FILTER (WHERE resolved_at IS NOT NULL)                  AS resolved,
                MIN(created_at) FILTER (WHERE requires_review AND resolved_at IS NULL)
                                                                                 AS oldest_pending
           FROM anomaly_events WHERE agent_id = $1",
    )
    .bind(aid)
    .fetch_one(db)
    .await;

    match l2 {
        Ok(r) => {
            let n = |k: &str| -> i64 { r.try_get::<i64, _>(k).unwrap_or(0) };
            let (total, pending, resolved) = (n("total"), n("pending"), n("resolved"));
            let oldest = r
                .try_get::<Option<chrono::DateTime<Utc>>, _>("oldest_pending")
                .ok()
                .flatten();
            let status = if resolved > 0 && pending == 0 {
                "closed"
            } else if pending > 0 {
                "partial"
            } else {
                "open"
            };
            let detail = if pending > 0 {
                let age = oldest
                    .map(|t| {
                        format!(
                            ", oldest {} day(s) old",
                            ((Utc::now() - t).num_seconds() / 86_400).max(0)
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "{pending} anomal(ies) awaiting review{age}. {resolved} previously \
                     resolved — visit the review queue."
                )
            } else if resolved > 0 {
                format!("{resolved} anomal(ies) reviewed and resolved, none outstanding.")
            } else {
                "No anomalies have ever been raised for this agent. Nothing to correct — which \
                 means the loop has not been exercised, not that it works."
                    .to_string()
            };
            loops.push(loop_verdict(
                "2",
                "HITL correction",
                "agent",
                status,
                detail,
                "anomaly_events",
                json!({
                    "total": total, "pending_review": pending, "resolved": resolved,
                    "oldest_pending_at": oldest,
                }),
            ));
        }
        Err(e) => loops.push(loop_verdict(
            "2",
            "HITL correction",
            "agent",
            "unmeasured",
            format!("Could not read anomaly_events: {e}"),
            "anomaly_events",
            Value::Null,
        )),
    }

    // ── Loop 3a — workspace coherence (inner) ────────────────────────────
    //
    // Workspace-scoped, and therefore measurable per-agent only as "across the
    // workspaces this agent is in". That was the excuse for hardcoding it; it
    // is not a good one, because the join is two tables wide.
    let l3 = sqlx::query(
        "SELECT COUNT(DISTINCT wa.workspace_id)                     AS workspaces,
                COUNT(DISTINCT ce.workspace_id)                     AS evaluated_workspaces,
                COUNT(ce.eval_id)                                   AS evaluations,
                MAX(ce.created_at)                                  AS last_eval_at,
                AVG(ce.global_score)                                AS mean_score
           FROM workspace_agents wa
           LEFT JOIN coherence_evaluations ce ON ce.workspace_id = wa.workspace_id
          WHERE wa.agent_id = $1",
    )
    .bind(aid)
    .fetch_one(db)
    .await;

    match l3 {
        Ok(r) => {
            let n = |k: &str| -> i64 { r.try_get::<i64, _>(k).unwrap_or(0) };
            let (ws, evaluated, evals) =
                (n("workspaces"), n("evaluated_workspaces"), n("evaluations"));
            let last = r
                .try_get::<Option<chrono::DateTime<Utc>>, _>("last_eval_at")
                .ok()
                .flatten();
            let mean = r.try_get::<Option<f64>, _>("mean_score").ok().flatten();

            // A coherence score below 0.4 is the documented chronic-low band,
            // so a loop that is evaluating and sitting there is turning but not
            // correcting — partial, not closed.
            let status = if ws == 0 {
                "open"
            } else if evals == 0 {
                "open"
            } else if mean.map(|m| m < 0.4).unwrap_or(false) {
                "partial"
            } else {
                "closed"
            };
            let detail = if ws == 0 {
                "Not a member of any workspace, so there is no discourse to cohere. Loop 3 runs \
                 on workspaces, not on lone agents."
                    .to_string()
            } else if evals == 0 {
                format!(
                    "In {ws} workspace(s), none of which has ever been coherence-evaluated. \
                     Check COHERENCE_AUTO_EVAL_INTERVAL or invoke cohere_and_coordinate."
                )
            } else {
                let score = mean
                    .map(|m| format!("Γ(C) mean {:.2}", m))
                    .unwrap_or_else(|| "no score".into());
                let age = last
                    .map(|t| {
                        format!(
                            " · last {} day(s) ago",
                            ((Utc::now() - t).num_seconds() / 86_400).max(0)
                        )
                    })
                    .unwrap_or_default();
                let verdict = if mean.map(|m| m < 0.4).unwrap_or(false) {
                    " — chronic low coherence; the brief is being produced but not acted on"
                } else {
                    ""
                };
                format!(
                    "{evals} evaluation(s) across {evaluated}/{ws} workspace(s) · {score}{age}{verdict}"
                )
            };
            loops.push(loop_verdict(
                "3a",
                "Coherence (inner)",
                "workspace",
                status,
                detail,
                "workspace_agents ⋈ coherence_evaluations",
                json!({
                    "workspaces": ws, "evaluated_workspaces": evaluated,
                    "evaluations": evals, "mean_global_score": mean, "last_eval_at": last,
                }),
            ));
        }
        Err(e) => loops.push(loop_verdict(
            "3a",
            "Coherence (inner)",
            "workspace",
            "unmeasured",
            format!("Could not join workspace_agents to coherence_evaluations: {e}"),
            "workspace_agents ⋈ coherence_evaluations",
            Value::Null,
        )),
    }

    // ── Loop 4 — composition evolution ────────────────────────────────
    //
    // Closed means a strategist proposed a membership change and a human
    // accepted it. A version history consisting only of `proposed_by='user'`
    // rows is a human editing a team, which is not the loop.
    let l4 = sqlx::query(
        "SELECT COUNT(DISTINCT wa.workspace_id)                            AS workspaces,
                COUNT(cv.composition_version_id)                           AS versions,
                COUNT(cv.composition_version_id) FILTER (
                    WHERE cv.proposed_by IS NOT NULL
                      AND cv.proposed_by <> 'user')                         AS strategist_proposals,
                COUNT(cv.composition_version_id) FILTER (
                    WHERE cv.proposed_by IS NOT NULL
                      AND cv.proposed_by <> 'user'
                      AND cv.accepted_by IS NOT NULL)                       AS accepted_proposals,
                MAX(cv.created_at)                                          AS last_version_at
           FROM workspace_agents wa
           LEFT JOIN composition_versions cv ON cv.workspace_id = wa.workspace_id
          WHERE wa.agent_id = $1",
    )
    .bind(aid)
    .fetch_one(db)
    .await;

    match l4 {
        Ok(r) => {
            let n = |k: &str| -> i64 { r.try_get::<i64, _>(k).unwrap_or(0) };
            let (ws, versions) = (n("workspaces"), n("versions"));
            let (proposed, accepted) = (n("strategist_proposals"), n("accepted_proposals"));
            let last = r
                .try_get::<Option<chrono::DateTime<Utc>>, _>("last_version_at")
                .ok()
                .flatten();

            let status = if accepted > 0 {
                "closed"
            } else if proposed > 0 {
                "partial"
            } else {
                "open"
            };
            let detail = if accepted > 0 {
                format!(
                    "{accepted} of {proposed} strategist proposal(s) accepted across {ws} \
                     workspace(s) — team membership has actually changed on the strength of \
                     accumulated sessions."
                )
            } else if proposed > 0 {
                format!(
                    "{proposed} strategist proposal(s) raised, none accepted. The loop reaches \
                     the owner and stops there — review the proposals."
                )
            } else if ws == 0 {
                "Not a member of any workspace. Loop 4 evolves compositions, so there is \
                 nothing for it to act on."
                    .to_string()
            } else if versions > 0 {
                format!(
                    "{versions} composition version(s) across {ws} workspace(s), all \
                     human-authored — no strategist has proposed a change yet. Stage 4 \
                     dreaming needs roughly 10 sessions of history before it will."
                )
            } else {
                format!(
                    "In {ws} workspace(s), none of which has a composition identity yet — no \
                     mission, no strategist, so nothing versions."
                )
            };
            loops.push(loop_verdict(
                "4",
                "Composition evolution",
                "composition",
                status,
                detail,
                "workspace_agents ⋈ composition_versions",
                json!({
                    "workspaces": ws, "versions": versions,
                    "strategist_proposals": proposed, "accepted_proposals": accepted,
                    "last_version_at": last,
                }),
            ));
        }
        Err(e) => loops.push(loop_verdict(
            "4",
            "Composition evolution",
            "composition",
            "unmeasured",
            format!("Could not join workspace_agents to composition_versions: {e}"),
            "workspace_agents ⋈ composition_versions",
            Value::Null,
        )),
    }

    // ── Loops 1b and 5a — both read the calibration profile ────────────────
    //
    // Calling `compute_agent_calibration` rather than re-deriving the numbers:
    // the Brier-skill gating below is only meaningful if it is gating on the
    // same figures `/api/agents/:id/calibration` reports, and a second
    // implementation would drift.
    let calib = fermi::calibration::compute_agent_calibration(
        db,
        &db_agent,
        &fermi::calibration::CalibrationQuery::default(),
    )
    .await;

    match calib {
        Ok(c) => {
            // Loop 1b — projection accuracy against real SOSA observations.
            let proj = c["projection_accuracy_mean"].as_f64();
            let n_proj = c["n_projection_observations"].as_i64().unwrap_or(0);
            loops.push(loop_verdict(
                "1b",
                "Projection accuracy",
                "agent",
                if proj.is_some() { "closed" } else { "open" },
                match proj {
                    Some(p) => format!(
                        "{:.0}% mean accuracy over n={n_proj} matched observation(s).",
                        p * 100.0
                    ),
                    None => {
                        "No projected value has been matched to a real observation yet.".to_string()
                    }
                },
                "GET /api/agents/:id/calibration ← observations",
                json!({ "projection_accuracy_mean": proj, "n_projection_observations": n_proj }),
            ));

            // Loop 5a — Brier calibration. Gated on skill over the base rate,
            // never on the raw score: on the 48 World Cup tournament-winner
            // forecasts (47 NO, 1 YES) a forecaster that knows nothing scores
            // ~98% raw, and gating on that reported base-rate skew as a closed
            // loop.
            let score = c["calibration_score"].as_f64();
            let n_res = c["n_resolved_forecasts"].as_i64().unwrap_or(0);
            let bss = c["brier_skill_score"].as_f64();
            let base = c["outcome_base_rate"].as_f64();
            let ev = c["evidence_class"].as_str().unwrap_or("none").to_string();

            // EVIDENCE axis: how much the number means.
            let (mut status, evidence_detail) = match score {
                None => ("open", "No resolved forecasts yet.".to_string()),
                Some(s) => {
                    let raw = format!("{:.0}% raw · n={n_res}", s * 100.0);
                    if n_res < 5 {
                        ("partial", format!("{raw} — needs ≥5 resolved forecasts."))
                    } else {
                        match bss {
                            None => (
                                "partial",
                                format!("{raw} — no base-rate reference (all outcomes resolved alike); skill undefined."),
                            ),
                            Some(b) if b > 0.05 => {
                                let caveat = if ev == "provisional" || ev == "thin" {
                                    format!(" · {ev} evidence")
                                } else {
                                    String::new()
                                };
                                (
                                    "closed",
                                    format!(
                                        "{raw} · skill +{b:.2} vs {:.0}% base rate{caveat}",
                                        base.unwrap_or(0.0) * 100.0
                                    ),
                                )
                            }
                            Some(b) => (
                                "partial",
                                format!(
                                    "{raw} but skill {b:.2} — no better than always predicting the \
                                     {:.0}% base rate. The raw score is base-rate skew, not calibration.",
                                    base.unwrap_or(0.0) * 100.0
                                ),
                            ),
                        }
                    }
                }
            };

            // MECHANISM axis: whether the chain that produced the number works.
            //
            // Run only when there is something to interpret and the caller may
            // see it. Skipping on n=0 is not laziness: the probe is thirteen
            // aggregate queries over `fermi_forecasts` with lateral jsonb
            // expansion, and for a non-forecasting agent the answer changes
            // nothing — an empty loop is `open` regardless of wiring.
            let mechanism: &str;
            let mut failing: Vec<Value> = Vec::new();
            let mut not_scopable: Vec<Value> = Vec::new();
            let (mut mech_ok, mut mech_viol) = (0usize, 0usize);

            if n_res == 0 {
                mechanism = "not_applicable";
            } else {
                // Agent-scoped, so no admin gate: every count is filtered to
                // this agent's own roster or its own signals. The fleet probe
                // could report `broken` because of a different tenant's
                // orphaned forecast, which is precisely the unattributable
                // claim this endpoint exists to remove.
                let m = probe_loop5_mechanism_for_agent(db, aid).await;
                failing = m.failing();
                not_scopable = m.not_scopable();
                mech_ok = m.ok;
                mech_viol = m.violations;
                mechanism = m.verdict;
            }

            let evidence = evidence_band(n_res, &ev);

            // The gate. A number produced by a broken chain is not a weak
            // measurement, it is not a measurement — so it must not be able to
            // reach `closed` on the strength of looking good.
            let mut detail = evidence_detail.clone();
            match mechanism {
                "broken" => {
                    status = "broken";
                    let ids: Vec<&str> = failing.iter().filter_map(|c| c["id"].as_str()).collect();
                    let named = if ids.is_empty() {
                        "see the mechanism probe".to_string()
                    } else {
                        ids.join(", ")
                    };
                    detail = format!(
                        "WIRING BROKEN — {mech_viol} mechanism violation(s) ({named}). The score \
                         is not trustworthy at any sample size, and more forecasts will not help: \
                         {evidence_detail}"
                    );
                }
                "inconclusive" => {
                    // Cannot certify, so cannot close. Distinct from `broken`:
                    // the probe failed, the chain did not.
                    if status == "closed" {
                        status = "partial";
                    }
                    detail = format!(
                        "{evidence_detail} · mechanism UNKNOWN — a probe query errored, so \
                         soundness could not be established."
                    );
                }
                "sound" => {
                    detail = format!(
                        "{evidence_detail} · this agent's wiring verified sound ({mech_ok}/{} \
                         scopable mechanism checks clean), evidence is {evidence} — so any \
                         weakness here is data volume, not a fault.",
                        LOOP5_AGENT_CHECKS.len()
                    );
                }
                _ => {}
            }

            loops.push(loop_verdict(
                "5a",
                "Brier calibration",
                "agent",
                status,
                detail,
                if mechanism == "sound" || mechanism == "broken" || mechanism == "inconclusive" {
                    "GET /api/agents/:id/calibration + 9 MECHANISM checks (scripts/loop5_brier_mechanical_check.sql)"
                } else {
                    "GET /api/agents/:id/calibration ← fermi_forecasts + eval_signals"
                },
                json!({
                    "calibration_score": score,
                    "n_resolved_forecasts": n_res,
                    "brier_skill_score": bss,
                    "outcome_base_rate": base,
                    "evidence_class": ev,
                    // The two axes, kept apart on purpose. `mechanism` says
                    // whether the chain works; `evidence_band` says how much has
                    // come through it. They fail independently and have opposite
                    // remedies — repair versus wait — so a single status cannot
                    // carry both.
                    "health": {
                        "mechanism": mechanism,
                        // Scoped to this agent, so a fault named here is this
                        // agent's fault and not the fleet's.
                        "mechanism_scope": "agent",
                        "mechanism_checks_ok": mech_ok,
                        "mechanism_checks_total": LOOP5_AGENT_CHECKS.len(),
                        "mechanism_violations": mech_viol,
                        "failing_checks": failing,
                        // Named rather than omitted: a scoped "all clean" that
                        // silently skipped a check would overclaim.
                        "not_scopable": not_scopable,
                        "fleet_probe_note": "L5-M03 (forecasts attributable to no agent) cannot \
                                             be filed under any agent by construction. Only the \
                                             admin fleet probe sees it.",
                        "evidence_band": evidence,
                        "interpretation": if mechanism == "not_applicable" {
                            "No forecast has resolved for this agent, so there is no Loop 5 \
                             signal to be thin or broken. Mechanism soundness is moot until \
                             one does."
                        } else {
                            loop5_interpretation(mechanism, evidence)
                        },
                    },
                    "mechanism_probe": "GET /api/observatory/loops/brier/mechanism",
                }),
            ));
        }
        Err(e) => {
            for (id, name) in [("1b", "Projection accuracy"), ("5a", "Brier calibration")] {
                loops.push(loop_verdict(
                    id,
                    name,
                    "agent",
                    "unmeasured",
                    format!("Calibration profile could not be computed: {e}"),
                    "GET /api/agents/:id/calibration",
                    Value::Null,
                ));
            }
        }
    }

    // Stable display order, independent of the order the queries returned in.
    const ORDER: &[&str] = &["1a", "1b", "2", "3a", "4", "5a"];
    loops.sort_by_key(|l| {
        ORDER
            .iter()
            .position(|o| Some(*o) == l["id"].as_str())
            .unwrap_or(usize::MAX)
    });

    let count = |s: &str| loops.iter().filter(|l| l["status"] == s).count();

    Ok(Json(json!({
        "agent_id": db_agent.agent_id,
        "agent_name": db_agent.agent_name,
        "measured_at": Utc::now(),
        "loops": loops,
        "summary": {
            "closed": count("closed"),
            "partial": count("partial"),
            "open": count("open"),
            // A loop whose machinery is wired wrong. Ranked above `open`
            // because an absent loop is a backlog and a broken one is a bug
            // actively producing wrong numbers.
            "broken": count("broken"),
            // Non-zero means this page is missing information, not that the
            // loops are broken. Kept out of `open` for exactly that reason.
            "unmeasured": count("unmeasured"),
        },
        "note": "Five statuses, and the distinctions are load-bearing. `closed`: turning. \
                 `partial`: turning, but the signal is thin or unskilled — remedy is volume. \
                 `broken`: the machinery is wired wrong — remedy is repair, and more volume \
                 makes it worse. `open`: measurably not turning. `unmeasured`: a query failed \
                 and this surface cannot say either way. Every loop carries the `source` it was \
                 derived from — nothing on this endpoint is a constant.",
    })))
}

#[cfg(test)]
mod loop_health_tests {
    use super::*;

    /// The regression this module exists for.
    ///
    /// `macro_forecaster` had 140 runs, 4 eval runs and 2 tracked dimensions,
    /// and the Loops tab reported "1a Individual learning — closed". It had
    /// never consolidated, so nothing it had ever been told about itself had
    /// been written back anywhere. The loop was half open and reported shut.
    #[test]
    fn eval_runs_alone_do_not_close_the_learning_loop() {
        let (status, detail) = classify_loop1a(Loop1aInputs {
            eval_runs: 4,
            eval_signals: 12,
            dimensions: 2,
            episodes: 140,
            backlog: 140,
            ..Default::default()
        });
        assert_eq!(
            status, "partial",
            "signal without correction is half a loop"
        );
        assert!(
            detail.contains("never dreamt"),
            "must name the missing half, got: {detail}"
        );
        assert!(
            detail.contains("140 episode(s) waiting"),
            "must quantify what has not been learned from, got: {detail}"
        );
    }

    #[test]
    fn both_halves_turning_is_closed() {
        let (status, detail) = classify_loop1a(Loop1aInputs {
            eval_runs: 6,
            eval_signals: 30,
            dimensions: 8,
            completed_cycles: 3,
            entities: 40,
            facts: 12,
            rules: 5,
            episodes: 200,
            ..Default::default()
        });
        assert_eq!(status, "closed");
        assert!(detail.contains("57 ontology row(s)"), "got: {detail}");
    }

    /// A cycle that ran and extracted nothing corrected nothing. Counting the
    /// cycle rather than its yield is what let 91 zero-yield cycles look like a
    /// healthy loop.
    #[test]
    fn a_zero_yield_cycle_is_not_a_correction() {
        let i = Loop1aInputs {
            eval_runs: 2,
            completed_cycles: 5,
            episodes: 80,
            ..Default::default()
        };
        assert!(!i.has_correction());
        let (status, detail) = classify_loop1a(i);
        assert_eq!(status, "partial");
        assert!(
            detail.contains("learning nothing"),
            "a cycle that extracted nothing must say so, got: {detail}"
        );
    }

    /// Consolidating without ever being evaluated is also half a loop — the
    /// agent is accumulating knowledge with no measure of whether it improved.
    #[test]
    fn consolidation_without_evaluation_is_also_partial() {
        let (status, detail) = classify_loop1a(Loop1aInputs {
            completed_cycles: 4,
            entities: 30,
            rules: 3,
            episodes: 90,
            ..Default::default()
        });
        assert_eq!(status, "partial");
        assert!(detail.contains("no eval run"), "got: {detail}");
    }

    /// An idle agent is not a broken agent. 537 of 731 agents on this fleet
    /// have zero episodes, so conflating "nothing to learn from" with "failed
    /// to learn" reports most of the platform as broken when it is merely new.
    #[test]
    fn an_agent_with_no_episodes_is_told_to_run_first() {
        let (status, detail) = classify_loop1a(Loop1aInputs::default());
        assert_eq!(status, "open");
        assert!(detail.contains("execute it first"), "got: {detail}");
    }

    /// The ontologist case.
    ///
    /// It is handed to the consolidation worker as a bare `LLMProvider`, so it
    /// produces no episodes while doing the extraction work for the whole
    /// fleet. Reporting that as `open` — "execute it first" — is derived
    /// correctly from the tables and wrong about the world, and it is wrong in
    /// the most misleading direction: it tells you to start something that has
    /// been running all along.
    #[test]
    fn an_off_ledger_service_is_unmeasured_not_idle() {
        let i = Loop1aInputs {
            off_ledger_service: true,
            ..Default::default()
        };
        let (status, detail) = classify_loop1a(i);

        assert_eq!(
            status, "unmeasured",
            "an empty ledger for a service agent means we cannot see its learning, \
             not that it has none"
        );
        assert_ne!(status, "open");
        assert!(
            !detail.contains("execute it first"),
            "must not tell the operator to start an agent that already runs: {detail}"
        );
        assert!(detail.contains("dreaming-pipeline service"), "{detail}");
    }

    /// The flag must not leak into ordinary agents: a normal agent with no
    /// episodes really has never run, and should still be told so.
    #[test]
    fn an_ordinary_idle_agent_is_still_open() {
        let (status, detail) = classify_loop1a(Loop1aInputs {
            off_ledger_service: false,
            ..Default::default()
        });
        assert_eq!(status, "open");
        assert!(detail.contains("execute it first"), "{detail}");
    }

    /// Once the dream ledger gives a service agent episodes, it should be read
    /// like any other agent — the exemption is for an empty ledger, not a
    /// permanent excuse.
    #[test]
    fn a_service_agent_with_episodes_is_judged_normally() {
        let (status, _) = classify_loop1a(Loop1aInputs {
            off_ledger_service: true,
            eval_runs: 2,
            episodes: 40,
            completed_cycles: 1,
            entities: 12,
            rules: 3,
            ..Default::default()
        });
        assert_eq!(status, "closed");

        // And with episodes but neither half turning, it is genuinely open.
        let (status, detail) = classify_loop1a(Loop1aInputs {
            off_ledger_service: true,
            episodes: 40,
            backlog: 40,
            ..Default::default()
        });
        assert_eq!(status, "open");
        assert!(detail.contains("40 episode(s) exist"), "{detail}");
    }

    #[test]
    fn failed_cycles_are_named_rather_than_counted_as_absence() {
        let (status, detail) = classify_loop1a(Loop1aInputs {
            failed_cycles: 3,
            episodes: 40,
            backlog: 40,
            ..Default::default()
        });
        assert_eq!(status, "open");
        assert!(
            detail.contains("3 cycle(s) failed"),
            "a loop that tried and failed must not look like one that never tried, got: {detail}"
        );
    }

    /// `unmeasured` must stay distinct from `open`. `open` asserts the loop is
    /// not turning; `unmeasured` admits the page cannot tell. Collapsing them
    /// turns a missing table into a confident negative verdict.
    #[test]
    fn the_five_statuses_are_distinct() {
        let statuses = ["closed", "partial", "open", "broken", "unmeasured"];
        let v = loop_verdict(
            "3a",
            "Coherence (inner)",
            "workspace",
            "unmeasured",
            "probe failed".into(),
            "workspace_agents ⋈ coherence_evaluations",
            Value::Null,
        );
        assert_eq!(v["status"], "unmeasured");
        assert_ne!(v["status"], "open");
        assert_ne!(v["status"], "broken");
        // Every verdict must be attributable; an unsourced row is the thing
        // this endpoint replaced.
        assert!(v["source"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(v["scope"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(statuses.contains(&v["status"].as_str().unwrap()));
    }

    // ── Loop 5: thin versus broken ───────────────────────────────────────
    //
    // The whole point of separating MECHANISM from EVIDENCE. These two states
    // produce similar-looking weak numbers and have opposite remedies, and a
    // single status cannot carry both.

    #[test]
    fn thin_and_broken_are_never_the_same_message() {
        let thin = loop5_interpretation("sound", "thin");
        let broken = loop5_interpretation("broken", "thin");
        assert_ne!(thin, broken);

        // Thin: wait. Explicitly says nothing needs fixing, because the most
        // expensive wrong move here is debugging a loop that is merely young.
        assert!(thin.contains("THIN"), "{thin}");
        assert!(thin.contains("Nothing needs fixing"), "{thin}");
        assert!(thin.contains("volume"), "{thin}");

        // Broken: repair. Must say that volume makes it worse — the opposite
        // instruction — and must not tell the operator to be patient.
        assert!(broken.contains("BROKEN"), "{broken}");
        assert!(
            broken.contains("not thin"),
            "the broken message must actively rule out the thin reading: {broken}"
        );
        assert!(broken.contains("Repair"), "{broken}");
        assert!(
            !broken.to_lowercase().contains("nothing needs fixing"),
            "{broken}"
        );
    }

    /// Broken wiring dominates the evidence axis. A great-looking score coming
    /// through a chain that drops or double-counts signals is not a strong
    /// measurement; it is not a measurement.
    #[test]
    fn broken_wiring_overrides_good_evidence() {
        for evidence in ["none", "provisional", "thin", "usable"] {
            let m = loop5_interpretation("broken", evidence);
            assert!(
                m.contains("BROKEN"),
                "evidence={evidence} must not soften a broken verdict: {m}"
            );
        }
    }

    // ── The fleet/agent parity contract ──────────────────────────────────
    //
    // Three copies of these checks exist (fleet table, agent table, and
    // scripts/loop5_brier_mechanical_check.sql). The first two are enforced
    // here; the third is enforced by the header comment and by the ids being
    // greppable across all three.

    #[test]
    fn agent_and_fleet_checks_declare_the_same_ids() {
        let fleet: Vec<&str> = LOOP5_MECHANISM_CHECKS.iter().map(|(id, ..)| *id).collect();
        let mut agent: Vec<&str> = LOOP5_AGENT_CHECKS.iter().map(|(id, ..)| *id).collect();
        agent.extend(LOOP5_UNSCOPABLE.iter().map(|(id, _)| *id));
        agent.sort_unstable();

        let mut expected = fleet.clone();
        expected.sort_unstable();

        assert_eq!(
            agent, expected,
            "every fleet MECHANISM check must be either agent-scoped or explicitly declared \
             unscopable — a check that is neither has been silently dropped from the \
             per-agent verdict"
        );
    }

    #[test]
    fn agent_and_fleet_checks_agree_on_severity() {
        for (id, severity, ..) in LOOP5_AGENT_CHECKS {
            let fleet_sev = LOOP5_MECHANISM_CHECKS
                .iter()
                .find(|(fid, ..)| fid == id)
                .map(|(_, s, ..)| *s)
                .unwrap_or("<missing>");
            assert_eq!(
                *severity, fleet_sev,
                "{id} is {severity} when scoped to an agent and {fleet_sev} across the fleet; \
                 the same fault cannot have two severities"
            );
        }
    }

    /// Every agent-scoped check must actually be scoped. A check that forgot its
    /// filter would silently report another tenant's fault against this agent —
    /// the exact bug the agent-scoped table was added to fix.
    #[test]
    fn every_agent_check_is_actually_scoped_to_an_agent() {
        for (id, _, sql) in LOOP5_AGENT_CHECKS {
            assert!(
                sql.contains("{ROSTER}") || sql.contains("$1"),
                "{id} references neither the roster predicate nor $1, so it is not \
                 agent-scoped and would count fleet-wide violations against one agent"
            );
        }
    }

    /// The roster predicate must exist once, not once per check. Eight copies is
    /// how it drifts out of step with `eval_brier.rs::latest_for_agent`.
    #[test]
    fn the_roster_predicate_is_written_once_and_matches_the_three_shape_join() {
        for shape in ["agent_id", "agent_name", "name"] {
            assert!(
                ROSTER_PREDICATE.contains(&format!("'{shape}'")),
                "roster predicate must match the {shape} shape, like every other reader"
            );
        }
        assert!(ROSTER_PREDICATE.contains("$1"));
        // No check should inline its own copy.
        for (id, _, sql) in LOOP5_AGENT_CHECKS {
            assert!(
                !sql.contains("re->>'agent_name'") || sql.contains("{ROSTER}"),
                "{id} appears to inline the roster join instead of using {{ROSTER}}"
            );
        }
    }

    /// Structural sanity on the hand-written SQL constants.
    ///
    /// These cannot prove the queries are *correct* — that needs a database and
    /// a schema — but they catch the failure modes that hand-edited SQL string
    /// literals actually hit: an unbalanced paren, a stray quote, or a
    /// `{ROSTER}` that never got substituted and would reach Postgres verbatim.
    ///
    /// A malformed check is not silent at runtime either: `probe_*` catches the
    /// per-check `Err`, records `status: ERROR`, and the verdict becomes
    /// `inconclusive` — reported to the operator as "mechanism UNKNOWN", never
    /// as `sound`.
    #[test]
    fn agent_check_sql_is_structurally_well_formed() {
        for (id, _, sql) in LOOP5_AGENT_CHECKS {
            let resolved = sql.replace("{ROSTER}", ROSTER_PREDICATE);

            // A placeholder is `{UPPERCASE}`. `{36}` in L5-M07 is a POSIX regex
            // quantifier and must not trip this.
            let unsubstituted: Vec<&str> = resolved
                .match_indices('{')
                .filter(|(i, _)| {
                    resolved[*i + 1..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase())
                })
                .map(|(i, _)| &resolved[i..(i + 12).min(resolved.len())])
                .collect();
            assert!(
                unsubstituted.is_empty(),
                "{id} still has an unsubstituted placeholder after {{ROSTER}} expansion \
                 ({unsubstituted:?}); it would be sent to Postgres literally"
            );

            let opens = resolved.matches('(').count();
            let closes = resolved.matches(')').count();
            assert_eq!(opens, closes, "{id} has unbalanced parentheses");

            assert_eq!(
                resolved.matches('\'').count() % 2,
                0,
                "{id} has an odd number of single quotes"
            );

            assert!(
                resolved.trim_start().to_uppercase().starts_with("SELECT")
                    || resolved.trim_start().to_uppercase().starts_with("WITH"),
                "{id} must be a read-only SELECT/WITH — this probe never writes"
            );

            for forbidden in [
                "INSERT ",
                "UPDATE ",
                "DELETE ",
                "DROP ",
                "ALTER ",
                "TRUNCATE ",
            ] {
                assert!(
                    !resolved.to_uppercase().contains(forbidden),
                    "{id} contains {forbidden} — the mechanism probe is strictly read-only"
                );
            }
        }
    }

    /// The `{ROSTER}` splice must land inside a WHERE/AND context in every check
    /// that uses it, not be concatenated somewhere it changes meaning.
    #[test]
    fn the_roster_splice_is_always_a_conjunct() {
        for (id, _, sql) in LOOP5_AGENT_CHECKS {
            if let Some(pos) = sql.find("{ROSTER}") {
                let before = sql[..pos].trim_end();
                assert!(
                    before.to_uppercase().ends_with("AND")
                        || before.to_uppercase().ends_with("WHERE"),
                    "{id} splices the roster predicate after `{}`; it must be a WHERE/AND \
                     conjunct or it silently changes what the check counts",
                    before.split_whitespace().last().unwrap_or("")
                );
            }
        }
    }

    /// A check that cannot be attributed to an agent must be declared, not
    /// dropped, or a scoped "all clean" claims more than it measured.
    #[test]
    fn unscopable_checks_are_declared_with_a_reason() {
        assert!(
            !LOOP5_UNSCOPABLE.is_empty(),
            "L5-M03 counts forecasts attributable to no agent and cannot be scoped"
        );
        for (id, why) in LOOP5_UNSCOPABLE {
            assert!(why.len() > 60, "{id} needs a real reason, got: {why}");
            assert!(
                !LOOP5_AGENT_CHECKS.iter().any(|(aid, ..)| aid == id),
                "{id} is declared both scopable and unscopable"
            );
        }
    }

    /// `NOT_SCOPABLE` must never be counted as a fault against the agent.
    #[test]
    fn a_not_scopable_check_is_not_a_failing_check() {
        let m = Loop5Mechanism {
            verdict: "sound",
            violations: 0,
            ok: 8,
            errored: 0,
            checks: vec![
                json!({"id":"L5-M01","class":"MECHANISM","status":"OK"}),
                json!({"id":"L5-M03","class":"MECHANISM","status":"NOT_SCOPABLE"}),
                json!({"id":"L5-M06","class":"MECHANISM","status":"VIOLATION"}),
            ],
        };
        let failing: Vec<String> = m
            .failing()
            .iter()
            .filter_map(|c| c["id"].as_str().map(str::to_owned))
            .collect();
        assert_eq!(failing, vec!["L5-M06".to_string()]);
        assert_eq!(m.not_scopable().len(), 1);
    }

    /// `inconclusive` (a probe query errored) is not `broken` (the chain is
    /// wrong). One is a broken tool, the other a broken subject.
    #[test]
    fn an_errored_probe_is_not_a_broken_loop() {
        let inc = loop5_interpretation("inconclusive", "usable");
        assert!(inc.contains("UNKNOWN"), "{inc}");
        assert!(!inc.contains("BROKEN"), "{inc}");
        assert_ne!(inc, loop5_interpretation("broken", "usable"));
    }

    #[test]
    fn sound_and_sufficient_is_the_only_message_that_endorses_the_number() {
        let good = loop5_interpretation("sound", "usable");
        assert!(good.contains("real measurement"), "{good}");
        for m in [
            loop5_interpretation("sound", "thin"),
            loop5_interpretation("sound", "none"),
            loop5_interpretation("broken", "usable"),
            loop5_interpretation("inconclusive", "usable"),
            // An unrecognised mechanism must fall through to the catch-all
            // rather than accidentally endorsing the number.
            loop5_interpretation("something_new", "usable"),
        ] {
            assert!(!m.contains("real measurement"), "{m}");
        }
    }

    #[test]
    fn evidence_band_never_reports_usable_on_an_empty_set() {
        assert_eq!(evidence_band(0, "usable"), "none");
        assert_eq!(evidence_band(0, ""), "none");
        assert_eq!(evidence_band(3, ""), "provisional");
        assert_eq!(evidence_band(10, ""), "thin");
        assert_eq!(evidence_band(50, ""), "usable");
        // The calibration endpoint's own class wins when it has one.
        assert_eq!(evidence_band(50, "thin"), "thin");
    }
}
