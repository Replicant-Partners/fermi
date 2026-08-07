//! Driver annotations — objections anchored to a specific assumption.
//!
//! Spec 32. See `docs/specs/SPEC_32_DRIVER_ANNOTATIONS.md`.
//!
//! ## What this is for
//!
//! The operator's model is that teams "coordinate on trajectories and
//! research and **assumptions**". Provenance, history and the ops board
//! cover the first two. This covers the third — the single most common
//! thing one forecaster says to another:
//!
//! > *"your base rate for `elo_current` is wrong, here's why"*
//!
//! Before this it happened in Slack, or as a probability revision with a
//! `reason` string, or not at all — none of which attach to the thing being
//! disputed, so the objection is invisible to whoever opens the forecast
//! next. Which is exactly when it matters.
//!
//! ## Anchored to the driver, not the forecast
//!
//! A forecast-level comment thread would have been easier and much less
//! useful. Disagreement here is almost never about the question; it's about
//! one input. Anchoring at `(forecast, driver)` means the objection renders
//! next to the number it disputes, survives revisions of other drivers, and
//! makes "which assumptions are contested" a query — which the ops board
//! turns into coordination work.
//!
//! ## Permissions
//!
//! **Anyone who can view may annotate.** This is deliberate and is the one
//! place the moderate permission model bends toward the wiki. A `view`
//! grant exists so people can *read and react to* a forecast; telling a
//! reader "you may see this but not say it's wrong" would defeat the point
//! of publishing it. Annotating changes no forecast state — it is the
//! cheapest possible reversible act.
//!
//! **Resolving requires `edit`**, because accepting a challenge is a claim
//! about what the forecast now says, and declining one closes someone
//! else's objection.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::visibility::{can_edit, can_view};
use fermi_auth::{AuthPrincipal, ObjectType, Visibility};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use uuid::Uuid;

use crate::handlers::collab::resolve_user_names;
use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════
// Requests
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct CreateAnnotationRequest {
    /// The disputed driver. `None` annotates the forecast as a whole —
    /// allowed, because "the whole framing is wrong" is a real thing to say
    /// and forcing it onto an arbitrary driver would misfile it.
    pub driver_name: Option<String>,
    pub body: String,
    /// `challenge` (default) | `question` | `note`
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveAnnotationRequest {
    /// `accepted` — acted on, the driver changed.
    /// `declined` — considered and rejected.
    pub status: String,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListAnnotationsQuery {
    /// `open` restricts to unresolved. Default returns everything, because
    /// a resolved challenge is context the next reader wants.
    pub status: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/forecasts/:id/annotations
// ═══════════════════════════════════════════════════════════════════════

/// Every annotation on a forecast, grouped by driver for rendering.
///
/// View-gated, matching Spec 31's history: if you can read the forecast you
/// can read what people have said about it. An objection visible only to
/// editors would leave readers trusting a number the team is actively
/// disputing.
pub async fn list_annotations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Query(q): Query<ListAnnotationsQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_view(pool, &forecast_id, &principal).await?;

    let open_only = q.status.as_deref() == Some("open");
    let rows = sqlx::query(
        "SELECT id::text AS id, driver_name, author_id, body, kind, status,
                resolved_by, resolved_at, resolution_note, at_commit,
                created_at, updated_at
           FROM driver_annotations
          WHERE forecast_id = $1
            AND ($2 = false OR status = 'open')
          ORDER BY (status = 'open') DESC, created_at DESC",
    )
    .bind(&forecast_id)
    .bind(open_only)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // One batch name lookup for authors and resolvers together.
    let mut ids: Vec<String> = Vec::new();
    for r in &rows {
        if let Ok(a) = r.try_get::<String, _>("author_id") {
            ids.push(a);
        }
        if let Ok(Some(rb)) = r.try_get::<Option<String>, _>("resolved_by") {
            ids.push(rb);
        }
    }
    let names = resolve_user_names(pool, &ids).await;

    let annotations: Vec<JsonValue> = rows
        .iter()
        .map(|r| {
            let author: String = r.try_get("author_id").unwrap_or_default();
            let resolver: Option<String> =
                r.try_get::<Option<String>, _>("resolved_by").ok().flatten();
            json!({
                "id":              r.try_get::<String, _>("id").ok(),
                "driver_name":     r.try_get::<Option<String>, _>("driver_name").ok().flatten(),
                "author_id":       author.clone(),
                "author_display_name": names.get(&author).cloned(),
                "body":            r.try_get::<String, _>("body").ok(),
                "kind":            r.try_get::<String, _>("kind").ok(),
                "status":          r.try_get::<String, _>("status").ok(),
                "resolved_by":     resolver.clone(),
                "resolved_by_display_name": resolver.as_ref().and_then(|x| names.get(x).cloned()),
                "resolved_at":     ts(r, "resolved_at"),
                "resolution_note": r.try_get::<Option<String>, _>("resolution_note").ok().flatten(),
                "at_commit":       r.try_get::<Option<String>, _>("at_commit").ok().flatten(),
                "created_at":      ts(r, "created_at"),
            })
        })
        .collect();

    // Per-driver open counts, so the composer can badge a driver as
    // contested without the client re-deriving it (and getting it subtly
    // different from the ops-board detector).
    let mut open_by_driver: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for a in &annotations {
        if a.get("status").and_then(|s| s.as_str()) == Some("open") {
            let key = a
                .get("driver_name")
                .and_then(|d| d.as_str())
                .unwrap_or("__forecast__")
                .to_string();
            *open_by_driver.entry(key).or_insert(0) += 1;
        }
    }

    Ok(Json(json!({
        "forecast_id":    forecast_id,
        "annotations":    annotations,
        "count":          annotations.len(),
        "open_by_driver": open_by_driver,
        // Who is asking. Delete is author-only, and without this a client
        // has no way to know which rows are its own — it would have to
        // offer Delete on every row and let the server 403 the ones that
        // aren't, which teaches the operator that buttons lie.
        "me":             principal.user_id(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// POST /api/forecasts/:id/annotations
// ═══════════════════════════════════════════════════════════════════════

/// Raise an objection, question or note against a driver.
///
/// **View access is enough.** A `view` grant exists so people can read and
/// react; "you may see this but not say it's wrong" would defeat the point
/// of publishing. Annotating mutates no forecast state — it is the cheapest
/// reversible act in the product, and the wiki argument (reversibility
/// beats prevention) applies at its strongest here.
pub async fn create_annotation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Json(body): Json<CreateAnnotationRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    let pool = &state.db;
    require_view(pool, &forecast_id, &principal).await?;

    let text = body.body.trim();
    if text.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "body is required".into()));
    }
    if text.chars().count() > 4000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "body is too long (4000 characters max)".into(),
        ));
    }

    let kind = match body.kind.as_deref() {
        Some(k @ ("challenge" | "question" | "note")) => k,
        None => "challenge",
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown kind '{}': expected challenge|question|note", other),
            ))
        }
    };

    // Pin the annotation to the revision it was written against, so the UI
    // can say "raised when this read 1780" once the value has moved. Without
    // it a months-old objection reads as though it were about today's
    // number. Best-effort: an unversioned forecast simply has none.
    let at_commit: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT t.git_latest_commit FROM fermi_forecasts f
           JOIN teams t ON t.id = f.workspace_id WHERE f.id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
    .filter(|s| !s.is_empty());

    let user_id = principal.user_id();
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO driver_annotations
            (id, forecast_id, driver_name, author_id, body, kind, at_commit)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&forecast_id)
    .bind(
        body.driver_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(&user_id)
    .bind(text)
    .bind(kind)
    .bind(&at_commit)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Tell the forecast owner. A challenge nobody sees is the same as no
    // challenge — and the person best placed to answer is precisely the one
    // not looking at the screen.
    notify_owner(
        &state,
        &forecast_id,
        &user_id,
        kind,
        body.driver_name.as_deref(),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id.to_string(), "status": "open" })),
    ))
}

// ═══════════════════════════════════════════════════════════════════════
// POST /api/forecasts/:id/annotations/:annotation_id/resolve
// ═══════════════════════════════════════════════════════════════════════

/// Close an annotation as accepted or declined.
///
/// Requires `edit`: accepting a challenge is a claim about what the forecast
/// now says, and declining closes someone else's objection. Neither is a
/// reader's call.
///
/// Resolutions are recorded, never deleted. The difference between
/// *accepted* and *declined* is exactly the reasoning a team wants to
/// re-read, and a resolved challenge is context for the next person — not
/// clutter.
pub async fn resolve_annotation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((forecast_id, annotation_id)): Path<(String, Uuid)>,
    Json(body): Json<ResolveAnnotationRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_edit(pool, &forecast_id, &principal).await?;

    let status = match body.status.as_str() {
        s @ ("accepted" | "declined") => s,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown status '{}': expected accepted|declined", other),
            ))
        }
    };

    let user_id = principal.user_id();
    let result = sqlx::query(
        "UPDATE driver_annotations
            SET status = $3, resolved_by = $4, resolved_at = NOW(),
                resolution_note = $5, updated_at = NOW()
          WHERE id = $1 AND forecast_id = $2 AND status = 'open'",
    )
    .bind(annotation_id)
    .bind(&forecast_id)
    .bind(status)
    .bind(&user_id)
    .bind(&body.note)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        // The forecast_id is in the WHERE clause so a crafted annotation id
        // can't resolve someone else's annotation from this route.
        return Err((
            StatusCode::NOT_FOUND,
            "Annotation not found on this forecast, or already resolved".into(),
        ));
    }

    Ok(Json(json!({ "id": annotation_id, "status": status })))
}

// ═══════════════════════════════════════════════════════════════════════
// DELETE /api/forecasts/:id/annotations/:annotation_id
// ═══════════════════════════════════════════════════════════════════════

/// Hard-delete — **author only**, for genuine mistakes.
///
/// Not available to editors: letting someone delete an objection raised
/// against their own work is the one way this feature could be used to hide
/// disagreement rather than surface it. Editors get `declined`, which is on
/// the record.
pub async fn delete_annotation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((forecast_id, annotation_id)): Path<(String, Uuid)>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_view(pool, &forecast_id, &principal).await?;

    let result = sqlx::query(
        "DELETE FROM driver_annotations
          WHERE id = $1 AND forecast_id = $2 AND author_id = $3",
    )
    .bind(annotation_id)
    .bind(&forecast_id)
    .bind(principal.user_id())
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the author can delete an annotation. Resolve it as declined instead — \
             that keeps the objection and your reasoning on the record."
                .into(),
        ));
    }
    Ok(Json(json!({ "status": "deleted" })))
}

// ═══════════════════════════════════════════════════════════════════════
// Orphan sweep
// ═══════════════════════════════════════════════════════════════════════

/// The set of driver names a forecast currently declares.
///
/// **Drivers live in the FPL program, not in a column.** `fermi_forecasts`
/// has a `drivers` JSONB field, but nothing populates it — every row in
/// production holds `[]`. The authoritative declaration is `driver <name>`
/// in `fpl_source`, which is what the executor, the LSP and BayesOps all
/// read (`bayesops.driver_name` is keyed the same way). Anchoring
/// annotations anywhere else would attach them to a phantom.
///
/// Returns `None` when the name set can't be established — no source, or
/// source that doesn't parse. That is *not* the same as "no drivers", and
/// callers must not treat it as such; see `mark_orphaned_annotations`.
async fn declared_driver_names(pool: &PgPool, forecast_id: &str) -> Option<HashSet<String>> {
    // Two levels of Option: no such forecast, and a forecast with no
    // program yet. Both mean "can't establish the name set", which is not
    // "there are no drivers".
    let src: String = sqlx::query_scalar::<_, Option<String>>(
        "SELECT fpl_source FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()?;

    driver_names_in(&src)
}

/// The parsing half of [`declared_driver_names`], separated so it can be
/// tested without a database.
///
/// `None` on unparseable source is load-bearing, not laziness — see the
/// fail-safe note on [`mark_orphaned_annotations`].
fn driver_names_in(src: &str) -> Option<HashSet<String>> {
    let tokens = fermi::Lexer::new(src).tokenize().ok()?;
    let program = fermi::Parser::new(tokens).parse().ok()?;
    Some(program.drivers().iter().map(|d| d.name.clone()).collect())
}

/// Reconcile annotation status against the drivers the program actually
/// declares — orphaning those whose driver has gone, and un-orphaning those
/// whose driver has come back.
///
/// `driver_name` has no foreign key: drivers are FPL declarations, not rows,
/// so a rename or deletion strands its annotations. Normalising drivers into
/// a table would be a far larger change than this feature justifies, so we
/// detect instead. An open challenge pointing at a driver that no longer
/// exists is worse than noise — it reads as live disagreement about
/// something that isn't there.
///
/// Two properties this deliberately has:
///
/// * **Fail-safe.** If the source is missing or doesn't parse, nothing is
///   touched. The composer autosaves mid-keystroke, so a half-written
///   program is a routine state, and mass-orphaning every annotation on a
///   transient syntax error would be destructive.
/// * **Reversible.** Orphaning is a derived observation about the current
///   program, not a decision, so it is undone when the program is. A Spec 31
///   revert that restores a deleted driver restores its objections with it —
///   consistent with the rest of the collaboration model, where undo is what
///   makes shared write safe. Only `orphaned` rows are revived; a human's
///   `accepted`/`declined` is a judgement and stays put.
///
/// Best-effort and idempotent: safe to call on every update.
pub async fn mark_orphaned_annotations(pool: &PgPool, forecast_id: &str) {
    let Some(names) = declared_driver_names(pool, forecast_id).await else {
        return;
    };
    let names: Vec<String> = names.into_iter().collect();

    let _ = sqlx::query(
        "UPDATE driver_annotations
            SET status = 'orphaned', updated_at = NOW()
          WHERE forecast_id = $1
            AND status = 'open'
            AND driver_name IS NOT NULL
            AND NOT (driver_name = ANY($2))",
    )
    .bind(forecast_id)
    .bind(&names)
    .execute(pool)
    .await;

    let _ = sqlx::query(
        "UPDATE driver_annotations
            SET status = 'open', updated_at = NOW()
          WHERE forecast_id = $1
            AND status = 'orphaned'
            AND driver_name = ANY($2)",
    )
    .bind(forecast_id)
    .bind(&names)
    .execute(pool)
    .await;
}

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

fn ts(row: &sqlx::postgres::PgRow, col: &str) -> Option<String> {
    row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(col)
        .ok()
        .flatten()
        .map(|t| t.to_rfc3339())
}

async fn notify_owner(
    state: &AppState,
    forecast_id: &str,
    actor_id: &str,
    kind: &str,
    driver: Option<&str>,
) {
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, question_text
           FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let Some(row) = row else { return };
    let owner: String = row.try_get("owner_id").unwrap_or_default();
    // Don't notify yourself about your own note.
    if owner.is_empty() || owner == actor_id {
        return;
    }

    let question: String = row.try_get("question_text").unwrap_or_default();
    let names = resolve_user_names(&state.db, &[actor_id.to_string()]).await;
    let who = names
        .get(actor_id)
        .cloned()
        .unwrap_or_else(|| actor_id.to_string());

    let what = match (kind, driver) {
        ("challenge", Some(d)) => format!("{} challenged “{}”", who, d),
        ("question", Some(d)) => format!("{} asked about “{}”", who, d),
        ("challenge", None) => format!("{} raised a challenge", who),
        ("question", None) => format!("{} asked a question", who),
        (_, Some(d)) => format!("{} noted something on “{}”", who, d),
        _ => format!("{} left a note", who),
    };

    crate::create_notification(
        &state.db,
        &owner,
        "driver_annotation",
        &what,
        Some(&question.chars().take(120).collect::<String>()),
    )
    .await;
}

async fn require_view(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let (owner, vis) = acl_row(pool, forecast_id).await?;
    let ok = can_view(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner,
        Visibility::from_legacy(&vis),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !ok {
        return Err((StatusCode::FORBIDDEN, "Access denied".into()));
    }
    Ok(())
}

async fn require_edit(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let (owner, vis) = acl_row(pool, forecast_id).await?;
    let ok = can_edit(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner,
        Visibility::from_legacy(&vis),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !ok {
        return Err((
            StatusCode::FORBIDDEN,
            "Resolving an annotation needs edit access — accepting a challenge is a \
             claim about what this forecast now says."
                .into(),
        ));
    }
    Ok(())
}

async fn acl_row(
    pool: &PgPool,
    forecast_id: &str,
) -> Result<(String, String), (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".to_string()))?;

    Ok((
        row.try_get("owner_id").unwrap_or_default(),
        row.try_get("visibility").unwrap_or_default(),
    ))
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::driver_names_in;

    /// A real production program, copied verbatim from `fpl_source`.
    ///
    /// It is here because the first cut of this feature anchored
    /// annotations to `fermi_forecasts.drivers` (JSONB), which looks like
    /// the obvious home for drivers and is in fact empty on every row in
    /// production. Drivers are a language construct. This fixture is the
    /// standing evidence for that, in the shape the parser actually sees.
    const REAL_PROGRAM: &str = r#"
question "Will Vinicius Junior join Arsenal?"

driver strength_factor continuous {
    distribution: triangular(0.5, 1, 1.5)
    unit: "multiplier"
    rationale: "How strong is the case for this outcome? 1.0 = neutral"
}

driver conditions continuous {
    distribution: triangular(0.7, 1, 1.3)
    unit: "multiplier"
    rationale: "Are conditions favorable (>1) or unfavorable (<1)?"
}

driver disruption binary {
    probability: 0.15p
    impact_multiplier: 1.5
    rationale: "Probability of a disruptive event that amplifies the outcome"
}

model: ((strength_factor * conditions) * (if disruption then 1.5 else 1))

simulate 10000 iterations
"#;

    #[test]
    fn extracts_every_declared_driver_from_a_real_program() {
        let names = driver_names_in(REAL_PROGRAM).expect("real program must parse");
        let mut got: Vec<_> = names.iter().map(String::as_str).collect();
        got.sort_unstable();
        assert_eq!(got, ["conditions", "disruption", "strength_factor"]);
    }

    /// The whole reason the sweep is fail-safe. The composer autosaves
    /// mid-keystroke, so half-written programs are a routine state — not an
    /// exceptional one. If this returned `Some(empty)` instead of `None`,
    /// every annotation on the forecast would be orphaned the moment
    /// someone opened an unclosed brace.
    #[test]
    fn unparseable_source_is_unknown_not_empty() {
        assert!(driver_names_in("driver half_written continuous {").is_none());
        assert!(driver_names_in("!!! not fpl at all").is_none());
    }

    /// Distinct from the above: a program that genuinely declares nothing
    /// IS knowledge, and must orphan any annotation pointing at a driver.
    #[test]
    fn valid_program_with_no_drivers_is_a_known_empty_set() {
        let names = driver_names_in("question \"anything?\"\nmodel: 1\n")
            .expect("a driverless program is still a parseable program");
        assert!(names.is_empty());
    }
}
