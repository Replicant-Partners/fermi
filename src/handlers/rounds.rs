//! Rounds — what needs you, in what order.
//!
//! # Why this is not a dashboard
//!
//! `templates/dashboard.html` is a directory: eight blocks and four modals,
//! around thirty actions at identical visual weight, and a miniature of every
//! other page in the product. Nothing is ranked, so the strongest surface in the
//! platform ends up reachable only through a button inside a tile.
//!
//! A clinician's round is the opposite shape: an ordered visit to whoever needs
//! attention, on a cadence. The ordering is the feature. This endpoint therefore
//! returns three things and puts them in one order:
//!
//! 1. **Decisions** — things blocked on a human, soonest-stale first.
//! 2. **Instrument** — what the platform can and cannot currently tell you.
//! 3. **Resume** — what you were last working on.
//!
//! # The second one is the unusual one
//!
//! Every other surface in the product answers *"what is the state of X"*. The
//! instrument block answers *"can this platform answer that question at all"*,
//! by rendering [`crate::panel_absence`] through
//! [`crate::panel_contract`] at [`Density::Scan`].
//!
//! That is the design thesis made visible: four of the nine defect classes in
//! `FEEDBACK_LOOPS.md` are invisible at the surface by construction and render
//! identically as "No data yet". A UI that shows its own blind spots is not
//! decoration; it is the difference between a loop that has stalled and a panel
//! that has been quietly lying since it shipped.
//!
//! # What it deliberately does not do
//!
//! It runs no gate and makes no decision. Every reading it serves is computed by
//! a contract that owns it — the split the glasses shell states as *"It decides
//! nothing. […] The glasses are I/O."* Rounds is another such shell.

use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use fermi::panel_absence;
use fermi::panel_contract::{stamp_absence, Density};
use fermi_auth::AuthPrincipal;

use crate::AppState;

/// A thing waiting on a person.
///
/// One shape for every source, because a queue whose rows are not comparable is
/// a list of lists and cannot be ordered — which is the dashboard's defect
/// restated.
fn decision(
    kind: &str,
    subject: &str,
    subject_href: String,
    what: String,
    age_days: Option<f64>,
    severity: &str,
) -> Value {
    json!({
        "kind": kind,
        "subject": subject,
        "subject_href": subject_href,
        "what": what,
        "age_days": age_days,
        "severity": severity,
    })
}

/// Keep the rows, or record why there are none.
///
/// `unwrap_or_default()` on a query is how *"nothing is waiting for you"* and
/// *"I could not ask"* become the same sentence. On a decision queue that is the
/// worst available confusion: the reassuring reading is the one a swallowed
/// error produces, and the person stops looking.
fn kept<T>(problems: &mut Vec<String>, source: &str, r: Result<Vec<T>, sqlx::Error>) -> Vec<T> {
    match r {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(source, error = %e, "rounds: query failed");
            problems.push(format!("{source}: {e}"));
            Vec::new()
        }
    }
}

/// `GET /api/me/rounds`
pub async fn rounds_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db = &state.db;
    let mut decisions: Vec<Value> = Vec::new();
    // Queries that could not run. Served, not logged and forgotten — a queue
    // that cannot read its own sources must not render as an empty one.
    let mut problems: Vec<String> = Vec::new();

    // ── Anomalies awaiting review (Loop 2) ───────────────────────────────
    //
    // Scoped to this user's agents. `requires_review AND resolved_at IS NULL`
    // is the queue the HITL page shows; joining to `agents` is what makes it
    // *yours* rather than the platform's.
    let anomalies = kept(
        &mut problems,
        "anomaly_events",
        sqlx::query_as::<sqlx::Postgres, (String, String, String, f64)>(
            "SELECT ae.kind, ae.severity, COALESCE(a.display_alias, a.agent_name),
                EXTRACT(EPOCH FROM (now() - ae.created_at)) / 86400.0
           FROM anomaly_events ae
           JOIN agents a ON a.agent_id = ae.agent_id
          WHERE a.user_id = $1
            AND ae.requires_review
            AND ae.resolved_at IS NULL
          ORDER BY ae.created_at ASC
          LIMIT 25",
        )
        .bind(&user_id)
        .fetch_all(db)
        .await,
    );

    for (kind, severity, agent, age) in anomalies {
        decisions.push(decision(
            "anomaly",
            &agent.clone(),
            format!("/observatory?agent={agent}"),
            format!("{kind} flagged for review"),
            Some(age),
            &severity,
        ));
    }

    // ── Agent-wide corrections awaiting a second reviewer ────────────────
    //
    // The gate with no UI: today the HITL page tells the operator, in prose, to
    // POST to the consensus endpoint by hand. Surfacing the request as a queue
    // row is the first half of fixing that.
    let consensus = kept(
        &mut problems,
        "two_reviewer_requests",
        sqlx::query_as::<sqlx::Postgres, (String, f64)>(
            "SELECT COALESCE(a.display_alias, a.agent_name),
                EXTRACT(EPOCH FROM (now() - r.created_at)) / 86400.0
           FROM two_reviewer_requests r
           JOIN agents a ON a.agent_id = r.agent_id
          WHERE a.user_id = $1 AND r.second_reviewed_at IS NULL
          ORDER BY r.created_at ASC
          LIMIT 25",
        )
        .bind(&user_id)
        .fetch_all(db)
        .await,
    );

    for (agent, age) in consensus {
        decisions.push(decision(
            "consensus",
            &agent.clone(),
            format!("/observatory/hitl"),
            "agent-wide correction needs a second reviewer".to_string(),
            Some(age),
            "critical",
        ));
    }

    // ── Roster changes proposed but not applied (Loop 4) ─────────────────
    let proposals = kept(
        &mut problems,
        "composition_versions",
        sqlx::query_as::<sqlx::Postgres, (uuid::Uuid, String, f64)>(
            "SELECT cv.workspace_id, COALESCE(t.name, 'workspace'),
                EXTRACT(EPOCH FROM (now() - cv.created_at)) / 86400.0
           FROM composition_versions cv
           JOIN teams t ON t.id = cv.workspace_id
          WHERE t.owner_id = $1
            AND cv.accepted_by IS NULL
            AND cv.rejected_by IS NULL
          ORDER BY cv.created_at ASC
          LIMIT 25",
        )
        .bind(&user_id)
        .fetch_all(db)
        .await,
    );

    for (ws, name, age) in proposals {
        decisions.push(decision(
            "proposal",
            &name,
            format!("/workspace/{ws}"),
            "a roster change is waiting for you".to_string(),
            Some(age),
            "info",
        ));
    }

    // ── Agents that can no longer learn ──────────────────────────────────
    //
    // Not an error anywhere: consolidation simply stops. An exhausted dream
    // budget is Loop 1 going quiet, and quiet is the failure mode this whole
    // design is against.
    let exhausted = kept(
        &mut problems,
        "agents.dream_budget",
        sqlx::query_as::<sqlx::Postgres, (String,)>(
            "SELECT COALESCE(display_alias, agent_name)
           FROM agents
          WHERE user_id = $1
            AND dreaming_budget_credits > 0
            AND dreaming_credits_used >= dreaming_budget_credits
          LIMIT 25",
        )
        .bind(&user_id)
        .fetch_all(db)
        .await,
    );

    for (agent,) in exhausted {
        decisions.push(decision(
            "budget",
            &agent.clone(),
            format!("/agent/{agent}#manage"),
            "dream budget exhausted — this agent has stopped learning".to_string(),
            None,
            "warning",
        ));
    }

    // Oldest first within severity: a critical thing that has waited a week
    // outranks a critical thing from this morning, and both outrank an info.
    let rank = |s: &str| match s {
        "critical" => 0,
        "warning" => 1,
        _ => 2,
    };
    decisions.sort_by(|a, b| {
        let (sa, sb) = (
            a["severity"].as_str().unwrap_or("info"),
            b["severity"].as_str().unwrap_or("info"),
        );
        rank(sa).cmp(&rank(sb)).then_with(|| {
            b["age_days"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["age_days"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    // ── Instrument: what this platform can and cannot tell you ───────────
    let observation = fermi::native_evaluators::Observation {
        writes: fermi::write_accounting::accounts(),
        gates: fermi::gate_trust::accounts(),
        loops: fermi::loop_model::evaluate(db).await,
        liveness: fermi::liveness_trust::latest(),
        gate_ledger: Some(fermi::gate_trust::ledger_status()),
    };

    let instrument: Vec<Value> = panel_absence::PANELS
        .iter()
        .map(|p| {
            let a = panel_absence::resolve(p, &observation);
            let s = stamp_absence(p, &a, Density::Scan);
            json!({
                "panel": p.id,
                "shows": p.shows,
                "reading": s.reading,
                "marker": s.marker,
                "marker_word": s.marker_word,
                "token": s.token,
                "lines": s.lines,
                "answered_by": a.answered_by,
                "detail": a.detail,
                "remediation": a.remediation,
            })
        })
        .collect();

    // ── Resume ───────────────────────────────────────────────────────────
    let agents = kept(
        &mut problems,
        "agents.recent",
        sqlx::query_as::<sqlx::Postgres, (String, String, Option<String>, i64)>(
            "SELECT a.agent_name, COALESCE(a.display_alias, a.agent_name), a.status,
                COUNT(e.episode_id)::bigint
           FROM agents a
           LEFT JOIN episodes e ON e.agent_id = a.agent_id
          WHERE a.user_id = $1 AND a.status <> 'archived'
          GROUP BY a.agent_name, a.display_alias, a.status
          ORDER BY MAX(e.created_at) DESC NULLS LAST
          LIMIT 6",
        )
        .bind(&user_id)
        .fetch_all(db)
        .await,
    );

    let workspaces = kept(
        &mut problems,
        "teams.recent",
        sqlx::query_as::<sqlx::Postgres, (uuid::Uuid, String, i64)>(
            "SELECT t.id, t.name, COUNT(wa.agent_id)::bigint
           FROM teams t
           LEFT JOIN workspace_agents wa ON wa.workspace_id = t.id
          WHERE t.owner_id = $1
          GROUP BY t.id, t.name
          ORDER BY t.created_at DESC
          LIMIT 6",
        )
        .bind(&user_id)
        .fetch_all(db)
        .await,
    );

    Ok(Json(json!({
        "decisions": decisions,
        "instrument": instrument,
        "resume": {
            "agents": agents.iter().map(|(name, alias, status, runs)| json!({
                "agent_name": name, "label": alias, "status": status, "runs": runs,
            })).collect::<Vec<_>>(),
            "workspaces": workspaces.iter().map(|(id, name, members)| json!({
                "id": id, "name": name, "members": members,
            })).collect::<Vec<_>>(),
        },
        // Sources that could not be read. A decision queue rendering empty
        // because its query failed is the one confusion this surface must not
        // produce, so the failure travels with the answer.
        "unreadable": problems,
        // Named on the response, not inferred by the page: every counter this
        // endpoint reads from memory dies on restart, and a surface that cannot
        // say `since boot` will imply a longer history than it has.
        "counters_since_boot": true,
    })))
}
