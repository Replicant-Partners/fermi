//! Workspace Resolution
//!
//! Generalises forecast resolution from `fermi_forecasts` (single-question
//! Yes/No probability) to ANY workspace. Every workspace is a belief that
//! will eventually be resolved — a team's WC win probability resolves at
//! tournament end, an H2H match workspace resolves when the match is
//! played, a generic forecast workspace resolves at its target_date.
//!
//! This handler is the universal resolution entry point. It:
//!
//!   1. Validates the workspace exists, the caller is a member, and the
//!      workspace is in a resolvable state (`active`).
//!   2. Writes the resolution metadata to `teams` (resolved_at,
//!      resolution_outcome JSONB, resolution_source, etc.) AND transitions
//!      `workspace_status` to `completed` (or `failed` if caller opted in
//!      via the `failure` flag — used for "this never happened" closures).
//!   3. Publishes the outcome as a workspace output keyed `'resolution'`
//!      so it gets the cross-workspace dependency propagation for free
//!      (downstream workspaces receive a `upstream_output_updated` event
//!      and BayesOps refit can wake them up).
//!   4. Computes a Brier score against the workspace's last published
//!      probability output (`predicted_probability` by convention) if
//!      both inputs are binary-valued.
//!   5. **BAYESOPS HOOK** — calls `refit_workspace` for THIS workspace
//!      and every upstream workspace whose outputs this one consumes.
//!      Today this is a TODO insertion point waiting for the BayesOps
//!      string's implementation; the resolution metadata is fully
//!      written before the hook fires so a failure in the hook never
//!      corrupts the resolution itself.
//!
//! Routes:
//!   POST /api/workspaces/:workspace_id/resolve
//!
//! Request body:
//!   {
//!     "outcome": <any JSON value — domain-specific shape>,
//!     "resolved_at": <RFC3339 timestamp, optional, defaults to now>,
//!     "resolution_notes": <string, optional>,
//!     "resolution_source": <string, optional — provenance tag>,
//!     "failure": <bool, optional, default false — set true to mark
//!                 workspace_status = 'failed' instead of 'completed'>,
//!   }
//!
//! Response body:
//!   {
//!     "workspace_id": "...",
//!     "workspace_status": "completed" | "failed",
//!     "resolved_at": "...",
//!     "outcome": { ... },
//!     "brier_score": <f32 or null>,
//!     "predicted_probability": <f64 or null — what we scored against>,
//!     "downstream_notified": <integer — count of workspaces awakened>,
//!     "refit_triggered": <bool — was the BayesOps hook called>,
//!   }

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::Row;

use crate::AppState;
use fermi_auth::{teams, AuthPrincipal};

// ─── Request / response types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ResolveWorkspaceRequest {
    /// Domain-specific outcome payload. The shape depends on the
    /// workspace's question; e.g. `{ "won_tournament": true }` for a
    /// team-prior workspace, `{ "winner_team_id": "ARG" }` for an H2H
    /// match. The handler does not interpret the JSON structure — that's
    /// the consumer's job. We do however look for the conventional
    /// `value: <0.0..=1.0>` field when computing the Brier score against
    /// the workspace's published `predicted_probability` (see below).
    pub outcome: JsonValue,

    /// Optional caller-provided resolution timestamp (RFC3339). Defaults
    /// to NOW(). Useful when back-filling outcomes that became known
    /// earlier than the resolve call.
    #[serde(default)]
    pub resolved_at: Option<DateTime<Utc>>,

    #[serde(default)]
    pub resolution_notes: Option<String>,

    /// Provenance tag. Free-form. Recommend one of:
    ///   "manual_user", "fifa_official", "polymarket_resolution",
    ///   "automated_target_date", "external_api:<source>".
    #[serde(default)]
    pub resolution_source: Option<String>,

    /// Default false. When true, transitions the workspace to
    /// `workspace_status = 'failed'` instead of `'completed'`. Used for
    /// workspaces that close without a meaningful resolution (e.g. the
    /// underlying question became undefined, the tournament was
    /// cancelled, the asset stopped trading).
    #[serde(default)]
    pub failure: bool,
}

// ─── Handler ─────────────────────────────────────────────────────────

/// POST /api/workspaces/:workspace_id/resolve
pub async fn resolve_workspace_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<ResolveWorkspaceRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".into()))?;

    // ── Membership check ──
    teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".into()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".into()))?;

    // ── Workspace state precondition ──
    //
    // Only `active` workspaces can be resolved. Resolving a workspace
    // twice is a usage error (use a separate `update-resolution`
    // endpoint if/when we need to amend a resolution after the fact).
    let current_status: Option<String> = sqlx::query_scalar(
        "SELECT workspace_status FROM teams WHERE id = $1"
    )
    .bind(ws_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let current_status = current_status
        .ok_or((StatusCode::NOT_FOUND, "Workspace not found".into()))?;

    if current_status != "active" {
        return Err((
            StatusCode::CONFLICT,
            format!("Workspace is already in '{}' state; only 'active' workspaces can be resolved", current_status),
        ));
    }

    let resolved_at = req.resolved_at.unwrap_or_else(Utc::now);
    let new_status = if req.failure { "failed" } else { "completed" };

    // ── Brier score against last published predicted_probability ──
    //
    // Convention: a workspace that publishes a binary probability does
    // so under the key `predicted_probability` with value
    // `{ "value": <0.0..=1.0>, ... }`. The published value can be either
    //   - a scalar (single probability), or
    //   - an object containing a `value` field.
    // We pull the most recent version. If neither the published output
    // nor the resolution outcome supports a binary scoring, brier stays
    // None and the column stays NULL — that's fine, the resolution
    // still happens.
    let predicted_probability: Option<f64> = sqlx::query_scalar(
        "SELECT value FROM workspace_outputs
         WHERE workspace_id = $1 AND key = 'predicted_probability'"
    )
    .bind(ws_uuid)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .and_then(|v: JsonValue| extract_probability(&v));

    let outcome_binary: Option<f64> = extract_probability(&req.outcome);

    let brier_score: Option<f32> = match (predicted_probability, outcome_binary) {
        (Some(p), Some(o)) => Some(((p - o).powi(2)) as f32),
        _ => None,
    };

    // ── Begin DB transaction so the resolve is atomic ──
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Write resolution metadata to teams ──
    sqlx::query(
        "UPDATE teams SET
            workspace_status   = $2,
            resolved_at        = $3,
            resolved_by        = $4,
            resolution_outcome = $5,
            resolution_notes   = $6,
            resolution_source  = $7,
            brier_score        = $8
         WHERE id = $1"
    )
    .bind(ws_uuid)
    .bind(new_status)
    .bind(resolved_at)
    .bind(&user_id)
    .bind(&req.outcome)
    .bind(req.resolution_notes.as_deref())
    .bind(req.resolution_source.as_deref())
    .bind(brier_score)
    .execute(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Publish outcome as workspace_outputs row ──
    //
    // Two reasons to also write the outcome here (in addition to the
    // teams column):
    //   1. The dependency-propagation event fan-out in set_output_handler
    //      is keyed on workspace_outputs writes. By doing the write
    //      ourselves, downstream workspaces awake via the same
    //      `upstream_output_updated` mechanism that any other output
    //      change uses.
    //   2. The outputs API surface is the public, versioned consumable.
    //      Anything reading from `workspace_outputs[...].resolution`
    //      sees the same data as a teams-column query.
    sqlx::query(
        "INSERT INTO workspace_outputs (workspace_id, key, value, version, updated_at, updated_by)
         VALUES ($1, 'resolution', $2, 1, NOW(), $3)
         ON CONFLICT (workspace_id, key) DO UPDATE SET
            value      = EXCLUDED.value,
            version    = workspace_outputs.version + 1,
            updated_at = NOW(),
            updated_by = EXCLUDED.updated_by"
    )
    .bind(ws_uuid)
    .bind(json!({
        "outcome":            req.outcome,
        "resolved_at":        resolved_at,
        "resolved_by":        user_id,
        "workspace_status":   new_status,
        "resolution_source":  req.resolution_source,
        "resolution_notes":   req.resolution_notes,
        "brier_score":        brier_score,
        "predicted_probability": predicted_probability,
    }))
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Notify downstream workspaces ──
    //
    // Mirror the fan-out behaviour of set_output_handler. We do it
    // INSIDE the transaction so the resolution and the notifications
    // commit together; the receivers see the world consistently.
    let downstream: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT downstream_id FROM workspace_dependencies
         WHERE upstream_id = $1
           AND (key_filter IS NULL OR key_filter = 'resolution')",
    )
    .bind(ws_uuid)
    .fetch_all(&mut *tx)
    .await
    .unwrap_or_default();

    for ds_id in &downstream {
        let _ = sqlx::query(
            "INSERT INTO workspace_messages (workspace_id, sender_type, sender_id, sender_name, content, message_type, metadata)
             VALUES ($1, 'system', $2, 'Workspace Resolved', $3, 'system_event', $4)",
        )
        .bind(ds_id)
        .bind(&workspace_id)
        .bind(format!("Upstream workspace {} resolved with outcome", workspace_id))
        .bind(json!({
            "event": "upstream_resolved",
            "upstream_workspace_id": workspace_id,
            "outcome": req.outcome,
            "brier_score": brier_score,
        }))
        .execute(&mut *tx)
        .await;
    }

    tx.commit()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ─────────────────────────────────────────────────────────────────
    //
    //  ╔══════════════════════════════════════════════════════════════╗
    //  ║                                                              ║
    //  ║   BAYESOPS HOOK — INSERT POINT                              ║
    //  ║                                                              ║
    //  ║   The parallel BayesOps string fills this in. Spec:         ║
    //  ║                                                              ║
    //  ║   1. Call `refit_workspace(ws_uuid)` for THIS workspace.    ║
    //  ║      Reads:  workspace_outputs[ws_uuid].resolution           ║
    //  ║              workspace_outputs[ws_uuid].observations[] (opt)║
    //  ║      Writes: params.<driver_name>_fitted as                 ║
    //  ║              FittedDistribution JSON                         ║
    //  ║                                                              ║
    //  ║   2. For each UPSTREAM workspace whose outputs this one     ║
    //  ║      consumed (read from workspace_dependencies), call      ║
    //  ║      `refit_workspace(upstream_id)`. Their priors should    ║
    //  ║      update against the observed downstream outcome.        ║
    //  ║                                                              ║
    //  ║   Failures here MUST NOT propagate to the caller —          ║
    //  ║   the resolution is already committed. Log and continue.    ║
    //  ║                                                              ║
    //  ║   Recommend tokio::spawn so the resolve endpoint stays       ║
    //  ║   responsive even when refits are heavy.                     ║
    //  ║                                                              ║
    //  ╚══════════════════════════════════════════════════════════════╝
    //
    let refit_triggered = false; // TODO(bayesops): set to true once wired

    Ok(Json(json!({
        "workspace_id":          workspace_id,
        "workspace_status":      new_status,
        "resolved_at":           resolved_at,
        "outcome":               req.outcome,
        "brier_score":           brier_score,
        "predicted_probability": predicted_probability,
        "downstream_notified":   downstream.len(),
        "refit_triggered":       refit_triggered,
    })))
}

// ─── Helpers ─────────────────────────────────────────────────────────

/// Extract a binary-scorable probability from a JSON value. Accepts:
///   • bare number 0.0..=1.0     → returned as-is
///   • bare bool                 → true → 1.0, false → 0.0
///   • { "value": <number> }     → extracted
///   • { "value": <bool> }       → coerced
///   • { "won_tournament": bool} → semantic alias for binary tournament prior
///
/// Returns None for any other shape (multi-class outcomes, free-form
/// strings, etc.) — Brier scoring is intentionally limited to binary
/// cases. Multi-class scoring is Phase 6+.
fn extract_probability(v: &JsonValue) -> Option<f64> {
    match v {
        JsonValue::Number(n) => n.as_f64(),
        JsonValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        JsonValue::Object(obj) => {
            // Prefer explicit `value`; fall back to common semantic
            // aliases used by the WC factor-model workspaces.
            for key in &["value", "won_tournament", "advanced", "won"] {
                if let Some(inner) = obj.get(*key) {
                    if let Some(p) = extract_probability(inner) {
                        return Some(p);
                    }
                }
            }
            None
        }
        _ => None,
    }
}
