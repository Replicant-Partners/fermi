//! The ops board — detected coordination work for a team (Spec 27).
//!
//! ## What an op is
//!
//! A goal-constrained unit of joint work, in the fleet-operation sense: an
//! objective, a scope, the people already involved, and a stated clearing
//! condition.
//!
//! ## Detected, never authored
//!
//! Nothing here is stored. Every op is a condition *currently true* of the
//! team's shared surface, recomputed per request. That one decision buys
//! most of the design:
//!
//! * **The definition of done is the detector going quiet.** There is no
//!   lifecycle, no close button, no assignee column, and structurally no
//!   way to accumulate stale tickets. An op exists exactly as long as the
//!   situation does.
//! * **It is retroactively correct.** The board is populated on day one
//!   from forecasts that existed long before it, with no backfill.
//! * **It cannot drift from reality.** A stored op can disagree with the
//!   world; a derived one cannot.
//!
//! The cost is that an op cannot carry state a human wants to add — "I'm
//! on this", a discussion thread, a deliberate snooze. That is a real
//! limitation and the right moment to add a table is when someone asks for
//! one of those, not before.
//!
//! ## Why teams standing + ops bounded
//!
//! A team is permanent: a roster, a treasury, a record. An op is
//! goal-constrained and disposable. Keeping the objective on the op rather
//! than the team is what lets the team outlive any particular push, which
//! is why `teams.mission` is deliberately untouched by this module.
//!
//! ## Urgency
//!
//! `urgency` is 0–100 and comparable ACROSS kinds, because the whole point
//! of one board is ranking dissimilar work against each other. Each
//! detector documents its own scale. The bucketing into
//! `critical/high/normal/low` happens here so clients don't hardcode
//! thresholds we may retune.
//!
//! The bands are disjoint by design, and their ORDER is the product
//! judgement — damage outranks disagreement outranks maintenance:
//!
//! | band   | detector                                  |
//! |--------|-------------------------------------------|
//! | 80–100 | `cascade_review` — coherence is broken now |
//! | 45–90  | `resolution_due` — overdue climbs, upcoming sits low |
//! | 50–79  | `contested`, `contested_assumption` — disagreement |
//! | 25–49  | `ungrounded` — a number with no research behind it |
//! | 20–44  | `unreviewed` — background maintenance      |
//!
//! ## Events vs conditions
//!
//! An op's GRANULARITY follows from which of the two it is. A cascade
//! landing or a challenge being written is an EVENT: one artifact, one
//! response, one op. "Nothing has research behind it" is a CONDITION of the
//! surface, and gets exactly one op naming the count — see the note above
//! detector 4 for why the per-artifact version made the board unusable.
//!
//! `contested` and `contested_assumption` deliberately share a band: one
//! infers disagreement from opposing revisions, the other reads it stated
//! outright, and neither is more urgent than the other in the abstract.
//! `tests::urgency_bands_are_contiguous_and_ordered` and its neighbours
//! pin this, because a well-meaning tweak could silently invert it.

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi_auth::AuthPrincipal;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::handlers::collab::{require_team_member, resolve_user_names, team_surface};
use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════
// Op model
// ═══════════════════════════════════════════════════════════════════════

struct Op {
    /// `"<kind>:<primary scope id>"`. Stable across polls so the console
    /// can hold a selection while the board refreshes — a random id per
    /// poll would make the list unusable.
    id: String,
    kind: &'static str,
    urgency: i32,
    objective: String,
    done_when: &'static str,
    /// When the *condition* started, not when we noticed. An op that has
    /// been true for three weeks is a different problem from one that
    /// appeared this morning, and the board should say which.
    since: Option<chrono::DateTime<chrono::Utc>>,
    primary: Option<(&'static str, String, Option<String>)>,
    forecast_ids: Vec<String>,
    portfolio_ids: Vec<String>,
    /// (user_id, role). Names resolved in one batch at the end.
    participants: Vec<(String, &'static str)>,
    metrics: JsonValue,
    detail: JsonValue,
}

impl Op {
    fn to_json(&self, names: &HashMap<String, String>) -> JsonValue {
        json!({
            "id":            self.id,
            "kind":          self.kind,
            "urgency":       self.urgency,
            "urgency_label": urgency_label(self.urgency),
            "objective":     self.objective,
            "done_when":     self.done_when,
            "since":         self.since.map(|t| t.to_rfc3339()),
            "primary":       self.primary.as_ref().map(|(k, id, title)| json!({
                                 "type": k, "id": id, "title": title
                             })),
            "scope": {
                "forecast_ids":  self.forecast_ids,
                "portfolio_ids": self.portfolio_ids,
            },
            "participants": self.participants.iter().map(|(uid, role)| json!({
                "user_id":      uid,
                "display_name": names.get(uid).cloned(),
                "role":         role,
            })).collect::<Vec<_>>(),
            "metrics": self.metrics,
            "detail":  self.detail,
        })
    }
}

fn urgency_label(u: i32) -> &'static str {
    match u {
        85..=i32::MAX => "critical",
        65..=84 => "high",
        40..=64 => "normal",
        _ => "low",
    }
}

/// Clamp a computed urgency into the band its detector owns, so one
/// pathological input can't let a low-stakes op outrank a critical one.
fn banded(base: i32, bonus: i32, ceiling: i32) -> i32 {
    (base + bonus.max(0)).min(ceiling).clamp(0, 100)
}

fn days_since(t: Option<chrono::DateTime<chrono::Utc>>) -> i64 {
    t.map(|t| (chrono::Utc::now() - t).num_days().max(0))
        .unwrap_or(0)
}

fn truncate_q(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", head.trim_end())
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/teams/:id/ops
// ═══════════════════════════════════════════════════════════════════════

/// The team's ops board.
///
/// Scoped to the team's shared surface (the same set
/// `collab::team_surface` computes for the Shared and Activity tabs), so
/// an op can never point at work the team can't see. Members-only.
pub async fn team_ops_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<Uuid>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_team_member(pool, team_id, &principal).await?;

    let (forecast_ids, portfolio_ids) = team_surface(pool, team_id).await?;

    let mut ops: Vec<Op> = Vec::new();
    if !forecast_ids.is_empty() {
        // Each detector is independent and failure-isolated: a broken or
        // slow detector degrades the board rather than 500ing it. A
        // partial ops board is useful; no ops board is not.
        ops.extend(detect_cascade_review(pool, &forecast_ids).await);
        ops.extend(detect_contested(pool, &forecast_ids).await);
        ops.extend(detect_resolution_due(pool, &forecast_ids).await);
        ops.extend(detect_unreviewed(pool, &forecast_ids).await);
        ops.extend(detect_contested_assumption(pool, &forecast_ids).await);
        ops.extend(detect_ungrounded(pool, &forecast_ids).await);
    }

    // Rank across kinds. Ties break on the older condition first: work
    // that has been ignored longer should surface above work that just
    // appeared at the same severity.
    ops.sort_by(|a, b| {
        b.urgency
            .cmp(&a.urgency)
            .then_with(|| a.since.cmp(&b.since))
    });

    let all_ids: Vec<String> = ops
        .iter()
        .flat_map(|o| o.participants.iter().map(|(u, _)| u.clone()))
        .collect();
    let names = resolve_user_names(pool, &all_ids).await;

    let mut by_kind: HashMap<&str, usize> = HashMap::new();
    for o in &ops {
        *by_kind.entry(o.kind).or_insert(0) += 1;
    }

    let json_ops: Vec<JsonValue> = ops.iter().map(|o| o.to_json(&names)).collect();

    Ok(Json(json!({
        "team_id": team_id,
        "surface": {
            "forecast_count":  forecast_ids.len(),
            "portfolio_count": portfolio_ids.len(),
        },
        "ops": json_ops,
        "counts": {
            "total":   json_ops.len(),
            "by_kind": by_kind,
        },
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// Detector 1 — cascade_review
// ═══════════════════════════════════════════════════════════════════════

/// Cascades queued and awaiting a decision.
///
/// The highest-value detector, because an unreviewed cascade means the
/// team's forecasts are *known* to be mutually incoherent: something
/// resolved, its siblings should have moved, and nobody has said whether
/// they may. Until v0.11.9 this queue was visible only to whoever
/// triggered it, so on a shared portfolio it was nobody's job.
///
/// Grouped by trigger forecast rather than one op per queue row: the
/// operator's unit of work is "deal with the fallout of X resolving", not
/// "click apply 6 times".
///
/// **Urgency 80–100.** Starts high (coherence is broken *now*) and climbs
/// with age. Nothing else should outrank an unreviewed cascade.
async fn detect_cascade_review(pool: &PgPool, forecast_ids: &[String]) -> Vec<Op> {
    let rows = match sqlx::query(
        "SELECT pc.trigger_forecast_id,
                pc.outcome,
                pc.trigger_kind,
                pc.owner_id,
                pc.created_at,
                pc.proposed_snapshot,
                ff.question_text
           FROM public.pending_cascades pc
           JOIN public.fermi_forecasts ff ON ff.id = pc.trigger_forecast_id
          WHERE pc.status = 'pending'
            AND pc.trigger_forecast_id = ANY($1)
          ORDER BY pc.created_at",
    )
    .bind(forecast_ids)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[ops] cascade detector failed");
            return Vec::new();
        }
    };

    // Fold the queue rows into one op per trigger, accumulating the union
    // of affected forecasts from each row's dry-run snapshot. The snapshot
    // is what the operator was going to be shown before authorising, so
    // it's the honest blast radius.
    struct Acc {
        question: String,
        outcome: Option<bool>,
        trigger_kind: String,
        pending: usize,
        affected: HashSet<String>,
        owners: HashSet<String>,
        since: Option<chrono::DateTime<chrono::Utc>>,
    }
    let mut grouped: HashMap<String, Acc> = HashMap::new();

    for r in &rows {
        let fid: String = match r.try_get("trigger_forecast_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let created: Option<chrono::DateTime<chrono::Utc>> = r.try_get("created_at").ok();
        let entry = grouped.entry(fid).or_insert_with(|| Acc {
            question: r
                .try_get::<String, _>("question_text")
                .unwrap_or_else(|_| "—".into()),
            outcome: r.try_get::<Option<bool>, _>("outcome").ok().flatten(),
            trigger_kind: r
                .try_get::<String, _>("trigger_kind")
                .unwrap_or_else(|_| "updated".into()),
            pending: 0,
            affected: HashSet::new(),
            owners: HashSet::new(),
            since: created,
        });
        entry.pending += 1;
        if let Ok(Some(o)) = r.try_get::<Option<String>, _>("owner_id") {
            entry.owners.insert(o);
        }
        if let Ok(Some(snap)) = r.try_get::<Option<JsonValue>, _>("proposed_snapshot") {
            if let Some(deltas) = snap.get("deltas").and_then(|d| d.as_array()) {
                for d in deltas {
                    if let Some(id) = d.get("forecast_id").and_then(|v| v.as_str()) {
                        entry.affected.insert(id.to_string());
                    }
                }
            }
        }
        // Rows are ORDER BY created_at, so the first one seen is oldest.
        if entry.since.is_none() {
            entry.since = created;
        }
    }

    grouped
        .into_iter()
        .map(|(fid, acc)| {
            let age = days_since(acc.since);
            let outcome_word = match (acc.trigger_kind.as_str(), acc.outcome) {
                ("resolved", Some(true)) => "resolving YES".to_string(),
                ("resolved", Some(false)) => "resolving NO".to_string(),
                ("resolved", None) => "resolving".to_string(),
                _ => "moving".to_string(),
            };
            let affected = acc.affected.len();
            let mut scope: Vec<String> = acc.affected.into_iter().collect();
            scope.sort();
            if !scope.contains(&fid) {
                scope.push(fid.clone());
            }

            Op {
                id: format!("cascade_review:{}", fid),
                kind: "cascade_review",
                // 5 urgency per day waiting: an unreviewed cascade a week
                // old is as bad as it gets.
                urgency: banded(80, (age * 5) as i32, 100),
                objective: format!(
                    "Review {} queued cascade{} from ‹{}› {}",
                    acc.pending,
                    if acc.pending == 1 { "" } else { "s" },
                    truncate_q(&acc.question, 60),
                    outcome_word
                ),
                done_when: "every cascade queued by this trigger is applied or dismissed",
                since: acc.since,
                primary: Some(("forecast", fid.clone(), Some(acc.question))),
                forecast_ids: scope,
                portfolio_ids: Vec::new(),
                participants: acc
                    .owners
                    .into_iter()
                    .map(|o| (o, "trigger_owner"))
                    .collect(),
                metrics: json!({ "pending": acc.pending, "affected": affected, "waiting_days": age }),
                detail: json!({ "trigger_kind": acc.trigger_kind, "outcome": acc.outcome }),
            }
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Detector 2 — contested
// ═══════════════════════════════════════════════════════════════════════

/// Two or more people have moved the same forecast in opposite directions.
///
/// The flagship detector, and the one that only became possible because
/// v0.11.7 added `fermi_forecast_updates.actor_user_id`. Genuine
/// disagreement between forecasters is the most valuable thing a team can
/// surface — it is where the assumptions actually live — and before this
/// it was invisible even to the two people doing it.
///
/// Requires a meaningful move in BOTH directions (>2pp each way) inside a
/// 21-day window, from at least two distinct humans. A single person
/// oscillating is not a disagreement, and 2pp is below the noise floor of
/// a re-run.
///
/// **Urgency 50–79.** Real, but always below an unreviewed cascade
/// (floor 80): a disagreement is *information*, an unreviewed cascade is
/// *damage*, and damage outranks information. That rule is what makes the
/// board's ranking explainable to the people using it. Scales with the
/// spread, because a 40pp gap is a different conversation from a 5pp one.
///
/// Note this will be quiet on pre-v0.11.7 history — those revisions have
/// no recorded actor and migration 176 deliberately did not invent one.
/// The board fills in as the team works.
async fn detect_contested(pool: &PgPool, forecast_ids: &[String]) -> Vec<Op> {
    let rows = match sqlx::query(
        "SELECT u.forecast_id,
                ff.question_text,
                COUNT(DISTINCT u.actor_user_id)                       AS actors,
                MIN(u.new_probability - u.previous_probability)        AS min_delta,
                MAX(u.new_probability - u.previous_probability)        AS max_delta,
                MIN(u.created_at)                                     AS since
           FROM public.fermi_forecast_updates u
           JOIN public.fermi_forecasts ff ON ff.id = u.forecast_id
          WHERE u.forecast_id = ANY($1)
            AND u.actor_user_id IS NOT NULL
            AND ff.status = 'active'
            AND u.created_at > NOW() - INTERVAL '21 days'
          GROUP BY u.forecast_id, ff.question_text
         HAVING COUNT(DISTINCT u.actor_user_id) >= 2
            AND MIN(u.new_probability - u.previous_probability) < -0.02
            AND MAX(u.new_probability - u.previous_probability) >  0.02",
    )
    .bind(forecast_ids)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[ops] contested detector failed");
            return Vec::new();
        }
    };

    if rows.is_empty() {
        return Vec::new();
    }

    // Per-actor net movement on the contested forecasts, so the objective
    // can name who is pulling which way. A bare "2 people disagree" makes
    // the operator go digging; "Alice +6pp, Bo −9pp" is actionable.
    let contested_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("forecast_id").ok())
        .collect();

    let mut per_actor: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    if let Ok(actor_rows) = sqlx::query(
        "SELECT u.forecast_id,
                u.actor_user_id,
                SUM(u.new_probability - u.previous_probability)::float8 AS net
           FROM public.fermi_forecast_updates u
          WHERE u.forecast_id = ANY($1)
            AND u.actor_user_id IS NOT NULL
            AND u.created_at > NOW() - INTERVAL '21 days'
          GROUP BY u.forecast_id, u.actor_user_id
          ORDER BY ABS(SUM(u.new_probability - u.previous_probability)) DESC",
    )
    .bind(&contested_ids)
    .fetch_all(pool)
    .await
    {
        for r in &actor_rows {
            if let (Ok(fid), Ok(Some(actor)), Ok(net)) = (
                r.try_get::<String, _>("forecast_id"),
                r.try_get::<Option<String>, _>("actor_user_id"),
                r.try_get::<f64, _>("net"),
            ) {
                per_actor.entry(fid).or_default().push((actor, net));
            }
        }
    }

    let names_needed: Vec<String> = per_actor
        .values()
        .flat_map(|v| v.iter().map(|(a, _)| a.clone()))
        .collect();
    let names = resolve_user_names(pool, &names_needed).await;

    rows.iter()
        .filter_map(|r| {
            let fid: String = r.try_get("forecast_id").ok()?;
            let question: String = r
                .try_get::<String, _>("question_text")
                .unwrap_or_else(|_| "—".into());
            let actors: i64 = r.try_get("actors").unwrap_or(2);
            // previous/new_probability are REAL, so their difference is
            // REAL → f32 at the sqlx boundary.
            let min_d: f32 = r.try_get("min_delta").unwrap_or(0.0);
            let max_d: f32 = r.try_get("max_delta").unwrap_or(0.0);
            let since: Option<chrono::DateTime<chrono::Utc>> = r.try_get("since").ok();
            let spread_pp = ((max_d - min_d) * 100.0) as f64;

            let movers = per_actor.get(&fid).cloned().unwrap_or_default();
            let summary = movers
                .iter()
                .take(2)
                .map(|(a, net)| {
                    format!(
                        "{} {}{:.0}pp",
                        names.get(a).cloned().unwrap_or_else(|| a.clone()),
                        if *net >= 0.0 { "+" } else { "−" },
                        (net * 100.0).abs()
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");

            Some(Op {
                id: format!("contested:{}", fid),
                kind: "contested",
                // 1 urgency per pp of spread, so a 30pp disagreement lands
                // at the top of its band and a 5pp one near the bottom.
                urgency: banded(50, spread_pp.round() as i32, 79),
                objective: if summary.is_empty() {
                    format!(
                        "Reconcile {} opposing revisions on ‹{}›",
                        actors,
                        truncate_q(&question, 60)
                    )
                } else {
                    format!("Reconcile ‹{}› — {}", truncate_q(&question, 48), summary)
                },
                // Stated in terms of the actual mechanism, so the board
                // teaches how it works rather than implying a button.
                done_when: "someone revises toward the other, or the disagreement ages \
                            out of the 21-day window",
                since,
                primary: Some(("forecast", fid.clone(), Some(question))),
                forecast_ids: vec![fid],
                portfolio_ids: Vec::new(),
                participants: movers.iter().map(|(a, _)| (a.clone(), "reviser")).collect(),
                metrics: json!({
                    "spread_pp": (spread_pp * 10.0).round() / 10.0,
                    "actors":    actors,
                }),
                detail: json!({
                    "per_actor_net_pp": movers.iter().map(|(a, n)| json!({
                        "user_id": a,
                        "display_name": names.get(a).cloned(),
                        "net_pp": (n * 1000.0).round() / 10.0,
                    })).collect::<Vec<_>>(),
                }),
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Detector 3 — resolution_due
// ═══════════════════════════════════════════════════════════════════════

/// Active forecasts whose target date has arrived or is about to.
///
/// A forecast that silently sails past its resolution date is the quiet
/// way a calibration record rots: no Brier is recorded, and the trajectory
/// just stops. Overdue outranks upcoming, because an unresolved past-due
/// forecast is already costing the team its score.
///
/// **Urgency 45–90.** Overdue starts at 70 and climbs; upcoming sits in
/// the normal band.
async fn detect_resolution_due(pool: &PgPool, forecast_ids: &[String]) -> Vec<Op> {
    let rows = match sqlx::query(
        "SELECT ff.id,
                ff.question_text,
                ff.owner_id::text AS owner_id,
                ff.target_date,
                ff.predicted_probability,
                ff.updated_at,
                EXTRACT(EPOCH FROM (ff.target_date - NOW())) / 86400.0 AS days_out
           FROM public.fermi_forecasts ff
          WHERE ff.id = ANY($1)
            AND ff.status = 'active'
            AND ff.target_date IS NOT NULL
            AND ff.target_date < NOW() + INTERVAL '14 days'",
    )
    .bind(forecast_ids)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[ops] resolution_due detector failed");
            return Vec::new();
        }
    };

    rows.iter()
        .filter_map(|r| {
            let fid: String = r.try_get("id").ok()?;
            let question: String = r
                .try_get::<String, _>("question_text")
                .unwrap_or_else(|_| "—".into());
            let owner: Option<String> = r.try_get::<String, _>("owner_id").ok();
            let days_out: f64 = r
                .try_get::<Option<f64>, _>("days_out")
                .ok()
                .flatten()
                .unwrap_or(0.0);
            let target: Option<chrono::DateTime<chrono::Utc>> = r
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("target_date")
                .ok()
                .flatten();
            let overdue = days_out < 0.0;
            let d = days_out.abs().round() as i64;

            let when = if overdue {
                match d {
                    0 => "due today".to_string(),
                    1 => "1 day overdue".to_string(),
                    n => format!("{} days overdue", n),
                }
            } else {
                match d {
                    0 => "due today".to_string(),
                    1 => "due tomorrow".to_string(),
                    n => format!("due in {} days", n),
                }
            };

            Some(Op {
                id: format!("resolution_due:{}", fid),
                kind: "resolution_due",
                urgency: if overdue {
                    banded(70, d as i32 * 2, 90)
                } else {
                    // Closer = more urgent: 14 days out is the floor.
                    banded(45, (14 - d.min(14)) as i32, 64)
                },
                objective: format!("Resolve ‹{}› — {}", truncate_q(&question, 58), when),
                done_when: "the forecast is resolved, voided, or its target date is moved",
                // The condition began at the target date for overdue rows;
                // for upcoming ones it hasn't really begun, so leave it
                // unset rather than implying an age.
                since: if overdue { target } else { None },
                primary: Some(("forecast", fid.clone(), Some(question))),
                forecast_ids: vec![fid],
                portfolio_ids: Vec::new(),
                participants: owner.into_iter().map(|o| (o, "owner")).collect(),
                metrics: json!({
                    "days_out": (days_out * 10.0).round() / 10.0,
                    "overdue":  overdue,
                }),
                detail: json!({ "target_date": target.map(|t| t.to_rfc3339()) }),
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Detectors 4 & 6 — surface conditions, rolled up
// ═══════════════════════════════════════════════════════════════════════
//
// EVENTS vs CONDITIONS — the rule that decides an op's granularity.
//
// A cascade landing, a target date arriving, someone writing a challenge:
// those are EVENTS. Each names one artifact and one distinct response, so
// each is its own op.
//
// "Nothing has been reviewed" and "nothing has research behind it" are
// CONDITIONS of the surface as a whole. Emitting one op per artifact turned
// the board into six rows of the same sentence with a different question in
// it — a lint list, not coordination. Nobody picks those up one at a time,
// and the repetition buried the events that actually needed a response.
//
// So conditions roll up: one op per condition, naming the count, carrying
// the members in `detail.items` for the console to expand. The unit of
// coordination is the condition, because that is the unit of the decision
// ("we need to get research onto the EPL book"), not the artifact.

/// How many members of a rolled-up condition are listed in `detail.items`.
///
/// The count in the objective is the true total; this only bounds what the
/// expander renders. A surface with 200 unreviewed forecasts is one
/// decision, not 200 rows to scroll.
const ROLLUP_SAMPLE: usize = 25;

/// One member of a rolled-up condition.
struct SurfaceItem {
    forecast_id: String,
    question: String,
    owner: Option<String>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    probability: f32,
    age_days: i64,
}

impl SurfaceItem {
    fn to_json(&self) -> JsonValue {
        json!({
            "forecast_id":     self.forecast_id,
            "question":        self.question,
            "age_days":        self.age_days,
            "probability_pct": (self.probability * 100.0).round(),
        })
    }
}

/// Shared shape for the two roll-ups: same columns, different predicate.
async fn surface_items(pool: &PgPool, sql: &str, forecast_ids: &[String]) -> Vec<SurfaceItem> {
    let rows = match sqlx::query(sql).bind(forecast_ids).fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "[ops] surface condition query failed");
            return Vec::new();
        }
    };
    rows.iter()
        .filter_map(|r| {
            let created: Option<chrono::DateTime<chrono::Utc>> = r.try_get("created_at").ok();
            Some(SurfaceItem {
                forecast_id: r.try_get("id").ok()?,
                question: r
                    .try_get::<String, _>("question_text")
                    .unwrap_or_else(|_| "—".into()),
                owner: r.try_get::<String, _>("owner_id").ok(),
                since: created,
                probability: r.try_get("predicted_probability").unwrap_or(0.0),
                age_days: days_since(created),
            })
        })
        .collect()
}

/// Participants for a roll-up: the distinct owners of its members.
///
/// Deduplicated, because one person owning nine unreviewed forecasts is one
/// person to talk to, not nine avatars.
fn rollup_participants(items: &[SurfaceItem]) -> Vec<(String, &'static str)> {
    let mut seen = HashSet::new();
    items
        .iter()
        .filter_map(|i| i.owner.clone())
        .filter(|o| seen.insert(o.clone()))
        .map(|o| (o, "owner"))
        .collect()
}

/// Live forecasts on the shared surface that nobody has ever revised.
///
/// A number published once and never touched has had exactly one person's
/// judgement applied to it. On a *shared* surface that is a gap, not a
/// state: the whole reason to share a forecast with a team is to get more
/// than one mind on it.
///
/// Deliberately keyed on "zero revisions" rather than "no revision by a
/// second actor", because the latter needs `actor_user_id` and would
/// therefore be blind to everything published before v0.11.7. This version
/// fires correctly on legacy data.
///
/// **Urgency 20–44 (low).** Real, but it should never crowd out a broken
/// cascade or an active disagreement. This is the background maintenance
/// tier of the board. Scales with the oldest member, not the count: a
/// hundred forecasts stale for a week is less urgent than one stale for a
/// year, and counting instead would let a big book shout down a real
/// problem.
async fn detect_unreviewed(pool: &PgPool, forecast_ids: &[String]) -> Vec<Op> {
    let items = surface_items(
        pool,
        "SELECT ff.id,
                ff.question_text,
                ff.owner_id::text AS owner_id,
                ff.created_at,
                ff.predicted_probability
           FROM public.fermi_forecasts ff
          WHERE ff.id = ANY($1)
            AND ff.status = 'active'
            AND ff.created_at < NOW() - INTERVAL '7 days'
            AND NOT EXISTS (SELECT 1 FROM public.fermi_forecast_updates u
                            WHERE u.forecast_id = ff.id)
          ORDER BY ff.created_at",
        forecast_ids,
    )
    .await;

    if items.is_empty() {
        return Vec::new();
    }

    let oldest = items.iter().map(|i| i.age_days).max().unwrap_or(0);
    let n = items.len();

    vec![Op {
        // Stable across polls, and stable as members come and go — the
        // condition is the thing, so its identity can't depend on which
        // forecasts currently satisfy it.
        id: "unreviewed:surface".to_string(),
        kind: "unreviewed",
        urgency: banded(20, (oldest / 7) as i32 * 4, 44),
        objective: if n == 1 {
            format!(
                "Stress-test ‹{}› — live at {:.0}% for {} days, never revised",
                truncate_q(&items[0].question, 50),
                items[0].probability * 100.0,
                items[0].age_days
            )
        } else {
            format!(
                "Stress-test {} forecasts nobody has ever revised — oldest is {} days old",
                n, oldest
            )
        },
        done_when: "anyone records a revision — including one that confirms the number",
        since: items.iter().filter_map(|i| i.since).min(),
        // A roll-up points at its worst member so the row still has
        // somewhere to click. The objective states the aggregate; the
        // primary is where to start.
        primary: items
            .iter()
            .max_by_key(|i| i.age_days)
            .map(|i| ("forecast", i.forecast_id.clone(), Some(i.question.clone()))),
        forecast_ids: items.iter().map(|i| i.forecast_id.clone()).collect(),
        portfolio_ids: Vec::new(),
        participants: rollup_participants(&items),
        metrics: json!({ "count": n, "oldest_age_days": oldest }),
        detail: json!({
            "items": items.iter().take(ROLLUP_SAMPLE).map(|i| i.to_json())
                          .collect::<Vec<_>>(),
            "truncated": n > ROLLUP_SAMPLE,
        }),
    }]
}

// ═══════════════════════════════════════════════════════════════════════
// Detector 6 — ungrounded
// ═══════════════════════════════════════════════════════════════════════

/// Live forecasts carrying a number with no research behind it at all —
/// no evidence, no agent.
///
/// This is the condition `unreviewed` was accidentally reporting. On the
/// surface that prompted it, every active forecast had zero evidence and
/// zero agents, while the team's *resolved* work carried four agents and
/// four to six evidence items apiece. "Never revised" was the symptom;
/// "there is nothing here to revise from" is the cause — and you cannot
/// meaningfully stress-test a number you have no evidence for, so the old
/// board was asking for the second-order thing first.
///
/// The composer already says this per driver ("No agents — assign one to
/// research this driver"). The board didn't, so the one person who could
/// act on it had to open each forecast to find out.
///
/// **Urgency 25–49.** Slightly above `unreviewed` and overlapping it,
/// because an ungrounded forecast is strictly worse than a merely stale
/// one — it is stale *and* unsupported — but it is still maintenance, and
/// must stay below a disagreement (floor 50).
async fn detect_ungrounded(pool: &PgPool, forecast_ids: &[String]) -> Vec<Op> {
    let items = surface_items(
        pool,
        // jsonb_typeof guards the column being '{}' or a string rather than
        // an array — both occur in this table's history.
        "SELECT ff.id,
                ff.question_text,
                ff.owner_id::text AS owner_id,
                ff.created_at,
                ff.predicted_probability
           FROM public.fermi_forecasts ff
          WHERE ff.id = ANY($1)
            AND ff.status = 'active'
            AND ff.created_at < NOW() - INTERVAL '7 days'
            AND COALESCE(jsonb_array_length(
                    CASE WHEN jsonb_typeof(ff.evidence) = 'array'
                         THEN ff.evidence ELSE '[]'::jsonb END), 0) = 0
            AND COALESCE(jsonb_array_length(
                    CASE WHEN jsonb_typeof(ff.agents_used) = 'array'
                         THEN ff.agents_used ELSE '[]'::jsonb END), 0) = 0
          ORDER BY ff.created_at",
        forecast_ids,
    )
    .await;

    if items.is_empty() {
        return Vec::new();
    }

    let oldest = items.iter().map(|i| i.age_days).max().unwrap_or(0);
    let n = items.len();

    vec![Op {
        id: "ungrounded:surface".to_string(),
        kind: "ungrounded",
        urgency: banded(25, (oldest / 7) as i32 * 4, 49),
        objective: if n == 1 {
            format!(
                "Research ‹{}› — live at {:.0}% with no evidence and no agent",
                truncate_q(&items[0].question, 46),
                items[0].probability * 100.0
            )
        } else {
            format!(
                "Ground {} live forecasts that have no evidence and no agent behind them",
                n
            )
        },
        done_when: "each one has at least one evidence item or an assigned agent",
        since: items.iter().filter_map(|i| i.since).min(),
        primary: items
            .iter()
            .max_by_key(|i| i.age_days)
            .map(|i| ("forecast", i.forecast_id.clone(), Some(i.question.clone()))),
        forecast_ids: items.iter().map(|i| i.forecast_id.clone()).collect(),
        portfolio_ids: Vec::new(),
        participants: rollup_participants(&items),
        metrics: json!({ "count": n, "oldest_age_days": oldest }),
        detail: json!({
            "items": items.iter().take(ROLLUP_SAMPLE).map(|i| i.to_json())
                          .collect::<Vec<_>>(),
            "truncated": n > ROLLUP_SAMPLE,
        }),
    }]
}

// ═══════════════════════════════════════════════════════════════════════
// Detector 5 — contested_assumption
// ═══════════════════════════════════════════════════════════════════════

/// Someone has written down an objection to a specific driver and nobody
/// has answered it.
///
/// This is the Spec 32 payoff, and it is the detector the other four were
/// working towards. `contested` (detector 2) *infers* disagreement from
/// probabilities moving in opposite directions — real, but it can only ever
/// tell you two people disagree, never about what. A challenge is the same
/// disagreement made explicit and anchored to the input it is about, which
/// is the difference between "reconcile this forecast" and "settle whether
/// the base rate for `elo_current` is right".
///
/// Only `kind = 'challenge'` counts. A `note` explicitly implies no action,
/// and a `question` is answered by talking rather than by deciding — neither
/// is work the team needs ranked against a broken cascade.
///
/// Orphaned annotations fall out for free: the sweep moves them off `open`,
/// so a challenge against a driver that no longer exists stops generating
/// an op. That is exactly why the sweep exists — a board item you cannot
/// act on because its subject is gone would be worse than no board item.
///
/// **Urgency 50–79**, the same band as `contested`, for the same reason:
/// disagreement is information, and information ranks below the damage a
/// broken cascade represents (floor 80). Within the band it climbs with
/// age, because the failure mode here is not disagreement — it's an
/// objection that nobody ever answered.
async fn detect_contested_assumption(pool: &PgPool, forecast_ids: &[String]) -> Vec<Op> {
    let rows = match sqlx::query(
        "SELECT a.forecast_id,
                ff.question_text,
                COUNT(*)                                  AS n_open,
                MIN(a.created_at)                         AS since,
                ARRAY_AGG(DISTINCT a.driver_name)
                    FILTER (WHERE a.driver_name IS NOT NULL) AS drivers,
                ARRAY_AGG(DISTINCT a.author_id)           AS authors
           FROM public.driver_annotations a
           JOIN public.fermi_forecasts ff ON ff.id = a.forecast_id
          WHERE a.forecast_id = ANY($1)
            AND a.status = 'open'
            AND a.kind = 'challenge'
            AND ff.status = 'active'
          GROUP BY a.forecast_id, ff.question_text",
    )
    .bind(forecast_ids)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("ops: contested_assumption detector failed: {}", e);
            return Vec::new();
        }
    };

    rows.into_iter()
        .map(|r| {
            let fid: String = r.get("forecast_id");
            let question: String = r.try_get("question_text").unwrap_or_default();
            let n_open: i64 = r.try_get("n_open").unwrap_or(0);
            let since: Option<chrono::DateTime<chrono::Utc>> = r.try_get("since").ok().flatten();
            let drivers: Vec<String> = r
                .try_get::<Option<Vec<String>>, _>("drivers")
                .ok()
                .flatten()
                .unwrap_or_default();
            let authors: Vec<String> = r
                .try_get::<Option<Vec<String>>, _>("authors")
                .ok()
                .flatten()
                .unwrap_or_default();

            let age = days_since(since);

            // Name the disputed inputs, not just the count — "the base rate
            // for elo_current is disputed" is actionable in a way that
            // "1 open challenge" is not.
            let subject = match drivers.len() {
                0 => String::new(),
                1 => format!(" — {}", drivers[0]),
                2 => format!(" — {}, {}", drivers[0], drivers[1]),
                n => format!(" — {}, {} +{} more", drivers[0], drivers[1], n - 2),
            };

            Op {
                id: format!("contested_assumption:{}", fid),
                kind: "contested_assumption",
                // 2 per day unanswered, 5 per extra challenge. A single
                // objection left for a fortnight reaches the top of the
                // band on age alone, which is the point.
                urgency: banded(50, (age * 2) as i32 + (n_open as i32 - 1) * 5, 79),
                objective: format!(
                    "Answer {} open challenge{} on ‹{}›{}",
                    n_open,
                    if n_open == 1 { "" } else { "s" },
                    truncate_q(&question, 44),
                    subject
                ),
                // Named for the mechanism, and note that BOTH outcomes
                // close it. "Declined" is a real answer; the board must not
                // read as pressure to agree.
                done_when: "each open challenge is accepted or declined",
                since,
                primary: Some(("forecast", fid.clone(), Some(question))),
                forecast_ids: vec![fid],
                portfolio_ids: Vec::new(),
                participants: authors.into_iter().map(|a| (a, "challenger")).collect(),
                metrics: json!({
                    "open_challenges": n_open,
                    "drivers":         drivers.len(),
                    "age_days":        age,
                }),
                detail: json!({ "driver_names": drivers }),
            }
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Tests — the pure parts
// ═══════════════════════════════════════════════════════════════════════
//
// The SQL is covered by scripts/spec26_sql_check.sh. What's worth pinning
// here is the urgency algebra, because it encodes a product judgement
// (broken coherence outranks disagreement outranks maintenance) that a
// well-meaning tweak could silently invert.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urgency_bands_are_contiguous_and_ordered() {
        assert_eq!(urgency_label(100), "critical");
        assert_eq!(urgency_label(85), "critical");
        assert_eq!(urgency_label(84), "high");
        assert_eq!(urgency_label(65), "high");
        assert_eq!(urgency_label(64), "normal");
        assert_eq!(urgency_label(40), "normal");
        assert_eq!(urgency_label(39), "low");
        assert_eq!(urgency_label(0), "low");
    }

    #[test]
    fn banded_respects_its_ceiling_and_never_escapes_0_100() {
        assert_eq!(banded(80, 0, 100), 80);
        assert_eq!(banded(80, 5 * 4, 100), 100);
        // A ceiling below the natural sum is the point: `contested` must
        // not be able to reach the cascade band no matter how large the
        // spread.
        assert_eq!(banded(50, 900, 84), 84);
        // Negative bonuses are clamped, so a clock skew can't demote an op.
        assert_eq!(banded(50, -20, 84), 50);
    }

    // The band arithmetic encodes a product judgement, so it gets asserted
    // rather than left to a comment. These four tests are the ranking
    // argument; if a future tweak inverts one, it should have to delete an
    // assertion and say why.

    /// **Damage outranks information.** No disagreement, however large,
    /// may outrank an unreviewed cascade — incoherent numbers are a fact
    /// about the team's output, a disagreement is an input to it.
    #[test]
    fn no_disagreement_outranks_broken_coherence() {
        let cascade_floor = banded(80, 0, 100);
        let contested_ceiling = banded(50, i32::MAX / 2, 79);
        assert!(
            contested_ceiling < cascade_floor,
            "contested {} must stay below cascade floor {}",
            contested_ceiling,
            cascade_floor
        );
    }

    /// A disagreement is never an emergency: `contested` must not reach
    /// the `critical` band, or the board cries wolf on normal analysis.
    #[test]
    fn contested_never_reaches_critical() {
        assert_eq!(urgency_label(banded(50, i32::MAX / 2, 79)), "high");
    }

    /// Maintenance is always last. An untouched forecast must never
    /// outrank real work, no matter how long it has sat.
    #[test]
    fn maintenance_never_outranks_real_work() {
        let unreviewed_ceiling = banded(20, i32::MAX / 2, 44);
        assert!(unreviewed_ceiling < banded(45, 0, 64), "vs upcoming due");
        assert!(unreviewed_ceiling < banded(50, 0, 79), "vs contested floor");
        assert!(unreviewed_ceiling < banded(80, 0, 100), "vs cascade floor");
        assert_eq!(urgency_label(unreviewed_ceiling), "normal");
    }

    /// Overdue resolutions ARE allowed to reach into the cascade band —
    /// intentional, not an oversight. A forecast weeks past its target
    /// date is losing the team its calibration record; that is damage of
    /// the same class as an unapplied cascade, so it competes on merit.
    /// Upcoming ones stay in `normal` where they can be planned around.
    #[test]
    fn overdue_resolution_may_compete_with_cascades() {
        let overdue_ceiling = banded(70, 999, 90);
        assert!(overdue_ceiling >= banded(80, 0, 100));
        assert_eq!(urgency_label(banded(45, 14, 64)), "normal");
        assert_eq!(urgency_label(banded(45, 0, 64)), "normal");
    }

    /// A stated objection and an inferred one share a band on purpose, so
    /// neither systematically buries the other. Nothing in the design says
    /// "someone wrote it down" is more urgent than "the numbers diverged",
    /// or the reverse — they are the same class of work and should
    /// interleave by age and size, which only holds if the bands match.
    #[test]
    fn stated_and_inferred_disagreement_share_a_band() {
        let inferred = (banded(50, 0, 79), banded(50, i32::MAX / 2, 79));
        let stated = (banded(50, 0, 79), banded(50, i32::MAX / 2, 79));
        assert_eq!(inferred, stated);
        // And the shared band still respects the two rules either side of
        // it: below broken coherence, above background maintenance.
        assert!(stated.1 < banded(80, 0, 100), "must stay below cascades");
        assert!(
            stated.0 > banded(20, i32::MAX / 2, 44),
            "must outrank maintenance"
        );
    }

    /// The failure mode `contested_assumption` exists to catch is not
    /// disagreement — it is an objection nobody ever answered. So age alone
    /// must be able to carry it to the top of its band, without needing a
    /// pile-on of extra challenges to get attention.
    #[test]
    fn a_single_unanswered_challenge_escalates_on_age_alone() {
        let one_challenge = |age: i64| banded(50, (age * 2) as i32, 79);
        assert_eq!(one_challenge(0), 50, "fresh: bottom of the band");
        assert_eq!(urgency_label(one_challenge(0)), "normal");
        assert_eq!(one_challenge(7), 64, "a week: still normal");
        assert_eq!(urgency_label(one_challenge(8)), "high", "over a week: high");
        assert_eq!(one_challenge(15), 79, "a fortnight: band ceiling");
        assert_eq!(one_challenge(400), 79, "and it stops there");
    }

    /// `ungrounded` outranks `unreviewed` (an unsupported number is worse
    /// than a merely stale one) but still loses to any disagreement. The
    /// two maintenance bands overlap on purpose — they are the same tier —
    /// while the floor of disagreement stays above both.
    #[test]
    fn research_gaps_outrank_staleness_but_lose_to_disagreement() {
        let unreviewed_floor = banded(20, 0, 44);
        let unreviewed_ceiling = banded(20, i32::MAX / 2, 44);
        let ungrounded_floor = banded(25, 0, 49);
        let ungrounded_ceiling = banded(25, i32::MAX / 2, 49);

        assert!(
            ungrounded_floor > unreviewed_floor,
            "ungrounded starts higher"
        );
        assert!(ungrounded_ceiling > unreviewed_ceiling, "and ends higher");
        assert!(
            ungrounded_ceiling < banded(50, 0, 79),
            "maintenance must never reach the disagreement floor"
        );
        assert_eq!(urgency_label(ungrounded_ceiling), "normal");
    }

    /// Roll-ups scale with their OLDEST member, never their count.
    ///
    /// If count drove urgency, importing a book of 200 fresh forecasts
    /// would immediately outrank a single forecast that has been ignored
    /// for a year — the board would reward volume over neglect. Size
    /// belongs in the objective text, not the ranking.
    #[test]
    fn rollup_urgency_tracks_age_not_count() {
        let by_age = |oldest_days: i64| banded(20, (oldest_days / 7) as i32 * 4, 44);

        assert_eq!(by_age(0), 20, "fresh sits at the floor");
        assert_eq!(by_age(7), 24, "+4 per week");
        assert_eq!(by_age(41), 40);
        // Saturates at six weeks. Past that the board stops distinguishing
        // "badly neglected" from "even more badly neglected", which is
        // correct — both are the same conversation, and the band exists so
        // maintenance can never shout over a broken cascade.
        assert_eq!(by_age(42), 44, "ceiling reached at six weeks");
        assert_eq!(by_age(365), 44, "and stays there");

        // The ranking argument: one long-neglected forecast beats two
        // hundred fresh ones, because urgency never reads the count at all.
        assert!(by_age(60) > by_age(7));
    }

    #[test]
    fn question_truncation_is_char_safe() {
        // Multi-byte input must not panic or split a codepoint — the WC
        // dataset is full of accented team names.
        let s = "¿Ganará Argentina la Copa Mundial de 2026 en el estadio?";
        let out = truncate_q(s, 20);
        assert!(out.chars().count() <= 20);
        assert!(out.ends_with('…'));
        assert_eq!(truncate_q("short", 20), "short");
    }

    #[test]
    fn days_since_never_goes_negative() {
        let future = chrono::Utc::now() + chrono::Duration::days(5);
        assert_eq!(days_since(Some(future)), 0);
        assert_eq!(days_since(None), 0);
    }
}
