//! # Orchestras — first-class registry with admin-gated joins.
//!
//! v0.11.2. Makes orchestra membership a proper substrate instead of a
//! hidden `agents.fermi_contract IS NOT NULL` column condition.
//!
//! ## Model
//!
//! An "orchestra" is a named grouping of agents that share a
//! coordination pattern (Fermi = domain-constrained MoE for
//! forecasting; xaman_ek = platform-wide navigator). Two shapes:
//!
//!   * **Implicit orchestras** — membership = published. `xaman_ek` is
//!     one. Registration is a no-op; every publish auto-joins.
//!
//!   * **Explicit orchestras** — membership requires a declared
//!     contract on the agent (e.g. `fermi_contract`). `fermi` is one.
//!     Registration requires a request + admin approval.
//!
//! Approval sets the contract on `agents`; the roster view picks it up
//! automatically. Rejection preserves the rationale for
//! six-months-later readability.
//!
//! ## Football-manager model (see design conversation)
//!
//! The strategist's calibration is roster-locked (Brier lives in the
//! forecasts made by *this specific team*) but roster-orthogonal on
//! the delta (Team Brier − Counterfactual Brier isolates the
//! strategist's synthesis skill). This module owns the *roster* side.
//! The `fermi_forecasts.counterfactual_brier` column added by mig-172
//! reserves space for the delta computation; not populated by this
//! release.
//!
//! ## Admin gating
//!
//! An orchestra admin is the owner of the strategist agent card. For
//! `fermi`, that's whoever owns the `fermi` agent in the DB (currently
//! Ivan; transferable via the standard ownership-reassign path).
//! Platform admins can always approve regardless of orchestra
//! ownership.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::{resolve_agent, AppState};

// ═══════════════════════════════════════════════════════════════════
// Orchestra registry
// ═══════════════════════════════════════════════════════════════════

/// Known orchestras. Extend by adding an entry here + a view in a
/// migration. The strategist_agent_name is used to gate admin
/// approval — the owner of that agent is the orchestra's maintainer.
struct OrchestraSpec {
    name: &'static str,
    strategist_agent_name: Option<&'static str>,
    membership_rule: &'static str,
    view_name: &'static str,
    /// Implicit orchestras don't accept requests (membership is
    /// auto-derived from publish state).
    accepts_requests: bool,
}

const ORCHESTRAS: &[OrchestraSpec] = &[
    OrchestraSpec {
        name: "fermi",
        strategist_agent_name: Some("fermi"),
        membership_rule: "explicit: fermi_contract declared, admin-approved",
        view_name: "orchestra_fermi_members",
        accepts_requests: true,
    },
    OrchestraSpec {
        name: "xaman_ek",
        strategist_agent_name: Some("xaman_ek"),
        membership_rule: "implicit: every published agent",
        view_name: "orchestra_xaman_ek_members",
        accepts_requests: false,
    },
];

fn orchestra_by_name(name: &str) -> Option<&'static OrchestraSpec> {
    ORCHESTRAS.iter().find(|o| o.name == name)
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/orchestras
// ═══════════════════════════════════════════════════════════════════

pub async fn list_orchestras_handler(
    State(state): State<AppState>,
    _principal: Option<axum::extract::Extension<AuthPrincipal>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Anonymous-visible: the roster is public-facing information.
    let mut items: Vec<Value> = Vec::with_capacity(ORCHESTRAS.len());

    for spec in ORCHESTRAS {
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM public.{}", spec.view_name))
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);

        // Get strategist agent metadata for display.
        let strategist = if let Some(sname) = spec.strategist_agent_name {
            sqlx::query(
                "SELECT agent_id, agent_name, description, user_id \
                   FROM public.agents WHERE agent_name = $1",
            )
            .bind(sname)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(|row| {
                json!({
                    "agent_id":    row.try_get::<uuid::Uuid, _>("agent_id").ok(),
                    "agent_name":  row.try_get::<String, _>("agent_name").ok(),
                    "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
                    "owner_user_id": row.try_get::<Option<String>, _>("user_id").ok().flatten(),
                })
            })
        } else {
            None
        };

        items.push(json!({
            "name":              spec.name,
            "member_count":      count,
            "membership_rule":   spec.membership_rule,
            "accepts_requests":  spec.accepts_requests,
            "strategist":        strategist,
        }));
    }

    Ok(Json(json!({ "orchestras": items })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/orchestras/{name}/members
// ═══════════════════════════════════════════════════════════════════

pub async fn list_orchestra_members_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let spec = orchestra_by_name(&name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Unknown orchestra: {}", name),
        )
    })?;

    // Cast to text-array explicitly since the two views expose slightly
    // different column sets — a fully-typed query per view would be
    // cleaner, but format!() into a whitelisted view name is safe
    // (name comes from the ORCHESTRAS const, not user input).
    let rows = sqlx::query(&format!(
        "SELECT agent_id, agent_name, agent_type, tier, description, tags, \
                fermi_contract, output_contract, owner_user_id, \
                created_at, updated_at \
           FROM public.{} \
          ORDER BY agent_name",
        spec.view_name
    ))
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let members: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "agent_id":         r.try_get::<uuid::Uuid, _>("agent_id").ok(),
                "agent_name":       r.try_get::<String, _>("agent_name").ok(),
                "agent_type":       r.try_get::<String, _>("agent_type").ok(),
                "tier":             r.try_get::<String, _>("tier").ok(),
                "description":      r.try_get::<Option<String>, _>("description").ok().flatten(),
                "tags":             r.try_get::<Vec<String>, _>("tags").ok().unwrap_or_default(),
                "fermi_contract":   r.try_get::<Option<Value>, _>("fermi_contract").ok().flatten(),
                "output_contract":  r.try_get::<Option<Value>, _>("output_contract").ok().flatten(),
                "owner_user_id":    r.try_get::<Option<String>, _>("owner_user_id").ok().flatten(),
                "created_at":       r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|d| d.to_rfc3339()),
                "updated_at":       r.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "orchestra":       name,
        "membership_rule": spec.membership_rule,
        "member_count":    members.len(),
        "members":         members,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/agents/{id}/orchestras
// ═══════════════════════════════════════════════════════════════════

pub async fn agent_orchestras_handler(
    State(state): State<AppState>,
    Path(agent_id_or_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent = resolve_agent(&state, &agent_id_or_name).await?;

    let mut memberships: Vec<Value> = Vec::new();
    for spec in ORCHESTRAS {
        let is_member: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM public.{} WHERE agent_id = $1)",
            spec.view_name
        ))
        .bind(agent.agent_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

        memberships.push(json!({
            "orchestra":  spec.name,
            "is_member":  is_member,
            "membership_rule": spec.membership_rule,
        }));
    }

    // Also surface any pending requests this agent has open.
    let pending: Vec<Value> = sqlx::query(
        "SELECT request_id, orchestra_name, status, created_at, review_note \
           FROM public.orchestra_membership_requests \
          WHERE agent_id = $1 AND status = 'pending' \
          ORDER BY created_at DESC",
    )
    .bind(agent.agent_id)
    .fetch_all(&state.db)
    .await
    .map(|rows| {
        rows.iter()
            .map(|r| {
                json!({
                    "request_id":     r.try_get::<uuid::Uuid, _>("request_id").ok(),
                    "orchestra":      r.try_get::<String, _>("orchestra_name").ok(),
                    "status":         r.try_get::<String, _>("status").ok(),
                    "created_at":     r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|d| d.to_rfc3339()),
                    "review_note":    r.try_get::<Option<String>, _>("review_note").ok().flatten(),
                })
            })
            .collect()
    })
    .unwrap_or_default();

    Ok(Json(json!({
        "agent_id":         agent.agent_id.to_string(),
        "agent_name":       agent.agent_name,
        "memberships":      memberships,
        "pending_requests": pending,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/orchestras/{name}/requests — submit membership request
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct MembershipRequest {
    /// The agent being proposed for membership. Owner of this agent
    /// (or a platform admin acting on their behalf) is the only
    /// caller allowed to submit.
    pub agent_id: String,

    /// Proposed contract payload. For Fermi, expected shape:
    /// `{ finding_labels: [...], multiplier_range: [min,max],
    ///    kg_fact_categories: [...] }`. Validated per-orchestra
    /// below; unknown fields preserved verbatim.
    #[serde(default)]
    pub proposed_contract: Value,

    /// Optional free-form rationale ("why should this agent join?").
    #[serde(default)]
    pub rationale: Option<String>,
}

pub async fn submit_orchestra_request_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(name): Path<String>,
    Json(req): Json<MembershipRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let spec = orchestra_by_name(&name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Unknown orchestra: {}", name),
        )
    })?;

    if !spec.accepts_requests {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Orchestra '{}' does not accept explicit requests — membership is implicit ({})",
                name, spec.membership_rule
            ),
        ));
    }

    let agent = resolve_agent(&state, &req.agent_id).await?;
    let user_id = principal.user_id();

    // Ownership gate: only the agent's owner (or a platform admin)
    // can propose membership. This is a lifecycle-style operation,
    // not a public write.
    let is_owner = agent.owner_id.as_deref() == Some(&user_id);
    let is_admin = principal.can_admin();
    if !is_owner && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the agent's owner (or a platform admin) can submit a membership request".into(),
        ));
    }

    // Validate the proposed contract shape per-orchestra.
    if spec.name == "fermi" {
        validate_fermi_contract(&req.proposed_contract).map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid Fermi contract: {}", msg),
            )
        })?;
    }

    // Refuse duplicate pending request for the same (orchestra, agent).
    let already_pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM public.orchestra_membership_requests \
                        WHERE orchestra_name = $1 AND agent_id = $2 AND status = 'pending')",
    )
    .bind(&name)
    .bind(agent.agent_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);
    if already_pending {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "A pending {} membership request already exists for agent {}",
                name, agent.agent_name
            ),
        ));
    }

    // Refuse if agent is already a member (nothing to request).
    let already_member: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM public.{} WHERE agent_id = $1)",
        spec.view_name
    ))
    .bind(agent.agent_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);
    if already_member {
        return Err((
            StatusCode::CONFLICT,
            format!("Agent {} is already a {} member", agent.agent_name, name),
        ));
    }

    let request_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.orchestra_membership_requests \
         (request_id, orchestra_name, agent_id, requested_by, proposed_contract, rationale) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(request_id)
    .bind(&name)
    .bind(agent.agent_id)
    .bind(&user_id)
    .bind(&req.proposed_contract)
    .bind(&req.rationale)
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("insert request: {}", e),
        )
    })?;

    tracing::info!(
        orchestra = %name,
        agent_id = %agent.agent_id,
        agent_name = %agent.agent_name,
        requested_by = %user_id,
        request_id = %request_id,
        "Orchestra membership request submitted"
    );

    Ok(Json(json!({
        "request_id":   request_id,
        "orchestra":    name,
        "agent_id":     agent.agent_id,
        "agent_name":   agent.agent_name,
        "status":       "pending",
        "next_step":    format!("A {} orchestra admin will review this request. You can withdraw it at any time.", name),
    })))
}

/// Minimal Fermi-contract shape validation. Rejects obviously-wrong
/// payloads early so the admin doesn't waste review cycles on typos.
fn validate_fermi_contract(v: &Value) -> Result<(), String> {
    let obj = v
        .as_object()
        .ok_or("proposed_contract must be a JSON object")?;

    // `finding_labels` is the only strictly-required field. Enforces
    // that at least one label is declared (MULTIPLIER is expected
    // per the Fermi orchestra protocol).
    let labels = obj
        .get("finding_labels")
        .ok_or("finding_labels is required (e.g. [\"BASE RATE\", \"MULTIPLIER\"])")?
        .as_array()
        .ok_or("finding_labels must be an array")?;
    if labels.is_empty() {
        return Err("finding_labels must have at least one entry".into());
    }
    for (i, l) in labels.iter().enumerate() {
        if !l.is_string() {
            return Err(format!("finding_labels[{}] must be a string", i));
        }
    }

    // multiplier_range: optional; if present must be [min, max] with min < max.
    if let Some(r) = obj.get("multiplier_range") {
        let arr = r.as_array().ok_or("multiplier_range must be [min, max]")?;
        if arr.len() != 2 {
            return Err("multiplier_range must be a 2-element array".into());
        }
        let min = arr[0]
            .as_f64()
            .ok_or("multiplier_range[0] must be a number")?;
        let max = arr[1]
            .as_f64()
            .ok_or("multiplier_range[1] must be a number")?;
        if !(min < max) {
            return Err("multiplier_range min must be < max".into());
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/orchestras/{name}/requests — admin inbox
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct RequestsQuery {
    /// Filter by status. Default: `pending`.
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn list_orchestra_requests_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(name): Path<String>,
    Query(q): Query<RequestsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_orchestra_admin(&state, &principal, &name).await?;

    let status_filter = q.status.as_deref().unwrap_or("pending");

    let rows = sqlx::query(
        "SELECT r.request_id, r.orchestra_name, r.agent_id, r.requested_by, \
                r.proposed_contract, r.rationale, r.status, \
                r.reviewed_by, r.reviewed_at, r.review_note, \
                r.created_at, r.updated_at, \
                a.agent_name, a.agent_type, a.tier, a.description, \
                a.status AS agent_status, a.total_executions, \
                COALESCE(u.display_name, u.email, u.user_id) AS requester_display \
           FROM public.orchestra_membership_requests r \
           JOIN public.agents a ON a.agent_id = r.agent_id \
           LEFT JOIN public.users u ON u.user_id = r.requested_by \
          WHERE r.orchestra_name = $1 \
            AND ($2 = 'all' OR r.status = $2) \
          ORDER BY r.created_at DESC \
          LIMIT 200",
    )
    .bind(&name)
    .bind(status_filter)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "request_id":         r.try_get::<uuid::Uuid, _>("request_id").ok(),
                "orchestra":          r.try_get::<String, _>("orchestra_name").ok(),
                "status":             r.try_get::<String, _>("status").ok(),
                "agent": {
                    "agent_id":         r.try_get::<uuid::Uuid, _>("agent_id").ok(),
                    "agent_name":       r.try_get::<String, _>("agent_name").ok(),
                    "agent_type":       r.try_get::<String, _>("agent_type").ok(),
                    "tier":             r.try_get::<String, _>("tier").ok(),
                    "description":      r.try_get::<Option<String>, _>("description").ok().flatten(),
                    "status":           r.try_get::<String, _>("agent_status").ok(),
                    "total_executions": r.try_get::<i32, _>("total_executions").ok(),
                },
                "requester": {
                    "user_id":    r.try_get::<String, _>("requested_by").ok(),
                    "display":    r.try_get::<Option<String>, _>("requester_display").ok().flatten(),
                },
                "proposed_contract":  r.try_get::<Value, _>("proposed_contract").ok(),
                "rationale":          r.try_get::<Option<String>, _>("rationale").ok().flatten(),
                "reviewed_by":        r.try_get::<Option<String>, _>("reviewed_by").ok().flatten(),
                "reviewed_at":        r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("reviewed_at").ok().flatten().map(|d| d.to_rfc3339()),
                "review_note":        r.try_get::<Option<String>, _>("review_note").ok().flatten(),
                "created_at":         r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "orchestra": name,
        "status":    status_filter,
        "count":     items.len(),
        "requests":  items,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/orchestras/{name}/requests/{id}/approve
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct ApprovalRequest {
    /// Optional note recorded with the approval (visible to requester).
    #[serde(default)]
    pub note: Option<String>,

    /// Optionally override the proposed contract at approval time
    /// (admin might trim `finding_labels` or narrow `multiplier_range`
    /// during review). If absent, the proposed_contract from the
    /// request is used verbatim.
    #[serde(default)]
    pub final_contract: Option<Value>,
}

pub async fn approve_orchestra_request_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((name, request_id)): Path<(String, String)>,
    Json(req): Json<ApprovalRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_orchestra_admin(&state, &principal, &name).await?;

    let request_uuid: uuid::Uuid = request_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid request_id UUID".to_string(),
        )
    })?;

    // Load the pending request.
    let row = sqlx::query(
        "SELECT agent_id, proposed_contract, status \
           FROM public.orchestra_membership_requests \
          WHERE request_id = $1 AND orchestra_name = $2",
    )
    .bind(request_uuid)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Request not found".to_string()))?;

    let current_status: String = row.try_get("status").unwrap_or_default();
    if current_status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            format!("Request is not pending (current: {})", current_status),
        ));
    }

    let agent_id: uuid::Uuid = row.try_get("agent_id").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("row shape: {}", e),
        )
    })?;
    let proposed: Value = row.try_get("proposed_contract").unwrap_or(Value::Null);
    let final_contract = req.final_contract.unwrap_or(proposed);
    let admin_user_id = principal.user_id();

    // Everything happens in one transaction so partial state can't
    // leak (contract set but request still pending, or vice versa).
    let mut tx = state.db.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("begin tx: {}", e),
        )
    })?;

    // Set the contract on the agent. For 'fermi' the column is
    // `fermi_contract`; extending to other orchestras will need a
    // per-orchestra mapping here.
    if name == "fermi" {
        sqlx::query(
            "UPDATE public.agents SET fermi_contract = $1, updated_at = NOW() \
              WHERE agent_id = $2",
        )
        .bind(&final_contract)
        .bind(agent_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("set contract: {}", e),
            )
        })?;
    } else {
        // Placeholder for future explicit orchestras. Should never
        // reach here today because only 'fermi' has accepts_requests
        // = true.
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("no contract mapping for orchestra '{}'", name),
        ));
    }

    // Update the request.
    sqlx::query(
        "UPDATE public.orchestra_membership_requests \
            SET status = 'approved', reviewed_by = $1, reviewed_at = NOW(), \
                review_note = $2, proposed_contract = $3, updated_at = NOW() \
          WHERE request_id = $4",
    )
    .bind(&admin_user_id)
    .bind(&req.note)
    .bind(&final_contract)
    .bind(request_uuid)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("update request: {}", e),
        )
    })?;

    // Governance audit trail (mig-164 lives on).
    sqlx::query(
        "INSERT INTO public.admin_bypass_events \
         (admin_user_id, target_type, target_id, action, details) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&admin_user_id)
    .bind("agent")
    .bind(agent_id.to_string())
    .bind("orchestra_approve")
    .bind(json!({
        "orchestra":  name,
        "request_id": request_uuid.to_string(),
        "contract":   final_contract,
        "note":       req.note,
    }))
    .execute(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("audit: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("commit: {}", e)))?;

    tracing::info!(
        orchestra = %name,
        agent_id = %agent_id,
        approved_by = %admin_user_id,
        request_id = %request_uuid,
        "Orchestra membership request approved"
    );

    Ok(Json(json!({
        "request_id":  request_uuid,
        "orchestra":   name,
        "agent_id":    agent_id,
        "status":      "approved",
        "reviewed_by": admin_user_id,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/orchestras/{name}/requests/{id}/reject
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct RejectionRequest {
    /// Required. What did the admin see that blocked approval?
    /// Preserved on the request row so the requester can revise and
    /// re-submit.
    pub note: String,
}

pub async fn reject_orchestra_request_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((name, request_id)): Path<(String, String)>,
    Json(req): Json<RejectionRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_orchestra_admin(&state, &principal, &name).await?;

    if req.note.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "A rejection note is required so the requester knows why and how to revise".into(),
        ));
    }

    let request_uuid: uuid::Uuid = request_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid request_id UUID".to_string(),
        )
    })?;

    let admin_user_id = principal.user_id();

    let n_updated = sqlx::query(
        "UPDATE public.orchestra_membership_requests \
            SET status = 'rejected', reviewed_by = $1, reviewed_at = NOW(), \
                review_note = $2, updated_at = NOW() \
          WHERE request_id = $3 AND orchestra_name = $4 AND status = 'pending'",
    )
    .bind(&admin_user_id)
    .bind(&req.note)
    .bind(request_uuid)
    .bind(&name)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .rows_affected();

    if n_updated == 0 {
        return Err((
            StatusCode::CONFLICT,
            "Request not found or not pending".into(),
        ));
    }

    tracing::info!(
        orchestra = %name,
        rejected_by = %admin_user_id,
        request_id = %request_uuid,
        "Orchestra membership request rejected"
    );

    Ok(Json(json!({
        "request_id":  request_uuid,
        "orchestra":   name,
        "status":      "rejected",
        "reviewed_by": admin_user_id,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/orchestras/{name}/requests/{id}/withdraw
// ═══════════════════════════════════════════════════════════════════

pub async fn withdraw_orchestra_request_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((name, request_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let request_uuid: uuid::Uuid = request_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid request_id UUID".to_string(),
        )
    })?;

    let user_id = principal.user_id();
    let is_admin = principal.can_admin();

    // Anyone who submitted the request can withdraw it. Platform
    // admins can withdraw any request. Orchestra admins go through
    // reject with a note.
    let row = sqlx::query(
        "SELECT requested_by, status \
           FROM public.orchestra_membership_requests \
          WHERE request_id = $1 AND orchestra_name = $2",
    )
    .bind(request_uuid)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Request not found".to_string()))?;

    let requester: String = row.try_get("requested_by").unwrap_or_default();
    let status: String = row.try_get("status").unwrap_or_default();

    if status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            format!("Request is not pending (current: {})", status),
        ));
    }
    if requester != user_id && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the original requester or a platform admin can withdraw".into(),
        ));
    }

    sqlx::query(
        "UPDATE public.orchestra_membership_requests \
            SET status = 'withdrawn', updated_at = NOW() \
          WHERE request_id = $1",
    )
    .bind(request_uuid)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "request_id": request_uuid,
        "status":     "withdrawn",
    })))
}

// ═══════════════════════════════════════════════════════════════════
// v0.11.3 — Dynamic roster injection into strategist system prompts
// ═══════════════════════════════════════════════════════════════════
//
// Fermi's system prompt in agents/curated/fermi/agent_card.json used
// to hard-code its specialist roster (macro_forecaster, equity_analyst,
// sentiment_analyzer, entity_investigator). That meant Mario's
// approved guidance_tracker never appeared in Fermi's decomposition
// plan — the curated prompt didn't know about it.
//
// Fix: at execute time, look up the strategist's live roster from the
// orchestra_*_members view and append a `## CURRENT ROSTER` block to
// the system prompt. Fermi sees the actual approved members as of
// this invocation, no card edit required when new members join.
//
// Called from execute_agent_handler and execute_agent_stream_handler.
// Non-strategist agents pass through unchanged.

/// Injection strategy per strategist. Small orchestras get a full
/// per-member roster; large orchestras get a per-tier digest so the
/// prompt-token budget doesn't blow up as the catalogue grows.
enum InjectionStrategy {
    /// One line per member with `agent_type` and a short description.
    /// Suitable up to ~30 members (±1k tokens).
    FullRoster { view: &'static str },
    /// Per-tier counts + N exemplar names per tier, plus a nudge to
    /// use `list_agents` for anything not in the digest. Bounded
    /// regardless of catalogue size.
    TierDigest {
        view: &'static str,
        exemplars_per_tier: usize,
    },
}

/// Strategist agents that get roster context injected into their
/// system prompt at execute time. Kept in sync with the
/// strategist_agent_name entries in the ORCHESTRAS const above.
///
/// - `fermi` gets a full roster (small, curated, structural).
/// - `xaman_ek` gets a tier digest (large, open, ontological).
fn strategist_injection(agent_id: &str) -> Option<InjectionStrategy> {
    match agent_id {
        "fermi" => Some(InjectionStrategy::FullRoster {
            view: "orchestra_fermi_members",
        }),
        "xaman_ek" => Some(InjectionStrategy::TierDigest {
            view: "orchestra_xaman_ek_members",
            // 8 names per tier keeps the block ~500 tokens even at
            // 500+ members. `list_agents` handles anything beyond
            // the digest.
            exemplars_per_tier: 8,
        }),
        _ => None,
    }
}

/// Mutates `card.system_prompt` to append a roster/digest block if
/// the agent is a known strategist. Returns the (possibly mutated)
/// card.
///
/// Never fails: if the DB query errors out, we log and return the
/// card unchanged. A missing roster is far better than a failed
/// execution.
pub async fn inject_orchestra_context(
    db: &sqlx::PgPool,
    mut card: fermi::agent_backend::AgentCard,
) -> fermi::agent_backend::AgentCard {
    let Some(strategy) = strategist_injection(card.agent_id.as_str()) else {
        return card;
    };

    let block_opt = match strategy {
        InjectionStrategy::FullRoster { view } => {
            build_full_roster_block(db, &card.agent_id, view).await
        }
        InjectionStrategy::TierDigest {
            view,
            exemplars_per_tier,
        } => build_tier_digest_block(db, &card.agent_id, view, exemplars_per_tier).await,
    };

    let Some(block) = block_opt else {
        return card;
    };

    // Append; don't replace. The curated prompt has the strategist's
    // methodology and output shape — the roster is context added to
    // that. If the card has no system prompt at all (unusual for a
    // strategist), the roster still lands as the whole prompt.
    let existing = card.system_prompt.clone().unwrap_or_default();
    card.system_prompt = Some(format!("{}{}", existing, block));

    card
}

/// Full-roster block: one line per member. See `FullRoster`.
async fn build_full_roster_block(db: &sqlx::PgPool, agent_id: &str, view: &str) -> Option<String> {
    let rows = match sqlx::query(&format!(
        "SELECT agent_name, agent_type, description \
           FROM public.{} \
          ORDER BY agent_name",
        view
    ))
    .fetch_all(db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!(
                "[orchestras] full-roster injection failed for {}: {} — continuing with static prompt",
                agent_id, e
            );
            return None;
        }
    };

    if rows.is_empty() {
        // No approved members yet. Skip injection rather than emit
        // an empty roster block (which would just confuse the LLM).
        return None;
    }

    let mut block = String::from(
        "\n\n## CURRENT ROSTER (dynamic, v0.11.3+)\n\n\
         You are the strategist of this orchestra. The following members are \
         currently approved for consultation. Prefer these members when \
         decomposing your task via execute_agent. If a needed specialty isn't \
         covered by the current roster, note the gap in your decomposition \
         rather than inventing an agent name that doesn't exist.\n\n",
    );

    for row in &rows {
        let name: String = row.try_get("agent_name").unwrap_or_default();
        let agent_type: String = row.try_get("agent_type").unwrap_or_default();
        let desc: Option<String> = row.try_get("description").ok().flatten();
        let short_desc = desc
            .as_deref()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(140)
            .collect::<String>();
        block.push_str(&format!("- `{}` ({}) — {}\n", name, agent_type, short_desc));
    }

    Some(block)
}

/// Tier-digest block: per-tier counts + N exemplar names per tier,
/// plus a nudge to use `list_agents` for anything not shown. See
/// `TierDigest`.
///
/// Bounded output: regardless of catalogue size, the block is roughly
/// (n_tiers * (exemplars_per_tier + 2 lines of context)) — typically
/// well under 500 tokens.
async fn build_tier_digest_block(
    db: &sqlx::PgPool,
    agent_id: &str,
    view: &str,
    exemplars_per_tier: usize,
) -> Option<String> {
    // One query, one round-trip: aggregate counts + top-N exemplar
    // names per tier via a windowed subselect. `array_agg` respects
    // the FILTER clause, so we get exactly the first N alphabetical
    // names per tier without post-processing in Rust.
    //
    // ORDER BY agent_name is deterministic and human-scannable.
    // A future refinement can order by total_executions once the
    // xaman_ek view exposes it.
    let sql = format!(
        "SELECT tier, \
                COUNT(*) AS n_total, \
                array_agg(agent_name ORDER BY agent_name) \
                    FILTER (WHERE rn <= $1) AS exemplars \
           FROM (SELECT tier, agent_name, \
                        row_number() OVER (PARTITION BY tier ORDER BY agent_name) AS rn \
                   FROM public.{}) t \
          GROUP BY tier \
          ORDER BY \
            CASE tier \
              WHEN 'system'    THEN 0 \
              WHEN 'curated'   THEN 1 \
              WHEN 'community' THEN 2 \
              ELSE 3 \
            END, tier",
        view
    );

    let rows = match sqlx::query(&sql)
        .bind(exemplars_per_tier as i64)
        .fetch_all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!(
                "[orchestras] tier-digest injection failed for {}: {} — continuing with static prompt",
                agent_id, e
            );
            return None;
        }
    };

    if rows.is_empty() {
        return None;
    }

    // Compute the grand total for the header line.
    let n_total: i64 = rows
        .iter()
        .map(|r| r.try_get::<i64, _>("n_total").unwrap_or(0))
        .sum();

    let mut block = format!(
        "\n\n## CATALOGUE DIGEST (dynamic, v0.11.3+)\n\n\
         You are the platform navigator. The Bestiary currently has \
         {} published agents across the tiers below. Per-tier counts \
         plus a small alphabetical sample of names are shown so you \
         can answer catalogue-shape questions inline. For any \
         specific agent or capability not visible in this digest, \
         use your `list_agents` tool — don't guess or invent names.\n\n",
        n_total
    );

    for row in &rows {
        let tier: String = row.try_get("tier").unwrap_or_default();
        let n: i64 = row.try_get("n_total").unwrap_or(0);
        let names: Vec<String> = row
            .try_get::<Vec<String>, _>("exemplars")
            .unwrap_or_default();
        let sample = names.join(", ");
        let overflow = (n as usize).saturating_sub(names.len());
        let overflow_str = if overflow > 0 {
            format!(" (+{} more)", overflow)
        } else {
            String::new()
        };
        block.push_str(&format!(
            "- **{}** ({} agents): {}{}\n",
            tier, n, sample, overflow_str
        ));
    }

    Some(block)
}

// ════════════════════════════════════════════════════════════════
// v0.11.3-follow-up — Manager-effect readout
// ════════════════════════════════════════════════════════════════
//
// GET /api/orchestras/:name/manager-effect
//
// Returns the strategist's public track record: aggregate Brier and
// counterfactual Brier over resolved forecasts that used the
// strategist, plus the last N rows for a timeline chart. The
// per-forecast `manager_effect = brier_score - counterfactual_brier`
// is the roster-orthogonal signal defined by the football-manager
// model.
//
// Only meaningful for strategist orchestras (fermi). Anonymous-
// visible — leaderboards are public-facing.

#[derive(Deserialize)]
pub struct ManagerEffectQuery {
    /// Cap on the returned forecast rows. Default 50, hard max 200.
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn orchestra_manager_effect_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<ManagerEffectQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let spec = orchestra_by_name(&name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Unknown orchestra: {}", name),
        )
    })?;
    let strategist = spec.strategist_agent_name.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            format!(
                "Orchestra '{}' has no strategist — manager-effect is undefined",
                name
            ),
        )
    })?;

    let limit = q.limit.unwrap_or(50).clamp(1, 200);

    // Aggregate row. Both averages are over the same predicate
    // (resolved forecasts that used the strategist), so counts and
    // NULL handling stay consistent. `n_with_counterfactual` <=
    // `n_resolved` because pre-v0.11.3-follow-up forecasts and
    // drafts saved before base-rate research populate NULL.
    let agg_row = sqlx::query(
        r#"SELECT
             COUNT(*) FILTER (WHERE brier_score IS NOT NULL)             AS n_resolved,
             COUNT(*) FILTER (WHERE counterfactual_brier IS NOT NULL)    AS n_with_counterfactual,
             AVG(brier_score)::float8                                    AS mean_brier,
             AVG(counterfactual_brier)::float8                           AS mean_counterfactual,
             AVG(brier_score - counterfactual_brier)
                 FILTER (WHERE counterfactual_brier IS NOT NULL)::float8 AS mean_manager_effect
           FROM public.fermi_forecasts
          WHERE status = 'resolved'
            AND brier_score IS NOT NULL
            AND agents_used @> jsonb_build_array(
                    jsonb_build_object('agent_name', $1::text))"#,
    )
    .bind(strategist)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let n_resolved: i64 = agg_row.try_get("n_resolved").unwrap_or(0);
    let n_with_counterfactual: i64 = agg_row.try_get("n_with_counterfactual").unwrap_or(0);
    let mean_brier: Option<f64> = agg_row.try_get("mean_brier").ok().flatten();
    let mean_counterfactual: Option<f64> = agg_row.try_get("mean_counterfactual").ok().flatten();
    let mean_manager_effect: Option<f64> = agg_row.try_get("mean_manager_effect").ok().flatten();

    // Timeline rows — resolved forecasts, most recent first. We
    // include NULL-counterfactual rows so the client can render
    // brier-only history for pre-v0.11.3-follow-up data with a
    // "cf: n/a" marker; charts filter to rows where both fields
    // exist.
    let rows = sqlx::query(
        r#"SELECT id::text                       AS id,
                  question_text,
                  predicted_probability,
                  counterfactual_probability,
                  actual_outcome,
                  brier_score,
                  counterfactual_brier,
                  resolved_at
             FROM public.fermi_forecasts
            WHERE status = 'resolved'
              AND brier_score IS NOT NULL
              AND agents_used @> jsonb_build_array(
                      jsonb_build_object('agent_name', $1::text))
            ORDER BY resolved_at DESC
            LIMIT $2"#,
    )
    .bind(strategist)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let forecasts: Vec<Value> = rows
        .iter()
        .map(|r| {
            // Compute manager_effect out of the json! macro — same
            // reason as get_forecast_handler in forecasts.rs.
            let brier: Option<f64> = r
                .try_get::<Option<f32>, _>("brier_score")
                .ok()
                .flatten()
                .map(|v| v as f64);
            let cf_brier: Option<f64> = r
                .try_get::<Option<f32>, _>("counterfactual_brier")
                .ok()
                .flatten()
                .map(|v| v as f64);
            let manager_effect: Option<f64> = match (brier, cf_brier) {
                (Some(b), Some(c)) => Some(b - c),
                _ => None,
            };
            json!({
                "id":                          r.try_get::<String, _>("id").ok(),
                "question_text":               r.try_get::<String, _>("question_text").ok(),
                "predicted_probability":       r.try_get::<Option<f32>, _>("predicted_probability").ok().flatten().map(|v| v as f64),
                "counterfactual_probability":  r.try_get::<Option<f32>, _>("counterfactual_probability").ok().flatten().map(|v| v as f64),
                "actual_outcome":              r.try_get::<Option<bool>, _>("actual_outcome").ok().flatten(),
                "brier_score":                 brier,
                "counterfactual_brier":        cf_brier,
                "manager_effect":              manager_effect,
                "resolved_at":                 r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at").ok().flatten().map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "orchestra":              name,
        "strategist":             strategist,
        "n_resolved":             n_resolved,
        "n_with_counterfactual":  n_with_counterfactual,
        "mean_brier":             mean_brier,
        "mean_counterfactual":    mean_counterfactual,
        "mean_manager_effect":    mean_manager_effect,
        "forecasts":              forecasts,
    })))
}

// ════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

/// Admin gate: the caller must own the orchestra's strategist agent
/// OR be a platform admin. Fails with 403 otherwise.
async fn require_orchestra_admin(
    state: &AppState,
    principal: &AuthPrincipal,
    orchestra_name: &str,
) -> Result<(), (StatusCode, String)> {
    // Platform admins always allowed.
    if principal.can_admin() {
        return Ok(());
    }

    let spec = orchestra_by_name(orchestra_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Unknown orchestra: {}", orchestra_name),
        )
    })?;

    let strategist_name = spec.strategist_agent_name.ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            format!("Orchestra '{}' has no admin surface", orchestra_name),
        )
    })?;

    // Load strategist agent to find its owner.
    let strategist_owner: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM public.agents WHERE agent_name = $1")
            .bind(strategist_name)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .flatten();

    match strategist_owner {
        Some(owner) if owner == principal.user_id() => Ok(()),
        Some(_) => Err((
            StatusCode::FORBIDDEN,
            format!(
                "Only the owner of the '{}' strategist agent (or a platform admin) can approve/reject",
                strategist_name
            ),
        )),
        None => Err((
            StatusCode::FORBIDDEN,
            format!(
                "Orchestra '{}' has no strategist owner (agent '{}' missing or unowned)",
                orchestra_name, strategist_name
            ),
        )),
    }
}
