//! Team collaboration surfaces — provenance, attribution, activity.
//!
//! Spec 26 (`docs/specs/SPEC_26_TEAM_COLLABORATION.md`). Seven read
//! endpoints plus the batch helpers the forecast/portfolio list handlers
//! call to answer three questions the console previously could not:
//!
//!   1. **"Who shared this with me, and how?"** — [`forecast_access_provenance`]
//!      / [`portfolio_access_provenance`] resolve, for a whole page of
//!      rows in one query, the strongest true access path plus the
//!      grantor and timestamp behind it.
//!   2. **"Is this a portfolio item or standalone?"** —
//!      [`forecast_portfolio_memberships`] attaches `portfolios[]` to
//!      every row. Empty ⇒ standalone.
//!   3. **"Which teammate did which thing?"** — the three activity feeds
//!      ([`forecast_activity_handler`], [`portfolio_activity_handler`],
//!      [`team_activity_handler`]) and the per-member roll-up
//!      ([`team_contributions_handler`]).
//!
//! ## Derived, not dual-written
//!
//! There is no `collab_events` table. Every event is derived by UNION
//! over the tables that already hold the truth (forecasts, forecast
//! updates, object_shares, portfolio membership, team membership,
//! invites). Two consequences, both deliberate:
//!
//!   * the feeds are correct for all pre-Spec-26 history on day one, and
//!   * no writer can forget to log, because there is nothing to log to.
//!
//! The cost is a wide UNION per request. Every branch hits an existing
//! index and every feed is `LIMIT`-bounded, so the shape is fine at the
//! scale a human team operates at (O(100s) of forecasts per team).
//!
//! ## Attribution honesty
//!
//! `fermi_forecast_updates.actor_user_id` (migration 176) is NULLable
//! with no backfill. Rows without an actor surface as
//! `actor_kind: "system"` and a null name — we never attribute a
//! revision to the forecast owner just to fill the column, because the
//! UI cannot then distinguish a guess from a fact.

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::visibility::{can_access, can_view, inherited_access_by_ids_sql};
use fermi_auth::{teams, AuthPrincipal, ObjectType, Visibility};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════
// Query params
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default)]
pub struct ActivityQuery {
    /// Max events to return. Clamped to 200.
    pub limit: Option<i64>,
    /// Restrict to one actor (`users.user_id`). This is what turns
    /// "what has the team been doing" into "what has Alice been doing"
    /// with one click.
    pub actor: Option<String>,
    /// Comma-separated event kinds, e.g. `revised,resolved`.
    pub kind: Option<String>,
}

impl ActivityQuery {
    fn limit_clamped(&self) -> i64 {
        self.limit.unwrap_or(60).clamp(1, 200)
    }

    fn kinds(&self) -> Option<HashSet<String>> {
        self.kind.as_ref().map(|k| {
            k.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Display-name resolution
// ═══════════════════════════════════════════════════════════════════════

/// Batch-resolve `users.user_id` → best available human label. Used by
/// every surface in this module, so it lives here rather than being
/// re-implemented per handler (shares.rs has its own older copy scoped
/// to share rows).
///
/// Best-effort by contract: a DB error yields an empty map and callers
/// fall back to raw ids. A missing name is a cosmetic problem, never a
/// reason to fail a request.
pub async fn resolve_user_names(pool: &PgPool, ids: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let unique: Vec<String> = ids
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if unique.is_empty() {
        return out;
    }
    if let Ok(rows) = sqlx::query(
        "SELECT user_id::text AS uid,
                COALESCE(display_name, name, email, user_id::text) AS label,
                avatar_url
         FROM users WHERE user_id::text = ANY($1)",
    )
    .bind(&unique)
    .fetch_all(pool)
    .await
    {
        for r in rows {
            if let (Ok(uid), Ok(label)) = (
                r.try_get::<String, _>("uid"),
                r.try_get::<String, _>("label"),
            ) {
                out.insert(uid, label);
            }
        }
    }
    out
}

/// Batch-resolve `teams.id::text` → team name.
pub async fn resolve_team_names(pool: &PgPool, ids: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let unique: Vec<String> = ids
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if unique.is_empty() {
        return out;
    }
    if let Ok(rows) =
        sqlx::query("SELECT id::text AS tid, name FROM teams WHERE id::text = ANY($1)")
            .bind(&unique)
            .fetch_all(pool)
            .await
    {
        for r in rows {
            if let (Ok(tid), Ok(name)) = (
                r.try_get::<String, _>("tid"),
                r.try_get::<String, _>("name"),
            ) {
                out.insert(tid, name);
            }
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// Provenance — "who shared this with me, and how"
// ═══════════════════════════════════════════════════════════════════════

/// One object's effective access path for one principal.
///
/// `access_via` precedence mirrors `fermi_auth::visibility::can_access`
/// exactly, so the label always describes the branch that actually
/// granted access:
///
/// | `access_via`  | meaning |
/// |---------------|---------|
/// | `owner`       | the caller owns it |
/// | `user_share`  | an explicit `object_shares` row names the caller |
/// | `team_owned`  | the object's `team_id` is a team the caller is in |
/// | `team_share`  | an `object_shares` row targets a team the caller is in |
/// | `portfolio`   | inherited from a shared portfolio (forecasts only) |
/// | `public`      | `visibility='public'` |
/// | `link`        | `visibility='shared'` — discoverable by anyone with the id |
///
/// The console renders the strongest *true* statement instead of a pile
/// of badges, which is what makes "shared by Alice via WC analysts"
/// possible where before there was only an unexplained row.
#[derive(Debug, Clone, Default)]
pub struct AccessProvenance {
    pub access_via: String,
    pub permission: String,
    pub shared_by: Option<String>,
    pub shared_by_display_name: Option<String>,
    pub shared_at: Option<String>,
    pub team_id: Option<String>,
    pub team_name: Option<String>,
    pub via_portfolio_id: Option<String>,
    pub via_portfolio_title: Option<String>,
    pub share_count: i64,
}

impl AccessProvenance {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "access_via":              self.access_via,
            "permission":              self.permission,
            "shared_by":               self.shared_by,
            "shared_by_display_name":  self.shared_by_display_name,
            "shared_at":               self.shared_at,
            "team_id":                 self.team_id,
            "team_name":               self.team_name,
            "via_portfolio_id":        self.via_portfolio_id,
            "via_portfolio_title":     self.via_portfolio_title,
            "share_count":             self.share_count,
        })
    }
}

/// Shared shape for the two provenance queries. `$1` = `TEXT[]` of
/// object ids, `$2` = caller's user_id, `{table}`/`{type}` interpolated
/// (never user input — two call sites, both literal).
fn provenance_sql(table: &str, object_type: &str) -> String {
    format!(
        r#"
SELECT o.id::text                                        AS object_id,
       o.owner_id::text                                  AS owner_id,
       o.visibility                                      AS visibility,
       o.team_id::text                                   AS team_id,
       us.permission                                     AS user_perm,
       us.granted_by                                     AS user_grantor,
       us.created_at                                     AS user_at,
       ts.permission                                     AS team_perm,
       ts.granted_by                                     AS team_grantor,
       ts.created_at                                     AS team_at,
       ts.share_target                                   AS team_share_target,
       (o.team_id IS NOT NULL AND EXISTS (
            SELECT 1 FROM team_members m
            WHERE m.team_id = o.team_id AND m.member_id = $2))  AS team_owned_member,
       (SELECT COUNT(*) FROM object_shares s
        WHERE s.object_type = '{type}' AND s.object_id = o.id::text) AS share_count
FROM {table} o
-- Direct user share naming the caller. At most one row: object_shares
-- is UNIQUE on (object_type, object_id, share_type, share_target).
LEFT JOIN object_shares us
       ON us.object_type = '{type}'
      AND us.object_id   = o.id::text
      AND us.share_type  = 'user'
      AND us.share_target = $2
-- Strongest team share reaching the caller. LATERAL + LIMIT 1 so a user
-- in three teams that all have a share doesn't fan the result out.
LEFT JOIN LATERAL (
    SELECT os.permission, os.granted_by, os.created_at, os.share_target
    FROM object_shares os
    JOIN team_members tm ON tm.team_id::text = os.share_target
                        AND tm.member_id     = $2
    WHERE os.object_type = '{type}'
      AND os.object_id   = o.id::text
      AND os.share_type  = 'team'
    ORDER BY CASE os.permission
                WHEN 'admin' THEN 3 WHEN 'edit' THEN 2 ELSE 1 END DESC
    LIMIT 1
) ts ON TRUE
WHERE o.id::text = ANY($1)
"#,
        table = table,
        type = object_type
    )
}

/// Resolve access provenance for a page of forecasts in one round trip
/// (two, when any row needs the portfolio-inheritance fallback).
///
/// Returns a map keyed by forecast id. Ids the caller genuinely can't
/// see are simply absent — this helper explains access, it does not
/// grant it, and it is only ever called on rows an ACL-filtered query
/// already returned.
pub async fn forecast_access_provenance(
    pool: &PgPool,
    user_id: &str,
    ids: &[String],
) -> HashMap<String, AccessProvenance> {
    let mut out = base_provenance(pool, user_id, ids, "fermi_forecasts", "forecast").await;

    // Portfolio inheritance only matters for rows nothing else explained.
    // Reuses the ACL's own SQL const so the explanation can't drift from
    // the enforcement (Spec 26 §2.2).
    let unexplained: Vec<String> = ids
        .iter()
        .filter(|id| {
            out.get(*id)
                .map(|p| p.access_via.is_empty())
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    if !unexplained.is_empty() {
        if let Ok(rows) = sqlx::query(&inherited_access_by_ids_sql())
            .bind(&unexplained)
            .bind(user_id)
            .fetch_all(pool)
            .await
        {
            let mut team_ids: Vec<String> = Vec::new();
            let mut staged: Vec<(String, String, String, String, Option<String>)> = Vec::new();
            for r in rows {
                let fid: String = match r.try_get("forecast_id") {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let perm: String = r.try_get("permission").unwrap_or_else(|_| "view".into());
                let pid: String = r.try_get("portfolio_id").unwrap_or_default();
                let ptitle: String = r.try_get("portfolio_title").unwrap_or_default();
                let tid: Option<String> = r.try_get("team_id").ok().flatten();
                if let Some(ref t) = tid {
                    team_ids.push(t.clone());
                }
                staged.push((fid, perm, pid, ptitle, tid));
            }
            let team_names = resolve_team_names(pool, &team_ids).await;
            for (fid, perm, pid, ptitle, tid) in staged {
                if let Some(entry) = out.get_mut(&fid) {
                    entry.access_via = "portfolio".into();
                    entry.permission = perm;
                    entry.via_portfolio_id = Some(pid);
                    entry.via_portfolio_title = Some(ptitle);
                    entry.team_name = tid.as_ref().and_then(|t| team_names.get(t).cloned());
                    entry.team_id = tid;
                }
            }
        }
    }

    // Anything still unexplained fell through to bare visibility.
    for prov in out.values_mut() {
        if prov.access_via.is_empty() {
            prov.access_via = "unknown".into();
        }
    }
    out
}

/// Portfolio counterpart. Portfolios don't inherit from anything, so
/// this is just the base resolution.
pub async fn portfolio_access_provenance(
    pool: &PgPool,
    user_id: &str,
    ids: &[String],
) -> HashMap<String, AccessProvenance> {
    let mut out = base_provenance(pool, user_id, ids, "fermi_portfolios", "portfolio").await;
    for prov in out.values_mut() {
        if prov.access_via.is_empty() {
            prov.access_via = "unknown".into();
        }
    }
    out
}

/// The shared body of the two provenance resolvers. Leaves `access_via`
/// empty when no branch matched so the forecast variant knows which ids
/// still need the inheritance fallback.
async fn base_provenance(
    pool: &PgPool,
    user_id: &str,
    ids: &[String],
    table: &str,
    object_type: &str,
) -> HashMap<String, AccessProvenance> {
    let mut out: HashMap<String, AccessProvenance> = HashMap::new();
    if ids.is_empty() {
        return out;
    }
    let unique: Vec<String> = ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let rows = match sqlx::query(&provenance_sql(table, object_type))
        .bind(&unique)
        .bind(user_id)
        .fetch_all(pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, table = table, "[collab] provenance query failed");
            return out;
        }
    };

    // Stage first, then batch-resolve the names the rows actually need.
    struct Staged {
        object_id: String,
        owner_id: String,
        visibility: String,
        team_id: Option<String>,
        user_perm: Option<String>,
        user_grantor: Option<String>,
        user_at: Option<String>,
        team_perm: Option<String>,
        team_grantor: Option<String>,
        team_at: Option<String>,
        team_share_target: Option<String>,
        team_owned_member: bool,
        share_count: i64,
    }

    let mut staged: Vec<Staged> = Vec::new();
    let mut grantors: Vec<String> = Vec::new();
    let mut team_refs: Vec<String> = Vec::new();

    for r in &rows {
        let s = Staged {
            object_id: r.try_get::<String, _>("object_id").unwrap_or_default(),
            owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
            visibility: r.try_get::<String, _>("visibility").unwrap_or_default(),
            team_id: r.try_get::<Option<String>, _>("team_id").ok().flatten(),
            user_perm: r.try_get::<Option<String>, _>("user_perm").ok().flatten(),
            user_grantor: r
                .try_get::<Option<String>, _>("user_grantor")
                .ok()
                .flatten(),
            user_at: ts_string(r, "user_at"),
            team_perm: r.try_get::<Option<String>, _>("team_perm").ok().flatten(),
            team_grantor: r
                .try_get::<Option<String>, _>("team_grantor")
                .ok()
                .flatten(),
            team_at: ts_string(r, "team_at"),
            team_share_target: r
                .try_get::<Option<String>, _>("team_share_target")
                .ok()
                .flatten(),
            team_owned_member: r
                .try_get::<Option<bool>, _>("team_owned_member")
                .ok()
                .flatten()
                .unwrap_or(false),
            share_count: r.try_get::<i64, _>("share_count").unwrap_or(0),
        };
        if let Some(ref g) = s.user_grantor {
            grantors.push(g.clone());
        }
        if let Some(ref g) = s.team_grantor {
            grantors.push(g.clone());
        }
        if let Some(ref t) = s.team_id {
            team_refs.push(t.clone());
        }
        if let Some(ref t) = s.team_share_target {
            team_refs.push(t.clone());
        }
        staged.push(s);
    }

    let names = resolve_user_names(pool, &grantors).await;
    let team_names = resolve_team_names(pool, &team_refs).await;

    for s in staged {
        let mut prov = AccessProvenance {
            share_count: s.share_count,
            ..Default::default()
        };

        // Precedence chain — identical ordering to can_access so the
        // label always names the branch that actually granted access.
        if s.owner_id == user_id {
            prov.access_via = "owner".into();
            prov.permission = "admin".into();
        } else if let Some(perm) = s.user_perm.clone() {
            prov.access_via = "user_share".into();
            prov.permission = perm;
            prov.shared_by_display_name =
                s.user_grantor.as_ref().and_then(|g| names.get(g).cloned());
            prov.shared_by = s.user_grantor.clone();
            prov.shared_at = s.user_at.clone();
        } else if s.team_owned_member {
            prov.access_via = "team_owned".into();
            prov.permission = "edit".into();
            prov.team_name = s.team_id.as_ref().and_then(|t| team_names.get(t).cloned());
            prov.team_id = s.team_id.clone();
        } else if let Some(perm) = s.team_perm.clone() {
            prov.access_via = "team_share".into();
            prov.permission = perm;
            prov.shared_by_display_name =
                s.team_grantor.as_ref().and_then(|g| names.get(g).cloned());
            prov.shared_by = s.team_grantor.clone();
            prov.shared_at = s.team_at.clone();
            prov.team_name = s
                .team_share_target
                .as_ref()
                .and_then(|t| team_names.get(t).cloned());
            prov.team_id = s.team_share_target.clone();
        } else if s.visibility == "public" {
            prov.access_via = "public".into();
            prov.permission = "view".into();
        } else if s.visibility == "shared" {
            prov.access_via = "link".into();
            prov.permission = "view".into();
        }
        // else: leave access_via empty — the forecast resolver will try
        // portfolio inheritance next.

        out.insert(s.object_id, prov);
    }

    out
}

/// Read a TIMESTAMPTZ column as an RFC-3339 string, tolerating absence.
fn ts_string(row: &sqlx::postgres::PgRow, col: &str) -> Option<String> {
    row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(col)
        .ok()
        .flatten()
        .map(|t| t.to_rfc3339())
}

// ═══════════════════════════════════════════════════════════════════════
// Portfolio context — "is this standalone or curated?"
// ═══════════════════════════════════════════════════════════════════════

/// For a page of forecast ids, which portfolios each belongs to.
///
/// This is the fix for "I can't tell the portfolio context from the
/// standalone context": an empty vec means **standalone**, one entry
/// means it lives in that portfolio, more than one means it's shared
/// curation across books.
///
/// Scoped to portfolios the caller can see, so a colleague's private
/// book that happens to contain a forecast I own doesn't leak its title
/// to me through this field.
pub async fn forecast_portfolio_memberships(
    pool: &PgPool,
    user_id: &str,
    ids: &[String],
) -> HashMap<String, Vec<JsonValue>> {
    let mut out: HashMap<String, Vec<JsonValue>> = HashMap::new();
    if ids.is_empty() {
        return out;
    }
    let unique: Vec<String> = ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let rows = sqlx::query(
        "SELECT pf.forecast_id,
                p.id::text        AS portfolio_id,
                p.title,
                p.owner_id::text  AS owner_id,
                p.team_id::text   AS team_id,
                pf.added_at,
                pf.added_by
         FROM fermi_portfolio_forecasts pf
         JOIN fermi_portfolios p ON p.id = pf.portfolio_id
         WHERE pf.forecast_id = ANY($1)
           AND (
                 p.owner_id::text = $2
              OR p.visibility IN ('shared', 'public')
              OR (p.team_id IS NOT NULL AND EXISTS (
                    SELECT 1 FROM team_members m
                    WHERE m.team_id = p.team_id AND m.member_id = $2))
              OR EXISTS (
                    SELECT 1 FROM object_shares s
                    LEFT JOIN team_members tm
                           ON s.share_type = 'team'
                          AND s.share_target = tm.team_id::text
                          AND tm.member_id = $2
                    WHERE s.object_type = 'portfolio'
                      AND s.object_id   = p.id::text
                      AND ((s.share_type = 'user' AND s.share_target = $2)
                        OR (s.share_type = 'team' AND tm.member_id IS NOT NULL)))
               )
         ORDER BY pf.added_at DESC",
    )
    .bind(&unique)
    .bind(user_id)
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[collab] portfolio membership query failed");
            return out;
        }
    };

    let adders: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<Option<String>, _>("added_by").ok().flatten())
        .collect();
    let names = resolve_user_names(pool, &adders).await;

    for r in &rows {
        let fid: String = match r.try_get("forecast_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let added_by: Option<String> = r.try_get::<Option<String>, _>("added_by").ok().flatten();
        out.entry(fid).or_default().push(json!({
            "id":                    r.try_get::<String, _>("portfolio_id").ok(),
            "title":                 r.try_get::<String, _>("title").ok(),
            "owner_id":              r.try_get::<String, _>("owner_id").ok(),
            "team_id":               r.try_get::<Option<String>, _>("team_id").ok().flatten(),
            "added_at":              ts_string(r, "added_at"),
            "added_by":              added_by.clone(),
            "added_by_display_name": added_by.as_ref().and_then(|a| names.get(a).cloned()),
        }));
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// Activity feed — derived event stream
// ═══════════════════════════════════════════════════════════════════════

/// The forecast-scoped event UNION (Spec 26 §4.2).
///
/// `$1` = `TEXT[]` of forecast ids. Every branch projects the same 16
/// columns so the UNION type-checks; unused slots carry explicit casts
/// (`NULL::real`, `NULL::text`) because Postgres cannot infer a type for
/// a bare NULL in the first branch of a UNION.
///
/// Not `LIMIT`ed here — the caller filters and truncates in Rust so that
/// `?actor=`/`?kind=` narrow the *result*, not the window scanned.
const FORECAST_EVENTS_SQL: &str = r#"
-- Authoring. 'created' for drafts, 'published' once live: a draft
-- nobody else can act on isn't a team event yet.
SELECT f.created_at                     AS ts,
       CASE WHEN f.status = 'draft' THEN 'created' ELSE 'published' END AS kind,
       f.owner_id::text                 AS actor,
       NULL::text                       AS agent_id,
       f.id                             AS forecast_id,
       f.question_text                  AS question_text,
       NULL::real                       AS prev_probability,
       f.predicted_probability          AS new_probability,
       NULL::text                       AS reason,
       NULL::text                       AS revision_trigger,
       NULL::boolean                    AS outcome,
       NULL::real                       AS brier_score,
       NULL::text                       AS ref_type,
       NULL::text                       AS ref_id,
       NULL::text                       AS ref_label,
       NULL::text                       AS permission
FROM fermi_forecasts f
WHERE f.id = ANY($1)

UNION ALL

-- Revisions. actor_user_id is the human; agent_id is the tool they
-- pointed at the problem. Both are surfaced — "Alice · via elo-scout".
SELECT u.created_at,
       'revised',
       u.actor_user_id,
       u.agent_id,
       u.forecast_id,
       f.question_text,
       u.previous_probability,
       u.new_probability,
       u.reason,
       u.revision_trigger,
       NULL::boolean,
       NULL::real,
       NULL::text,
       NULL::text,
       NULL::text,
       NULL::text
FROM fermi_forecast_updates u
JOIN fermi_forecasts f ON f.id = u.forecast_id
WHERE u.forecast_id = ANY($1)

UNION ALL

-- Resolution. resolved_by has been recorded since migration 094, so
-- this branch is attributed for all history.
SELECT f.resolved_at,
       'resolved',
       f.resolved_by,
       NULL::text,
       f.id,
       f.question_text,
       NULL::real,
       f.predicted_probability,
       f.resolution_notes,
       NULL::text,
       f.actual_outcome,
       f.brier_score,
       NULL::text,
       NULL::text,
       NULL::text,
       NULL::text
FROM fermi_forecasts f
WHERE f.id = ANY($1) AND f.resolved_at IS NOT NULL

UNION ALL

-- Access grants. Only extant shares — a revoked share's row is gone,
-- so revocations are structurally unrecoverable (Spec 26 §6).
SELECT s.created_at,
       'shared',
       s.granted_by,
       NULL::text,
       s.object_id,
       f.question_text,
       NULL::real,
       NULL::real,
       NULL::text,
       NULL::text,
       NULL::boolean,
       NULL::real,
       s.share_type,
       s.share_target,
       NULL::text,
       s.permission
FROM object_shares s
JOIN fermi_forecasts f ON f.id = s.object_id
WHERE s.object_type = 'forecast' AND s.object_id = ANY($1)

UNION ALL

-- Curation. added_by is backfilled to the portfolio owner for pre-176
-- rows (migration 176 rationale).
SELECT pf.added_at,
       'portfolio_add',
       pf.added_by,
       NULL::text,
       pf.forecast_id,
       f.question_text,
       NULL::real,
       NULL::real,
       NULL::text,
       NULL::text,
       NULL::boolean,
       NULL::real,
       'portfolio',
       pf.portfolio_id,
       p.title,
       NULL::text
FROM fermi_portfolio_forecasts pf
JOIN fermi_forecasts   f ON f.id = pf.forecast_id
JOIN fermi_portfolios  p ON p.id = pf.portfolio_id
WHERE pf.forecast_id = ANY($1)

UNION ALL

-- Invitations addressed at a specific forecast.
SELECT i.created_at,
       'invited',
       i.inviter_id,
       NULL::text,
       i.target_id,
       f.question_text,
       NULL::real,
       NULL::real,
       i.message,
       i.status,
       NULL::boolean,
       NULL::real,
       'invite',
       i.id::text,
       COALESCE(i.invitee_email, i.invitee_user_id),
       i.permission
FROM forecast_invites i
JOIN fermi_forecasts f ON f.id = i.target_id
WHERE i.target_type = 'forecast' AND i.target_id = ANY($1)

ORDER BY ts DESC
"#;

/// A single derived event, pre-serialisation.
struct RawEvent {
    ts: Option<chrono::DateTime<chrono::Utc>>,
    kind: String,
    actor: Option<String>,
    agent_id: Option<String>,
    object_type: String,
    object_id: String,
    object_title: Option<String>,
    prev: Option<f32>,
    newp: Option<f32>,
    reason: Option<String>,
    revision_trigger: Option<String>,
    outcome: Option<bool>,
    brier: Option<f32>,
    ref_type: Option<String>,
    ref_id: Option<String>,
    ref_label: Option<String>,
    permission: Option<String>,
}

impl RawEvent {
    /// One-line human summary. Built server-side so every client — the
    /// console, the web UI, an MCP tool — tells the same story.
    fn summary(&self) -> String {
        let pct = |v: Option<f32>| {
            v.map(|x| format!("{:.0}%", x * 100.0))
                .unwrap_or_else(|| "?".into())
        };
        match self.kind.as_str() {
            "created" => format!("drafted at {}", pct(self.newp)),
            "published" => format!("published at {}", pct(self.newp)),
            "revised" => {
                let base = format!("revised {} → {}", pct(self.prev), pct(self.newp));
                match (self.revision_trigger.as_deref(), self.agent_id.as_deref()) {
                    (Some(t), Some(a)) if t != "manual" => format!("{} ({} · {})", base, t, a),
                    (Some(t), None) if t != "manual" => format!("{} ({})", base, t),
                    (_, Some(a)) => format!("{} (via {})", base, a),
                    _ => base,
                }
            }
            "resolved" => {
                let out = match self.outcome {
                    Some(true) => "YES",
                    Some(false) => "NO",
                    None => "—",
                };
                match self.brier {
                    Some(b) => format!("resolved {} · Brier {:.3}", out, b),
                    None => format!("resolved {}", out),
                }
            }
            "shared" => {
                let target = self.ref_label.clone().or_else(|| self.ref_id.clone());
                let scope = match self.ref_type.as_deref() {
                    Some("team") => "team",
                    _ => "user",
                };
                format!(
                    "shared with {} {} ({})",
                    scope,
                    target.unwrap_or_else(|| "—".into()),
                    self.permission.clone().unwrap_or_else(|| "view".into())
                )
            }
            "portfolio_add" => format!(
                "added to portfolio ‹{}›",
                self.ref_label.clone().unwrap_or_else(|| "—".into())
            ),
            "portfolio_created" => "created this portfolio".to_string(),
            "invited" => format!(
                "invited {} ({}) — {}",
                self.ref_label.clone().unwrap_or_else(|| "—".into()),
                self.permission.clone().unwrap_or_else(|| "view".into()),
                self.revision_trigger
                    .clone()
                    .unwrap_or_else(|| "pending".into())
            ),
            "member_joined" => format!(
                "joined the team as {}",
                self.permission.clone().unwrap_or_else(|| "member".into())
            ),
            other => other.to_string(),
        }
    }

    fn to_json(&self, names: &HashMap<String, String>) -> JsonValue {
        // Attribution honesty: an unattributed row is 'system', never
        // silently blamed on the owner (Spec 26 §4.1).
        let (actor_kind, actor_display) = match self.actor.as_deref() {
            Some(a) if !a.is_empty() => (
                "user",
                names.get(a).cloned().or_else(|| Some(a.to_string())),
            ),
            _ if self.agent_id.is_some() => ("agent", self.agent_id.clone()),
            _ => ("system", None),
        };

        json!({
            "ts":                 self.ts.map(|t| t.to_rfc3339()),
            "kind":               self.kind,
            "actor_id":           self.actor,
            "actor_display_name": actor_display,
            "actor_kind":         actor_kind,
            "agent_id":           self.agent_id,
            "object_type":        self.object_type,
            "object_id":          self.object_id,
            "object_title":       self.object_title,
            "summary":            self.summary(),
            "detail": {
                "previous_probability": self.prev.map(|v| v as f64),
                "new_probability":      self.newp.map(|v| v as f64),
                "reason":               self.reason,
                "revision_trigger":     self.revision_trigger,
                "actual_outcome":       self.outcome,
                "brier_score":          self.brier.map(|v| v as f64),
                "ref_type":             self.ref_type,
                "ref_id":               self.ref_id,
                "ref_label":            self.ref_label,
                "permission":           self.permission,
            },
        })
    }
}

fn read_forecast_event(r: &sqlx::postgres::PgRow) -> RawEvent {
    RawEvent {
        ts: r
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("ts")
            .ok()
            .flatten(),
        kind: r.try_get::<String, _>("kind").unwrap_or_default(),
        actor: r
            .try_get::<Option<String>, _>("actor")
            .ok()
            .flatten()
            .filter(|s| !s.is_empty()),
        agent_id: r.try_get::<Option<String>, _>("agent_id").ok().flatten(),
        object_type: "forecast".into(),
        object_id: r.try_get::<String, _>("forecast_id").unwrap_or_default(),
        object_title: r
            .try_get::<Option<String>, _>("question_text")
            .ok()
            .flatten(),
        prev: r
            .try_get::<Option<f32>, _>("prev_probability")
            .ok()
            .flatten(),
        newp: r
            .try_get::<Option<f32>, _>("new_probability")
            .ok()
            .flatten(),
        reason: r.try_get::<Option<String>, _>("reason").ok().flatten(),
        revision_trigger: r
            .try_get::<Option<String>, _>("revision_trigger")
            .ok()
            .flatten(),
        outcome: r.try_get::<Option<bool>, _>("outcome").ok().flatten(),
        brier: r.try_get::<Option<f32>, _>("brier_score").ok().flatten(),
        ref_type: r.try_get::<Option<String>, _>("ref_type").ok().flatten(),
        ref_id: r.try_get::<Option<String>, _>("ref_id").ok().flatten(),
        ref_label: r.try_get::<Option<String>, _>("ref_label").ok().flatten(),
        permission: r.try_get::<Option<String>, _>("permission").ok().flatten(),
    }
}

/// Load, filter, sort and truncate the forecast-scoped event stream for
/// a set of forecast ids.
async fn load_forecast_events(
    pool: &PgPool,
    forecast_ids: &[String],
    q: &ActivityQuery,
) -> Vec<RawEvent> {
    if forecast_ids.is_empty() {
        return Vec::new();
    }
    let rows = match sqlx::query(FORECAST_EVENTS_SQL)
        .bind(forecast_ids)
        .fetch_all(pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[collab] forecast event query failed");
            return Vec::new();
        }
    };
    let kinds = q.kinds();
    rows.iter()
        .map(read_forecast_event)
        .filter(|e| e.ts.is_some())
        .filter(|e| match &q.actor {
            Some(a) => e.actor.as_deref() == Some(a.as_str()),
            None => true,
        })
        .filter(|e| match &kinds {
            Some(k) => k.contains(&e.kind),
            None => true,
        })
        .collect()
}

/// Sort newest-first, truncate, resolve actor names, serialise.
async fn finish_events(pool: &PgPool, mut events: Vec<RawEvent>, limit: i64) -> Vec<JsonValue> {
    events.sort_by(|a, b| b.ts.cmp(&a.ts));
    events.truncate(limit as usize);
    let actors: Vec<String> = events.iter().filter_map(|e| e.actor.clone()).collect();
    let names = resolve_user_names(pool, &actors).await;
    events.iter().map(|e| e.to_json(&names)).collect()
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/forecasts/:id/activity
// ═══════════════════════════════════════════════════════════════════════

/// Attributed history of one forecast: who drafted it, who moved the
/// number and why, which agent produced the evidence, who shared it with
/// whom, who pulled it into which portfolio, who resolved it.
pub async fn forecast_activity_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_forecast_view(pool, &forecast_id, &principal).await?;

    let ids = vec![forecast_id.clone()];
    let events = load_forecast_events(pool, &ids, &q).await;
    let json_events = finish_events(pool, events, q.limit_clamped()).await;

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "events": json_events,
        "count": json_events.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/portfolios/:id/activity
// ═══════════════════════════════════════════════════════════════════════

/// Portfolio-scoped feed: portfolio-level events (created, shared,
/// forecasts added) plus every event on every member forecast. This is
/// the "what has the team been doing in this book" view.
pub async fn portfolio_activity_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_portfolio_view(pool, &portfolio_id, &principal).await?;

    let member_ids: Vec<String> =
        sqlx::query("SELECT forecast_id FROM fermi_portfolio_forecasts WHERE portfolio_id = $1")
            .bind(&portfolio_id)
            .fetch_all(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .iter()
            .filter_map(|r| r.try_get::<String, _>("forecast_id").ok())
            .collect();

    let mut events = load_forecast_events(pool, &member_ids, &q).await;
    events.extend(portfolio_level_events(pool, &[portfolio_id.clone()], &q).await);

    let json_events = finish_events(pool, events, q.limit_clamped()).await;

    Ok(Json(json!({
        "portfolio_id": portfolio_id,
        "forecast_count": member_ids.len(),
        "events": json_events,
        "count": json_events.len(),
    })))
}

/// Portfolio-level events: creation and access grants. Member-forecast
/// events come from [`load_forecast_events`]; the `portfolio_add` event
/// lives there too (it's keyed by forecast, which is what the reader
/// cares about).
async fn portfolio_level_events(
    pool: &PgPool,
    portfolio_ids: &[String],
    q: &ActivityQuery,
) -> Vec<RawEvent> {
    if portfolio_ids.is_empty() {
        return Vec::new();
    }
    let sql = r#"
SELECT p.created_at            AS ts,
       'portfolio_created'     AS kind,
       p.owner_id::text        AS actor,
       p.id::text              AS object_id,
       p.title                 AS object_title,
       NULL::text              AS ref_type,
       NULL::text              AS ref_id,
       NULL::text              AS ref_label,
       NULL::text              AS permission
FROM fermi_portfolios p
WHERE p.id::text = ANY($1)

UNION ALL

SELECT s.created_at,
       'shared',
       s.granted_by,
       s.object_id,
       p.title,
       s.share_type,
       s.share_target,
       NULL::text,
       s.permission
FROM object_shares s
JOIN fermi_portfolios p ON p.id = s.object_id
WHERE s.object_type = 'portfolio' AND s.object_id = ANY($1)

ORDER BY ts DESC
"#;
    let rows = match sqlx::query(sql).bind(portfolio_ids).fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[collab] portfolio event query failed");
            return Vec::new();
        }
    };
    let kinds = q.kinds();
    rows.iter()
        .map(|r| RawEvent {
            ts: r
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("ts")
                .ok()
                .flatten(),
            kind: r.try_get::<String, _>("kind").unwrap_or_default(),
            actor: r
                .try_get::<Option<String>, _>("actor")
                .ok()
                .flatten()
                .filter(|s| !s.is_empty()),
            agent_id: None,
            object_type: "portfolio".into(),
            object_id: r.try_get::<String, _>("object_id").unwrap_or_default(),
            object_title: r
                .try_get::<Option<String>, _>("object_title")
                .ok()
                .flatten(),
            prev: None,
            newp: None,
            reason: None,
            revision_trigger: None,
            outcome: None,
            brier: None,
            ref_type: r.try_get::<Option<String>, _>("ref_type").ok().flatten(),
            ref_id: r.try_get::<Option<String>, _>("ref_id").ok().flatten(),
            ref_label: r.try_get::<Option<String>, _>("ref_label").ok().flatten(),
            permission: r.try_get::<Option<String>, _>("permission").ok().flatten(),
        })
        .filter(|e| e.ts.is_some())
        .filter(|e| match &q.actor {
            Some(a) => e.actor.as_deref() == Some(a.as_str()),
            None => true,
        })
        .filter(|e| match &kinds {
            Some(k) => k.contains(&e.kind),
            None => true,
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Team surface — what a team can collectively see
// ═══════════════════════════════════════════════════════════════════════

/// The forecast + portfolio ids that make up a team's shared surface
/// (Spec 26 §4.4):
///
///   1. forecasts/portfolios owned by the team (`team_id`),
///   2. objects explicitly shared with the team (`object_shares`),
///   3. forecasts inside (1) or (2) portfolios, subject to the same leak
///      guard the ACL applies — a colleague's private forecast parked in
///      a team book does not become team-visible unless its owner is on
///      the team.
pub(crate) async fn team_surface(
    pool: &PgPool,
    team_id: Uuid,
) -> Result<(Vec<String>, Vec<String>), (StatusCode, String)> {
    let tid_text = team_id.to_string();

    let portfolio_ids: Vec<String> = sqlx::query(
        "SELECT p.id::text AS id FROM fermi_portfolios p WHERE p.team_id = $1
         UNION
         SELECT s.object_id FROM object_shares s
         WHERE s.object_type = 'portfolio'
           AND s.share_type  = 'team'
           AND s.share_target = $2",
    )
    .bind(team_id)
    .bind(&tid_text)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .iter()
    .filter_map(|r| r.try_get::<String, _>("id").ok())
    .collect();

    let forecast_ids: Vec<String> = sqlx::query(
        "SELECT f.id AS id FROM fermi_forecasts f WHERE f.team_id = $1
         UNION
         SELECT s.object_id FROM object_shares s
         WHERE s.object_type = 'forecast'
           AND s.share_type  = 'team'
           AND s.share_target = $2
         UNION
         -- Portfolio-inherited members, leak-guarded exactly as the ACL
         -- does: forecast owner is the portfolio owner, or is on this team.
         SELECT pf.forecast_id
         FROM fermi_portfolio_forecasts pf
         JOIN fermi_portfolios p ON p.id = pf.portfolio_id
         JOIN fermi_forecasts  f ON f.id = pf.forecast_id
         WHERE p.id::text = ANY($3)
           AND (f.owner_id::text = p.owner_id::text
             OR EXISTS (SELECT 1 FROM team_members tm
                        WHERE tm.team_id = $1 AND tm.member_id = f.owner_id::text))",
    )
    .bind(team_id)
    .bind(&tid_text)
    .bind(&portfolio_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .iter()
    .filter_map(|r| r.try_get::<String, _>("id").ok())
    .collect();

    Ok((forecast_ids, portfolio_ids))
}

/// Membership as activity: joins are team events too, and they explain
/// why the roster looks the way it does.
async fn team_membership_events(pool: &PgPool, team_id: Uuid, q: &ActivityQuery) -> Vec<RawEvent> {
    let rows = match sqlx::query(
        "SELECT tm.joined_at AS ts, tm.member_id, tm.member_type, tm.role,
                tm.invited_by, t.name AS team_name
         FROM team_members tm
         JOIN teams t ON t.id = tm.team_id
         WHERE tm.team_id = $1
         ORDER BY tm.joined_at DESC",
    )
    .bind(team_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[collab] team membership event query failed");
            return Vec::new();
        }
    };
    let kinds = q.kinds();
    rows.iter()
        .map(|r| RawEvent {
            ts: r
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("ts")
                .ok()
                .flatten(),
            kind: "member_joined".into(),
            actor: r.try_get::<Option<String>, _>("member_id").ok().flatten(),
            agent_id: match r.try_get::<Option<String>, _>("member_type").ok().flatten() {
                // An agent member's "actor" IS the agent; carry it in
                // agent_id so to_json classifies actor_kind correctly.
                Some(t) if t == "agent" => {
                    r.try_get::<Option<String>, _>("member_id").ok().flatten()
                }
                _ => None,
            },
            object_type: "team".into(),
            object_id: team_id.to_string(),
            object_title: r.try_get::<Option<String>, _>("team_name").ok().flatten(),
            prev: None,
            newp: None,
            reason: None,
            revision_trigger: None,
            outcome: None,
            brier: None,
            ref_type: Some("invited_by".into()),
            ref_id: r.try_get::<Option<String>, _>("invited_by").ok().flatten(),
            ref_label: None,
            permission: r.try_get::<Option<String>, _>("role").ok().flatten(),
        })
        .filter(|e| e.ts.is_some())
        .filter(|e| match &q.actor {
            Some(a) => e.actor.as_deref() == Some(a.as_str()),
            None => true,
        })
        .filter(|e| match &kinds {
            Some(k) => k.contains(&e.kind),
            None => true,
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/teams/:id/activity
// ═══════════════════════════════════════════════════════════════════════

/// The team feed. Every event, by any actor, across everything the team
/// can see — the direct answer to "the team context is hard to follow".
///
/// `?actor=<user_id>` narrows to one teammate; `?kind=revised,resolved`
/// narrows to one class of act. Together they turn the feed into the
/// "which team members did which things" query without a new endpoint.
pub async fn team_activity_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<Uuid>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_team_member(pool, team_id, &principal).await?;

    let (forecast_ids, portfolio_ids) = team_surface(pool, team_id).await?;

    let mut events = load_forecast_events(pool, &forecast_ids, &q).await;
    events.extend(portfolio_level_events(pool, &portfolio_ids, &q).await);
    events.extend(team_membership_events(pool, team_id, &q).await);

    let json_events = finish_events(pool, events, q.limit_clamped()).await;

    Ok(Json(json!({
        "team_id": team_id,
        "surface": {
            "forecast_count":  forecast_ids.len(),
            "portfolio_count": portfolio_ids.len(),
        },
        "events": json_events,
        "count": json_events.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/teams/:id/contributions
// ═══════════════════════════════════════════════════════════════════════

/// Per-member contribution roll-up over the team's shared surface.
///
/// The Roster tab used to be a list of names and roles — organisational
/// trivia. This makes it a working document: who is moving numbers, who
/// is resolving, who is curating, who has gone quiet.
///
/// All counts are scoped to the team surface, so a member's unrelated
/// personal forecasting doesn't inflate their team contribution.
pub async fn team_contributions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<Uuid>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_team_member(pool, team_id, &principal).await?;

    let (forecast_ids, portfolio_ids) = team_surface(pool, team_id).await?;

    let rows = sqlx::query(
        r#"
SELECT tm.member_id,
       tm.member_type,
       tm.role,
       tm.joined_at,
       tm.invited_by,
       (SELECT COUNT(*) FROM fermi_forecast_updates u
        WHERE u.actor_user_id = tm.member_id AND u.forecast_id = ANY($2)) AS revisions,
       (SELECT COUNT(*) FROM fermi_forecasts f
        WHERE f.resolved_by = tm.member_id AND f.id = ANY($2))            AS resolutions,
       (SELECT COUNT(*) FROM fermi_forecasts f
        WHERE f.owner_id::text = tm.member_id AND f.id = ANY($2))         AS authored,
       (SELECT COUNT(*) FROM object_shares s
        WHERE s.granted_by = tm.member_id
          AND ((s.object_type = 'forecast'  AND s.object_id = ANY($2))
            OR (s.object_type = 'portfolio' AND s.object_id = ANY($3))))  AS shares_granted,
       (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
        WHERE pf.added_by = tm.member_id AND pf.portfolio_id = ANY($3))   AS curations,
       GREATEST(
           (SELECT MAX(u.created_at) FROM fermi_forecast_updates u
            WHERE u.actor_user_id = tm.member_id AND u.forecast_id = ANY($2)),
           (SELECT MAX(f.updated_at) FROM fermi_forecasts f
            WHERE f.owner_id::text = tm.member_id AND f.id = ANY($2)),
           (SELECT MAX(s.created_at) FROM object_shares s
            WHERE s.granted_by = tm.member_id
              AND ((s.object_type = 'forecast'  AND s.object_id = ANY($2))
                OR (s.object_type = 'portfolio' AND s.object_id = ANY($3))))
       )                                                                  AS last_active_at
FROM team_members tm
WHERE tm.team_id = $1
ORDER BY tm.joined_at
"#,
    )
    .bind(team_id)
    .bind(&forecast_ids)
    .bind(&portfolio_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let member_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("member_id").ok())
        .collect();
    let names = resolve_user_names(pool, &member_ids).await;

    let members: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            let mid: String = r.try_get("member_id").unwrap_or_default();
            let revisions = r.try_get::<i64, _>("revisions").unwrap_or(0);
            let resolutions = r.try_get::<i64, _>("resolutions").unwrap_or(0);
            let authored = r.try_get::<i64, _>("authored").unwrap_or(0);
            let shares = r.try_get::<i64, _>("shares_granted").unwrap_or(0);
            let curations = r.try_get::<i64, _>("curations").unwrap_or(0);
            json!({
                "member_id":           mid.clone(),
                "member_display_name": names.get(&mid).cloned(),
                "member_type":         r.try_get::<String, _>("member_type").ok(),
                "role":                r.try_get::<String, _>("role").ok(),
                "joined_at":           ts_string(r, "joined_at"),
                "invited_by":          r.try_get::<Option<String>, _>("invited_by").ok().flatten(),
                "revisions":           revisions,
                "resolutions":         resolutions,
                "authored":            authored,
                "shares_granted":      shares,
                "curations":           curations,
                // One number for sorting the roster by "who is actually
                // carrying this team". Deliberately unweighted — any
                // weighting would be a judgement the operator should make.
                "total_actions":       revisions + resolutions + authored + shares + curations,
                "last_active_at":      ts_string(r, "last_active_at"),
            })
        })
        .collect();

    Ok(Json(json!({
        "team_id": team_id,
        "surface": {
            "forecast_count":  forecast_ids.len(),
            "portfolio_count": portfolio_ids.len(),
        },
        "members": members,
        "count": members.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/teams/:id/shared
// ═══════════════════════════════════════════════════════════════════════

/// Canonical inventory of what is shared with a team, and by whom.
///
/// Replaces the console's old client-side guess (filter my own forecasts
/// by `team_id`), which could only ever see objects the *caller* owned —
/// so a forecast a teammate shared with the team was invisible in the
/// Teams panel while being perfectly visible in the Portfolio panel.
/// That inconsistency is the "team views feel anemic" complaint.
///
/// Every row carries `via` (`team_owned` | `team_share` | `portfolio`),
/// the grantor, the timestamp and the permission, so the operator can
/// always answer "why is this here".
pub async fn team_shared_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<Uuid>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_team_member(pool, team_id, &principal).await?;
    let tid_text = team_id.to_string();

    // ── Portfolios ────────────────────────────────────────────────────
    let portfolio_rows = sqlx::query(
        r#"
SELECT p.id::text        AS id,
       p.title,
       p.description,
       p.owner_id::text   AS owner_id,
       p.visibility,
       p.domain,
       p.team_id::text    AS team_id,
       p.created_at,
       p.updated_at,
       CASE WHEN p.team_id = $1 THEN 'team_owned' ELSE 'team_share' END AS via,
       os.permission      AS permission,
       os.granted_by      AS shared_by,
       os.created_at      AS shared_at,
       (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
        WHERE pf.portfolio_id = p.id)                                   AS forecast_count,
       (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
        JOIN fermi_forecasts f ON f.id = pf.forecast_id
        WHERE pf.portfolio_id = p.id AND f.status = 'resolved')         AS resolved_count,
       (SELECT AVG(f.brier_score)::float8 FROM fermi_portfolio_forecasts pf
        JOIN fermi_forecasts f ON f.id = pf.forecast_id
        WHERE pf.portfolio_id = p.id AND f.brier_score IS NOT NULL)     AS avg_brier
FROM fermi_portfolios p
LEFT JOIN object_shares os
       ON os.object_type  = 'portfolio'
      AND os.object_id    = p.id::text
      AND os.share_type   = 'team'
      AND os.share_target = $2
WHERE p.team_id = $1 OR os.id IS NOT NULL
ORDER BY p.updated_at DESC
"#,
    )
    .bind(team_id)
    .bind(&tid_text)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ── Forecasts: direct (team-owned or team-shared) ──────────────────
    let direct_rows = sqlx::query(
        r#"
SELECT f.id,
       f.question_text,
       f.owner_id::text  AS owner_id,
       f.predicted_probability,
       f.status,
       f.brier_score,
       f.actual_outcome,
       f.visibility,
       f.domain,
       f.tags,
       f.target_date,
       f.created_at,
       f.updated_at,
       f.resolved_at,
       f.team_id::text   AS team_id,
       CASE WHEN f.team_id = $1 THEN 'team_owned' ELSE 'team_share' END AS via,
       os.permission     AS permission,
       os.granted_by     AS shared_by,
       os.created_at     AS shared_at,
       NULL::text        AS via_portfolio_id,
       NULL::text        AS via_portfolio_title,
       (SELECT COUNT(*) FROM fermi_forecast_updates u
        WHERE u.forecast_id = f.id
          AND u.created_at > NOW() - INTERVAL '7 days')                 AS n_recent_updates
FROM fermi_forecasts f
LEFT JOIN object_shares os
       ON os.object_type  = 'forecast'
      AND os.object_id    = f.id
      AND os.share_type   = 'team'
      AND os.share_target = $2
WHERE f.team_id = $1 OR os.id IS NOT NULL
"#,
    )
    .bind(team_id)
    .bind(&tid_text)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let direct_ids: HashSet<String> = direct_rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect();

    // ── Forecasts: inherited via a team portfolio ──────────────────────
    //
    // Same leak guard as the ACL. Rows already present directly are
    // filtered out in Rust so the stronger provenance wins.
    let portfolio_ids: Vec<String> = portfolio_rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect();

    let inherited_rows = if portfolio_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query(
            r#"
SELECT DISTINCT ON (f.id)
       f.id,
       f.question_text,
       f.owner_id::text  AS owner_id,
       f.predicted_probability,
       f.status,
       f.brier_score,
       f.actual_outcome,
       f.visibility,
       f.domain,
       f.tags,
       f.target_date,
       f.created_at,
       f.updated_at,
       f.resolved_at,
       f.team_id::text   AS team_id,
       'portfolio'       AS via,
       COALESCE(os.permission, 'edit') AS permission,
       COALESCE(os.granted_by, p.owner_id::text) AS shared_by,
       COALESCE(os.created_at, pf.added_at)      AS shared_at,
       p.id::text        AS via_portfolio_id,
       p.title           AS via_portfolio_title,
       (SELECT COUNT(*) FROM fermi_forecast_updates u
        WHERE u.forecast_id = f.id
          AND u.created_at > NOW() - INTERVAL '7 days')                 AS n_recent_updates
FROM fermi_portfolio_forecasts pf
JOIN fermi_portfolios p ON p.id = pf.portfolio_id
JOIN fermi_forecasts  f ON f.id = pf.forecast_id
LEFT JOIN object_shares os
       ON os.object_type  = 'portfolio'
      AND os.object_id    = p.id::text
      AND os.share_type   = 'team'
      AND os.share_target = $2
WHERE p.id::text = ANY($3)
  AND (f.owner_id::text = p.owner_id::text
    OR EXISTS (SELECT 1 FROM team_members tm
               WHERE tm.team_id = $1 AND tm.member_id = f.owner_id::text))
ORDER BY f.id, pf.added_at DESC
"#,
        )
        .bind(team_id)
        .bind(&tid_text)
        .bind(&portfolio_ids)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    // ── Name resolution for every grantor + owner in one pass ─────────
    let mut people: Vec<String> = Vec::new();
    for r in portfolio_rows
        .iter()
        .chain(direct_rows.iter())
        .chain(inherited_rows.iter())
    {
        if let Ok(Some(v)) = r.try_get::<Option<String>, _>("shared_by") {
            people.push(v);
        }
        if let Ok(v) = r.try_get::<String, _>("owner_id") {
            people.push(v);
        }
    }
    let names = resolve_user_names(pool, &people).await;

    let portfolios: Vec<JsonValue> = portfolio_rows
        .iter()
        .map(|r| {
            let owner: String = r.try_get("owner_id").unwrap_or_default();
            let shared_by: Option<String> =
                r.try_get::<Option<String>, _>("shared_by").ok().flatten();
            json!({
                "id":                     r.try_get::<String, _>("id").ok(),
                "title":                  r.try_get::<String, _>("title").ok(),
                "description":            r.try_get::<Option<String>, _>("description").ok().flatten(),
                "owner_id":               owner.clone(),
                "owner_display_name":     names.get(&owner).cloned(),
                "visibility":             r.try_get::<String, _>("visibility").ok(),
                "domain":                 r.try_get::<Option<String>, _>("domain").ok().flatten(),
                "team_id":                r.try_get::<Option<String>, _>("team_id").ok().flatten(),
                "forecast_count":         r.try_get::<i64, _>("forecast_count").unwrap_or(0),
                "resolved_count":         r.try_get::<i64, _>("resolved_count").unwrap_or(0),
                "avg_brier":              r.try_get::<Option<f64>, _>("avg_brier").ok().flatten(),
                "via":                    r.try_get::<String, _>("via").ok(),
                "permission":             r.try_get::<Option<String>, _>("permission").ok().flatten(),
                "shared_by":              shared_by.clone(),
                "shared_by_display_name": shared_by.as_ref().and_then(|s| names.get(s).cloned()),
                "shared_at":              ts_string(r, "shared_at"),
                "created_at":             ts_string(r, "created_at"),
                "updated_at":             ts_string(r, "updated_at"),
            })
        })
        .collect();

    let to_forecast_json = |r: &sqlx::postgres::PgRow| -> JsonValue {
        let owner: String = r.try_get("owner_id").unwrap_or_default();
        let shared_by: Option<String> = r.try_get::<Option<String>, _>("shared_by").ok().flatten();
        json!({
            "id":                     r.try_get::<String, _>("id").ok(),
            "question_text":          r.try_get::<String, _>("question_text").ok(),
            "owner_id":               owner.clone(),
            "owner_display_name":     names.get(&owner).cloned(),
            "predicted_probability":  r.try_get::<Option<f32>, _>("predicted_probability").ok().flatten().map(|v| v as f64),
            "status":                 r.try_get::<String, _>("status").ok(),
            "brier_score":            r.try_get::<Option<f32>, _>("brier_score").ok().flatten().map(|v| v as f64),
            "actual_outcome":         r.try_get::<Option<bool>, _>("actual_outcome").ok().flatten(),
            "visibility":             r.try_get::<String, _>("visibility").ok(),
            "domain":                 r.try_get::<Option<String>, _>("domain").ok().flatten(),
            "tags":                   r.try_get::<Vec<String>, _>("tags").ok(),
            "target_date":            ts_string(r, "target_date"),
            "team_id":                r.try_get::<Option<String>, _>("team_id").ok().flatten(),
            "created_at":             ts_string(r, "created_at"),
            "updated_at":             ts_string(r, "updated_at"),
            "resolved_at":            ts_string(r, "resolved_at"),
            "n_recent_updates":       r.try_get::<i64, _>("n_recent_updates").unwrap_or(0),
            "via":                    r.try_get::<String, _>("via").ok(),
            "permission":             r.try_get::<Option<String>, _>("permission").ok().flatten(),
            "shared_by":              shared_by.clone(),
            "shared_by_display_name": shared_by.as_ref().and_then(|s| names.get(s).cloned()),
            "shared_at":              ts_string(r, "shared_at"),
            "via_portfolio_id":       r.try_get::<Option<String>, _>("via_portfolio_id").ok().flatten(),
            "via_portfolio_title":    r.try_get::<Option<String>, _>("via_portfolio_title").ok().flatten(),
        })
    };

    let mut forecasts: Vec<JsonValue> = direct_rows.iter().map(to_forecast_json).collect();
    forecasts.extend(
        inherited_rows
            .iter()
            .filter(|r| {
                r.try_get::<String, _>("id")
                    .map(|id| !direct_ids.contains(&id))
                    .unwrap_or(false)
            })
            .map(to_forecast_json),
    );

    // Newest-activity-first: the team wants to see what's moving.
    forecasts.sort_by(|a, b| {
        let key = |v: &JsonValue| {
            v.get("updated_at")
                .and_then(|x| x.as_str())
                .or_else(|| v.get("created_at").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string()
        };
        key(b).cmp(&key(a))
    });

    Ok(Json(json!({
        "team_id": team_id,
        "forecasts": forecasts,
        "portfolios": portfolios,
        "counts": {
            "forecasts":  forecasts.len(),
            "portfolios": portfolios.len(),
            "inherited":  forecasts.iter()
                              .filter(|f| f.get("via").and_then(|v| v.as_str()) == Some("portfolio"))
                              .count(),
        },
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/{forecasts,portfolios}/:id/access
// ═══════════════════════════════════════════════════════════════════════

/// Full access picture for one forecast: my own path in, every direct
/// share with grantor and timestamp, every share *inherited* from a
/// portfolio, and the flattened list of people who can actually see it
/// (teams expanded to members).
///
/// The Access tab previously showed raw `object_shares` rows and nothing
/// else, so the two most common questions — "does the whole team
/// actually have this?" and "why can Bo see it, I never shared it with
/// him?" — had no answer in the UI.
pub async fn forecast_access_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    let (owner_id, visibility) = require_forecast_view(pool, &forecast_id, &principal).await?;
    let user_id = principal.user_id();

    let level = can_access(
        pool,
        &principal,
        ObjectType::Forecast,
        &forecast_id,
        &owner_id,
        Visibility::from_legacy(&visibility),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let ids = vec![forecast_id.clone()];
    let prov = forecast_access_provenance(pool, &user_id, &ids)
        .await
        .remove(&forecast_id)
        .unwrap_or_default();

    let direct = enriched_shares(pool, "forecast", &forecast_id).await;

    // Inherited: every portfolio share that reaches this forecast,
    // regardless of whether it reaches *me*. Shown read-only in the UI
    // because the grant lives on the portfolio, not here.
    let inherited = sqlx::query(
        r#"
SELECT p.id::text     AS portfolio_id,
       p.title        AS portfolio_title,
       os.id::text    AS share_id,
       os.share_type,
       os.share_target,
       os.permission,
       os.granted_by,
       os.created_at
FROM fermi_portfolio_forecasts pf
JOIN fermi_portfolios p ON p.id = pf.portfolio_id
JOIN fermi_forecasts  f ON f.id = pf.forecast_id
JOIN object_shares   os ON os.object_type = 'portfolio' AND os.object_id = p.id::text
WHERE pf.forecast_id = $1
  AND (f.owner_id::text = p.owner_id::text
    OR (os.share_type = 'team' AND EXISTS (
          SELECT 1 FROM team_members tm
          WHERE tm.team_id::text = os.share_target
            AND tm.member_id     = f.owner_id::text)))
ORDER BY os.created_at
"#,
    )
    .bind(&forecast_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut people: Vec<String> = vec![owner_id.clone()];
    let mut team_refs: Vec<String> = Vec::new();
    for r in &inherited {
        if let Ok(v) = r.try_get::<String, _>("granted_by") {
            people.push(v);
        }
        if let (Ok(t), Ok(tgt)) = (
            r.try_get::<String, _>("share_type"),
            r.try_get::<String, _>("share_target"),
        ) {
            if t == "team" {
                team_refs.push(tgt);
            } else {
                people.push(tgt);
            }
        }
    }
    let names = resolve_user_names(pool, &people).await;
    let team_names = resolve_team_names(pool, &team_refs).await;

    let inherited_json: Vec<JsonValue> = inherited
        .iter()
        .map(|r| {
            let st: String = r.try_get("share_type").unwrap_or_default();
            let tgt: String = r.try_get("share_target").unwrap_or_default();
            let gb: String = r.try_get("granted_by").unwrap_or_default();
            json!({
                "share_id":                 r.try_get::<String, _>("share_id").ok(),
                "portfolio_id":             r.try_get::<String, _>("portfolio_id").ok(),
                "portfolio_title":          r.try_get::<String, _>("portfolio_title").ok(),
                "share_type":               st.clone(),
                "share_target":             tgt.clone(),
                "share_target_display_name": if st == "team" {
                    team_names.get(&tgt).cloned()
                } else {
                    names.get(&tgt).cloned()
                },
                "permission":               r.try_get::<String, _>("permission").ok(),
                "granted_by":               gb.clone(),
                "granted_by_display_name":  names.get(&gb).cloned(),
                "created_at":               ts_string(r, "created_at"),
            })
        })
        .collect();

    // Effective viewers: flatten every path into "these humans can see
    // this, and here's why". Team shares expand to their roster.
    let viewers = effective_viewers(pool, &owner_id, &direct, &inherited_json).await;

    Ok(Json(json!({
        "forecast_id":   forecast_id,
        "owner_id":      owner_id.clone(),
        "owner_display_name": names.get(&owner_id).cloned(),
        "visibility":    visibility,
        "my_permission": match level {
            fermi_auth::visibility::AccessLevel::Granted(p) => p.as_str(),
            fermi_auth::visibility::AccessLevel::Denied => "none",
        },
        "my_access":     prov.to_json(),
        "direct_shares": direct,
        "inherited_shares": inherited_json,
        "viewers":       viewers,
    })))
}

/// Portfolio counterpart of [`forecast_access_handler`]. No inherited
/// section — portfolios are the top of the containment chain — but it
/// adds `cascades_to`, the count of member forecasts that inherit these
/// grants, which is the number that makes the consequence of sharing a
/// portfolio legible before you click.
pub async fn portfolio_access_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    let (owner_id, visibility) = require_portfolio_view(pool, &portfolio_id, &principal).await?;
    let user_id = principal.user_id();

    let level = can_access(
        pool,
        &principal,
        ObjectType::Portfolio,
        &portfolio_id,
        &owner_id,
        Visibility::from_legacy(&visibility),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let ids = vec![portfolio_id.clone()];
    let prov = portfolio_access_provenance(pool, &user_id, &ids)
        .await
        .remove(&portfolio_id)
        .unwrap_or_default();

    let direct = enriched_shares(pool, "portfolio", &portfolio_id).await;

    // How many member forecasts these grants actually reach — the leak
    // guard means it's not simply "all of them".
    let cascade_row = sqlx::query(
        "SELECT COUNT(*) AS n
         FROM fermi_portfolio_forecasts pf
         JOIN fermi_portfolios p ON p.id = pf.portfolio_id
         JOIN fermi_forecasts  f ON f.id = pf.forecast_id
         WHERE pf.portfolio_id = $1
           AND (f.owner_id::text = p.owner_id::text
             OR EXISTS (SELECT 1 FROM object_shares os
                        JOIN team_members tm ON tm.team_id::text = os.share_target
                        WHERE os.object_type = 'portfolio'
                          AND os.object_id   = p.id::text
                          AND os.share_type  = 'team'
                          AND tm.member_id   = f.owner_id::text))",
    )
    .bind(&portfolio_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get::<i64, _>("n").ok())
    .unwrap_or(0);

    let total_members =
        sqlx::query("SELECT COUNT(*) AS n FROM fermi_portfolio_forecasts WHERE portfolio_id = $1")
            .bind(&portfolio_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get::<i64, _>("n").ok())
            .unwrap_or(0);

    let names = resolve_user_names(pool, &[owner_id.clone()]).await;
    let viewers = effective_viewers(pool, &owner_id, &direct, &[]).await;

    Ok(Json(json!({
        "portfolio_id":  portfolio_id,
        "owner_id":      owner_id.clone(),
        "owner_display_name": names.get(&owner_id).cloned(),
        "visibility":    visibility,
        "my_permission": match level {
            fermi_auth::visibility::AccessLevel::Granted(p) => p.as_str(),
            fermi_auth::visibility::AccessLevel::Denied => "none",
        },
        "my_access":     prov.to_json(),
        "direct_shares": direct,
        "cascades_to":   cascade_row,
        "forecast_count": total_members,
        "viewers":       viewers,
    })))
}

/// `object_shares` for one object, enriched with display names *and*
/// `created_at` (which `handlers::shares::enrich_shares` drops — without
/// it the UI can't say "shared 3 days ago", so every share looks equally
/// fresh).
async fn enriched_shares(pool: &PgPool, object_type: &str, object_id: &str) -> Vec<JsonValue> {
    let rows = sqlx::query(
        "SELECT id::text AS id, object_type, object_id, share_type, share_target,
                permission, granted_by, created_at
         FROM object_shares
         WHERE object_type = $1 AND object_id = $2
         ORDER BY created_at",
    )
    .bind(object_type)
    .bind(object_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut users: Vec<String> = Vec::new();
    let mut team_ids: Vec<String> = Vec::new();
    for r in &rows {
        if let Ok(v) = r.try_get::<String, _>("granted_by") {
            users.push(v);
        }
        match (
            r.try_get::<String, _>("share_type"),
            r.try_get::<String, _>("share_target"),
        ) {
            (Ok(t), Ok(tgt)) if t == "team" => team_ids.push(tgt),
            (Ok(_), Ok(tgt)) => users.push(tgt),
            _ => {}
        }
    }
    let names = resolve_user_names(pool, &users).await;
    let team_names = resolve_team_names(pool, &team_ids).await;

    // Team shares carry their roster inline: "shared with WC analysts
    // (4 people)" is the statement the operator needs, and fetching it
    // per-team client-side was the reason the Access tab felt hollow.
    let mut rosters: HashMap<String, Vec<JsonValue>> = HashMap::new();
    if !team_ids.is_empty() {
        if let Ok(member_rows) = sqlx::query(
            "SELECT tm.team_id::text AS team_id, tm.member_id, tm.member_type, tm.role
             FROM team_members tm
             WHERE tm.team_id::text = ANY($1)
             ORDER BY tm.joined_at",
        )
        .bind(&team_ids)
        .fetch_all(pool)
        .await
        {
            let member_ids: Vec<String> = member_rows
                .iter()
                .filter_map(|r| r.try_get::<String, _>("member_id").ok())
                .collect();
            let member_names = resolve_user_names(pool, &member_ids).await;
            for r in &member_rows {
                let tid: String = match r.try_get("team_id") {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let mid: String = r.try_get("member_id").unwrap_or_default();
                rosters.entry(tid).or_default().push(json!({
                    "member_id":           mid.clone(),
                    "member_display_name": member_names.get(&mid).cloned(),
                    "member_type":         r.try_get::<String, _>("member_type").ok(),
                    "role":                r.try_get::<String, _>("role").ok(),
                }));
            }
        }
    }

    rows.iter()
        .map(|r| {
            let st: String = r.try_get("share_type").unwrap_or_default();
            let tgt: String = r.try_get("share_target").unwrap_or_default();
            let gb: String = r.try_get("granted_by").unwrap_or_default();
            let is_team = st == "team";
            json!({
                "id":            r.try_get::<String, _>("id").ok(),
                "object_type":   r.try_get::<String, _>("object_type").ok(),
                "object_id":     r.try_get::<String, _>("object_id").ok(),
                "share_type":    st.clone(),
                "share_target":  tgt.clone(),
                "share_target_display_name": if is_team {
                    team_names.get(&tgt).cloned()
                } else {
                    names.get(&tgt).cloned()
                },
                "permission":    r.try_get::<String, _>("permission").ok(),
                "granted_by":    gb.clone(),
                "granted_by_display_name": names.get(&gb).cloned(),
                "created_at":    ts_string(r, "created_at"),
                "members":       if is_team {
                    json!(rosters.get(&tgt).cloned().unwrap_or_default())
                } else {
                    JsonValue::Null
                },
            })
        })
        .collect()
}

/// Flatten direct + inherited shares into "these people can see it, and
/// here's the reason" — teams expanded to their rosters, strongest
/// reason per person wins, owner first.
async fn effective_viewers(
    pool: &PgPool,
    owner_id: &str,
    direct: &[JsonValue],
    inherited: &[JsonValue],
) -> Vec<JsonValue> {
    let rank = |p: &str| match p {
        "admin" => 3,
        "edit" => 2,
        _ => 1,
    };
    // user_id → (permission, via, via_label)
    let mut best: HashMap<String, (String, String, Option<String>)> = HashMap::new();
    best.insert(owner_id.to_string(), ("admin".into(), "owner".into(), None));

    let consider = |best: &mut HashMap<String, (String, String, Option<String>)>,
                    uid: String,
                    perm: String,
                    via: String,
                    label: Option<String>| {
        match best.get(&uid) {
            Some((existing, _, _)) if rank(existing) >= rank(&perm) => {}
            _ => {
                best.insert(uid, (perm, via, label));
            }
        }
    };

    for group in [direct, inherited] {
        for s in group {
            let perm = s
                .get("permission")
                .and_then(|v| v.as_str())
                .unwrap_or("view")
                .to_string();
            let st = s.get("share_type").and_then(|v| v.as_str()).unwrap_or("");
            let via_portfolio = s
                .get("portfolio_title")
                .and_then(|v| v.as_str())
                .map(String::from);
            let team_label = s
                .get("share_target_display_name")
                .and_then(|v| v.as_str())
                .map(String::from);

            if st == "team" {
                let members = s
                    .get("members")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                // Inherited team shares don't carry an inline roster
                // (they're read off the portfolio), so fetch it.
                let members = if members.is_empty() {
                    let tgt = s
                        .get("share_target")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    team_roster(pool, &tgt).await
                } else {
                    members
                };
                for m in members {
                    let Some(mid) = m.get("member_id").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if m.get("member_type").and_then(|v| v.as_str()) == Some("agent") {
                        continue;
                    }
                    let via = match &via_portfolio {
                        Some(_) => "portfolio_team_share",
                        None => "team_share",
                    };
                    let label = match (&via_portfolio, &team_label) {
                        (Some(p), Some(t)) => Some(format!("{} · via portfolio ‹{}›", t, p)),
                        (None, Some(t)) => Some(t.clone()),
                        (Some(p), None) => Some(format!("via portfolio ‹{}›", p)),
                        _ => None,
                    };
                    consider(&mut best, mid.to_string(), perm.clone(), via.into(), label);
                }
            } else if let Some(tgt) = s.get("share_target").and_then(|v| v.as_str()) {
                let via = match &via_portfolio {
                    Some(_) => "portfolio_user_share",
                    None => "user_share",
                };
                let label = via_portfolio
                    .as_ref()
                    .map(|p| format!("via portfolio ‹{}›", p));
                consider(&mut best, tgt.to_string(), perm.clone(), via.into(), label);
            }
        }
    }

    let ids: Vec<String> = best.keys().cloned().collect();
    let names = resolve_user_names(pool, &ids).await;

    let mut out: Vec<JsonValue> = best
        .into_iter()
        .map(|(uid, (perm, via, label))| {
            json!({
                "user_id":      uid.clone(),
                "display_name": names.get(&uid).cloned(),
                "permission":   perm,
                "via":          via,
                "via_label":    label,
            })
        })
        .collect();

    // Owner first, then strongest permission, then name — stable and
    // scannable rather than hash order.
    out.sort_by(|a, b| {
        let key = |v: &JsonValue| {
            let via = v.get("via").and_then(|x| x.as_str()).unwrap_or("");
            let perm = v
                .get("permission")
                .and_then(|x| x.as_str())
                .unwrap_or("view");
            let name = v
                .get("display_name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_lowercase();
            (if via == "owner" { 0 } else { 1 }, -rank(perm), name)
        };
        key(a).cmp(&key(b))
    });
    out
}

async fn team_roster(pool: &PgPool, team_id: &str) -> Vec<JsonValue> {
    if team_id.is_empty() {
        return Vec::new();
    }
    let rows = sqlx::query(
        "SELECT member_id, member_type, role FROM team_members
         WHERE team_id::text = $1 ORDER BY joined_at",
    )
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.iter()
        .map(|r| {
            json!({
                "member_id":   r.try_get::<String, _>("member_id").ok(),
                "member_type": r.try_get::<String, _>("member_type").ok(),
                "role":        r.try_get::<String, _>("role").ok(),
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Access guards
// ═══════════════════════════════════════════════════════════════════════

/// Returns `(owner_id, visibility)` on success. 404 for a missing row —
/// the caller has no business learning that a private forecast exists.
async fn require_forecast_view(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(String, String), (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".to_string()))?;

    let owner_id: String = row.try_get("owner_id").unwrap_or_default();
    let visibility: String = row.try_get("visibility").unwrap_or_default();

    let granted = can_view(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner_id,
        Visibility::from_legacy(&visibility),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !granted {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }
    Ok((owner_id, visibility))
}

async fn require_portfolio_view(
    pool: &PgPool,
    portfolio_id: &str,
    principal: &AuthPrincipal,
) -> Result<(String, String), (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_portfolios WHERE id = $1",
    )
    .bind(portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Portfolio not found".to_string()))?;

    let owner_id: String = row.try_get("owner_id").unwrap_or_default();
    let visibility: String = row.try_get("visibility").unwrap_or_default();

    let granted = can_view(
        pool,
        principal,
        ObjectType::Portfolio,
        portfolio_id,
        &owner_id,
        Visibility::from_legacy(&visibility),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !granted {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }
    Ok((owner_id, visibility))
}

/// Team feeds are members-only (plus platform admins). A team's activity
/// is exactly the kind of thing that must not be readable by id alone.
pub(crate) async fn require_team_member(
    pool: &PgPool,
    team_id: Uuid,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    if principal.can_admin() {
        return Ok(());
    }
    let role = teams::get_member_role(pool, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if role.is_none() {
        return Err((StatusCode::FORBIDDEN, "Not a team member".to_string()));
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════
// Tests — the DB-free half
// ══════════════════════════════════════════════════════════════════
//
// The SQL half is covered by `scripts/spec26_sql_check.sh`, which stands
// up a throwaway Postgres cluster — the queries here are runtime strings,
// so a planner is the only thing that can actually check them.
//
// What's left is pure and worth pinning: the event summariser (its output
// is the user-visible sentence on three surfaces, so a regression is
// immediately felt), the attribution classifier (getting `system` wrong
// means silently blaming the owner for unattributed history), and the
// query-param parsing.

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: &str) -> RawEvent {
        RawEvent {
            ts: None,
            kind: kind.to_string(),
            actor: None,
            agent_id: None,
            object_type: "forecast".into(),
            object_id: "f1".into(),
            object_title: None,
            prev: None,
            newp: None,
            reason: None,
            revision_trigger: None,
            outcome: None,
            brier: None,
            ref_type: None,
            ref_id: None,
            ref_label: None,
            permission: None,
        }
    }

    #[test]
    fn revision_summary_shows_both_endpoints() {
        let mut e = ev("revised");
        e.prev = Some(0.41);
        e.newp = Some(0.47);
        e.revision_trigger = Some("manual".into());
        assert_eq!(e.summary(), "revised 41% → 47%");
    }

    /// An agent-assisted revision must name BOTH the trigger and the
    /// agent. Losing either half is the exact failure Spec 26 §4.1 exists
    /// to fix.
    #[test]
    fn revision_summary_names_agent_and_trigger() {
        let mut e = ev("revised");
        e.prev = Some(0.10);
        e.newp = Some(0.25);
        e.revision_trigger = Some("bayesops_refit".into());
        e.agent_id = Some("elo-scout".into());
        let s = e.summary();
        assert!(s.contains("bayesops_refit"), "{}", s);
        assert!(s.contains("elo-scout"), "{}", s);
    }

    /// A manual revision that used an agent still credits the agent, but
    /// without the redundant "manual" label.
    #[test]
    fn manual_revision_with_agent_says_via() {
        let mut e = ev("revised");
        e.prev = Some(0.5);
        e.newp = Some(0.6);
        e.revision_trigger = Some("manual".into());
        e.agent_id = Some("scout".into());
        assert_eq!(e.summary(), "revised 50% → 60% (via scout)");
    }

    /// Missing endpoints render as `?` rather than a fabricated 0%, which
    /// would read as a real forecast the team never made.
    #[test]
    fn missing_probabilities_render_as_unknown() {
        let mut e = ev("revised");
        e.newp = Some(0.3);
        assert_eq!(e.summary(), "revised ? → 30%");
    }

    #[test]
    fn resolution_summary_carries_outcome_and_brier() {
        let mut e = ev("resolved");
        e.outcome = Some(true);
        e.brier = Some(0.0841);
        assert_eq!(e.summary(), "resolved YES · Brier 0.084");

        let mut no_brier = ev("resolved");
        no_brier.outcome = Some(false);
        assert_eq!(no_brier.summary(), "resolved NO");
    }

    #[test]
    fn share_summary_distinguishes_team_from_user() {
        let mut team = ev("shared");
        team.ref_type = Some("team".into());
        team.ref_label = Some("WC analysts".into());
        team.permission = Some("edit".into());
        assert_eq!(team.summary(), "shared with team WC analysts (edit)");

        let mut user = ev("shared");
        user.ref_type = Some("user".into());
        user.ref_id = Some("bob".into());
        assert_eq!(user.summary(), "shared with user bob (view)");
    }

    #[test]
    fn curation_summary_names_the_portfolio() {
        let mut e = ev("portfolio_add");
        e.ref_label = Some("WC 2026".into());
        assert_eq!(e.summary(), "added to portfolio ‹WC 2026›");
    }

    /// Unattributed rows (pre-migration-176 history, cron writers) must
    /// classify as `system` with NO display name. Falling back to the
    /// owner here would produce a UI that cannot distinguish a guess from
    /// a fact — the whole reason migration 176 has no backfill.
    #[test]
    fn unattributed_event_is_system_not_owner() {
        let e = ev("revised");
        let json = e.to_json(&HashMap::new());
        assert_eq!(json["actor_kind"], "system");
        assert!(json["actor_display_name"].is_null());
        assert!(json["actor_id"].is_null());
    }

    /// An event with an agent but no human is an agent action, not a
    /// system one — scheduled agent runs should read as the agent.
    #[test]
    fn agent_only_event_is_attributed_to_the_agent() {
        let mut e = ev("revised");
        e.agent_id = Some("elo-scout".into());
        let json = e.to_json(&HashMap::new());
        assert_eq!(json["actor_kind"], "agent");
        assert_eq!(json["actor_display_name"], "elo-scout");
    }

    /// A known human resolves to their display name; an unknown one falls
    /// back to the raw id rather than vanishing.
    #[test]
    fn human_actor_resolves_name_with_id_fallback() {
        let mut e = ev("revised");
        e.actor = Some("u-alice".into());
        let mut names = HashMap::new();
        names.insert("u-alice".to_string(), "Alice".to_string());
        assert_eq!(e.to_json(&names)["actor_display_name"], "Alice");
        assert_eq!(e.to_json(&HashMap::new())["actor_display_name"], "u-alice");
    }

    /// An empty-string actor is drift, not an identity. Treating it as one
    /// would render a nameless row that looks clickable but filters to
    /// nothing.
    #[test]
    fn empty_actor_string_is_not_an_identity() {
        let mut e = ev("revised");
        e.actor = Some(String::new());
        assert_eq!(e.to_json(&HashMap::new())["actor_kind"], "system");
    }

    #[test]
    fn activity_limit_is_clamped_to_sane_bounds() {
        let dflt = ActivityQuery::default();
        assert_eq!(dflt.limit_clamped(), 60);

        let huge = ActivityQuery {
            limit: Some(100_000),
            ..Default::default()
        };
        assert_eq!(huge.limit_clamped(), 200);

        // Zero and negatives would produce an empty or panicking
        // truncate; clamp to 1 so the caller always gets a valid feed.
        let zero = ActivityQuery {
            limit: Some(0),
            ..Default::default()
        };
        assert_eq!(zero.limit_clamped(), 1);
        let neg = ActivityQuery {
            limit: Some(-5),
            ..Default::default()
        };
        assert_eq!(neg.limit_clamped(), 1);
    }

    #[test]
    fn kind_filter_parses_and_trims() {
        let q = ActivityQuery {
            kind: Some(" revised , resolved ,, ".into()),
            ..Default::default()
        };
        let kinds = q.kinds().expect("filter present");
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains("revised"));
        assert!(kinds.contains("resolved"));

        assert!(ActivityQuery::default().kinds().is_none());
    }

    /// The `access_via` vocabulary is a contract with the console, which
    /// switches on these exact strings to pick a badge and a sentence. A
    /// silent rename would degrade every row to the "unknown" glyph.
    #[test]
    fn provenance_serialises_the_agreed_vocabulary() {
        let p = AccessProvenance {
            access_via: "team_share".into(),
            permission: "edit".into(),
            shared_by: Some("u-bo".into()),
            shared_by_display_name: Some("Bo".into()),
            team_name: Some("WC analysts".into()),
            share_count: 3,
            ..Default::default()
        };
        let j = p.to_json();
        assert_eq!(j["access_via"], "team_share");
        assert_eq!(j["permission"], "edit");
        assert_eq!(j["shared_by_display_name"], "Bo");
        assert_eq!(j["team_name"], "WC analysts");
        assert_eq!(j["share_count"], 3);
    }

    /// Default provenance must not claim ownership. `AccessProvenance` is
    /// `Default`-constructed on the failure path of the access handlers,
    /// and a default that read as `owner` would silently overstate the
    /// caller's rights in the UI.
    #[test]
    fn default_provenance_claims_nothing() {
        let j = AccessProvenance::default().to_json();
        assert_eq!(j["access_via"], "");
        assert_eq!(j["permission"], "");
        assert_eq!(j["share_count"], 0);
    }
}
