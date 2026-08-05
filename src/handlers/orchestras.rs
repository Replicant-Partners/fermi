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

/// Names that trigger roster injection. Kept in sync with the
/// strategist_agent_name entries in the ORCHESTRAS const above.
const STRATEGIST_AGENTS: &[(&str, &str)] = &[
    ("fermi", "orchestra_fermi_members"),
    // `xaman_ek` intentionally skipped: it has 100+ members and its
    // own list_agents tool for catalogue queries. Injecting the full
    // roster into every invocation is prompt-token wasteful. Add
    // here when we build a compact per-tier / per-tag digest view.
];

/// Mutates `card.system_prompt` to append a live-roster block if the
/// agent is a known strategist. Returns the (possibly mutated) card.
///
/// Never fails: if the DB query errors out, we log and return the
/// card unchanged. A missing roster is far better than a failed
/// execution.
pub async fn inject_orchestra_context(
    db: &sqlx::PgPool,
    mut card: fermi::agent_backend::AgentCard,
) -> fermi::agent_backend::AgentCard {
    let Some((_, view_name)) = STRATEGIST_AGENTS
        .iter()
        .find(|(name, _)| *name == card.agent_id.as_str())
    else {
        return card;
    };

    let rows = match sqlx::query(&format!(
        "SELECT agent_name, agent_type, description \
           FROM public.{} \
          ORDER BY agent_name",
        view_name
    ))
    .fetch_all(db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!(
                "[orchestras] roster injection failed for {}: {} — continuing with static prompt",
                card.agent_id, e
            );
            return card;
        }
    };

    if rows.is_empty() {
        // No approved members yet. Skip injection rather than emit
        // an empty roster block (which would just confuse the LLM).
        return card;
    }

    // Build the roster block. Format is compact so it doesn't
    // dominate the strategist's prompt token budget — one line per
    // member with agent_type + short description.
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

    // Append; don't replace. The curated prompt has the strategist's
    // methodology and output shape — the roster is context added to
    // that. If the card has no system prompt at all (unusual for a
    // strategist), the roster still lands as the whole prompt.
    let existing = card.system_prompt.clone().unwrap_or_default();
    card.system_prompt = Some(format!("{}{}", existing, block));

    card
}

// ═══════════════════════════════════════════════════════════════════
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
