//! Per-target share handlers for forecasts and portfolios (Spec 24 §3.3).
//!
//! Six endpoints, two trios:
//!
//!   GET    /api/forecasts/:id/shares
//!   POST   /api/forecasts/:id/shares
//!   DELETE /api/forecasts/:id/shares/:share_id
//!
//!   GET    /api/portfolios/:id/shares
//!   POST   /api/portfolios/:id/shares
//!   DELETE /api/portfolios/:id/shares/:share_id
//!
//! Distinct from the existing `POST /api/shares` (`handlers::teams`) which
//! takes a free-form `object_type` and performs zero authorization. These
//! routes pin the object type at the route level and gate on ownership of
//! the target — closer to where the bug class lives (someone shares a
//! forecast they don't own).
//!
//! Wave-2 authorization (Sprint 2.4): create/revoke gated on `can_admin`
//! via `fermi_auth::visibility::can_access`, letting team admins manage
//! shares on team-owned objects without owning every individual row.
//! Anyone with `can_view` access to the target can list its shares.
//!
//! Schema-drift note: the principal's `user_id()` returns
//! `users.user_id` (text), but `fermi_forecasts.owner_id` and
//! `fermi_portfolios.owner_id` are `uuid` columns. The ownership check
//! parses the principal's text user_id as a UUID and compares — the same
//! brittle pattern the rest of `forecasts.rs` uses
//! (`forecasts.rs:455, 580, 1054`, …). A clean fix is part of Wave 2.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi_auth::visibility::{can_access, can_view};
use fermi_auth::{teams, AuthPrincipal, ObjectType, Permission, ShareType, Visibility};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;

/// Enrich a raw share list with display names so the console can render
/// "Alice (view)" instead of "9c3a…-view". Best-effort: any row that
/// can't be resolved falls through with the raw target/granted_by
/// string, and DB failure downgrades to the un-enriched list rather than
/// erroring out the whole request (a name lookup is UX-nice, not
/// safety-critical).
async fn enrich_shares(pool: &PgPool, shares: Vec<fermi_auth::ObjectShare>) -> Vec<JsonValue> {
    // Collect user_id targets (share_type="user") + all granted_by ids;
    // team share_targets are UUIDs but we can resolve their names too.
    let mut user_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut team_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in &shares {
        user_ids.insert(s.granted_by.clone());
        match s.share_type {
            ShareType::User => {
                user_ids.insert(s.share_target.clone());
            }
            ShareType::Team => {
                team_ids.insert(s.share_target.clone());
            }
        }
    }

    let mut user_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if !user_ids.is_empty() {
        let ids: Vec<String> = user_ids.into_iter().collect();
        if let Ok(rows) = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "SELECT user_id::text, display_name, email FROM users WHERE user_id::text = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await
        {
            for (uid, name, email) in rows {
                if let Some(n) = name.or(email) {
                    user_names.insert(uid, n);
                }
            }
        }
    }

    let mut team_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if !team_ids.is_empty() {
        let ids: Vec<String> = team_ids.into_iter().collect();
        if let Ok(rows) = sqlx::query_as::<_, (String, String)>(
            "SELECT id::text, name FROM teams WHERE id::text = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await
        {
            for (tid, name) in rows {
                team_names.insert(tid, name);
            }
        }
    }

    shares
        .into_iter()
        .map(|s| {
            let target_name = match s.share_type {
                ShareType::User => user_names.get(&s.share_target).cloned(),
                ShareType::Team => team_names.get(&s.share_target).cloned(),
            };
            let granted_by_name = user_names.get(&s.granted_by).cloned();
            json!({
                "id": s.id,
                "object_type": s.object_type.as_str(),
                "object_id": s.object_id,
                "share_type": s.share_type.as_str(),
                "share_target": s.share_target,
                "share_target_display_name": target_name,
                "permission": s.permission.as_str(),
                "granted_by": s.granted_by,
                "granted_by_display_name": granted_by_name,
            })
        })
        .collect()
}

// ─── Request shape ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    /// `"user"` or `"team"`. The wire vocabulary matches
    /// `object_shares.share_type`.
    share_type: String,
    /// For `share_type="user"`: the recipient's `users.user_id` (text,
    /// from `/api/users/lookup`). For `share_type="team"`: the team UUID
    /// as text. We don't UUID-parse here — `object_shares.share_target`
    /// is TEXT, and the team-id-as-text convention is established
    /// elsewhere (see `can_access` line 93: `os.share_target = tm.team_id::text`).
    share_target: String,
    /// `"view"` | `"edit"` | `"admin"`. Defaults to `"view"`.
    permission: Option<String>,
}

// ─── Forecast shares ───────────────────────────────────────────────────

/// GET /api/forecasts/:id/shares — list active shares for one forecast.
///
/// Caller must be able to view the forecast (same matrix as
/// `get_forecast_handler`: owner / shared|public / team member). We
/// don't gate this on owner-only because collaborators with view access
/// have a legitimate need to see who else can see the row.
pub async fn list_forecast_shares_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_view_access_to_forecast(pool, &forecast_id, &principal).await?;

    let shares = teams::list_object_shares(pool, ObjectType::Forecast, &forecast_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let count = shares.len();
    let enriched = enrich_shares(pool, shares).await;

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "shares": enriched,
        "count": count,
    })))
}

/// POST /api/forecasts/:id/shares — create one share.
///
/// Caller must own the forecast (Wave-1 ACL). Wave-2 widens to
/// `can_admin` via `fermi_auth::visibility`. Body matches
/// `CreateShareRequest`. Idempotent at the DB level
/// (UNIQUE(object_type, object_id, share_type, share_target) with
/// ON CONFLICT DO UPDATE SET permission), so repeat POSTs upgrade or
/// downgrade an existing grant rather than failing — same shape the
/// generic `share_object` helper has shipped with for the rest of the
/// codebase.
pub async fn create_forecast_share_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Json(body): Json<CreateShareRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    let pool = &state.db;
    require_admin_access_to_forecast(pool, &forecast_id, &principal).await?;

    let share_type = parse_share_type(&body.share_type)?;
    let permission = parse_permission(body.permission.as_deref());
    let share_target = body.share_target.trim();
    if share_target.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "share_target is required".into()));
    }

    let share = teams::share_object(
        pool,
        ObjectType::Forecast,
        &forecast_id,
        share_type,
        share_target,
        permission,
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(share))))
}

/// DELETE /api/forecasts/:id/shares/:share_id — revoke one share.
///
/// Caller must own the forecast. The {share_id, forecast_id} pair is
/// verified server-side so a forecast owner can't accidentally (or
/// maliciously) revoke a share that targets a different object via a
/// crafted share_id. Returns 404 if the pair doesn't match.
pub async fn revoke_forecast_share_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((forecast_id, share_id)): Path<(String, Uuid)>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_admin_access_to_forecast(pool, &forecast_id, &principal).await?;
    verify_share_matches_target(pool, share_id, "forecast", &forecast_id).await?;

    teams::revoke_share(pool, share_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "revoked", "share_id": share_id })))
}

// ─── Portfolio shares ──────────────────────────────────────────────────

/// GET /api/portfolios/:id/shares — symmetric with the forecast variant.
pub async fn list_portfolio_shares_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_view_access_to_portfolio(pool, &portfolio_id, &principal).await?;

    let shares = teams::list_object_shares(pool, ObjectType::Portfolio, &portfolio_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let count = shares.len();
    let enriched = enrich_shares(pool, shares).await;

    Ok(Json(json!({
        "portfolio_id": portfolio_id,
        "shares": enriched,
        "count": count,
    })))
}

pub async fn create_portfolio_share_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
    Json(body): Json<CreateShareRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    let pool = &state.db;
    require_admin_access_to_portfolio(pool, &portfolio_id, &principal).await?;

    let share_type = parse_share_type(&body.share_type)?;
    let permission = parse_permission(body.permission.as_deref());
    let share_target = body.share_target.trim();
    if share_target.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "share_target is required".into()));
    }

    let share = teams::share_object(
        pool,
        ObjectType::Portfolio,
        &portfolio_id,
        share_type,
        share_target,
        permission,
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(share))))
}

pub async fn revoke_portfolio_share_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((portfolio_id, share_id)): Path<(String, Uuid)>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_admin_access_to_portfolio(pool, &portfolio_id, &principal).await?;
    verify_share_matches_target(pool, share_id, "portfolio", &portfolio_id).await?;

    teams::revoke_share(pool, share_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "revoked", "share_id": share_id })))
}

// ─── Helpers ───────────────────────────────────────────────────────────

fn parse_share_type(s: &str) -> Result<ShareType, (StatusCode, String)> {
    match s {
        "user" => Ok(ShareType::User),
        "team" => Ok(ShareType::Team),
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("invalid share_type '{}': expected user|team", s),
        )),
    }
}

fn parse_permission(s: Option<&str>) -> Permission {
    match s {
        Some("edit") => Permission::Edit,
        Some("admin") => Permission::Admin,
        _ => Permission::View,
    }
}

/// Look up a forecast's `(owner_id, visibility, team_id)` triple.
/// Returns 404 if missing — the caller has no business knowing whether
/// a private forecast exists at all.
async fn forecast_acl_row(
    pool: &PgPool,
    forecast_id: &str,
) -> Result<(String, String, Option<Uuid>), (StatusCode, String)> {
    let row = sqlx::query_as::<_, (String, String, Option<Uuid>)>(
        "SELECT owner_id::text, visibility, team_id
         FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;
    Ok(row)
}

async fn portfolio_acl_row(
    pool: &PgPool,
    portfolio_id: &str,
) -> Result<(String, String, Option<Uuid>), (StatusCode, String)> {
    let row = sqlx::query_as::<_, (String, String, Option<Uuid>)>(
        "SELECT owner_id::text, visibility, team_id
         FROM fermi_portfolios WHERE id = $1",
    )
    .bind(portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Portfolio not found".into()))?;
    Ok(row)
}

/// View-ACL via can_view (Spec 24 §3.2 Wave 2: honours user-shares too).
async fn require_view_access_to_forecast(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let (owner_id, visibility, _team_id) = forecast_acl_row(pool, forecast_id).await?;
    let vis = Visibility::from_legacy(&visibility);
    let granted = can_view(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if granted {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Access denied".into()))
    }
}

async fn require_view_access_to_portfolio(
    pool: &PgPool,
    portfolio_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let (owner_id, visibility, _team_id) = portfolio_acl_row(pool, portfolio_id).await?;
    let vis = Visibility::from_legacy(&visibility);
    let granted = can_view(
        pool,
        principal,
        ObjectType::Portfolio,
        portfolio_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if granted {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Access denied".into()))
    }
}

/// Admin-ACL via can_access (Spec 24 §3.2 Wave 2: can_admin via
/// object_shares so team admins can manage shares too).
async fn require_admin_access_to_forecast(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let (owner_id, visibility, _t) = forecast_acl_row(pool, forecast_id).await?;
    let vis = Visibility::from_legacy(&visibility);
    let level = can_access(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if level.has_admin() {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Admin access required".into()))
    }
}

async fn require_admin_access_to_portfolio(
    pool: &PgPool,
    portfolio_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let (owner_id, visibility, _t) = portfolio_acl_row(pool, portfolio_id).await?;
    let vis = Visibility::from_legacy(&visibility);
    let level = can_access(
        pool,
        principal,
        ObjectType::Portfolio,
        portfolio_id,
        &owner_id,
        vis,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if level.has_admin() {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Admin access required".into()))
    }
}

/// Guard against a malicious DELETE that names a share_id pointing at a
/// different object. Returns 404 if no row matches the (id, object_type,
/// object_id) triple. Without this check, an owner of forecast A could
/// pass a share_id belonging to forecast B and we'd happily delete it.
async fn verify_share_matches_target(
    pool: &PgPool,
    share_id: Uuid,
    object_type: &str,
    object_id: &str,
) -> Result<(), (StatusCode, String)> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM object_shares
             WHERE id = $1 AND object_type = $2 AND object_id = $3
         )",
    )
    .bind(share_id)
    .bind(object_type)
    .bind(object_id)
    .fetch_one(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if exists {
        Ok(())
    } else {
        Err((
            StatusCode::NOT_FOUND,
            "Share not found for this target".into(),
        ))
    }
}
