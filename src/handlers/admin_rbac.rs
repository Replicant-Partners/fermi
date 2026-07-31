//! Substrate-level admin RBAC endpoints — v0.10.4.
//!
//! Three surfaces, all platform-admin-only, all tenant-agnostic:
//!
//!   * `GET  /api/admin/rbac/orphans`  — dump `rbac_orphans` view.
//!   * `POST /api/admin/rbac/reassign` — polymorphic owner reassign
//!     against any resource enumerated in [`ObjectType::owner_table`].
//!   * `POST /api/admin/rbac/heal`     — re-run the mig 162 empty-string
//!     + id::text drift heal on demand (dry-run or apply). Useful when
//!     new orphans accumulate between deploys.
//!
//! These replace the per-resource `admin_agent_ownership_*` and
//! `admin_teams_ownership_*` endpoints — that pattern was
//! `n * m` boilerplate per new resource and per new operation. Every
//! tenant app now gets orphan visibility and reassignment for free
//! by adding one `ObjectType` variant + one `SELECT` block in the
//! `rbac_orphans` view.
//!
//! Design guarantees:
//!
//!   * Reassign refuses to write a `new_owner_user_id` that isn't in
//!     `users.user_id`. Prevents "fix the orphan by creating a new
//!     orphan" typos.
//!   * Reassign always uses the (table, pk_col, owner_col) triple
//!     from `ObjectType::owner_table`. No format-string SQL injection
//!     from user input — the resource type is enum-validated at the
//!     axum extractor level.
//!   * All handlers gated by `rbac::require_platform_admin`.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{rbac, AuthPrincipal, ObjectType};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

// ═══════════════════════════════════════════════════════════════════
// GET /api/admin/rbac/orphans
// ═══════════════════════════════════════════════════════════════════

/// Query params: optional `?resource=agents` to filter.
#[derive(Debug, Deserialize)]
pub struct OrphansQuery {
    /// Filter to a single resource table. When omitted, returns all.
    #[serde(default)]
    pub resource: Option<String>,
    /// Cap on rows returned. Defaults to 500; max 5000.
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn admin_rbac_orphans_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<OrphansQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    rbac::require_platform_admin(&principal)?;

    let limit = q.limit.unwrap_or(500).clamp(1, 5000);

    let (sql, has_filter) = match q.resource {
        Some(_) => (
            "SELECT resource, row_id, owner_col, owner_ref, label, created_at
               FROM public.rbac_orphans
              WHERE resource = $1
              ORDER BY resource, created_at NULLS LAST
              LIMIT $2",
            true,
        ),
        None => (
            "SELECT resource, row_id, owner_col, owner_ref, label, created_at
               FROM public.rbac_orphans
              ORDER BY resource, created_at NULLS LAST
              LIMIT $1",
            false,
        ),
    };

    let query = if has_filter {
        sqlx::query(sql)
            .bind(q.resource.as_deref().unwrap_or(""))
            .bind(limit)
    } else {
        sqlx::query(sql).bind(limit)
    };

    let rows = query
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut out: Vec<Value> = Vec::with_capacity(rows.len());
    // Per-resource counts for a quick health readout.
    let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();

    for r in &rows {
        let resource: String = r.try_get("resource").unwrap_or_default();
        *counts.entry(resource.clone()).or_insert(0) += 1;

        out.push(json!({
            "resource": resource,
            "row_id":    r.try_get::<String, _>("row_id").unwrap_or_default(),
            "owner_col": r.try_get::<String, _>("owner_col").unwrap_or_default(),
            "owner_ref": r.try_get::<Option<String>, _>("owner_ref").ok().flatten(),
            "label":     r.try_get::<Option<String>, _>("label").ok().flatten(),
            "created_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                .ok().flatten().map(|t| t.to_rfc3339()),
        }));
    }

    // Also compute an overall count so a caller doesn't have to
    // paginate to check "is the invariant clean?" — a bare
    // `total_orphans = 0` is the trust signal.
    let total_orphans: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM public.rbac_orphans")
            .fetch_one(&state.db)
            .await
            .unwrap_or_default();

    Ok(Json(json!({
        "total_orphans": total_orphans,
        "returned":       out.len(),
        "limit":          limit,
        "by_resource":    counts,
        "orphans":        out,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/admin/rbac/reassign
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct ReassignRequest {
    /// The `ObjectType::as_str()` value — e.g. `"agent"`, `"creature"`.
    /// Must be an `ObjectType` variant with an `owner_table` mapping;
    /// bad values are 400s.
    pub resource: String,
    /// Primary-key values (as text) for the rows to reassign. Empty
    /// list is a 400.
    pub row_ids: Vec<String>,
    /// The new owner. `None` sets the owner column to NULL (system-
    /// orphaned; use for empty-string cleanups where no rightful owner
    /// can be identified). Otherwise must exist in `users.user_id`.
    #[serde(default)]
    pub new_owner_user_id: Option<String>,
}

pub async fn admin_rbac_reassign_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ReassignRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    rbac::require_platform_admin(&principal)?;

    if req.row_ids.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "row_ids is empty".into()));
    }

    // Resource → (table, pk_col, owner_col). Enum validation blocks
    // both unknown resource strings and resources we haven't mapped
    // yet (e.g. Capability). No format-string injection risk because
    // the three identifiers come from the enum, not user input.
    let object_type = ObjectType::from_str(&req.resource)
        .ok_or((StatusCode::BAD_REQUEST, format!("unknown resource '{}'", req.resource)))?;
    let (table, pk_col, owner_col) = object_type.owner_table().ok_or((
        StatusCode::BAD_REQUEST,
        format!(
            "resource '{}' has no owner_table mapping; not reassignable",
            req.resource
        ),
    ))?;

    // Verify the new owner exists — a typo would silently orphan the
    // row we're trying to fix (which is the exact bug we're solving).
    if let Some(ref uid) = req.new_owner_user_id {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT user_id FROM public.users WHERE user_id = $1",
        )
        .bind(uid)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if exists.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("new_owner_user_id '{}' not found in users", uid),
            ));
        }
    }

    // Perform per-row UPDATEs. Kept per-row so partial failure (bad
    // pk cast, e.g. non-UUID string for a UUID pk column) doesn't
    // roll back the whole batch, and the response can name which
    // rows landed vs failed.
    //
    // The pk column type varies by table (some UUID, some TEXT); we
    // rely on Postgres's implicit text→uuid cast plus the row_id
    // being valid. If it isn't, per-row error is returned.
    let mut results: Vec<Value> = Vec::with_capacity(req.row_ids.len());
    let update_sql = format!(
        "UPDATE public.{} SET {} = $2 WHERE {}::text = $1 RETURNING {}::text AS pk",
        table, owner_col, pk_col, pk_col
    );

    for row_id in &req.row_ids {
        let res = sqlx::query(&update_sql)
            .bind(row_id)
            .bind(&req.new_owner_user_id)
            .fetch_optional(&state.db)
            .await;
        let (status, error) = match res {
            Ok(Some(_)) => ("updated", None),
            Ok(None) => ("not_found", None),
            Err(e) => ("error", Some(e.to_string())),
        };
        results.push(json!({
            "row_id": row_id,
            "status": status,
            "error":  error,
        }));
    }

    tracing::info!(
        admin_user_id = %principal.user_id(),
        resource      = %req.resource,
        row_count     = req.row_ids.len(),
        new_owner     = req.new_owner_user_id.as_deref().unwrap_or("<null>"),
        "[rbac.reassign] reassigned resource ownership",
    );

    Ok(Json(json!({
        "resource":        req.resource,
        "new_owner_user_id": req.new_owner_user_id,
        "results":         results,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/admin/rbac/heal
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct HealRequest {
    /// When true, count healable rows without writing.
    #[serde(default)]
    pub dry_run: bool,
    /// Restrict heal to a single resource, or all when None.
    #[serde(default)]
    pub resource: Option<String>,
}

pub async fn admin_rbac_heal_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<HealRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    rbac::require_platform_admin(&principal)?;

    // Whitelist of (table, owner_col) — matches migration 162's list.
    // Kept in sync with `ObjectType::owner_table` values that are also
    // in the substrate FK migration. Any resource NOT in this list is
    // silently ignored (it wasn't part of mig 162's coverage yet).
    let targets: &[(&str, &str)] = &[
        ("agents", "user_id"),
        ("teams", "owner_id"),
        ("apps", "owner_user_id"),
        ("creatures", "owner_id"),
        ("creature_collections", "owner_id"),
        ("creature_flights", "owner_id"),
        ("creature_tethers", "owner_id"),
        ("creature_devices", "owner_id"),
        ("creature_goals", "owner_id"),
        ("swarm_events", "creator_id"),
        ("swarm_sessions", "owner_id"),
        ("swarm_sub_flocks", "owner_id"),
        ("sosa_platforms", "owner_id"),
        ("observation_sessions", "owner_id"),
        ("forage_observations", "owner_id"),
        ("ar_beacons", "creator_id"),
        ("ar_grid_maps", "creator_id"),
        ("shopping_profiles", "user_id"),
        ("rabble_co_presence", "owner_id"),
        ("forecast_relationships", "owner_id"),
        ("forecast_relationship_groups", "owner_id"),
        ("pending_cascades", "owner_id"),
    ];

    let mut summary: Vec<Value> = Vec::new();
    for (table, col) in targets {
        if let Some(ref only) = req.resource {
            if only.as_str() != *table {
                continue;
            }
        }

        // Count empty-string rows.
        let empty_count_sql = format!(
            "SELECT COUNT(*) FROM public.{} WHERE {} = ''",
            table, col
        );
        let empty_count: i64 = sqlx::query_scalar(&empty_count_sql)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

        // Count id::text-drift rows.
        let idcast_count_sql = format!(
            "SELECT COUNT(*) FROM public.{} t \
              WHERE EXISTS (SELECT 1 FROM public.users u WHERE t.{} = u.id::text) \
                AND NOT EXISTS (SELECT 1 FROM public.users u2 WHERE u2.user_id = t.{})",
            table, col, col
        );
        let idcast_count: i64 = sqlx::query_scalar(&idcast_count_sql)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

        let mut empty_healed = 0i64;
        let mut idcast_healed = 0i64;
        if !req.dry_run {
            // Empty-string → NULL.
            if empty_count > 0 {
                let sql = format!(
                    "UPDATE public.{} SET {} = NULL WHERE {} = '' RETURNING 1",
                    table, col, col
                );
                let rows = sqlx::query(&sql).fetch_all(&state.db).await;
                if let Ok(rs) = rows {
                    empty_healed = rs.len() as i64;
                }
            }
            // id::text → users.user_id.
            if idcast_count > 0 {
                let sql = format!(
                    "UPDATE public.{} t SET {} = u.user_id \
                     FROM public.users u \
                     WHERE t.{} = u.id::text \
                       AND t.{} <> u.user_id \
                       AND NOT EXISTS (SELECT 1 FROM public.users u2 WHERE u2.user_id = t.{}) \
                     RETURNING 1",
                    table, col, col, col, col
                );
                let rows = sqlx::query(&sql).fetch_all(&state.db).await;
                if let Ok(rs) = rows {
                    idcast_healed = rs.len() as i64;
                }
            }
        }

        summary.push(json!({
            "resource":       table,
            "owner_column":   col,
            "empty_rows":     empty_count,
            "idcast_rows":    idcast_count,
            "empty_healed":   if req.dry_run { 0 } else { empty_healed },
            "idcast_healed":  if req.dry_run { 0 } else { idcast_healed },
        }));
    }

    tracing::info!(
        admin_user_id = %principal.user_id(),
        dry_run       = req.dry_run,
        resource      = req.resource.as_deref().unwrap_or("<all>"),
        "[rbac.heal] ran drift heal",
    );

    Ok(Json(json!({
        "dry_run":  req.dry_run,
        "summary":  summary,
    })))
}
