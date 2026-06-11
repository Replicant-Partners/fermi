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
    // Curated agents (owner_id = NULL, tier = "curated") are observable by
    // any authenticated user — they are platform-level agents not owned by
    // any individual. Observatory is read-only for non-owners.
    let is_curated = db_agent.owner_id.is_none() && db_agent.tier == "curated";
    if !is_owner && !is_admin && !is_curated {
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

// ─── Fleet endpoints ─────────────────────────────────────────────────────────

pub async fn fleet_summary_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let agents = state.memory_store.list_agents_for_owner(&user_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let agent_ids: Vec<Uuid> = agents.iter().map(|a| a.agent_id).collect();

    let run_counts: std::collections::HashMap<Uuid, i64> = if !agent_ids.is_empty() {
        sqlx::query("SELECT agent_id, COUNT(*) as cnt FROM eval_runs WHERE agent_id = ANY($1) GROUP BY agent_id")
            .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
            .iter().map(|r| (r.get("agent_id"), r.get::<i64,_>("cnt"))).collect()
    } else { std::collections::HashMap::new() };

    let open_anom_map: std::collections::HashMap<Uuid, i64> = if !agent_ids.is_empty() {
        sqlx::query("SELECT agent_id, COUNT(*) as cnt FROM anomaly_events WHERE agent_id = ANY($1) AND resolved_at IS NULL GROUP BY agent_id")
            .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
            .iter().map(|r| (r.get("agent_id"), r.get::<i64,_>("cnt"))).collect()
    } else { std::collections::HashMap::new() };

    let mut buckets = std::collections::BTreeMap::<String, i32>::new();
    let mut provider_health: std::collections::BTreeMap<String, serde_json::Value> = std::collections::BTreeMap::new();

    for ag in &agents {
        let runs = run_counts.get(&ag.agent_id).copied().unwrap_or(0);
        let anom = open_anom_map.get(&ag.agent_id).copied().unwrap_or(0);
        *buckets.entry(maturity_stage(runs, anom)).or_insert(0) += 1;
        let p = ag.llm_provider.clone();
        let entry = provider_health.entry(p).or_insert(serde_json::json!({"agent_count":0i32,"open_anomalies":0i32,"agents":[]}));
        if let Some(obj) = entry.as_object_mut() {
            *obj.entry("agent_count").or_insert(serde_json::json!(0)) = serde_json::json!(obj["agent_count"].as_i64().unwrap_or(0)+1);
            *obj.entry("open_anomalies").or_insert(serde_json::json!(0)) = serde_json::json!(obj["open_anomalies"].as_i64().unwrap_or(0)+anom);
            if let Some(arr) = obj.get_mut("agents").and_then(|v| v.as_array_mut()) { arr.push(serde_json::json!(ag.agent_name)); }
        }
    }
    let curated_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE user_id IS NULL AND tier = 'curated'")
        .fetch_one(&state.db).await.unwrap_or(0);
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
           ORDER BY ate.agent_id, ate.created_at DESC"#
    ).bind(&user_id).bind(since).fetch_all(&state.db).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agent_dim: std::collections::HashMap<(Uuid,String),(String,String,Vec<f64>)> = std::collections::HashMap::new();
    for row in &rows {
        let aid: Uuid = row.get("agent_id");
        let name: String = row.get("agent_name");
        let provider: String = row.try_get("provider").unwrap_or_default();
        if let Some(obj) = row.try_get::<serde_json::Value,_>("dim_scores").ok().and_then(|v| v.as_object().cloned()) {
            for (dim, val) in obj {
                if let Some(s) = val.as_f64() {
                    agent_dim.entry((aid, dim)).or_insert((name.clone(), provider.clone(), vec![])).2.push(s);
                }
            }
        }
    }

    let mut signals: std::collections::HashMap<(String,String),Vec<(String,f64)>> = std::collections::HashMap::new();
    for ((_aid,dim),(name,provider,scores)) in &agent_dim {
        if scores.len() < 2 { continue; }
        let decline = scores.last().unwrap() - scores[0];
        if decline > 0.10 { signals.entry((provider.clone(),dim.clone())).or_default().push((name.clone(),decline)); }
    }

    let mut fleet_anomalies: Vec<Value> = signals.iter()
        .filter(|(_,aff)| aff.len() >= 3)
        .map(|((provider,dim),aff)| {
            let avg = aff.iter().map(|(_,d)| d).sum::<f64>() / aff.len() as f64;
            serde_json::json!({
                "suspected_provider": provider, "dimension": dim,
                "affected_agent_count": aff.len(),
                "affected_agents": aff.iter().map(|(n,_)| n).collect::<Vec<_>>(),
                "avg_decline": (avg*100.0).round()/100.0,
                "severity": if avg>0.25{"high"} else if avg>0.15{"medium"} else {"low"},
            })
        }).collect();
    fleet_anomalies.sort_by_key(|a| match a["severity"].as_str().unwrap_or("") {"high"=>0,"medium"=>1,_=>2});

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
    let agents = state.memory_store.list_agents_for_owner(&user_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let agent_ids: Vec<Uuid> = agents.iter().map(|a| a.agent_id).collect();
    if agent_ids.is_empty() { return Ok(Json(serde_json::json!({"agents":[]}))); }

    let run_c: std::collections::HashMap<Uuid,i64> =
        sqlx::query("SELECT agent_id,COUNT(*) as cnt FROM eval_runs WHERE agent_id=ANY($1) GROUP BY agent_id")
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get("agent_id"),r.get::<i64,_>("cnt"))).collect();
    let anom_c: std::collections::HashMap<Uuid,i64> =
        sqlx::query("SELECT agent_id,COUNT(*) as cnt FROM anomaly_events WHERE agent_id=ANY($1) AND resolved_at IS NULL GROUP BY agent_id")
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get("agent_id"),r.get::<i64,_>("cnt"))).collect();
    let scores_c: std::collections::HashMap<Uuid,serde_json::Value> =
        sqlx::query(r#"SELECT DISTINCT ON (agent_id) agent_id,dim_scores FROM agent_timeline_entries WHERE agent_id=ANY($1) AND dim_scores IS NOT NULL AND dim_scores!='{}'::jsonb ORDER BY agent_id,created_at DESC"#)
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get::<Uuid,_>("agent_id"),r.try_get::<serde_json::Value,_>("dim_scores").unwrap_or(serde_json::json!({})))).collect();
    let dyad_c: std::collections::HashMap<Uuid,i64> =
        sqlx::query("SELECT agent_id,COUNT(*) as cnt FROM dyad_state WHERE agent_id=ANY($1) GROUP BY agent_id")
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get("agent_id"),r.get::<i64,_>("cnt"))).collect();
    let tc_c: std::collections::HashMap<Uuid,(i64,i64)> =
        sqlx::query("SELECT agent_id,COUNT(*) as tot,COUNT(*) FILTER (WHERE rubric IS NULL) as nor FROM eval_test_cases WHERE agent_id=ANY($1) AND is_active=true GROUP BY agent_id")
        .bind(&agent_ids).fetch_all(&state.db).await.unwrap_or_default()
        .iter().map(|r|(r.get("agent_id"),(r.get::<i64,_>("tot"),r.get::<i64,_>("nor")))).collect();

    let result: Vec<Value> = agents.iter().map(|a| {
        let runs = run_c.get(&a.agent_id).copied().unwrap_or(0);
        let anom = anom_c.get(&a.agent_id).copied().unwrap_or(0);
        let maturity = maturity_stage(runs, anom);
        let sc = scores_c.get(&a.agent_id).cloned().unwrap_or(serde_json::json!({}));
        let dyads = dyad_c.get(&a.agent_id).copied().unwrap_or(0);
        let (tct,tcnr) = tc_c.get(&a.agent_id).copied().unwrap_or((0,0));
        let health: Option<f64> = sc.as_object().and_then(|obj|{
            let v: Vec<f64> = obj.values().filter_map(|x|x.as_f64()).collect();
            if v.is_empty(){None}else{Some(v.iter().sum::<f64>()/v.len() as f64)}
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
    }).collect();
    Ok(Json(serde_json::json!({"agents":result})))
}

fn maturity_stage(eval_runs: i64, open_anomalies: i64) -> String {
    if open_anomalies > 0 { return "flagged".into(); }
    match eval_runs { 0=>"newborn", 1..=4=>"intake", 5..=19=>"learning", 20..=49=>"functioning", _=>"established" }.into()
}

fn build_care_plan(runs:i64,tc_total:i64,tc_no_rubric:i64,anomalies:i64,maturity:&str,health:Option<f64>)->String{
    if anomalies>0{return format!("{} open flag(s) — review in HITL queue.",anomalies);}
    if runs==0{return if tc_total==0{"No test cases yet. Seed from sample_queries then run baseline eval.".into()}else{"Test cases ready. Run a baseline eval to activate all evaluators.".into()};}
    if tc_no_rubric>0{return format!("{} test case(s) missing rubrics — Sotopia inapplicable. Click ✦ Generate rubrics.",tc_no_rubric);}
    if runs<5{return format!("{} more eval run(s) to activate LifelongBench (needs ≥5).",5-runs);}
    if let Some(h)=health{if h<0.5{return "Health below 50% — run a scan to check for drift.".into();}}
    match maturity{
        "established"=>"Well-established. Continue regular evals to maintain calibration.".into(),
        "functioning"=>"Functioning well. Consider reviewing dyad relationships.".into(),
        _=>"Continue running evals to build calibration history.".into(),
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
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='dyad_profiles')"
    ).fetch_one(&state.db).await.unwrap_or(false);
    if !table_exists {
        return Ok(Json(serde_json::json!({
            "formed": 0,
            "message": "dyad_profiles table not yet created — migration 133 pending. Will resolve on next deploy.",
        })));
    }
    let pairs = sqlx::query(
        r#"SELECT e.agent_id, e.dyad_id, COUNT(*) as cnt,
                  MIN(e.timestamp_ref) as first_at, MAX(e.timestamp_ref) as last_at
           FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
           WHERE a.user_id=$1 AND e.dyad_id IS NOT NULL
             AND e.dyad_id NOT IN (SELECT dyad_id FROM dyad_profiles)
           GROUP BY e.agent_id, e.dyad_id HAVING COUNT(*)>=3"#
    ).bind(&user_id).fetch_all(&state.db).await
    .map_err(|e|(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()))?;

    let mut formed=0usize;
    for row in &pairs {
        let aid:Uuid=row.get("agent_id");
        let did:String=row.get("dyad_id");
        let cnt:i64=row.get("cnt");
        let fa:chrono::DateTime<Utc>=row.get("first_at");
        let la:chrono::DateTime<Utc>=row.get("last_at");
        let human_id=did.splitn(3,':').nth(2).unwrap_or(&did).to_string();
        let _=sqlx::query(
            r#"INSERT INTO dyad_profiles(dyad_id,agent_id,human_id,auto_formed,formed_at,
                first_interaction_at,last_interaction_at,total_interactions)
               VALUES($1,$2,$3,true,NOW(),$4,$5,$6)
               ON CONFLICT(dyad_id) DO UPDATE SET
                 last_interaction_at=EXCLUDED.last_interaction_at,
                 total_interactions=EXCLUDED.total_interactions,updated_at=NOW()"#
        ).bind(&did).bind(aid).bind(&human_id).bind(fa).bind(la).bind(cnt as i32)
        .execute(&state.db).await;
        formed+=1;
    }
    Ok(Json(serde_json::json!({"formed":formed,"message":format!("Auto-formed {} dyad profile(s).",formed)})))
}

pub async fn agent_relationships_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = require_owner_or_admin(&state, &principal, &agent_id).await?;
    let dyads = state.memory_store.list_dyads_for_agent(db_agent.agent_id).await
        .map_err(|e|(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()))?;
    let dyad_ids: Vec<String> = dyads.iter().map(|d|d.dyad_id.clone()).collect();
    // dyad_profiles may not exist yet (migration 133 pending) — fall back gracefully
    let profiles: std::collections::HashMap<String,serde_json::Value> = if !dyad_ids.is_empty() {
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
    } else { std::collections::HashMap::new() };

    let relationships: Vec<Value> = dyads.iter().map(|d|{
        let profile=profiles.get(&d.dyad_id).cloned().unwrap_or(serde_json::json!({}));
        let display_name=profile["display_name"].as_str().map(|s|s.to_string())
            .unwrap_or_else(||d.dyad_id.splitn(3,':').nth(2).unwrap_or(&d.dyad_id).to_string());
        let health=(d.rapport+d.trust+d.reciprocity)/3.0;
        let status=if health>=0.75{"strong"}else if health>=0.5{"developing"}else if d.episode_count<3{"new"}else{"needs_attention"};
        serde_json::json!({"dyad_id":d.dyad_id,"display_name":display_name,"human_id":d.human_id,
            "rapport":d.rapport,"trust":d.trust,"reciprocity":d.reciprocity,
            "episode_count":d.episode_count,"health":(health*100.0).round()/100.0,
            "relationship_status":status,"last_updated_at":d.last_updated_at,"profile":profile})
    }).collect();

    Ok(Json(serde_json::json!({"agent_id":db_agent.agent_name,"relationship_count":relationships.len(),"relationships":relationships})))
}

pub async fn patch_dyad_profile_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(dyad_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id=principal.user_id();
    let owns:bool=sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dyad_profiles dp JOIN agents a ON a.agent_id=dp.agent_id WHERE dp.dyad_id=$1 AND a.user_id=$2)"
    ).bind(&dyad_id).bind(&user_id).fetch_one(&state.db).await.unwrap_or(false);
    if !owns{return Err((StatusCode::FORBIDDEN,"Not the owner of this dyad".into()));}
    sqlx::query("UPDATE dyad_profiles SET display_name=COALESCE($2,display_name),notes=COALESCE($3,notes),updated_at=NOW() WHERE dyad_id=$1")
    .bind(&dyad_id)
    .bind(body.get("display_name").and_then(|v|v.as_str()))
    .bind(body.get("notes").and_then(|v|v.as_str()))
    .execute(&state.db).await
    .map_err(|e|(StatusCode::SERVICE_UNAVAILABLE, format!("dyad_profiles unavailable (migration pending?): {}", e)))?;
    Ok(Json(serde_json::json!({"updated":true})))
}
