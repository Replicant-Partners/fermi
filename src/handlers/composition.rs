//! Composition version lifecycle handlers — tune-team RSI loop.
//!
//! Routes:
//!   GET  /api/workspaces/:id/composition/versions
//!   POST /api/workspaces/:id/composition/versions/:version_id/accept
//!   POST /api/workspaces/:id/composition/versions/:version_id/reject
//!   POST /api/workspaces/:id/composition/propose    (strategist tool)
//!   POST /api/workspaces/:id/composition/dream      (triggers dreaming)
//!
//! See: docs/architecture/COMPOSITION_FEEDBACK_LOOP_PLAN.md

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use agent_bestiary_memory::{CompositionVersion, WorkspaceMessage};
use fermi_auth::{rbac, AuthPrincipal, ObjectType, Visibility};

use crate::AppState;

// ─── Permission helper ───────────────────────────────────────────────────────

async fn require_workspace_owner_or_admin(
    state: &AppState,
    principal: &AuthPrincipal,
    workspace_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    let row = sqlx::query("SELECT owner_id FROM teams WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Workspace not found".into()))?;

    let owner_id: String = row
        .try_get("owner_id")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // v0.10.5: substrate RBAC. Composition edits are Admin-scoped.
    // Uses ObjectType::Team since teams IS the workspace table.
    rbac::require_admin_on(
        &state.db,
        principal,
        ObjectType::Team,
        &workspace_id.to_string(),
        &owner_id,
        Visibility::Private,
    )
    .await?;
    Ok(())
}

// ─── GET /api/workspaces/:id/composition/versions ───────────────────────────

pub async fn list_composition_versions_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let versions = state
        .memory_store
        .list_composition_versions(workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "versions": versions })))
}

// ─── POST /api/workspaces/:id/composition/versions/:version_id/accept ───────

pub async fn accept_composition_version_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, version_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_workspace_owner_or_admin(&state, &principal, workspace_id).await?;

    let user_id = principal.user_id();
    state
        .memory_store
        .resolve_composition_version(version_id, &user_id, true, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "version_id": version_id,
        "status": "accepted",
        "accepted_by": user_id,
    })))
}

// ─── POST /api/workspaces/:id/composition/versions/:version_id/reject ───────

#[derive(Deserialize)]
pub struct RejectRequest {
    pub note: Option<String>,
}

pub async fn reject_composition_version_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, version_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<RejectRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_workspace_owner_or_admin(&state, &principal, workspace_id).await?;

    let user_id = principal.user_id();
    state
        .memory_store
        .resolve_composition_version(version_id, &user_id, false, body.note.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Store the rejection as an episode in the strategist's memory so the
    // next dreaming cycle learns from this feedback.
    if let Some(note) = &body.note {
        if !note.is_empty() {
            // Look up the workspace's coordination_strategist_id
            if let Ok(Some(row)) =
                sqlx::query("SELECT coordination_strategist_id FROM teams WHERE id = $1")
                    .bind(workspace_id)
                    .fetch_optional(&state.db)
                    .await
            {
                let strategist_id: Option<Uuid> =
                    row.try_get("coordination_strategist_id").unwrap_or(None);

                if let Some(sid) = strategist_id {
                    let episode = agent_bestiary_memory::Episode {
                        response_text: None,
                        assertions: None,
                        episode_id: Uuid::new_v4(),
                        agent_id: sid,
                        timestamp_ref: Utc::now(),
                        query: format!(
                            "Composition proposal {} rejected by workspace owner.",
                            version_id
                        ),
                        context: serde_json::json!({
                            "rejection_note": note,
                            "version_id": version_id,
                            "workspace_id": workspace_id,
                            "correction_type": "composition_proposal_rejection",
                        }),
                        execution_status: agent_bestiary_memory::ExecutionStatus::Success,
                        error_details: None,
                        execution_time_ms: 0,
                        tokens_used: None,
                        cost_usd: None,
                        // A rejection event, not an LLM run — no tokens spent,
                        // so no split and no rate basis.
                        input_tokens: None,
                        output_tokens: None,
                        cost_basis: None,
                        cost_rate_key: None,
                        parent_episode_id: None,
                        embedding: None,
                        consolidated: false,
                        tags: vec![
                            "composition_rejection".to_string(),
                            "dreaming_material".to_string(),
                        ],
                        provenance: agent_bestiary_memory::Provenance::HumanCorrected,
                        authority_weight: 1.0,
                        dyad_id: None,
                        persona_version_at_write: None,
                        provider_used: None,
                        model_used: None,
                    };
                    // Synthetic rejection episode — embedding intentionally NULL.
                    // Stamp source_ref so the row is identifiable for later cleanup.
                    let source_ref = serde_json::json!({
                        "kind": "composition_rejection",
                    });
                    let _ = state
                        .memory_store
                        .store_episode_with_provenance(episode, None, Some(source_ref))
                        .await;
                }
            }
        }
    }

    Ok(Json(json!({
        "version_id": version_id,
        "status": "rejected",
        "rejected_by": user_id,
    })))
}

// ─── POST /api/workspaces/:id/composition/propose ────────────────────────────

#[derive(Deserialize)]
pub struct ProposeRequest {
    pub diff_summary: String,
    pub rationale: String,
    pub member_agent_ids: Option<Vec<String>>,
    pub member_weights: Option<serde_json::Value>,
    pub homophily_detected: Option<bool>,
    pub proposed_by: Option<String>,
}

pub async fn propose_composition_version_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<ProposeRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let member_ids: Option<Vec<Uuid>> = body
        .member_agent_ids
        .as_ref()
        .map(|ids| ids.iter().filter_map(|s| Uuid::parse_str(s).ok()).collect());

    let diff_summary = if body.homophily_detected.unwrap_or(false) {
        format!("[homophily detected] {}", body.diff_summary)
    } else {
        body.diff_summary.clone()
    };

    let version = CompositionVersion {
        composition_version_id: Uuid::new_v4(),
        workspace_id,
        version_number: 0, // overwritten by create_composition_version
        mission: None,
        coordination_strategist_id: None,
        member_agent_ids: member_ids,
        member_weights: body.member_weights.clone(),
        diff_summary: Some(diff_summary),
        proposed_by: Some(
            body.proposed_by
                .clone()
                .unwrap_or_else(|| "cohere_and_coordinate".to_string()),
        ),
        accepted_by: None,
        rejected_by: None,
        rejection_note: Some(body.rationale.clone()),
        created_at: Utc::now(),
    };

    let version_id = state
        .memory_store
        .create_composition_version(&version)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "version_id": version_id,
        "workspace_id": workspace_id,
        "status": "pending",
        "message": "Composition change proposal created — workspace owner must accept or reject.",
    })))
}

// ─── POST /api/workspaces/:id/composition/dream ──────────────────────────────

pub async fn composition_dream_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_workspace_owner_or_admin(&state, &principal, workspace_id).await?;

    // Fetch workspace member agents + their valence from DB
    let rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias, a.valence \
         FROM workspace_agents wa \
         JOIN agents a ON a.agent_id = wa.agent_id \
         WHERE wa.workspace_id = $1 \
         ORDER BY wa.added_at",
    )
    .bind(workspace_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let member_summaries: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "agent_id": r.try_get::<Uuid, _>("agent_id").ok(),
                "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
                "display_alias": r.try_get::<Option<String>, _>("display_alias").unwrap_or(None),
                "valence": r.try_get::<Option<serde_json::Value>, _>("valence").unwrap_or(None),
            })
        })
        .collect();

    // Count pending proposals for context
    let versions = state
        .memory_store
        .list_composition_versions(workspace_id)
        .await
        .unwrap_or_default();

    let pending_count = versions
        .iter()
        .filter(|v| v.accepted_by.is_none() && v.rejected_by.is_none())
        .count();

    // Build the tension-audit prompt
    let dream_prompt = format!(
        "@cohere_and_coordinate [COMPOSITION DREAMING — TENSION AUDIT]\n\n\
         Review your consolidated episodic memory for this workspace. \
         Then assess the current team.\n\n\
         Current team ({} members):\n{}\n\n\
         Pending composition proposals: {}\n\n\
         Your task:\n\
         1. Read your consolidated dreaming episodes for recurring coherence patterns \
            (which TEC principles are chronically weak, what incoherence types recur).\n\
         2. Compute the team's valence distribution: arousal spread and valence spread \
            across the members above. Flag homophily if spread < 0.25 on either axis.\n\
         3. If you detect a structural issue (homophily, chronic destructive incoherence, \
            a role gap the current team cannot fill): call propose_composition_change \
            with evidence-grounded rationale. Name the pattern — do not pick the \
            replacement agent.\n\
         4. If productive incoherence is being suppressed (Γ(C) rising fast without \
            high P4 evidence engagement): issue an anti-convergence alert instead.\n\
         5. If the team is healthy, say so with specifics from your consolidated memory.\n\n\
         Feedback must be structural and evidence-grounded, not prescriptive.",
        member_summaries.len(),
        serde_json::to_string_pretty(&member_summaries).unwrap_or_default(),
        pending_count,
    );

    // Post as a workspace message — routes through normal execute flow,
    // response arrives via SSE stream.
    let msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id,
        sender_type: "user".to_string(),
        sender_id: principal.user_id(),
        sender_name: Some("system".to_string()),
        content: dream_prompt,
        message_type: "agent_invocation".to_string(),
        metadata: serde_json::json!({ "source": "composition_dream" }),
        created_at: Utc::now(),
    };

    state
        .memory_store
        .store_workspace_message(&msg)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Charge 5 credits from the workspace wallet (same as Dream Notes tier).
    // Fire-and-forget — don't fail the request if billing is unavailable.
    let ws_id_str = workspace_id.to_string();
    let _ = crate::handlers::workspace::charge_workspace_gas(
        &state.db,
        workspace_id,
        &ws_id_str,
        5,
        "composition_dream",
        "Composition dreaming session",
        None,
    )
    .await;

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "status": "dreaming_initiated",
        "message": "Composition dreaming session started. cohere_and_coordinate response will arrive in workspace chat.",
        "members_assessed": member_summaries.len(),
    })))
}
