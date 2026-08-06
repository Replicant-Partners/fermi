//! Operator-gated cascade queue.
//!
//! When a forecast resolves (manually via /api/forecasts/:id/resolve OR
//! via an upstream workspace resolution that propagates here), we don't
//! auto-fire the relationship cascade. Instead, a `pending_cascades` row
//! is queued for each non-archived group the resolved forecast
//! is a member of. The operator reviews the queue and applies/dismisses
//! each entry.
//!
//! Operator-gate rule: every parameter mutation passes through a human.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::handlers::relationships::propagation::{
    dispatch_propagation, dispatch_propagation_group, get_group_member_ids, PropagateRequest,
};
use crate::AppState;

pub async fn queue_pending_cascade(
    pool: &PgPool,
    trigger_forecast_id: &str,
    trigger_kind: &str,
    outcome: Option<bool>,
    source: &str,
    owner_id: &str,
) {
    // Read the trigger's group tags. On any failure (e.g. a DB still
    // missing the relationship_groups column on an un-migrated deploy, or
    // a transient error) we DON'T abort — we fall through to the legacy
    // forecast_relationships path so a resolution still queues a cascade.
    // Silently returning here is what left resolved teams stranded with
    // no redistribution.
    let group_ids: Vec<String> = match sqlx::query(
        "SELECT unnest(relationship_groups) AS group_id
          FROM public.fermi_forecasts
          WHERE id = $1
            AND relationship_groups <> '{}'",
    )
    .bind(trigger_forecast_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>("group_id").ok())
            .collect(),
        Err(e) => {
            tracing::warn!(
                forecast = %trigger_forecast_id,
                error = %e,
                "[cascade-queue] failed to fetch relationship_groups; \
                 falling through to legacy relationships"
            );
            Vec::new()
        }
    };

    if group_ids.is_empty() {
        queue_pending_cascade_legacy(
            pool,
            trigger_forecast_id,
            trigger_kind,
            outcome,
            source,
            owner_id,
        )
        .await;
        return;
    }

    for group_id in &group_ids {
        let group = match sqlx::query(
            "SELECT kind, parameters FROM public.forecast_relationship_groups
              WHERE group_id = $1 AND archived_at IS NULL",
        )
        .bind(group_id)
        .fetch_optional(pool)
        .await
        {
            Ok(Some(r)) => r,
            _ => continue,
        };

        let kind: String = group.try_get("kind").unwrap_or_default();
        let parameters: JsonValue = group.try_get("parameters").unwrap_or(JsonValue::Null);

        let already: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM public.pending_cascades
              WHERE group_id = $1
                AND trigger_forecast_id = $2
                AND status = 'pending'",
        )
        .bind(group_id)
        .bind(trigger_forecast_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        if already.is_some() {
            continue;
        }

        let dry_req = PropagateRequest {
            trigger_forecast_id: trigger_forecast_id.to_string(),
            trigger_kind: trigger_kind.to_string(),
            outcome,
        };
        let snapshot =
            match dispatch_propagation_group(&kind, group_id, &parameters, &dry_req, pool, true)
                .await
            {
                Ok(result) => json!({
                    "n_projected": result.n_updated,
                    "deltas": result.deltas.iter().map(|d| json!({
                        "forecast_id": d.forecast_id,
                        "previous_probability": d.previous_probability,
                        "new_probability": d.new_probability,
                        "delta_pp": d.delta_pp,
                    })).collect::<Vec<_>>(),
                    "note": result.note,
                }),
                Err((_, e)) => {
                    tracing::warn!(
                        group = %group_id,
                        forecast = %trigger_forecast_id,
                        error = %e,
                        "[cascade-queue] dry-run failed"
                    );
                    continue;
                }
            };

        if let Err(e) = sqlx::query(
            "INSERT INTO public.pending_cascades (
                  group_id, trigger_forecast_id, trigger_kind,
                  outcome, source, status, owner_id, proposed_snapshot
              )
              VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)",
        )
        .bind(group_id)
        .bind(trigger_forecast_id)
        .bind(trigger_kind)
        .bind(outcome)
        .bind(source)
        .bind(owner_id)
        .bind(&snapshot)
        .execute(pool)
        .await
        {
            tracing::warn!(
                group = %group_id,
                forecast = %trigger_forecast_id,
                error = %e,
                "[cascade-queue] insert failed"
            );
        } else {
            tracing::info!(
                group = %group_id,
                forecast = %trigger_forecast_id,
                kind = %trigger_kind,
                source = %source,
                "[cascade-queue] queued cascade"
            );
        }
    }
}

async fn queue_pending_cascade_legacy(
    pool: &PgPool,
    trigger_forecast_id: &str,
    trigger_kind: &str,
    outcome: Option<bool>,
    source: &str,
    owner_id: &str,
) {
    let rels = match sqlx::query(
        "SELECT id, kind, forecast_ids, parameters
          FROM public.forecast_relationships
          WHERE archived_at IS NULL
            AND $1 = ANY(forecast_ids)",
    )
    .bind(trigger_forecast_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                forecast = %trigger_forecast_id,
                error = %e,
                "[cascade-queue-legacy] failed to fetch relationships"
            );
            return;
        }
    };

    if rels.is_empty() {
        return;
    }

    for rel in &rels {
        let rel_id: Uuid = match rel.try_get("id") {
            Ok(u) => u,
            Err(_) => continue,
        };
        let kind: String = rel.try_get("kind").unwrap_or_default();
        let forecast_ids: Vec<String> = rel.try_get("forecast_ids").unwrap_or_default();
        let parameters: JsonValue = rel.try_get("parameters").unwrap_or(JsonValue::Null);

        let already: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM public.pending_cascades
              WHERE relationship_id = $1
                AND trigger_forecast_id = $2
                AND status = 'pending'",
        )
        .bind(rel_id)
        .bind(trigger_forecast_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        if already.is_some() {
            continue;
        }

        let dry_req = PropagateRequest {
            trigger_forecast_id: trigger_forecast_id.to_string(),
            trigger_kind: trigger_kind.to_string(),
            outcome,
        };
        let snapshot =
            match dispatch_propagation(&kind, &forecast_ids, &parameters, &dry_req, pool, true)
                .await
            {
                Ok(result) => json!({
                    "n_projected": result.n_updated,
                    "deltas": result.deltas.iter().map(|d| json!({
                        "forecast_id": d.forecast_id,
                        "previous_probability": d.previous_probability,
                        "new_probability": d.new_probability,
                        "delta_pp": d.delta_pp,
                    })).collect::<Vec<_>>(),
                    "note": result.note,
                }),
                Err((_, e)) => {
                    tracing::warn!(
                        rel = %rel_id,
                        forecast = %trigger_forecast_id,
                        error = %e,
                        "[cascade-queue-legacy] dry-run failed"
                    );
                    continue;
                }
            };

        if let Err(e) = sqlx::query(
            "INSERT INTO public.pending_cascades (
                  relationship_id, trigger_forecast_id, trigger_kind,
                  outcome, source, status, owner_id, proposed_snapshot
              )
              VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)",
        )
        .bind(rel_id)
        .bind(trigger_forecast_id)
        .bind(trigger_kind)
        .bind(outcome)
        .bind(source)
        .bind(owner_id)
        .bind(&snapshot)
        .execute(pool)
        .await
        {
            tracing::warn!(
                rel = %rel_id,
                forecast = %trigger_forecast_id,
                error = %e,
                "[cascade-queue-legacy] insert failed"
            );
        } else {
            tracing::info!(
                rel = %rel_id,
                forecast = %trigger_forecast_id,
                kind = %trigger_kind,
                source = %source,
                "[cascade-queue-legacy] queued cascade"
            );
        }
    }
}

pub async fn list_pending_cascades_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let status = params
        .get("status")
        .cloned()
        .unwrap_or_else(|| "pending".to_string());

    // Spec 27: the queue follows FORECAST ACCESS, not sole ownership.
    //
    // `pending_cascades.owner_id` used to gate this list, which made the
    // single most coordination-hungry object in the product private to one
    // person: a team could share a portfolio, jointly manage the forecasts
    // in it, and still have exactly one member able to see — or act on —
    // the cascades their resolutions queued. Everyone else saw an empty
    // queue while coherence silently rotted.
    //
    // `owner_id` is retained as attribution (who triggered it), which is
    // what it should always have been. Visibility is now the same
    // predicate `can_access` enforces, so if you can see the trigger
    // forecast you can see that resolving it queued work.
    if status == "all" {
        let rows = sqlx::query(&format!(
            "SELECT pc.id, pc.group_id, pc.relationship_id, pc.trigger_forecast_id,
                    pc.trigger_kind, pc.outcome, pc.source, pc.status,
                    pc.created_at, pc.decided_at, pc.decided_by, pc.notes,
                    pc.proposed_snapshot, pc.applied_deltas,
                    COALESCE(frg.kind, fr.kind) AS relationship_kind,
                    COALESCE(frg.description, fr.description) AS relationship_description,
                    ff.question_text AS trigger_question_text,
                    ff.predicted_probability AS trigger_probability
              FROM public.pending_cascades pc
              LEFT JOIN public.forecast_relationship_groups frg ON frg.group_id = pc.group_id
              LEFT JOIN public.forecast_relationships fr ON fr.id = pc.relationship_id
              JOIN public.fermi_forecasts ff ON ff.id = pc.trigger_forecast_id
              WHERE {access}
              ORDER BY pc.created_at DESC
              LIMIT 200",
            access = fermi_auth::visibility::forecast_view_predicate("ff", 1)
        ))
        .bind(&user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let entries: Vec<JsonValue> = rows.iter().map(|r| cascade_row_to_json(r)).collect();

        return Ok(Json(json!({
            "pending": entries,
            "count": entries.len(),
            "status": "all",
        })));
    }

    let rows = sqlx::query(&format!(
        "SELECT pc.id, pc.group_id, pc.relationship_id, pc.trigger_forecast_id,
                pc.trigger_kind, pc.outcome, pc.source, pc.status,
                pc.created_at, pc.decided_at, pc.decided_by, pc.notes,
                pc.proposed_snapshot, pc.applied_deltas,
                COALESCE(frg.kind, fr.kind) AS relationship_kind,
                COALESCE(frg.description, fr.description) AS relationship_description,
                ff.question_text AS trigger_question_text,
                ff.predicted_probability AS trigger_probability
          FROM public.pending_cascades pc
          LEFT JOIN public.forecast_relationship_groups frg ON frg.group_id = pc.group_id
          LEFT JOIN public.forecast_relationships fr ON fr.id = pc.relationship_id
          JOIN public.fermi_forecasts ff ON ff.id = pc.trigger_forecast_id
          WHERE {access} AND pc.status = $2
          ORDER BY pc.created_at DESC
          LIMIT 100",
        access = fermi_auth::visibility::forecast_view_predicate("ff", 1)
    ))
    .bind(&user_id)
    .bind(&status)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let entries: Vec<JsonValue> = rows.iter().map(|r| cascade_row_to_json(r)).collect();

    Ok(Json(json!({
        "pending": entries,
        "count": entries.len(),
        "status": status,
    })))
}

fn cascade_row_to_json(r: &sqlx::postgres::PgRow) -> JsonValue {
    json!({
        "id": r.try_get::<Uuid, _>("id").ok().map(|u| u.to_string()),
        "group_id": r.try_get::<Option<String>, _>("group_id").ok().flatten(),
        "relationship_id": r.try_get::<Option<Uuid>, _>("relationship_id").ok().flatten().map(|u| u.to_string()),
        "relationship_kind": r.try_get::<String, _>("relationship_kind").ok(),
        "relationship_description": r.try_get::<Option<String>, _>("relationship_description").ok().flatten(),
        "trigger_forecast_id": r.try_get::<String, _>("trigger_forecast_id").ok(),
        "trigger_question_text": r.try_get::<String, _>("trigger_question_text").ok(),
        "trigger_probability": r
            .try_get::<Option<f32>, _>("trigger_probability")
            .ok()
            .flatten()
            .map(|v| v as f64),
        "trigger_kind": r.try_get::<String, _>("trigger_kind").ok(),
        "outcome": r.try_get::<Option<bool>, _>("outcome").ok().flatten(),
        "source": r.try_get::<String, _>("source").ok(),
        "status": r.try_get::<String, _>("status").ok(),
        "proposed_snapshot": r.try_get::<JsonValue, _>("proposed_snapshot").ok(),
        "applied_deltas": r.try_get::<JsonValue, _>("applied_deltas").ok(),
        "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
        "decided_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at").ok().flatten().map(|t| t.to_rfc3339()),
        "decided_by": r.try_get::<Option<String>, _>("decided_by").ok().flatten(),
    })
}

/// Require EDIT access on a pending cascade's trigger forecast.
///
/// The authorisation anchor for every cascade decision. Cascade work is
/// inherently shared — resolving one forecast queues adjustments to its
/// siblings, which may belong to several people — so "who owns the queue
/// row" was never the right question. "May you change this forecast" is,
/// and it routes through the one canonical helper, which means team shares
/// and portfolio inheritance are honoured automatically.
pub(crate) async fn require_cascade_edit(
    pool: &sqlx::PgPool,
    cascade_id: Uuid,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT ff.id AS fid, ff.owner_id::text AS owner_id, ff.visibility
           FROM public.pending_cascades pc
           JOIN public.fermi_forecasts ff ON ff.id = pc.trigger_forecast_id
          WHERE pc.id = $1",
    )
    .bind(cascade_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Cascade not found".to_string()))?;

    let fid: String = row.try_get("fid").unwrap_or_default();
    let owner_id: String = row.try_get("owner_id").unwrap_or_default();
    let visibility: String = row.try_get("visibility").unwrap_or_default();

    let granted = fermi_auth::visibility::can_edit(
        pool,
        principal,
        fermi_auth::ObjectType::Forecast,
        &fid,
        &owner_id,
        fermi_auth::Visibility::from_legacy(&visibility),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !granted {
        return Err((
            StatusCode::FORBIDDEN,
            "You need edit access to the trigger forecast to decide this cascade".to_string(),
        ));
    }
    Ok(())
}

pub async fn dismiss_pending_cascade_handler(
    Path(cascade_id): Path<String>,
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<crate::handlers::relationships::ApplyDismissRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id().to_string();
    let cid = Uuid::parse_str(&cascade_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid cascade id".into()))?;

    // Spec 27: EDIT on the trigger forecast, not ownership of the queue
    // row. An ops board that shows a teammate work they cannot action is a
    // nag list, and a cascade is precisely the thing a team needs anyone
    // on shift to be able to clear. `owner_id` stays as attribution.
    require_cascade_edit(&state.db, cid, &principal).await?;

    let result = sqlx::query(
        "UPDATE public.pending_cascades
          SET status = 'dismissed',
              decided_at = NOW(),
              decided_by = $2,
              notes = $3
          WHERE id = $1
            AND status = 'pending'",
    )
    .bind(cid)
    .bind(&user_id)
    .bind(&req.notes)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Cascade not found or no longer pending".into(),
        ));
    }

    Ok(Json(json!({
        "id": cascade_id,
        "status": "dismissed",
    })))
}

pub async fn cascade_history_handler(
    Path(forecast_id): Path<String>,
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let incoming = sqlx::query(
        "SELECT pc.id, pc.group_id, pc.trigger_forecast_id, pc.trigger_kind,
                pc.outcome, pc.status, pc.created_at, pc.decided_at,
                pc.applied_deltas, pc.proposed_snapshot
          FROM public.pending_cascades pc
          WHERE pc.applied_deltas @> $1::jsonb
            OR pc.proposed_snapshot @> $1::jsonb
          ORDER BY pc.created_at DESC
          LIMIT 50",
    )
    .bind(serde_json::json!([{"forecast_id": forecast_id}]))
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let outgoing = sqlx::query(
        "SELECT pc.id, pc.group_id, pc.trigger_forecast_id, pc.trigger_kind,
                pc.outcome, pc.status, pc.created_at, pc.decided_at,
                pc.applied_deltas, pc.proposed_snapshot
          FROM public.pending_cascades pc
          WHERE pc.trigger_forecast_id = $1
          ORDER BY pc.created_at DESC
          LIMIT 50",
    )
    .bind(&forecast_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let incoming_entries: Vec<JsonValue> =
        incoming.iter().map(|r| cascade_row_to_json(r)).collect();
    let outgoing_entries: Vec<JsonValue> =
        outgoing.iter().map(|r| cascade_row_to_json(r)).collect();

    Ok(Json(json!({
        "incoming": incoming_entries,
        "outgoing": outgoing_entries,
    })))
}
