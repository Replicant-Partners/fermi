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

/// One entry in the audit target registry — what a resource looks
/// like on-disk. Kept as a static array so the audit is
/// self-documenting AND survives partial-schema deploys (a table
/// or column that doesn't exist on this deploy is logged and
/// skipped, not fatal).
///
/// `label_col` is the column we return as the resource's
/// human-readable identifier (or `COALESCE(a, b)` when more than
/// one option makes sense). `created_col` is `NULL` for tables
/// without a canonical created_at (e.g. `agents` uses
/// `updated_at` but we surface NULL to keep the shape uniform).
struct AuditTarget {
    resource: &'static str,
    table: &'static str,
    pk_col: &'static str,
    owner_col: &'static str,
    label_expr: &'static str,
    created_col: Option<&'static str>,
}

const AUDIT_TARGETS: &[AuditTarget] = &[
    AuditTarget {
        resource: "agents",
        table: "agents",
        pk_col: "agent_id",
        owner_col: "user_id",
        label_expr: "agent_name",
        created_col: None,
    },
    AuditTarget {
        resource: "teams",
        table: "teams",
        pk_col: "id",
        owner_col: "owner_id",
        label_expr: "name",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "apps",
        table: "apps",
        pk_col: "id",
        owner_col: "owner_user_id",
        label_expr: "slug",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "fermi_forecasts",
        table: "fermi_forecasts",
        pk_col: "id",
        owner_col: "owner_id",
        label_expr: "question_text",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "fermi_portfolios",
        table: "fermi_portfolios",
        pk_col: "id",
        owner_col: "owner_id",
        label_expr: "title",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "fermi_notebooks",
        table: "fermi_notebooks",
        pk_col: "id",
        owner_col: "owner_id",
        label_expr: "title",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "creatures",
        table: "creatures",
        pk_col: "creature_id",
        owner_col: "owner_id",
        label_expr: "COALESCE(specimen_name, scientific_name)",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "creature_collections",
        table: "creature_collections",
        pk_col: "collection_id",
        owner_col: "owner_id",
        label_expr: "name",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "creature_flights",
        table: "creature_flights",
        pk_col: "flight_id",
        owner_col: "owner_id",
        label_expr: "COALESCE(location_name, h3_cell)",
        created_col: Some("started_at"),
    },
    AuditTarget {
        resource: "swarm_events",
        table: "swarm_events",
        pk_col: "swarm_id",
        owner_col: "creator_id",
        label_expr: "name",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "swarm_sessions",
        table: "swarm_sessions",
        pk_col: "session_id",
        owner_col: "owner_id",
        label_expr: "name",
        created_col: Some("started_at"),
    },
    AuditTarget {
        resource: "sosa_platforms",
        table: "sosa_platforms",
        pk_col: "platform_id",
        owner_col: "owner_id",
        label_expr: "name",
        created_col: Some("created_at"),
    },
    AuditTarget {
        resource: "observation_sessions",
        table: "observation_sessions",
        pk_col: "session_id",
        owner_col: "owner_id",
        label_expr: "name",
        created_col: Some("started_at"),
    },
    AuditTarget {
        resource: "ar_beacons",
        table: "ar_beacons",
        pk_col: "beacon_id",
        owner_col: "creator_id",
        label_expr: "COALESCE(location_name, h3_cell)",
        created_col: Some("created_at"),
    },
];

/// Query one audit target. Returns the orphan rows found, or `None`
/// if the table/column doesn't exist on this deploy (logged as a
/// warning but not fatal). This is what makes v0.10.7's rewrite
/// robust against the mig-163 CREATE-VIEW failure mode: any single
/// broken target skips itself instead of taking the whole endpoint
/// down.
async fn fetch_orphans_for_target(
    pool: &sqlx::PgPool,
    target: &AuditTarget,
    per_target_limit: i64,
) -> Option<Vec<Value>> {
    // Compose the SELECT with the target's identifiers substituted
    // in. The identifiers come from a hard-coded const array — no
    // format-string injection surface from request bodies.
    let created_expr = target.created_col.unwrap_or("NULL::timestamptz");
    let sql = format!(
        "SELECT {pk}::text AS row_id, \
                {owner}::text AS owner_ref, \
                ({label})::text AS label, \
                {created} AS created_at \
           FROM public.{table} t \
          WHERE t.{owner} IS NOT NULL \
            AND NOT EXISTS ( \
                SELECT 1 FROM public.users u \
                 WHERE u.user_id = t.{owner} \
            ) \
          ORDER BY {created} NULLS LAST \
          LIMIT $1",
        pk = target.pk_col,
        owner = target.owner_col,
        label = target.label_expr,
        created = created_expr,
        table = target.table,
    );

    match sqlx::query(&sql)
        .bind(per_target_limit)
        .fetch_all(pool)
        .await
    {
        Ok(rows) => {
            let mut out = Vec::with_capacity(rows.len());
            for r in &rows {
                out.push(json!({
                    "resource":   target.resource,
                    "row_id":     r.try_get::<Option<String>, _>("row_id").ok().flatten(),
                    "owner_col":  target.owner_col,
                    "owner_ref":  r.try_get::<Option<String>, _>("owner_ref").ok().flatten(),
                    "label":      r.try_get::<Option<String>, _>("label").ok().flatten(),
                    "created_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                        .ok().flatten().map(|t| t.to_rfc3339()),
                }));
            }
            Some(out)
        }
        Err(e) => {
            tracing::warn!(
                resource = target.resource,
                table = target.table,
                error = %e,
                "[rbac.orphans] skipping target — query failed (missing column/table on this deploy?)",
            );
            None
        }
    }
}

pub async fn admin_rbac_orphans_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<OrphansQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    rbac::require_platform_admin(&principal)?;

    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    // Cap per-target to avoid one massively-drifted resource
    // starving the others. `limit` still gates the returned
    // total after aggregation.
    let per_target_limit = limit.min(1000);

    let mut all_orphans: Vec<Value> = Vec::new();
    let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut skipped: Vec<String> = Vec::new();

    for target in AUDIT_TARGETS {
        if let Some(ref resource_filter) = q.resource {
            if resource_filter.as_str() != target.resource {
                continue;
            }
        }
        match fetch_orphans_for_target(&state.db, target, per_target_limit).await {
            Some(rows) => {
                counts.insert(target.resource.to_string(), rows.len() as i64);
                all_orphans.extend(rows);
            }
            None => {
                skipped.push(target.resource.to_string());
            }
        }
    }

    // Truncate to `limit` after aggregation so the response body
    // stays bounded regardless of how much drift a single tenant
    // has accumulated.
    let total_orphans = all_orphans.len() as i64;
    if all_orphans.len() as i64 > limit {
        all_orphans.truncate(limit as usize);
    }

    Ok(Json(json!({
        "total_orphans":    total_orphans,
        "returned":         all_orphans.len(),
        "limit":            limit,
        "by_resource":      counts,
        "skipped_resources": skipped,
        "orphans":          all_orphans,
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
    let object_type = ObjectType::from_str(&req.resource).ok_or((
        StatusCode::BAD_REQUEST,
        format!("unknown resource '{}'", req.resource),
    ))?;
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
        let exists: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM public.users WHERE user_id = $1")
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
        let empty_count_sql = format!("SELECT COUNT(*) FROM public.{} WHERE {} = ''", table, col);
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
