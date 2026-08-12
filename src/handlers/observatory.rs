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

    let run_counts: std::collections::HashMap<Uuid, i64> = if !agent_ids.is_empty() {
        sqlx::query("SELECT agent_id, COUNT(*) as cnt FROM eval_runs WHERE agent_id = ANY($1) GROUP BY agent_id")
            .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
            .iter().map(|r| (r.get("agent_id"), r.get::<i64,_>("cnt"))).collect()
    } else {
        std::collections::HashMap::new()
    };

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

    for ag in &agents {
        let runs = run_counts.get(&ag.agent_id).copied().unwrap_or(0);
        let anom = open_anom_map.get(&ag.agent_id).copied().unwrap_or(0);
        *buckets.entry(maturity_stage(runs, anom)).or_insert(0) += 1;
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

    let result: Vec<Value> = agents
        .iter()
        .map(|a| {
            let runs = run_c.get(&a.agent_id).copied().unwrap_or(0);
            let anom = anom_c.get(&a.agent_id).copied().unwrap_or(0);
            let maturity = maturity_stage(runs, anom);
            let sc = scores_c
                .get(&a.agent_id)
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let dyads = dyad_c.get(&a.agent_id).copied().unwrap_or(0);
            let (tct, tcnr) = tc_c.get(&a.agent_id).copied().unwrap_or((0, 0));
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
                "persona_version": a.persona_version, "total_executions": a.total_executions,
                "eval_runs": runs, "open_anomalies": anom, "dyad_count": dyads,
                "maturity": maturity, "overall_health": health, "latest_scores": sc,
                "care_plan": build_care_plan(runs,tct,tcnr,anom,&maturity,health),
                "last_consolidated_at": a.last_consolidated_at,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({"agents":result})))
}

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
            let display_name = profile["display_name"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| human_id.clone());

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

    Ok(Json(serde_json::json!({
        "agent_id": db_agent.agent_name,
        "relationship_count": relationships.len(),
        "scored_count": scored_count,
        // Surfaced so the UI can say "N awaiting scan" instead of implying
        // the relationships do not exist.
        "pending_scan_count": pending_count,
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
    let owns:bool=sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dyad_profiles dp JOIN agents a ON a.agent_id=dp.agent_id WHERE dp.dyad_id=$1 AND a.user_id=$2)"
    ).bind(&dyad_id).bind(&user_id).fetch_one(&state.db).await.unwrap_or(false);
    if !owns {
        return Err((StatusCode::FORBIDDEN, "Not the owner of this dyad".into()));
    }
    sqlx::query("UPDATE dyad_profiles SET display_name=COALESCE($2,display_name),notes=COALESCE($3,notes),updated_at=NOW() WHERE dyad_id=$1")
    .bind(&dyad_id)
    .bind(body.get("display_name").and_then(|v|v.as_str()))
    .bind(body.get("notes").and_then(|v|v.as_str()))
    .execute(&state.db).await
    .map_err(|e|(StatusCode::SERVICE_UNAVAILABLE, format!("dyad_profiles unavailable (migration pending?): {}", e)))?;
    Ok(Json(serde_json::json!({"updated":true})))
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

    let mut checks: Vec<Value> = Vec::new();
    let mut violations = 0usize;
    let mut ok = 0usize;
    let mut errored = 0usize;

    for (id, severity, description, sql) in LOOP5_MECHANISM_CHECKS {
        // A failing probe query must never 500 the probe: an errored check is
        // itself a finding, and reporting it as INCONCLUSIVE is more honest
        // than either hiding it or pretending the loop is sound.
        match sqlx::query_scalar::<_, i64>(sql).fetch_one(&state.db).await {
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
        match sqlx::query_scalar::<_, i64>(sql).fetch_one(&state.db).await {
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

    Ok(Json(json!({
        "loop": "5a",
        "label": "Brier calibration",
        "verdict": verdict,
        "verdict_detail": match verdict {
            "sound" => "The chain moves signals correctly. Any weakness in the numbers is thin data or skew, not wiring.",
            "broken" => "Do not trust any Loop 5 number until the MECHANISM violations are resolved.",
            _ => "A probe query errored — fix it before drawing conclusions either way.",
        },
        "mechanism_violations": violations,
        "mechanism_ok": ok,
        "errored": errored,
        "checks": checks,
        "note": "MECHANISM checks are sample-size independent and must be clean at n=1. INFO checks measure how much the signal currently means and are never failures. Full rationale per check: scripts/loop5_brier_mechanical_check.sql",
    })))
}
